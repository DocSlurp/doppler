//! Markdown from DocSlurp doppler `pdftotext -bbox-layout -wordmeta -rules` XHTML.
//!
//! A byte-for-byte port of `doppler_md.py` (the DocSlurp app's doppler Markdown prototype). It
//! uses only observable page evidence: word boxes, fonts, sizes, colours, vector rules and
//! poppler's flow/block/line grouping.
//!
//! The input is derived from customer PDFs and treated as hostile: no `unsafe`, no recursion
//! beyond the region depth cap, and memory proportional to the input (see
//! [`tables::gutter_columns`] for the one place Python allocates by coordinate range).

#![forbid(unsafe_code)]

mod entities;
pub mod layout;
pub mod model;
pub mod params;
pub mod patterns;
pub mod prose;
pub mod pyc;
pub mod tables;
mod unicode_tables;

pub use params::{Params, UseBlocks};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use layout::{Elem, regions};
use model::Page;
use prose::{Seq, SeqItem, paragraphs};
use pyc::{Counter, FloatCounter, char_len, fcmp, median, pmax, round0, strip};
use tables::{find_tables, grid_markdown, ruled_tables};

/// `PAGE_NUM_RE.match(text)`: `^\W*(?:page\s*)?\d{1,4}(?:\s*(?:of|/)\s*\d{1,4})?\W*$` with
/// re.I. Every quantifier is followed by a character it cannot consume (a `\w` after `\W*`, a
/// digit after `\s*`, a non-digit after `\d{1,4}`), so greedy matching never needs to backtrack
/// except over the optional "of N" group, which is tried both ways. `$` before a final newline
/// is covered by the trailing `\W*`.
fn page_number(text: &str) -> bool {
    let c: Vec<char> = text.chars().collect();
    let skip = |mut i: usize, f: fn(char) -> bool| {
        while i < c.len() && f(c[i]) {
            i += 1;
        }
        i
    };
    let digits = |i: usize| {
        let mut j = i;
        while j < c.len() && j - i < 4 && pyc::is_decimal(c[j]) {
            j += 1;
        }
        if j == i || (j < c.len() && pyc::is_decimal(c[j])) { None } else { Some(j) }
    };
    let word = |i: usize, w: &str| {
        w.chars().enumerate().all(|(k, ch)| c.get(i + k).is_some_and(|&x| x.to_lowercase().eq(std::iter::once(ch))))
    };
    let not_word = |ch: char| !pyc::is_word_char(ch);
    let tail_ok = |i: usize| skip(i, not_word) == c.len();
    let mut i = skip(0, not_word);
    if word(i, "page") {
        i = skip(i + 4, pyc::is_space);
    }
    let Some(i) = digits(i) else { return false };
    let j = skip(i, pyc::is_space);
    let after_sep = if word(j, "of") {
        Some(j + 2)
    } else if c.get(j) == Some(&'/') {
        Some(j + 1)
    } else {
        None
    };
    if let Some(k) = after_sep
        && let Some(k) = digits(skip(k, pyc::is_space))
        && tail_ok(k)
    {
        return true;
    }
    tail_ok(i)
}

/// `re.sub(r'\d+', '#', text.strip().lower())`
fn norm_key(text: &str) -> String {
    let mut out = String::new();
    let mut in_digits = false;
    for ch in strip(text).to_lowercase().chars() {
        if pyc::is_decimal(ch) {
            if !in_digits {
                out.push('#');
            }
            in_digits = true;
        } else {
            out.push(ch);
            in_digits = false;
        }
    }
    out
}

/// Line ids (page index, line index) of repeated header/footer lines and page numbers.
fn furniture(pages: &[Page], p: &Params) -> HashSet<(usize, usize)> {
    let mut drop = HashSet::new();
    let band = p.furniture_band;
    let mut counts: HashMap<String, usize> = HashMap::new();
    let mut cand: Vec<(usize, usize, String)> = Vec::new();
    for (pi, page) in pages.iter().enumerate() {
        let mut seen: HashSet<String> = HashSet::new();
        for (li, line) in page.lines.iter().enumerate() {
            let mid = (line.y0 + line.y1) / 2.0;
            if mid < page.height * band || mid > page.height * (1.0 - band) {
                let key = norm_key(line.text());
                if seen.insert(key.clone()) {
                    *counts.entry(key.clone()).or_default() += 1;
                }
                cand.push((pi, li, key));
            }
        }
    }
    let need = pmax(2.0, p.furniture_min_share * pages.len() as f64);
    for (pi, li, key) in cand {
        let lines = &pages[pi].lines;
        let line = &lines[li];
        let repeated = pages.len() >= 2 && counts.get(&key).copied().unwrap_or(0) as f64 >= need;
        let page_number = || {
            page_number(line.text())
                && !lines
                    .iter()
                    .enumerate()
                    .any(|(j, o)| j != li && (o.base() - line.base()).abs() <= 0.5 * pmax(line.size(), 1.0))
        };
        if repeated || page_number() {
            drop.insert((pi, li));
        }
    }
    drop
}

/// (body font size, body colour): the most common rounded size and colour by character count.
fn body_size(pages: &[Page]) -> (f64, String) {
    let mut c = FloatCounter::new();
    let mut colors: Counter<Rc<str>> = Counter::new();
    for page in pages {
        for line in &page.lines {
            for w in &line.words {
                let n = char_len(&w.text) as i64;
                colors.add(w.color.clone(), n);
                if w.size != 0.0 {
                    c.add(round0(w.size * 2.0) / 2.0, n);
                }
            }
        }
    }
    let color = colors.most_common().map(|s| s.to_string()).unwrap_or_default();
    (c.most_common().unwrap_or(10.0), color)
}

/// One Markdown string per page.
pub fn to_markdown(xhtml: &str, p: &Params) -> Vec<String> {
    let pages = model::parse(xhtml);
    let drop = furniture(&pages, p);
    let (body, body_color) = body_size(&pages);
    let mut result = Vec::with_capacity(pages.len());
    for (pi, page) in pages.iter().enumerate() {
        let lines = &page.lines;
        let ids: Vec<usize> = (0..lines.len()).filter(|&i| !drop.contains(&(pi, i))).collect();
        let mut per_block: Counter<i64> = Counter::new();
        for &i in &ids {
            per_block.add(lines[i].block, 1);
        }
        let use_blocks = match p.use_blocks {
            UseBlocks::On => true,
            UseBlocks::Auto => !per_block.is_empty() && median(per_block.counts().map(|c| c as f64).collect()) >= 1.5,
            UseBlocks::Off => false,
        };
        let ruled = if p.rules && p.tables { ruled_tables(page, &ids, body, p) } else { Vec::new() };
        let in_ruled: HashSet<usize> = ruled.iter().flat_map(|(m, _, _)| m.iter().copied()).collect();
        let tables_md: Vec<Rc<str>> = ruled.iter().map(|(_, g, _)| Rc::from(grid_markdown(g))).collect();
        let mut elements: Vec<Elem> = ids
            .iter()
            .filter(|i| !in_ruled.contains(i))
            .map(|&i| {
                let l = &lines[i];
                Elem { line: Some(i), table: 0, x0: l.x0, x1: l.x1, y0: l.y0, y1: l.y1 }
            })
            .collect();
        elements.extend(ruled.iter().enumerate().map(|(k, (_, _, b))| Elem {
            line: None,
            table: k,
            x0: b.0,
            y0: b.1,
            x1: b.2,
            y1: b.3,
        }));
        let mut parts: Vec<String> = Vec::new();
        for region in regions(&elements, lines, body, p, 0) {
            let rids: Vec<usize> = region.iter().filter_map(|e| e.line).collect();
            let mut seq: Vec<SeqItem> = region
                .iter()
                .filter(|e| e.line.is_none())
                .map(|e| SeqItem { item: Seq::Table(tables_md[e.table].clone()), y0: e.y0, x0: e.x0 })
                .collect();
            let mut consumed: HashSet<usize> = HashSet::new();
            if p.tables {
                for (members, grid) in find_tables(page, &rids, p, body) {
                    consumed.extend(members.iter().copied());
                    let y0 = members.iter().map(|&i| lines[i].y0).fold(f64::INFINITY, pyc::pmin);
                    seq.push(SeqItem { item: Seq::Table(Rc::from(grid_markdown(&grid))), y0, x0: 0.0 });
                }
            }
            seq.extend(region.iter().filter_map(|e| {
                let i = e.line?;
                (!consumed.contains(&i)).then(|| SeqItem { item: Seq::Line(lines[i].clone()), y0: e.y0, x0: e.x0 })
            }));
            seq.sort_by(|a, b| fcmp(a.y0, b.y0).then(fcmp(a.x0, b.x0)));
            parts.extend(paragraphs(&seq, body, &body_color, p, use_blocks));
        }
        let kept: Vec<&str> = parts.iter().map(String::as_str).filter(|x| !strip(x).is_empty()).collect();
        result.push(kept.join("\n\n"));
    }
    result
}
