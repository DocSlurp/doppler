//! Vector figure markers: charts and diagrams drawn with paths, so a caller can put them back.
//!
//! The producer (`pdftotext -images`) summarises the painted drawing of a page that is not
//! axis-aligned (curves and diagonal segments) as `<graphic>` clusters; rules, boxes, table
//! grids, underlines and frames are left out there. Here neighbouring clusters become one
//! drawing; a drawing with enough of it (many diagonal or curved segments, in several
//! directions or with curves, and not tiny) is a figure. Its region grows by the rules that
//! touch it (axes, ticks, the boxes of a diagram) and by the short text lines around it (tick
//! labels, axis titles, legends). The figure gets the marker `![image](doppler-image:pN.gK)`,
//! a paragraph of its own in reading order, with the text inside its region after it; `K` is
//! the position of its first `<graphic>` in the producer's list for the page (one-based); a bar chart drawn only
//! with boxes (no `<graphic>`) takes the next K past the page's last graphic, one per bar region, in order.

use std::rc::Rc;

use crate::images::{Fate, Figure, Note, clusters};
use crate::layout::Budget;
use crate::model::Page;
use crate::params::Params;
use crate::pyc::{pmax, pmin};

type Bbox = (f64, f64, f64, f64);

/// The most drawings of one page that can become figures (merging them is quadratic per round);
/// the producer writes at most 256 clusters a page.
const MAX_FIGURES: usize = 256;

/// Rounds of growing a figure by rules, labels and sparse drawing.
const GROW_ROUNDS: usize = 5;

/// Decide the vector drawing of `page`: figures to place (each with the lines inside it as
/// labels), one report note per drawing, and the pictures a figure took in. `raster` are the
/// picture figures already placed, with their ids: drawing over a larger picture (axes over a
/// bitmap plot) is part of that picture, and pictures inside a larger drawing (bitmaps in a
/// diagram) are part of the drawing: `(index into raster, figure id)`.
///
/// Each pass over the page's rules or lines is charged to `budget`. Once it runs out, drawings
/// not yet checked are `Small`, and figures stop growing and merging where they are.
#[allow(clippy::too_many_arguments)]
pub fn place(
    page: &Page,
    drop: &[bool],
    body: f64,
    p: &Params,
    number: u32,
    raster: &[(Figure, String)],
    seal: &str,
    budget: &mut Budget,
) -> (Vec<Figure>, Vec<Note>, Vec<(usize, String)>) {
    let gs = &page.graphics;
    let bars = bar_regions(page, p, budget);
    if gs.is_empty() && bars.is_empty() {
        return (Vec::new(), Vec::new(), Vec::new());
    }
    let boxes: Vec<Bbox> = gs.iter().map(|g| (g.x0, g.y0, g.x1, g.y1)).collect();
    let all: Vec<usize> = (0..gs.len()).collect();
    let groups = clusters(&boxes, &all, pmax(p.vector_group_gap * body, 2.0));
    let band = p.furniture_band * page.height;
    let page_area = pmax(page.width * page.height, 1.0);
    let mut notes: Vec<Note> = Vec::with_capacity(groups.len() + bars.len());
    // accepted drawings: (note index, first graphic, region, labels)
    let mut regions: Vec<(usize, usize, Bbox, Vec<usize>)> = Vec::new();
    for unit in &groups {
        let b = union(unit.iter().map(|&i| boxes[i]));
        let (w, h) = (b.2 - b.0, b.3 - b.1);
        let sum =
            |f: fn(&crate::model::Graphic) -> u64| unit.iter().map(|&i| f(&gs[i])).fold(0u64, u64::saturating_add);
        let (segments, curves, paths) = (sum(|g| g.segments), sum(|g| g.curves), sum(|g| g.paths));
        let directions = unit.iter().map(|&i| gs[i].directions).max().unwrap_or(0);
        let rich = segments as f64 >= p.vector_min_segments
            && (curves as f64 >= p.vector_min_curves || directions as f64 >= p.vector_min_directions);
        // a thin drawing is a figure only when it has many pieces (a row of arrows, not a
        // brace) and the rules it touches make it one of some size (the boxes they join)
        let mut thin = || {
            pmin(w, h) < p.vector_min_thickness * body
                && (!budget.spend(2 * page.rules.len()) || {
                    let g = grow_by_rules(page, b);
                    (paths as f64) < p.vector_thin_min_paths || pmin(g.2 - g.0, g.3 - g.1) < p.vector_min_extent * body
                })
        };
        let fate = if b.3 <= band || b.1 >= page.height - band {
            Some(Fate::Furniture)
        } else if regions.len() >= MAX_FIGURES || !rich || pmax(w, h) < p.vector_min_extent * body || thin() {
            Some(Fate::Small)
        } else {
            None
        };
        if fate.is_none() {
            regions.push((notes.len(), unit[0], b, Vec::new()));
        }
        notes.push(Note {
            id: format!("p{number}.g{}", unit[0] + 1),
            page: number,
            bbox: b,
            px: (0, 0),
            mask: false,
            vector: true,
            fate: fate.unwrap_or(Fate::Block),
            members: unit.len(),
            group: None,
            obj: None,
            plain: false,
            text: true,
            file: None,
            degraded: false,
            link: None,
            repeat_of: None,
            scan: None,
        });
    }
    // Bar charts: runs of filled boxes on one baseline (bar_regions), after the page's drawings so
    // those keep their ids and their place as the first of any merge (council L6), and only while
    // the page has room under MAX_FIGURES (council L1). They have no `<graphic>`, so their ids
    // follow the graphics: `pN.gK` with K past the last graphic, one per run.
    for (j, b) in bars.iter().enumerate().take(MAX_FIGURES.saturating_sub(regions.len())) {
        let rep = gs.len() + j;
        regions.push((notes.len(), rep, *b, Vec::new()));
        notes.push(Note {
            id: format!("p{number}.g{}", rep + 1),
            page: number,
            bbox: *b,
            px: (0, 0),
            mask: false,
            vector: true,
            fate: Fate::Block,
            members: 1,
            group: None,
            obj: None,
            plain: false,
            text: true,
            file: None,
            degraded: false,
            link: None,
            repeat_of: None,
            scan: None,
        });
    }
    // grow each drawing by the rules that touch it and the short lines around it (gridlines and
    // their tick labels, one step a round), and take in the sparse drawing it then reaches (the
    // arrows between the boxes of a diagram), until nothing changes or GROW_ROUNDS
    let gap = pmax(p.vector_group_gap * body, 2.0);
    let mut sparse: Vec<usize> = (0..notes.len()).filter(|&k| notes[k].fate == Fate::Small).collect();
    // one round of one region: two passes over the rules and three over the lines, and the
    // sparse drawing
    let round_cost = 2 * page.rules.len() + 3 * page.lines.len();
    'grow: for round in 0..GROW_ROUNDS {
        let mut took = false;
        for r in regions.iter_mut() {
            if !budget.spend(round_cost + sparse.len()) {
                break 'grow;
            }
            let before = r.2;
            let grown = grow_by_rules(page, r.2);
            (r.2, r.3) = grow_by_labels(page, drop, grown, body, p);
            took |= r.2 != before;
            if round + 1 == GROW_ROUNDS {
                continue;
            }
            let reach = (r.2.0 - gap, r.2.1 - gap, r.2.2 + gap, r.2.3 + gap);
            let id = notes[r.0].id.clone();
            sparse.retain(|&k| {
                let b = notes[k].bbox;
                if !(b.0 <= reach.2 && b.2 >= reach.0 && b.1 <= reach.3 && b.3 >= reach.1) {
                    return true;
                }
                r.2 = union([r.2, b]);
                took = true;
                notes[k].fate = Fate::Grouped;
                notes[k].group = Some(id.clone());
                notes[r.0].members += notes[k].members;
                false
            });
        }
        if !took {
            break;
        }
    }
    // drawings whose regions now overlap are one figure, marked at the first of them; merging
    // can make a region reach another, so repeat until none overlap (each round removes one)
    let mut merged = regions;
    'outer: loop {
        if !budget.spend(merged.len() * merged.len() / 2 + notes.len()) {
            break;
        }
        for i in 0..merged.len() {
            for j in i + 1..merged.len() {
                if overlap(merged[i].2, merged[j].2) > 0.0 {
                    let (nj, _, bj, lj) = merged.remove(j);
                    let ni = merged[i].0;
                    merged[i].2 = union([merged[i].2, bj]);
                    merged[i].3.extend(lj);
                    notes[nj].fate = Fate::Grouped;
                    notes[nj].group = Some(notes[ni].id.clone());
                    notes[ni].members += notes[nj].members;
                    // anything grouped at nj now belongs to ni
                    let (old, new) = (notes[nj].id.clone(), notes[ni].id.clone());
                    for n in notes.iter_mut() {
                        if n.group.as_deref() == Some(old.as_str()) {
                            n.group = Some(new.clone());
                        }
                    }
                    continue 'outer;
                }
            }
        }
        break;
    }
    let mut figures = Vec::new();
    let mut absorbed: Vec<(usize, String)> = Vec::new();
    for (ni, rep, b, mut labels) in merged {
        budget.spend(page.lines.len() + raster.len());
        // a merged region can now hold more lines than its parts' labels
        labels.extend(page.lines.iter().enumerate().filter_map(|(li, l)| {
            let (cx, cy) = ((l.x0 + l.x1) / 2.0, (l.y0 + l.y1) / 2.0);
            let kept = !drop.get(li).copied().unwrap_or(true) && !l.words.is_empty();
            (kept && b.0 <= cx && cx <= b.2 && b.1 <= cy && cy <= b.3).then_some(li)
        }));
        labels.sort_unstable();
        labels.dedup();
        let (w, h) = (b.2 - b.0, b.3 - b.1);
        let area = pmax(w * h, 1.0);
        notes[ni].bbox = b;
        if pmin(w, h) < p.vector_min_extent * body || area < p.vector_min_share * page_area {
            notes[ni].fate = Fate::Small;
            continue;
        }
        // drawing among a mass of words (a form, a callout full of prose) is not a figure
        let words: f64 = labels
            .iter()
            .flat_map(|&li| page.lines[li].words.iter())
            .map(|wd| pmax(wd.x1 - wd.x0, 0.0) * pmax(wd.y1 - wd.y0, 0.0))
            .sum();
        let prose = labels.iter().filter(|&&li| page.lines[li].words.len() as f64 >= p.vector_prose_words).count();
        if words > p.vector_max_text_share * area || prose as f64 > p.vector_max_prose_lines {
            notes[ni].fate = Fate::Text;
            continue;
        }
        // a picture that overlaps the drawing: the larger of the two stands for both
        let mut inside: Vec<usize> = Vec::new();
        let mut within: Option<&String> = None;
        for (k, (f, id)) in raster.iter().enumerate() {
            let fb = (f.x0, f.y0, f.x1, f.y1);
            let farea = pmax((fb.2 - fb.0) * (fb.3 - fb.1), 1.0);
            if overlap(fb, b) < 0.5 * pmin(area, farea) {
                continue;
            }
            if farea > area {
                within = within.or(Some(id));
            } else {
                inside.push(k);
            }
        }
        if let Some(id) = within {
            notes[ni].fate = Fate::Grouped;
            notes[ni].group = Some(id.clone());
            continue;
        }
        let mut b = b;
        for &k in &inside {
            let f = &raster[k].0;
            b = union([b, (f.x0, f.y0, f.x1, f.y1)]);
            absorbed.push((k, notes[ni].id.clone()));
        }
        notes[ni].bbox = b;
        figures.push(Figure {
            marker: Rc::from(crate::images::sealed(number, &format!("g{}", rep + 1), seal)),
            x0: b.0,
            y0: b.1,
            x1: b.2,
            y1: b.3,
            labels,
        });
    }
    (figures, notes, absorbed)
}

/// Most filled boxes of one page considered for bar charts (the producer keeps up to 20000 rules).
const MAX_BAR_BOXES: usize = 4000;
/// Most bar runs one page yields (a chart page has a few; far below MAX_FIGURES).
const MAX_BAR_RUNS: usize = 64;
/// Bars whose bases differ by at most this share a baseline (points).
const BAR_BASE_TOLERANCE: f64 = 0.5;
/// An axis lies within this of the baseline (points) and spans this share of the run along it.
const BAR_AXIS_REACH: f64 = 2.0;
const BAR_AXIS_SPAN: f64 = 0.8;
/// Rounds of taking in stacked boxes.
const BAR_STACK_ROUNDS: usize = 4;

/// `#rrggbb` as its channels.
fn rgb(color: &str) -> Option<(u8, u8, u8)> {
    let h = color.strip_prefix('#')?;
    if h.len() != 6 || !h.is_ascii() {
        return None;
    }
    let c = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).ok();
    Some((c(0)?, c(2)?, c(4)?))
}

/// A fill that is never a bar: white (a background), or near black (redaction boxes; council M1/M2).
/// Shared with PageFacts.cc `barFillExcluded` (8-bit channels: all 255, or all at most 20).
fn bar_fill_excluded(color: &str) -> bool {
    match rgb(color) {
        Some((r, g, b)) => (r == 255 && g == 255 && b == 255) || (r <= 20 && g <= 20 && b <= 20),
        None => true,
    }
}

/// A stroked line along one axis: its position across (sorted on it) and its extent along.
#[derive(Clone, Copy)]
struct Line {
    at: f64,
    lo: f64,
    hi: f64,
}

/// Lines sorted by `at`, and those within `reach` of `at` (binary search, then a scan).
fn lines_near(lines: &[Line], at: f64, reach: f64) -> &[Line] {
    let lo = lines.partition_point(|l| l.at < at - reach);
    let hi = lines.partition_point(|l| l.at <= at + reach);
    &lines[lo..hi.max(lo)]
}

/// Bar charts drawn with filled axis-aligned boxes, which the producer writes as `<box>` rules and
/// never as `<graphic>` drawing. The same rule as PageFacts.cc `barClusters` (the router's facts):
/// - a run is at least `bar_min_count` boxes whose bases (the bottom edge of vertical bars, the
///   left edge of horizontal ones) are within half a point, with gaps of at most `bar_max_gap`
///   times the widest bar so far;
/// - its lengths differ (`bar_min_spread`: shaded table rows and zebra stripes are one length);
/// - few hold a word's centre (`bar_max_text_share`: shaded cells and highlights hold them);
/// - a stroked axis lies along the baseline and spans most of the run (`BAR_AXIS_SPAN`): a box's
///   own outline or a cell border beside one box is not an axis (form fields, council M2);
/// - white and near-black fills (backgrounds, redactions), boxes covering a quarter of the page and
///   the furniture bands are never bars.
/// A run then takes in the boxes stacked on it, and runs that overlap are one chart. At most
/// MAX_BAR_BOXES boxes and MAX_BAR_RUNS runs a page; every comparison is charged to `budget`, and
/// none is made once it runs out.
fn bar_regions(page: &Page, p: &Params, budget: &mut Budget) -> Vec<Bbox> {
    let page_area = pmax(page.width * page.height, 1.0);
    let band = p.furniture_band * page.height;
    let filled: Vec<Bbox> = page
        .rules
        .iter()
        .filter(|r| r.boxed && !bar_fill_excluded(&r.color))
        .map(|r| (r.x0, r.y0, r.x1, r.y1))
        .filter(|b| {
            let (w, h) = (b.2 - b.0, b.3 - b.1);
            w.is_finite() && h.is_finite() && w > 0.0 && h > 0.0 && w * h < 0.25 * page_area && b.3 > band && b.1 < page.height - band
        })
        .take(MAX_BAR_BOXES)
        .collect();
    if (filled.len() as f64) < p.bar_min_count || !budget.spend(4 * filled.len() + page.rules.len()) {
        return Vec::new();
    }
    // stroked lines: horizontal ones (by y) and vertical ones (by x), each sorted once
    let mut across: Vec<Line> = Vec::new();
    let mut down: Vec<Line> = Vec::new();
    for r in page.rules.iter().filter(|r| !r.boxed) {
        let (w, h) = (r.x1 - r.x0, r.y1 - r.y0);
        if !(w.is_finite() && h.is_finite()) {
            continue;
        }
        if h <= BAR_AXIS_REACH && w > h {
            across.push(Line { at: (r.y0 + r.y1) / 2.0, lo: r.x0, hi: r.x1 });
        } else if w <= BAR_AXIS_REACH && h > w {
            down.push(Line { at: (r.x0 + r.x1) / 2.0, lo: r.y0, hi: r.y1 });
        }
    }
    across.sort_by(|a, b| a.at.total_cmp(&b.at));
    down.sort_by(|a, b| a.at.total_cmp(&b.at));
    // word centres, sorted by x
    let mut words: Vec<(f64, f64)> = page
        .lines
        .iter()
        .flat_map(|l| l.words.iter())
        .map(|w| ((w.x0 + w.x1) / 2.0, (w.y0 + w.y1) / 2.0))
        .filter(|c| c.0.is_finite() && c.1.is_finite())
        .collect();
    if !budget.spend(2 * (words.len() + across.len() + down.len()) + 1) {
        return Vec::new();
    }
    words.sort_by(|a, b| a.0.total_cmp(&b.0));
    let ctx = BarContext { p, across: &across, down: &down, words: &words };
    let mut found: Vec<(Bbox, bool)> = Vec::new();
    'orient: for vertical in [true, false] {
        let base = |b: &Bbox| if vertical { b.3 } else { b.0 };
        let along = |b: &Bbox| if vertical { b.0 } else { b.1 };
        let mut order: Vec<usize> = (0..filled.len()).collect();
        order.sort_by(|&a, &b| base(&filled[a]).total_cmp(&base(&filled[b])));
        let mut start = 0;
        while start < order.len() {
            // a baseline: bases within BAR_BASE_TOLERANCE of its first (council L4: no rounding edge)
            let first = base(&filled[order[start]]);
            let mut end = start + 1;
            while end < order.len() && base(&filled[order[end]]) - first <= BAR_BASE_TOLERANCE {
                end += 1;
            }
            let group = &mut order[start..end];
            group.sort_by(|&a, &b| along(&filled[a]).total_cmp(&along(&filled[b])));
            // split the baseline into runs of neighbouring bars (a running widest bar, council L2)
            let mut run: Vec<Bbox> = Vec::new();
            let mut thick = 0.0f64;
            for &k in group.iter() {
                let b = filled[k];
                let t = if vertical { b.2 - b.0 } else { b.3 - b.1 };
                if let Some(last) = run.last() {
                    let gap = if vertical { b.0 - last.2 } else { b.1 - last.3 };
                    if gap < -0.5 || gap > p.bar_max_gap * pmax(thick, t) {
                        if let Some(r) = bar_run(&run, vertical, &ctx, budget) {
                            found.push((r, vertical));
                            if found.len() >= MAX_BAR_RUNS {
                                break 'orient;
                            }
                        }
                        run.clear();
                        thick = 0.0;
                    }
                }
                run.push(b);
                thick = pmax(thick, t);
            }
            if let Some(r) = bar_run(&run, vertical, &ctx, budget) {
                found.push((r, vertical));
                if found.len() >= MAX_BAR_RUNS {
                    break 'orient;
                }
            }
            start = end;
        }
    }
    // a run takes in the boxes stacked on it (along its own orientation, until nothing changes;
    // council L3), and runs that now overlap are one chart
    let mut out: Vec<Bbox> = Vec::new();
    for (mut r, vertical) in found {
        for _ in 0..BAR_STACK_ROUNDS {
            if !budget.spend(filled.len() + 1) {
                return out;
            }
            let before = r;
            for b in &filled {
                let stacked = if vertical {
                    b.0 >= r.0 - 1.0 && b.2 <= r.2 + 1.0 && b.1 <= r.3 + 1.0 && b.3 >= r.1 - 1.0
                } else {
                    b.1 >= r.1 - 1.0 && b.3 <= r.3 + 1.0 && b.0 <= r.2 + 1.0 && b.2 >= r.0 - 1.0
                };
                if stacked {
                    r = union([r, *b]);
                }
            }
            if r == before {
                break;
            }
        }
        if !budget.spend(out.len() + 1) {
            return out;
        }
        if let Some(o) = out.iter_mut().find(|o| overlap(**o, r) > 0.0) {
            *o = union([*o, r]);
        } else {
            out.push(r);
        }
    }
    out
}

struct BarContext<'a> {
    p: &'a Params,
    across: &'a [Line],
    down: &'a [Line],
    words: &'a [(f64, f64)],
}

/// A run of boxes on one baseline as a bar chart's region, or None (see bar_regions). Every
/// comparison is charged: the spread first (cheapest), then words, then the axis.
fn bar_run(run: &[Bbox], vertical: bool, c: &BarContext, budget: &mut Budget) -> Option<Bbox> {
    if (run.len() as f64) < c.p.bar_min_count || !budget.spend(run.len()) {
        return None;
    }
    let lengths = run.iter().map(|b| if vertical { b.3 - b.1 } else { b.2 - b.0 });
    let (lo, hi) = lengths.fold((f64::INFINITY, 0.0f64), |(lo, hi), l| (pmin(lo, l), pmax(hi, l)));
    if !(lo > 0.0 && hi >= c.p.bar_min_spread * lo) {
        return None;
    }
    // bars holding a word's centre: the words in the bar's x-range (binary search), then y
    let limit = (c.p.bar_max_text_share * run.len() as f64).floor() as usize;
    let mut with_text = 0usize;
    for b in run {
        let from = c.words.partition_point(|w| w.0 < b.0);
        let mut held = false;
        for w in &c.words[from..] {
            if !budget.spend(1) {
                return None;
            }
            if w.0 > b.2 {
                break;
            }
            if b.1 <= w.1 && w.1 <= b.3 {
                held = true;
                break;
            }
        }
        if held {
            with_text += 1;
            if with_text > limit {
                return None;
            }
        }
    }
    let r = union(run.iter().copied());
    // the axis: a stroked line along the baseline, within BAR_AXIS_REACH of it, spanning most of the run
    let (lines, at, lo, hi) = if vertical { (c.across, r.3, r.0, r.2) } else { (c.down, r.0, r.1, r.3) };
    let need = BAR_AXIS_SPAN * (hi - lo);
    for l in lines_near(lines, at, BAR_AXIS_REACH) {
        if !budget.spend(1) {
            return None;
        }
        if pmin(l.hi, hi) - pmax(l.lo, lo) >= need {
            return Some(r);
        }
    }
    None
}

/// The rules that touch `b` (1 point of slack), then those touching the box grown by them, at
/// most twice. Rules longer than half the page (frames, column and header rules) and rules
/// that reach past the box both across and down are skipped.
fn grow_by_rules(page: &Page, mut b: Bbox) -> Bbox {
    let mut used = vec![false; page.rules.len()];
    for _ in 0..2 {
        let mut next = b;
        for (k, r) in page.rules.iter().enumerate() {
            if used[k] {
                continue;
            }
            let (rw, rh) = (r.x1 - r.x0, r.y1 - r.y0);
            // a frame, a column rule or a header rule is no part of a figure's own drawing
            if rw > 0.5 * page.width || rh > 0.5 * page.height || !(rw.is_finite() && rh.is_finite()) {
                continue;
            }
            // nor one far longer than the drawing (a separator it happens to touch)
            if pmax(rw, rh) > 2.0 * pmax(b.2 - b.0, b.3 - b.1) {
                continue;
            }
            // one that reaches past the drawing both across and down encloses it or leads away
            // from it (a frame, a shaded panel, the next form box): not the drawing's own
            let across = r.x0 < b.0 - 1.0 || r.x1 > b.2 + 1.0;
            let down = r.y0 < b.1 - 1.0 || r.y1 > b.3 + 1.0;
            if across && down {
                continue;
            }
            if r.x0 <= b.2 + 1.0 && r.x1 >= b.0 - 1.0 && r.y0 <= b.3 + 1.0 && r.y1 >= b.1 - 1.0 {
                used[k] = true;
                next = union([next, (r.x0, r.y0, r.x1, r.y1)]);
            }
        }
        if next == b {
            break;
        }
        b = next;
    }
    b
}

/// The kept lines inside `b` (centre inside), then short lines within `vector_label_gap` body
/// sizes of it (tick labels, axis titles, a legend), twice. Returns the grown box and the
/// lines, in page order.
fn grow_by_labels(page: &Page, drop: &[bool], mut b: Bbox, body: f64, p: &Params) -> (Bbox, Vec<usize>) {
    let mut labels: Vec<usize> = Vec::new();
    let mut taken = vec![false; page.lines.len()];
    let gap = p.vector_label_gap * body;
    for pass in 0..3 {
        let mut next = b;
        for (li, line) in page.lines.iter().enumerate() {
            if taken[li] || drop.get(li).copied().unwrap_or(true) || line.words.is_empty() {
                continue;
            }
            let (cx, cy) = ((line.x0 + line.x1) / 2.0, (line.y0 + line.y1) / 2.0);
            let inside = b.0 <= cx && cx <= b.2 && b.1 <= cy && cy <= b.3;
            let near = line.x0 <= b.2 + gap && line.x1 >= b.0 - gap && line.y0 <= b.3 + gap && line.y1 >= b.1 - gap;
            let short = (line.words.len() as f64) <= p.vector_label_words && line.x1 - line.x0 <= pmax(b.2 - b.0, 0.0);
            if inside || (pass > 0 && near && short) {
                taken[li] = true;
                labels.push(li);
                next = union([next, (line.x0, line.y0, line.x1, line.y1)]);
            }
        }
        b = next;
    }
    labels.sort_unstable();
    (b, labels)
}

fn union(boxes: impl IntoIterator<Item = Bbox>) -> Bbox {
    boxes.into_iter().fold((f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY), |a, b| {
        (pmin(a.0, b.0), pmin(a.1, b.1), pmax(a.2, b.2), pmax(a.3, b.3))
    })
}

fn overlap(a: Bbox, b: Bbox) -> f64 {
    let w = pmin(a.2, b.2) - pmax(a.0, b.0);
    let h = pmin(a.3, b.3) - pmax(a.1, b.1);
    if w > 0.0 && h > 0.0 { w * h } else { 0.0 }
}
