//! Paragraphs, lists and headings.

use crate::layout::Budget;
use crate::model::{Line, Word, is_cjk};
use crate::page_layout::{FlowItem, PlacedTable, finish_region, render_blocks};
use crate::params::Params;
use crate::pyc::{self, Counter, char_len, median, pmax, pmin, rstrip, strip};
use crate::tables::{Table, bullet, numbered};
use std::rc::Rc;

/// `re.search(r'[^\W\d_]', text)`: a letter, or a number that is not a decimal digit.
fn has_letter(text: &str) -> bool {
    text.chars().any(|c| c != '_' && pyc::is_word_char(c) && !pyc::is_decimal(c))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// `'- '`
    Bullet,
    /// `NUM`: numbered list item; may still be a numbered heading
    Num,
}

/// An ordered element for [`paragraphs`].
pub enum Seq {
    /// a table with its left and right edges (the line extent of a gutter table, the rule
    /// extent of a ruled one)
    Table {
        table: Rc<Table>,
        x0: f64,
        x1: f64,
        y1: f64,
    },
    Line(Rc<Line>),
    /// a rotated region rendered in its own frame: a barrier like a table
    Rendered(Rc<Vec<String>>),
}

pub struct SeqItem {
    pub item: Seq,
    pub y0: f64,
    pub x0: f64,
}

/// (size, all bold, dominant colour) of a line; italic is unused by `style_change`.
fn style(line: &Line) -> (f64, bool, Rc<str>) {
    let mut colors: Counter<Rc<str>> = Counter::new();
    for w in &line.words {
        colors.add(w.color.clone(), 1);
    }
    let color = colors.most_common().cloned().unwrap_or_else(|| Rc::from(""));
    (line.size(), line.words.iter().all(|w| w.bold), color)
}

pub(crate) fn style_change(a: &Line, b: &Line, p: &Params) -> bool {
    let (sa, sb) = (style(a), style(b));
    (sa.0 - sb.0).abs() > p.style_size * pmax(sa.0, 1.0) || sa.1 != sb.1 || (p.color_breaks && sa.2 != sb.2)
}

pub(crate) fn ends_hyphenated_letter(text: &str) -> bool {
    // re.search(r'[A-Za-zÀ-ɏ]-$', text)
    let t = text.strip_suffix('\n').unwrap_or(text);
    let check = |t: &str| {
        let mut it = t.chars().rev();
        it.next() == Some('-') && it.next().is_some_and(|c| matches!(c, 'A'..='Z' | 'a'..='z' | 'À'..='ɏ'))
    };
    check(text) || (t.len() != text.len() && check(t))
}

#[allow(clippy::if_same_then_else)] // mirrors the Python elif chain
pub fn join_lines(lines: &[Rc<Line>], p: &Params) -> String {
    let mut text = String::new();
    for line in lines {
        append_line(&mut text, strip(line.text()), p);
    }
    text
}

/// Continue `text` with the next line `t` (stripped): joined without a space across CJK text,
/// a hyphen before a lowercase letter (`dehyphenate`) or a soft hyphen, else with one space.
#[allow(clippy::if_same_then_else)] // mirrors the Python elif chain
pub(crate) fn append_line(text: &mut String, t: &str, p: &Params) {
    let first = t.chars().next();
    if text.is_empty() {
        *text = t.to_string();
    } else if text.chars().next_back().is_some_and(is_cjk) && first.is_some_and(is_cjk) {
        text.push_str(t);
    } else if p.dehyphenate && ends_hyphenated_letter(text) && first.is_some_and(pyc::is_lower) {
        text.pop();
        text.push_str(t);
    } else if text.ends_with('\u{ad}') {
        text.pop();
        text.push_str(t);
    } else {
        text.push(' ');
        text.push_str(t);
    }
}

/// A list marker at the start of a line: (kind, the line without a bullet).
fn list_prefix(line: &Rc<Line>) -> (Option<Kind>, Rc<Line>) {
    let Some(w) = line.words.first() else { return (None, line.clone()) };
    let first: &str = &w.text;
    if numbered(first) && line.words.len() > 1 {
        return (Some(Kind::Num), line.clone());
    }
    if bullet(first) && line.words.len() > 1 {
        return (Some(Kind::Bullet), Rc::new(Line::new(line.words[1..].to_vec(), line.block, line.flow)));
    }
    let mut cs = first.chars();
    if let (Some(c0), Some(c1)) = (cs.next(), cs.next())
        && matches!(c0, '•' | '●' | '▪' | '■' | '◦' | '‣')
        && c1 != ' '
    {
        let rest_text = &first[c0.len_utf8()..];
        let mut rest = vec![Word { text: Rc::from(rest_text), color: Rc::from(""), ..w.clone() }];
        rest.extend_from_slice(&line.words[1..]);
        return (Some(Kind::Bullet), Rc::new(Line::new(rest, line.block, line.flow)));
    }
    (None, line.clone())
}

fn heading_level(lines: &[Rc<Line>], body: f64, body_color: &str, p: &Params) -> u8 {
    let text = join_lines(lines, p);
    if lines.len() > 3 || char_len(&text) as f64 > p.heading_max_chars || !has_letter(&text) {
        return 0;
    }
    let size = median(lines.iter().map(|l| l.size()).collect());
    let ratio = if body != 0.0 { size / body } else { 1.0 };
    if text.ends_with(['.', ',', ';']) && ratio < p.h2_ratio {
        return 0;
    }
    if ratio >= p.h1_ratio {
        return 1;
    }
    if ratio >= p.h2_ratio {
        return 2;
    }
    if ratio >= p.heading_ratio {
        return 3;
    }
    if lines.len() <= 2
        && char_len(&text) as f64 <= p.minor_heading_chars
        && !text.ends_with(['.', ':', ',', ';'])
        && ratio >= 0.95
    {
        let mut words = lines.iter().flat_map(|l| l.words.iter());
        let bold = words.clone().all(|w| w.bold);
        let colored = p.color_headings && words.all(|w| !w.color.is_empty() && &*w.color != body_color);
        if (p.bold_headings && bold) || ratio >= p.minor_heading_ratio || colored {
            return 3;
        }
    }
    0
}

/// A grouped paragraph, before neighbouring content determines its presentation.
pub(crate) struct Paragraph {
    pub text: String,
    pub level: u8,
    pub ratio: f64,
    pub kind: Option<Kind>,
    pub lines: Vec<Rc<Line>>,
}

pub(crate) const SENT_END: [char; 9] = ['.', '!', '?', ':', ';', '"', '”', '’', ')'];

struct Para<'a> {
    p: &'a Params,
    body: f64,
    body_color: &'a str,
    items: Vec<FlowItem>,
    cur: Vec<Rc<Line>>,
    kind: Option<Kind>,
}

impl Para<'_> {
    fn emit(&mut self) {
        if !self.cur.is_empty() {
            let level = if self.kind != Some(Kind::Bullet) {
                heading_level(&self.cur, self.body, self.body_color, self.p)
            } else {
                0
            };
            let ratio =
                if self.body != 0.0 { median(self.cur.iter().map(|l| l.size()).collect()) / self.body } else { 1.0 };
            let text = join_lines(&self.cur, self.p);
            let lines = std::mem::take(&mut self.cur);
            self.items.push(FlowItem::Paragraph(Paragraph { text, level, ratio, kind: self.kind, lines }));
        }
        self.cur.clear();
        self.kind = None;
    }
}

/// Ordered elements (lines and tables) to Markdown paragraphs.
///
/// Standalone grouping helper: the caller must assign table membership, omit member
/// lines from the sequence, and supply reading order and block policy. This does not
/// detect tables or partition a page. Use [`crate::to_markdown_report`] for complete
/// page ownership and layout.
#[allow(clippy::if_same_then_else)] // mirrors the Python elif chain
pub fn paragraphs(seq: &[SeqItem], body: f64, body_color: &str, p: &Params, use_blocks: bool) -> Vec<String> {
    paragraphs_within(seq, body, body_color, p, use_blocks, &mut Budget::new(f64::INFINITY)).unwrap_or_default()
}

/// [`paragraphs`] within a work budget: each same-baseline merge charges the merged line's word
/// count (a run of n fragments on one baseline rebuilds lines of 1..n words), and the result
/// is `None` once the budget runs out.
#[allow(clippy::if_same_then_else)] // mirrors the Python elif chain
pub fn paragraphs_within(
    seq: &[SeqItem],
    body: f64,
    body_color: &str,
    p: &Params,
    use_blocks: bool,
    budget: &mut Budget,
) -> Option<Vec<String>> {
    let items = group_within(seq, body, body_color, p, use_blocks, budget)?;
    Some(render_blocks(finish_region(items, p), crate::TableFormat::Gfm, false))
}

/// Group lines around table barriers. Assignment and presentation belong to page layout.
#[allow(clippy::if_same_then_else)] // mirrors the Python elif chain
pub(crate) fn group_within(
    seq: &[SeqItem],
    body: f64,
    body_color: &str,
    p: &Params,
    use_blocks: bool,
    budget: &mut Budget,
) -> Option<Vec<FlowItem>> {
    let mut st = Para { p, body, body_color, items: Vec::new(), cur: Vec::new(), kind: None };
    let pitches: Vec<f64> = seq
        .windows(2)
        .filter_map(|w| match (&w[0].item, &w[1].item) {
            (Seq::Line(a), Seq::Line(b)) => {
                let d = b.base() - a.base();
                (0.0 < d && d < 3.0 * pmax(a.size(), 1.0)).then_some(d)
            }
            _ => None,
        })
        .collect();
    let pitch = if pitches.is_empty() { None } else { Some(median(pitches)) };
    let mut prev: Option<Rc<Line>> = None;
    // whether `prev` is the same object as the last line in `cur` (Python's `cur[0] is prev`)
    let mut prev_is_last = false;
    let (mut cur_left, mut cur_right) = (0.0f64, 0.0f64);
    for e in seq {
        let line = match &e.item {
            Seq::Table { table, x0, x1, y1 } => {
                st.emit();
                let t = PlacedTable { table: table.clone(), x0: *x0, x1: *x1, y0: e.y0, y1: *y1, header: None };
                st.items.push(FlowItem::Table(t));
                prev = None;
                prev_is_last = false;
                continue;
            }
            Seq::Rendered(parts) => {
                st.emit();
                st.items.push(FlowItem::Rendered(parts.clone()));
                prev = None;
                prev_is_last = false;
                continue;
            }
            Seq::Line(l) => l.clone(),
        };
        if let Some(pv) = &prev
            && (line.base() - pv.base()).abs() <= p.row_tol * pmax(line.size(), 1.0)
            && line.x0 > pv.x1 - 1.0
        {
            // same baseline: one visual line split by poppler (e.g. a list marker and its text)
            let marker_only = st.cur.len() == 1 && st.kind.is_some() && prev_is_last;
            // release `prev` so a line merged on an earlier step is uniquely owned and its words
            // move instead of being copied (a run of n fragments on one baseline was O(n^2) clones)
            drop(prev.take());
            let mut ws = st
                .cur
                .pop()
                .map(|l| Rc::try_unwrap(l).map_or_else(|l| l.words.clone(), |l| l.words))
                .unwrap_or_default();
            ws.extend_from_slice(&line.words);
            if !budget.spend(ws.len()) {
                return None;
            }
            let merged = Rc::new(Line::new(ws, line.block, line.flow));
            st.cur.push(merged.clone());
            if marker_only {
                cur_left = line.x0;
            }
            cur_right = pmax(cur_right, line.x1);
            prev = Some(merged);
            prev_is_last = true;
            continue;
        }
        let stripped = strip(line.text());
        let is_bullet = bullet(stripped);
        if numbered(stripped) || is_bullet {
            st.emit();
            st.kind = Some(if is_bullet { Kind::Bullet } else { Kind::Num });
            let first = if is_bullet {
                let w = Word {
                    x0: line.x0,
                    y0: line.y0,
                    x1: line.x0,
                    y1: line.y1,
                    text: Rc::from(""),
                    bold: false,
                    italic: false,
                    size: line.size(),
                    base: line.base(),
                    space: true,
                    color: Rc::from(""),
                    glyphs: Default::default(),
                    invisible: false,
                    rot: 0,
                };
                Line::new(vec![w], line.block, line.flow)
            } else {
                Line::new(line.words.clone(), line.block, line.flow)
            };
            let first = Rc::new(first);
            st.cur = vec![first.clone()];
            cur_left = line.x1;
            cur_right = line.x1;
            prev = Some(first);
            prev_is_last = true;
            continue;
        }
        let (prefix, line2) = list_prefix(&line);
        let rtl = line.rtl();
        let mut new = prev.is_none();
        if let Some(pv) = &prev {
            let gap = line.base() - pv.base();
            let col_change = line.x0 > pv.x1 || line.x1 < pv.x0;
            let width = pmax(cur_right - cur_left, 1.0);
            let sent_end = || rstrip(pv.text()).ends_with(SENT_END);
            let cur0 = st.cur.first();
            let display = |l: &Line| {
                p.display_math_paragraphs
                    && l.words.len() <= 3
                    && l.words.first().is_some_and(|w| w.text.starts_with("$$"))
            };
            new = if prefix.is_some() || col_change || gap <= 0.0 || display(&line) || display(pv) {
                true
            } else if use_blocks && line.block != pv.block {
                true
            } else if gap > p.para_gap * pitch.filter(|&x| x != 0.0).unwrap_or(1.2 * pmax(pv.size(), 1.0)) {
                true
            } else if style_change(pv, &line, p) {
                true
            } else if !rtl && pv.x1 < cur_right - (1.0 - p.short_line) * width && sent_end() {
                true
            } else if rtl && pv.x0 > cur_left + (1.0 - p.short_line) * width && sent_end() {
                true
            } else if st.kind.is_some() && !rtl && cur0.is_some_and(|c| line.x0 < c.x0 - 0.5 * pmax(line.size(), 1.0)) {
                true
            } else if st.kind.is_some() && rtl && cur0.is_some_and(|c| line.x1 > c.x1 + 0.5 * pmax(line.size(), 1.0)) {
                true
            } else if st.kind.is_none()
                && !rtl
                && line.x0 > cur_left + 1.2 * pmax(line.size(), 1.0)
                && pv.x1 >= cur_right - 2.0 * pmax(pv.size(), 1.0)
            {
                true // first-line indent
            } else {
                st.kind.is_none()
                    && rtl
                    && line.x1 < cur_right - 1.2 * pmax(line.size(), 1.0)
                    && pv.x0 <= cur_left + 2.0 * pmax(pv.size(), 1.0)
            };
        }
        if new {
            st.emit();
            cur_left = line.x0;
            cur_right = line.x1;
        }
        if prefix.is_some() {
            st.kind = prefix;
        }
        prev_is_last = Rc::ptr_eq(&line2, &line);
        st.cur.push(line2);
        cur_left = pmin(cur_left, line.x0);
        cur_right = pmax(cur_right, line.x1);
        prev = Some(line);
    }
    st.emit();
    Some(st.items)
}
