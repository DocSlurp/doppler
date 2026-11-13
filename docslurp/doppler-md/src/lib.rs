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
pub mod heading_levels;
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

/// The smallest body font size an estimate gives and a caller may give (points).
pub const BODY_SIZE_MIN: f64 = 0.5;
/// The largest body font size an estimate gives and a caller may give (points).
pub const BODY_SIZE_MAX: f64 = 1000.0;
/// The longest body colour an estimate gives and a caller may give (characters).
pub const BODY_COLOR_MAX_CHARS: usize = 64;

/// Whether `pt` is a body font size a caller may give: finite, within
/// [`BODY_SIZE_MIN`]..=[`BODY_SIZE_MAX`]. Every estimate is one.
pub fn body_size_ok(pt: f64) -> bool {
    (BODY_SIZE_MIN..=BODY_SIZE_MAX).contains(&pt)
}

/// Whether `c` is a body colour a caller may give: at most [`BODY_COLOR_MAX_CHARS`]
/// characters, no control characters. Every estimate is one.
pub fn body_color_ok(c: &str) -> bool {
    c.chars().count() <= BODY_COLOR_MAX_CHARS && !c.chars().any(char::is_control)
}

/// (body font size, body colour): the most common rounded size and colour by character count.
/// The size is a multiple of 0.5 within [`body_size_ok`] (a document's sizes outside it are
/// clamped) and the colour passes [`body_color_ok`] (other colours are not counted), so a
/// caller can always give an estimate back as `--body-size` / `--body-color`.
pub(crate) fn body_size<'a>(pages: impl Iterator<Item = &'a Page>) -> (f64, String) {
    let mut c = FloatCounter::new();
    let mut colors: Counter<Rc<str>> = Counter::new();
    for page in pages {
        for line in &page.lines {
            for w in &line.words {
                let n = char_len(&w.text) as i64;
                if body_color_ok(&w.color) {
                    colors.add(w.color.clone(), n);
                }
                if w.size != 0.0 && w.size.is_finite() {
                    c.add((round0(w.size * 2.0) / 2.0).clamp(BODY_SIZE_MIN, BODY_SIZE_MAX), n);
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
    /// lines plus layout work units charged to the document budget
    pub doc_work: u64,
    /// the document budget at the last page read: `max_doc_work` plus `doc_work_per_page` per
    /// page of the document up to it, at most `max_doc_work_total` (see [`Params::doc_work_limit`])
    pub doc_work_limit: f64,
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
    /// the body size and colour used, and where they came from
    pub body: BodyInfo,
    /// a page range converted with context (see [`Window::skip`])
    pub range: Option<RangeInfo>,
    /// with [`HeadingLevels::Rank`]: heading sizes ranked by the seed, and in all at the end
    pub heading_sizes: Option<(usize, usize)>,
    /// pages read, context pages included (`parse.pages` of a range counts its own pages only);
    /// not in the JSON
    pub pages_read: usize,
}

/// The most image entries a report keeps for a document (the producer's own document cap).
pub const MAX_DOC_IMAGES: usize = 100_000;

impl Report {
    /// Whether the input had any `<page>` (input that is not doppler XHTML has none).
    pub fn page_found(&self) -> bool {
        self.parse.pages > 0
    }

    /// The body estimate alone, as a JSON object (`pdftomarkdown --estimate-body`).
    pub fn body_to_json(&self) -> String {
        format!("{{{}}}", &self.body_members()[1..])
    }

    /// The body estimate's JSON members, with a leading comma.
    fn body_members(&self) -> String {
        let b = &self.body;
        let given: Vec<&str> = [(b.size_given, "\"size\""), (b.color_given, "\"color\"")]
            .into_iter()
            .filter(|g| g.0)
            .map(|g| g.1)
            .collect();
        format!(
            ",\"body_size\":{},\"body_color\":{},\"body_sample_pages\":{},\"body_sample_text_pages\":{},\"body_given\":[{}]",
            json_number(b.size),
            serde_json::Value::String(b.color.clone()),
            b.sample_pages,
            b.sample_text_pages,
            given.join(",")
        )
    }

    /// The body estimate and the range, as JSON members (each with a leading comma).
    fn body_json(&self) -> String {
        let mut s = self.body_members();
        if let Some((seeded, all)) = self.heading_sizes {
            s += &format!(",\"heading_sizes\":{{\"seeded\":{seeded},\"total\":{all}}}");
        }
        if let Some(r) = &self.range {
            let why: Vec<String> = r.identity.iter().map(|w| format!("\"{w}\"")).collect();
            s += &format!(
                ",\"range\":{{\"first_page\":{},\"context_before\":{},\"context_after\":{},\"identity\":[{}],\"doc_work_limit\":{}}}",
                r.first_page,
                r.context_before,
                r.context_after,
                why.join(","),
                json_number(r.doc_work_limit)
            );
        }
        s
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
                "\"doc_work\":{},\"doc_work_limit\":{},\"page_work_max\":{},\"degraded\":[{}]{}{}{}{}{}}}"
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
            json_number(self.doc_work_limit),
            self.page_work_max,
            degraded.join(","),
            self.body_json(),
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

/// A finite number as JSON (`null` otherwise); integers without a fraction.
fn json_number(v: f64) -> String {
    if v.is_finite() { format!("{v}") } else { "null".to_string() }
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

/// How the body font size and colour are estimated (see [`Window`]).
/// `Pages` and `TextPages` also end once the pages held reach [`BODY_SAMPLE_MAX_ITEMS`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BodySample {
    /// over every page of the document (every page waits for the last one)
    Document,
    /// over the first N pages, blank ones included (the windowed rule before 2026-11:
    /// N = window + 1, which takes front matter for the body on small windows)
    Pages(usize),
    /// over the pages up to and including the N-th that has text, or the first 2N pages when
    /// fewer have text (scans without a text layer): pages are held, and none is written, until
    /// then (or the end of the input)
    TextPages(usize),
}

/// The body sample of a furniture window by default: pages up to the 16th with text. Measured
/// on 18 long PDFs (FURNITURE.md), 16 pages give the whole document's body size on 16 of 18,
/// the same as 32 (which misses a different one), with no change in the judged golden cases;
/// shorter samples (the old first window + 1 pages) gave it on 4 to 13 of 15. 16 halves what
/// is held before the first page is written (owner decision).
pub const DEFAULT_BODY_PAGES: usize = 16;

/// A page-count body sample ([`BodySample::Pages`], [`BodySample::TextPages`]) is also complete
/// once the pages it holds have this many words, rules and glyph records in all: the
/// pages held before the first is written are bounded in size, not only in number. Ordinary
/// text pages hold a few thousand; 32 of them stay well under it.
pub const BODY_SAMPLE_MAX_ITEMS: usize = 256 * 1024;

/// The furniture window of `pdftomarkdown --stream` (pages either side). 4 (owner decision,
/// FURNITURE.md): with the body sample separated, it leaves about 15% fewer running heads and
/// footers than 16 with no change in the judged golden cases; 16 agrees more closely with the
/// whole-document run, which itself keeps the most running heads.
pub const STREAM_WINDOW: usize = 4;

/// What a [`Converter`] looks at beyond the page it writes, and which pages it writes.
#[derive(Clone, Debug, PartialEq)]
pub struct Window {
    /// Furniture window: running headers and footers, repeated watermark words and backdrop
    /// pictures are judged over this many pages either side of each page. `None`: over the
    /// whole document (every page waits for the last one).
    pub furniture: Option<usize>,
    /// Pages the body size and colour are estimated from. `None`:
    /// `TextPages(DEFAULT_BODY_PAGES)` with a furniture window, else `Document`.
    pub body: Option<BodySample>,
    /// Body font size given by the caller (no estimate).
    pub body_size: Option<f64>,
    /// Body colour given by the caller (no estimate).
    pub body_color: Option<String>,
    /// Leading context: the first `skip` pages are read (they count towards repetition and
    /// the body estimate) but not written or reported.
    pub skip: usize,
    /// Pages written after the leading context; the pages after them are trailing context.
    /// `None`: every page after the leading context.
    pub emit: Option<usize>,
    /// Trailing context pages asked for (for the report's identity check only).
    pub after: usize,
    /// The PDF page number of the first page read (1 for a whole document): the document
    /// work budget at a page depends on it, and the report's identity check.
    pub first_page: u32,
    /// How heading levels are set (see [`HeadingLevels`]).
    pub headings: HeadingLevels,
}

/// How heading levels are set.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum HeadingLevels {
    /// by the heading's size relative to the body size (`h1_ratio`, `h2_ratio`)
    #[default]
    Size,
    /// by the rank of the heading's size among the document's heading sizes, seeded from the
    /// first pages and only added to after (see [`heading_levels`]); the body size still
    /// decides which paragraphs are headings
    Rank,
}

impl Default for Window {
    fn default() -> Self {
        Window {
            furniture: None,
            body: None,
            body_size: None,
            body_color: None,
            skip: 0,
            emit: None,
            after: 0,
            first_page: 1,
            headings: HeadingLevels::Size,
        }
    }
}

impl Window {
    /// A furniture window of `w` pages either side (`None`: the whole document) with the
    /// default body estimate.
    pub fn furniture(w: Option<usize>) -> Self {
        Window { furniture: w, ..Window::default() }
    }

    /// The body sample this window uses.
    pub fn body_sample(&self) -> BodySample {
        self.body.unwrap_or(match self.furniture {
            Some(_) => BodySample::TextPages(DEFAULT_BODY_PAGES),
            None => BodySample::Document,
        })
    }

    /// Whether this is a page range (leading or trailing context, or a range that does not start
    /// at the first page).
    fn is_range(&self) -> bool {
        self.skip > 0 || self.emit.is_some() || self.first_page > 1
    }
}

/// The body estimate a conversion used (always in the report).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BodyInfo {
    pub size: f64,
    pub color: String,
    /// pages read when the estimate was fixed (0 when both were given)
    pub sample_pages: usize,
    /// of those, pages with text
    pub sample_text_pages: usize,
    pub size_given: bool,
    pub color_given: bool,
}

/// A page range converted with context: what was read around it, and why its pages may differ
/// from the same pages of a whole-document conversion with the same window.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RangeInfo {
    /// PDF page number of the first page written
    pub first_page: u32,
    pub context_before: usize,
    pub context_after: usize,
    /// reasons the written pages may differ from a whole-document run (empty: identical,
    /// provided the whole run degrades no page for the document budget)
    pub identity: Vec<&'static str>,
    /// the document budget at the first page written (it only grows after it): the ranges of a
    /// document are identical to the whole run when, for each range, its `doc_work` and that of
    /// the ranges before it sum to at most this
    pub doc_work_limit: f64,
}

/// Page-at-a-time conversion with a bounded lookahead. Two decisions look beyond the page: the
/// body font size and colour (estimated from a sample of pages and fixed before the first page
/// is written, see [`BodySample`], or given), and running headers and footers (a margin line
/// repeated on `furniture_min_share` of the pages within the furniture window either side).
/// The converter holds the pages read but not yet written: at most the furniture window + 1,
/// or the body sample when it is longer (a page-count sample also ends at
/// [`BODY_SAMPLE_MAX_ITEMS`]), plus the margin keys of `window` written pages.
///
/// With no furniture window and the document body estimate (`Window::default()`), both
/// decisions see the whole document and the output is [`to_markdown_report_with`]'s. A window
/// at least as long as the document does too only when the body sample also covers the
/// document ([`BodySample::Document`], or a given body equal to the whole document's): the
/// default sample, 32 pages with text, sees only the start of a longer document.
///
/// Every detection of content by its repetition across pages (running headers and footers,
/// repeated watermark words and backdrop pictures) sees only the furniture window.
pub struct Converter<'p> {
    p: &'p Params,
    o: Options,
    win: Window,
    sample: BodySample,
    /// received pages not yet written, oldest first: page `next` onwards
    ahead: VecDeque<(Page, Margins)>,
    /// margins of written pages still inside the window, oldest first
    behind: VecDeque<Margins>,
    counts: MarginCounts,
    /// pages in `counts`: the window around the next page to write
    counted: usize,
    /// pages whose margins went into `counts` (0..added)
    added: usize,
    received: usize,
    text_pages: usize,
    /// words, rules and glyph records of the pages received (for the body sample)
    items: usize,
    /// the body estimate was fixed by the end of the input, not by a complete sample
    body_at_end: bool,
    next: usize,
    body: Option<(f64, String)>,
    body_info: BodyInfo,
    spent: u64,
    report: Report,
    /// what the page just written left open, for the next to continue (only with markers)
    carry: stitch::Carry,
    /// `carry` may differ from a whole-document run's (a range whose context does not reach
    /// back to the start of a table that runs on)
    carry_unsure: bool,
    /// `carry_unsure` when the first written page was reached
    carry_unsure_at_first: bool,
    /// lines and words of the context pages (taken out of the parse report)
    context_lines: usize,
    context_words: usize,
    /// this conversion's seal on its own markers (see [`images::seal`])
    seal: String,
    /// with [`HeadingLevels::Rank`], the document's heading sizes once seeded
    headings: Option<heading_levels::HeadingTable>,
    /// the heading seed was fixed by the end of the input, not by a complete sample
    headings_at_end: bool,
}

impl<'p> Converter<'p> {
    /// A converter with a furniture window of `window` pages (`None`: the whole document) and
    /// the default body estimate.
    pub fn new(p: &'p Params, o: Options, window: Option<usize>) -> Self {
        Self::with(p, o, Window::furniture(window))
    }

    pub fn with(p: &'p Params, o: Options, win: Window) -> Self {
        let sample = win.body_sample();
        let body_info = BodyInfo {
            size_given: win.body_size.is_some(),
            color_given: win.body_color.is_some(),
            ..BodyInfo::default()
        };
        let carry_unsure = o.markers && win.first_page > 1;
        let mut c = Converter {
            p,
            o,
            win,
            sample,
            ahead: VecDeque::new(),
            behind: VecDeque::new(),
            counts: MarginCounts::default(),
            counted: 0,
            added: 0,
            received: 0,
            text_pages: 0,
            items: 0,
            body_at_end: false,
            next: 0,
            body: None,
            body_info,
            spent: 0,
            report: Report::default(),
            carry: stitch::Carry::default(),
            carry_unsure,
            carry_unsure_at_first: false,
            context_lines: 0,
            context_words: 0,
            seal: if o.markers { images::seal() } else { String::new() },
            headings: None,
            headings_at_end: false,
        };
        c.decide_body(false);
        c
    }

    /// The body (size, colour), once fixed: when the sample is complete, both were given, or
    /// at the end of the input.
    pub fn body(&self) -> Option<(f64, &str)> {
        self.body.as_ref().map(|(s, c)| (*s, c.as_str()))
    }

    /// Whether the page read at local index `i` is written (not context).
    fn emitted(&self, i: usize) -> bool {
        i >= self.win.skip && self.win.emit.is_none_or(|e| i - self.win.skip < e)
    }

    /// Fixes the body when the sample is complete (`end`: the input ended).
    fn decide_body(&mut self, end: bool) {
        if self.body.is_some() {
            return;
        }
        let given = (self.win.body_size, self.win.body_color.clone());
        let both_given = matches!(given, (Some(_), Some(_)));
        let complete = both_given
            || match self.sample {
                BodySample::Document => false,
                BodySample::Pages(n) => self.received >= n || self.items >= BODY_SAMPLE_MAX_ITEMS,
                BodySample::TextPages(n) => {
                    self.text_pages >= n || self.received >= n.saturating_mul(2) || self.items >= BODY_SAMPLE_MAX_ITEMS
                }
            };
        if !(end || complete) {
            return;
        }
        self.body_at_end = !complete;
        let (size, color) = match given {
            (Some(s), Some(c)) => (s, c),
            (s, c) => {
                self.body_info.sample_pages = self.received;
                self.body_info.sample_text_pages = self.text_pages;
                let (size, color) = body_size(self.ahead.iter().map(|(pg, _)| pg));
                (s.unwrap_or(size), c.unwrap_or(color))
            }
        };
        self.body_info.size = size;
        self.body_info.color = color.clone();
        self.body = Some((size, color));
    }

    /// Seeds the heading table (with [`HeadingLevels::Rank`]) once the first
    /// [`DEFAULT_BODY_PAGES`] pages with text are read, as the body's `TextPages` sample (`end`:
    /// the input ended). Nothing is written before.
    fn decide_headings(&mut self, end: bool) {
        if self.win.headings != HeadingLevels::Rank || self.headings.is_some() {
            return;
        }
        let n = DEFAULT_BODY_PAGES;
        let complete =
            self.text_pages >= n || self.received >= n.saturating_mul(2) || self.items >= BODY_SAMPLE_MAX_ITEMS;
        if !(end || complete) {
            return;
        }
        self.headings_at_end = !complete;
        // no page has been written yet: `ahead` holds every page read
        self.headings = Some(heading_levels::HeadingTable::seed(self.ahead.iter().map(|(pg, _)| pg), self.p));
    }

    /// Whether pages can be written: the body is fixed, and the heading table seeded when
    /// levels are ranked.
    fn ready(&self) -> bool {
        self.body.is_some() && (self.win.headings != HeadingLevels::Rank || self.headings.is_some())
    }

    /// Adds the next page; passes (page index, Markdown) to `out` for each page whose window
    /// is now complete. Page indices count the written pages only (context pages are not).
    pub fn push(&mut self, mut page: Page, out: &mut dyn FnMut(usize, String)) {
        let local = self.received;
        let emitted = self.emitted(local);
        if !emitted {
            self.context_lines += page.lines.len();
            self.context_words += page.lines.iter().map(|l| l.words.len()).sum::<usize>();
        }
        if !self.o.markers {
            page.images = Vec::new();
            page.graphics = Vec::new();
        }
        if self.p.watermarks {
            let n = watermark::strip_local(&mut page, self.p);
            if emitted {
                self.report.watermark_words += n;
            }
        }
        if self.o.math {
            let (lines, stats) = math::rewrite_page(&page, &math::MathOptions::default());
            page.lines = lines;
            if emitted {
                let pi = local - self.win.skip;
                if stats.ocr_layer {
                    self.report.degraded.push((pi, Degraded::OcrTextLayer));
                } else if stats.unreadable {
                    self.report.degraded.push((pi, Degraded::UnreadableText));
                }
                self.report.math.push(stats);
            }
        }
        if page.lines.iter().any(|l| !l.words.is_empty()) {
            self.text_pages += 1;
        }
        let words: usize = page.lines.iter().map(|l| l.words.len()).sum();
        // images and graphics are capped per page by the parser, and counting them would make
        // the sample depend on --markers
        self.items = self.items.saturating_add(words + page.rules.len() + page.glyphs.len() + page.extra_glyphs.len());
        let m = Margins::of(&page, self.p);
        self.received += 1;
        self.ahead.push_back((page, m));
        self.decide_body(false);
        self.decide_headings(false);
        while self.ready() && self.win.furniture.is_some_and(|w| self.ahead.len() > w) {
            self.write_next(out);
        }
    }

    /// Ends the input: writes the remaining pages and returns the report, with `parse` as its
    /// parse report.
    pub fn finish(mut self, mut parse: ParseReport, out: &mut dyn FnMut(usize, String)) -> Report {
        self.decide_body(true);
        self.decide_headings(true);
        while !self.ahead.is_empty() {
            self.write_next(out);
        }
        // the parse report covers the written pages only
        let context = self.received - (self.next_emitted());
        if self.win.is_range() {
            let (skip, kept) = (self.win.skip, self.next_emitted());
            let in_range = |i: &usize| *i >= skip && *i - skip < kept;
            parse.rules_truncated_pages =
                parse.rules_truncated_pages.iter().filter(|i| in_range(i)).map(|i| i - skip).collect();
            parse.glyphs_truncated_pages =
                parse.glyphs_truncated_pages.iter().filter(|i| in_range(i)).map(|i| i - skip).collect();
            let info: Vec<_> =
                parse.page_info.iter().enumerate().filter(|(i, _)| in_range(i)).map(|(_, p)| p.clone()).collect();
            parse.page_info = info;
            parse.pages = parse.pages.saturating_sub(context);
            parse.page_closes = parse.page_closes.saturating_sub(context);
            parse.lines = parse.lines.saturating_sub(self.context_lines);
            parse.words = parse.words.saturating_sub(self.context_words);
            self.report.range = Some(self.range_info());
        }
        self.report.parse = parse;
        self.report.pages_read = self.received;
        self.report.doc_work = self.spent;
        self.report.doc_work_limit =
            self.p.doc_work_limit((self.win.first_page as usize).saturating_sub(1) + self.received);
        self.report.body = self.body_info.clone();
        self.report.heading_sizes = self.headings.as_ref().map(|t| (t.seeded, t.len()));
        self.report
    }

    /// Pages written (not context).
    fn next_emitted(&self) -> usize {
        let after_skip = self.received.saturating_sub(self.win.skip);
        self.win.emit.map_or(after_skip, |e| after_skip.min(e))
    }

    fn range_info(&self) -> RangeInfo {
        let w = &self.win;
        let emitted = self.next_emitted();
        let got_after = self.received - w.skip.min(self.received) - emitted;
        let first_written = w.first_page as usize + w.skip;
        let mut identity: Vec<&'static str> = Vec::new();
        // the body estimate saw other pages than a whole run's
        let given = self.body_info.size_given && self.body_info.color_given;
        // the range ended before the sample did
        let sample_cut = self.body_at_end;
        if !given && (w.first_page > 1 || (w.emit.is_some() && sample_cut)) {
            identity.push("body");
        }
        match w.furniture {
            None => identity.push("furniture"),
            Some(f) => {
                // too few pages either side, unless the document starts or ends there
                let short_before = w.skip < f && w.first_page > 1;
                let short_after = w.emit.is_some() && got_after == w.after && w.after < f;
                if short_before || short_after {
                    identity.push("context");
                }
            }
        }
        if self.carry_unsure_at_first {
            identity.push("markers");
        }
        // ranked heading levels: the seed is the document's first pages, and sizes are added
        // in page order from there
        if w.headings == HeadingLevels::Rank && (w.first_page > 1 || (w.emit.is_some() && self.headings_at_end)) {
            identity.push("headings");
        }
        if first_written > 1 {
            // the work of the pages before the range is not charged here
            identity.push("doc_work");
        }
        RangeInfo {
            first_page: first_written as u32,
            context_before: w.skip.min(self.received),
            context_after: got_after,
            identity,
            doc_work_limit: self.p.doc_work_limit(first_written),
        }
    }

    fn write_next(&mut self, out: &mut dyn FnMut(usize, String)) {
        if self.ahead.is_empty() {
            return;
        }
        // margins of the pages up to `furniture` after this one go into the counts
        let upto = match self.win.furniture {
            Some(w) => self.next.saturating_add(w).saturating_add(1).min(self.received),
            None => self.received,
        };
        while self.added < upto {
            if let Some((_, m)) = self.ahead.get(self.added - self.next) {
                self.counts.add(m);
            }
            self.counted += 1;
            self.added += 1;
        }
        let Some((page, m)) = self.ahead.pop_front() else { return };
        let local = self.next;
        self.next += 1;
        if let Some(w) = self.win.furniture {
            while self.behind.len() > w {
                if let Some(old) = self.behind.pop_front() {
                    self.counts.remove(&old);
                    self.counted -= 1;
                }
            }
        }
        let emitted = self.emitted(local);
        let before = local < self.win.skip;
        if emitted {
            if local == self.win.skip {
                self.carry_unsure_at_first = self.carry_unsure;
            }
            self.write_page(local - self.win.skip, page, &m, out);
        } else if before && self.o.markers {
            // a leading context page is laid out (not written, not reported) only for what it
            // leaves open for the next page (continuation markers). Its work is charged to
            // the document budget (a whole run charges it too, and more before it), so the
            // range stays within the document budget
            let report = std::mem::take(&mut self.report);
            let unsure = self.carry_unsure;
            self.write_page(local, page, &m, &mut |_, _| {});
            // what this page leaves open depends on what it was handed only through a table
            // that runs on from the previous page to its end
            self.carry_unsure = unsure && self.carry.depends_on_prev();
            self.report = report;
        }
        if self.win.furniture.is_some() {
            self.behind.push_back(m);
        }
    }

    /// Lays out one page and passes its Markdown to `out` (`pi`: its index in the report).
    fn write_page(&mut self, pi: usize, mut page: Page, m: &Margins, out: &mut dyn FnMut(usize, String)) {
        let math_params = Params { display_math_paragraphs: true, ..self.p.clone() };
        let p = if self.o.math { &math_params } else { self.p };
        let (body, body_color) = self.body.clone().unwrap_or((10.0, String::new()));
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
        let drop = furniture(&page, fresh.as_ref().unwrap_or(m), &self.counts, self.counted, p);
        let n = page.lines.len();
        // the budget grows with the pages of the document up to this one (the PDF's page
        // number: a range or a stream sees what a whole run sees here)
        let doc_left = p.doc_work_limit((self.win.first_page as usize).saturating_sub(1) + self.next)
            - self.spent as f64
            - n as f64;
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
                if let Some(table) = &mut self.headings {
                    layout.relevel(table, p, &mut budget);
                }
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
