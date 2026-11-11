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

/// Per point: whether a dark filled box lies under it (light text on a dark banner is content).
///
/// All points at once, by a sweep over x with Fenwick trees over the boxes' y edges: O((points +
/// boxes) log boxes). A page can carry many boxes and many large pale words, and testing each
/// word against every box is quadratic.
fn on_dark_boxes(page: &Page, points: &[(f64, f64)]) -> Vec<bool> {
    let mut out = vec![false; points.len()];
    // a box with unordered or NaN edges holds no point
    let dark: Vec<(f64, f64, f64, f64)> = page
        .rules
        .iter()
        .filter(|r| r.boxed && r.x0 <= r.x1 && r.y0 <= r.y1 && luminance(&r.color) < 0.5)
        .map(|r| (r.x0, r.y0, r.x1, r.y1))
        .collect();
    if dark.is_empty() || points.is_empty() {
        return out;
    }
    let mut ys: Vec<f64> = dark.iter().flat_map(|b| [b.1, b.3]).collect();
    ys.sort_by(|a, b| fcmp(*a, *b));
    ys.dedup_by(|a, b| a == b);
    let at = |y: f64| ys.partition_point(|&v| v < y);
    // active boxes counted by their top edge and by their bottom edge
    let mut tops = Fenwick::new(ys.len());
    let mut bottoms = Fenwick::new(ys.len());
    let mut by_x0: Vec<usize> = (0..dark.len()).collect();
    by_x0.sort_by(|&a, &b| fcmp(dark[a].0, dark[b].0));
    let mut by_x1: Vec<usize> = (0..dark.len()).collect();
    by_x1.sort_by(|&a, &b| fcmp(dark[a].2, dark[b].2));
    let mut queries: Vec<usize> =
        (0..points.len()).filter(|&i| !points[i].0.is_nan() && !points[i].1.is_nan()).collect();
    queries.sort_by(|&a, &b| fcmp(points[a].0, points[b].0));
    let (mut opened, mut closed) = (0, 0);
    for q in queries {
        let (cx, cy) = points[q];
        // boxes with x0 <= cx are active until x1 < cx (x0 <= x1, so a closed box was opened)
        while opened < dark.len() && dark[by_x0[opened]].0 <= cx {
            let b = dark[by_x0[opened]];
            tops.add(at(b.1), 1);
            bottoms.add(at(b.3), 1);
            opened += 1;
        }
        while closed < dark.len() && dark[by_x1[closed]].2 < cx {
            let b = dark[by_x1[closed]];
            tops.add(at(b.1), -1);
            bottoms.add(at(b.3), -1);
            closed += 1;
        }
        // active boxes with y0 <= cy, less those with y1 < cy (whose y0 < cy too)
        let above = tops.sum(ys.partition_point(|&v| v <= cy));
        let ended = bottoms.sum(ys.partition_point(|&v| v < cy));
        out[q] = above > ended;
    }
    out
}

/// Counts at positions, with prefix sums, both in O(log n).
struct Fenwick(Vec<i64>);

impl Fenwick {
    fn new(n: usize) -> Fenwick {
        Fenwick(vec![0; n + 1])
    }

    fn add(&mut self, at: usize, v: i64) {
        let mut i = at + 1;
        while i < self.0.len() {
            self.0[i] += v;
            i += i & i.wrapping_neg();
        }
    }

    /// The sum over positions `0..end`.
    fn sum(&self, end: usize) -> i64 {
        let (mut i, mut s) = (end.min(self.0.len() - 1), 0);
        while i > 0 {
            s += self.0[i];
            i -= i & i.wrapping_neg();
        }
        s
    }
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
    let pale_words: Vec<(usize, usize, (f64, f64))> = page
        .lines
        .iter()
        .enumerate()
        .flat_map(|(li, l)| l.words.iter().enumerate().map(move |(wi, w)| (li, wi, w)))
        .filter(|(_, _, w)| {
            let l = luminance(&w.color);
            large(w, body, p) && l >= p.watermark_min_lum && l < 0.97
        })
        .map(|(li, wi, w)| (li, wi, centre(w)))
        .collect();
    let points: Vec<(f64, f64)> = pale_words.iter().map(|c| c.2).collect();
    let pale: HashSet<(usize, usize)> = pale_words
        .iter()
        .zip(on_dark_boxes(page, &points))
        .filter(|(_, dark)| !dark)
        .map(|((li, wi, _), _)| (*li, *wi))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{GlyphRange, Rule};

    fn rule(x0: f64, y0: f64, x1: f64, y1: f64, color: &str) -> Rule {
        Rule { x0, y0, x1, y1, color: color.to_string(), boxed: true }
    }

    fn page(rules: Vec<Rule>) -> Page {
        Page {
            number: Some(1),
            images: Vec::new(),
            images_truncated: false,
            graphics: Vec::new(),
            width: 612.0,
            height: 792.0,
            lines: Vec::new(),
            rules,
            rules_truncated: None,
            glyphs: Vec::new(),
            fonts: Vec::new(),
            extra_glyphs: Vec::new(),
            glyphs_truncated: false,
        }
    }

    /// The quadratic check this replaces.
    fn naive(page: &Page, (cx, cy): (f64, f64)) -> bool {
        page.rules
            .iter()
            .any(|r| r.boxed && r.x0 <= cx && cx <= r.x1 && r.y0 <= cy && cy <= r.y1 && luminance(&r.color) < 0.5)
    }

    #[test]
    fn dark_box_index_matches_the_scan() {
        // a small deterministic generator (xorshift): boxes on a coarse grid so edges coincide
        let mut s: u64 = 0x9e37_79b9_7f4a_7c15;
        let mut next = |n: f64| {
            s ^= s << 13;
            s ^= s >> 7;
            s ^= s << 17;
            (s % n as u64) as f64
        };
        for _ in 0..50 {
            let mut rules = Vec::new();
            for _ in 0..40 {
                let (x0, y0) = (next(20.0), next(20.0));
                let (w, h) = (next(8.0), next(8.0));
                let color = if next(3.0) == 0.0 { "#eeeeee" } else { "#101010" };
                // some unordered and NaN boxes, which hold nothing
                let (x0, x1) = if next(10.0) == 0.0 { (x0 + w, x0) } else { (x0, x0 + w) };
                let y1 = if next(25.0) == 0.0 { f64::NAN } else { y0 + h };
                rules.push(rule(x0, y0, x1, y1, color));
            }
            rules.push(rule(f64::NEG_INFINITY, 5.0, 3.0, f64::INFINITY, "#000000"));
            let pg = page(rules);
            let mut points: Vec<(f64, f64)> = (0..200).map(|_| (next(60.0) / 2.0, next(60.0) / 2.0)).collect();
            points.push((f64::NAN, 4.0));
            points.push((-1e300, 6.0));
            let got = on_dark_boxes(&pg, &points);
            for (k, &pt) in points.iter().enumerate() {
                assert_eq!(got[k], naive(&pg, pt), "{pt:?}");
            }
        }
    }

    fn pale_page(pale: usize, boxes: usize) -> Page {
        let mut rules = Vec::with_capacity(boxes);
        for i in 0..boxes {
            // dark boxes, none under a word
            let (x, y) = ((i % 300) as f64 * 2.0, 3000.0 + ((i / 300) % 300) as f64 * 2.0);
            rules.push(rule(x, y, x + 1.0, y + 1.0, "#101010"));
        }
        let mut pg = page(rules);
        let word = |x0: f64, y: f64, size: f64, text: &str, color: &str| Word {
            x0,
            x1: x0 + size,
            y0: y,
            y1: y + size,
            text: Rc::from(text),
            bold: false,
            italic: false,
            size,
            base: y + 0.8 * size,
            space: true,
            color: Rc::from(color),
            rot: 0,
            invisible: false,
            glyphs: GlyphRange::default(),
        };
        let body: Vec<Word> = (0..pale / 2 + 10)
            .map(|i| word(72.0 + i as f64 * 0.01, 90.0, 10.0, "bodytextbodytext", "#000000"))
            .collect();
        pg.lines.push(Rc::new(Line::new(body, 0, 0)));
        for li in 0..pale / 50 {
            let ws =
                (0..50).map(|k| word(72.0 + k as f64 * 10.0, 100.0 + li as f64 * 0.5, 20.0, "W", "#cccccc")).collect();
            pg.lines.push(Rc::new(Line::new(ws, 1 + li as i64, 0)));
        }
        pg
    }

    /// The council's sizes (200k pale words on 20k boxes, 20k on 100k): about a second or two
    /// when quadratic, tens of milliseconds now. Timed only in release builds.
    #[test]
    fn pale_words_on_many_boxes_are_near_linear() {
        let p = Params::default();
        for (pale, boxes) in [(200_000, 20_000), (20_000, 100_000)] {
            let mut pg = pale_page(pale, boxes);
            let start = std::time::Instant::now();
            let removed = strip_local(&mut pg, &p);
            let took = start.elapsed();
            assert_eq!(removed, pale / 50 * 50);
            if !cfg!(debug_assertions) {
                assert!(took.as_secs_f64() < 1.0, "{pale} pale words, {boxes} boxes: {took:?}");
            }
        }
    }
}
