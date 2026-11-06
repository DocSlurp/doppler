//! Typeset math from glyph records (`pdftotext -glyphs`): find formulas on a page, read
//! their structure and replace their words with LaTeX (`$...$` inline, `$$...$$` display).
//!
//! Two sources, cheapest first:
//! - **ActualText LaTeX** (tier A): producers such as axessibility and accsupp put the
//!   formula's LaTeX source in a marked-content `/ActualText`. A span whose text parses as
//!   LaTeX (commands, scripts or groups; balanced braces) is used as is.
//! - **Glyph layout** (tier C): glyphs whose font, code point or geometry is math evidence
//!   form runs on lines and clusters of lines; [`layout`] builds the formula and its LaTeX
//!   with a doubt score. A formula with too much doubt, an unknown glyph or a work-limit hit
//!   keeps its original words, and its page is listed as uncertain so that a caller can send
//!   only those pages to a vision model.
//!
//! Glyph identities come only from the text layer: a formula is never given a symbol that
//! no glyph drew.

pub mod layout;
pub mod symbols;

use std::rc::Rc;

use crate::model::{Glyph, Line, Page, Rule, UniSource, Word};
use layout::{Atom, Bar, Ctx};
use symbols::{Class, FontRole};

/// Limits and thresholds of the math reader.
#[derive(Clone, Debug)]
pub struct MathOptions {
    /// formulas whose doubt is above this keep their original words
    pub max_doubt: f64,
    /// glyphs per formula; larger formulas keep their words
    pub max_formula_glyphs: usize,
    /// layout work units per formula and per page
    pub max_formula_work: u64,
    pub max_page_work: u64,
    /// pages with more glyphs, lines or rules than this are not read for math (the page-level
    /// passes compare lines with lines)
    pub max_page_glyphs: usize,
    pub max_page_lines: usize,
    pub max_page_rules: usize,
    /// bars per formula
    pub max_formula_bars: usize,
}

impl Default for MathOptions {
    fn default() -> Self {
        MathOptions {
            max_doubt: 0.45,
            max_formula_glyphs: 1500,
            max_formula_work: 200_000,
            max_page_work: 2_000_000,
            max_page_glyphs: 60_000,
            max_page_lines: 5_000,
            max_page_rules: 5_000,
            max_formula_bars: 200,
        }
    }
}

/// What the math reader did on one page.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PageMath {
    pub inline: usize,
    pub display: usize,
    /// formulas taken from ActualText LaTeX
    pub actual_text: usize,
    /// formulas found but left as text (too doubtful, unknown glyphs, or limits)
    pub uncertain: usize,
    /// the page was not read for math (too many glyphs, or no glyph records)
    pub skipped: bool,
    /// math glyphs the page drew outside any formula that was rewritten
    pub math_glyphs_left: usize,
    /// math glyphs (math font or math code point) the page drew
    pub math_glyphs: usize,
}

/// How strongly a glyph says "math".
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Ev {
    Text,
    /// digits, ASCII operators and punctuation in a text font
    Weak,
    /// a short italic word in a text font (variables in Word/OMML and LibreOffice)
    Var,
    /// a math font or a math code point
    Strong,
}

/// One glyph of a line, with its word and its evidence.
#[derive(Clone, Debug)]
struct G {
    word: usize,
    atom: Atom,
    ev: Ev,
    /// raw text of the glyph (for words that keep part of their glyphs)
    text: String,
}

fn math_codepoint(c: char) -> bool {
    matches!(c,
        '\u{2190}'..='\u{21ff}' | '\u{2200}'..='\u{22ff}' | '\u{2308}'..='\u{230b}' | '\u{239b}'..='\u{23b3}' | '\u{27c0}'..='\u{27ff}'
        | '\u{2980}'..='\u{2aff}' | '\u{1d400}'..='\u{1d7ff}' | 'ℝ' | 'ℕ' | 'ℤ' | 'ℚ' | 'ℂ' | 'ℓ' | 'ℏ' | '℘' | '±' | '×' | '÷')
}

/// Symbols that open list items or mark footnotes; alone they are not math.
fn list_mark(t: &str) -> bool {
    matches!(
        t,
        "•" | "◦"
            | "∙"
            | "·"
            | "∗"
            | "†"
            | "‡"
            | "§"
            | "¶"
            | "⋆"
            | "★"
            | "–"
            | "—"
            | "\u{f0b7}"
            | "\u{f0a7}"
            | "−"
    )
}

/// `(1)`, `(2.3)`, `(4a)`, `[5]`: an equation number at the end of a display.
fn equation_label(t: &str) -> bool {
    let inner = t
        .strip_prefix('(')
        .and_then(|r| r.strip_suffix(')'))
        .or_else(|| t.strip_prefix('[').and_then(|r| r.strip_suffix(']')));
    let Some(inner) = inner else { return false };
    !inner.is_empty()
        && inner.len() <= 12
        && inner.chars().next().is_some_and(|c| c.is_ascii_alphanumeric())
        && inner.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '′' | '\''))
        && inner.chars().any(|c| c.is_ascii_digit())
}

/// The atoms of one line's glyphs. Words without glyph records give no atoms (their glyphs
/// count as text).
fn line_glyphs(page: &Page, line: &Line) -> Vec<G> {
    let mut out = Vec::new();
    for (wi, w) in line.words.iter().enumerate() {
        let gs = page.word_glyphs(w);
        if gs.is_empty() {
            out.push(G { word: wi, atom: text_atom(w), ev: Ev::Text, text: w.text.to_string() });
            continue;
        }
        if gs.len() == 1
            && gs[0].src == UniSource::ActualText
            && (w.text.chars().count() > 4 || actual_text_latex(&w.text).is_some())
        {
            // the text of an /ActualText span: math only when it is LaTeX
            let ev = if actual_text_latex(&w.text).is_some() { Ev::Strong } else { Ev::Text };
            out.push(G { word: wi, atom: text_atom(w), ev, text: w.text.to_string() });
            continue;
        }
        let letters = gs.iter().filter(|g| g.text.chars().all(|c| c.is_alphabetic())).count();
        let start = out.len();
        for (gi, g) in gs.iter().enumerate() {
            let (atom, ev) = glyph_atom(page, w, g, gi == 0, letters);
            out.push(G { word: wi, atom, ev, text: g.text.to_string() });
        }
        // text-font letters in a word with math glyphs (the d of \mathrm{d}t, a function name
        // next to its argument) belong to the math, unless they spell a longer word
        let word = &mut out[start..];
        if word.iter().any(|g| g.ev == Ev::Strong) {
            let text_letters =
                word.iter().filter(|g| g.ev == Ev::Text && g.atom.sym.text.chars().all(char::is_alphabetic)).count();
            if text_letters <= 3 {
                for g in word.iter_mut().filter(|g| g.ev == Ev::Text) {
                    g.ev = Ev::Weak;
                }
            }
        }
    }
    out
}

fn text_atom(w: &Word) -> Atom {
    Atom {
        x0: w.x0,
        x1: w.x1,
        base: w.base,
        size: w.size.max(1.0),
        sym: symbols::Sym {
            text: w.text.to_string(),
            class: Class::Ord,
            style: symbols::Style::Default,
            big: false,
            piece: None,
            known: true,
        },
        role: FontRole::Text,
        upright_text: false,
        tex_ext: false,
        word_start: true,
        span: None,
    }
}

fn glyph_atom(page: &Page, w: &Word, g: &Glyph, first: bool, letters: usize) -> (Atom, Ev) {
    let font = page.font(g);
    let name = font.map_or("", |f| &f.name);
    let role = symbols::font_role(name);
    let fallback = g.src == UniSource::Code;
    // invisible and control characters say nothing
    let text: String = g
        .text
        .chars()
        .filter(|&c| !(c < ' ' || matches!(c, '\u{200b}'..='\u{200f}' | '\u{2060}'..='\u{2064}' | '\u{feff}')))
        .collect();
    let mut sym = symbols::classify(role, g.name.as_deref(), &text, g.code, fallback);
    // a math font whose text is a raw-code guess (no ToUnicode entry, no usable name) or a
    // Latin-1 letter is badly encoded: its symbols are unknown, not guessed
    if role.is_math() && role != FontRole::Symbol {
        let named = g.name.as_deref().is_some_and(|n| {
            symbols::classify(role, Some(n), "", g.code, true).known && !n.starts_with('g') && !n.starts_with("cid")
        });
        let latin1 = sym.text.chars().any(|c| {
            ('\u{c0}'..='\u{24f}').contains(&c) && c != '×' && c != '÷' || ('\u{2018}'..='\u{201f}').contains(&c)
        });
        if (fallback && !named && !g.text.is_empty()) || latin1 {
            sym.known = false;
        }
    }
    let (_, italic) = symbols::font_bold_italic(name);
    let size = g.size.unwrap_or(w.size).max(1.0);
    let base = g.base.unwrap_or(w.base);
    let c = sym.text.chars().next();
    let ev = if role.is_math() {
        Ev::Strong
    } else if sym.class == Class::Accent {
        Ev::Weak
    } else if sym.text.chars().any(math_codepoint)
        || sym.class == Class::LargeOp
        || sym.class == Class::Radical
        || sym.piece.is_some()
        || sym.big
    {
        Ev::Strong
    } else if let Some(c) = c {
        if c.is_ascii_digit() || "+=<>()[]/|,.:;!'*-".contains(c) {
            Ev::Weak
        } else if c.is_alphabetic() && italic && letters <= 2 && c.is_ascii() {
            Ev::Var
        } else if ('\u{391}'..='\u{3f6}').contains(&c) && letters <= 2 {
            Ev::Weak
        } else {
            Ev::Text
        }
    } else {
        Ev::Text
    };
    let upright_text = matches!(role, FontRole::Text) && !italic && c.is_some_and(|c| c.is_alphabetic());
    // a zero-width spacing mark right after a glyph is an accent (tagged PDFs give accents
    // as ActualText ˆ ¯ → . ¨)
    let mut sym = sym;
    if g.w <= 0.01 && sym.class != Class::Accent {
        let acc = match sym.text.as_str() {
            "ˆ" | "^" => Some('\u{302}'),
            "¯" | "‾" => Some('\u{304}'),
            "˜" | "~" => Some('\u{303}'),
            "→" | "⃗" => Some('\u{20d7}'),
            "." | "˙" => Some('\u{307}'),
            "¨" => Some('\u{308}'),
            "´" => Some('\u{301}'),
            "`" => Some('\u{300}'),
            "˘" => Some('\u{306}'),
            "ˇ" => Some('\u{30c}'),
            _ => None,
        };
        if let Some(a) = acc {
            sym.text = a.to_string();
            sym.class = Class::Accent;
        }
    }
    let tex_ext = role == FontRole::MathExtension && !font.is_some_and(|f| f.cid);
    let atom = Atom {
        x0: g.x,
        x1: g.x + g.w.max(0.0),
        base,
        size,
        sym,
        role,
        upright_text,
        tex_ext,
        word_start: first,
        span: None,
    };
    (atom, ev)
}

/// An atom for a named glyph that belongs to no word.
fn extra_atom(page: &Page, g: &Glyph) -> Atom {
    let font = page.font(g);
    let name = font.map_or("", |f| &f.name);
    let role = symbols::font_role(name);
    let mut sym = symbols::classify(role, g.name.as_deref(), &g.text, g.code, true);
    // only names the tables know: a raw code says nothing
    sym.known = sym.known && g.name.is_some() && !sym.text.is_empty() && !sym.text.chars().all(char::is_whitespace);
    let size = g.size.unwrap_or(10.0).max(1.0);
    Atom {
        x0: g.x,
        x1: g.x + g.w.max(0.0),
        base: g.base.unwrap_or(0.0),
        size,
        sym,
        role,
        upright_text: false,
        tex_ext: role == FontRole::MathExtension && !font.is_some_and(|f| f.cid),
        word_start: true,
        span: None,
    }
}

/// Is the glyph an operator that must not start or end a formula?
fn dangling(g: &G) -> bool {
    let t = g.atom.sym.text.as_str();
    g.ev == Ev::Weak && matches!(t, "," | "." | ";" | ":" | "!" | "?" | "-" | "*" | "'" | "/")
}

/// Runs of math glyphs on one line: `(start, end)` glyph indices, end exclusive.
fn runs(gs: &[G]) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < gs.len() {
        if gs[i].ev == Ev::Text {
            i += 1;
            continue;
        }
        let mut j = i;
        while j < gs.len() && gs[j].ev != Ev::Text {
            j += 1;
        }
        let (mut s, mut e) = (i, j);
        // trim dangling punctuation and unmatched brackets of a text font
        loop {
            let before = (s, e);
            while s < e && dangling(&gs[s]) {
                s += 1;
            }
            while e > s && dangling(&gs[e - 1]) {
                e -= 1;
            }
            let opens = |t: &str| matches!(t, "(" | "[");
            let closes = |t: &str| matches!(t, ")" | "]");
            let depth = |a: usize, b: usize| {
                let mut d = 0i32;
                let mut min = 0i32;
                for g in &gs[a..b] {
                    if opens(&g.atom.sym.text) {
                        d += 1;
                    } else if closes(&g.atom.sym.text) {
                        d -= 1;
                        min = min.min(d);
                    }
                }
                (d, min)
            };
            let (d, min) = depth(s, e);
            if d > 0 && s < e && opens(&gs[s].atom.sym.text) && gs[s].ev == Ev::Weak {
                s += 1;
            } else if min < 0 && e > s && closes(&gs[e - 1].atom.sym.text) && gs[e - 1].ev == Ev::Weak {
                e -= 1;
            }
            if (s, e) == before {
                break;
            }
        }
        let strong = gs[s..e].iter().filter(|g| g.ev == Ev::Strong).count();
        let lone_mark = e - s == 1 && list_mark(&gs[s].atom.sym.text);
        if strong > 0 && !lone_mark && s < e {
            out.push((s, e));
        }
        i = j;
    }
    out
}

/// A line as glyphs and its runs.
struct LineMath {
    gs: Vec<G>,
    runs: Vec<(usize, usize)>,
    /// every glyph is math (except an equation label at the end)
    math_only: bool,
    /// glyph index where an equation label starts
    label: Option<usize>,
    strong: bool,
    /// the line is only an equation number
    label_only: bool,
}

fn analyse_line(page: &Page, line: &Line) -> LineMath {
    let gs = line_glyphs(page, line);
    // an equation label: the last word, far right of the rest
    let mut label = None;
    if let Some(last) = gs.last() {
        let lw = last.word;
        let w = &line.words[lw];
        if lw > 0 && equation_label(w.text.trim()) {
            let prev = &line.words[lw - 1];
            if w.x0 - prev.x1 > 2.0 * w.size.max(1.0) {
                label = gs.iter().position(|g| g.word == lw);
            }
        }
    }
    // a line that is only an equation number (Word sets it apart with a tab)
    let label_only = line.words.len() == 1 && equation_label(line.words[0].text.trim());
    if label_only {
        return LineMath { gs, runs: Vec::new(), math_only: false, label: None, strong: false, label_only };
    }
    let body = &gs[..label.unwrap_or(gs.len())];
    let runs = runs(body);
    let strong = body.iter().any(|g| g.ev == Ev::Strong);
    let non_text = !body.is_empty() && body.iter().all(|g| g.ev != Ev::Text);
    let math_only = non_text;
    LineMath { gs, runs, math_only, label, strong, label_only }
}

/// Text of ActualText LaTeX, cleaned, if the span is LaTeX.
pub fn actual_text_latex(t: &str) -> Option<String> {
    let t = t.trim();
    if t.is_empty() || t.len() > 4000 || t.contains('$') || t.chars().any(|c| (c as u32) < 0x20 && c != '\t') {
        return None;
    }
    let b = t.as_bytes();
    let command = b.windows(3).any(|w| w[0] == b'\\' && w[1].is_ascii_alphabetic() && w[2].is_ascii_alphabetic());
    let script = b
        .windows(2)
        .any(|w| (w[0] == b'^' || w[0] == b'_') && (w[1] == b'{' || w[1].is_ascii_alphanumeric() || w[1] == b'\\'));
    if !(command || script) {
        return None;
    }
    // balanced braces, and environments that are closed
    let mut depth = 0i32;
    let mut prev = ' ';
    for c in t.chars() {
        if prev != '\\' {
            if c == '{' {
                depth += 1;
            } else if c == '}' {
                depth -= 1;
                if depth < 0 {
                    return None;
                }
            }
        }
        prev = if prev == '\\' && c == '\\' { ' ' } else { c };
    }
    if depth != 0 || t.matches("\\begin{").count() != t.matches("\\end{").count() {
        return None;
    }
    for bad in [
        "\\documentclass",
        "\\begin{document}",
        "\\input",
        "\\include",
        "\\def",
        "\\newcommand",
        "\\write",
        "\\immediate",
        "\\catcode",
    ] {
        if t.contains(bad) {
            return None;
        }
    }
    // labels and numbering are not part of the formula
    let mut s = t.to_string();
    while let Some(k) = s.find("\\label{") {
        let rest = &s[k + 7..];
        let Some(end) = rest.find('}') else { break };
        s.replace_range(k..k + 7 + end + 1, "");
    }
    for drop in ["\\nonumber", "\\notag"] {
        s = s.replace(drop, "");
    }
    let s = s.split_whitespace().collect::<Vec<_>>().join(" ");
    (!s.is_empty()).then_some(s)
}

/// The rules of a page that can be fraction bars or vincula, as bars.
fn bars_of(rules: &[Rule]) -> Vec<Bar> {
    rules
        .iter()
        .filter(|r| !r.boxed && r.x1 - r.x0 > 0.8 && r.y1 - r.y0 <= 1.6 && (r.x1 - r.x0) > 2.0 * (r.y1 - r.y0))
        .map(|r| Bar { x0: r.x0, x1: r.x1, y: (r.y0 + r.y1) / 2.0 })
        .collect()
}

/// A short bar with a short line centred over it and another under it is a fraction even
/// when its text is in a text font (`\frac{\text{a}}{\text{b}}`); so are the lines on the
/// bar's axis just beside it. Their glyphs become math candidates. At least one glyph of the
/// group must already be math evidence (an operator or a math font), so table rules with
/// cells above and below do not qualify.
fn fraction_evidence(page: &Page, lines: &mut [LineMath], bars: &[Bar]) {
    if bars.len() > 2000 || lines.len() > 5000 {
        return;
    }
    let ext = |l: &LineMath| {
        let x0 = l.gs.iter().map(|g| g.atom.x0).fold(f64::INFINITY, f64::min);
        let x1 = l.gs.iter().map(|g| g.atom.x1).fold(f64::NEG_INFINITY, f64::max);
        let s = l.gs.iter().map(|g| g.atom.size).fold(1.0, f64::max);
        (x0, x1, s)
    };
    let mut promote: Vec<usize> = Vec::new();
    for b in bars {
        if b.x1 - b.x0 > 0.5 * page.width {
            continue;
        }
        let (mut num, mut den): (Option<(usize, f64)>, Option<(usize, f64)>) = (None, None);
        let mut side: Vec<usize> = Vec::new();
        for (li, l) in lines.iter().enumerate() {
            if l.gs.is_empty() || l.gs.len() > 80 {
                continue;
            }
            let (x0, x1, s) = ext(l);
            let base = page.lines[li].base();
            let centred = x0 >= b.x0 - 0.5 * s
                && x1 <= b.x1 + 0.5 * s
                && ((x0 + x1) / 2.0 - (b.x0 + b.x1) / 2.0).abs() <= 0.25 * (b.x1 - b.x0) + 0.5 * s;
            if centred && base < b.y && b.y - base < 1.6 * s && num.is_none_or(|(_, d)| b.y - base < d) {
                num = Some((li, b.y - base));
            } else if centred && base > b.y && base - b.y < 2.2 * s && den.is_none_or(|(_, d)| base - b.y < d) {
                den = Some((li, base - b.y));
            } else if (base - b.y).abs() < 0.8 * s
                && (x1 < b.x0 && b.x0 - x1 < 1.5 * s || x0 > b.x1 && x0 - b.x1 < 1.5 * s)
                && page.lines[li].words.len() <= 4
            {
                side.push(li);
            }
        }
        let (Some((n, _)), Some((d, _))) = (num, den) else { continue };
        let (nx0, nx1, _) = ext(&lines[n]);
        let (dx0, dx1, s) = ext(&lines[d]);
        if b.x1 - b.x0 > (nx1 - nx0).max(dx1 - dx0) + 2.0 * s {
            continue;
        }
        let group: Vec<usize> = [n, d].into_iter().chain(side.iter().copied()).collect();
        let evidence = group.iter().any(|&li| {
            lines[li].gs.iter().any(|g| {
                g.ev == Ev::Strong || (g.ev == Ev::Weak && matches!(g.atom.sym.class, Class::Rel | Class::Bin))
            })
        });
        if evidence {
            promote.extend(group);
        }
    }
    for li in promote {
        let l = &mut lines[li];
        for g in l.gs.iter_mut() {
            if g.ev == Ev::Text {
                g.ev = Ev::Weak;
            }
        }
        l.math_only = true;
        l.strong = true;
    }
}

/// Short upright words on a displayed math line (`\mathrm{m}\,\mathrm{s}^{-1}`,
/// `\text{if}`) are part of the formula. The line must be set off from the prose around it,
/// have math glyphs, and have no text word longer than three letters.
fn display_text_words(page: &Page, lines: &mut [LineMath]) {
    let n = lines.len();
    for li in 0..n {
        let l = &lines[li];
        if l.math_only || !l.strong || l.gs.len() > 200 {
            continue;
        }
        let end = l.label.unwrap_or(l.gs.len());
        let body = &l.gs[..end];
        let mut ok = true;
        let mut text = 0usize;
        let mut k = 0;
        while k < body.len() {
            if body[k].ev != Ev::Text {
                k += 1;
                continue;
            }
            let w = body[k].word;
            let letters = page.lines[li].words[w].text.chars().filter(|c| c.is_alphabetic()).count();
            if letters > 3 {
                ok = false;
                break;
            }
            while k < body.len() && body[k].word == w {
                text += 1;
                k += 1;
            }
        }
        if !ok || text * 2 > body.len() {
            continue;
        }
        // set off from prose: not starting at the left edge of the lines near it
        let line = &page.lines[li];
        let size = line.size().max(1.0);
        let mut left = f64::INFINITY;
        for (k, o) in page.lines.iter().enumerate() {
            if k == li || lines[k].math_only {
                continue;
            }
            let near =
                (o.y1 > line.y0 - 6.0 * size && o.y0 < line.y0) || (o.y0 < line.y1 + 6.0 * size && o.y1 > line.y1);
            if near && o.x1 > line.x0 - 2.0 * size && o.x0 < line.x1 + 2.0 * size {
                left = left.min(o.x0);
            }
        }
        if left.is_finite() && line.x0 - left <= 1.5 * size {
            continue;
        }
        let l = &mut lines[li];
        for g in l.gs[..end].iter_mut() {
            if g.ev == Ev::Text {
                g.ev = Ev::Weak;
            }
        }
        l.math_only = true;
        l.runs = vec![(0, end)];
    }
}

/// A formula found on a page: glyphs of some lines, and how to put it back.
struct Formula {
    /// (line, glyph start, glyph end) pieces, the first is the anchor
    parts: Vec<(usize, usize, usize)>,
    display: bool,
    /// the anchor line's equation label glyph start, for displays
    label: Option<usize>,
    /// lines that are only an equation number beside this display
    label_lines: Vec<usize>,
}

/// Read the math of a page and rewrite its lines. `bars` are the page's thin rules.
pub fn rewrite_page(page: &Page, opts: &MathOptions) -> (Vec<Rc<Line>>, PageMath) {
    let mut stats = PageMath::default();
    if page.glyphs.is_empty()
        || page.glyphs.len() > opts.max_page_glyphs
        || page.lines.len() > opts.max_page_lines
        || page.rules.len() > opts.max_page_rules
    {
        stats.skipped = true;
        return (page.lines.clone(), stats);
    }
    let mut lines: Vec<LineMath> = page.lines.iter().map(|l| analyse_line(page, l)).collect();
    let bars = bars_of(&page.rules);
    fraction_evidence(page, &mut lines, &bars);
    display_text_words(page, &mut lines);
    let n = lines.len();
    // per line, which glyphs are already in a formula
    let mut used: Vec<Vec<bool>> = lines.iter().map(|l| vec![false; l.gs.len()]).collect();
    let mut formulas: Vec<Formula> = Vec::new();

    // clusters of math-only lines that are vertically close and overlap horizontally
    let bbox = |l: &LineMath, a: usize, b: usize| {
        let gs = &l.gs[a..b];
        let x0 = gs.iter().map(|g| g.atom.x0).fold(f64::INFINITY, f64::min);
        let x1 = gs.iter().map(|g| g.atom.x1).fold(f64::NEG_INFINITY, f64::max);
        // tall symbols (big operators, radicals, big delimiters) reach further; an extension-font
        // glyph hangs from its baseline
        let ext = |g: &G| {
            let (a, s) = (&g.atom, g.atom.size);
            let tall = matches!(a.sym.class, Class::LargeOp | Class::Radical | Class::Piece) || a.sym.big;
            if a.tex_ext {
                (a.base - 0.2 * s, a.base + 2.2 * s)
            } else if tall {
                (a.base - 1.4 * s, a.base + 0.9 * s)
            } else {
                (a.base - 0.8 * s, a.base + 0.3 * s)
            }
        };
        let top = gs.iter().map(|g| ext(g).0).fold(f64::INFINITY, f64::min);
        let bot = gs.iter().map(|g| ext(g).1).fold(f64::NEG_INFINITY, f64::max);
        let size = gs.iter().filter(|g| !g.atom.tex_ext).map(|g| g.atom.size).fold(0.0, f64::max).max(1.0);
        (x0, x1, top, bot, size)
    };
    let body_end = |l: &LineMath| l.label.unwrap_or(l.gs.len());
    let mut order: Vec<usize> = (0..n).filter(|&i| lines[i].math_only && body_end(&lines[i]) > 0).collect();
    let boxes: Vec<(f64, f64, f64, f64, f64)> = (0..n)
        .map(|i| if lines[i].math_only { bbox(&lines[i], 0, body_end(&lines[i])) } else { (0.0, 0.0, 0.0, 0.0, 1.0) })
        .collect();
    order.sort_by(|&a, &b| boxes[a].2.total_cmp(&boxes[b].2));
    // union-find over math-only lines that are close vertically and horizontally; lines are
    // sorted by top, so each line looks back only while earlier lines can still be close
    let mut parent: Vec<usize> = (0..n).collect();
    fn find(p: &mut [usize], mut i: usize) -> usize {
        while p[i] != i {
            p[i] = p[p[i]];
            i = p[i];
        }
        i
    }
    let mut max_bot = f64::NEG_INFINITY;
    let mut max_size = 1.0f64;
    for (k, &i) in order.iter().enumerate() {
        let (x0, x1, top, bot, size) = boxes[i];
        let mut looked = 0;
        for &j in order[..k].iter().rev() {
            let (jx0, jx1, jtop, jbot, jsize) = boxes[j];
            let s = size.max(jsize);
            looked += 1;
            if looked > 64 || jtop < top - 20.0 * max_size.max(s) && max_bot < top - 0.5 * s {
                break;
            }
            let vgap = (top - jbot).max(jtop - bot);
            let hgap = (x0 - jx1).max(jx0 - x1);
            if vgap < 0.5 * s && hgap < 2.5 * s {
                let (a, b) = (find(&mut parent, i), find(&mut parent, j));
                parent[a] = b;
            }
        }
        max_bot = max_bot.max(bot);
        max_size = max_size.max(size);
    }
    let mut clusters: Vec<Vec<usize>> = Vec::new();
    let mut root_cluster: std::collections::HashMap<usize, usize> = std::collections::HashMap::new();
    for &i in &order {
        let r = find(&mut parent, i);
        let ci = *root_cluster.entry(r).or_insert_with(|| {
            clusters.push(Vec::new());
            clusters.len() - 1
        });
        clusters[ci].push(i);
    }
    if std::env::var("DOPPLER_MATH_DEBUG").as_deref() == Ok("3") {
        for (li, l) in lines.iter().enumerate() {
            let t: String = l.gs.iter().map(|g| g.atom.sym.text.as_str()).collect();
            eprintln!("line {li} math_only={} strong={} runs={:?} {t}", l.math_only, l.strong, l.runs);
        }
        eprintln!("clusters {clusters:?}");
    }
    // a cluster next to a prose line's run belongs to that run (an inline fraction or limits)
    let mut attached: Vec<Vec<usize>> = vec![Vec::new(); clusters.len()];
    let mut cluster_taken = vec![false; clusters.len()];
    for (ci, c) in clusters.iter().enumerate() {
        if !c.iter().any(|&k| lines[k].strong) && c.len() == 1 && !lines[c[0]].gs.iter().any(|g| g.ev == Ev::Var) {
            // weak-only singleton clusters (a line of numbers) are not math
            cluster_taken[ci] = true;
            continue;
        }
        let (cx0, cx1, ctop, cbot, csize) =
            c.iter().fold((f64::INFINITY, f64::NEG_INFINITY, f64::INFINITY, f64::NEG_INFINITY, 0.0f64), |acc, &k| {
                let (a, b, t, u, s) = bbox(&lines[k], 0, body_end(&lines[k]));
                (acc.0.min(a), acc.1.max(b), acc.2.min(t), acc.3.max(u), acc.4.max(s))
            });
        // the nearest prose line with a run under or over the cluster
        let mut best: Option<(usize, usize, f64)> = None;
        for (li, l) in lines.iter().enumerate() {
            if l.math_only || l.runs.is_empty() {
                continue;
            }
            for (ri, &(a, b)) in l.runs.iter().enumerate() {
                let (x0, x1, top, bot, size) = bbox(l, a, b);
                let s = size.max(csize);
                let vgap = (top - cbot).max(ctop - bot);
                let overlap = x1.min(cx1) - x0.max(cx0);
                // scripts and limits are smaller than the run; fraction parts sit across a bar
                let smaller = csize <= 0.85 * size;
                let (lo, hi) = (cx0.max(x0), cx1.min(x1));
                let across_bar = bars.iter().any(|b| {
                    b.x1 > lo
                        && b.x0 < hi
                        && ((b.y > cbot - 0.5 * s && b.y < top + 0.5 * s)
                            || (b.y < ctop + 0.5 * s && b.y > bot - 0.5 * s))
                });
                if vgap < 0.3 * s
                    && (smaller || across_bar)
                    && overlap > 0.3 * (cx1 - cx0).min(x1 - x0).max(0.5)
                    && best.is_none_or(|(_, _, g)| vgap < g)
                {
                    best = Some((li, ri, vgap));
                }
            }
        }
        if let Some((li, ri, _)) = best {
            let key = li * 4096 + ri;
            attached[ci].push(key);
            cluster_taken[ci] = true;
        }
    }
    // inline formulas: every run of a prose line, with the clusters attached to it
    for (li, l) in lines.iter().enumerate() {
        if l.math_only {
            continue;
        }
        for (ri, &(a, b)) in l.runs.iter().enumerate() {
            let key = li * 4096 + ri;
            let mut parts = vec![(li, a, b)];
            for (ci, keys) in attached.iter().enumerate() {
                if keys.contains(&key) {
                    for &k in &clusters[ci] {
                        parts.push((k, 0, body_end(&lines[k])));
                    }
                }
            }
            formulas.push(Formula { parts, display: false, label: None, label_lines: Vec::new() });
        }
    }
    // display formulas: the remaining clusters
    for (ci, c) in clusters.iter().enumerate() {
        if cluster_taken[ci] {
            continue;
        }
        // the anchor: the line with the most strong glyphs
        let anchor = *c.iter().max_by_key(|&&k| lines[k].gs.iter().filter(|g| g.ev == Ev::Strong).count()).unwrap();
        let mut parts = vec![(anchor, 0, body_end(&lines[anchor]))];
        for &k in c {
            if k != anchor {
                parts.push((k, 0, body_end(&lines[k])));
            }
        }
        let label = c.iter().find_map(|&k| lines[k].label.map(|g| (k, g)));
        // a math-only line inside a paragraph (starting at its left edge) is inline
        let display = is_display(page, &lines, c, &bbox);
        // equation numbers on lines of their own, right of a display line on their baseline
        let mut label_lines = Vec::new();
        if display {
            for (li, l) in lines.iter().enumerate() {
                if !l.label_only {
                    continue;
                }
                let lw = &page.lines[li];
                let beside = c.iter().any(|&k| {
                    let (_, x1, _, _, size) = bbox(&lines[k], 0, body_end(&lines[k]));
                    (page.lines[k].base() - lw.base()).abs() <= 0.5 * size && lw.x0 > x1
                });
                if beside {
                    label_lines.push(li);
                }
            }
        }
        formulas.push(Formula {
            parts,
            display,
            label: label.map(|(_, g)| g).filter(|_| label.is_some_and(|(k, _)| k == anchor)),
            label_lines,
        });
    }
    // a label line belongs to one display
    let mut label_taken = vec![false; n];
    for f in &mut formulas {
        f.label_lines.retain(|&li| !std::mem::replace(&mut label_taken[li], true));
    }

    // read every formula
    let debug = std::env::var_os("DOPPLER_MATH_DEBUG").is_some();
    let mut extra_used = vec![false; page.extra_glyphs.len()];
    let mut extra_order: Vec<usize> = (0..page.extra_glyphs.len()).collect();
    extra_order.sort_by(|&a, &b| page.extra_glyphs[a].x.total_cmp(&page.extra_glyphs[b].x));
    let mut work: u64 = 0;
    let mut replace: Vec<(usize, usize, usize, String, bool)> = Vec::new(); // (line, start, end, tex, display)
    let mut remove: Vec<(usize, usize, usize)> = Vec::new();
    for f in &formulas {
        let glyphs: usize = f.parts.iter().map(|&(_, a, b)| b - a).sum();
        let mut atoms: Vec<Atom> = Vec::with_capacity(glyphs);
        let mut actual: Vec<String> = Vec::new();
        let mut unknown = 0usize;
        for &(li, a, b) in &f.parts {
            for g in &lines[li].gs[a..b] {
                let w = &page.lines[li].words[g.word];
                let gl = page.word_glyphs(w);
                if gl.len() == 1 && gl[0].src == UniSource::ActualText && actual_text_latex(&w.text).is_some() {
                    actual.push(w.text.to_string());
                    continue;
                }
                if !g.atom.sym.known {
                    unknown += 1;
                }
                atoms.push(g.atom.clone());
            }
        }
        // named glyphs that belong to no word (big delimiters with raw whitespace codes) inside
        // the formula's box
        if !atoms.is_empty() && !page.extra_glyphs.is_empty() {
            let em = atoms.iter().map(|a| a.size).fold(1.0, f64::max);
            let (x0, x1, top, bot) =
                atoms.iter().fold((f64::INFINITY, f64::NEG_INFINITY, f64::INFINITY, f64::NEG_INFINITY), |acc, a| {
                    (
                        acc.0.min(a.x0),
                        acc.1.max(a.x1),
                        acc.2.min(a.base - 1.5 * a.size),
                        acc.3.max(a.base + 1.0 * a.size),
                    )
                });
            let from = extra_order.partition_point(|&k| page.extra_glyphs[k].x < x0 - 1.5 * em);
            for &k in &extra_order[from..] {
                let g = &page.extra_glyphs[k];
                if g.x > x1 + 1.5 * em {
                    break;
                }
                let b = g.base.unwrap_or(0.0);
                if !extra_used[k] && b >= top && b <= bot {
                    let a = extra_atom(page, g);
                    if a.sym.known {
                        extra_used[k] = true;
                        atoms.push(a);
                    }
                }
            }
        }
        // numbers alone (figure tick labels, a negative number in prose) stay text
        let mut ctx_failed_bars = false;
        let numbers_only = !atoms.is_empty()
            && atoms.iter().all(|a| {
                a.sym.text.chars().all(|c| c.is_ascii_digit() || matches!(c, '.' | ',' | '−' | '-' | '+' | ' '))
            });
        let tex = if numbers_only {
            None
        } else if !actual.is_empty() && atoms.is_empty() {
            match actual.iter().map(|t| actual_text_latex(t)).collect::<Option<Vec<String>>>() {
                Some(v) => {
                    stats.actual_text += 1;
                    Some(v.join(" "))
                }
                None => None,
            }
        } else if glyphs > opts.max_formula_glyphs || work > opts.max_page_work || unknown > 0 || !actual.is_empty() {
            if debug {
                let t: String = atoms.iter().map(|a| a.sym.text.as_str()).collect();
                eprintln!("math: skipped glyphs={glyphs} unknown={unknown} actual={} text={t}", actual.len());
            }
            None
        } else {
            let (x0, x1, top, bot) =
                atoms.iter().fold((f64::INFINITY, f64::NEG_INFINITY, f64::INFINITY, f64::NEG_INFINITY), |acc, a| {
                    (
                        acc.0.min(a.x0),
                        acc.1.max(a.x1),
                        acc.2.min(a.base - 1.2 * a.size),
                        acc.3.max(a.base + 0.6 * a.size),
                    )
                });
            let em = atoms.iter().map(|a| a.size).fold(1.0, f64::max);
            let top = top - 1.0 * em;
            let fb: Vec<Bar> =
                bars.iter().copied().filter(|b| b.x0 >= x0 - em && b.x1 <= x1 + em && b.y > top && b.y < bot).collect();
            if fb.len() > opts.max_formula_bars {
                ctx_failed_bars = true;
            }
            if debug && std::env::var("DOPPLER_MATH_DEBUG").as_deref() == Ok("2") {
                for a in &atoms {
                    eprintln!(
                        "  atom {:?} x {:.2}-{:.2} b {:.2} s {:.2} {:?} big={} ext={}",
                        a.sym.text, a.x0, a.x1, a.base, a.size, a.sym.class, a.sym.big, a.tex_ext
                    );
                }
                for b in &fb {
                    eprintln!("  bar x {:.2}-{:.2} y {:.2}", b.x0, b.x1, b.y);
                }
            }
            let mut ctx = Ctx::new(opts.max_formula_work);
            ctx.failed = ctx_failed_bars;
            let tree = layout::parse(atoms, fb, &mut ctx);
            work += ctx.work;
            let tex = layout::latex(&tree);
            if debug {
                eprintln!(
                    "math: display={} glyphs={} doubt={:.2} failed={} work={} tex={}",
                    f.display, glyphs, ctx.doubt, ctx.failed, ctx.work, tex
                );
            }
            (!ctx.failed && ctx.doubt <= opts.max_doubt && !tex.is_empty()).then_some(tex)
        };
        match tex {
            Some(tex) => {
                let (li, a, b) = f.parts[0];
                let end = if f.display { f.label.unwrap_or(b) } else { b };
                let mut text = if f.display { format!("$${tex}$$") } else { format!("${tex}$") };
                for &ll in &f.label_lines {
                    text.push(' ');
                    text.push_str(page.lines[ll].text().trim());
                    remove.push((ll, 0, lines[ll].gs.len()));
                }
                replace.push((li, a, end, text, f.display));
                for &(k, s, e) in &f.parts[1..] {
                    remove.push((k, s, e));
                }
                for &(k, s, e) in &f.parts {
                    used[k][s..e].iter_mut().for_each(|u| *u = true);
                }
                if f.display {
                    stats.display += 1;
                } else {
                    stats.inline += 1;
                }
            }
            None if numbers_only => {}
            None => stats.uncertain += 1,
        }
    }
    stats.math_glyphs = lines.iter().map(|l| l.gs.iter().filter(|g| g.ev == Ev::Strong).count()).sum();
    stats.math_glyphs_left = lines
        .iter()
        .enumerate()
        .map(|(li, l)| l.gs.iter().enumerate().filter(|(gi, g)| g.ev == Ev::Strong && !used[li][*gi]).count())
        .sum();
    if replace.is_empty() {
        return (page.lines.clone(), stats);
    }
    // rebuild the lines
    let mut cut: Vec<Vec<(usize, usize, Option<(String, bool)>)>> = vec![Vec::new(); n];
    for (li, a, b, tex, display) in replace {
        cut[li].push((a, b, Some((tex, display))));
    }
    for (li, a, b) in remove {
        cut[li].push((a, b, None));
    }
    let mut out = Vec::with_capacity(n);
    for (li, line) in page.lines.iter().enumerate() {
        if cut[li].is_empty() {
            out.push(line.clone());
            continue;
        }
        cut[li].sort_by_key(|c| c.0);
        if let Some(l) = rebuild(line, &lines[li].gs, &cut[li]) {
            out.push(Rc::new(l));
        }
    }
    (out, stats)
}

/// A display is set off from prose: it does not start at the left edge of the text around it.
fn is_display(
    page: &Page,
    lines: &[LineMath],
    cluster: &[usize],
    bbox: &dyn Fn(&LineMath, usize, usize) -> (f64, f64, f64, f64, f64),
) -> bool {
    let (x0, x1, top, bot, size) =
        cluster.iter().fold((f64::INFINITY, f64::NEG_INFINITY, f64::INFINITY, f64::NEG_INFINITY, 0.0f64), |acc, &k| {
            let e = lines[k].label.unwrap_or(lines[k].gs.len());
            let (a, b, t, u, s) = bbox(&lines[k], 0, e);
            (acc.0.min(a), acc.1.max(b), acc.2.min(t), acc.3.max(u), acc.4.max(s))
        });
    // prose lines within a few line heights above and below, in the same column
    let mut left = f64::INFINITY;
    let mut right = f64::NEG_INFINITY;
    for (k, l) in page.lines.iter().enumerate() {
        if cluster.contains(&k) || lines[k].math_only {
            continue;
        }
        let near = (l.y1 > top - 6.0 * size && l.y0 < top) || (l.y0 < bot + 6.0 * size && l.y1 > bot);
        let column = l.x1 > x0 - 2.0 * size && l.x0 < x1 + 2.0 * size;
        if near && column {
            left = left.min(l.x0);
            right = right.max(l.x1);
        }
    }
    if !left.is_finite() {
        return true;
    }
    x0 - left > 1.5 * size || (right - x1 > 4.0 * size && x0 - left > 0.5 * size)
}

/// A line with glyph ranges replaced by LaTeX words or removed. `None` if nothing is left.
fn rebuild(line: &Line, gs: &[G], cuts: &[(usize, usize, Option<(String, bool)>)]) -> Option<Line> {
    let mut words: Vec<Word> = Vec::new();
    let mut i = 0usize;
    let mut k = 0usize;
    // the glyphs of each word that are kept, as (word, first, end) runs
    while i < gs.len() {
        if k < cuts.len() && cuts[k].0 == i {
            let (a, b, ref what) = cuts[k];
            let b = b.max(a + 1).min(gs.len());
            if let Some((tex, display)) = what {
                let span = &gs[a..b];
                let x0 = span.iter().map(|g| g.atom.x0).fold(f64::INFINITY, f64::min);
                let x1 = span.iter().map(|g| g.atom.x1).fold(f64::NEG_INFINITY, f64::max);
                let first = &line.words[span[0].word];
                let last = &line.words[span[span.len() - 1].word];
                let ends_word = b >= gs.len() || gs[b].word != span[span.len() - 1].word;
                let _ = display;
                let text = tex.clone();
                words.push(Word {
                    x0: x0.min(first.x0.max(x0)),
                    x1,
                    y0: first.y0,
                    y1: first.y1,
                    text: Rc::from(text),
                    bold: false,
                    italic: false,
                    size: first.size,
                    base: first.base,
                    space: if ends_word { last.space } else { false },
                    color: first.color.clone(),
                    glyphs: Default::default(),
                });
            }
            i = b;
            k += 1;
            continue;
        }
        // kept glyphs up to the next cut or the end of their word
        let w = gs[i].word;
        let stop = if k < cuts.len() { cuts[k].0 } else { gs.len() };
        let mut j = i;
        while j < stop && gs[j].word == w {
            j += 1;
        }
        let word = &line.words[w];
        let whole = (i == 0 || gs[i - 1].word != w) && (j >= gs.len() || gs[j].word != w);
        if whole {
            words.push(word.clone());
        } else {
            let text: String = gs[i..j].iter().map(|g| g.text.as_str()).collect();
            let x0 = gs[i].atom.x0;
            let x1 = gs[j - 1].atom.x1;
            let ends_word = j >= gs.len() || gs[j].word != w;
            words.push(Word {
                x0,
                x1,
                text: Rc::from(text),
                space: if ends_word { word.space } else { false },
                glyphs: Default::default(),
                ..word.clone()
            });
        }
        i = j;
    }
    words.retain(|w| !w.text.is_empty());
    if words.is_empty() {
        return None;
    }
    // `$a$$b$` would read as a display delimiter, and `$a$2` does not close: keep a space
    for k in 1..words.len() {
        let next_digit_or_math =
            words[k].text.starts_with('$') || words[k].text.starts_with(|c: char| c.is_ascii_digit());
        if !words[k - 1].space && words[k - 1].text.ends_with('$') && next_digit_or_math {
            words[k - 1].space = true;
        }
    }
    Some(Line::new(words, line.block, line.flow))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn actual_text() {
        assert_eq!(actual_text_latex("\\frac{a}{b}").as_deref(), Some("\\frac{a}{b}"));
        assert_eq!(actual_text_latex("x_i^2 \\label{eq:1}").as_deref(), Some("x_i^2"));
        assert_eq!(actual_text_latex("fi"), None);
        assert_eq!(actual_text_latex("\\frac{a}{b"), None);
        assert_eq!(actual_text_latex("}{"), None);
        assert_eq!(actual_text_latex("$x$"), None);
        assert_eq!(actual_text_latex("\\begin{matrix}a"), None);
        assert_eq!(actual_text_latex("\\def\\x{1} \\x^2"), None);
        assert_eq!(actual_text_latex(&"\\alpha".repeat(1000)), None);
    }

    #[test]
    fn labels() {
        assert!(equation_label("(1)"));
        assert!(equation_label("(2.13)"));
        assert!(equation_label("(4a)"));
        assert!(!equation_label("(a)"));
        assert!(!equation_label("(x+y)"));
        assert!(!equation_label("1"));
    }
}
