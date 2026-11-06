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
pub mod math;
pub mod model;
mod page_layout;
pub mod params;
pub mod patterns;
pub mod prose;
pub mod pyc;
pub mod tables;
mod unicode_tables;

pub use model::ParseReport;
pub use params::{Params, UseBlocks};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use layout::Budget;
use model::Page;
use page_layout::PageLayout;
use pyc::{Counter, FloatCounter, char_len, fcmp, pmax, round0, strip};

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
    for ch in pyc::py_lower(strip(text)).chars() {
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

/// Per page, per line: a repeated header/footer line or a page number.
fn furniture(pages: &[Page], p: &Params, _body: f64) -> Vec<Vec<bool>> {
    let mut drop: Vec<Vec<bool>> = pages.iter().map(|pg| vec![false; pg.lines.len()]).collect();
    let band = p.furniture_band;
    let mut counts: HashMap<(bool, String), usize> = HashMap::new();
    let mut exact: HashMap<(bool, String), usize> = HashMap::new();
    let mut cand = Vec::new();
    for (pi, page) in pages.iter().enumerate() {
        let mut seen = HashSet::new();
        let mut seen_exact = HashSet::new();
        for (li, line) in page.lines.iter().enumerate() {
            let mid = (line.y0 + line.y1) / 2.0;
            let top = mid < page.height * band;
            if top || mid > page.height * (1.0 - band) {
                let key = (top, norm_key(line.text()));
                let literal = (top, pyc::py_lower(strip(line.text())).into_owned());
                if seen.insert(key.clone()) {
                    *counts.entry(key.clone()).or_default() += 1;
                }
                if seen_exact.insert(literal.clone()) {
                    *exact.entry(literal.clone()).or_default() += 1;
                }
                cand.push((pi, li, key, literal));
            }
        }
    }
    let need = pmax(2.0, p.furniture_min_share * pages.len() as f64);
    // each page's line baselines, sorted on first use: "no other line on this baseline" is a
    // neighbour lookup instead of a scan of the page
    let mut bases: Vec<Option<Vec<f64>>> = vec![None; pages.len()];
    for (pi, li, key, literal) in cand {
        let lines = &pages[pi].lines;
        let line = &lines[li];
        // Different numbers can carry document meaning. In preservation mode,
        // only literal repetition is evidence of a running header or footer;
        // typography and distance from body text cannot justify deleting it.
        let count =
            if p.furniture_preserve_headings { exact.get(&literal) } else { counts.get(&key) }.copied().unwrap_or(0);
        let repeated = pages.len() >= 2 && count as f64 >= need;
        let mut page_number = || {
            page_number(line.text()) && {
                let sorted = bases[pi].get_or_insert_with(|| {
                    let mut v: Vec<f64> = lines.iter().map(|o| o.base()).collect();
                    v.sort_by(|a, b| fcmp(*a, *b));
                    v
                });
                !layout::near_other(sorted, line.base(), 0.5 * pmax(line.size(), 1.0))
            }
        };
        if repeated || page_number() {
            drop[pi][li] = true;
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

/// Why a page was read as plain lines instead of being laid out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Degraded {
    /// more lines than `max_page_lines`
    PageLines,
    /// more rules than `max_page_rules`
    PageRules,
    /// the layout used up `max_page_work`
    PageWork,
    /// the document's `max_doc_work` was used up
    DocWork,
    /// (math mode) most of the page's text is an invisible OCR layer
    OcrTextLayer,
    /// (math mode) the page's text layer cannot be read (garbage characters, glyphs that decode
    /// to nothing)
    UnreadableText,
}

impl Degraded {
    pub fn as_str(self) -> &'static str {
        match self {
            Degraded::PageLines => "page_lines",
            Degraded::PageRules => "page_rules",
            Degraded::PageWork => "page_work",
            Degraded::DocWork => "doc_work",
            Degraded::OcrTextLayer => "ocr_text_layer",
            Degraded::UnreadableText => "unreadable_text",
        }
    }
}

/// What a conversion skipped, lost or simplified, alongside its Markdown.
#[derive(Clone, Debug, Default)]
pub struct Report {
    pub parse: ParseReport,
    /// (0-based page index, reason) of every page read as plain lines
    pub degraded: Vec<(usize, Degraded)>,
    /// lines plus layout work units charged to `max_doc_work`
    pub doc_work: u64,
    /// the most layout work units one page used
    pub page_work_max: u64,
    /// per page, what the math reader did (empty without [`Options::math`])
    pub math: Vec<math::PageMath>,
}

impl Report {
    /// Whether the input had any `<page>` (input that is not doppler XHTML has none).
    pub fn page_found(&self) -> bool {
        self.parse.pages > 0
    }

    /// One line of JSON.
    pub fn to_json(&self) -> String {
        let r = &self.parse;
        let rules_truncated_pages = r.rules_truncated_pages.iter().map(|i| (i + 1).to_string()).collect::<Vec<_>>();
        let degraded: Vec<String> = self
            .degraded
            .iter()
            .map(|(i, why)| format!("{{\"page\":{},\"reason\":\"{}\"}}", i + 1, why.as_str()))
            .collect();
        let math = if self.math.is_empty() {
            String::new()
        } else {
            let sum = |f: fn(&math::PageMath) -> usize| self.math.iter().map(f).sum::<usize>();
            let pages = |f: fn(&math::PageMath) -> bool| {
                self.math
                    .iter()
                    .enumerate()
                    .filter(|(_, m)| f(m))
                    .map(|(i, _)| (i + 1).to_string())
                    .collect::<Vec<_>>()
                    .join(",")
            };
            format!(
                concat!(
                    ",\"math\":{{\"inline\":{},\"display\":{},\"actualText\":{},\"uncertain\":{},\"partial\":{},\"uncertainWhy\":{{\"unknownGlyph\":{},\"doubt\":{},\"limit\":{},\"mixedActualText\":{}}},\"glyphsLeft\":{},",
                    "\"mathGlyphs\":{},\"skippedPages\":[{}],\"glyphsSkipped\":{},\"glyphsTruncatedPages\":[{}]}},\"mathUncertainPages\":[{}]"
                ),
                sum(|m| m.inline),
                sum(|m| m.display),
                sum(|m| m.actual_text),
                sum(|m| m.uncertain),
                sum(|m| m.partial),
                sum(|m| m.uncertain_unknown),
                sum(|m| m.uncertain_doubt),
                sum(|m| m.uncertain_limit),
                sum(|m| m.uncertain_mixed),
                sum(|m| m.math_glyphs_left),
                sum(|m| m.math_glyphs),
                pages(|m| m.skipped),
                r.glyphs_skipped,
                r.glyphs_truncated_pages.iter().map(|i| (i + 1).to_string()).collect::<Vec<_>>().join(","),
                pages(|m| m.uncertain > 0 || m.partial > 0 || m.ocr_layer || m.unreadable)
            )
        };
        format!(
            concat!(
                "{{\"page_found\":{},\"pages\":{},\"unclosed_pages\":{},\"lines\":{},\"words\":{},",
                "\"words_skipped\":{{\"missing_coord\":{},\"bad_number\":{},\"outside_line\":{},\"unclosed_line\":{}}},",
                "\"unclosed_line_at_eof\":{},\"rules_skipped\":{},\"rules_truncated_pages\":[{}],",
                "\"doc_work\":{},\"page_work_max\":{},\"degraded\":[{}]{}}}"
            ),
            self.page_found(),
            r.pages,
            r.pages.saturating_sub(r.page_closes),
            r.lines,
            r.words,
            r.words_missing_coord,
            r.words_bad_number,
            r.words_outside_line,
            r.words_unclosed_line,
            r.unclosed_line_at_eof,
            r.rules_skipped,
            rules_truncated_pages.join(","),
            self.doc_work,
            self.page_work_max,
            degraded.join(","),
            math
        )
    }
}

/// One Markdown string per page.
pub fn to_markdown(xhtml: &str, p: &Params) -> Vec<String> {
    to_markdown_report(xhtml, p).0
}

/// A degraded page: its kept lines in poppler order, one per Markdown line.
fn plain_lines(page: &Page, drop: &[bool]) -> String {
    let kept: Vec<&str> = page
        .lines
        .iter()
        .zip(drop)
        .filter(|(_, d)| !**d)
        .map(|(l, _)| strip(l.text()))
        .filter(|t| !t.is_empty())
        .collect();
    kept.join("\n")
}

/// How tables are written.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TableFormat {
    /// GFM pipe tables (the first row is always the header row).
    #[default]
    Gfm,
    /// HTML `<table>` blocks: escaped cells, `<thead>` only for a detected header, `dir="rtl"`
    /// for right-to-left tables. Prose stays Markdown.
    Html,
}

/// Output choices that are not layout tuning (those are [`Params`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Options {
    pub tables: TableFormat,
    /// Read typeset math from `pdftotext -glyphs` records and write it as LaTeX (`$...$`,
    /// `$$...$$`). Off: glyph records are ignored and the output is unchanged.
    pub math: bool,
}

/// One Markdown string per page, and a [`Report`] of what was skipped or simplified.
pub fn to_markdown_report(xhtml: &str, p: &Params) -> (Vec<String>, Report) {
    to_markdown_report_with(xhtml, p, &Options::default())
}

/// [`to_markdown_report`] with output [`Options`].
pub fn to_markdown_report_with(xhtml: &str, p: &Params, o: &Options) -> (Vec<String>, Report) {
    let mut report = Report::default();
    let mut pages = model::parse_with(xhtml, &mut report.parse, o.math);
    let math_params;
    let p = if o.math {
        math_params = Params { display_math_paragraphs: true, ..p.clone() };
        &math_params
    } else {
        p
    };
    if o.math {
        let mo = math::MathOptions::default();
        for (pi, page) in pages.iter_mut().enumerate() {
            let (lines, stats) = math::rewrite_page(page, &mo);
            page.lines = lines;
            if stats.ocr_layer {
                report.degraded.push((pi, Degraded::OcrTextLayer));
            } else if stats.unreadable {
                report.degraded.push((pi, Degraded::UnreadableText));
            }
            report.math.push(stats);
        }
    }
    let pages = pages;
    let (body, body_color) = body_size(&pages);
    let drop = furniture(&pages, p, body);
    let mut result = Vec::with_capacity(pages.len());
    let mut spent: u64 = 0;
    for (pi, page) in pages.iter().enumerate() {
        let lines = &page.lines;
        let n = lines.len();
        let doc_left = p.max_doc_work - spent as f64 - n as f64;
        let early = if n as f64 > p.max_page_lines {
            Some(Degraded::PageLines)
        } else if page.rules.len() as f64 > p.max_page_rules {
            Some(Degraded::PageRules)
        } else if doc_left < 0.0 {
            Some(Degraded::DocWork)
        } else {
            None
        };
        if let Some(why) = early {
            report.degraded.push((pi, why));
            result.push(plain_lines(page, &drop[pi]));
            continue;
        }
        spent += n as u64;
        let mut budget = Budget::new(pyc::pmin(p.max_page_work, doc_left));
        match PageLayout::within(page, &drop[pi], body, &body_color, p, o.tables, &mut budget) {
            Some(layout) => result.push(layout.markdown(o.tables, o.math)),
            None => {
                let why = if p.max_page_work <= doc_left { Degraded::PageWork } else { Degraded::DocWork };
                report.degraded.push((pi, why));
                result.push(plain_lines(page, &drop[pi]));
            }
        }
        spent += budget.used;
        report.page_work_max = report.page_work_max.max(budget.used);
    }
    report.doc_work = spent;
    (result, report)
}
