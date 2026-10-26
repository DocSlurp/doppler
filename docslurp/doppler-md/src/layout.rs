//! Region-first reading order (bands and column gutters).

use std::rc::Rc;

use crate::model::Line;
use crate::params::Params;
use crate::pyc::{fcmp, median, pmax, pmin, strip};
use crate::tables::{bullet, numbered};

/// A layout element: a page line (by index) or a ruled table (by index into the page's tables).
#[derive(Clone, Copy, Debug)]
pub struct Elem {
    pub line: Option<usize>,
    pub table: usize,
    pub x0: f64,
    pub x1: f64,
    pub y0: f64,
    pub y1: f64,
}

/// Python caps region recursion here.
pub const MAX_DEPTH: usize = 6;

fn sort_yx(es: &mut [Elem]) {
    es.sort_by(|a, b| fcmp(a.y0, b.y0).then(fcmp(a.x0, b.x0)));
}

/// x intervals no element crosses, at least column_gutter x body wide, with text on both sides.
pub fn gutters_of(elements: &[Elem], body: f64, p: &Params) -> Vec<(f64, f64)> {
    if elements.len() < 2 {
        return Vec::new();
    }
    let mut spans: Vec<(f64, f64)> = elements.iter().map(|e| (e.x0, e.x1)).collect();
    spans.sort_by(|a, b| fcmp(a.0, b.0).then(fcmp(a.1, b.1)));
    let mut out = Vec::new();
    let mut reach = spans[0].1;
    for &(x0, x1) in &spans[1..] {
        if x0 - reach >= p.column_gutter * body {
            out.push((reach, x0));
        }
        reach = pmax(reach, x1);
    }
    out
}

/// Split elements at y positions no element overlaps (boxes shrunk by 20% to ignore ascender overlap).
pub fn bands(elements: &[Elem]) -> Vec<Vec<Elem>> {
    let mut es = elements.to_vec();
    sort_yx(&mut es);
    let mut out: Vec<Vec<Elem>> = Vec::new();
    let mut reach = 0.0;
    for e in es {
        let h = 0.2 * (e.y1 - e.y0);
        match out.last_mut() {
            Some(last) if e.y0 + h < reach => {
                last.push(e);
                reach = pmax(reach, e.y1 - h);
            }
            _ => {
                out.push(vec![e]);
                reach = e.y1 - h;
            }
        }
    }
    out
}

/// A gutter inside a table (short cells sharing baselines) or after a column of list markers.
pub fn table_like(left: &[Elem], right: &[Elem], lines: &[Rc<Line>], p: &Params) -> bool {
    let left: Vec<&Line> = left.iter().filter_map(|e| e.line.map(|i| &*lines[i])).collect();
    let right: Vec<&Line> = right.iter().filter_map(|e| e.line.map(|i| &*lines[i])).collect();
    if left.is_empty() || right.is_empty() {
        return false;
    }
    if left.iter().all(|l| {
        let t = strip(l.text());
        numbered(t) || bullet(t)
    }) {
        return true;
    }
    let words = |ls: &[&Line]| median(ls.iter().map(|l| l.words.len() as f64).collect());
    if words(&left) > p.table_cell_words && words(&right) > p.table_cell_words {
        return false;
    }
    let bases: Vec<f64> = left.iter().map(|l| l.base()).collect();
    let aligned =
        right.iter().filter(|l| bases.iter().any(|&b| (l.base() - b).abs() <= p.row_tol * pmax(l.size(), 1.0))).count();
    aligned as f64 >= p.table_align * right.len() as f64
}

/// Layout regions in reading order. Runs of bands sharing a non-table vertical gutter are read
/// column by column.
pub fn regions(elements: &[Elem], lines: &[Rc<Line>], body: f64, p: &Params, depth: usize) -> Vec<Vec<Elem>> {
    let mut elements = elements.to_vec();
    sort_yx(&mut elements);
    if elements.len() < 3 || depth > MAX_DEPTH {
        return if elements.is_empty() { Vec::new() } else { vec![elements] };
    }
    let bs = bands(&elements);
    let mut out: Vec<Vec<Elem>> = Vec::new();
    let mut pending: Vec<Elem> = Vec::new();
    let mut i = 0;
    while i < bs.len() {
        let mut split = None;
        let mut gs = gutters_of(&bs[i], body, p);
        gs.sort_by(|g, h| fcmp(g.0 - g.1, h.0 - h.1));
        for (mut a, mut b) in gs {
            // follow the gutter down; elements reaching into it from one side narrow it
            let mut j = i;
            let mut members: Vec<Elem> = Vec::new();
            let mut multi = 0usize;
            while j < bs.len() {
                let band = &bs[j];
                let (mut na, mut nb) = (a, b);
                for e in band {
                    if e.x0 < nb - 1.0 && e.x1 > na + 1.0 {
                        if (e.x0 + e.x1) / 2.0 < (a + b) / 2.0 && e.x0 < a + 1.0 {
                            na = pmax(na, e.x1);
                        } else if e.x1 > b - 1.0 {
                            nb = pmin(nb, e.x0);
                        } else {
                            nb = na; // an element inside the gutter
                        }
                    }
                }
                if nb - na < p.column_gutter * body {
                    break;
                }
                a = na;
                b = nb;
                members.extend_from_slice(band);
                multi += !gutters_of(band, body, p).is_empty() as usize;
                j += 1;
            }
            let left: Vec<Elem> = members.iter().copied().filter(|e| e.x1 <= a + 1.0).collect();
            let right: Vec<Elem> = members.iter().copied().filter(|e| e.x0 >= b - 1.0).collect();
            if multi as f64 >= p.column_min_bands
                && left.len() >= 2
                && right.len() >= 2
                && !table_like(&left, &right, lines, p)
            {
                split = Some((j, left, right));
                break;
            }
        }
        let Some((j, left, right)) = split else {
            pending.extend_from_slice(&bs[i]);
            i += 1;
            continue;
        };
        if !pending.is_empty() {
            out.push(std::mem::take(&mut pending));
        }
        out.extend(regions(&left, lines, body, p, depth + 1));
        out.extend(regions(&right, lines, body, p, depth + 1));
        i = j;
    }
    if !pending.is_empty() {
        out.push(pending);
    }
    out
}
