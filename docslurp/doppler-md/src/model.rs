//! Words, lines and pages, and the tag scanner over doppler XHTML (`parse` in doppler_md.py).

use memchr::{memchr, memmem};
use std::borrow::Cow;
use std::cell::OnceCell;
use std::rc::Rc;

use crate::pyc::{self, PyRe, median, pmax, py_float};

pub fn is_rtl(c: char) -> bool {
    matches!(c, '\u{0590}'..='\u{08ff}' | '\u{fb1d}'..='\u{fdff}' | '\u{fe70}'..='\u{feff}')
}

pub fn is_ltr(c: char) -> bool {
    matches!(c, 'A'..='Z' | 'a'..='z' | '\u{00c0}'..='\u{024f}' | '\u{0370}'..='\u{03ff}' | '\u{0400}'..='\u{04ff}')
}

pub fn is_cjk(c: char) -> bool {
    matches!(c, '\u{3000}'..='\u{30ff}' | '\u{3400}'..='\u{9fff}' | '\u{f900}'..='\u{faff}' | '\u{ff00}'..='\u{ffef}' | '\u{ac00}'..='\u{d7af}')
}

fn is_control(c: char) -> bool {
    matches!(c, '\u{00}'..='\u{08}' | '\u{0b}'..='\u{1f}' | '\u{7f}' | '\u{fffd}')
}

static LEADING_PUNCT: PyRe = PyRe::new(|c| format!(r"\A([,.;:!?)\]]+)([{}].*)\z", c.d));

/// `LEADING_PUNCT_RE.sub(r'\2\1', t)`: ",28" drawn left of a number reads "28,".
fn leading_punct_sub(t: &str) -> Cow<'_, str> {
    if let Some(c) = LEADING_PUNCT.get(t).captures(t) {
        return Cow::Owned(format!("{}{}", &c[2], &c[1]));
    }
    if let Some(s) = t.strip_suffix('\n')
        && let Some(c) = LEADING_PUNCT.get(s).captures(s)
    {
        return Cow::Owned(format!("{}{}\n", &c[2], &c[1]));
    }
    Cow::Borrowed(t)
}

#[derive(Clone, Debug)]
pub struct Word {
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
    pub text: Rc<str>,
    pub bold: bool,
    pub italic: bool,
    pub size: f64,
    pub base: f64,
    pub space: bool,
    pub color: Rc<str>,
}

impl Word {
    /// (bold, italic) from a font name, as the `Word.bold` / `Word.italic` properties.
    pub fn font_style(font: &str) -> (bool, bool) {
        let f = font.to_lowercase();
        let italic = f.contains("italic") || f.contains("oblique") || f.ends_with(",i");
        let bold = f.contains("bold")
            || f.contains("black")
            || f.contains("heavy")
            || f.contains("semibold")
            || f.ends_with(",b");
        (bold, italic)
    }
}

#[derive(Clone, Debug)]
pub struct Line {
    pub words: Vec<Word>,
    pub block: i64,
    pub flow: i64,
    pub x0: f64,
    pub x1: f64,
    pub y0: f64,
    pub y1: f64,
    size: OnceCell<f64>,
    base: OnceCell<f64>,
    rtl: OnceCell<bool>,
    text: OnceCell<String>,
}

impl Line {
    pub fn new(words: Vec<Word>, block: i64, flow: i64) -> Line {
        // Python's min()/max() raise on an empty line; no caller builds one, and 0.0 keeps the
        // port panic-free if one ever appears.
        let fold = |f: fn(&Word) -> f64, max: bool| {
            let mut it = words.iter().map(f);
            let first = it.next().unwrap_or(0.0);
            it.fold(first, |a, b| if max { pmax(a, b) } else { pyc::pmin(a, b) })
        };
        Line {
            x0: fold(|w| w.x0, false),
            x1: fold(|w| w.x1, true),
            y0: fold(|w| w.y0, false),
            y1: fold(|w| w.y1, true),
            words,
            block,
            flow,
            size: OnceCell::new(),
            base: OnceCell::new(),
            rtl: OnceCell::new(),
            text: OnceCell::new(),
        }
    }

    pub fn size(&self) -> f64 {
        *self.size.get_or_init(|| match self.words.first() {
            Some(w) if w.size != 0.0 => median(self.words.iter().map(|w| w.size).collect()),
            _ => (self.y1 - self.y0) * 0.8,
        })
    }

    pub fn base(&self) -> f64 {
        *self.base.get_or_init(|| match self.words.first() {
            Some(w) if w.base != 0.0 => median(self.words.iter().map(|w| w.base).collect()),
            _ => self.y1,
        })
    }

    pub fn rtl(&self) -> bool {
        *self.rtl.get_or_init(|| {
            // r > 0 needs a right-to-left character, and none is ASCII
            if self.words.iter().all(|w| w.text.is_ascii()) {
                return false;
            }
            let (mut r, mut l) = (0usize, 0usize);
            for w in &self.words {
                for c in w.text.chars() {
                    r += is_rtl(c) as usize;
                    l += is_ltr(c) as usize;
                }
            }
            r > 0 && r >= l
        })
    }

    pub fn text(&self) -> &str {
        self.text.get_or_init(|| self.compute_text())
    }

    fn compute_text(&self) -> String {
        let words = &self.words;
        let n = words.len();
        let glue: Vec<bool> =
            words.windows(2).map(|p| p[0].space || p[1].x0 - p[0].x1 > 0.15 * pmax(p[0].size, 1.0)).collect();
        let rtl = self.rtl();
        let mut order: Vec<usize> = (0..n).collect();
        if n > 1 && rtl {
            // words are in visual (left to right) order: read right to left, keeping runs of
            // left-to-right words (Latin, numbers) in their own order
            let mut runs: Vec<Vec<usize>> = Vec::new();
            let mut cur: Vec<usize> = Vec::new();
            for i in (0..n).rev() {
                if words[i].text.chars().any(is_rtl) {
                    if !cur.is_empty() {
                        cur.reverse();
                        runs.push(std::mem::take(&mut cur));
                    }
                    runs.push(vec![i]);
                } else {
                    cur.push(i);
                }
            }
            if !cur.is_empty() {
                cur.reverse();
                runs.push(cur);
            }
            order = runs.into_iter().flatten().collect();
        }
        let mut out = String::new();
        for (k, &i) in order.iter().enumerate() {
            let t: &str = &words[i].text;
            if rtl && !t.chars().any(is_rtl) {
                out.push_str(&leading_punct_sub(t));
            } else {
                out.push_str(t);
            }
            if k + 1 < order.len() {
                let j = order[k + 1];
                let sp = if i.abs_diff(j) == 1 { glue[i.min(j)] } else { true };
                if sp {
                    out.push(' ');
                }
            }
        }
        out
    }
}

#[derive(Clone, Debug)]
pub struct Rule {
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
    pub color: String,
    pub boxed: bool,
}

#[derive(Debug)]
pub struct Page {
    pub width: f64,
    pub height: f64,
    /// in poppler reading order
    pub lines: Vec<Rc<Line>>,
    /// doppler -rules
    pub rules: Vec<Rule>,
}

/// `dict(ATTR_RE.findall(s))` with `(\w+)="([^"]*)"`; later duplicates win on lookup.
pub struct Attrs<'a>(Vec<(&'a str, &'a str)>);

impl<'a> Attrs<'a> {
    pub fn parse(s: &'a str) -> Attrs<'a> {
        let mut out = Vec::new();
        let b = s.as_bytes();
        let mut i = 0;
        while i < s.len() {
            // maximal run of \w characters starting at i
            let mut j = i;
            for (k, c) in s[i..].char_indices() {
                if pyc::is_word_char(c) {
                    j = i + k + c.len_utf8();
                } else {
                    break;
                }
            }
            if j == i {
                i += s[i..].chars().next().map_or(1, char::len_utf8);
                continue;
            }
            // Every suffix of a failing run fails the same way, so resume after the run.
            if b.get(j) == Some(&b'=') && b.get(j + 1) == Some(&b'"') {
                if let Some(q) = memchr(b'"', &b[j + 2..]) {
                    out.push((&s[i..j], &s[j + 2..j + 2 + q]));
                    i = j + 2 + q + 1;
                    continue;
                }
            }
            i = j;
        }
        Attrs(out)
    }

    pub fn get(&self, k: &str) -> Option<&'a str> {
        self.0.iter().rev().find(|(n, _)| *n == k).map(|(_, v)| *v)
    }
}

enum Tag<'a> {
    Open(&'a str, &'a str),
    Close(&'a str),
    Word(&'a str, &'a str),
    Rule(bool, &'a str),
}

const NAMES: [&str; 4] = ["page", "flow", "block", "line"];

/// One step of `TAG_RE.finditer`: the match starting at `lt` (a '<'), and its end.
fn tag_at(s: &str, lt: usize) -> Option<(Tag<'_>, usize)> {
    let b = s.as_bytes();
    let rest = &s[lt + 1..];
    // </(page|flow|block|line)>
    if let Some(r) = rest.strip_prefix('/') {
        for n in NAMES {
            if r.starts_with(n) && r.as_bytes().get(n.len()) == Some(&b'>') {
                return Some((Tag::Close(n), lt + 2 + n.len() + 1));
            }
        }
        return None;
    }
    // <(page|flow|block|line)\b([^>]*)>
    for n in NAMES {
        if rest.starts_with(n) {
            let after = lt + 1 + n.len();
            if s[after..].chars().next().is_some_and(pyc::is_word_char) {
                return None;
            }
            let gt = after + memchr(b'>', &b[after..])?;
            return Some((Tag::Open(n, &s[after..gt]), gt + 1));
        }
    }
    // <word ([^>]*)>(.*?)</word>
    if rest.starts_with("word ") {
        let a = lt + 6;
        let gt = a + memchr(b'>', &b[a..])?;
        let t = gt + 1;
        let end = t + memmem::find(&b[t..], b"</word>")?;
        if memchr(b'\n', &b[t..end]).is_some() {
            return None;
        }
        return Some((Tag::Word(&s[a..gt], &s[t..end]), end + 7));
    }
    // <(rule|box) ([^>]*)/>
    for (n, boxed) in [("rule ", false), ("box ", true)] {
        if rest.starts_with(n) {
            let a = lt + 1 + n.len();
            let gt = a + memchr(b'>', &b[a..])?;
            if gt > a && b[gt - 1] == b'/' {
                return Some((Tag::Rule(boxed, &s[a..gt - 1]), gt + 1));
            }
            return None;
        }
    }
    None
}

fn coord(a: &Attrs, k: &str) -> Option<f64> {
    py_float(a.get(k)?)
}

/// `float(a.get(k, 0) or 0)`
fn opt_num(a: &Attrs, k: &str) -> Option<f64> {
    match a.get(k) {
        None | Some("") => Some(0.0),
        Some(v) => py_float(v),
    }
}

fn word_text(raw: &str) -> String {
    pyc::html_unescape(raw).chars().filter(|&c| !is_control(c)).collect()
}

/// Parse doppler XHTML into pages. Where Python would raise (a missing or malformed required
/// number), the offending word or rule is skipped instead; non-finite numbers count as malformed.
pub fn parse(xhtml: &str) -> Vec<Page> {
    let mut pages: Vec<Page> = Vec::new();
    let mut words: Option<Vec<Word>> = None;
    let (mut flow, mut block) = (-1i64, -1i64);
    let b = xhtml.as_bytes();
    let mut pos = 0;
    while let Some(off) = memchr(b'<', &b[pos..]) {
        let lt = pos + off;
        let Some((tag, end)) = tag_at(xhtml, lt) else {
            pos = lt + 1;
            continue;
        };
        pos = end;
        let have_page = !pages.is_empty();
        match tag {
            Tag::Rule(boxed, attrs) => {
                if !have_page {
                    continue;
                }
                let a = Attrs::parse(attrs);
                let (Some(x0), Some(y0), Some(x1), Some(y1)) =
                    (coord(&a, "xMin"), coord(&a, "yMin"), coord(&a, "xMax"), coord(&a, "yMax"))
                else {
                    continue;
                };
                let color = a.get("color").unwrap_or("#000000").to_string();
                if let Some(pg) = pages.last_mut() {
                    pg.rules.push(Rule { x0, y0, x1, y1, color, boxed });
                }
            }
            Tag::Open("page", attrs) => {
                let a = Attrs::parse(attrs);
                let dim = |k: &str, d: f64| a.get(k).map_or(Some(d), py_float).unwrap_or(d);
                pages.push(Page {
                    width: dim("width", 612.0),
                    height: dim("height", 792.0),
                    lines: Vec::new(),
                    rules: Vec::new(),
                });
            }
            Tag::Open("flow", _) => flow += 1,
            Tag::Open("block", _) => block += 1,
            Tag::Open(_, _) => words = Some(Vec::new()),
            Tag::Close(name) => {
                if name == "line"
                    && have_page
                    && let Some(ws) = words.as_ref()
                    && !ws.is_empty()
                {
                    let ws = words.take().unwrap_or_default();
                    if let Some(pg) = pages.last_mut() {
                        pg.lines.push(Rc::new(Line::new(ws, block, flow)));
                    }
                }
            }
            Tag::Word(attrs, text) => {
                let Some(ws) = words.as_mut() else { continue };
                let a = Attrs::parse(attrs);
                let (Some(x0), Some(y0), Some(x1), Some(y1), Some(size), Some(base)) = (
                    coord(&a, "xMin"),
                    coord(&a, "yMin"),
                    coord(&a, "xMax"),
                    coord(&a, "yMax"),
                    opt_num(&a, "fontSize"),
                    opt_num(&a, "baseline"),
                ) else {
                    continue;
                };
                let (bold, italic) = Word::font_style(a.get("fontName").unwrap_or(""));
                ws.push(Word {
                    x0,
                    y0,
                    x1,
                    y1,
                    text: Rc::from(word_text(text)),
                    bold,
                    italic,
                    size,
                    base,
                    space: a.get("spaceAfter").unwrap_or("1") != "0",
                    color: Rc::from(a.get("color").unwrap_or("")),
                });
            }
        }
    }
    pages
}
