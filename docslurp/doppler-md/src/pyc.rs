//! Python semantics the converter depends on: `str.isspace`/`strip`/`split`, the `re` module's
//! Unicode `\w`/`\s` classes and `$` (which also matches before a trailing newline),
//! `statistics.median`, `round()`, `float()`, `Counter.most_common` tie order and `html.unescape`.

use std::borrow::Cow;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::hash::Hash;

use crate::entities::{HTML5, INVALID_CHARREFS, INVALID_CODEPOINTS};
use crate::unicode_tables as tables;

fn in_table(table: &[(u32, u32)], c: char) -> bool {
    let c = c as u32;
    table
        .binary_search_by(|&(lo, hi)| {
            if hi < c {
                Ordering::Less
            } else if lo > c {
                Ordering::Greater
            } else {
                Ordering::Equal
            }
        })
        .is_ok()
}

/// `str.isspace()` (and `re` `\s`).
pub fn is_space(c: char) -> bool {
    if c.is_ascii() {
        return matches!(c, '\t' | '\n' | '\x0b' | '\x0c' | '\r' | '\x1c'..='\x1f' | ' ');
    }
    in_table(tables::SPACE, c)
}

/// `re` `\w` for str patterns: `str.isalnum()` or underscore.
pub fn is_word_char(c: char) -> bool {
    if c.is_ascii() {
        return c.is_ascii_alphanumeric() || c == '_';
    }
    in_table(tables::WORD, c)
}

/// `str.isdecimal()` (and `re` `\d`).
pub fn is_decimal(c: char) -> bool {
    if c.is_ascii() {
        return c.is_ascii_digit();
    }
    in_table(tables::DECIMAL, c)
}

/// `str.islower()` of a one-character string.
pub fn is_lower(c: char) -> bool {
    if c.is_ascii() {
        return c.is_ascii_lowercase();
    }
    in_table(tables::LOWER, c)
}

pub fn strip(s: &str) -> &str {
    s.trim_matches(is_space)
}

pub fn rstrip(s: &str) -> &str {
    s.trim_end_matches(is_space)
}

/// `len(s.split())`
pub fn split_count(s: &str) -> usize {
    s.split(is_space).filter(|t| !t.is_empty()).count()
}

/// `len(s)` in code points.
pub fn char_len(s: &str) -> usize {
    s.chars().count()
}

/// Total order on floats that treats -0.0 and 0.0 as equal, like Python's `<` on non-NaN floats.
/// NaN never reaches the converter (non-finite numbers are rejected at parse time), but a total
/// order keeps `sort_by` from panicking on any input.
pub fn fcmp(a: f64, b: f64) -> Ordering {
    (a + 0.0).total_cmp(&(b + 0.0))
}

/// Python `max(a, b)`: the first argument unless the second is strictly greater.
pub fn pmax(a: f64, b: f64) -> f64 {
    if b > a { b } else { a }
}

/// Python `min(a, b)`: the first argument unless the second is strictly smaller.
pub fn pmin(a: f64, b: f64) -> f64 {
    if b < a { b } else { a }
}

/// `statistics.median`: the middle value, or the mean of the two middle values. Python raises on
/// an empty sequence; NaN makes every comparison false, the closest non-raising equivalent.
pub fn median(mut v: Vec<f64>) -> f64 {
    let n = v.len();
    if n == 0 {
        return f64::NAN;
    }
    v.sort_by(|a, b| fcmp(*a, *b));
    if n % 2 == 1 { v[n / 2] } else { (v[n / 2 - 1] + v[n / 2]) / 2.0 }
}

/// `round(x)` (to an integer, half to even).
pub fn round0(x: f64) -> f64 {
    x.round_ties_even()
}

/// `round(x, 1)`: Python rounds the exact binary value to one decimal, half to even; Rust's
/// exact fixed-precision formatting does the same, and parsing it back is correctly rounded.
pub fn round1(x: f64) -> f64 {
    if !x.is_finite() {
        return x;
    }
    format!("{x:.1}").parse().unwrap_or(x)
}

/// Powers of ten exactly representable as f64.
const POW10: [f64; 23] = [
    1e0, 1e1, 1e2, 1e3, 1e4, 1e5, 1e6, 1e7, 1e8, 1e9, 1e10, 1e11, 1e12, 1e13, 1e14, 1e15, 1e16, 1e17, 1e18, 1e19,
    1e20, 1e21, 1e22,
];

/// Fast path of [`py_float`] for `-?\d+(\.\d*)?` with at most 15 significant digits (every
/// coordinate pdftotext writes): the mantissa and the power of ten are both exact, so one IEEE
/// division is correctly rounded and equals what `str::parse` returns (Clinger's fast path, the
/// one `dec2flt` takes first). Anything else returns None and goes through the general parser.
fn plain_decimal(b: &[u8]) -> Option<f64> {
    let (neg, digits) = match b.first() {
        Some(b'-') => (true, &b[1..]),
        _ => (false, b),
    };
    let mut m: u64 = 0;
    let mut n = 0usize; // digits accumulated
    let mut frac = 0usize; // digits after the point
    let mut point = false;
    let mut any = false;
    for &c in digits {
        match c {
            b'0'..=b'9' => {
                any = true;
                if m == 0 && c == b'0' {
                    // leading zeros add no significant digits
                } else {
                    n += 1;
                    if n > 15 {
                        return None;
                    }
                }
                m = m * 10 + (c - b'0') as u64;
                if point {
                    frac += 1;
                }
            }
            b'.' if !point => point = true,
            _ => return None,
        }
    }
    if !any || frac >= POW10.len() || (point && digits.first() == Some(&b'.')) {
        return None;
    }
    let v = m as f64 / POW10[frac];
    Some(if neg { -v } else { v })
}

/// `float(s)` for finite values. Python strips whitespace and allows `_` between digits; it also
/// accepts inf/nan, which this port rejects (None) because no coordinate or size can be infinite.
pub fn py_float(s: &str) -> Option<f64> {
    if let Some(v) = plain_decimal(s.as_bytes()) {
        return Some(v);
    }
    let t = strip(s);
    let v: f64 = if t.contains('_') {
        let b: Vec<char> = t.chars().collect();
        let mut out = String::with_capacity(t.len());
        for (i, &c) in b.iter().enumerate() {
            if c == '_' {
                let ok = i > 0 && i + 1 < b.len() && b[i - 1].is_ascii_digit() && b[i + 1].is_ascii_digit();
                if !ok {
                    return None;
                }
            } else {
                out.push(c);
            }
        }
        out.parse().ok()?
    } else {
        t.parse().ok()?
    };
    // Rust accepts "inf"/"nan" spellings that Python also accepts; both are rejected here.
    if v.is_finite() { Some(v) } else { None }
}

/// `int(s, 16)` for a slice of at most two code points (the colour channels of `#rrggbb`).
pub fn py_hex(s: &str) -> Option<i64> {
    let t = strip(s);
    let (neg, digits) = match t.chars().next() {
        Some('+') => (false, &t[1..]),
        Some('-') => (true, &t[1..]),
        _ => (false, t),
    };
    if digits.is_empty() || !digits.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let v = i64::from_str_radix(digits, 16).ok()?;
    Some(if neg { -v } else { v })
}

/// Code-point slice `s[a:b]` with Python's clamping for non-negative bounds.
pub fn cp_slice(s: &str, a: usize, b: usize) -> &str {
    let mut it = s.char_indices().map(|(i, _)| i).chain(std::iter::once(s.len()));
    let n = char_len(s);
    let a = a.min(n);
    let b = b.min(n).max(a);
    let start = it.nth(a).unwrap_or(s.len());
    let end = if b == a { start } else { it.nth(b - a - 1).unwrap_or(s.len()) };
    &s[start..end]
}

/// `collections.Counter` with `most_common(1)` semantics: the largest count, earliest insertion
/// on ties. Adding zero still inserts the key, as `c[k] += 0` does.
pub struct Counter<K> {
    index: HashMap<K, usize>,
    items: Vec<(K, i64)>,
}

impl<K: Hash + Eq + Clone> Counter<K> {
    pub fn new() -> Self {
        Counter { index: HashMap::new(), items: Vec::new() }
    }

    pub fn add(&mut self, k: K, n: i64) {
        match self.index.get(&k) {
            Some(&i) => self.items[i].1 += n,
            None => {
                self.index.insert(k.clone(), self.items.len());
                self.items.push((k, n));
            }
        }
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn most_common(&self) -> Option<&K> {
        let mut best: Option<&(K, i64)> = None;
        for it in &self.items {
            if best.is_none_or(|b| it.1 > b.1) {
                best = Some(it);
            }
        }
        best.map(|b| &b.0)
    }

    pub fn counts(&self) -> impl Iterator<Item = i64> + '_ {
        self.items.iter().map(|x| x.1)
    }
}

impl<K: Hash + Eq + Clone> Default for Counter<K> {
    fn default() -> Self {
        Self::new()
    }
}

/// Float-keyed counter (keys compare like Python floats: -0.0 == 0.0; the first key inserted is
/// the one reported).
pub struct FloatCounter {
    inner: Counter<u64>,
    first: HashMap<u64, f64>,
}

impl FloatCounter {
    pub fn new() -> Self {
        FloatCounter { inner: Counter::new(), first: HashMap::new() }
    }

    pub fn add(&mut self, k: f64, n: i64) {
        let bits = (k + 0.0).to_bits();
        self.first.entry(bits).or_insert(k);
        self.inner.add(bits, n);
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    pub fn most_common(&self) -> Option<f64> {
        self.inner.most_common().map(|b| self.first[b])
    }
}

impl Default for FloatCounter {
    fn default() -> Self {
        Self::new()
    }
}

fn html5(name: &str) -> Option<&'static str> {
    HTML5.binary_search_by(|(k, _)| k.cmp(&name)).ok().map(|i| HTML5[i].1)
}

fn charref(s: &str) -> String {
    if let Some(num) = s.strip_prefix('#') {
        let (digits, radix) = match num.strip_prefix(['x', 'X']) {
            Some(h) => (h, 16u32),
            None => (num, 10u32),
        };
        let digits = digits.trim_end_matches(';');
        // Python parses an arbitrarily long number; anything past U+10FFFF is out of range.
        let mut v: u64 = 0;
        for c in digits.chars() {
            v = v * radix as u64 + c.to_digit(radix).unwrap_or(0) as u64;
            if v > 0x10FFFF {
                v = 0x110000;
            }
        }
        let num = v as u32;
        if let Ok(i) = INVALID_CHARREFS.binary_search_by(|(k, _)| k.cmp(&num)) {
            return INVALID_CHARREFS[i].1.to_string();
        }
        if (0xD800..=0xDFFF).contains(&num) || num > 0x10FFFF {
            return "\u{fffd}".to_string();
        }
        if INVALID_CODEPOINTS.binary_search(&num).is_ok() {
            return String::new();
        }
        return char::from_u32(num).map(String::from).unwrap_or_else(|| "\u{fffd}".into());
    }
    if let Some(v) = html5(s) {
        return v.to_string();
    }
    let bounds: Vec<usize> = s.char_indices().map(|(i, _)| i).collect();
    let n = bounds.len();
    // longest prefix s[:x] for x in range(len(s) - 1, 1, -1)
    let mut x = n.saturating_sub(1);
    while x > 1 {
        let cut = bounds[x];
        if let Some(v) = html5(&s[..cut]) {
            return format!("{v}{}", &s[cut..]);
        }
        x -= 1;
    }
    format!("&{s}")
}

/// The match of `&(#[0-9]+;?|#[xX][0-9a-fA-F]+;?|[^\t\n\f <&#;]{1,32};?)` at `s[0] == '&'`:
/// the length of the group (after the '&'), if any.
fn charref_len(s: &str) -> Option<usize> {
    let b = s.as_bytes();
    let semi = |i: usize| if b.get(i) == Some(&b';') { i + 1 } else { i };
    if b.get(1) == Some(&b'#') {
        let dec = b[2..].iter().take_while(|c| c.is_ascii_digit()).count();
        if dec > 0 {
            return Some(semi(2 + dec) - 1);
        }
        if matches!(b.get(2), Some(b'x' | b'X')) {
            let hex = b[3..].iter().take_while(|c| c.is_ascii_hexdigit()).count();
            if hex > 0 {
                return Some(semi(3 + hex) - 1);
            }
        }
        return None; // '#' is excluded from the third alternative
    }
    let mut end = 1;
    for (n, c) in s[1..].chars().enumerate() {
        if n == 32 || matches!(c, '\t' | '\n' | '\x0c' | ' ' | '<' | '&' | '#' | ';') {
            break;
        }
        end += c.len_utf8();
    }
    if end == 1 { None } else { Some(semi(end) - 1) }
}

/// `html.unescape`.
pub fn html_unescape(s: &str) -> Cow<'_, str> {
    if !s.contains('&') {
        return Cow::Borrowed(s);
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        match charref_len(rest) {
            Some(n) => {
                out.push_str(&charref(&rest[1..1 + n]));
                rest = &rest[1 + n..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    Cow::Owned(out)
}
