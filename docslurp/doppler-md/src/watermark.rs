//! Watermarks: large pale or diagonal text and pictures that sit behind a page's content.
//!
//! A watermark is not content. Left in, a pale "CONFIDENTIAL" laid at 45 degrees becomes a
//! heading of one letter per fragment, a table of letters, or a word glued to the first
//! paragraph. Evidence, any of:
//!
//! - **pale**: a visible word at least `watermark_size` times the page's body size, in a grey
//!   light enough to be a tint (`watermark_min_lum` up to, but not including, white) and not on
//!   a dark box. White text is left alone: it is how titles are set on dark banners and photos.
//! - **diagonal**: three or more words of one font and size, at least `watermark_size` times
//!   the body size, on a line that is neither horizontal nor vertical. Poppler reads each word
//!   of such a run as a rotated fragment.
//! - **repeated**: the same large word at the same place on at least as many pages as a running
//!   header must repeat on (`furniture_min_share`, at least 2). Pictures likewise: the same
//!   picture at the same place on that many pages.

use std::collections::HashSet;
use std::rc::Rc;

use crate::model::{Image, Line, Page, Word};
use crate::params::Params;
use crate::pyc::{fcmp, pmax, strip};
use crate::tables::luminance;

fn centre(w: &Word) -> (f64, f64) {
    ((w.x0 + w.x1) / 2.0, (w.y0 + w.y1) / 2.0)
}

/// A dark filled box under the word's centre: light text on a dark banner is content.
fn on_dark_box(page: &Page, w: &Word) -> bool {
    let (cx, cy) = centre(w);
    page.rules
        .iter()
        .any(|r| r.boxed && r.x0 <= cx && cx <= r.x1 && r.y0 <= cy && cy <= r.y1 && luminance(&r.color) < 0.5)
}

/// Whether the word is large for this page and drawn visibly.
fn large(w: &Word, body: f64, p: &Params) -> bool {
    !w.invisible && w.size >= p.watermark_size * pmax(body, 1.0)
}

/// Words (line index, word index) in diagonal runs: three or more large words of one font size
/// whose centres, in order of x, lie along a line that is at least 15 degrees from both axes.
fn diagonal(page: &Page, body: f64, p: &Params) -> HashSet<(usize, usize)> {
    let mut cand: Vec<(usize, usize, f64, f64, f64)> = Vec::new();
    for (li, line) in page.lines.iter().enumerate() {
        for (wi, w) in line.words.iter().enumerate() {
            if w.rot != 0 && large(w, body, p) {
                let (cx, cy) = centre(w);
                cand.push((li, wi, cx, cy, w.size));
            }
        }
    }
    let mut out = HashSet::new();
    if cand.len() < 3 {
        return out;
    }
    cand.sort_by(|a, b| fcmp(a.2, b.2).then(fcmp(a.3, b.3)));
    let slanted = |a: &(usize, usize, f64, f64, f64), b: &(usize, usize, f64, f64, f64)| -> Option<f64> {
        if (a.4 - b.4).abs() > 0.05 * pmax(a.4, 1.0) {
            return None;
        }
        let (dx, dy) = (b.2 - a.2, b.3 - a.3);
        let deg = dy.atan2(dx).to_degrees().abs();
        let off_axis = deg.min((deg - 90.0).abs()).min((deg - 180.0).abs());
        (off_axis >= 15.0 && dx.hypot(dy) > 0.0).then_some(dy.atan2(dx).to_degrees())
    };
    let mut run: Vec<usize> = Vec::new();
    let mut angle = 0.0;
    let flush = |run: &mut Vec<usize>, out: &mut HashSet<(usize, usize)>| {
        if run.len() >= 3 {
            out.extend(run.iter().map(|&i| (cand[i].0, cand[i].1)));
        }
        run.clear();
    };
    for i in 1..cand.len() {
        match slanted(&cand[i - 1], &cand[i]) {
            Some(a) if run.is_empty() || (a - angle).abs() < 12.0 => {
                if run.is_empty() {
                    run.push(i - 1);
                    angle = a;
                }
                run.push(i);
            }
            Some(a) => {
                flush(&mut run, &mut out);
                run.extend([i - 1, i]);
                angle = a;
            }
            None => flush(&mut run, &mut out),
        }
    }
    flush(&mut run, &mut out);
    out
}

/// Take the flagged words out of the page's lines (a line left with none goes). Returns how
/// many words went.
fn remove(page: &mut Page, mut flagged: impl FnMut(usize, usize, &Word) -> bool) -> usize {
    let mut removed = 0;
    let mut lines: Vec<Rc<Line>> = Vec::with_capacity(page.lines.len());
    for (li, line) in page.lines.iter().enumerate() {
        let keep: Vec<bool> = line.words.iter().enumerate().map(|(wi, w)| !flagged(li, wi, w)).collect();
        if keep.iter().all(|k| *k) {
            lines.push(line.clone());
            continue;
        }
        removed += keep.iter().filter(|k| !**k).count();
        let words: Vec<Word> = line.words.iter().zip(&keep).filter(|(_, k)| **k).map(|(w, _)| w.clone()).collect();
        if !words.is_empty() {
            lines.push(Rc::new(Line::new(words, line.block, line.flow)));
        }
    }
    page.lines = lines;
    removed
}

/// Remove the watermark words that this page shows by itself (pale or diagonal). Returns how
/// many words went.
pub(crate) fn strip_local(page: &mut Page, p: &Params) -> usize {
    let body = crate::body_size(std::iter::once(&*page)).0;
    let diag = diagonal(page, body, p);
    let pale: HashSet<(usize, usize)> = page
        .lines
        .iter()
        .enumerate()
        .flat_map(|(li, l)| l.words.iter().enumerate().map(move |(wi, w)| (li, wi, w)))
        .filter(|(_, _, w)| {
            let l = luminance(&w.color);
            large(w, body, p) && l >= p.watermark_min_lum && l < 0.97 && !on_dark_box(page, w)
        })
        .map(|(li, wi, _)| (li, wi))
        .collect();
    if diag.is_empty() && pale.is_empty() {
        return 0;
    }
    remove(page, |li, wi, _| diag.contains(&(li, wi)) || pale.contains(&(li, wi)))
}

/// Where a large word is, for finding it on other pages: its text and position on a 4 point grid.
fn word_key(w: &Word) -> String {
    format!("{}|{}|{}|{}", strip(&w.text).to_lowercase(), (w.x0 / 4.0).round(), (w.y0 / 4.0).round(), w.size.round())
}

/// The keys of the page's large words.
pub(crate) fn word_keys(page: &Page, p: &Params) -> (f64, HashSet<String>) {
    let body = crate::body_size(std::iter::once(page)).0;
    let keys = page
        .lines
        .iter()
        .flat_map(|l| l.words.iter())
        .filter(|w| large(w, body, p) && !strip(&w.text).is_empty())
        .map(word_key)
        .collect();
    (body, keys)
}

/// The key of a picture big enough to be a backdrop (at least 5% of the page), by its size in
/// samples and its place on a 6 point grid.
pub(crate) fn image_key(page: &Page, im: &Image) -> Option<String> {
    let area = (im.x1 - im.x0) * (im.y1 - im.y0);
    (area >= 0.05 * pmax(page.width * page.height, 1.0)).then(|| {
        format!("{}x{}|{}|{}|{}", im.px_w, im.px_h, (im.x0 / 6.0).round(), (im.y0 / 6.0).round(), (im.x1 / 6.0).round())
    })
}

/// Remove the large words that repeat (`counts` holds the pages with each key, the page's own
/// included; `need` is how many make a watermark). Returns how many words went.
pub(crate) fn strip_repeated(
    page: &mut Page,
    body: f64,
    counts: &std::collections::HashMap<String, usize>,
    need: f64,
    p: &Params,
) -> usize {
    remove(page, |_, _, w| large(w, body, p) && counts.get(&word_key(w)).is_some_and(|&n| n as f64 >= need))
}
