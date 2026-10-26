//! Gutter tables and ruled tables (the `tables` section of doppler_md.py).

use crate::layout::near_any;
use crate::model::{Line, Page, Word};
use crate::params::Params;
use crate::pyc::{self, PyRe, char_len, fcmp, median, pmax, pmin, round0, round1, split_count, strip};
use std::rc::Rc;

pub static NUMBERED: PyRe = PyRe::new(|c| {
    let (d, s) = (&c.d, &c.s);
    format!(
        r"\A(?:\(?[{d}]{{1,3}}(?:\.[{d}]{{1,3}}){{0,3}}[.)]|[{d}]{{1,3}}(?:\.[{d}]{{1,3}}){{1,3}}|\(?[a-z]\)|\([A-Za-z]\)|\(?[ivx]{{1,4}}\)|[IVX]{{1,4}}\.|§[{s}]?[{d}]+)\z"
    )
});

/// `NUMBERED_RE.match(s)`
pub fn numbered(s: &str) -> bool {
    NUMBERED.py_match(s)
}

pub fn is_bullet_char(c: char) -> bool {
    matches!(
        c,
        '•' | '●'
            | '▪'
            | '■'
            | '◦'
            | '‣'
            | '⁃'
            | '∙'
            | '·'
            | '–'
            | '-'
            | '*'
            | '➢'
            | '✓'
            | '✔'
            | '►'
            | '▸'
    )
}

/// `BULLET_RE.match(s)`: one bullet character (optionally followed by a final newline).
pub fn bullet(s: &str) -> bool {
    let s = s.strip_suffix('\n').unwrap_or(s);
    let mut it = s.chars();
    matches!((it.next(), it.next()), (Some(c), None) if is_bullet_char(c))
}

static NUMERIC: PyRe = PyRe::new(|c| {
    let (d, s) = (&c.d, &c.s);
    format!(
        r"\A(?:[{s}(+\-\u{{2212}}\u{{2013}}]*[$\u{{20ac}}\u{{00a3}}\u{{00a5}}\u{{20a9}}\u{{20b9}}]?[{s}]*[{d}][{d}{s}.,'\u{{2019}}\u{{202f}}\u{{00a0}}]*(?:[.,][{d}]+)?[{s}]*(?:%|\u{{20ac}}|\$|[A-Z]{{3}}|k|m|bn|x)?[){s}]*\z|[{d}]{{1,4}}[/.\-][{d}]{{1,2}}(?:[/.\-][{d}]{{1,4}})?\z|[\u{{2013}}\u{{2014}}\-]\z)"
    )
});

/// `bool(NUMERIC_RE.match(text.strip()))`
pub fn numeric(text: &str) -> bool {
    NUMERIC.py_match(strip(text))
}

/// A visual row: lines sharing a baseline.
#[derive(Clone, Debug)]
pub struct Row {
    pub base: f64,
    pub ids: Vec<usize>,
    /// a line that does not split is its own (shared) cell
    pub cells: Vec<Rc<Line>>,
    pub size: f64,
    pub multi: bool,
}

/// Whether the gap after word `a` (before `b`) splits a line into cells.
fn cell_break(a: &Word, b: &Word, p: &Params) -> bool {
    let sz = if a.size != 0.0 { a.size } else { a.y1 - a.y0 };
    b.x0 - a.x1 > p.cell_gap * pmax(sz, 1.0)
}

/// `len(segments(line))` without building the cells.
pub fn segment_count(line: &Line, p: &Params) -> usize {
    1 + line.words.windows(2).filter(|w| cell_break(&w[0], &w[1], p)).count()
}

/// [`segments`] sharing the line itself when it has no cell gap (the common case): the cell
/// Python builds from all of a line's words equals the line.
pub fn segments_rc(line: &Rc<Line>, p: &Params) -> Vec<Rc<Line>> {
    if line.words.windows(2).all(|w| !cell_break(&w[0], &w[1], p)) {
        return vec![line.clone()];
    }
    segments(line, p).into_iter().map(Rc::new).collect()
}

/// Split a line into cells at gaps wider than cell_gap x font size.
pub fn segments(line: &Line, p: &Params) -> Vec<Line> {
    let mut out: Vec<Vec<Word>> = Vec::new();
    let mut cur: Vec<Word> = line.words.first().cloned().into_iter().collect();
    for w in line.words.windows(2) {
        let (a, b) = (&w[0], &w[1]);
        if cell_break(a, b, p) {
            out.push(std::mem::take(&mut cur));
        }
        cur.push(b.clone());
    }
    out.push(cur);
    out.into_iter().map(|ws| Line::new(ws, line.block, line.flow)).collect()
}

/// Group lines (by id) into visual rows sharing a baseline, sorted top to bottom.
pub fn visual_rows(page: &Page, ids: &[usize], p: &Params) -> Vec<Row> {
    let lines = &page.lines;
    let mut sorted: Vec<usize> = ids.to_vec();
    sorted.sort_by(|&a, &b| fcmp(lines[a].base(), lines[b].base()));
    let mut groups: Vec<(f64, Vec<usize>)> = Vec::new();
    for i in sorted {
        let line = &lines[i];
        match groups.last_mut() {
            Some(g) if (g.0 - line.base()).abs() <= p.row_tol * pmax(line.size(), 1.0) => g.1.push(i),
            _ => groups.push((line.base(), vec![i])),
        }
    }
    groups.into_iter().map(|(base, ids)| make_row(lines, base, ids, p)).collect()
}

/// One visual row from its lines in baseline order.
fn make_row(lines: &[Rc<Line>], base: f64, mut ids: Vec<usize>, p: &Params) -> Row {
    ids.sort_by(|&a, &b| fcmp(lines[a].x0, lines[b].x0));
    let cells: Vec<Rc<Line>> = ids.iter().flat_map(|&i| segments_rc(&lines[i], p)).collect();
    let size = median(cells.iter().map(|c| c.size()).collect());
    let long = cells.iter().filter(|c| c.words.len() as f64 >= p.long_cell_words).count();
    let multi = cells.len() >= 2 && long < 2;
    Row { base, ids, cells, size, multi }
}

/// Positions a [`RowWindow`] walks over non-kept lines or along a baseline chain before it
/// gives up and the caller builds every row instead.
const ROW_WINDOW_STEPS: usize = 256;

/// `visual_rows` of the kept lines of `order` (line ids already stably sorted by baseline, all
/// finite), built only around the baselines a caller asks for.
///
/// visual_rows starts a row at a line whose baseline is more than its tolerance from the row's
/// first line. Baselines ascend, so a line further than its tolerance from the previous kept line
/// starts a row whatever came before: grouping can restart there and gives the same rows.
struct RowWindow<'a, F: Fn(usize) -> bool> {
    lines: &'a [Rc<Line>],
    order: &'a [usize],
    keep: F,
    p: &'a Params,
}

impl<F: Fn(usize) -> bool> RowWindow<'_, F> {
    fn tol(&self, i: usize) -> f64 {
        self.p.row_tol * pmax(self.lines[i].size(), 1.0)
    }

    /// The kept position at or before `pos`, then back to the position that starts its row;
    /// None when the walk is too long.
    fn row_start_at_or_before(&self, pos: usize) -> Option<Option<usize>> {
        let mut steps = 0;
        let mut k = pos;
        loop {
            if (self.keep)(self.order[k]) {
                break;
            }
            steps += 1;
            if k == 0 {
                return Some(None);
            }
            if steps > ROW_WINDOW_STEPS {
                return None;
            }
            k -= 1;
        }
        // k is kept: walk back until a kept line far enough from its kept predecessor
        loop {
            let mut j = k;
            let prev = loop {
                if j == 0 {
                    break None;
                }
                j -= 1;
                steps += 1;
                if steps > ROW_WINDOW_STEPS {
                    return None;
                }
                if (self.keep)(self.order[j]) {
                    break Some(j);
                }
            };
            let Some(j) = prev else { return Some(Some(k)) };
            let (bk, bj) = (self.lines[self.order[k]].base(), self.lines[self.order[j]].base());
            if bk - bj > self.tol(self.order[k]) {
                return Some(Some(k));
            }
            k = j;
        }
    }

    /// The next row starting at the first kept position at or after `*pos` (which must start a
    /// row, or be followed only by non-kept lines before one).
    fn next_row(&self, pos: &mut usize) -> Option<Row> {
        let order = self.order;
        while *pos < order.len() && !(self.keep)(order[*pos]) {
            *pos += 1;
        }
        let &first = order.get(*pos)?;
        *pos += 1;
        let base = self.lines[first].base();
        let mut ids = vec![first];
        while let Some(&k) = order.get(*pos) {
            if (self.keep)(k) {
                if (base - self.lines[k].base()).abs() > self.tol(k) {
                    break;
                }
                ids.push(k);
            }
            *pos += 1;
        }
        Some(make_row(self.lines, base, ids, self.p))
    }
}

/// Python's `int(x)` (truncation toward zero). Clamped to +-1e30 so the coverage arithmetic
/// below cannot overflow; Python would need a list of more than 1e30 slots (MemoryError) long
/// before such a coordinate could change its output.
fn trunc(x: f64) -> i128 {
    x.clamp(-1e30, 1e30).trunc() as i128
}

/// Column intervals separated by x gaps that (almost) no multi-cell row covers with a word.
///
/// Python fills a coverage array one slot per integer x between the leftmost and rightmost word
/// and scans it slot by slot. This computes the same array as run lengths from interval events,
/// so memory and time follow the number of words rather than the coordinate range (a hostile
/// xMax=1e9 cannot allocate gigabytes); within a run of equal coverage every slot after the first
/// is a no-op in Python's scan, so scanning runs gives identical columns.
pub fn gutter_columns(rows: &[Row], p: &Params) -> Vec<(f64, f64)> {
    let multi: Vec<&Row> = rows.iter().filter(|r| r.multi).collect();
    let words: Vec<&Word> = multi.iter().flat_map(|r| r.cells.iter().flat_map(|c| c.words.iter())).collect();
    let Some(first) = words.first() else {
        return Vec::new(); // Python: min() of an empty sequence raises
    };
    let (mut minx, mut maxx) = (first.x0, first.x1);
    for w in &words[1..] {
        minx = pmin(minx, w.x0);
        maxx = pmax(maxx, w.x1);
    }
    let lo = trunc(minx);
    let hi = trunc(maxx).saturating_add(2);
    let len = hi.saturating_sub(lo).saturating_add(1).max(0);
    // coverage events: +1 at the start of each row's covered run, -1 after its end
    let mut events: Vec<(i128, i64)> = Vec::new();
    for r in &multi {
        let mut iv: Vec<(i128, i128)> = r
            .cells
            .iter()
            .flat_map(|c| c.words.iter())
            .map(|w| (trunc(w.x0) - lo, trunc(w.x1) - lo))
            .filter(|(a, b)| b >= a)
            .collect();
        iv.sort();
        let mut cur: Option<(i128, i128)> = None;
        for (a, b) in iv {
            match cur {
                Some((ca, cb)) if a <= cb.saturating_add(1) => cur = Some((ca, cb.max(b))),
                Some(c) => {
                    events.push((c.0, 1));
                    events.push((c.1 + 1, -1));
                    cur = Some((a, b));
                }
                None => cur = Some((a, b)),
            }
        }
        if let Some(c) = cur {
            events.push((c.0, 1));
            events.push((c.1 + 1, -1));
        }
    }
    events.sort();
    let allowed = p.gutter_tol * multi.len() as f64;
    let size = median(multi.iter().map(|r| r.size).collect());
    // each row's word starts and ends, sorted once, built on the first narrow gap
    let mut edges: Option<Vec<(Vec<f64>, Vec<f64>)>> = None;
    let mut aligned = |a: i128, b: i128| {
        // narrow gap: most rows have a word starting right after it or ending right before it
        let (fa, fb) = ((a + lo) as f64, (b + lo) as f64);
        let edges = edges.get_or_insert_with(|| {
            multi
                .iter()
                .map(|r| {
                    let ws = r.cells.iter().flat_map(|c| c.words.iter());
                    let mut x0s: Vec<f64> = ws.clone().map(|w| w.x0).collect();
                    let mut x1s: Vec<f64> = ws.map(|w| w.x1).collect();
                    x0s.sort_by(|a, b| fcmp(*a, *b));
                    x1s.sort_by(|a, b| fcmp(*a, *b));
                    (x0s, x1s)
                })
                .collect()
        });
        let hits = edges.iter().filter(|(x0s, x1s)| near_any(x0s, fb, 1.5) || near_any(x1s, fa, 1.5)).count();
        hits as f64 >= p.narrow_align * multi.len() as f64
    };
    let mut cols: Vec<(i128, i128)> = Vec::new();
    let (mut start, mut gap_start): (Option<i128>, Option<i128>) = (None, None);
    let mut n: i64 = 0;
    let mut k = 0;
    let mut x: i128 = 0;
    while x < len {
        while k < events.len() && events[k].0 <= x {
            n += events[k].1;
            k += 1;
        }
        let next = if k < events.len() { events[k].0.min(len) } else { len };
        if n as f64 > allowed {
            match (start, gap_start) {
                (None, _) => start = Some(x),
                (Some(s), Some(g)) => {
                    let width = (x - g) as f64;
                    if width >= p.min_gutter * size || (width >= p.narrow_gutter * size - 1.0 && aligned(g - 1, x)) {
                        cols.push((s, g - 1));
                        start = Some(x);
                    }
                }
                _ => {}
            }
            gap_start = None;
        } else if start.is_some() && gap_start.is_none() {
            gap_start = Some(x);
        }
        x = next;
    }
    if let Some(s) = start {
        cols.push((s, gap_start.unwrap_or(len) - 1));
    }
    cols.into_iter().map(|(a, b)| ((a + lo) as f64, (b + lo) as f64)).collect()
}

fn line_text(ws: Vec<Word>) -> String {
    if ws.is_empty() {
        return String::new();
    }
    strip(Line::new(ws, 0, 0).text()).to_string()
}

pub type Grid = Vec<Vec<String>>;

/// (member line ids, grid, bbox x0 y0 x1 y1)
pub type RuledTable = (Vec<usize>, Grid, (f64, f64, f64, f64));

fn keep_filled(grid: Grid, ncols: usize) -> Grid {
    let keep: Vec<usize> = (0..ncols).filter(|&i| grid.iter().any(|row| !row[i].is_empty())).collect();
    grid.into_iter().map(|row| keep.iter().map(|&i| row[i].clone()).collect()).collect()
}

pub fn build_grid(rows: &[Row], cols: &[(f64, f64)], p: &Params) -> Grid {
    if cols.is_empty() {
        return Vec::new();
    }
    let column = |w: &Word| {
        let mid = (w.x0 + w.x1) / 2.0;
        if let Some(k) = cols.iter().position(|&(a, b)| a - 1.0 <= mid && mid <= b + 1.0) {
            return k;
        }
        let mut best = 0;
        for i in 1..cols.len() {
            let d = |i: usize| ((cols[i].0 + cols[i].1) / 2.0 - mid).abs();
            if d(i) < d(best) {
                best = i;
            }
        }
        best
    };
    let multi: Vec<&Row> = rows.iter().filter(|r| r.multi).collect();
    let pitches: Vec<f64> = multi.windows(2).map(|w| w[1].base - w[0].base).collect();
    let pitch = if pitches.is_empty() { None } else { Some(median(pitches)) };
    let mut grid: Grid = Vec::new();
    let mut prev: Option<&Row> = None;
    for r in rows {
        let mut cells: Vec<Vec<Word>> = vec![Vec::new(); cols.len()];
        for c in &r.cells {
            for w in &c.words {
                cells[column(w)].push(w.clone());
            }
        }
        let texts: Vec<String> = cells.into_iter().map(line_text).collect();
        let filled = texts.iter().filter(|x| !x.is_empty()).count();
        if let (Some(last), Some(pr)) = (grid.last_mut(), prev) {
            let close = pitch.is_some_and(|pt| pt != 0.0 && r.base - pr.base < p.wrap_pitch * pt);
            let fewer = filled < last.iter().filter(|x| !x.is_empty()).count();
            if (close && fewer) || (filled == 1 && texts[0].is_empty() && !r.multi) {
                for (k, t) in texts.iter().enumerate() {
                    if !t.is_empty() {
                        last[k] = strip(&format!("{} {}", last[k], t)).to_string(); // wrapped cell continuation
                    }
                }
                prev = Some(r);
                continue;
            }
        }
        grid.push(texts);
        prev = Some(r);
    }
    keep_filled(grid, cols.len())
}

/// Two columns: list numbers on the left, prose on the right.
pub fn numbered_list(grid: &Grid, p: &Params) -> bool {
    if grid.first().is_none_or(|r| r.len() != 2) {
        return false;
    }
    let keys: Vec<&String> = grid.iter().map(|r| &r[0]).filter(|k| !k.is_empty()).collect();
    let vals: Vec<&String> = grid.iter().map(|r| &r[1]).filter(|v| !v.is_empty()).collect();
    !keys.is_empty()
        && keys.iter().filter(|k| numbered(k)).count() as f64 >= 0.8 * keys.len() as f64
        && median(vals.iter().map(|v| split_count(v) as f64).collect()) >= p.list_value_words
}

pub fn plausible(grid: &Grid, p: &Params) -> bool {
    if grid.len() < 2 || grid[0].len() < 2 || numbered_list(grid, p) {
        return false;
    }
    let full = grid.iter().filter(|row| row.iter().filter(|x| !x.is_empty()).count() >= 2).count();
    if (full as f64) < p.table_min_rows {
        return false;
    }
    if grid[0].len() == 2 {
        let body = &grid[1..];
        for k in 0..2 {
            let vals: Vec<&String> = body.iter().map(|r| &r[k]).filter(|v| !v.is_empty()).collect();
            if !vals.is_empty()
                && vals.iter().filter(|v| numeric(v)).count() as f64 / vals.len() as f64 >= p.two_col_numeric
            {
                return true;
            }
        }
        // key/value table: short labels on the left, values that are not long prose on the right
        let keys: Vec<&String> = grid.iter().map(|r| &r[0]).filter(|k| !k.is_empty()).collect();
        let vals: Vec<&String> = grid.iter().map(|r| &r[1]).filter(|v| !v.is_empty()).collect();
        return keys.len() as f64 >= p.kv_min_rows
            && median(keys.iter().map(|k| char_len(k) as f64).collect()) <= p.kv_key_chars
            && median(vals.iter().map(|v| split_count(v) as f64).collect()) <= p.kv_value_words;
    }
    true
}

/// A larger-font row, or a bold row starting at the table's left edge, ends the table.
fn section_label(row: &Row, run: &[Row], body: f64, p: &Params) -> bool {
    let Some(cell) = row.cells.first() else { return false };
    if body != 0.0 && cell.size() / body >= p.heading_ratio {
        return true;
    }
    let mut xs = run.iter().flat_map(|r| r.cells.iter().map(|c| c.x0));
    let Some(first) = xs.next() else { return false };
    let left = xs.fold(first, pmin);
    cell.words.iter().all(|w| w.bold)
        && (cell.x0 - left).abs() <= pmax(cell.size(), 1.0)
        && !run.last().into_iter().flat_map(|r| r.cells.iter().flat_map(|c| c.words.iter())).all(|w| w.bold)
}

/// [(row ids, grid)] for runs of aligned multi-cell rows.
pub fn find_tables(page: &Page, ids: &[usize], p: &Params, body: f64) -> Vec<(Vec<usize>, Grid)> {
    let rows = visual_rows(page, ids, p);
    let mut found = Vec::new();
    let mut run: Vec<Row> = Vec::new();
    let flush = |run: &mut Vec<Row>, found: &mut Vec<(Vec<usize>, Grid)>| {
        while run.last().is_some_and(|r| !r.multi) {
            run.pop();
        }
        if run.iter().filter(|r| r.multi).count() as f64 >= p.table_min_rows {
            let cols = gutter_columns(run, p);
            if cols.len() >= 2 {
                let grid = build_grid(run, &cols, p);
                if plausible(&grid, p) {
                    found.push((run.iter().flat_map(|r| r.ids.iter().copied()).collect(), grid));
                }
            }
        }
        run.clear();
    };
    for r in rows {
        let near = run.last().is_some_and(|l| r.base - l.base <= p.table_row_gap * pmax(pmax(r.size, l.size), 1.0));
        if r.multi {
            if !run.is_empty() && !near {
                flush(&mut run, &mut found);
            }
            run.push(r);
        } else if !run.is_empty() && near && r.cells.len() == 1 && !section_label(&r, &run, body, p) {
            run.push(r); // wrapped cell text or a section label inside a table
        } else {
            flush(&mut run, &mut found);
        }
    }
    flush(&mut run, &mut found);
    found
}

pub fn grid_markdown(grid: &Grid) -> String {
    let esc = |x: &String| x.replace('|', "\\|");
    let row = |r: &Vec<String>| format!("| {} |", r.iter().map(esc).collect::<Vec<_>>().join(" | "));
    let Some(head) = grid.first() else { return String::new() };
    let mut lines = vec![row(head), format!("|{}|", vec![" --- "; head.len()].join("|"))];
    lines.extend(grid[1..].iter().map(row));
    lines.join("\n")
}

pub fn luminance(color: &str) -> f64 {
    let ch = |i: usize| pyc::py_hex(pyc::cp_slice(color, i, i + 2));
    match (ch(1), ch(3), ch(5)) {
        (Some(r), Some(g), Some(b)) => {
            0.2126 * (r as f64 / 255.0) + 0.7152 * (g as f64 / 255.0) + 0.0722 * (b as f64 / 255.0)
        }
        _ => 0.0,
    }
}

type VRule = (f64, f64, f64);

/// Column intervals from inner vertical rules, else from word gutters; None if not a table.
fn table_columns(
    rows: &mut [Row],
    v: &[VRule],
    x0: f64,
    x1: f64,
    y0: f64,
    y1: f64,
    p: &Params,
) -> Option<Vec<(f64, f64)>> {
    // v is sorted by x (x is never NaN)
    let v = &v[v.partition_point(|r| r.0 <= x0 + 2.0)..];
    let v = &v[..v.partition_point(|r| r.0 < x1 - 2.0)];
    let mut vx: Vec<f64> = v
        .iter()
        .filter(|&&(x, ya, yb)| x0 + 2.0 < x && x < x1 - 2.0 && pmin(yb, y1) - pmax(ya, y0) > 0.5 * (y1 - y0))
        .map(|&(x, _, _)| round1(x))
        .collect();
    vx.sort_by(|a, b| fcmp(*a, *b));
    vx.dedup_by(|a, b| a == b);
    let mut merged: Vec<f64> = Vec::new();
    for x in vx {
        if merged.last().is_none_or(|&m| x - m > 2.0) {
            merged.push(x);
        }
    }
    if !merged.is_empty() {
        let mut edges = vec![x0];
        edges.extend(merged);
        edges.push(x1);
        return Some(edges.windows(2).map(|e| (e[0], e[1])).collect());
    }
    for r in rows.iter_mut() {
        r.multi = r.cells.len() >= 2;
    }
    if rows.iter().filter(|r| r.multi).count() < 2 {
        return None;
    }
    Some(gutter_columns(rows, p))
}

struct Group {
    x0: f64,
    x1: f64,
    ys: Vec<f64>,
}

/// Tables outlined by vector rules (doppler -rules): [(members, grid, bbox)].
pub fn ruled_tables(
    page: &Page,
    ids: &[usize],
    body: f64,
    p: &Params,
) -> Vec<RuledTable> {
    if page.rules.is_empty() {
        return Vec::new();
    }
    let lines: &[Rc<Line>] = &page.lines;
    // (vertical middle, id) in ascending order, for the lines inside a y band; every middle is
    // finite or infinite (never NaN: coordinates are finite), so the order is a total one
    let mut by_mid: Vec<(f64, usize)> = ids.iter().map(|&i| ((lines[i].y0 + lines[i].y1) / 2.0, i)).collect();
    by_mid.sort_by(|a, b| fcmp(a.0, b.0));
    let ids_ascending = ids.windows(2).all(|w| w[0] < w[1]);
    let mut h: Vec<(f64, f64, f64)> = Vec::new();
    let mut v: Vec<VRule> = Vec::new();
    for r in &page.rules {
        let (w, hh) = (r.x1 - r.x0, r.y1 - r.y0);
        if r.boxed {
            // shaded rows and headers: their top and bottom edges separate rows; skip page backgrounds
            if w >= 0.5 * body && w < 0.95 * page.width && hh < 0.5 * page.height {
                h.push((r.x0, r.x1, r.y0));
                h.push((r.x0, r.x1, r.y1));
            }
        } else if luminance(&r.color) > p.rule_max_lum {
            continue;
        } else if w >= hh && w >= 0.5 * body {
            h.push((r.x0, r.x1, (r.y0 + r.y1) / 2.0));
        } else if hh > w && hh >= 0.8 * body {
            v.push(((r.x0 + r.x1) / 2.0, r.y0, r.y1));
        }
    }
    // by x, so table_columns and caption_band look at the rules in an x range only (their
    // results do not depend on the order of v)
    v.sort_by(|a, b| fcmp(a.0, b.0));
    // join collinear segments that touch (cell borders drawn cell by cell)
    h.sort_by(|a, b| fcmp(round0(a.2), round0(b.2)).then(fcmp(a.0, b.0)));
    let mut joined: Vec<(f64, f64, f64)> = Vec::new();
    for (x0, x1, y) in h {
        match joined.last_mut() {
            Some(j) if (j.2 - y).abs() <= 1.0 && x0 <= j.1 + 2.0 => *j = (j.0, pmax(j.1, x1), j.2),
            _ => joined.push((x0, x1, y)),
        }
    }
    let mut h: Vec<(f64, f64, f64)> = joined.into_iter().filter(|h| h.1 - h.0 >= p.rule_min_len * body).collect();
    h.sort_by(|a, b| fcmp(a.2, b.2));
    let mut groups: Vec<Group> = Vec::new();
    // Groups (in creation order) whose last rule is close enough to take the next one. Rules
    // come in ascending y and a group's last y only moves when it takes a rule, so once
    // `y - last` exceeds the gap it does for every later rule: dropping such groups leaves the
    // first match unchanged without rescanning every group for every rule.
    let mut open: Vec<usize> = Vec::new();
    let gap = p.rule_row_gap * body;
    for (x0, x1, y) in h {
        let mut hit = None;
        open.retain(|&gi| {
            let g = &groups[gi];
            if !g.ys.last().is_some_and(|&l| y - l <= gap) {
                return false;
            }
            if hit.is_none() {
                let ov = pmin(x1, g.x1) - pmax(x0, g.x0);
                if ov >= 0.8 * pmin(x1 - x0, g.x1 - g.x0) {
                    hit = Some(gi);
                }
            }
            true
        });
        match hit.map(|gi| &mut groups[gi]) {
            Some(g) => {
                g.ys.push(y);
                g.x0 = pmin(g.x0, x0);
                g.x1 = pmax(g.x1, x1);
            }
            None => {
                open.push(groups.len());
                groups.push(Group { x0, x1, ys: vec![y] });
            }
        }
    }
    // split a group where the rule spacing jumps (two tables sharing a width)
    let mut split: Vec<Group> = Vec::new();
    for g in &groups {
        let mut sorted = g.ys.clone();
        sorted.sort_by(|a, b| fcmp(*a, *b));
        let mut ys: Vec<f64> = Vec::new();
        for y in sorted {
            if ys.last().is_none_or(|&l| y - l > 1.5) {
                ys.push(y);
            }
        }
        let gaps: Vec<f64> = ys.windows(2).map(|w| w[1] - w[0]).collect();
        let limit = if gaps.is_empty() { 0.0 } else { p.rule_split * median(gaps.clone()) };
        let caption_band = |a: f64, b: f64| {
            // a band of single-cell text that no vertical rule crosses separates two ruled tables
            let lo = v.partition_point(|r| r.0 < g.x0 - 2.0);
            let crossed = v[lo..v[lo..].partition_point(|r| r.0 <= g.x1 + 2.0) + lo]
                .iter()
                .any(|&(x, ya, yb)| g.x0 - 2.0 <= x && x <= g.x1 + 2.0 && pmin(yb, b) - pmax(ya, a) > 0.5 * (b - a));
            if crossed || v.is_empty() {
                return false;
            }
            let mut any = false;
            // the result does not depend on the order lines are visited in
            let lo = by_mid.partition_point(|&(m, _)| m <= a);
            let hi = by_mid.partition_point(|&(m, _)| m < b).max(lo);
            for &(my, i) in &by_mid[lo..hi] {
                let l = &lines[i];
                let mx = (l.x0 + l.x1) / 2.0;
                if a < my && my < b && g.x0 - 2.0 <= mx && mx <= g.x1 + 2.0 {
                    any = true;
                    if segment_count(l, p) != 1 {
                        return false;
                    }
                }
            }
            any
        };
        let mut cur: Vec<f64> = ys.first().copied().into_iter().collect();
        for w in ys.windows(2) {
            let (a, b) = (w[0], w[1]);
            if (b - a > limit && gaps.len() >= 3) || caption_band(a, b) {
                split.push(Group { x0: g.x0, x1: g.x1, ys: std::mem::take(&mut cur) });
            }
            cur.push(b);
        }
        if !cur.is_empty() {
            split.push(Group { x0: g.x0, x1: g.x1, ys: cur });
        }
    }
    let mut out = Vec::new();
    // ids stably sorted by baseline, as visual_rows sorts any subset of them
    let mut order: Vec<usize> = ids.to_vec();
    order.sort_by(|&a, &b| fcmp(lines[a].base(), lines[b].base()));
    let finite_bases = order.iter().all(|&i| lines[i].base().is_finite());
    let mut used = vec![false; lines.len()];
    let mut in_members = vec![false; lines.len()];
    for g in &split {
        let ys = &g.ys;
        if ys.len() < 2 {
            continue;
        }
        let (x0, x1, y0, y1) = (g.x0, g.x1, ys[0], ys[ys.len() - 1]);
        let inside = |i: usize| {
            let m = (lines[i].x0 + lines[i].x1) / 2.0;
            x0 - 2.0 <= m && m <= x1 + 2.0
        };
        let member = |i: usize| {
            let my = (lines[i].y0 + lines[i].y1) / 2.0;
            !used[i] && inside(i) && y0 - 1.0 <= my && my <= y1 + 1.0
        };
        let mut members: Vec<usize> = if ids_ascending {
            // the lines whose middle is in [y0 - 1, y1 + 1], back in ids order
            let lo = by_mid.partition_point(|&(m, _)| m < y0 - 1.0);
            let hi = by_mid.partition_point(|&(m, _)| m <= y1 + 1.0).max(lo);
            let mut m: Vec<usize> = by_mid[lo..hi].iter().map(|&(_, i)| i).filter(|&i| member(i)).collect();
            m.sort_unstable();
            m
        } else {
            ids.iter().copied().filter(|&i| member(i)).collect()
        };
        if members.len() < 2 {
            continue;
        }
        let mut rows = visual_rows(page, &members, p);
        let Some(cols) = table_columns(&mut rows, &v, x0, x1, y0, y1, p) else { continue };
        if cols.len() < 2 {
            continue;
        }
        // a header row just above the first rule and unruled rows just below the last one,
        // when every cell sits inside one column
        let fits = |r: &Row, cols: &[(f64, f64)]| {
            r.cells.len() >= 2 && r.cells.iter().all(|c| cols.iter().any(|&(a, b)| a - 2.0 <= c.x0 && c.x1 <= b + 2.0))
        };
        let pitches: Vec<f64> = rows.windows(2).map(|w| w[1].base - w[0].base).collect();
        let pitch = if pitches.is_empty() { median(vec![2.0 * body]) } else { median(pitches) };
        // the other lines' visual rows: only the last one above the first rule and the first
        // ones below the last rule are used, so build just those when the page allows it
        let n_members = members.len();
        for &i in &members {
            in_members[i] = true;
        }
        let win = RowWindow { lines, order: &order, keep: |i: usize| !used[i] && !in_members[i] && inside(i), p };
        let windowed = (|| {
            if !finite_bases {
                return None;
            }
            let above_end = order.partition_point(|&i| lines[i].base() < y0 + 1.0);
            let above = match above_end {
                0 => None,
                e => match win.row_start_at_or_before(e - 1)? {
                    None => None,
                    Some(start) => {
                        // the row holding the last kept line above y0 + 1
                        let mut pos = start;
                        let mut last = None;
                        while pos < above_end {
                            last = win.next_row(&mut pos);
                        }
                        last
                    }
                },
            };
            let below_start = order.partition_point(|&i| lines[i].base() <= y1 - 1.0);
            let start = match below_start {
                0 => 0,
                b => win.row_start_at_or_before(b - 1)?.unwrap_or(b),
            };
            Some((above, start))
        })();
        let fallback;
        let (above, mut below): (Option<Row>, Box<dyn Iterator<Item = Row> + '_>) = match windowed {
            Some((above, mut pos)) => (above, Box::new(std::iter::from_fn(move || win.next_row(&mut pos)))),
            None => {
                let other_ids: Vec<usize> = ids.iter().copied().filter(|&i| (win.keep)(i)).collect();
                fallback = visual_rows(page, &other_ids, p);
                let above = fallback.iter().rfind(|r| r.base < y0 + 1.0).cloned();
                (above, Box::new(fallback.iter().cloned()))
            }
        };
        if let Some(last) = &above
            && y0 - last.base <= 1.2 * pitch
            && fits(last, &cols)
        {
            members.extend(last.ids.iter().copied());
        }
        let mut edge = rows[rows.len() - 1].base;
        for r in below.by_ref().filter(|r| r.base > y1 - 1.0) {
            if r.base - edge > 1.6 * pitch || !fits(&r, &cols) {
                break;
            }
            members.extend(r.ids.iter().copied());
            edge = r.base;
        }
        drop(below);
        for &i in &members[..n_members] {
            in_members[i] = false;
        }
        let mut rows = visual_rows(page, &members, p);
        let cols = match table_columns(&mut rows, &v, x0, x1, y0, y1, p) {
            Some(c) if !c.is_empty() => c,
            _ => cols,
        };
        let bands: Vec<(f64, f64)> = ys.windows(2).map(|w| (w[0], w[1])).collect();
        let grid = if bands.len() as f64 >= p.band_rows * rows.len() as f64 && bands.len() >= 2 {
            let mut grid: Grid = Vec::new();
            // rows outside the ruled span (a header above, unruled rows below) are rows of their own
            let head = rows.iter().filter(|r| r.base < ys[0] - 1.0).map(|r| (r.base - 0.5, r.base + 0.5));
            let tail: Vec<(f64, f64)> =
                rows.iter().filter(|r| r.base > ys[ys.len() - 1] + 1.0).map(|r| (r.base - 0.5, r.base + 0.5)).collect();
            let spans: Vec<(f64, f64)> = head.chain(bands.iter().copied()).chain(tail).collect();
            for (a, b) in spans {
                let band_rows: Vec<&Row> = rows.iter().filter(|r| a - 1.0 <= r.base && r.base <= b + 1.0).collect();
                if band_rows.is_empty() {
                    continue;
                }
                let mut cells: Vec<Vec<Word>> = vec![Vec::new(); cols.len()];
                for r in band_rows {
                    for c in &r.cells {
                        for w in &c.words {
                            let mid = (w.x0 + w.x1) / 2.0;
                            let k = cols
                                .iter()
                                .position(|&(ca, cb)| ca - 1.0 <= mid && mid <= cb + 1.0)
                                .unwrap_or(cols.len() - 1);
                            cells[k].push(w.clone());
                        }
                    }
                }
                grid.push(cells.into_iter().map(line_text).collect());
            }
            keep_filled(grid, cols.len())
        } else {
            for r in rows.iter_mut() {
                r.multi = r.cells.len() >= 2;
            }
            build_grid(&rows, &cols, p)
        };
        if grid.len() < 2 || grid[0].len() < 2 || numbered_list(&grid, p) {
            continue;
        }
        for &i in &members {
            used[i] = true;
        }
        out.push((members, grid, (x0, y0, x1, y1)));
    }
    out
}
