//! Ruled tables drawn as filled cell boxes (parameter `box_cell_tables`).
//!
//! Some producers draw a table as one filled rectangle per cell (usually white, sometimes a
//! light or dark shade) with the borders as thin dark rectangles between them, and often a
//! further same-colour rectangle behind each line of text inside a cell. The row search in
//! [`crate::tables::ruled_tables`] treats every box edge as a row separator, so those inner
//! line boxes cut each multi-line cell into one row per line. Here the cells are read from the
//! boxes themselves: boxes nested in a same-colour box are dropped, a box mostly covered by
//! boxes of another colour is a background and is dropped, and the rest are joined where they
//! touch across a thin border. A joined group whose boxes tile a grid (shared column and row
//! edges, at most a few boxes spanning columns) is a table: every word goes to the cell box
//! holding it, at the cell's top-left grid slot, so the lines of one cell stay in one cell.

use crate::TableFormat;
use crate::layout::Budget;
use crate::model::{Page, Word};
use crate::params::Params;
use crate::prose::append_line;
use crate::pyc::{fcmp, pmax, pmin, strip};
use crate::tables::{RuledTable, Table, keep_filled, line_text, luminance, numbered_list, rows_rtl};

/// Boxes further apart than this (points, or `TOUCH_BODY` body sizes when that is more) do not
/// share a border (a border, or a header's padding strip, lies between cells).
const TOUCH: f64 = 3.0;
const TOUCH_BODY: f64 = 0.6;
/// Edges this close (points) are one grid line.
const EDGE: f64 = 2.5;
/// More candidate boxes than this on a page: the pass is skipped (the other ruled pass runs).
const MAX_BOXES: usize = 2048;
const MAX_ROWS: usize = 200;
const MAX_COLS: usize = 40;
/// Share of a table's boxes that may span more than one column (header group titles); a form
/// whose rows each split the width differently has many more.
const MAX_COLSPAN_SHARE: f64 = 0.25;
/// Share of the grid's slots the boxes must cover (the rest are cells without a fill).
const MIN_COVER: f64 = 0.75;

#[derive(Clone, Copy)]
struct B {
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
    /// index of the box's colour in `colors`
    color: usize,
}

impl B {
    fn area(&self) -> f64 {
        (self.x1 - self.x0) * (self.y1 - self.y0)
    }
    fn contains(&self, o: &B) -> bool {
        self.x0 <= o.x0 + 0.5 && self.y0 <= o.y0 + 0.5 && o.x1 <= self.x1 + 0.5 && o.y1 <= self.y1 + 0.5
    }
}

fn find(parent: &mut [usize], mut i: usize) -> usize {
    while parent[i] != i {
        parent[i] = parent[parent[i]];
        i = parent[i];
    }
    i
}

/// Cluster sorted values whose neighbours are at most `EDGE` apart; returns the first of each.
fn edges(mut v: Vec<f64>) -> Vec<f64> {
    v.sort_by(|a, b| fcmp(*a, *b));
    let mut out: Vec<f64> = Vec::new();
    let mut last = f64::NEG_INFINITY;
    for x in v {
        if x - last > EDGE {
            out.push(x);
        }
        last = x;
    }
    out
}

/// Tables read from cell boxes. Lines they take are marked in `used`. `None` once the budget
/// runs out.
pub(crate) fn box_cell_tables(
    page: &Page,
    ids: &[usize],
    body: f64,
    p: &Params,
    format: TableFormat,
    used: &mut [bool],
    budget: &mut Budget,
) -> Option<Vec<RuledTable>> {
    let mut out = Vec::new();
    if page.rules_truncated == Some(true) {
        return Some(out);
    }
    if !budget.spend(page.rules.len()) {
        return None;
    }
    let mut colors: Vec<&str> = Vec::new();
    let mut boxes: Vec<B> = Vec::new();
    for r in page.rules.iter().filter(|r| r.boxed) {
        let (w, h) = (r.x1 - r.x0, r.y1 - r.y0);
        // a cell holds a line of text; page backgrounds and full-width bands are not cells
        if !(w >= 0.5 * body && h >= 0.8 * body && w < 0.95 * page.width && h < 0.5 * page.height) {
            continue;
        }
        if boxes.len() == MAX_BOXES {
            return Some(out);
        }
        if !budget.spend(r.color.len()) {
            return None;
        }
        let color = match colors.iter().position(|c| *c == r.color.as_str()) {
            Some(c) => c,
            None => {
                colors.push(r.color.as_str());
                colors.len() - 1
            }
        };
        boxes.push(B { x0: r.x0, y0: r.y0, x1: r.x1, y1: r.y1, color });
    }
    if boxes.len() < 4 {
        return Some(out);
    }
    boxes.sort_by(|a, b| fcmp(a.y0, b.y0).then(fcmp(a.x0, b.x0)));
    let n = boxes.len();
    // Containment, in one sweep down the page: a box can only contain boxes that start below
    // its top and end above its bottom. `active` holds the boxes whose bottom is not yet passed.
    // (container, contained) pairs
    let mut pairs: Vec<(usize, usize)> = Vec::new();
    let mut active: Vec<usize> = Vec::new();
    for i in 0..n {
        let b = boxes[i];
        if !budget.spend(active.len() + 1) {
            return None;
        }
        active.retain(|&a| boxes[a].y1 + 0.5 >= b.y0);
        for &a in &active {
            let o = boxes[a];
            if o.contains(&b) {
                pairs.push((a, i));
            } else if b.contains(&o) {
                pairs.push((i, a));
            }
        }
        active.push(i);
        if pairs.len() > 8 * MAX_BOXES {
            // a pile of overlapping boxes is no grid of cells
            return Some(out);
        }
    }
    // a box mostly covered by boxes of another colour is a background behind cells
    let mut other_cover = vec![0.0f64; n];
    for &(a, c) in &pairs {
        if boxes[a].color != boxes[c].color {
            other_cover[a] += boxes[c].area();
        }
    }
    let mut keep: Vec<bool> = (0..n).map(|i| other_cover[i] < 0.5 * boxes[i].area()).collect();
    // then a box inside another kept box is a line's or a mark's background inside a cell (of
    // two equal boxes the first is kept)
    for &(a, c) in &pairs {
        if keep[a] && (boxes[a].area() > boxes[c].area() || a < c) {
            keep[c] = false;
        }
    }
    let cells: Vec<B> = (0..n).filter(|&i| keep[i]).map(|i| boxes[i]).collect();
    let n = cells.len();
    // join boxes that touch across a border
    let touch = pmax(TOUCH, TOUCH_BODY * body);
    let mut parent: Vec<usize> = (0..n).collect();
    let mut active: Vec<usize> = Vec::new();
    for i in 0..n {
        let b = cells[i];
        if !budget.spend(active.len() + 1) {
            return None;
        }
        active.retain(|&a| cells[a].y1 + touch >= b.y0);
        for &a in &active {
            let o = cells[a];
            let xo = pmin(o.x1, b.x1) - pmax(o.x0, b.x0);
            let yo = pmin(o.y1, b.y1) - pmax(o.y0, b.y0);
            let beside = yo > 0.5 * pmin(o.y1 - o.y0, b.y1 - b.y0)
                && ((b.x0 - o.x1).abs() <= touch || (o.x0 - b.x1).abs() <= touch);
            let stacked = xo > 0.5 * pmin(o.x1 - o.x0, b.x1 - b.x0)
                && ((b.y0 - o.y1).abs() <= touch || (o.y0 - b.y1).abs() <= touch);
            if beside || stacked {
                let (ra, rb) = (find(&mut parent, a), find(&mut parent, i));
                parent[ra.max(rb)] = ra.min(rb);
            }
        }
        active.push(i);
    }
    let mut groups: Vec<Vec<usize>> = Vec::new();
    let mut slot = vec![usize::MAX; n];
    for i in 0..n {
        let r = find(&mut parent, i);
        if slot[r] == usize::MAX {
            slot[r] = groups.len();
            groups.push(Vec::new());
        }
        groups[slot[r]].push(i);
    }
    for g in groups {
        if g.len() < 4 {
            continue;
        }
        if let Some(t) = grid_of(page, ids, &cells, &g, body, p, format, used, budget)? {
            out.push(t);
        }
    }
    Some(out)
}

/// The table of one joined group of cell boxes, if they tile a grid that holds its text.
#[allow(clippy::too_many_arguments)]
fn grid_of(
    page: &Page,
    ids: &[usize],
    cells: &[B],
    g: &[usize],
    body: f64,
    p: &Params,
    format: TableFormat,
    used: &mut [bool],
    budget: &mut Budget,
) -> Option<Option<RuledTable>> {
    if !budget.spend(g.len() * 4) {
        return None;
    }
    // A box across the whole width at the top or bottom (a banner, a title, a note under the
    // table) is not a row of the grid: trim those and read the boxes between.
    let mut g: Vec<usize> = g.to_vec();
    let banner;
    g.sort_by(|&a, &b| fcmp(cells[a].y0, cells[b].y0).then(fcmp(cells[a].x0, cells[b].x0)));
    {
        let left = g.iter().map(|&i| cells[i].x0).fold(f64::INFINITY, pmin);
        let right = g.iter().map(|&i| cells[i].x1).fold(f64::NEG_INFINITY, pmax);
        let full = |i: usize| cells[i].x0 <= left + EDGE && cells[i].x1 >= right - EDGE;
        let lo = g.iter().take_while(|&&i| full(i)).count();
        let hi = g.len() - g[lo..].iter().rev().take_while(|&&i| full(i)).count();
        g = g[lo..hi].to_vec();
        banner = lo > 0;
    }
    if g.len() < 4 {
        return Some(None);
    }
    let g = &g[..];
    let xs = edges(g.iter().map(|&i| cells[i].x0).collect());
    let ys = edges(g.iter().map(|&i| cells[i].y0).collect());
    let (nc, nr) = (xs.len(), ys.len());
    if nc < 2 || nr < 2 || nc > MAX_COLS || nr > MAX_ROWS {
        return Some(None);
    }
    let x0 = g.iter().map(|&i| cells[i].x0).fold(f64::INFINITY, pmin);
    let y0 = g.iter().map(|&i| cells[i].y0).fold(f64::INFINITY, pmin);
    let x1 = g.iter().map(|&i| cells[i].x1).fold(f64::NEG_INFINITY, pmax);
    let y1 = g.iter().map(|&i| cells[i].y1).fold(f64::NEG_INFINITY, pmax);
    // the grid slot of a position: the last edge at or before it (with the edge tolerance)
    let col = |x: f64| xs.partition_point(|&e| e <= x + EDGE).saturating_sub(1);
    let row = |y: f64| ys.partition_point(|&e| e <= y + EDGE).saturating_sub(1);
    if !budget.spend(nc * nr) {
        return None;
    }
    let mut owner = vec![usize::MAX; nc * nr];
    // (first row, last row, first column, last column) of each box of the group
    let mut rects: Vec<(usize, usize, usize, usize)> = Vec::with_capacity(g.len());
    let mut spanning = 0usize;
    for (k, &i) in g.iter().enumerate() {
        let b = cells[i];
        let (ca, ra) = (col(b.x0), row(b.y0));
        // the last edge that starts inside the box
        let cb = xs.partition_point(|&e| e < b.x1 - EDGE).saturating_sub(1).max(ca);
        let rb = ys.partition_point(|&e| e < b.y1 - EDGE).saturating_sub(1).max(ra);
        if cb > ca {
            spanning += 1;
        }
        if !budget.spend((rb - ra + 1) * (cb - ca + 1)) {
            return None;
        }
        for r in ra..=rb {
            for c in ca..=cb {
                if owner[r * nc + c] != usize::MAX {
                    // overlapping boxes are no tiling
                    return Some(None);
                }
                owner[r * nc + c] = k;
            }
        }
        rects.push((ra, rb, ca, cb));
    }
    let covered = owner.iter().filter(|&&o| o != usize::MAX).count();
    if (covered as f64) < MIN_COVER * (nc * nr) as f64 || spanning as f64 > MAX_COLSPAN_SHARE * g.len() as f64 {
        return Some(None);
    }
    // An unfilled slot is a cell without a box. It spans the rows of the cell beside it (left,
    // else right) as far as they are unfilled too, so its text lines up with that cell's row.
    for c in 0..nc {
        for r in 0..nr {
            if owner[r * nc + c] != usize::MAX {
                continue;
            }
            let beside = [c.checked_sub(1), (c + 1 < nc).then_some(c + 1)]
                .into_iter()
                .flatten()
                .map(|n| owner[r * nc + n])
                .find(|&o| o != usize::MAX);
            let last = beside.map_or(r, |o| rects[o].1);
            let mut rb = r;
            while rb < last && owner[(rb + 1) * nc + c] == usize::MAX {
                rb += 1;
            }
            if !budget.spend(rb - r + 2) {
                return None;
            }
            for q in r..=rb {
                owner[q * nc + c] = rects.len();
            }
            rects.push((r, rb, c, c));
        }
    }
    // every column edge starts boxes in two rows or more (a grid, not a row of fields)
    let mut starts = vec![0usize; nc];
    for &(_, _, ca, _) in &rects {
        starts[ca] += 1;
    }
    if starts.iter().any(|&s| s < 2) {
        return Some(None);
    }
    // the lines inside: each must lie wholly inside the group's outline
    let lines = &page.lines;
    let mut members: Vec<usize> = Vec::new();
    for &i in ids {
        if !budget.spend(1) {
            return None;
        }
        let l = &lines[i];
        let (mx, my) = ((l.x0 + l.x1) / 2.0, (l.y0 + l.y1) / 2.0);
        if used[i] || !(x0 - 1.0 <= mx && mx <= x1 + 1.0 && y0 - 1.0 <= my && my <= y1 + 1.0) {
            continue;
        }
        if l.x0 < x0 - EDGE || l.x1 > x1 + EDGE {
            // text running out of the boxes: they are not its table
            return Some(None);
        }
        members.push(i);
    }
    if members.len() < 2 {
        return Some(None);
    }
    // (not under a banner box: its text is no header row)
    let head = if banner { None } else { header_band(page, ids, used, (x0, y0, x1), body, p, budget)? };
    let mut ordered = members.clone();
    ordered.sort_by(|&a, &b| fcmp(lines[a].base(), lines[b].base()));
    // per box: its words, one Vec per source line
    let mut texts: Vec<Vec<Vec<Word>>> = vec![Vec::new(); rects.len()];
    for &i in &ordered {
        let l = &lines[i];
        if !budget.spend(l.words.len() * 2 + 1) {
            return None;
        }
        let mut last: Option<usize> = None;
        for w in &l.words {
            let (mx, my) = ((w.x0 + w.x1) / 2.0, (w.y0 + w.y1) / 2.0);
            let k = owner[row(my).min(nr - 1) * nc + col(mx).min(nc - 1)];
            if last != Some(k) || texts[k].is_empty() {
                texts[k].push(Vec::new());
            }
            if let Some(v) = texts[k].last_mut() {
                v.push(w.clone());
            }
            last = Some(k);
        }
    }
    let mut grid: Vec<Vec<String>> = vec![vec![String::new(); nc]; nr];
    let mut cell_text: Vec<String> = Vec::with_capacity(rects.len());
    for (k, parts) in texts.into_iter().enumerate() {
        let t = join_parts(parts, p);
        if !budget.spend(t.len() + 1) {
            return None;
        }
        let (ra, _, ca, _) = rects[k];
        grid[ra][ca] = t.clone();
        cell_text.push(t);
    }
    // a table: two rows or more with two filled cells
    if grid.iter().filter(|r| r.iter().filter(|c| !c.is_empty()).count() >= 2).count() < 2 {
        return Some(None);
    }
    let mut grid: Vec<Vec<String>> = grid.into_iter().filter(|r| r.iter().any(|c| !c.is_empty())).collect();
    // an unfilled header row above the boxes, under the table's top border
    let mut head_row: Option<Vec<String>> = None;
    let mut y0 = y0;
    'head: {
        let Some((top, head_ids)) = head else { break 'head };
        let mut cells: Vec<Vec<Vec<Word>>> = vec![Vec::new(); nc];
        let mut order = head_ids.clone();
        order.sort_by(|&a, &b| fcmp(lines[a].base(), lines[b].base()));
        for &i in &order {
            let l = &lines[i];
            if !budget.spend(l.words.len() * 2 + 1) {
                return None;
            }
            let mut last: Option<usize> = None;
            for (k, w) in l.words.iter().enumerate() {
                let c = col((w.x0 + w.x1) / 2.0).min(nc - 1);
                if let Some(prev) = last
                    && prev != c
                    && w.x0 - l.words[k - 1].x1 < p.cell_gap * pmax(w.size, 1.0)
                {
                    // a line running across the column edges (a title): no header row
                    break 'head;
                }
                if last != Some(c) || cells[c].is_empty() {
                    cells[c].push(Vec::new());
                }
                if let Some(v) = cells[c].last_mut() {
                    v.push(w.clone());
                }
                last = Some(c);
            }
        }
        let row: Vec<String> = cells.into_iter().map(|parts| join_parts(parts, p)).collect();
        if row.iter().filter(|c| !c.is_empty()).count() >= 2 {
            grid.insert(0, row.clone());
            head_row = Some(row);
            members.extend(head_ids);
            y0 = top;
        }
    }
    let cols: Vec<(f64, f64)> = (0..nc).map(|c| (xs[c], if c + 1 < nc { xs[c + 1] } else { x1 })).collect();
    let mut table: Table = keep_filled(grid, &cols, false);
    if table.rows.len() < 2 || table.rows[0].len() < 2 || numbered_list(&table.rows, p) {
        return Some(None);
    }
    if format == TableFormat::Html {
        table.spans = Some(spans_html(head_row.as_deref(), &rects, &cell_text, nr, budget)?);
    }
    for &i in &members {
        used[i] = true;
    }
    Some(Some((members, table, (x0, y0, x1, y1))))
}

/// A cell's lines (its words, one Vec per source line) joined as a paragraph's are.
fn join_parts(parts: Vec<Vec<Word>>, p: &Params) -> String {
    let mut t = String::new();
    for part in parts {
        let line = line_text(part);
        if !line.is_empty() {
            append_line(&mut t, &line, p);
        }
    }
    strip(&t).to_string()
}

/// The band between the table's top border and its first boxes, when it holds a row of
/// unfilled header cells: (border y, the band's lines). The border is a dark horizontal rule
/// (drawn whole or cell by cell) across the boxes' width, between 0.8 and 4 body sizes above
/// them; every line of the band lies inside the boxes' width.
fn header_band(
    page: &Page,
    ids: &[usize],
    used: &[bool],
    (x0, y0, x1): (f64, f64, f64),
    body: f64,
    p: &Params,
    budget: &mut Budget,
) -> Option<Option<(f64, Vec<usize>)>> {
    if !budget.spend(page.rules.len() + ids.len()) {
        return None;
    }
    let (lo, hi) = (y0 - 4.0 * body, y0 - 0.8 * body);
    // (y, x0, x1) of the dark horizontal rules in the window
    let mut segs: Vec<(f64, f64, f64)> = Vec::new();
    for r in &page.rules {
        let (w, h) = (r.x1 - r.x0, r.y1 - r.y0);
        let y = (r.y0 + r.y1) / 2.0;
        if r.boxed || h > 2.0 || w < h || !(lo <= y && y <= hi) || r.x1 < x0 || r.x0 > x1 {
            continue;
        }
        if !budget.spend(r.color.len().saturating_mul(3)) {
            return None;
        }
        if luminance(&r.color) <= p.rule_max_lum {
            segs.push((y, pmax(r.x0, x0), pmin(r.x1, x1)));
        }
    }
    if !budget.spend(segs.len().saturating_mul(2) + 1) {
        return None;
    }
    // nearest first; a border drawn cell by cell is several segments at one y
    segs.sort_by(|a, b| fcmp(b.0, a.0).then(fcmp(a.1, b.1)));
    let mut top = None;
    let mut i = 0;
    while i < segs.len() {
        let y = segs[i].0;
        let mut j = i;
        while j < segs.len() && y - segs[j].0 <= 1.0 {
            j += 1;
        }
        let mut band: Vec<(f64, f64)> = segs[i..j].iter().map(|s| (s.1, s.2)).collect();
        band.sort_by(|a, b| fcmp(a.0, b.0));
        let (mut covered, mut end) = (0.0, f64::NEG_INFINITY);
        for (a, b) in band {
            let a = pmax(a, end);
            if b > a {
                covered += b - a;
            }
            end = pmax(end, b);
        }
        if covered >= 0.9 * (x1 - x0) {
            top = Some(y);
            break;
        }
        i = j;
    }
    let Some(top) = top else { return Some(None) };
    let mut band = Vec::new();
    for &i in ids {
        let l = &page.lines[i];
        let my = (l.y0 + l.y1) / 2.0;
        if used[i] || !(top < my && my < y0) || l.x1 < x0 || l.x0 > x1 {
            continue;
        }
        if l.x0 < x0 - EDGE || l.x1 > x1 + EDGE {
            return Some(None);
        }
        band.push(i);
    }
    Some((!band.is_empty()).then_some((top, band)))
}

/// An HTML table with the boxes' row and column spans (every slot covered), under a header
/// row of single cells when there is one.
fn spans_html(
    head: Option<&[String]>,
    rects: &[(usize, usize, usize, usize)],
    texts: &[String],
    nr: usize,
    budget: &mut Budget,
) -> Option<String> {
    let rtl = rows_rtl(std::iter::once(texts));
    let mut out = if rtl { "<table dir=\"rtl\">\n<tbody>\n".to_string() } else { "<table>\n<tbody>\n".to_string() };
    if let Some(head) = head {
        let mut cells: Vec<&String> = head.iter().collect();
        if rtl {
            cells.reverse();
        }
        out.push_str("<tr>");
        for t in cells {
            if !budget.spend(t.len().saturating_mul(24) + 64) {
                return None;
            }
            out.push_str("<td>");
            out.push_str(&t.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;"));
            out.push_str("</td>");
        }
        out.push_str("</tr>\n");
    }
    let mut order: Vec<usize> = (0..rects.len()).collect();
    order.sort_by_key(|&k| (rects[k].0, rects[k].2));
    let mut at = 0;
    for r in 0..nr {
        let start = at;
        while at < order.len() && rects[order[at]].0 == r {
            at += 1;
        }
        let mut row: Vec<usize> = order[start..at].to_vec();
        if row.is_empty() {
            // every slot of this row belongs to boxes from rows above
            continue;
        }
        if rtl {
            row.reverse();
        }
        out.push_str("<tr>");
        for k in row {
            let (ra, rb, ca, cb) = rects[k];
            if !budget.spend(texts[k].len().saturating_mul(24) + 64) {
                return None;
            }
            out.push_str("<td");
            if rb > ra {
                out.push_str(&format!(" rowspan=\"{}\"", rb - ra + 1));
            }
            if cb > ca {
                out.push_str(&format!(" colspan=\"{}\"", cb - ca + 1));
            }
            out.push('>');
            out.push_str(
                &texts[k].replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;"),
            );
            out.push_str("</td>");
        }
        out.push_str("</tr>\n");
    }
    out.push_str("</tbody>\n</table>");
    Some(out)
}
