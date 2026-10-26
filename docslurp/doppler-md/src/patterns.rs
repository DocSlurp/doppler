//! Hand-written matchers for doppler_md.py's regular expressions (`NUMBERED_RE`,
//! `NUMERIC_RE`, `LEADING_PUNCT_RE`, `DATA_CELL_RE`), with Python's Unicode `\d` and `\s`.
//!
//! These used to be `regex` patterns with Python's classes interpolated as explicit ranges.
//! Compiling them took about a sixth of the CPU of a typical one-file process, so they are
//! written out here instead; each is linear in the input. `tests/patterns.rs` checks them
//! against the original patterns (the `regex` crate is a dev-dependency only).

use crate::pyc::{is_decimal as d, is_space as s};

/// `re.match(pattern, s)` where every alternative ends with `\z`: Python's `$` also matches
/// just before one trailing newline.
fn py_match(t: &str, full: fn(&[char]) -> bool) -> bool {
    let c: Vec<char> = t.chars().collect();
    full(&c) || (c.last() == Some(&'\n') && full(&c[..c.len() - 1]))
}

/// `[d]{1,3}` groups joined by '.', with a group count in `min..=max`.
fn dotted(c: &[char], min: usize, max: usize) -> bool {
    let mut groups = 0;
    for g in c.split(|&x| x == '.') {
        groups += 1;
        if g.is_empty() || g.len() > 3 || !g.iter().all(|&x| d(x)) {
            return false;
        }
    }
    (min..=max).contains(&groups)
}

/// `\A(?:\(?[d]{1,3}(?:\.[d]{1,3}){0,3}[.)]|[d]{1,3}(?:\.[d]{1,3}){1,3}|\(?[a-z]\)|\([A-Za-z]\)|\(?[ivx]{1,4}\)|[IVX]{1,4}\.|§[s]?[d]+)\z`
fn numbered_full(c: &[char]) -> bool {
    let open = c.first() == Some(&'(');
    let after_open = if open { &c[1..] } else { c };
    // \(?[d]{1,3}(?:\.[d]{1,3}){0,3}[.)]
    if let Some((&last, body)) = after_open.split_last()
        && matches!(last, '.' | ')')
        && dotted(body, 1, 4)
    {
        return true;
    }
    // [d]{1,3}(?:\.[d]{1,3}){1,3}
    if dotted(c, 2, 4) {
        return true;
    }
    // \(?[a-z]\) and \([A-Za-z]\) and \(?[ivx]{1,4}\)
    if let Some((&')', body)) = after_open.split_last() {
        if let [x] = body
            && (x.is_ascii_lowercase() || (open && x.is_ascii_uppercase()))
        {
            return true;
        }
        if (1..=4).contains(&body.len()) && body.iter().all(|x| matches!(x, 'i' | 'v' | 'x')) {
            return true;
        }
    }
    // [IVX]{1,4}\.
    if let Some((&'.', body)) = c.split_last()
        && (1..=4).contains(&body.len())
        && body.iter().all(|x| matches!(x, 'I' | 'V' | 'X'))
    {
        return true;
    }
    // §[s]?[d]+
    if let Some((&'§', rest)) = c.split_first() {
        let rest = match rest.first() {
            Some(&x) if s(x) => &rest[1..],
            _ => rest,
        };
        return !rest.is_empty() && rest.iter().all(|&x| d(x));
    }
    false
}

/// `NUMBERED_RE.match(t)`
pub fn numbered(t: &str) -> bool {
    py_match(t, numbered_full)
}

/// Positions reachable after `X*` from the positions in `reach`.
fn star(c: &[char], reach: &mut [bool], class: impl Fn(char) -> bool) {
    for i in 0..c.len() {
        if reach[i] && class(c[i]) {
            reach[i + 1] = true;
        }
    }
}

/// The first alternative of `NUMERIC_RE` (without `\A`/`\z`), as a set-of-positions
/// simulation: `reach[i]` means the pattern so far can end at code point `i`.
///
/// `[s(+\-−–]*[$€£¥₩₹]?[s]*[d][d s.,'’  ]*(?:[.,][d]+)?[s]*(?:%|€|\$|[A-Z]{3}|k|m|bn|x)?[)s]*`
fn amount_full(c: &[char]) -> bool {
    let n = c.len();
    let mut reach = vec![false; n + 1];
    reach[0] = true;
    star(c, &mut reach, |x| s(x) || matches!(x, '(' | '+' | '-' | '\u{2212}' | '\u{2013}'));
    // optional currency sign
    for i in (0..n).rev() {
        if reach[i] && matches!(c[i], '$' | '\u{20ac}' | '\u{00a3}' | '\u{00a5}' | '\u{20a9}' | '\u{20b9}') {
            reach[i + 1] = true;
        }
    }
    star(c, &mut reach, s);
    // exactly one digit
    let mut next = vec![false; n + 1];
    for i in 0..n {
        next[i + 1] = reach[i] && d(c[i]);
    }
    let mut reach = next;
    star(c, &mut reach, |x| d(x) || s(x) || matches!(x, '.' | ',' | '\'' | '\u{2019}' | '\u{202f}' | '\u{00a0}'));
    // optional decimal part [.,][d]+ (runs start right after a separator, so they are disjoint)
    let mut next = reach.clone();
    for i in 0..n {
        if reach[i] && matches!(c[i], '.' | ',') {
            let mut k = i + 1;
            while k < n && d(c[k]) {
                k += 1;
                next[k] = true;
            }
        }
    }
    let mut reach = next;
    star(c, &mut reach, s);
    // optional unit
    let mut next = reach.clone();
    for i in 0..n {
        if !reach[i] {
            continue;
        }
        if matches!(c[i], '%' | '\u{20ac}' | '$' | 'k' | 'm' | 'x') {
            next[i + 1] = true;
        }
        if i + 3 <= n && c[i..i + 3].iter().all(char::is_ascii_uppercase) {
            next[i + 3] = true;
        }
        if c[i..].starts_with(&['b', 'n']) {
            next[i + 2] = true;
        }
    }
    let mut reach = next;
    star(c, &mut reach, |x| x == ')' || s(x));
    reach[n]
}

/// `[d]{1,4}[/.\-][d]{1,2}(?:[/.\-][d]{1,4})?` over the whole input.
fn date_full(c: &[char]) -> bool {
    let sep = |x: char| matches!(x, '/' | '.' | '-');
    let parts: Vec<&[char]> = c.split(|&x| sep(x)).collect();
    let ok = |p: &[char], max: usize| !p.is_empty() && p.len() <= max && p.iter().all(|&x| d(x));
    match parts.as_slice() {
        [a, b] => ok(a, 4) && ok(b, 2),
        [a, b, e] => ok(a, 4) && ok(b, 2) && ok(e, 4),
        _ => false,
    }
}

fn numeric_full(c: &[char]) -> bool {
    matches!(c, ['\u{2013}' | '\u{2014}' | '-']) || date_full(c) || amount_full(c)
}

/// `NUMERIC_RE.match(t)` (the caller strips).
pub fn numeric_match(t: &str) -> bool {
    py_match(t, numeric_full)
}

/// `LEADING_PUNCT_RE.fullmatch`-style captures of `\A([,.;:!?)\]]+)([d].*)\z` (`.` excludes
/// '\n'): the punctuation run and the rest.
pub fn leading_punct(t: &str) -> Option<(&str, &str)> {
    let p = t.find(|x: char| !matches!(x, ',' | '.' | ';' | ':' | '!' | '?' | ')' | ']'))?;
    let rest = &t[p..];
    (p > 0 && rest.chars().next().is_some_and(d) && !rest.contains('\n')).then(|| (&t[..p], rest))
}

/// `$` at code point `i`: the end, or just before one trailing newline.
fn at_end(c: &[char], i: usize) -> bool {
    i == c.len() || (i + 1 == c.len() && c[i] == '\n')
}

fn is_sign(x: char) -> bool {
    matches!(x, '(' | '+' | '-' | '\u{2212}' | '\u{2013}')
}

/// `^[s(+\-−–]*[$€£¥₩₹฿][s]*[d]`. Neither run can consume what follows it, so greedy runs
/// never backtrack.
fn data_currency(c: &[char]) -> bool {
    let mut i = c.iter().take_while(|&&x| s(x) || is_sign(x)).count();
    if !c.get(i).is_some_and(|&x| {
        matches!(x, '$' | '\u{20ac}' | '\u{00a3}' | '\u{00a5}' | '\u{20a9}' | '\u{20b9}' | '\u{0e3f}')
    }) {
        return false;
    }
    i += 1;
    i += c[i..].iter().take_while(|&&x| s(x)).count();
    c.get(i).is_some_and(|&x| d(x))
}

/// `^[(+\-−–]?[d][d s.,'’  ]*[.,][d]+[s]*(?:%|€|\$|[A-Z]{3})?\)?$`. The decimal separator
/// can be any '.' or ',' inside the leading run, so each is tried; the digits after it are
/// taken whole (the tail cannot start with a digit), and every later token is forced. The
/// digit and space runs tried for different separators are disjoint, so this is linear.
fn data_decimal(c: &[char]) -> bool {
    let n = c.len();
    let mut i = usize::from(c.first().is_some_and(|&x| is_sign(x)));
    if !c.get(i).is_some_and(|&x| d(x)) {
        return false;
    }
    i += 1;
    let in_run = |x: char| d(x) || s(x) || matches!(x, '.' | ',' | '\'' | '\u{2019}' | '\u{202f}' | '\u{00a0}');
    let mut j = i;
    while j < n && in_run(c[j]) {
        if matches!(c[j], '.' | ',') {
            let mut k = j + 1;
            while k < n && d(c[k]) {
                k += 1;
            }
            if k > j + 1 {
                k += c[k..].iter().take_while(|&&x| s(x)).count();
                if k < n && matches!(c[k], '%' | '\u{20ac}' | '$') {
                    k += 1;
                } else if k + 3 <= n && c[k..k + 3].iter().all(char::is_ascii_uppercase) {
                    k += 3;
                }
                if k < n && c[k] == ')' {
                    k += 1;
                }
                if at_end(c, k) {
                    return true;
                }
            }
        }
        j += 1;
    }
    false
}

/// `[d][s]*%$` anywhere: a '%' at the end, after a run of spaces, after a digit.
fn data_percent(c: &[char]) -> bool {
    let n = c.len();
    let p = if n >= 1 && c[n - 1] == '%' {
        n - 1
    } else if n >= 2 && c[n - 1] == '\n' && c[n - 2] == '%' {
        n - 2
    } else {
        return false;
    };
    let spaces = c[..p].iter().rev().take_while(|&&x| s(x)).count();
    p > spaces && d(c[p - spaces - 1])
}

/// `^[d]{1,2}[/.\-][d]{1,2}[/.\-][d]{2,4}$`
fn data_date(c: &[char]) -> bool {
    let run = |i: usize| c[i.min(c.len())..].iter().take_while(|&&x| d(x)).count();
    let sep = |i: usize| c.get(i).is_some_and(|x| matches!(x, '/' | '.' | '-'));
    let a = run(0);
    if !(1..=2).contains(&a) || !sep(a) {
        return false;
    }
    let b = run(a + 1);
    let i = a + 1 + b;
    if !(1..=2).contains(&b) || !sep(i) {
        return false;
    }
    let e = run(i + 1);
    (2..=4).contains(&e) && at_end(c, i + 1 + e)
}

/// `DATA_CELL_RE.search(t)`: a cell that reads as data (an amount, a decimal, a signed number,
/// a percentage or a date) rather than a column label.
pub fn data_cell(t: &str) -> bool {
    let c: Vec<char> = t.chars().collect();
    data_currency(&c)
        || data_decimal(&c)
        || (c.first().is_some_and(|&x| matches!(x, '+' | '-' | '\u{2212}')) && c.get(1).is_some_and(|&x| d(x)))
        || data_percent(&c)
        || data_date(&c)
}
