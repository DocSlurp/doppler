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

/// The attribute names the converter reads.
const KEYS: [&str; 11] =
    ["xMin", "yMin", "xMax", "yMax", "fontName", "fontSize", "spaceAfter", "color", "baseline", "width", "height"];

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
        let out = &mut self.0;
        *out = [None; KEYS.len()];
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
                if let Some(k) = key_index(&s[i..j]) {
                    out[k] = Some(&s[j + 2..j + 2 + q]);
                }
                i = j + 2 + q + 1;
                continue;
            }
            i = j;
        }
    }

    /// The value of `k`, which must be one of [`KEYS`] (any other name reads as absent).
    pub fn get(&self, k: &str) -> Option<&'a str> {
        self.0[key_index(k)?]
    }
}

enum Tag<'a> {
    Open(&'a str, &'a str),
    Close(&'a str),
    Word(&'a str, &'a str),
    Rule(bool, &'a str),
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
fn tag_at<'s>(s: &'s str, lt: usize, scan: &mut Scan) -> Option<(Tag<'s>, usize)> {
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

fn word_text(raw: &str) -> Rc<str> {
    let t = pyc::html_unescape(raw);
    // is_control on the bytes: C0 controls other than tab and newline, DEL, U+FFFD
    let control = t.bytes().any(|c| (c < 0x20 && c != b'\t' && c != b'\n') || c == 0x7f) || t.contains('\u{fffd}');
    if control { Rc::from(t.chars().filter(|&c| !is_control(c)).collect::<String>()) } else { Rc::from(&*t) }
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

/// Parse doppler XHTML into pages. Where Python would raise (a missing or malformed required
/// number), the offending word or rule is skipped instead; non-finite numbers count as malformed.
pub fn parse(xhtml: &str) -> Vec<Page> {
    let mut pages: Vec<Page> = Vec::new();
    // words of the open <line> (`words` in Python is None until the first <line>)
    let mut words: Vec<Word> = Vec::new();
    let mut in_line = false;
    let (mut flow, mut block) = (-1i64, -1i64);
    let b = xhtml.as_bytes();
    let mut a = Attrs([None; KEYS.len()]);
    let mut scan = Scan::new();
    let mut last_color: Option<Rc<str>> = None;
    // font names repeat word after word: cache the last style
    let mut last_font: Option<(&str, (bool, bool))> = None;
    let mut pos = 0;
    while let Some(off) = memchr(b'<', &b[pos..]) {
        let lt = pos + off;
        let Some((tag, end)) = tag_at(xhtml, lt, &mut scan) else {
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
                a.parse_into(attrs);
                let (Some(x0), Some(y0), Some(x1), Some(y1)) =
                    (coord(&a, X_MIN), coord(&a, Y_MIN), coord(&a, X_MAX), coord(&a, Y_MAX))
                else {
                    continue;
                };
                let color = a.0[COLOR].unwrap_or("#000000").to_string();
                if let Some(pg) = pages.last_mut() {
                    pg.rules.push(Rule { x0, y0, x1, y1, color, boxed });
                }
            }
            Tag::Open("page", attrs) => {
                a.parse_into(attrs);
                let dim = |k: usize, d: f64| a.0[k].map_or(Some(d), py_float).unwrap_or(d);
                pages.push(Page {
                    width: dim(WIDTH, 612.0),
                    height: dim(HEIGHT, 792.0),
                    lines: Vec::new(),
                    rules: Vec::new(),
                });
            }
            Tag::Open("flow", _) => flow += 1,
            Tag::Open("block", _) => block += 1,
            Tag::Open(_, _) => {
                words.clear();
                in_line = true;
            }
            Tag::Close(name) => {
                if name == "line" && have_page && in_line && !words.is_empty() {
                    // exact-size copy out of the reused buffer (mem::take would hand the line
                    // the grown buffer and regrow a new one for every line)
                    #[allow(clippy::drain_collect)]
                    let ws: Vec<Word> = words.drain(..).collect();
                    in_line = false;
                    if let Some(pg) = pages.last_mut() {
                        pg.lines.push(Rc::new(Line::new(ws, block, flow)));
                    }
                }
            }
            Tag::Word(attrs, text) => {
                if !in_line {
                    continue;
                }
                a.parse_into(attrs);
                let (Some(x0), Some(y0), Some(x1), Some(y1), Some(size), Some(base)) = (
                    coord(&a, X_MIN),
                    coord(&a, Y_MIN),
                    coord(&a, X_MAX),
                    coord(&a, Y_MAX),
                    opt_num(&a, FONT_SIZE),
                    opt_num(&a, BASELINE),
                ) else {
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
                    color: interned(&mut last_color, a.0[COLOR].unwrap_or("")),
                });
            }
        }
    }
    pages
}

#[cfg(test)]
mod tests {
    use super::*;

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
