//! Rotated text read in its own frame.
//!
//! Layout and table detection assume text runs left to right with lines advancing down the
//! page. A table printed sideways breaks both: its rows become page columns, a row's cells
//! become separate lines, and the baselines are x coordinates. Here the lines of one rotation
//! are rebuilt in their own reading frame (each word box turned by the inverse of its rotation),
//! together with a copy of the rules around them, so the ordinary layout reads them; the
//! result is placed back on the page as one element at the lines' upright extent.

use std::rc::Rc;

use crate::model::{Line, Page, Rule, Word};
use crate::params::Params;
use crate::pyc::{pmax, pmin};

/// The rotation all of a line's words share, else 0 (the line keeps the upright treatment).
pub(crate) fn line_rot(line: &Line) -> u8 {
    let r = line.words.first().map_or(0, |w| w.rot);
    if line.words.iter().all(|w| w.rot == r) { r } else { 0 }
}

/// A page frame turned by `rot` quarter turns: maps page coordinates into the reading frame of
/// text with that rotation.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Frame {
    rot: u8,
    width: f64,
    height: f64,
}

impl Frame {
    pub(crate) fn new(rot: u8, width: f64, height: f64) -> Frame {
        Frame { rot, width, height }
    }

    /// Page width and height in the reading frame.
    pub(crate) fn dims(&self) -> (f64, f64) {
        if self.rot % 2 == 1 { (self.height, self.width) } else { (self.width, self.height) }
    }

    /// A box (x0, y0, x1, y1) in the reading frame. Rotation 1 reads down the page (+y) with
    /// lines advancing toward -x; rotation 3 reads up the page with lines advancing toward +x;
    /// rotation 2 is upside down.
    ///
    /// The result is always ordered (x0 <= x1, y0 <= y1): the producer can write an inverted
    /// box (xMin > xMax), and in the reframed layout such a box would sit on both sides of a
    /// column gutter and be read twice.
    pub(crate) fn rect(&self, x0: f64, y0: f64, x1: f64, y1: f64) -> (f64, f64, f64, f64) {
        let (w, h) = (self.width, self.height);
        let (a, b, c, d) = match self.rot {
            1 => (y0, w - x1, y1, w - x0),
            2 => (w - x1, h - y1, w - x0, h - y0),
            3 => (h - y1, x0, h - y0, x1),
            _ => (x0, y0, x1, y1),
        };
        (pmin(a, c), pmin(b, d), pmax(a, c), pmax(b, d))
    }

    /// A baseline (an x coordinate for rotations 1 and 3) in the reading frame; 0 stays 0, the
    /// producer's "no baseline".
    fn base(&self, b: f64) -> f64 {
        if b == 0.0 {
            return 0.0;
        }
        match self.rot {
            1 => self.width - b,
            2 => self.height - b,
            _ => b,
        }
    }

    fn word(&self, w: &Word) -> Word {
        let (x0, y0, x1, y1) = self.rect(w.x0, w.y0, w.x1, w.y1);
        Word { x0, y0, x1, y1, base: self.base(w.base), rot: 0, ..w.clone() }
    }

    pub(crate) fn line(&self, l: &Line) -> Line {
        Line::new(l.words.iter().map(|w| self.word(w)).collect(), l.block, l.flow)
    }

    fn rule(&self, r: &Rule) -> Rule {
        let (x0, y0, x1, y1) = self.rect(r.x0, r.y0, r.x1, r.y1);
        Rule { x0, y0, x1, y1, ..r.clone() }
    }
}

/// Kept lines of one rotation that are read in their own frame.
pub(crate) struct RotatedGroup {
    pub page: Page,
    /// the lines' extent on the (upright) page
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
}

/// Lines (by id) to lay out upright, and the rotated groups to lay out in their own frames.
///
/// A rotation forms a group when every word box of it is ordered and within a page's distance
/// of the page, it has at least `rotated_min_lines` kept lines and at most
/// `rotated_stray_share` of that many other kept lines (rounded up) have their centre inside
/// their extent; those stay upright. With more, the rotated lines are labels among upright
/// text and stay where they were. Each group gets the rules that touch its extent, then those touching the
/// grown extent (twice), so a ruled table's border far from its text comes along; rules are
/// copied, not moved, since a rule group without member lines is no table in either frame.
pub(crate) fn split(page: &Page, ids: &[usize], p: &Params) -> (Vec<usize>, Vec<RotatedGroup>) {
    if !p.rotated_frames || ids.iter().all(|&i| line_rot(&page.lines[i]) == 0) {
        return (ids.to_vec(), Vec::new());
    }
    let rots: Vec<u8> = ids.iter().map(|&i| line_rot(&page.lines[i])).collect();
    let mut groups = Vec::new();
    let mut taken = vec![false; ids.len()];
    for rot in 1..=3u8 {
        let members: Vec<usize> = (0..ids.len()).filter(|&k| rots[k] == rot).collect();
        if (members.len() as f64) < p.rotated_min_lines {
            continue;
        }
        let lines = |ks: &[usize]| ks.iter().map(|&k| &page.lines[ids[k]]).collect::<Vec<_>>();
        let ls = lines(&members);
        // only well-formed geometry is reframed: an inverted word box (xMin > xMax) or one far
        // off the page keeps the upright treatment, which reads every word once
        let (pw, ph) = (pmax(page.width.abs(), 1.0), pmax(page.height.abs(), 1.0));
        let sane = |w: &Word| {
            w.x0 <= w.x1
                && w.y0 <= w.y1
                && [w.x0, w.x1].iter().all(|&x| -pw <= x && x <= 2.0 * pw)
                && [w.y0, w.y1].iter().all(|&y| -ph <= y && y <= 2.0 * ph)
        };
        if !ls.iter().all(|l| l.words.iter().all(sane)) {
            continue;
        }
        let x0 = ls.iter().map(|l| l.x0).fold(f64::INFINITY, pmin);
        let y0 = ls.iter().map(|l| l.y0).fold(f64::INFINITY, pmin);
        let x1 = ls.iter().map(|l| l.x1).fold(f64::NEG_INFINITY, pmax);
        let y1 = ls.iter().map(|l| l.y1).fold(f64::NEG_INFINITY, pmax);
        if !(x0 <= x1 && y0 <= y1) {
            continue; // non-finite extent
        }
        let inside = |l: &Line| {
            let (cx, cy) = ((l.x0 + l.x1) / 2.0, (l.y0 + l.y1) / 2.0);
            x0 < cx && cx < x1 && y0 < cy && cy < y1
        };
        // a few other lines inside (a stamp, a Bates number) stay in the upright layout; more
        // mean the rotated lines are labels among upright text, which keep the old treatment
        let strays = (0..ids.len()).filter(|&k| rots[k] != rot && inside(&page.lines[ids[k]])).count();
        // rounded up: any positive share allows at least one, 0 allows none
        if strays as f64 > (p.rotated_stray_share * members.len() as f64).ceil() {
            continue;
        }
        let frame = Frame::new(rot, page.width, page.height);
        let (width, height) = frame.dims();
        let rules = nearby_rules(&page.rules, (x0, y0, x1, y1)).into_iter().map(|r| frame.rule(r)).collect();
        let sub = Page {
            width,
            height,
            lines: ls.iter().map(|l| Rc::new(frame.line(l))).collect(),
            rules,
            rules_truncated: page.rules_truncated,
        };
        for &k in &members {
            taken[k] = true;
        }
        groups.push(RotatedGroup { page: sub, x0, y0, x1, y1 });
    }
    let upright = (0..ids.len()).filter(|&k| !taken[k]).map(|k| ids[k]).collect();
    (upright, groups)
}

/// Rules touching `bbox` (1 point of slack), then those touching the extent grown by them,
/// twice more. Three linear passes.
fn nearby_rules(rules: &[Rule], bbox: (f64, f64, f64, f64)) -> Vec<&Rule> {
    let (mut x0, mut y0, mut x1, mut y1) = bbox;
    let mut keep = vec![false; rules.len()];
    for _ in 0..3 {
        let (mut nx0, mut ny0, mut nx1, mut ny1) = (x0, y0, x1, y1);
        for (k, r) in rules.iter().enumerate() {
            if !keep[k] && r.x0 <= x1 + 1.0 && r.x1 >= x0 - 1.0 && r.y0 <= y1 + 1.0 && r.y1 >= y0 - 1.0 {
                keep[k] = true;
                nx0 = pmin(nx0, r.x0);
                ny0 = pmin(ny0, r.y0);
                nx1 = pmax(nx1, r.x1);
                ny1 = pmax(ny1, r.y1);
            }
        }
        (x0, y0, x1, y1) = (nx0, ny0, nx1, ny1);
    }
    rules.iter().zip(keep).filter(|(_, k)| *k).map(|(r, _)| r).collect()
}
