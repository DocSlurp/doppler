//! Words, lines and pages, and the tag scanner over doppler XHTML (`parse` in doppler_md.py).

use memchr::{memchr, memmem};
use std::borrow::Cow;
use std::cell::OnceCell;
use std::rc::Rc;

use crate::patterns::leading_punct;
use crate::pyc::{self, median, pmax, py_float};

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

/// `LEADING_PUNCT_RE.sub(r'\2\1', t)`: ",28" drawn left of a number reads "28,".
fn leading_punct_sub(t: &str) -> Cow<'_, str> {
    if let Some((punct, rest)) = leading_punct(t) {
        return Cow::Owned(format!("{rest}{punct}"));
    }
    if let Some(s) = t.strip_suffix('\n')
        && let Some((punct, rest)) = leading_punct(s)
    {
        return Cow::Owned(format!("{rest}{punct}\n"));
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
    /// Quarter turns of the glyphs; absent or invalid rotation reads as zero.
    pub rot: u8,
    /// Invisible text (render mode 3, such as an OCR layer over a scan); read only with glyphs.
    pub invisible: bool,
    /// This word's glyph records (`pdftotext -glyphs`) as a range of [`Page::glyphs`]; empty
    /// without glyph records.
    pub glyphs: GlyphRange,
}

/// A range of [`Page::glyphs`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GlyphRange {
    pub start: u32,
    pub end: u32,
}

impl GlyphRange {
    pub fn len(&self) -> usize {
        self.end.saturating_sub(self.start) as usize
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub fn range(&self) -> std::ops::Range<usize> {
        self.start as usize..self.end.max(self.start) as usize
    }
}

/// Where a glyph's Unicode came from (`s` of a `pdftotext -glyphs` record).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UniSource {
    /// the font's ToUnicode CMap
    ToUnicode,
    /// the glyph name
    Name,
    /// a fallback from the character code, the built-in encoding or the CID collection
    Code,
    /// the text of an /ActualText span (one record for the whole span)
    ActualText,
}

/// One glyph of a word (`<g>` after a `<word>` with `pdftotext -glyphs`).
#[derive(Clone, Debug)]
pub struct Glyph {
    /// left edge and advance along the line
    pub x: f64,
    pub w: f64,
    /// baseline and font size, when they differ from the word's
    pub base: Option<f64>,
    pub size: Option<f64>,
    /// index into [`Page::fonts`], if the record named a font in the page's table
    pub font: Option<u16>,
    /// font-local character code (the CID for CID fonts); `u32::MAX` when absent
    pub code: u32,
    /// Unicode text (at most [`MAX_GLYPH_CHARS`] characters)
    pub text: Rc<str>,
    pub src: UniSource,
    /// glyph name, when the record had one
    pub name: Option<Rc<str>>,
}

/// Code points kept per glyph record (the producer writes at most 32).
pub const MAX_GLYPH_CHARS: usize = 32;
/// Glyph records kept per page and per document; later records are dropped and counted.
pub const MAX_PAGE_GLYPHS: usize = 100_000;
pub const MAX_DOC_GLYPHS: usize = 4_000_000;
/// Fonts kept per page table.
pub const MAX_PAGE_FONTS: usize = 1000;

/// One entry of a page's font table (`<font>` with `pdftotext -glyphs`).
#[derive(Clone, Debug, Default)]
pub struct FontInfo {
    /// the producer's document-wide font id
    pub id: i64,
    /// base font name without the subset tag
    pub name: Rc<str>,
    /// Type1, Type1C, TrueType, CIDType0C, Type3, ...
    pub kind: Rc<str>,
    pub encoding: Rc<str>,
    pub embedded: bool,
    pub symbolic: bool,
    pub cid: bool,
    pub has_to_unicode: bool,
    /// codes used on the page that the ToUnicode CMap does not map
    pub to_unicode_misses: u32,
}

impl Word {
    /// (bold, italic) from a font name, as the `Word.bold` / `Word.italic` properties.
    pub fn font_style(font: &str) -> (bool, bool) {
        let f = pyc::py_lower(font);
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

/// A visible image or image mask (doppler `-images`): where it lands on the page, and its own
/// size in samples. The image data is never read by the producer or the converter.
#[derive(Clone, Debug)]
pub struct Image {
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
    /// samples across and down, as the PDF declares them
    pub px_w: u32,
    pub px_h: u32,
    /// a stencil mask, not a picture
    pub mask: bool,
    /// the image XObject's object and generation numbers (`ref="num gen"`), so repeats of one
    /// object can be found; `None` for an inline image or a producer without it
    pub obj: Option<(u32, u32)>,
    /// how its pixels can be taken out without rendering (`copy=`, from its dictionary only)
    pub copy: CopyClass,
    /// a copy of its pixels shows what the page shows there: its rows and columns land on the
    /// page as stored (`orient="0"`) and nothing else shows over it, cuts or masks it
    /// (`overlaid="0"`). Either attribute missing reads as not, so an older producer renders.
    pub copy_exact: bool,
    /// a render of its box shows its own pixels and nothing else, so another placement of the
    /// same object at the same size renders the same: [`Image::copy_exact`], not an image mask
    /// (painted in the fill colour) and not masked (`smask=`, it shows what lies under it)
    pub plain: bool,
}

/// An image's copy class, as the producer's `copy=` says it: `jpeg` (a JPEG to copy unchanged),
/// `bilevel` (1-bit samples), `decode` (other samples decodable at native size), or `render`
/// (anything else, an inline image, or a producer without the attribute).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum CopyClass {
    #[default]
    Render,
    Jpeg,
    Bilevel,
    Decode,
}

/// A full-page scan, as the producer's `scan=` and `scanImage=` on `<page>` say it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScanInfo {
    pub kind: ScanKind,
    /// the covering image's one-based position in the page's image list
    pub image: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScanKind {
    /// one image covers the page
    Single,
    /// several images (strips, tiles) do together
    Tiled,
    /// the covering image is masked or carries another page-sized image (mixed raster content)
    Layered,
}

/// The most images one page keeps; a page with more is `images_truncated`.
pub const MAX_PAGE_IMAGES: usize = 2000;

/// A cluster of painted vector drawing that is not axis-aligned (doppler `-images`): curves and
/// diagonal segments, the strokes of charts and diagrams. Rules, boxes and grids are not in it.
#[derive(Clone, Debug)]
pub struct Graphic {
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
    /// painted paths
    pub paths: u64,
    /// diagonal straight segments plus curve segments
    pub segments: u64,
    /// Bezier curve segments
    pub curves: u64,
    /// in how many of 8 sectors of 180 degrees its diagonal segments point (0..=8)
    pub directions: u32,
}

/// The most vector drawing clusters one page keeps (the producer writes at most 256).
pub const MAX_PAGE_GRAPHICS: usize = 1024;

#[derive(Debug)]
pub struct Page {
    /// the PDF's own page number, from `number="N"` (doppler `-images` writes it)
    pub number: Option<u32>,
    /// doppler `-images`
    pub images: Vec<Image>,
    /// the producer or the parser dropped images of this page
    pub images_truncated: bool,
    /// doppler `-images`: the page is a full-page scan (`scan=`, `scanImage=`)
    pub scan: Option<ScanInfo>,
    /// doppler `-images`: clusters of drawing that is not axis-aligned
    pub graphics: Vec<Graphic>,
    pub width: f64,
    pub height: f64,
    /// in poppler reading order
    pub lines: Vec<Rc<Line>>,
    /// doppler -rules
    pub rules: Vec<Rule>,
    /// Upstream rule completeness: `Some(true)` for rulesTruncated="1", `Some(false)`
    /// for "0", and `None` when absent or unrecognized. Independent of kept rule count.
    pub rules_truncated: Option<bool>,
    /// glyph records of the page's words (only when parsed with glyphs; see [`Word::glyphs`])
    pub glyphs: Vec<Glyph>,
    /// the page's font table (only when parsed with glyphs)
    pub fonts: Vec<FontInfo>,
    /// named glyphs that belong to no word (`<xg>`), with their baseline and size
    pub extra_glyphs: Vec<Glyph>,
    /// the producer or the parser dropped glyph records of this page
    pub glyphs_truncated: bool,
}

impl Page {
    /// The glyphs of a word of this page.
    pub fn word_glyphs(&self, w: &Word) -> &[Glyph] {
        self.glyphs.get(w.glyphs.range()).unwrap_or(&[])
    }

    /// The font of a glyph.
    pub fn font(&self, g: &Glyph) -> Option<&FontInfo> {
        g.font.and_then(|i| self.fonts.get(i as usize))
    }
}

/// The attribute names the converter reads.
const KEYS: [&str; 30] = [
    "xMin",
    "yMin",
    "xMax",
    "yMax",
    "fontName",
    "fontSize",
    "spaceAfter",
    "color",
    "baseline",
    "width",
    "height",
    "rulesTruncated",
    "glyphsTruncated",
    "invisible",
    "rotation",
    "number",
    "imagesTruncated",
    "kind",
    "paths",
    "segments",
    "curves",
    "directions",
    "ref",
    "crop",
    "copy",
    "orient",
    "scan",
    "scanImage",
    "overlaid",
    "smask",
];

fn key_index(name: &str) -> Option<usize> {
    // (a match compiles to a switch on length, then one comparison)
    Some(match name {
        "xMin" => 0,
        "yMin" => 1,
        "xMax" => 2,
        "yMax" => 3,
        "fontName" => 4,
        "fontSize" => 5,
        "spaceAfter" => 6,
        "color" => 7,
        "baseline" => 8,
        "width" => 9,
        "height" => 10,
        "rulesTruncated" => 11,
        "glyphsTruncated" => 12,
        "invisible" => 13,
        "rotation" => 14,
        "number" => 15,
        "imagesTruncated" => 16,
        "kind" => 17,
        "paths" => 18,
        "segments" => 19,
        "curves" => 20,
        "directions" => 21,
        "ref" => 22,
        "crop" => 23,
        "copy" => 24,
        "orient" => 25,
        "scan" => 26,
        "scanImage" => 27,
        "overlaid" => 28,
        "smask" => 29,
        _ => return None,
    })
}

/// ASCII bytes in `re` `\w` (alphanumerics and underscore).
static ASCII_WORD: [bool; 128] = {
    let mut t = [false; 128];
    let mut c = 0;
    while c < 128 {
        t[c] = (c as u8).is_ascii_alphanumeric() || c == b'_' as usize;
        c += 1;
    }
    t
};

/// `dict(ATTR_RE.findall(s))` with `(\w+)="([^"]*)"`, keeping only the names in [`KEYS`];
/// later duplicates win, as in the dict.
pub struct Attrs<'a>([Option<&'a str>; KEYS.len()]);

impl<'a> Attrs<'a> {
    pub fn parse(s: &'a str) -> Attrs<'a> {
        let mut out = Attrs([None; KEYS.len()]);
        out.parse_into(s);
        out
    }

    /// Clear and refill from `s`.
    pub fn parse_into(&mut self, s: &'a str) {
        parse_attrs(s, &mut self.0, key_index);
    }

    /// The value of `k`, which must be one of [`KEYS`] (any other name reads as absent).
    pub fn get(&self, k: &str) -> Option<&'a str> {
        self.0[key_index(k)?]
    }
}

/// `(\w+)="([^"]*)"` pairs of `s` into the slots `key` names (later duplicates win); other
/// names are skipped. Linear: an unterminated value ends the scan.
#[inline]
fn parse_attrs<'a, const N: usize>(s: &'a str, out: &mut [Option<&'a str>; N], key: fn(&str) -> Option<usize>) {
    {
        *out = [None; N];
        let b = s.as_bytes();
        let mut i = 0;
        while i < b.len() {
            // maximal run of \w characters starting at i (ASCII bytes without decoding)
            let mut j = i;
            while j < b.len() {
                let c = b[j];
                if c < 0x80 {
                    if ASCII_WORD[c as usize] {
                        j += 1;
                        continue;
                    }
                    break;
                }
                match s[j..].chars().next() {
                    Some(ch) if pyc::is_word_char(ch) => j += ch.len_utf8(),
                    _ => break,
                }
            }
            if j == i {
                // not a word character: skip it (a whole code point; `i` is always a boundary)
                i += if b[i] < 0x80 { 1 } else { s[i..].chars().next().map_or(1, char::len_utf8) };
                continue;
            }
            // Every suffix of a failing run fails the same way, so resume after the run.
            if b.get(j) == Some(&b'=') && b.get(j + 1) == Some(&b'"') {
                let Some(q) = memchr(b'"', &b[j + 2..]) else {
                    // no closing quote anywhere, so no later pair can match either
                    break;
                };
                if let Some(k) = key(&s[i..j])
                    && k < N
                {
                    out[k] = Some(&s[j + 2..j + 2 + q]);
                }
                i = j + 2 + q + 1;
                continue;
            }
            i = j;
        }
    }
}

/// Attributes of a `<g>` glyph record.
const G_X: usize = 0;
const G_W: usize = 1;
const G_F: usize = 2;
const G_C: usize = 3;
const G_U: usize = 4;
const G_S: usize = 5;
const G_N: usize = 6;
const G_B: usize = 7;
const G_Z: usize = 8;
fn glyph_key(name: &str) -> Option<usize> {
    Some(match name {
        "x" => G_X,
        "w" => G_W,
        "f" => G_F,
        "c" => G_C,
        "u" => G_U,
        "s" => G_S,
        "n" => G_N,
        "b" => G_B,
        "z" => G_Z,
        _ => return None,
    })
}

/// Attributes of a `<font>` table entry.
const F_ID: usize = 0;
const F_NAME: usize = 1;
const F_TYPE: usize = 2;
const F_ENC: usize = 3;
const F_EMB: usize = 4;
const F_SYM: usize = 5;
const F_CID: usize = 6;
const F_TU: usize = 7;
const F_TUMISS: usize = 8;
fn font_key(name: &str) -> Option<usize> {
    Some(match name {
        "id" => F_ID,
        "name" => F_NAME,
        "type" => F_TYPE,
        "enc" => F_ENC,
        "emb" => F_EMB,
        "sym" => F_SYM,
        "cid" => F_CID,
        "tu" => F_TU,
        "tuMiss" => F_TUMISS,
        _ => return None,
    })
}

/// `u`: space-separated hex code points; invalid, surrogate or control values are dropped
/// and at most [`MAX_GLYPH_CHARS`] are kept.
fn glyph_text(u: &str) -> String {
    let mut out = String::new();
    for (n, h) in u.split(' ').filter(|h| !h.is_empty()).enumerate() {
        if n >= MAX_GLYPH_CHARS {
            break;
        }
        if h.len() > 8 {
            continue;
        }
        if let Ok(v) = u32::from_str_radix(h, 16)
            && let Some(c) = char::from_u32(v)
            && !is_break(c)
        {
            out.push(c);
        }
    }
    out
}

enum Tag<'a> {
    Open(&'a str, &'a str),
    Close(&'a str),
    Word(&'a str, &'a str),
    Rule(bool, &'a str),
    /// `<image .../>`
    Image(&'a str),
    /// `<graphic .../>`
    Graphic(&'a str),
    /// `<g .../>` (only when parsing glyphs)
    Glyph(&'a str),
    /// `<font .../>` (only when parsing glyphs)
    Font(&'a str),
    /// `<xg .../>`: a named glyph that belongs to no word (only when parsing glyphs)
    Extra(&'a str),
}

const NAMES: [&str; 4] = ["page", "flow", "block", "line"];

/// The next occurrence of a byte (or of `</word>`) at or after a position, remembering the
/// last answer. Queries only move forward during a scan, so every search resumes where the
/// last one ended and the whole scan stays linear, even on hostile input full of unclosed tags.
#[derive(Default)]
struct Next {
    /// (searched from, first hit at or after it)
    last: Option<(usize, Option<usize>)>,
}

impl Next {
    fn find(&mut self, b: &[u8], from: usize, search: impl Fn(&[u8]) -> Option<usize>) -> Option<usize> {
        if let Some((start, hit)) = self.last
            && start <= from
        {
            match hit {
                None => return None,
                Some(h) if h >= from => return Some(h),
                _ => {}
            }
        }
        let hit = search(&b[from..]).map(|k| from + k);
        self.last = Some((from, hit));
        hit
    }
}

/// Forward-only searches for [`tag_at`].
struct Scan {
    gt: Next,
    newline: Next,
    word_close: Next,
    finder: memmem::Finder<'static>,
}

impl Scan {
    fn new() -> Scan {
        Scan {
            gt: Next::default(),
            newline: Next::default(),
            word_close: Next::default(),
            finder: memmem::Finder::new(b"</word>"),
        }
    }

    fn gt(&mut self, b: &[u8], from: usize) -> Option<usize> {
        self.gt.find(b, from, |h| memchr(b'>', h))
    }
}

/// One step of `TAG_RE.finditer`: the match starting at `lt` (a '<'), and its end. Calls must
/// come in nondecreasing `lt` order (see [`Next`]).
fn tag_at<'s>(s: &'s str, lt: usize, scan: &mut Scan, glyphs: bool) -> Option<(Tag<'s>, usize)> {
    let b = s.as_bytes();
    let rest = &s[lt + 1..];
    // <g ([^>]*)/> and <font ([^>]*)/>
    if glyphs {
        for (n, kind) in [("g ", 0), ("font ", 1), ("xg ", 2)] {
            if rest.starts_with(n.trim_end()) && rest.as_bytes().get(n.len() - 1).is_some_and(u8::is_ascii_whitespace) {
                let a = lt + 1 + n.len();
                let gt = scan.gt(b, a)?;
                if gt > a && b[gt - 1] == b'/' {
                    let attrs = &s[a..gt - 1];
                    let tag = match kind {
                        0 => Tag::Glyph(attrs),
                        1 => Tag::Font(attrs),
                        _ => Tag::Extra(attrs),
                    };
                    return Some((tag, gt + 1));
                }
                return None;
            }
        }
    }
    // </(page|flow|block|line)>
    if let Some(r) = rest.strip_prefix('/') {
        for n in NAMES {
            if r.starts_with(n) && r.as_bytes().get(n.len()) == Some(&b'>') {
                return Some((Tag::Close(n), lt + 2 + n.len() + 1));
            }
        }
        return None;
    }
    // <word ([^>]*)>(.*?)</word>
    if rest.starts_with("word ") {
        let a = lt + 6;
        let gt = scan.gt(b, a)?;
        let t = gt + 1;
        let finder = &scan.finder;
        let end = scan.word_close.find(b, t, |h| finder.find(h))?;
        // `.` does not match a newline
        if scan.newline.find(b, t, |h| memchr(b'\n', h)).is_some_and(|nl| nl < end) {
            return None;
        }
        return Some((Tag::Word(&s[a..gt], &s[t..end]), end + 7));
    }
    // <(page|flow|block|line)\b([^>]*)>
    for n in NAMES {
        if rest.starts_with(n) {
            let after = lt + 1 + n.len();
            if s[after..].chars().next().is_some_and(pyc::is_word_char) {
                return None;
            }
            let gt = scan.gt(b, after)?;
            return Some((Tag::Open(n, &s[after..gt]), gt + 1));
        }
    }
    // <image ([^>]*)/>
    if rest.starts_with("image ") {
        let a = lt + 1 + "image ".len();
        let gt = scan.gt(b, a)?;
        if gt > a && b[gt - 1] == b'/' {
            return Some((Tag::Image(&s[a..gt - 1]), gt + 1));
        }
        return None;
    }
    // <graphic ([^>]*)/>
    if rest.starts_with("graphic ") {
        let a = lt + 1 + "graphic ".len();
        let gt = scan.gt(b, a)?;
        if gt > a && b[gt - 1] == b'/' {
            return Some((Tag::Graphic(&s[a..gt - 1]), gt + 1));
        }
        return None;
    }
    // <(rule|box) ([^>]*)/>
    for (n, boxed) in [("rule ", false), ("box ", true)] {
        if rest.starts_with(n) {
            let a = lt + 1 + n.len();
            let gt = scan.gt(b, a)?;
            if gt > a && b[gt - 1] == b'/' {
                return Some((Tag::Rule(boxed, &s[a..gt - 1]), gt + 1));
            }
            return None;
        }
    }
    None
}

// indices into KEYS
const X_MIN: usize = 0;
const Y_MIN: usize = 1;
const X_MAX: usize = 2;
const Y_MAX: usize = 3;
const FONT_NAME: usize = 4;
const FONT_SIZE: usize = 5;
const SPACE_AFTER: usize = 6;
const COLOR: usize = 7;
const BASELINE: usize = 8;
const WIDTH: usize = 9;
const HEIGHT: usize = 10;
const RULES_TRUNCATED: usize = 11;
const GLYPHS_TRUNCATED: usize = 12;
const INVISIBLE: usize = 13;
const ROTATION: usize = 14;
const NUMBER: usize = 15;
const IMAGES_TRUNCATED: usize = 16;
const KIND: usize = 17;
const PATHS: usize = 18;
const SEGMENTS: usize = 19;
const CURVES: usize = 20;
const DIRECTIONS: usize = 21;
const REF: usize = 22;
const CROP: usize = 23;
const COPY: usize = 24;
const ORIENT: usize = 25;
const SCAN: usize = 26;
const SCAN_IMAGE: usize = 27;
const OVERLAID: usize = 28;
const SMASK: usize = 29;

/// `"num gen"`: two decimal numbers separated by one space, else `None`.
fn object_ref(v: &str) -> Option<(u32, u32)> {
    let (n, g) = v.split_once(' ')?;
    let digits = |s: &str| !s.is_empty() && s.len() <= 10 && s.bytes().all(|b| b.is_ascii_digit());
    if !digits(n) || !digits(g) {
        return None;
    }
    Some((n.parse().ok()?, g.parse().ok()?))
}

fn coord(a: &Attrs, k: usize) -> Option<f64> {
    py_float(a.0[k]?)
}

/// `float(a.get(k, 0) or 0)`
fn opt_num(a: &Attrs, k: usize) -> Option<f64> {
    match a.0[k] {
        None | Some("") => Some(0.0),
        Some(v) => py_float(v),
    }
}

/// Characters that would end a Markdown line or table row (`BREAKS` in doppler_md.py).
fn is_break(c: char) -> bool {
    matches!(c, '\t' | '\n' | '\r' | '\u{85}' | '\u{2028}' | '\u{2029}')
}

/// `CONTROL_RE.sub('', html.unescape(raw).translate(BREAKS))`: line and paragraph breaks
/// (literal or from `&#10;`-style references) become spaces, so word text cannot forge
/// Markdown lines or table rows; other control characters and U+FFFD are removed.
fn word_text(raw: &str) -> Rc<str> {
    let t = pyc::html_unescape(raw);
    // on the bytes: any C0 control, DEL, or the lead bytes of U+0085 (C2 85), U+2028/U+2029
    // (E2 80 A8/A9) and U+FFFD (EF BF BD)
    let special = t.bytes().any(|c| c < 0x20 || c == 0x7f || c == 0xc2 || c == 0xe2 || c == 0xef);
    if special && t.chars().any(|c| is_break(c) || is_control(c)) {
        Rc::from(
            t.chars()
                .filter_map(|c| if is_break(c) { Some(' ') } else { (!is_control(c)).then_some(c) })
                .collect::<String>(),
        )
    } else {
        Rc::from(&*t)
    }
}

/// `Rc<str>` for a value that usually repeats the previous one (colours): share the last one.
fn interned(last: &mut Option<Rc<str>>, v: &str) -> Rc<str> {
    match last {
        Some(rc) if **rc == *v => rc.clone(),
        _ => {
            let rc: Rc<str> = Rc::from(v);
            *last = Some(rc.clone());
            rc
        }
    }
}

/// What [`parse`] skipped or found incomplete (see [`crate::Report`]).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ParseReport {
    /// `<page>` tags
    pub pages: usize,
    /// `</page>` tags
    pub page_closes: usize,
    /// lines and words kept
    pub lines: usize,
    pub words: usize,
    /// words without one of xMin, yMin, xMax, yMax
    pub words_missing_coord: usize,
    /// words with a coordinate, fontSize or baseline that is not a finite number
    pub words_bad_number: usize,
    /// words outside any open `<line>`
    pub words_outside_line: usize,
    /// words of a `<line>` that never closed on a page (truncated input, nested or pageless lines)
    pub words_unclosed_line: usize,
    /// the input ended inside a `<line>` that had words
    pub unclosed_line_at_eof: bool,
    /// rules or boxes without four finite coordinates
    pub rules_skipped: usize,
    /// images without four finite coordinates, or past [`MAX_PAGE_IMAGES`] on their page
    pub images_skipped: usize,
    /// graphics without four finite, ordered coordinates, or past [`MAX_PAGE_GRAPHICS`]
    pub graphics_skipped: usize,
    /// Zero-based page indices marked rulesTruncated="1" by the producer, including pages
    /// with no kept rules. This describes incomplete evidence, not layout degradation.
    pub rules_truncated_pages: Vec<usize>,
    /// glyph records dropped: not after a kept word, malformed, or past the caps
    pub glyphs_skipped: usize,
    /// font table entries dropped (malformed or past the cap)
    pub fonts_skipped: usize,
    /// Zero-based page indices marked glyphsTruncated="1" by the producer
    pub glyphs_truncated_pages: Vec<usize>,
    /// one entry per `<page>`, in order: its text layer and size (see [`PageInfo`])
    pub page_info: Vec<PageInfo>,
}

/// What one page holds: kept words, how many of them are invisible (render mode 3, usually an
/// OCR layer over a scan), and its crop box size when the producer wrote it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PageInfo {
    /// `"W H"`: the crop box size as `pdfinfo` prints it (`%g`), from doppler
    /// `pdftotext -crop-sizes` (`crop="W H"`); `None` when absent or not a short number
    pub crop: Option<Box<str>>,
    /// the PDF's page number (`number="N"`), when the producer wrote it
    pub number: Option<u32>,
    pub words: u32,
    pub invisible: u32,
}

impl PageInfo {
    /// `none` (no words), `invisible` (every word invisible), `visible` (none invisible) or `mixed`.
    pub fn text_layer(&self) -> &'static str {
        match (self.words, self.invisible) {
            (0, _) => "none",
            (w, i) if i >= w => "invisible",
            (_, 0) => "visible",
            _ => "mixed",
        }
    }
}

/// `scan="single|tiled|layered"` with `scanImage="K"` (K from 1 to [`MAX_PAGE_IMAGES`]).
fn scan_of(kind: Option<&str>, image: Option<&str>) -> Option<ScanInfo> {
    let kind = match kind? {
        "single" => ScanKind::Single,
        "tiled" => ScanKind::Tiled,
        "layered" => ScanKind::Layered,
        _ => return None,
    };
    let image = image?;
    if image.is_empty() || image.len() > 6 || !image.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let image: usize = image.parse().ok()?;
    (1..=MAX_PAGE_IMAGES).contains(&image).then_some(ScanInfo { kind, image })
}

/// A `crop="W H"` value worth passing on: short, and only number characters.
fn crop_value(v: &str) -> bool {
    !v.is_empty() && v.len() <= 48 && v.bytes().all(|b| b.is_ascii_digit() || b".+-e infa".contains(&b))
}

/// Parse doppler XHTML into pages. A word or rule with a missing or malformed required number
/// (non-finite numbers count as malformed) is skipped; `doppler_md.py` skips the same ones.
pub fn parse(xhtml: &str) -> Vec<Page> {
    parse_report(xhtml, &mut ParseReport::default())
}

/// [`parse`], counting what was skipped into `report`. Glyph records and font tables are
/// skipped like any unknown element.
pub fn parse_report(xhtml: &str, report: &mut ParseReport) -> Vec<Page> {
    parse_with(xhtml, report, false)
}

/// [`parse_report`], also reading `pdftotext -glyphs` glyph records and font tables when
/// `glyphs` is set. Records that do not follow a kept word, or that have a malformed number,
/// are dropped and counted.
pub fn parse_with(xhtml: &str, report: &mut ParseReport, glyphs: bool) -> Vec<Page> {
    let mut st = ParseState { report: std::mem::take(report), glyphs, ..ParseState::default() };
    let mut pages: Vec<Page> = Vec::new();
    st.run(xhtml, true, &mut |pg| pages.push(pg));
    *report = st.finish(&mut |pg| pages.push(pg));
    pages
}

/// [`parse_report`] state between pieces of input: the open page and `<line>`, and the flow and
/// block counters, which run across pages.
struct ParseState {
    glyphs: bool,
    glyph_target: bool,
    line_glyphs: Vec<Glyph>,
    doc_glyphs: usize,
    page_glyphs: usize,
    last_glyph_name: Option<Rc<str>>,
    font_index: std::collections::HashMap<i64, u16>,

    /// the last `<page>`: complete once the next `<page>` opens or the input ends (lines and
    /// rules after its `</page>` still belong to it)
    page: Option<Page>,
    // words of the open <line> (`words` in Python is None until the first <line>)
    words: Vec<Word>,
    in_line: bool,
    flow: i64,
    block: i64,
    last_color: Option<Rc<str>>,
    report: ParseReport,
}

impl Default for ParseState {
    fn default() -> Self {
        ParseState {
            glyphs: false,
            glyph_target: false,
            line_glyphs: Vec::new(),
            doc_glyphs: 0,
            page_glyphs: 0,
            last_glyph_name: None,
            font_index: Default::default(),
            page: None,
            words: Vec::new(),
            in_line: false,
            flow: -1,
            block: -1,
            last_color: None,
            report: ParseReport::default(),
        }
    }
}

/// Whether more input could change what [`tag_at`] decided (`None`) for the `<` at `lt`: a
/// `Some` only ever comes from delimiters found in `s`, but a `None` may come from one not
/// found yet. The longest fixed prefix tag_at reads is under 16 bytes. The searches go through
/// the run's forward-only caches, so a long run of rejected `<` stays linear.
fn undecided(s: &str, lt: usize, scan: &mut Scan) -> bool {
    let b = s.as_bytes();
    if b.len() < lt + 16 {
        return true;
    }
    let Some(gt) = scan.gt(b, lt + 1) else { return true };
    if !b[lt + 1..].starts_with(b"word ") {
        return false;
    }
    // still open without a `</word>`, unless a newline after the tag has already ruled it out:
    // a `</word>` can only start after it
    let t = gt + 1;
    let finder = &scan.finder;
    scan.word_close.find(b, t, |h| finder.find(h)).is_none() && scan.newline.find(b, t, |h| memchr(b'\n', h)).is_none()
}

impl ParseState {
    /// Parses `s` (the input after what earlier runs consumed) and passes each completed page
    /// to `emit`. Unless `eof`, stops before a tag that more input could change and returns
    /// where it stopped; the next run starts there.
    fn run(&mut self, s: &str, eof: bool, emit: &mut dyn FnMut(Page)) -> usize {
        let ParseState {
            page,
            words,
            in_line,
            flow,
            block,
            last_color,
            report,
            glyphs,
            glyph_target,
            line_glyphs,
            doc_glyphs,
            page_glyphs,
            last_glyph_name,
            font_index,
        } = self;
        let glyphs = *glyphs;
        let mut ga = [None; 9];
        let mut fa = [None; 9];
        let b = s.as_bytes();
        let mut a = Attrs([None; KEYS.len()]);
        let mut scan = Scan::new();
        // font names repeat word after word: cache the last style
        let mut last_font: Option<(&str, (bool, bool))> = None;
        let mut pos = 0;
        while let Some(off) = memchr(b'<', &b[pos..]) {
            let lt = pos + off;
            let Some((tag, end)) = tag_at(s, lt, &mut scan, glyphs) else {
                if !eof && undecided(s, lt, &mut scan) {
                    return lt;
                }
                if glyphs
                    && ["g", "xg"].iter().any(|name| {
                        s[lt + 1..].strip_prefix(name).is_some_and(|rest| {
                            rest.as_bytes().first().is_none_or(|b| b.is_ascii_whitespace() || matches!(b, b'/' | b'>'))
                        })
                    })
                {
                    report.glyphs_skipped += 1;
                    if let Some(pg) = page.as_mut() {
                        pg.glyphs_truncated = true;
                    }
                }
                pos = lt + 1;
                continue;
            };
            pos = end;
            let have_page = page.is_some();
            if !matches!(tag, Tag::Glyph(_)) {
                *glyph_target = false;
            }
            match tag {
                Tag::Extra(attrs) => {
                    let Some(pg) = page.as_mut() else { continue };
                    if *page_glyphs >= MAX_PAGE_GLYPHS || *doc_glyphs >= MAX_DOC_GLYPHS {
                        report.glyphs_skipped += 1;
                        pg.glyphs_truncated = true;
                        continue;
                    }
                    parse_attrs(attrs, &mut ga, glyph_key);
                    let (Some(x), Some(w), Some(base), Some(size)) = (
                        ga[G_X].and_then(py_float),
                        ga[G_W].and_then(py_float),
                        ga[G_B].and_then(py_float),
                        ga[G_Z].and_then(py_float).filter(|z| *z > 0.0),
                    ) else {
                        report.glyphs_skipped += 1;
                        pg.glyphs_truncated = true;
                        continue;
                    };
                    let font = ga[G_F].and_then(|f| f.parse::<i64>().ok()).and_then(|id| font_index.get(&id).copied());
                    pg.extra_glyphs.push(Glyph {
                        x,
                        w,
                        base: Some(base),
                        size: Some(size),
                        font,
                        code: ga[G_C].and_then(|c| c.parse::<u32>().ok()).unwrap_or(u32::MAX),
                        text: Rc::from(""),
                        src: UniSource::Code,
                        name: ga[G_N].map(|n| Rc::from(&*pyc::html_unescape(n))),
                    });
                    *page_glyphs += 1;
                    *doc_glyphs += 1;
                }
                Tag::Glyph(attrs) => {
                    let Some(word) = words.last_mut().filter(|_| *glyph_target && *in_line) else {
                        report.glyphs_skipped += 1;
                        if let Some(pg) = page.as_mut() {
                            pg.glyphs_truncated = true;
                        }
                        continue;
                    };
                    if *page_glyphs >= MAX_PAGE_GLYPHS || *doc_glyphs >= MAX_DOC_GLYPHS {
                        report.glyphs_skipped += 1;
                        if let Some(pg) = page.as_mut() {
                            pg.glyphs_truncated = true;
                        }
                        continue;
                    }
                    parse_attrs(attrs, &mut ga, glyph_key);
                    let (Some(x), Some(w)) = (ga[G_X].and_then(py_float), ga[G_W].and_then(py_float)) else {
                        report.glyphs_skipped += 1;
                        if let Some(pg) = page.as_mut() {
                            pg.glyphs_truncated = true;
                        }
                        continue;
                    };
                    let unicode = ga[G_U].unwrap_or("");
                    let invalid_text = unicode.split(' ').filter(|h| !h.is_empty()).enumerate().any(|(n, h)| {
                        n >= MAX_GLYPH_CHARS
                            || h.len() > 8
                            || u32::from_str_radix(h, 16).ok().and_then(char::from_u32).is_none()
                    });
                    let invalid_geometry = ga[G_B].is_some_and(|v| py_float(v).is_none())
                        || ga[G_Z].is_some_and(|v| py_float(v).is_none_or(|z| z <= 0.0));
                    if invalid_text || invalid_geometry {
                        report.glyphs_skipped += 1;
                        if let Some(pg) = page.as_mut() {
                            pg.glyphs_truncated = true;
                        }
                        continue;
                    }
                    let src = match ga[G_S] {
                        Some("t") => UniSource::ToUnicode,
                        Some("n") => UniSource::Name,
                        Some("a") => UniSource::ActualText,
                        _ => UniSource::Code,
                    };
                    let font = ga[G_F].and_then(|f| f.parse::<i64>().ok()).and_then(|id| font_index.get(&id).copied());
                    let name = ga[G_N].map(|n| match &*last_glyph_name {
                        Some(rc) if **rc == *n => rc.clone(),
                        _ => {
                            let rc: Rc<str> = Rc::from(&*pyc::html_unescape(n));
                            *last_glyph_name = Some(rc.clone());
                            rc
                        }
                    });
                    line_glyphs.push(Glyph {
                        x,
                        w,
                        base: ga[G_B].and_then(py_float),
                        size: ga[G_Z].and_then(py_float).filter(|z| *z > 0.0),
                        font,
                        code: ga[G_C].and_then(|c| c.parse::<u32>().ok()).unwrap_or(u32::MAX),
                        text: Rc::from(glyph_text(ga[G_U].unwrap_or(""))),
                        src,
                        name,
                    });
                    word.glyphs.end += 1;
                    *page_glyphs += 1;
                    *doc_glyphs += 1;
                }
                Tag::Font(attrs) => {
                    let Some(pg) = page.as_mut() else { continue };
                    if pg.fonts.len() >= MAX_PAGE_FONTS {
                        pg.glyphs_truncated = true;
                        report.fonts_skipped += 1;
                        continue;
                    }
                    parse_attrs(attrs, &mut fa, font_key);
                    let Some(id) =
                        fa[F_ID].and_then(|v| v.parse::<i64>().ok()).filter(|id| !font_index.contains_key(id))
                    else {
                        report.fonts_skipped += 1;
                        continue;
                    };
                    font_index.insert(id, pg.fonts.len() as u16);
                    let text = |k: usize| -> Rc<str> { Rc::from(&*pyc::html_unescape(fa[k].unwrap_or(""))) };
                    let flag = |k: usize| fa[k] == Some("1");
                    pg.fonts.push(FontInfo {
                        id,
                        name: text(F_NAME),
                        kind: text(F_TYPE),
                        encoding: text(F_ENC),
                        embedded: flag(F_EMB),
                        symbolic: flag(F_SYM),
                        cid: flag(F_CID),
                        has_to_unicode: flag(F_TU),
                        to_unicode_misses: fa[F_TUMISS].and_then(|v| v.parse().ok()).unwrap_or(0),
                    });
                }

                Tag::Rule(boxed, attrs) => {
                    if !have_page {
                        continue;
                    }
                    a.parse_into(attrs);
                    let (Some(x0), Some(y0), Some(x1), Some(y1)) =
                        (coord(&a, X_MIN), coord(&a, Y_MIN), coord(&a, X_MAX), coord(&a, Y_MAX))
                    else {
                        report.rules_skipped += 1;
                        continue;
                    };
                    let color = a.0[COLOR].unwrap_or("#000000").to_string();
                    if let Some(pg) = page.as_mut() {
                        pg.rules.push(Rule { x0, y0, x1, y1, color, boxed });
                    }
                }
                Tag::Image(attrs) => {
                    if !have_page {
                        continue;
                    }
                    a.parse_into(attrs);
                    let (Some(x0), Some(y0), Some(x1), Some(y1)) =
                        (coord(&a, X_MIN), coord(&a, Y_MIN), coord(&a, X_MAX), coord(&a, Y_MAX))
                    else {
                        report.images_skipped += 1;
                        continue;
                    };
                    let px = |k: usize| a.0[k].and_then(|v| v.parse::<u32>().ok()).unwrap_or(0);
                    let image = Image {
                        x0,
                        y0,
                        x1,
                        y1,
                        px_w: px(WIDTH),
                        px_h: px(HEIGHT),
                        mask: a.0[KIND] == Some("mask"),
                        obj: a.0[REF].and_then(object_ref),
                        copy: match a.0[COPY] {
                            Some("jpeg") => CopyClass::Jpeg,
                            Some("bilevel") => CopyClass::Bilevel,
                            Some("decode") => CopyClass::Decode,
                            _ => CopyClass::Render,
                        },
                        copy_exact: a.0[ORIENT] == Some("0") && a.0[OVERLAID] == Some("0"),
                        plain: a.0[ORIENT] == Some("0")
                            && a.0[OVERLAID] == Some("0")
                            && a.0[SMASK].is_none()
                            && a.0[KIND] != Some("mask"),
                    };
                    if let Some(pg) = page.as_mut() {
                        if pg.images.len() < MAX_PAGE_IMAGES {
                            pg.images.push(image);
                        } else {
                            pg.images_truncated = true;
                            report.images_skipped += 1;
                        }
                    }
                }
                Tag::Graphic(attrs) => {
                    if !have_page {
                        continue;
                    }
                    a.parse_into(attrs);
                    let (Some(x0), Some(y0), Some(x1), Some(y1)) =
                        (coord(&a, X_MIN), coord(&a, Y_MIN), coord(&a, X_MAX), coord(&a, Y_MAX))
                    else {
                        report.graphics_skipped += 1;
                        continue;
                    };
                    if x0 > x1 || y0 > y1 {
                        report.graphics_skipped += 1;
                        continue;
                    }
                    let n = |k: usize| a.0[k].and_then(|v| v.parse::<u64>().ok()).unwrap_or(0);
                    let graphic = Graphic {
                        x0,
                        y0,
                        x1,
                        y1,
                        paths: n(PATHS),
                        segments: n(SEGMENTS),
                        curves: n(CURVES),
                        directions: n(DIRECTIONS).min(8) as u32,
                    };
                    if let Some(pg) = page.as_mut() {
                        if pg.graphics.len() < MAX_PAGE_GRAPHICS {
                            pg.graphics.push(graphic);
                        } else {
                            report.graphics_skipped += 1;
                        }
                    }
                }
                Tag::Open("page", attrs) => {
                    report.pages += 1;
                    a.parse_into(attrs);
                    let dim = |k: usize, d: f64| a.0[k].map_or(Some(d), py_float).unwrap_or(d);
                    let rules_truncated = match a.0[RULES_TRUNCATED] {
                        Some("1") => Some(true),
                        Some("0") => Some(false),
                        _ => None,
                    };
                    if rules_truncated == Some(true) {
                        report.rules_truncated_pages.push(report.pages - 1);
                    }
                    report.page_info.push(PageInfo {
                        crop: a.0[CROP].filter(|v| crop_value(v)).map(Into::into),
                        number: a.0[NUMBER].and_then(|v| v.parse::<u32>().ok()),
                        ..Default::default()
                    });
                    *page_glyphs = 0;
                    line_glyphs.clear();
                    font_index.clear();
                    let next = Page {
                        number: a.0[NUMBER].and_then(|v| v.parse::<u32>().ok()),
                        images: Vec::new(),
                        images_truncated: a.0[IMAGES_TRUNCATED] == Some("1"),
                        scan: scan_of(a.0[SCAN], a.0[SCAN_IMAGE]),
                        graphics: Vec::new(),
                        glyphs: Vec::new(),
                        fonts: Vec::new(),
                        extra_glyphs: Vec::new(),
                        glyphs_truncated: glyphs && a.0[GLYPHS_TRUNCATED] == Some("1"),
                        width: dim(WIDTH, 612.0),
                        height: dim(HEIGHT, 792.0),
                        lines: Vec::new(),
                        rules: Vec::new(),
                        rules_truncated,
                    };
                    if let Some(done) = page.replace(next) {
                        if done.glyphs_truncated {
                            report.glyphs_truncated_pages.push(report.pages - 2);
                        }
                        emit(done);
                    }
                }
                Tag::Open("flow", _) => *flow += 1,
                Tag::Open("block", _) => *block += 1,
                Tag::Open(_, _) => {
                    report.words_unclosed_line += words.len();
                    words.clear();
                    line_glyphs.clear();
                    *in_line = true;
                }
                Tag::Close(name) => {
                    report.page_closes += (name == "page") as usize;
                    if name == "line" && have_page && *in_line && !words.is_empty() {
                        report.lines += 1;
                        report.words += words.len();
                        // exact-size copy out of the reused buffer (mem::take would hand the line
                        // the grown buffer and regrow a new one for every line)
                        #[allow(clippy::drain_collect)]
                        let mut ws: Vec<Word> = words.drain(..).collect();
                        *in_line = false;
                        if let Some(pg) = page.as_mut() {
                            if glyphs {
                                let mut at = pg.glyphs.len() as u32;
                                for w in &mut ws {
                                    let n = w.glyphs.len() as u32;
                                    w.glyphs = GlyphRange { start: at, end: at + n };
                                    at += n;
                                }
                                pg.glyphs.append(line_glyphs);
                            }
                            pg.lines.push(Rc::new(Line::new(ws, *block, *flow)));
                        }
                    }
                }
                Tag::Word(attrs, text) => {
                    if !*in_line {
                        report.words_outside_line += 1;
                        continue;
                    }
                    a.parse_into(attrs);
                    if [X_MIN, Y_MIN, X_MAX, Y_MAX].iter().any(|&k| a.0[k].is_none()) {
                        report.words_missing_coord += 1;
                        continue;
                    }
                    let (Some(x0), Some(y0), Some(x1), Some(y1), Some(size), Some(base)) = (
                        coord(&a, X_MIN),
                        coord(&a, Y_MIN),
                        coord(&a, X_MAX),
                        coord(&a, Y_MAX),
                        opt_num(&a, FONT_SIZE),
                        opt_num(&a, BASELINE),
                    ) else {
                        report.words_bad_number += 1;
                        continue;
                    };
                    let font = a.0[FONT_NAME].unwrap_or("");
                    let (bold, italic) = match last_font {
                        Some((f, style)) if f == font => style,
                        _ => {
                            let style = Word::font_style(font);
                            last_font = Some((font, style));
                            style
                        }
                    };
                    if let Some(info) = report.page_info.last_mut() {
                        info.words = info.words.saturating_add(1);
                        info.invisible = info.invisible.saturating_add((a.0[INVISIBLE] == Some("1")) as u32);
                    }
                    words.push(Word {
                        x0,
                        y0,
                        x1,
                        y1,
                        text: word_text(text),
                        bold,
                        italic,
                        size,
                        base,
                        space: a.0[SPACE_AFTER].unwrap_or("1") != "0",
                        color: interned(last_color, a.0[COLOR].unwrap_or("")),
                        glyphs: GlyphRange::default(),
                        invisible: glyphs && a.0[INVISIBLE] == Some("1"),
                        rot: match a.0[ROTATION] {
                            Some("1") => 1,
                            Some("2") => 2,
                            Some("3") => 3,
                            _ => 0,
                        },
                    });
                    *glyph_target = glyphs;
                }
            }
        }
        s.len()
    }

    /// The end of the input: emits the last page and returns the report.
    fn finish(mut self, emit: &mut dyn FnMut(Page)) -> ParseReport {
        if self.in_line && !self.words.is_empty() {
            self.report.unclosed_line_at_eof = true;
            self.report.words_unclosed_line += self.words.len();
        }
        if let Some(pg) = self.page.take() {
            if pg.glyphs_truncated {
                self.report.glyphs_truncated_pages.push(self.report.pages - 1);
            }
            emit(pg);
        }
        self.report
    }
}

/// Appends `bytes` to `out` as `String::from_utf8_lossy` decodes the whole input (each invalid
/// sequence becomes one U+FFFD; Python read the XHTML with errors='replace'). A sequence cut off
/// at the end of `bytes` waits in `partial` (at most 3 bytes) for the next call, unless `eof`.
pub fn decode_lossy(partial: &mut Vec<u8>, bytes: &[u8], eof: bool, out: &mut String) {
    let joined;
    let mut rest: &[u8] = if partial.is_empty() {
        bytes
    } else {
        partial.extend_from_slice(bytes);
        joined = std::mem::take(partial);
        &joined
    };
    loop {
        match std::str::from_utf8(rest) {
            Ok(s) => {
                out.push_str(s);
                return;
            }
            Err(e) => {
                let (valid, after) = rest.split_at(e.valid_up_to());
                // valid_up_to is a char boundary, so this never falls back
                out.push_str(std::str::from_utf8(valid).unwrap_or_default());
                match e.error_len() {
                    Some(n) => {
                        out.push('\u{FFFD}');
                        rest = &after[n..];
                    }
                    None if eof => {
                        out.push('\u{FFFD}');
                        return;
                    }
                    None => {
                        *partial = after.to_vec();
                        return;
                    }
                }
            }
        }
    }
}

/// [`parse_report`] over input that arrives in pieces: [`Parser::push`] text as it comes, and
/// each page comes out as soon as the next `<page>` opens. Pages and report are the same as one
/// `parse_report` over the whole input, however it is split. Only unconsumed input stays
/// buffered: usually part of a tag.
#[derive(Default)]
pub struct Parser {
    buf: String,
    /// rescan once `buf` reaches this length: after a stop, the pending bytes must double
    /// first, so an unclosed tag that keeps growing is rescanned O(log n) times, not per push
    retry_at: usize,
    state: ParseState,
}

impl Parser {
    /// Incremental parser with optional font and glyph records.
    pub fn with_glyphs(glyphs: bool) -> Self {
        Self { state: ParseState { glyphs, ..Default::default() }, ..Default::default() }
    }

    pub fn new() -> Self {
        Self::default()
    }

    /// Appends `text` and passes each page it completes to `emit`.
    pub fn push(&mut self, text: &str, emit: &mut dyn FnMut(Page)) {
        self.buf.push_str(text);
        if self.buf.len() < self.retry_at {
            return;
        }
        let used = self.state.run(&self.buf, false, emit);
        self.buf.drain(..used);
        self.retry_at = 2 * self.buf.len();
    }

    /// Ends the input: passes the remaining pages to `emit` and returns the report.
    pub fn finish(mut self, emit: &mut dyn FnMut(Page)) -> ParseReport {
        let buf = std::mem::take(&mut self.buf);
        self.state.run(&buf, true, emit);
        self.state.finish(emit)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn document_glyph_budget_exhaustion_mid_word_marks_page_incomplete() {
        let mut parser = Parser::with_glyphs(true);
        parser.state.doc_glyphs = MAX_DOC_GLYPHS - 1;
        let input = r#"<page><line><word xMin="1" yMin="1" xMax="20" yMax="12">a+b</word><g x="1" w="6" u="61"/><g x="7" w="6" u="2b"/><g x="13" w="6" u="62"/></line></page>"#;
        let mut pages = Vec::new();
        for chunk in input.as_bytes().chunks(1) {
            parser.push(std::str::from_utf8(chunk).unwrap(), &mut |p| pages.push(p));
        }
        let report = parser.finish(&mut |p| pages.push(p));
        assert_eq!(report.glyphs_skipped, 2);
        assert_eq!(report.glyphs_truncated_pages, vec![0]);
        assert!(pages[0].glyphs_truncated);
        assert_eq!(pages[0].glyphs.len(), 1);
        let (lines, stats) = crate::math::rewrite_page(&pages[0], &crate::math::MathOptions::default());
        assert_eq!(&*lines[0].words[0].text, "a+b");
        assert!(stats.skipped && stats.uncertain > 0);
    }

    #[test]
    fn font_cap_attributes_incomplete_evidence_to_each_affected_page() {
        let mut input = String::from("<page></page><page>");
        for id in 0..=MAX_PAGE_FONTS {
            input.push_str(&format!("<font id=\"{id}\"/>"));
        }
        input.push_str("</page><page><font id=\"0\"/></page>");
        for chunk_size in [1, 37, input.len()] {
            let mut parser = Parser::with_glyphs(true);
            let mut pages = Vec::new();
            for chunk in input.as_bytes().chunks(chunk_size) {
                parser.push(std::str::from_utf8(chunk).unwrap(), &mut |p| pages.push(p));
            }
            let report = parser.finish(&mut |p| pages.push(p));
            assert_eq!(report.fonts_skipped, 1);
            assert_eq!(report.glyphs_truncated_pages, vec![1]);
            assert_eq!(pages.iter().map(|p| p.glyphs_truncated).collect::<Vec<_>>(), vec![false, true, false]);
            assert_eq!(pages[1].fonts.len(), MAX_PAGE_FONTS);
            let (_, stats) = crate::math::rewrite_page(&pages[1], &crate::math::MathOptions::default());
            assert!(stats.skipped && stats.uncertain > 0);
        }
    }

    #[test]
    fn key_index_matches_keys() {
        for (i, k) in KEYS.iter().enumerate() {
            assert_eq!(key_index(k), Some(i));
        }
        assert_eq!(key_index("xmin"), None);
        let a = Attrs::parse(r#"x xMin="1" é="2" color="a" color="b" fontName="F"x="3" bad= "4" yMin="5"#);
        assert_eq!(
            (a.get("xMin"), a.get("color"), a.get("fontName"), a.get("yMin")),
            (Some("1"), Some("b"), Some("F"), None)
        );
        // an unterminated value ends the scan (linear even for a huge tag of `a="` repeats)
        let open = "a=\"".repeat(200_000);
        assert!(Attrs::parse(&format!("color=\"x\" {open}")).get("color") == Some("x"));
    }
}
