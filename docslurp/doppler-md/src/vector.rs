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
//! the position of its first `<graphic>` in the producer's list for the page (one-based).

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
    if gs.is_empty() {
        return (Vec::new(), Vec::new(), Vec::new());
    }
    let boxes: Vec<Bbox> = gs.iter().map(|g| (g.x0, g.y0, g.x1, g.y1)).collect();
    let all: Vec<usize> = (0..gs.len()).collect();
    let groups = clusters(&boxes, &all, pmax(p.vector_group_gap * body, 2.0));
    let band = p.furniture_band * page.height;
    let page_area = pmax(page.width * page.height, 1.0);
    let mut notes: Vec<Note> = Vec::with_capacity(groups.len());
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
            text: true,
            file: None,
            degraded: false,
            link: None,
            repeat_of: None,
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
