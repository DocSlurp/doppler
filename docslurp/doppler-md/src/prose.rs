//! Paragraphs, lists and headings.

use crate::model::{Line, Word, is_cjk};
use crate::params::Params;
use crate::pyc::{self, Counter, char_len, median, pmax, pmin, rstrip, strip};
use crate::tables::{bullet, numbered};
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
    Table(Rc<str>),
    Line(Rc<Line>),
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

fn style_change(a: &Line, b: &Line, p: &Params) -> bool {
    let (sa, sb) = (style(a), style(b));
    (sa.0 - sb.0).abs() > p.style_size * pmax(sa.0, 1.0) || sa.1 != sb.1 || (p.color_breaks && sa.2 != sb.2)
}

fn ends_hyphenated_letter(text: &str) -> bool {
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
        let t = strip(line.text());
        let first = t.chars().next();
        if text.is_empty() {
            text = t.to_string();
        } else if text.chars().next_back().is_some_and(is_cjk) && first.is_some_and(is_cjk) {
            text.push_str(t);
        } else if p.dehyphenate && ends_hyphenated_letter(&text) && first.is_some_and(pyc::is_lower) {
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
    text
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

struct Item {
    table: bool,
    text: String,
    level: u8,
    ratio: f64,
    kind: Option<Kind>,
}

const SENT_END: [char; 9] = ['.', '!', '?', ':', ';', '"', '”', '’', ')'];

struct Para<'a> {
    p: &'a Params,
    body: f64,
    body_color: &'a str,
    items: Vec<Item>,
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
            self.items.push(Item { table: false, text: join_lines(&self.cur, self.p), level, ratio, kind: self.kind });
        }
        self.cur.clear();
        self.kind = None;
    }
}

/// Ordered elements (lines and tables) to Markdown paragraphs.
#[allow(clippy::if_same_then_else)] // mirrors the Python elif chain
pub fn paragraphs(seq: &[SeqItem], body: f64, body_color: &str, p: &Params, use_blocks: bool) -> Vec<String> {
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
            Seq::Table(t) => {
                st.emit();
                st.items.push(Item { table: true, text: t.to_string(), level: 0, ratio: 1.0, kind: None });
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
            new = if prefix.is_some() || col_change || gap <= 0.0 {
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
    let items = st.items;
    let mut out = Vec::with_capacity(items.len());
    for (i, it) in items.iter().enumerate() {
        if it.table {
            out.push(it.text.clone());
            continue;
        }
        let near_table = (i > 0 && items[i - 1].table) || items.get(i + 1).is_some_and(|n| n.table);
        let minor = it.level != 0 && it.ratio < p.heading_ratio;
        if minor && p.captions && near_table {
            out.push(format!("*{}*", it.text));
            continue;
        }
        let mut level = it.level;
        if minor {
            let ok = items.get(i + 1).is_some_and(|n| {
                !n.table && n.level == 0 && (n.kind.is_some() || char_len(&n.text) as f64 >= p.minor_heading_body)
            });
            if !ok {
                level = 0;
            }
        }
        if level != 0 {
            out.push(format!("{} {}", "#".repeat(level as usize), it.text));
        } else if it.kind.is_some() {
            out.push(format!("- {}", it.text));
        } else {
            out.push(it.text.clone());
        }
    }
    out
}
