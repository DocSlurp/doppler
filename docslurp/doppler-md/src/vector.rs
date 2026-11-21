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
    for (j, (b, _)) in bars.iter().enumerate().take(MAX_FIGURES.saturating_sub(regions.len())) {
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
    include_bar_legends(page, drop, gs.len(), &bars, &mut regions, budget);
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

/// A fill that is never a bar: white (a background). Shared with PageFacts.cc `barFillExcluded`
/// (8-bit channels all 255). Redactions and form fields are told apart by the scale (bar_scale), not
/// by colour: they come in every colour, black bar charts are common (council r2).
fn bar_fill_excluded(color: &str) -> bool {
    match rgb(color) {
        Some((r, g, b)) => r == 255 && g == 255 && b == 255,
        None => true,
    }
}

/// Most numeric words of one page read for scales.
const MAX_SCALE_WORDS: usize = 20000;
/// A scale: at least this many numbers beside the run, increasing along its value axis.
const BAR_SCALE_MIN: usize = 3;
/// How far a scale may sit from the run: across (left of vertical bars, below or above horizontal
/// ones) and along (past the run's ends: the axis runs on past the longest bar).
const BAR_SCALE_ACROSS: f64 = 120.0;
const BAR_SCALE_ALONG: f64 = 40.0;

/// A tick label's number: an optional sign (`-`, U+2212 or `+`), digits with optional thousands
/// commas and one decimal point, an optional `%`; at most 16 characters. Shared with PageFacts.cc
/// `scaleNumber`.
fn scale_number(text: &str) -> Option<f64> {
    if text.chars().nth(16).is_some() {
        return None;
    }
    let t = text.strip_suffix('%').unwrap_or(text);
    let (neg, t) = if let Some(r) = t.strip_prefix('-').or_else(|| t.strip_prefix('\u{2212}')) {
        (true, r)
    } else {
        (false, t.strip_prefix('+').unwrap_or(t))
    };
    let mut digits = String::new();
    let (mut seen_dot, mut prev_digit, mut any) = (false, false, false);
    for c in t.chars() {
        match c {
            '0'..='9' => {
                digits.push(c);
                prev_digit = true;
                any = true;
            }
            ',' if prev_digit && !seen_dot => prev_digit = false,
            '.' if !seen_dot => {
                seen_dot = true;
                prev_digit = false;
                digits.push('.');
            }
            _ => return None,
        }
    }
    if !any || !prev_digit {
        return None;
    }
    let v: f64 = digits.parse().ok()?;
    Some(if neg { -v } else { v })
}

/// A numeric word: its box and value.
#[derive(Clone, Copy)]
struct Num {
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
    v: f64,
}

// Candidates alongside the value axis, with coordinates increasing in the value direction.
fn bar_ticks(r: Bbox, vertical: bool, nums: &[Num], side: usize, budget: &mut Budget) -> Option<Vec<(f64, f64, f64)>> {
    let mut seq: Vec<(f64, f64, f64)> = Vec::new();
    for n in nums {
        if !budget.spend(1) {
            return None;
        }
        let (cx, cy) = ((n.x0 + n.x1) / 2.0, (n.y0 + n.y1) / 2.0);
        let keep = if vertical {
            let along = cy >= r.1 - BAR_SCALE_ALONG && cy <= r.3 + 12.0;
            along
                && if side == 0 {
                    n.x1 <= r.0 + 4.0 && n.x1 >= r.0 - BAR_SCALE_ACROSS
                } else {
                    n.x0 >= r.2 - 4.0 && n.x0 <= r.2 + BAR_SCALE_ACROSS
                }
        } else {
            let along = cx >= r.0 - 12.0 && cx <= r.2 + BAR_SCALE_ALONG;
            along
                && if side == 0 {
                    n.y0 >= r.3 - 4.0 && n.y0 <= r.3 + BAR_SCALE_ACROSS
                } else {
                    n.y1 <= r.1 + 4.0 && n.y1 >= r.1 - BAR_SCALE_ACROSS
                }
        };
        if keep {
            // up is a smaller y on the page
            let cross = if vertical { if side == 0 { n.x1 } else { n.x0 } } else { cy };
            seq.push((if vertical { -cy } else { cx }, n.v, cross));
        }
    }
    Some(seq)
}

/// Whether the run has a scale (council r2 M1/M2: redactions and blank form fields have none): at
/// least BAR_SCALE_MIN numbers beside it whose values increase along the value axis, which for
/// vertical bars is up (tick labels left or right of the plot), for horizontal bars right (below or
/// above it). Every number looked at is charged.
fn bar_scale(r: Bbox, vertical: bool, nums: &[Num], aligned: bool, budget: &mut Budget) -> bool {
    for side in 0..2 {
        let Some(mut seq) = bar_ticks(r, vertical, nums, side, budget) else { return false };
        if seq.len() < BAR_SCALE_MIN {
            continue;
        }
        seq.sort_by(|a, b| a.0.total_cmp(&b.0));
        if aligned {
            // A scale is one typographic row or column. An unrelated nearby
            // number on a different row or column cannot complete or poison it.
            for &(_, _, edge) in &seq {
                let mut previous = None;
                let mut count = 0;
                for &(position, value, cross) in &seq {
                    if !budget.spend(1) {
                        return false;
                    }
                    if cross < edge || cross > edge + 4.0 {
                        continue;
                    }
                    if previous.is_some_and(|(last_position, last_value)| {
                        position <= last_position + 1.0 || value <= last_value
                    }) {
                        count = 0;
                    }
                    previous = Some((position, value));
                    count += 1;
                    if count == BAR_SCALE_MIN {
                        return true;
                    }
                }
            }
            continue;
        }
        if seq.windows(2).all(|w| w[1].1 > w[0].1) {
            return true;
        }
    }
    false
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
/// - few hold a word's centre, unless stable thickness and repeated interior grid crossings
///   support a labeled chart. Long separators with populated lanes on both sides are
///   cell-boundary evidence;
/// - a stroked axis lies along the baseline and spans most of the run (`BAR_AXIS_SPAN`): a box's
///   own outline or a cell border beside one box is not an axis (form fields, council M2);
/// - a scale: at least 3 numbers beside the run increasing along its value axis (bar_scale), which
///   redactions and blank form fields lack;
/// - white fills (backgrounds), boxes covering a quarter of the page and the furniture bands are never
///   bars.
/// A run then takes in the boxes stacked on it, and runs that overlap are one chart. At most
/// MAX_BAR_BOXES boxes and MAX_BAR_RUNS runs a page; every comparison is charged to `budget`, and
/// none is made once it runs out.
fn bar_regions(page: &Page, p: &Params, budget: &mut Budget) -> Vec<(Bbox, Option<BarAxis>)> {
    let page_area = pmax(page.width * page.height, 1.0);
    let band = p.furniture_band * page.height;
    let filled: Vec<Bbox> = page
        .rules
        .iter()
        .filter(|r| r.boxed && !bar_fill_excluded(&r.color))
        .map(|r| (r.x0, r.y0, r.x1, r.y1))
        .filter(|b| {
            let (w, h) = (b.2 - b.0, b.3 - b.1);
            w.is_finite()
                && h.is_finite()
                && w > 0.0
                && h > 0.0
                && w * h < 0.25 * page_area
                && b.3 > band
                && b.1 < page.height - band
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
    // Charge both word passes before allocating the centres or parsing labels.
    let word_count: usize = page.lines.iter().map(|line| line.words.len()).sum();
    if !budget.spend(2 * (word_count + across.len() + down.len()) + 1) {
        return Vec::new();
    }
    // word centres, sorted by x
    let mut words: Vec<(f64, f64)> = page
        .lines
        .iter()
        .flat_map(|l| l.words.iter())
        .map(|w| ((w.x0 + w.x1) / 2.0, (w.y0 + w.y1) / 2.0))
        .filter(|c| c.0.is_finite() && c.1.is_finite())
        .collect();
    words.sort_by(|a, b| a.0.total_cmp(&b.0));
    let nums: Vec<Num> = page
        .lines
        .iter()
        .flat_map(|l| l.words.iter())
        .filter_map(|w| {
            let v = scale_number(&w.text)?;
            (w.x0.is_finite() && w.x1.is_finite() && w.y0.is_finite() && w.y1.is_finite() && v.is_finite())
                .then_some(Num { x0: w.x0, y0: w.y0, x1: w.x1, y1: w.y1, v })
        })
        .take(MAX_SCALE_WORDS)
        .collect();
    let ctx = BarContext { p, across: &across, down: &down, words: &words, nums: &nums };
    let mut found = Vec::new();
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
                        if let Some((r, axis)) = bar_run(&run, vertical, &ctx, budget) {
                            found.push((r, vertical, axis));
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
            if let Some((r, axis)) = bar_run(&run, vertical, &ctx, budget) {
                found.push((r, vertical, axis));
                if found.len() >= MAX_BAR_RUNS {
                    break 'orient;
                }
            }
            start = end;
        }
    }
    // a run takes in the boxes stacked on it (along its own orientation, until nothing changes;
    // council L3), and runs that now overlap are one chart
    let mut out: Vec<(Bbox, Option<BarAxis>)> = Vec::new();
    for (mut r, vertical, axis) in found {
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
        if let Some((box_, saved_axis)) = out.iter_mut().find(|(box_, _)| overlap(*box_, r) > 0.0) {
            *box_ = union([*box_, r]);
            *saved_axis = saved_axis.or(axis);
        } else {
            out.push((r, axis));
        }
    }
    for (box_, axis) in &mut out {
        if let Some(axis) = axis {
            let bars = *box_;
            *box_ = union([bars, axis.ticks]);
            let (rules, tick_lo, tick_hi, lane_lo, lane_hi) = if axis.vertical {
                (&across, axis.ticks.1, axis.ticks.3, bars.0, bars.2)
            } else {
                (&down, axis.ticks.0, axis.ticks.2, bars.1, bars.3)
            };
            // Keep the full painted value grid, including the short margin
            // between the outermost bars and the ends of the grid strokes.
            let from = rules.partition_point(|line| line.at < tick_lo);
            for line in &rules[from..] {
                if !budget.spend(1) || line.at > tick_hi {
                    break;
                }
                if line.lo >= lane_lo - 12.0
                    && line.lo <= lane_lo + 12.0
                    && line.hi >= lane_hi - 12.0
                    && line.hi <= lane_hi + 12.0
                {
                    let stroke = if axis.vertical {
                        (line.lo, line.at, line.hi, line.at)
                    } else {
                        (line.at, line.lo, line.at, line.hi)
                    };
                    *box_ = union([*box_, stroke]);
                }
            }
        }
    }
    out
}

struct BarContext<'a> {
    p: &'a Params,
    across: &'a [Line],
    down: &'a [Line],
    words: &'a [(f64, f64)],
    nums: &'a [Num],
}

// The two tests use different directions. Cell separators run along the value axis in
// repeated gaps between bars. Plot gridlines run across the lanes and cross bar interiors.
// Return None on budget exhaustion so a partial scan cannot approve a candidate.
fn bar_grid_evidence(
    run: &[Bbox],
    r: Bbox,
    vertical: bool,
    c: &BarContext,
    needs_grid: bool,
    has_lane_labels: bool,
    budget: &mut Budget,
) -> Option<bool> {
    let (separators, grid, value_lo, value_hi, lane_lo, lane_hi) =
        if vertical { (c.down, c.across, r.1, r.3, r.0, r.2) } else { (c.across, c.down, r.0, r.2, r.1, r.3) };
    let lane_span = |b: &Bbox| if vertical { (b.0, b.2) } else { (b.1, b.3) };
    let value_span = value_hi - value_lo;
    let mut divided_gaps = 0;
    let (mut divider_lo, mut divider_hi) = (f64::NEG_INFINITY, f64::INFINITY);
    'gaps: for gap in run.windows(2) {
        let (_, lo) = lane_span(&gap[0]);
        let (hi, _) = lane_span(&gap[1]);
        if hi - lo < 2.0 {
            continue;
        }
        let from = separators.partition_point(|line| line.at < lo + 0.5);
        for line in &separators[from..] {
            if !budget.spend(1) {
                return None;
            }
            if line.at > hi - 0.5 {
                break;
            }
            if line.hi.min(value_hi) - line.lo.max(value_lo) >= 0.8 * value_span {
                divider_lo = divider_lo.max(line.lo);
                divider_hi = divider_hi.min(line.hi);
                divided_gaps += 1;
                if divided_gaps == 2 {
                    break 'gaps;
                }
                break;
            }
        }
    }
    let populated_pair = |left: (f64, f64), right: (f64, f64), budget: &mut Budget| -> Option<bool> {
        let mut occupied_lanes = 0;
        for bar in run {
            let (start, end) = lane_span(bar);
            let (x_lo, x_hi) = if vertical { (start, end) } else { (left.0, right.1) };
            let from = c.words.partition_point(|word| word.0 < x_lo);
            let (mut first, mut second) = (false, false);
            for &(x, y) in &c.words[from..] {
                if !budget.spend(1) {
                    return None;
                }
                if x > x_hi {
                    break;
                }
                let (value, lane) = if vertical { (y, x) } else { (x, y) };
                if !(start..=end).contains(&lane) {
                    continue;
                }
                first |= left.0 < value && value < left.1;
                second |= right.0 < value && value < right.1;
                if first && second {
                    occupied_lanes += 1;
                    break;
                }
            }
            if occupied_lanes == 2 {
                return Some(true);
            }
        }
        Some(false)
    };
    if divided_gaps >= 2 {
        if populated_pair((value_lo - 100.0, value_lo - 2.0), (value_hi + 2.0, value_hi + 100.0), budget)? {
            return Some(false);
        }
        // A run in the first or last table column may have no words on one side.
        // The same lane separators must continue across two populated columns,
        // closed by rules spanning the run. A free-standing legend does not qualify.
        for (outer_lo, outer_hi) in [(divider_lo, value_lo), (value_hi, divider_hi)] {
            if outer_hi - outer_lo < 24.0 {
                continue;
            }
            let mut edges = [0.0; 3];
            let mut count = 0;
            let from = grid.partition_point(|line| line.at < outer_lo - 1.0);
            for line in &grid[from..] {
                if !budget.spend(1) {
                    return None;
                }
                if line.at > outer_hi + 1.0 {
                    break;
                }
                if line.hi.min(lane_hi) - line.lo.max(lane_lo) < 0.8 * (lane_hi - lane_lo)
                    || (count > 0 && line.at - edges[count - 1] < 12.0)
                {
                    continue;
                }
                edges[count] = line.at;
                count += 1;
                if count == edges.len() {
                    break;
                }
            }
            if count != edges.len() {
                continue;
            }
            if populated_pair((edges[0], edges[1]), (edges[1], edges[2]), budget)? {
                return Some(false);
            }
        }
    }
    if !needs_grid {
        return Some(true);
    }

    if !budget.spend(run.len()) {
        return None;
    }
    let (mut min_thickness, mut max_thickness) = (f64::INFINITY, 0.0_f64);
    for bar in run {
        let (lo, hi) = lane_span(bar);
        min_thickness = min_thickness.min(hi - lo);
        max_thickness = max_thickness.max(hi - lo);
    }
    let even_thickness = min_thickness >= 0.8 * max_thickness;
    if !even_thickness && (!has_lane_labels || min_thickness < 0.35 * max_thickness) {
        return Some(false);
    }
    let mut previous = f64::NEG_INFINITY;
    let mut crossings = 0;
    let from = grid.partition_point(|line| line.at <= value_lo + 1.0);
    for line in &grid[from..] {
        if !budget.spend(1) {
            return None;
        }
        if line.at >= value_hi - 1.0 {
            break;
        }
        if line.at - previous < 2.0 || line.hi.min(lane_hi) - line.lo.max(lane_lo) < 0.8 * (lane_hi - lane_lo) {
            continue;
        }
        let mut bars_crossed = 0;
        for bar in run {
            if !budget.spend(1) {
                return None;
            }
            let (lo, hi) = if vertical { (bar.1, bar.3) } else { (bar.0, bar.2) };
            let (start, end) = lane_span(bar);
            if lo + 1.0 < line.at && line.at < hi - 1.0 && line.lo <= start + 1.0 && line.hi >= end - 1.0 {
                bars_crossed += 1;
                if bars_crossed == 2 && (!even_thickness || !has_lane_labels || crossings > 0) {
                    break;
                }
            }
        }
        if bars_crossed >= 2 {
            previous = line.at;
            crossings += 1;
        }
        // A solitary rule must cross every lane and the bars must be even.
        if crossings == 2 || (even_thickness && has_lane_labels && bars_crossed == run.len()) {
            return Some(true);
        }
    }
    Some(false)
}

#[derive(Clone, Copy)]
struct BarAxis {
    ticks: Bbox,
    first: f64,
    last: f64,
    vertical: bool,
    side: usize,
}

struct ScaleFit {
    position: f64,
    value: f64,
    cross: f64,
    units_per_point: f64,
    step: f64,
}

// Follow one uninterrupted cadence on the fitted row or column. Distant
// numbers and the next panel's reset scale cannot lengthen this series.
fn bar_axis_extent(fit: ScaleFit, vertical: bool, side: usize, nums: &[Num], budget: &mut Budget) -> Option<BarAxis> {
    const ANCHOR: usize = 16;
    let mut ticks: [Option<Bbox>; 48] = [None; 48];
    for n in nums {
        if !budget.spend(1) {
            return None;
        }
        let (cx, cy) = ((n.x0 + n.x1) / 2.0, (n.y0 + n.y1) / 2.0);
        let position = if vertical { -cy } else { cx };
        let cross = if vertical { if side == 0 { n.x1 } else { n.x0 } } else { cy };
        let step = (n.v - fit.value) / fit.step;
        let slot = step.round() + ANCHOR as f64;
        if (cross - fit.cross).abs() <= 4.0
            && (position - fit.position - (n.v - fit.value) / fit.units_per_point).abs() <= 3.0
            && (step.round() - step).abs() < 0.03
            && (0.0..ticks.len() as f64).contains(&slot)
        {
            ticks[slot as usize].get_or_insert((n.x0, n.y0, n.x1, n.y1));
        }
    }
    ticks[ANCHOR]?;
    let mut first = ANCHOR;
    let mut last = ANCHOR;
    while first > 0 && ticks[first - 1].is_some() {
        first -= 1;
    }
    while last + 1 < ticks.len() && ticks[last + 1].is_some() {
        last += 1;
    }
    Some(BarAxis {
        ticks: union(ticks[first..=last].iter().flatten().copied()),
        first: fit.value + (first as f64 - ANCHOR as f64) * fit.step,
        last: fit.value + (last as f64 - ANCHOR as f64) * fit.step,
        vertical,
        side,
    })
}

// A scale fit and printed values are independent evidence: the words used as
// ticks are outside the bar lanes, and endpoint words lie within their lanes.
fn bar_endpoint_values(run: &[Bbox], r: Bbox, vertical: bool, nums: &[Num], budget: &mut Budget) -> Option<BarAxis> {
    // Runs arrive in lane order. Assign each numeric word to at most one lane.
    let lane_start = |b: &Bbox| if vertical { b.0 } else { b.1 };
    let lane_end = |b: &Bbox| if vertical { b.2 } else { b.3 };
    let tip_at = |b: &Bbox| if vertical { -b.1 } else { b.2 };
    if !budget.spend(run.len()) {
        return None;
    }
    let thickness = run.iter().map(|b| lane_end(b) - lane_start(b));
    let (lo, hi) = thickness.fold((f64::INFINITY, 0.0_f64), |(lo, hi), t| (lo.min(t), hi.max(t)));
    if lo < 0.8 * hi {
        return None;
    }
    let mut closest: Vec<Option<(f64, f64)>> = vec![None; run.len()];
    for n in nums {
        if !budget.spend(1) {
            return None;
        }
        let (cx, cy) = ((n.x0 + n.x1) / 2.0, (n.y0 + n.y1) / 2.0);
        let (position, lane) = if vertical { (-cy, cx) } else { (cx, cy) };
        let i = run.partition_point(|b| lane_start(b) <= lane);
        if i == 0 || lane > lane_end(&run[i - 1]) {
            continue;
        }
        let b = &run[i - 1];
        let length = if vertical { b.3 - b.1 } else { b.2 - b.0 };
        let offset = position - tip_at(b);
        if offset >= -30.0_f64.min(0.6 * length)
            && offset <= 18.0
            && closest[i - 1].is_none_or(|(distance, _)| offset.abs() < distance)
        {
            closest[i - 1] = Some((offset.abs(), n.v));
        }
    }
    let labels: Vec<_> = run.iter().zip(closest).filter_map(|(b, label)| Some((tip_at(b), label?.1))).collect();
    let required = (2 * run.len()).div_ceil(3).max(3);
    if labels.len() < required {
        return None;
    }
    // Each line is determined by two ticks and needs a third independent tick
    // on the same row or column. Consider only small local tick populations.
    for side in 0..2 {
        let ticks = bar_ticks(r, vertical, nums, side, budget)?;
        if !(3..=24).contains(&ticks.len()) {
            continue;
        }
        for &(p0, v0, c0) in &ticks {
            for &(p1, v1, c1) in &ticks {
                if !budget.spend(1) {
                    return None;
                }
                if p1 - p0 < 20.0 || v1 <= v0 || (c1 - c0).abs() > 4.0 {
                    continue;
                }
                let units_per_point = (v1 - v0) / (p1 - p0);
                let position_at = |value| p0 + (value - v0) / units_per_point;
                let mut third = None;
                for &(p, v, cross) in &ticks {
                    if !budget.spend(1) {
                        return None;
                    }
                    if (p - p0).abs() > 5.0
                        && (p - p1).abs() > 5.0
                        && (cross - c0).abs() <= 4.0
                        && (position_at(v) - p).abs() <= 3.0
                    {
                        third = Some((p, v));
                        break;
                    }
                }
                let Some((p2, v2)) = third else { continue };
                let step = (v1 - v0).abs().min((v2 - v0).abs()).min((v2 - v1).abs());
                let mut matching = 0;
                for &(tip, value) in &labels {
                    if !budget.spend(1) {
                        return None;
                    }
                    matching += usize::from((position_at(value) - tip).abs() <= 4.0);
                }
                if matching >= required {
                    // Use the widest pair among all three corroborating ticks
                    // to limit drift from short or unequal-width tick labels.
                    let (start, end) = if p2 < p0 {
                        ((p2, v2), (p1, v1))
                    } else if p2 > p1 {
                        ((p0, v0), (p2, v2))
                    } else {
                        ((p0, v0), (p1, v1))
                    };
                    let units_per_point = (end.1 - start.1) / (end.0 - start.0);
                    return bar_axis_extent(
                        ScaleFit { position: start.0, value: start.1, cross: c0, units_per_point, step },
                        vertical,
                        side,
                        nums,
                        budget,
                    );
                }
            }
        }
    }
    None
}

/// A run of boxes on one baseline as a bar chart's region, or None (see bar_regions). Every
/// comparison is charged: the spread first (cheapest), then words, then the axis.
fn bar_run(run: &[Bbox], vertical: bool, c: &BarContext, budget: &mut Budget) -> Option<(Bbox, Option<BarAxis>)> {
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
        with_text += usize::from(held);
    }
    let r = union(run.iter().copied());
    // the axis: a stroked line along the baseline, within BAR_AXIS_REACH of it, spanning most of the run
    let (lines, at, lo, hi) = if vertical { (c.across, r.3, r.0, r.2) } else { (c.down, r.0, r.1, r.3) };
    let need = BAR_AXIS_SPAN * (hi - lo);
    let mut has_axis = false;
    for l in lines_near(lines, at, BAR_AXIS_REACH) {
        if !budget.spend(1) {
            return None;
        }
        if pmin(l.hi, hi) - pmax(l.lo, lo) >= need {
            has_axis = true;
            break;
        }
    }
    let labeled = with_text > limit;
    if has_axis
        && (!labeled || bar_grid_evidence(run, r, vertical, c, true, with_text >= run.len() - run.len() / 4, budget)?)
        && bar_scale(r, vertical, c.nums, labeled, budget)
        && (labeled || bar_grid_evidence(run, r, vertical, c, false, false, budget)?)
    {
        return Some((r, None));
    }
    if !has_axis {
        return None;
    }
    let axis = bar_endpoint_values(run, r, vertical, c.nums, budget)?;
    bar_grid_evidence(run, r, vertical, c, false, false, budget)?.then_some((r, Some(axis)))
}

// Charge word coordinates and text separately before scanning either.
fn has_legend_letters(
    line: &crate::model::Line,
    contains_x: impl Fn(f64) -> bool,
    budget: &mut Budget,
) -> Option<bool> {
    if !budget.spend(line.words.len()) {
        return None;
    }
    for word in &line.words {
        if contains_x(word.x0) {
            if !budget.scan(word.text.len()) {
                return None;
            }
            if word.text.chars().any(char::is_alphabetic) {
                return Some(true);
            }
        }
    }
    Some(false)
}

// Attach a swatch-backed legend below a horizontal endpoint chart. Two
// adjacent panels with the same titled scale and one legend belong to one
// rectangular crop, while their detection regions remain separate.
fn include_bar_legends(
    page: &Page,
    drop: &[bool],
    graphic_count: usize,
    bars: &[(Bbox, Option<BarAxis>)],
    regions: &mut [(usize, usize, Bbox, Vec<usize>)],
    budget: &mut Budget,
) {
    let mut attachments = Vec::new();
    for (i, region) in regions.iter().enumerate() {
        let Some(axis) = region.1.checked_sub(graphic_count).and_then(|j| bars.get(j)).and_then(|b| b.1) else {
            continue;
        };
        if axis.vertical
            || axis.side != 0
            || axis.last <= axis.first
            || !budget.spend(2 * page.lines.len() + page.images.len() + page.rules.len())
        {
            continue;
        }
        let t = axis.ticks;
        let height = t.3 - t.1;
        let kept = |li: usize| !drop.get(li).copied().unwrap_or(true);
        let mut title: Option<&crate::model::Line> = None;
        for (li, line) in page.lines.iter().enumerate() {
            if kept(li)
                && line.y0 >= t.3 - 1.0
                && line.y0 <= t.3 + height
                && line.x0 >= t.0
                && line.x1 <= t.2
                && title.is_none_or(|prior| line.y0 < prior.y0)
            {
                match has_legend_letters(line, |_| true, budget) {
                    Some(true) => title = Some(line),
                    Some(false) => {}
                    None => return,
                }
            }
        }
        let Some(title) = title else { continue };
        // Accept small raster or vector swatches immediately left of one short
        // line below the title. The swatch contributes its full painted bounds.
        let marks = page.images.iter().filter(|im| !im.mask).map(|im| (im.x0, im.y0, im.x1, im.y1)).chain(
            page.rules.iter().filter(|r| r.boxed && !bar_fill_excluded(&r.color)).map(|r| (r.x0, r.y0, r.x1, r.y1)),
        );
        let mut legend = None;
        for mark in marks {
            let w = mark.2 - mark.0;
            let h = mark.3 - mark.1;
            if !(0.35 * height..=2.0 * height).contains(&w)
                || !(0.35 * height..=2.0 * height).contains(&h)
                || mark.0 < region.2.0 - 2.0 * height
                || mark.2 >= t.2
                || mark.1 <= title.y1
                || mark.1 > title.y1 + 2.5 * height
            {
                continue;
            }
            if !budget.spend(page.lines.len()) {
                return;
            }
            for (li, line) in page.lines.iter().enumerate() {
                if kept(li) && line.x0 >= mark.0 - 1.0 && line.x1 <= t.2 && line.y0 < mark.3 && line.y1 > mark.1 {
                    match has_legend_letters(line, |x| x >= mark.2 && x <= mark.2 + 2.0 * height, budget) {
                        Some(true) => {
                            let bounds = union([mark, (line.x0, line.y0, line.x1, line.y1)]);
                            legend = Some(legend.map_or(bounds, |old| union([old, bounds])));
                            break;
                        }
                        Some(false) => {}
                        None => return,
                    }
                }
            }
        }
        if !budget.spend(title.words.len()) || !budget.scan(title.words.iter().map(|w| w.text.len()).sum()) {
            return;
        }
        attachments.push((i, axis, title.text(), legend));
    }
    for &(i, _, _, legend) in &attachments {
        if let Some(b) = legend {
            regions[i].2 = union([regions[i].2, b]);
        }
    }
    if !budget.spend(attachments.len() * regions.len() * attachments.len().max(1)) {
        return;
    }
    for pair in attachments.windows(2) {
        let [(i, a, title_a, legend_a), (j, b, title_b, legend_b)] = pair else { continue };
        if legend_a.is_some() == legend_b.is_some() || title_a != title_b || a.first != b.first || a.last != b.last {
            continue;
        }
        let left = regions[*i].2;
        let right = regions[*j].2;
        let (gap, span) =
            (left.0.max(right.0) - left.2.min(right.2), (a.ticks.2 - a.ticks.0).min(b.ticks.2 - b.ticks.0));
        if left.2.min(right.2) > left.0.max(right.0)
            || gap > 0.5 * span
            || (a.ticks.3 - b.ticks.3).abs() > (a.ticks.3 - a.ticks.1)
            || (left.1 - right.1).abs() > 3.0 * (a.ticks.3 - a.ticks.1)
        {
            continue;
        }
        let mut combined = union([left, right]);
        if let Some(legend) = legend_a.or(*legend_b) {
            // Complete any note that shares the legend's row rather than
            // slicing its descenders or the end of its last word.
            let h = legend.3 - legend.1;
            if !budget.spend(page.lines.len()) {
                return;
            }
            let row = page.lines.iter().enumerate().filter_map(|(li, line)| {
                (!drop.get(li).copied().unwrap_or(true)
                    && line.y0 >= legend.1 - 0.5 * h
                    && line.y1 <= legend.3 + 0.5 * h
                    && line.x0 >= combined.0
                    && line.x0 < combined.2
                    && line.x1 <= combined.2 + 3.0 * h)
                    .then_some((line.x0, line.y0, line.x1, line.y1))
            });
            combined = union(std::iter::once(combined).chain(row));
        }
        let padding = 2.0;
        combined = (
            (combined.0 - padding).max(0.0),
            (combined.1 - padding).max(0.0),
            (combined.2 + padding).min(page.width),
            (combined.3 + padding).min(page.height),
        );
        if regions.iter().enumerate().any(|(k, r)| k != *i && k != *j && overlap(combined, r.2) > 0.0) {
            continue;
        }
        regions[*i].2 = combined;
    }
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
