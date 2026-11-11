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

mod box_cells;
mod entities;
pub mod extract;
pub mod images;
pub mod layout;
pub mod math;
pub mod model;
mod page_layout;
pub mod params;
pub mod patterns;
pub mod prose;
pub mod pyc;
mod rotation;
mod stitch;
pub mod tables;
mod unicode_tables;
mod vector;
mod watermark;

pub use model::ParseReport;
pub use params::{Params, UseBlocks};
use std::collections::{HashMap, HashSet, VecDeque};
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

/// A margin line's repetition keys: digit-normalized and literal (case-folded), with the band.
type MarginKey = (bool, String);

/// A page's top- and bottom-band lines, the candidates for running headers and footers.
struct Margins {
    /// (line index, normalized key, literal key)
    cand: Vec<(usize, MarginKey, MarginKey)>,
    /// distinct keys of each kind: a page counts once per key
    keys: HashSet<MarginKey>,
    literals: HashSet<MarginKey>,
    /// the page's body size, and the keys of its large words and backdrop pictures
    body: f64,
    marks: HashSet<String>,
    pictures: HashSet<String>,
}

impl Margins {
    fn of(page: &Page, p: &Params) -> Margins {
        let band = p.furniture_band;
        let (body, marks) = if p.watermarks { watermark::word_keys(page, p) } else { (0.0, HashSet::new()) };
        let pictures = if p.watermarks {
            page.images.iter().filter_map(|im| watermark::image_key(page, im)).collect()
        } else {
            HashSet::new()
        };
        let mut m = Margins { cand: Vec::new(), keys: HashSet::new(), literals: HashSet::new(), body, marks, pictures };
        for (li, line) in page.lines.iter().enumerate() {
            let mid = (line.y0 + line.y1) / 2.0;
            let top = mid < page.height * band;
            if top || mid > page.height * (1.0 - band) {
                let key = (top, norm_key(line.text()));
                let literal = (top, pyc::py_lower(strip(line.text())).into_owned());
                m.keys.insert(key.clone());
                m.literals.insert(literal.clone());
                m.cand.push((li, key, literal));
            }
        }
        m
    }
}

/// How many pages of a window have each margin key.
#[derive(Default)]
struct MarginCounts {
    keys: HashMap<MarginKey, usize>,
    literals: HashMap<MarginKey, usize>,
    marks: HashMap<String, usize>,
    pictures: HashMap<String, usize>,
}

impl MarginCounts {
    fn add(&mut self, m: &Margins) {
        for k in &m.keys {
            *self.keys.entry(k.clone()).or_default() += 1;
        }
        for k in &m.literals {
            *self.literals.entry(k.clone()).or_default() += 1;
        }
        for k in &m.marks {
            *self.marks.entry(k.clone()).or_default() += 1;
        }
        for k in &m.pictures {
            *self.pictures.entry(k.clone()).or_default() += 1;
        }
    }

    fn remove(&mut self, m: &Margins) {
        for (map, keys) in [(&mut self.keys, &m.keys), (&mut self.literals, &m.literals)] {
            for k in keys {
                if let Some(n) = map.get_mut(k) {
                    *n -= 1;
                    if *n == 0 {
                        map.remove(k);
                    }
                }
            }
        }
        for (map, keys) in [(&mut self.marks, &m.marks), (&mut self.pictures, &m.pictures)] {
            for k in keys {
                if let Some(n) = map.get_mut(k) {
                    *n -= 1;
                    if *n == 0 {
                        map.remove(k);
                    }
                }
            }
        }
    }
}

/// Per line of `page`: a header/footer line repeated on enough of the `pages` pages that
/// `counts` covers, or a page number.
fn furniture(page: &Page, m: &Margins, counts: &MarginCounts, pages: usize, p: &Params) -> Vec<bool> {
    let mut drop = vec![false; page.lines.len()];
    let need = pmax(2.0, p.furniture_min_share * pages as f64);
    // the page's line baselines, sorted on first use: "no other line on this baseline" is a
    // neighbour lookup instead of a scan of the page
    let mut bases: Option<Vec<f64>> = None;
    let lines = &page.lines;
    for (li, key, literal) in &m.cand {
        let line = &lines[*li];
        // Different numbers can carry document meaning. In preservation mode,
        // only literal repetition is evidence of a running header or footer;
        // typography and distance from body text cannot justify deleting it.
        let count = if p.furniture_preserve_headings { counts.literals.get(literal) } else { counts.keys.get(key) }
            .copied()
            .unwrap_or(0);
        let repeated = pages >= 2 && count as f64 >= need;
        let mut page_number = || {
            page_number(line.text()) && {
                let sorted = bases.get_or_insert_with(|| {
                    let mut v: Vec<f64> = lines.iter().map(|o| o.base()).collect();
                    v.sort_by(|a, b| fcmp(*a, *b));
                    v
                });
                !layout::near_other(sorted, line.base(), 0.5 * pmax(line.size(), 1.0))
            }
        };
        if repeated || page_number() {
            drop[*li] = true;
        }
    }
    drop
}

/// (body font size, body colour): the most common rounded size and colour by character count.
fn body_size<'a>(pages: impl Iterator<Item = &'a Page>) -> (f64, String) {
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
    /// words taken out as watermarks (pale, diagonal or repeated large text)
    pub watermark_words: usize,
    /// every image the producer reported and what became of it (only with [`Options::markers`])
    pub images: Vec<images::Note>,
    /// files written by image extraction (`pdftomarkdown --images-dir`); `None` when it did not
    /// run. With it, every image of the JSON gains `file` (its name in the folder, or null).
    pub images_written: Option<usize>,
    /// why image extraction failed (the folder was removed); a fixed message, no document text
    pub images_error: Option<String>,
    /// whether pdfregions reported itself confined (its `confined=` status line)
    pub images_confined: Option<bool>,
    /// (0-based) pages whose pictures and drawings used up their placing budget: the rest were
    /// marked between paragraphs, unchecked for text, or left unmarked as small drawings
    pub images_budget_pages: Vec<usize>,
    /// images past [`MAX_DOC_IMAGES`] that have no entry in `images` (their markers stand)
    pub images_notes_dropped: usize,
}

/// The most image entries a report keeps for a document (the producer's own document cap).
pub const MAX_DOC_IMAGES: usize = 100_000;

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
        let watermark = if self.watermark_words > 0 {
            format!(",\"watermark_words\":{}", self.watermark_words)
        } else {
            String::new()
        };
        let images = if self.images.is_empty() {
            String::new()
        } else {
            let one = |n: &images::Note| {
                format!(
                    "{{\"id\":\"{}\",\"page\":{},\"bbox\":[{:.2},{:.2},{:.2},{:.2}],\"px\":[{},{}],\"kind\":\"{}\",\"placed\":\"{}\"{}{}{}{}}}",
                    n.id,
                    n.page,
                    coord(n.bbox.0),
                    coord(n.bbox.1),
                    coord(n.bbox.2),
                    coord(n.bbox.3),
                    n.px.0,
                    n.px.1,
                    if n.vector {
                        "vector"
                    } else if n.mask {
                        "mask"
                    } else {
                        "picture"
                    },
                    n.fate.as_str(),
                    if n.members > 1 { format!(",\"members\":{}", n.members) } else { String::new() },
                    n.group.as_ref().map(|g| format!(",\"group\":\"{g}\"")).unwrap_or_default(),
                    if n.degraded { ",\"degraded\":true" } else { "" },
                    match (&n.file, self.images_written) {
                        (Some(f), Some(_)) => format!(",\"file\":\"{f}\""),
                        (None, Some(_)) => ",\"file\":null".to_string(),
                        _ => String::new(),
                    }
                )
            };
            let mut budget = if self.images_budget_pages.is_empty() {
                String::new()
            } else {
                let pages: Vec<String> = self.images_budget_pages.iter().map(|i| (i + 1).to_string()).collect();
                format!(",\"images_budget_pages\":[{}]", pages.join(","))
            };
            if self.images_notes_dropped > 0 {
                budget += &format!(",\"images_notes_dropped\":{}", self.images_notes_dropped);
            }
            format!(",\"images\":[{}]{budget}", self.images.iter().map(one).collect::<Vec<_>>().join(","))
        };
        format!(
            concat!(
                "{{\"page_found\":{},\"pages\":{},\"unclosed_pages\":{},\"lines\":{},\"words\":{},",
                "\"words_skipped\":{{\"missing_coord\":{},\"bad_number\":{},\"outside_line\":{},\"unclosed_line\":{}}},",
                "\"unclosed_line_at_eof\":{},\"rules_skipped\":{},\"rules_truncated_pages\":[{}],",
                "\"doc_work\":{},\"page_work_max\":{},\"degraded\":[{}]{}{}{}{}}}"
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
            math,
            images,
            watermark,
            self.images_written.map(|n| format!(",\"images_written\":{n}")).unwrap_or_default()
                + &self.images_confined.map(|c| format!(",\"images_confined\":{c}")).unwrap_or_default()
                + &self
                    .images_error
                    .as_ref()
                    .map(|e| format!(",\"images_error\":{}", serde_json::Value::String(e.clone())))
                    .unwrap_or_default()
                + &if r.page_info.iter().all(|p| p.crop.is_none()) {
                    // only with the producer's crop sizes (pdftomarkdown --page-info)
                    String::new()
                } else {
                    let v: Vec<String> = r
                        .page_info
                        .iter()
                        .enumerate()
                        .map(|(i, p)| {
                            format!(
                                "{{\"page\":{},\"size\":{},\"words\":{},\"invisible\":{},\"text\":\"{}\"}}",
                                // the PDF's page number (pdftotext writes it with -crop-sizes), so a
                                // -f/-l range reports its own pages; else the position
                                p.number.unwrap_or(i as u32 + 1),
                                p.crop.as_deref().map_or("null".to_string(), |s| format!("\"{s}\"")),
                                p.words,
                                p.invisible,
                                p.text_layer()
                            )
                        })
                        .collect();
                    format!(",\"page_info\":[{}]", v.join(","))
                }
        )
    }
}

/// A report coordinate, within a billion points of the origin (direct XHTML can say 1e308, which
/// would print as hundreds of digits).
fn coord(v: f64) -> f64 {
    v.clamp(-1e9, 1e9)
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
    /// Mark where pictures sit in the text, for a caller that restitches them (see
    /// [`images`]). Needs `pdftotext -images`; off, image records are ignored and the output
    /// is unchanged.
    pub markers: bool,
}

/// One Markdown string per page, and a [`Report`] of what was skipped or simplified.
pub fn to_markdown_report(xhtml: &str, p: &Params) -> (Vec<String>, Report) {
    to_markdown_report_with(xhtml, p, &Options::default())
}

/// [`to_markdown_report`] with output [`Options`].
pub fn to_markdown_report_with(xhtml: &str, p: &Params, o: &Options) -> (Vec<String>, Report) {
    let mut parse = ParseReport::default();
    let pages = model::parse_with(xhtml, &mut parse, o.math);
    let mut out = Vec::with_capacity(pages.len());
    let mut c = Converter::new(p, *o, None);
    for page in pages {
        c.push(page, &mut |_, md| out.push(md));
    }
    let report = c.finish(parse, &mut |_, md| out.push(md));
    (out, report)
}

/// Page-at-a-time conversion with a bounded lookahead. Two decisions look beyond the page: the
/// body font size and colour (the most common over the pages buffered when the first page is
/// written, then fixed), and running headers and footers (a margin line repeated on
/// `furniture_min_share` of the pages within `window` pages either side). The converter keeps
/// at most `window + 1` parsed pages, and the margin keys of `window` written ones.
///
/// With `window: None`, or a window at least as long as the document, both decisions see the
/// whole document and the output is [`to_markdown_report_with`]'s.
///
/// Every detection of content by its repetition across pages (running headers and footers,
/// repeated watermark words and backdrop pictures) sees only the window. `Some(0)` sees one page
/// at a time and so removes none of it; use `None` or a window of several pages (`--stream` is
/// 16).
pub struct Converter<'p> {
    p: &'p Params,
    o: Options,
    window: Option<usize>,
    /// received pages not yet written, oldest first: page `next` onwards
    ahead: VecDeque<(Page, Margins)>,
    /// margins of written pages still inside the window, oldest first
    behind: VecDeque<Margins>,
    counts: MarginCounts,
    /// pages in `counts`: the window around the next page to write
    counted: usize,
    next: usize,
    body: Option<(f64, String)>,
    spent: u64,
    report: Report,
    /// what the page just written left open, for the next to continue (only with markers)
    carry: stitch::Carry,
    /// this conversion's seal on its own markers (see [`images::seal`])
    seal: String,
}

impl<'p> Converter<'p> {
    pub fn new(p: &'p Params, o: Options, window: Option<usize>) -> Self {
        Converter {
            p,
            o,
            window,
            ahead: VecDeque::new(),
            behind: VecDeque::new(),
            counts: MarginCounts::default(),
            counted: 0,
            next: 0,
            body: None,
            spent: 0,
            report: Report::default(),
            carry: stitch::Carry::default(),
            seal: if o.markers { images::seal() } else { String::new() },
        }
    }

    /// Adds the next page; passes (page index, Markdown) to `out` for each page whose window
    /// is now complete.
    pub fn push(&mut self, mut page: Page, out: &mut dyn FnMut(usize, String)) {
        if !self.o.markers {
            page.images = Vec::new();
            page.graphics = Vec::new();
        }
        if self.p.watermarks {
            self.report.watermark_words += watermark::strip_local(&mut page, self.p);
        }
        if self.o.math {
            let (lines, stats) = math::rewrite_page(&page, &math::MathOptions::default());
            page.lines = lines;
            let pi = self.next + self.ahead.len();
            if stats.ocr_layer {
                self.report.degraded.push((pi, Degraded::OcrTextLayer));
            } else if stats.unreadable {
                self.report.degraded.push((pi, Degraded::UnreadableText));
            }
            self.report.math.push(stats);
        }
        let m = Margins::of(&page, self.p);
        self.counts.add(&m);
        self.counted += 1;
        self.ahead.push_back((page, m));
        while self.window.is_some_and(|w| self.ahead.len() > w) {
            self.write_next(out);
        }
    }

    /// Ends the input: writes the remaining pages and returns the report, with `parse` as its
    /// parse report.
    pub fn finish(mut self, parse: ParseReport, out: &mut dyn FnMut(usize, String)) -> Report {
        while !self.ahead.is_empty() {
            self.write_next(out);
        }
        self.report.parse = parse;
        self.report.doc_work = self.spent;
        self.report
    }

    fn write_next(&mut self, out: &mut dyn FnMut(usize, String)) {
        let math_params = Params { display_math_paragraphs: true, ..self.p.clone() };
        let p = if self.o.math { &math_params } else { self.p };
        let (body, body_color) =
            self.body.get_or_insert_with(|| body_size(self.ahead.iter().map(|(pg, _)| pg))).clone();
        let Some((mut page, m)) = self.ahead.pop_front() else { return };
        let pi = self.next;
        self.next += 1;
        if let Some(w) = self.window {
            while self.behind.len() > w {
                if let Some(old) = self.behind.pop_front() {
                    self.counts.remove(&old);
                    self.counted -= 1;
                }
            }
        }
        // large words that repeat on enough pages (computed over the window, like running
        // headers); a page that loses words gets its margins read again
        let need = pmax(2.0, p.furniture_min_share * self.counted as f64);
        let mut repeated_pictures: Vec<bool> = Vec::new();
        let mut fresh: Option<Margins> = None;
        if p.watermarks && self.counted >= 2 {
            if !page.images.is_empty() {
                repeated_pictures = page
                    .images
                    .iter()
                    .map(|im| {
                        watermark::image_key(&page, im)
                            .is_some_and(|k| self.counts.pictures.get(&k).is_some_and(|&n| n as f64 >= need))
                    })
                    .collect();
            }
            let removed = watermark::strip_repeated(&mut page, m.body, &self.counts.marks, need, p);
            if removed > 0 {
                self.report.watermark_words += removed;
                fresh = Some(Margins::of(&page, p));
            }
        }
        let drop = furniture(&page, fresh.as_ref().unwrap_or(&m), &self.counts, self.counted, p);
        if self.window.is_some() {
            self.behind.push_back(m);
        }
        let n = page.lines.len();
        let doc_left = p.max_doc_work - self.spent as f64 - n as f64;
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
            self.report.degraded.push((pi, why));
            self.carry = stitch::Carry::default();
            let md = plain_lines(&page, &drop);
            out(pi, if self.o.markers { images::unseal(&md, &self.seal) } else { md });
            return;
        }
        self.spent += n as u64;
        let number = page.number.unwrap_or(pi as u32 + 1);
        // placing pictures and drawings has a page budget of its own (so that markers do not
        // take the layout's), charged to the document's
        let mut placing = Budget::new(pyc::pmin(p.max_page_work, doc_left));
        let first_note = self.report.images.len();
        let (mut figures, notes) = if page.images.is_empty() {
            (Vec::new(), Vec::new())
        } else {
            images::place(&mut page, &drop, body, p, number, &repeated_pictures, &self.seal, &mut placing)
        };
        self.report.images.extend(notes);
        if !page.graphics.is_empty() {
            let raster: Vec<(images::Figure, String)> = figures
                .iter()
                .map(|f| {
                    let marker = f.marker.replace(&self.seal, "");
                    let id = marker.trim_start_matches("![image](").trim_start_matches(images::SCHEME);
                    (f.clone(), id.trim_end_matches(')').to_string())
                })
                .collect();
            let (vector, notes, absorbed) =
                vector::place(&page, &drop, body, p, number, &raster, &self.seal, &mut placing);
            // pictures inside a vector figure are marked with it
            let mut gone = vec![false; figures.len()];
            for (k, id) in absorbed {
                gone[k] = true;
                let old = &raster[k].1;
                for n in &mut self.report.images[first_note..] {
                    if n.id == *old || n.group.as_deref() == Some(old.as_str()) {
                        n.fate = images::Fate::Grouped;
                        n.group = Some(id.clone());
                    }
                }
            }
            let mut k = 0;
            figures.retain(|_| {
                k += 1;
                !gone[k - 1]
            });
            figures.extend(vector);
            self.report.images.extend(notes);
        }
        if placing.used as f64 > placing.limit {
            self.report.images_budget_pages.push(pi);
        }
        self.spent += placing.used;
        let doc_left = doc_left - placing.used as f64;
        let mut budget = Budget::new(pyc::pmin(p.max_page_work, doc_left));
        let md = match PageLayout::within(&page, &drop, body, &body_color, p, self.o.tables, &mut budget, &figures) {
            Some(mut layout) => {
                if self.o.markers {
                    self.carry = layout.stitch(&self.carry, page.height, p, &self.seal);
                }
                layout.markdown(self.o.tables, self.o.math)
            }
            None => {
                let why = if p.max_page_work <= doc_left { Degraded::PageWork } else { Degraded::DocWork };
                self.report.degraded.push((pi, why));
                self.carry = stitch::Carry::default();
                let mut md = plain_lines(&page, &drop);
                // the pictures between paragraphs follow the text, top to bottom, so that no
                // marker is lost; their notes say so
                figures.sort_by(|a, b| fcmp(a.y0, b.y0).then(fcmp(a.x0, b.x0)));
                for f in &figures {
                    if !md.is_empty() {
                        md.push_str("\n\n");
                    }
                    md.push_str(&f.marker);
                }
                for n in &mut self.report.images[first_note..] {
                    if matches!(n.fate, images::Fate::Block | images::Fate::Inline) {
                        n.degraded = true;
                    }
                }
                md
            }
        };
        if self.report.images.len() > MAX_DOC_IMAGES {
            self.report.images_notes_dropped += self.report.images.len() - MAX_DOC_IMAGES;
            self.report.images.truncate(MAX_DOC_IMAGES);
        }
        self.spent += budget.used;
        self.report.page_work_max = self.report.page_work_max.max(budget.used);
        out(pi, if self.o.markers { images::unseal(&md, &self.seal) } else { md });
    }
}
