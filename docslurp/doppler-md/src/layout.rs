//! Region-first reading order (bands and column gutters).

use std::collections::HashSet;
use std::rc::Rc;

use crate::model::Line;
use crate::params::Params;
use crate::pyc::{fcmp, median, pmax, pmin, strip};
use crate::tables::{bullet, numbered};

/// A positioned page line or table payload. The default payload preserves the standalone
/// geometry helpers; page layout carries the table itself instead of a second index space.
#[derive(Clone, Copy, Debug)]
pub struct Elem<T = usize> {
    pub line: Option<usize>,
    pub table: T,
    pub x0: f64,
    pub x1: f64,
    pub y0: f64,
    pub y1: f64,
}

/// Python caps region recursion here.
pub const MAX_DEPTH: usize = 6;

fn sort_yx<T>(es: &mut [Elem<T>]) {
    es.sort_by(|a, b| fcmp(a.y0, b.y0).then(fcmp(a.x0, b.x0)));
}

/// x intervals no element crosses, at least column_gutter x body wide, with text on both sides.
pub fn gutters_of<T>(elements: &[Elem<T>], body: f64, p: &Params) -> Vec<(f64, f64)> {
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
pub fn bands<T: Clone>(elements: &[Elem<T>]) -> Vec<Vec<Elem<T>>> {
    let mut es = elements.to_vec();
    sort_yx(&mut es);
    let mut out: Vec<Vec<Elem<T>>> = Vec::new();
    let mut reach = 0.0;
    for e in es {
        let h = 0.2 * (e.y1 - e.y0);
        match out.last_mut() {
            Some(last) if e.y0 + h < reach => {
                reach = pmax(reach, e.y1 - h);
                last.push(e);
            }
            _ => {
                reach = e.y1 - h;
                out.push(vec![e]);
            }
        }
    }
    out
}

/// A gutter inside a table (short cells sharing baselines) or after a column of list markers.
pub fn table_like<T>(left: &[Elem<T>], right: &[Elem<T>], lines: &[Rc<Line>], p: &Params) -> bool {
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
    aligned_count(&left, &right, p) as f64 >= p.table_align * right.len() as f64
}

/// Right-side lines sharing a baseline (within `row_tol`) with a left-side line.
fn aligned_count(left: &[&Line], right: &[&Line], p: &Params) -> usize {
    let mut bases: Vec<f64> = left.iter().map(|l| l.base()).collect();
    bases.sort_by(|a, b| fcmp(*a, *b));
    // right-side baselines in ascending order too, so the neighbour search is one merge pass
    let mut queries: Vec<(f64, f64)> = right.iter().map(|l| (l.base(), p.row_tol * pmax(l.size(), 1.0))).collect();
    queries.sort_by(|a, b| fcmp(a.0, b.0));
    let nan = bases.first().is_some_and(|b| b.is_nan()) || bases.last().is_some_and(|b| b.is_nan());
    let mut k = 0;
    let mut aligned = 0usize;
    for (x, tol) in queries {
        if nan || !x.is_finite() {
            aligned += near_any(&bases, x, tol) as usize;
            continue;
        }
        // k = bases.partition_point(|&b| b < x), monotone in x
        while k < bases.len() && bases[k] < x {
            k += 1;
        }
        let hit = bases.get(k).is_some_and(|&b| (x - b).abs() <= tol) || (k > 0 && (x - bases[k - 1]).abs() <= tol);
        aligned += hit as usize;
    }
    aligned
}

/// A gutter inside a borderless table whose rows wrap (`wrapped_rows`): [`table_like`]'s test
/// with the right side's wrapped lines left out, since only a row's first line shares a
/// baseline across the gutter. The lines on both sides must fold into rows
/// ([`crate::tables::fold_wraps`]), the rows that have wrapped lines a median of three cells or
/// more (two columns of short labels beside paragraphs read as a page layout, not a table), and
/// the sides must not both be prose (`table_cell_words`). `None` once the budget runs out.
pub fn wrapped_table_like<T>(
    left: &[Elem<T>],
    right: &[Elem<T>],
    lines: &[Rc<Line>],
    p: &Params,
    budget: &mut Budget,
) -> Option<bool> {
    let left_ids: Vec<usize> = left.iter().filter_map(|e| e.line).collect();
    let right_ids: Vec<usize> = right.iter().filter_map(|e| e.line).collect();
    if left_ids.is_empty() || right_ids.is_empty() {
        return Some(false);
    }
    let words = |ids: &[usize]| median(ids.iter().map(|&i| lines[i].words.len() as f64).collect());
    if words(&left_ids) > p.table_cell_words && words(&right_ids) > p.table_cell_words {
        return Some(false);
    }
    let ids: Vec<usize> = left_ids.iter().chain(&right_ids).copied().collect();
    let rows = crate::tables::fold_wraps(crate::tables::visual_rows_of(lines, &ids, p), p, budget)?;
    let mut wrapped = HashSet::new();
    let mut cells = Vec::new();
    for r in rows.iter().filter(|r| !r.wraps.is_empty()) {
        cells.push(r.cells.len() as f64);
        // the row's lines below its first baseline
        wrapped.extend(r.ids.iter().copied().filter(|&i| {
            let l = &lines[i];
            (l.base() - r.base).abs() > p.row_tol * pmax(l.size(), 1.0)
        }));
    }
    if cells.is_empty() || median(cells) < 3.0 {
        return Some(false);
    }
    let left: Vec<&Line> = left_ids.iter().map(|&i| &*lines[i]).collect();
    let right: Vec<&Line> = right_ids.iter().filter(|i| !wrapped.contains(i)).map(|&i| &*lines[i]).collect();
    Some(!right.is_empty() && aligned_count(&left, &right, p) as f64 >= p.table_align * right.len() as f64)
}

/// `any(abs(x - b) <= tol for b in sorted)` for `sorted` in ascending `fcmp` order, in
/// O(log n): for a finite `x` the closest values are its two neighbours in sorted order
/// (floating-point subtraction is monotonic), and the predicate only depends on the distance.
/// A non-finite `x` (only reachable through overflowing medians) takes the linear scan.
pub fn near_any(sorted: &[f64], x: f64, tol: f64) -> bool {
    // total_cmp order puts any NaN first (negative sign) or last
    let nan = sorted.first().is_some_and(|b| b.is_nan()) || sorted.last().is_some_and(|b| b.is_nan());
    if !x.is_finite() || nan {
        return sorted.iter().any(|&b| (x - b).abs() <= tol);
    }
    let k = sorted.partition_point(|&b| b < x);
    sorted.get(k).is_some_and(|&b| (x - b).abs() <= tol) || (k > 0 && (x - sorted[k - 1]).abs() <= tol)
}

/// `any(abs(x - b) <= tol for b in sorted without one copy of x)`: [`near_any`] over the
/// other values, for an `x` taken from `sorted` itself.
pub fn near_other(sorted: &[f64], x: f64, tol: f64) -> bool {
    let nan = sorted.first().is_some_and(|b| b.is_nan()) || sorted.last().is_some_and(|b| b.is_nan());
    if !x.is_finite() || nan {
        let mut skipped = false;
        return sorted.iter().any(|&b| {
            if !skipped && b == x {
                skipped = true;
                return false;
            }
            (x - b).abs() <= tol
        });
    }
    let lo = sorted.partition_point(|&b| b < x);
    let hi = sorted.partition_point(|&b| b <= x);
    (hi - lo >= 2 && 0.0 <= tol)
        || sorted.get(hi).is_some_and(|&b| (x - b).abs() <= tol)
        || (lo > 0 && (x - sorted[lo - 1]).abs() <= tol)
}

/// A work allowance for a page's layout. One unit is one element visited: each [`regions_within`]
/// call charges its element count, each band a gutter is followed through charges the band's
/// elements, each gutter that reaches the column test charges its member count, and each
/// same-baseline merge in [`crate::prose::paragraphs_within`] charges the merged word count.
/// `doppler_md.py` charges the same units in the same places. Gutter-table detection
/// ([`crate::tables::find_tables_within`]) also charges twice the words of each run that
/// reaches column detection; Python does not, so its pages stop slightly later.
#[derive(Clone, Copy, Debug)]
pub struct Budget {
    pub used: u64,
    pub limit: f64,
}

impl Budget {
    pub fn new(limit: f64) -> Budget {
        Budget { used: 0, limit }
    }

    /// Charge `n` units; false once the total is over the limit.
    pub(crate) fn spend(&mut self, n: usize) -> bool {
        self.used = self.used.saturating_add(n as u64);
        self.used as f64 <= self.limit
    }
}

/// Layout regions in reading order. Runs of bands sharing a non-table vertical gutter are read
/// column by column.
pub fn regions<T: Clone>(
    elements: &[Elem<T>],
    lines: &[Rc<Line>],
    body: f64,
    p: &Params,
    depth: usize,
) -> Vec<Vec<Elem<T>>> {
    regions_within(elements, lines, body, p, depth, &mut Budget::new(f64::INFINITY)).unwrap_or_default()
}

/// [`regions`] within a work budget: `None` once the budget runs out (the region search is
/// superlinear on hostile pages, e.g. a grid of one-cell lines re-follows every gutter through
/// every later band).
pub fn regions_within<T: Clone>(
    elements: &[Elem<T>],
    lines: &[Rc<Line>],
    body: f64,
    p: &Params,
    depth: usize,
    budget: &mut Budget,
) -> Option<Vec<Vec<Elem<T>>>> {
    if !budget.spend(elements.len()) {
        return None;
    }
    let mut elements = elements.to_vec();
    sort_yx(&mut elements);
    if elements.len() < 3 || depth > MAX_DEPTH {
        return Some(if elements.is_empty() { Vec::new() } else { vec![elements] });
    }
    let bs = bands(&elements);
    // each band's own gutters, once: the start candidates for band i and the `multi` test of
    // every band a gutter is followed through
    let band_gutters: Vec<Vec<(f64, f64)>> = bs.iter().map(|b| gutters_of(b, body, p)).collect();
    let mut out: Vec<Vec<Elem<T>>> = Vec::new();
    let mut pending: Vec<Elem<T>> = Vec::new();
    // bands up to here belong to a table whose rows wrap (`wrapped_table_like`)
    let mut held = 0usize;
    let mut i = 0;
    while i < bs.len() {
        let mut split = None;
        let mut gs = band_gutters[i].clone();
        gs.sort_by(|g, h| fcmp(g.0 - g.1, h.0 - h.1));
        for (mut a, mut b) in gs {
            // follow the gutter down; elements reaching into it from one side narrow it
            let mut j = i;
            let mut multi = 0usize;
            while j < bs.len() {
                let band = &bs[j];
                if !budget.spend(band.len()) {
                    return None;
                }
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
                multi += !band_gutters[j].is_empty() as usize;
                j += 1;
            }
            // the members are bands i..j; nothing below depends on them unless enough bands
            // carry a gutter (each band start used to copy every member, O(n^2) on tall pages)
            if (multi as f64) < p.column_min_bands {
                continue;
            }
            if !budget.spend(bs[i..j].iter().map(Vec::len).sum()) {
                return None;
            }
            let members = bs[i..j].iter().flatten();
            let left: Vec<Elem<T>> = members.clone().cloned().filter(|e| e.x1 <= a + 1.0).collect();
            let right: Vec<Elem<T>> = members.cloned().filter(|e| e.x0 >= b - 1.0).collect();
            if left.len() >= 2 && right.len() >= 2 && !table_like(&left, &right, lines, p) {
                // bands already read as one wrapped table stay together: their last rows alone
                // are too few to show the row padding again
                let wrapped = p.wrapped_rows
                    && (i < held || {
                        let w = wrapped_table_like(&left, &right, lines, p, budget)?;
                        if w {
                            held = held.max(j);
                        }
                        w
                    });
                if !wrapped {
                    split = Some((j, left, right));
                    break;
                }
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
        out.extend(regions_within(&left, lines, body, p, depth + 1, budget)?);
        out.extend(regions_within(&right, lines, body, p, depth + 1, budget)?);
        i = j;
    }
    if !pending.is_empty() {
        out.push(pending);
    }
    Some(out)
}
