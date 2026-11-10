//! Gutter tables and ruled tables (the `tables` section of doppler_md.py).

use crate::TableFormat;
use crate::layout::{Budget, near_any};
use crate::model::{Line, Page, Word, is_ltr, is_rtl};
use crate::params::Params;
use crate::pyc::{self, char_len, fcmp, median, pmax, pmin, round0, round1, split_count, strip};
use std::rc::Rc;

/// `NUMBERED_RE.match(s)`
pub fn numbered(s: &str) -> bool {
    crate::patterns::numbered(s)
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

/// `bool(NUMERIC_RE.match(text.strip()))`
pub fn numeric(text: &str) -> bool {
    crate::patterns::numeric_match(strip(text))
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

/// Table rows with the x interval of each kept column (Python's `Grid` list subclass).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Table {
    pub rows: Grid,
    pub spans: Option<String>,
    pub cols: Vec<(f64, f64)>,
    /// found by the wide-spaced (sparse) borderless pass: a title above it becomes its header
    /// only when the title's words split into its columns
    pub sparse: bool,
    /// the first row is a header assembled from several stacked lines of header text
    pub header_row: bool,
}

/// (member line ids, table, bbox x0 y0 x1 y1)
pub type RuledTable = (Vec<usize>, Table, (f64, f64, f64, f64));

/// Drop the columns no row fills, keeping the intervals of the others.
fn keep_filled(grid: Grid, cols: &[(f64, f64)], header_row: bool) -> Table {
    let keep: Vec<usize> = (0..cols.len()).filter(|&i| grid.iter().any(|row| !row[i].is_empty())).collect();
    let rows = grid.into_iter().map(|row| keep.iter().map(|&i| row[i].clone()).collect()).collect();
    Table { rows, spans: None, cols: keep.iter().map(|&i| cols[i]).collect(), sparse: false, header_row }
}

/// The column of a word's midpoint, as `build_grid` has always chosen it: the first column
/// whose interval (1 point of slack each side) holds it, else the first column with the nearest
/// centre. Columns from `gutter_columns` and ruled tables are finite and ordered (each ends
/// where or before the next starts), so both are binary searches; any other column list takes
/// the linear scans.
struct ColumnLookup<'a> {
    cols: &'a [(f64, f64)],
    centers: Option<Vec<f64>>,
}

impl<'a> ColumnLookup<'a> {
    fn new(cols: &'a [(f64, f64)]) -> Self {
        let ordered = cols.iter().all(|&(a, b)| a.is_finite() && b.is_finite() && a <= b)
            && cols.windows(2).all(|w| w[0].1 <= w[1].0);
        let centers = ordered.then(|| cols.iter().map(|&(a, b)| (a + b) / 2.0).collect());
        ColumnLookup { cols, centers }
    }

    fn column(&self, mid: f64) -> usize {
        let cols = self.cols;
        let Some(centers) = &self.centers else {
            if let Some(k) = cols.iter().position(|&(a, b)| a - 1.0 <= mid && mid <= b + 1.0) {
                return k;
            }
            let d = |i: usize| ((cols[i].0 + cols[i].1) / 2.0 - mid).abs();
            let mut best = 0;
            for i in 1..cols.len() {
                if d(i) < d(best) {
                    best = i;
                }
            }
            return best;
        };
        // b + 1 ascends, so the first column that can hold mid is the first with mid <= b + 1;
        // if that one starts after mid, every later one does too
        let k = cols.partition_point(|&(_, b)| b + 1.0 < mid);
        if k < cols.len() && cols[k].0 - 1.0 <= mid {
            return k;
        }
        // centres ascend: the nearest is just below or at/above mid; ties go to the left (lower
        // index), and right of mid the first is the nearest
        let j = centers.partition_point(|&c| c < mid);
        let right = (j < centers.len()).then(|| (j, (centers[j] - mid).abs()));
        // left of mid the distance never grows toward mid (rounded subtraction is monotonic), so
        // the columns as near as the nearest form one block ending at j - 1: take its first,
        // as the linear scan does (far from the columns, many distances round to one value)
        let left = (j > 0).then(|| {
            let dl = (centers[j - 1] - mid).abs();
            (centers[..j].partition_point(|&c| (c - mid).abs() > dl), dl)
        });
        match (left, right) {
            (Some((l, dl)), Some((r, dr))) => {
                if dr < dl {
                    r
                } else {
                    l
                }
            }
            (Some((l, _)), None) => l,
            (None, Some((r, _))) => r,
            (None, None) => 0,
        }
    }
}

pub fn build_grid(rows: &[Row], cols: &[(f64, f64)], p: &Params) -> Table {
    if cols.is_empty() {
        return Table::default();
    }
    let lookup = ColumnLookup::new(cols);
    let column = |w: &Word| lookup.column((w.x0 + w.x1) / 2.0);
    let multi: Vec<&Row> = rows.iter().filter(|r| r.multi).collect();
    let pitches: Vec<f64> = multi.windows(2).map(|w| w[1].base - w[0].base).collect();
    let pitch = if pitches.is_empty() { None } else { Some(median(pitches)) };
    let mut grid: Grid = Vec::new();
    // per grid row: the baselines of its first and last visual rows, and its size
    let mut spans: Vec<(f64, f64, f64)> = Vec::new();
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
                if let Some(s) = spans.last_mut() {
                    s.1 = r.base;
                }
                prev = Some(r);
                continue;
            }
        }
        grid.push(texts);
        spans.push((r.base, r.base, r.size));
        prev = Some(r);
    }
    let header_row = p.merge_header_rows && merge_stacked_header(&mut grid, &spans, p);
    keep_filled(grid, cols, header_row)
}

/// A header printed on several stacked lines (a cell's words centred in a tall header, or a
/// label broken over lines) comes out of the row grouping as several sparse rows. When the
/// first rows are tighter than the rows of the body, hold no number, and their words fall
/// in different columns' cells, they are one header row: join each column's cells from the top.
/// Returns whether it did.
fn merge_stacked_header(grid: &mut Grid, spans: &[(f64, f64, f64)], p: &Params) -> bool {
    const MAX_STACK: usize = 5;
    if grid.len() < 4 {
        return false;
    }
    let gap = |i: usize| spans[i].0 - spans[i - 1].1;
    // the gaps inside the header are close to one line pitch; the body's are looser
    let body_gaps: Vec<f64> = (MAX_STACK.min(grid.len() - 1)..grid.len()).map(gap).filter(|g| *g > 0.0).collect();
    if body_gaps.len() < 2 {
        return false;
    }
    let body = median(body_gaps);
    let data = |row: &Vec<String>| row.iter().any(|c| numeric(strip(c)));
    let mut stack = 1;
    let mut tight: f64 = 0.0;
    while stack < MAX_STACK.min(grid.len() - 2) {
        let g = gap(stack);
        if g <= 0.0 || g > p.header_stack_gap * pmax(spans[stack].2, 1.0) || data(&grid[stack]) || data(&grid[0]) {
            break;
        }
        tight = tight.max(g);
        stack += 1;
    }
    // the stack ends where the spacing opens up: that gap is the header's bottom edge
    if stack < 2
        || body < p.header_stack_open * tight
        || stack >= grid.len() - 1
        || gap(stack) < p.header_stack_open * tight
    {
        return false;
    }
    let merged: Vec<String> = (0..grid[0].len())
        .map(|k| {
            strip(&grid[..stack].iter().map(|r| r[k].as_str()).filter(|t| !t.is_empty()).collect::<Vec<_>>().join(" "))
                .to_string()
        })
        .collect();
    grid.splice(..stack, [merged]);
    true
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
pub fn find_tables(page: &Page, ids: &[usize], p: &Params, body: f64) -> Vec<(Vec<usize>, Table)> {
    find_tables_within(page, ids, p, body, &mut Budget::new(f64::INFINITY)).unwrap_or_default()
}

/// Words in rows: the work units of building their columns and grid.
fn row_words(rows: &[Row]) -> usize {
    rows.iter().map(|r| r.cells.iter().map(|c| c.words.len()).sum::<usize>()).sum()
}

/// [`find_tables`] within a work budget: each run that reaches column detection charges its
/// words twice (columns, then grid; grouping the rows is linear and is not charged, like
/// parsing). `None` once the budget runs out.
pub fn find_tables_within(
    page: &Page,
    ids: &[usize],
    p: &Params,
    body: f64,
    budget: &mut Budget,
) -> Option<Vec<(Vec<usize>, Table)>> {
    let rows = visual_rows(page, ids, p);
    let mut found = Vec::new();
    let mut run: Vec<Row> = Vec::new();
    let mut flush = |run: &mut Vec<Row>, found: &mut Vec<(Vec<usize>, Table)>| -> bool {
        while run.last().is_some_and(|r| !r.multi) {
            run.pop();
        }
        if run.iter().filter(|r| r.multi).count() as f64 >= p.table_min_rows {
            if !budget.spend(2 * row_words(run)) {
                return false;
            }
            let cols = gutter_columns(run, p);
            if cols.len() >= 2 {
                let grid = build_grid(run, &cols, p);
                if plausible(&grid.rows, p) {
                    found.push((run.iter().flat_map(|r| r.ids.iter().copied()).collect(), grid));
                }
            }
        }
        run.clear();
        true
    };
    // the sparse pass reads the rows again; copy them only when it runs
    let again = p.sparse_tables.then(|| rows.clone());
    for r in rows {
        let near = run.last().is_some_and(|l| r.base - l.base <= p.table_row_gap * pmax(pmax(r.size, l.size), 1.0));
        if r.multi {
            if !run.is_empty() && !near && !flush(&mut run, &mut found) {
                return None;
            }
            run.push(r);
        } else if !run.is_empty() && near && r.cells.len() == 1 && !section_label(&r, &run, body, p) {
            run.push(r); // wrapped cell text or a section label inside a table
        } else if !flush(&mut run, &mut found) {
            return None;
        }
    }
    if !flush(&mut run, &mut found) {
        return None;
    }
    if let Some(rows) = again {
        sparse_tables(&rows, &mut found, p, budget)?;
    }
    Some(found)
}

/// Borderless tables whose rows are too far apart for the dense pass (or that it left for any
/// other reason): maximal runs of consecutive multi-cell rows, none in a table already, with
/// evenly spaced baselines at most `sparse_row_gap` font sizes apart. Rows may have different
/// cell counts: the run's columns come from the gutters all its rows share (`gutter_columns`, as
/// in the dense pass) and a row may leave a column empty. A run becomes a table only whole:
/// every word must fall inside a column and no two cells of a row may share one, every row must
/// stay its own grid row, and the grid must not read as a list, line or footnote numbers, or a
/// table of contents. Otherwise its rows stay as they were. A run grows row by row and the next
/// one starts where it stopped, so this is linear in the rows.
fn sparse_tables(rows: &[Row], found: &mut Vec<(Vec<usize>, Table)>, p: &Params, budget: &mut Budget) -> Option<()> {
    let used: std::collections::HashSet<usize> = found.iter().flat_map(|(ids, _)| ids.iter().copied()).collect();
    let free = |r: &Row| r.multi && r.ids.iter().all(|i| !used.contains(i));
    let even = |a: f64, b: f64, ratio: f64| pmax(a, b) <= ratio * pmin(a, b);
    let mut i = 0;
    while i < rows.len() {
        if !free(&rows[i]) {
            i += 1;
            continue;
        }
        let mut gaps: Vec<f64> = Vec::new();
        let mut j = i + 1;
        while j < rows.len() {
            let (prev, r) = (&rows[j - 1], &rows[j]);
            let gap = r.base - prev.base;
            if !free(r) || !(gap > 0.0 && gap <= p.sparse_row_gap * pmax(pmax(prev.size, r.size), 1.0)) {
                break;
            }
            gaps.push(gap);
            j += 1;
        }
        let start = i;
        i = j;
        // evenly spaced as a whole (the first gap, a header, may differ by twice the ratio). A
        // final row after a wider gap (a total, a signature line) does not veto the rows above
        // it: they are judged without it, and it joins only if its cells fit their columns.
        // Unevenness anywhere else leaves the run as it was rather than cut into partial tables.
        let uneven: Vec<usize> = gaps
            .windows(2)
            .enumerate()
            .filter(|&(k, w)| !even(w[0], w[1], if k == 0 { 2.0 * p.sparse_pitch_ratio } else { p.sparse_pitch_ratio }))
            .map(|(k, _)| k)
            .collect();
        let last_only = uneven.len() == 1 && uneven[0] + 2 == gaps.len() && gaps.len() >= 3;
        if !(uneven.is_empty() || last_only) {
            continue;
        }
        let core = &rows[start..if last_only { j - 1 } else { j }];
        if (core.len() as f64) < p.sparse_min_rows {
            continue;
        }
        if !budget.spend(2 * row_words(&rows[start..j])) {
            return None;
        }
        let cols = gutter_columns(core, p);
        if cols.len() < 2 || !whole(core, &cols) {
            continue;
        }
        let end = if last_only && whole(std::slice::from_ref(&rows[j - 1]), &cols) { j } else { start + core.len() };
        let run = &rows[start..end];
        // a multi-cell row next to the run (within twice sparse_row_gap: a table can go on past
        // a gap too wide to chain, e.g. rows of varying height) that fits its columns: the
        // table goes on, and the run alone would be a fragment of it. Rows further away (a
        // page number at the foot) do not count.
        let near =
            |a: &Row, b: &Row| (b.base - a.base).abs() <= 2.0 * p.sparse_row_gap * pmax(pmax(a.size, b.size), 1.0);
        let before = start.checked_sub(1).map(|k| (&rows[k], &rows[start]));
        let after = rows.get(end).map(|r| (r, &rows[end - 1]));
        if [before, after]
            .into_iter()
            .flatten()
            .any(|(r, edge)| r.multi && near(r, edge) && whole(std::slice::from_ref(r), &cols))
        {
            continue;
        }
        let mut grid = build_grid(run, &cols, p);
        // every column filled in two rows or more: a column only one row fills is a
        // misaligned header or a stray cell, not a column
        let filled_twice = (0..grid.rows.first().map_or(0, Vec::len))
            .all(|k| grid.rows.iter().filter(|r| !strip(&r[k]).is_empty()).count() >= 2);
        if grid.rows.len() == run.len()
            && filled_twice
            && grid.rows.first().is_some_and(|r| r.len() >= 2)
            && plausible(&grid.rows, p)
            && !marker_column(&grid.rows)
            && !counter_column(&grid.rows)
            && !contents(&grid.rows)
            && !checklist(&grid.rows)
        {
            grid.sparse = true;
            found.push((run.iter().flat_map(|r| r.ids.iter().copied()).collect(), grid));
        }
    }
    Some(())
}

/// Every word of the run inside one of `cols` (1 point of slack), and no column holding words
/// of two different cells of one row (a cell the gutters did not separate).
fn whole(run: &[Row], cols: &[(f64, f64)]) -> bool {
    let lookup = ColumnLookup::new(cols);
    let mut owners: Vec<(usize, usize)> = Vec::new();
    for r in run {
        owners.clear();
        for (ci, cell) in r.cells.iter().enumerate() {
            for w in &cell.words {
                let mid = (w.x0 + w.x1) / 2.0;
                let k = lookup.column(mid);
                if !(cols[k].0 - 1.0 <= mid && mid <= cols[k].1 + 1.0) {
                    return false;
                }
                owners.push((k, ci));
            }
        }
        owners.sort_unstable();
        owners.dedup();
        if owners.windows(2).any(|w| w[0].0 == w[1].0) {
            return false;
        }
    }
    true
}

/// A first column that reads as list or note markers: mostly punctuated markers (`1.`, `a)`,
/// `12a.`, bullets) or footnote symbols (`*`, `†`, `‡`, `§`, `¶`), letters counting `a`, `b`, `c`
/// beside cells without numbers, or mostly bare section numbers (`4.1`) beside prose (a median of
/// 3 or more words). Bare codes beside short cells (an exhibit or part number column) stay a
/// table.
fn marker_column(grid: &Grid) -> bool {
    let keys: Vec<&str> = grid.iter().map(|r| strip(&r[0])).filter(|k| !k.is_empty()).collect();
    if keys.is_empty() {
        return false;
    }
    // `1.`, `a)`, `(iv)` (NUMBERED_RE ending in punctuation), or `12a.`: digits, then letters
    let punctuated = |k: &str| {
        let lettered = k.strip_suffix('.').is_some_and(|b| {
            let d = b.bytes().take_while(u8::is_ascii_digit).count();
            (1..=4).contains(&d) && b.len() - d <= 2 && b.bytes().skip(d).all(|c| c.is_ascii_lowercase())
        });
        bullet(k) || (numbered(k) && k.ends_with(['.', ')'])) || lettered
    };
    let symbol = |k: &str| k.chars().count() <= 3 && k.chars().all(|c| matches!(c, '*' | '†' | '‡' | '§' | '¶' | '#'));
    let half = |f: &dyn Fn(&str) -> bool| 2 * keys.iter().filter(|k| f(k)).count() >= keys.len();
    let letters = {
        let v: Option<Vec<u8>> = grid
            .iter()
            .map(|r| match strip(&r[0]).as_bytes() {
                [c] if c.is_ascii_lowercase() => Some(*c),
                _ => None,
            })
            .collect();
        v.is_some_and(|v| v.windows(2).all(|w| w[1] == w[0] + 1)) && !numbers_beside(grid, 0)
    };
    half(&punctuated) || half(&symbol) || letters || (half(&numbered) && grid[0].len() >= 2 && prose_column(grid, 1))
}

/// Column `k` holds prose: a median of 3 or more words.
fn prose_column(grid: &Grid, k: usize) -> bool {
    median(grid.iter().map(|r| split_count(&r[k]) as f64).collect()) >= 3.0
}

/// Any cell outside column `k` is a number.
fn numbers_beside(grid: &Grid, k: usize) -> bool {
    grid.iter().any(|r| r.iter().enumerate().any(|(i, c)| i != k && !strip(c).is_empty() && numeric(c)))
}

/// A bare count of 1 to 4 ASCII digits.
fn small_int(t: &str) -> Option<u32> {
    let t = strip(t);
    (!t.is_empty() && t.len() <= 4 && t.bytes().all(|b| b.is_ascii_digit())).then(|| t.parse().ok()).flatten()
}

/// The first or last column counts 1, 2, 3, ... (each row one more) and no other column holds a
/// number: footnote numbers, verse or pleading line numbers, or a list numbered without
/// punctuation. A ranked table has numbers beside its index column and stays a table.
fn counter_column(grid: &Grid) -> bool {
    let n = grid[0].len();
    [0, n - 1].into_iter().any(|k| {
        let nums: Option<Vec<u32>> = grid.iter().map(|r| small_int(&r[k])).collect();
        nums.is_some_and(|v| v.windows(2).all(|w| w[1] == w[0] + 1)) && !numbers_beside(grid, k)
    })
}

/// A page number, or the first page of a range (`12-15`, `12–15`).
fn page_ref(t: &str) -> Option<u32> {
    let t = strip(t);
    match t.split_once(['-', '–', '—']) {
        Some((a, b)) => small_int(b).and(small_int(a)),
        None => small_int(t),
    }
}

/// A table of contents: every row ends in a page number or range, the pages never decrease,
/// and every other cell is text, apart from a first column of chapter or section numbers. It
/// needs evidence beyond that shape, since a sorted price list or a chronology has it too: the
/// first column of section numbers, or a header row whose last cell is a page word (`Page`).
fn contents(grid: &Grid) -> bool {
    let n = grid[0].len();
    let page_word = |t: &str| {
        let t = pyc::py_lower(strip(t));
        matches!(&*t, "page" | "pages" | "p." | "pp." | "pg." | "seite" | "página" | "pagina")
    };
    let header = page_word(&grid[0][n - 1]);
    let body = if header { &grid[1..] } else { &grid[..] };
    if body.len() < 2 {
        return false;
    }
    let pages: Option<Vec<u32>> = body.iter().map(|r| page_ref(&r[n - 1])).collect();
    let Some(pages) = pages else { return false };
    if pages.windows(2).any(|w| w[1] < w[0]) {
        return false;
    }
    let section =
        |t: &str| small_int(t).is_some() || numbered(t) || strip(t).split('.').all(|x| small_int(x).is_some());
    let first_numbers = n > 2 && body.iter().all(|r| !strip(&r[0]).is_empty() && section(&r[0]));
    let text_from = if first_numbers { 1 } else { 0 };
    let text = body.iter().all(|r| r[text_from..n - 1].iter().all(|c| strip(c).is_empty() || !numeric(c)))
        && body.iter().any(|r| r[text_from..n - 1].iter().any(|c| !strip(c).is_empty()));
    text && (first_numbers || header)
}

/// A checklist laid out in columns, not a table: every column's cells mostly start with a
/// checkbox or tick, or (checkbox widgets are form fields, absent from the text layer) two
/// columns of the same kind of item. Key/value tables differ between their columns: one is
/// mostly numbers, cells carry `label:`, or one column is much longer than the other.
fn checklist(grid: &Grid) -> bool {
    let k = grid.first().map_or(0, Vec::len);
    let column = |c: usize| grid.iter().map(|r| strip(&r[c])).filter(|t| !t.is_empty()).collect::<Vec<&str>>();
    let mostly = |cells: &[&str], f: &dyn Fn(&str) -> bool| {
        !cells.is_empty() && 2 * cells.iter().filter(|t| f(t)).count() >= cells.len()
    };
    let boxed = |t: &str| t.chars().next().is_some_and(is_checkbox_char);
    if k >= 2 && (0..k).all(|c| mostly(&column(c), &boxed)) {
        return true;
    }
    if k != 2 {
        return false;
    }
    let (a, b) = (column(0), column(1));
    let digits = |t: &str| t.chars().any(|ch| ch.is_numeric());
    let label = |t: &str| t.contains([':', '：']);
    if mostly(&a, &digits) || mostly(&b, &digits) || a.iter().chain(&b).any(|t| label(t)) {
        return false;
    }
    let len = |cells: &[&str]| median(cells.iter().map(|t| char_len(t) as f64).collect());
    let (la, lb) = (len(&a), len(&b));
    pmax(la, lb) <= 2.0 * pmin(la, lb)
}

fn is_checkbox_char(c: char) -> bool {
    matches!(c, '☐' | '☑' | '☒' | '□' | '▢' | '◻' | '◼' | '❏' | '❐' | '❑' | '❒' | '✓' | '✔' | '✗' | '✘')
}

/// A table whose text is mostly right-to-left (`grid_rtl`).
fn grid_rtl(grid: &Grid) -> bool {
    rows_rtl(grid.iter().map(Vec::as_slice))
}

fn rows_rtl<'a>(rows: impl Iterator<Item = &'a [String]>) -> bool {
    let (mut r, mut l) = (0usize, 0usize);
    for ch in rows.flatten().flat_map(|c| c.chars()) {
        r += is_rtl(ch) as usize;
        l += is_ltr(ch) as usize;
    }
    r > 0 && r >= l
}

pub fn grid_markdown(grid: &Grid) -> String {
    grid_markdown_with(grid, false)
}

/// Escape a GFM cell whose `$...$` spans are LaTeX written by the math reader: outside them,
/// `\` and `|` as usual; inside, backslashes stay (TeX needs them) and `|` becomes `\vert `,
/// which cannot end the cell.
fn escape_math_cell(x: &str) -> String {
    let mut out = String::with_capacity(x.len() + 8);
    let mut in_math = false;
    let b: Vec<char> = x.chars().collect();
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if c == '$' && (i == 0 || b[i - 1] != '\\') {
            in_math = !in_math;
            out.push(c);
            if b.get(i + 1) == Some(&'$') {
                // `$$` is one delimiter
                out.push('$');
                i += 1;
            }
        } else if in_math {
            if c == '|' {
                out.push_str("\\vert ");
            } else {
                out.push(c);
            }
        } else if c == '\\' {
            out.push_str("\\\\");
        } else if c == '|' {
            out.push_str("\\|");
        } else {
            out.push(c);
        }
        i += 1;
    }
    if in_math {
        // an unclosed span: escape it all the usual way
        return x.replace('\\', "\\\\").replace('|', "\\|");
    }
    out
}

/// [`grid_markdown`]; with `math`, cells keep the LaTeX of `$...$` spans intact.
pub fn grid_markdown_with(grid: &Grid, math: bool) -> String {
    let rtl = grid_rtl(grid); // right-to-left table: first column is the rightmost
    // backslash first, so a cell ending in `\` cannot turn the next `|` into a literal
    let esc = |x: &String| {
        if math && x.contains('$') { escape_math_cell(x) } else { x.replace('\\', "\\\\").replace('|', "\\|") }
    };
    let row = |r: &Vec<String>| {
        let cells: Vec<String> = if rtl { r.iter().rev().map(esc).collect() } else { r.iter().map(esc).collect() };
        format!("| {} |", cells.join(" | "))
    };
    let Some(head) = grid.first() else { return String::new() };
    let mut lines = vec![row(head), format!("|{}|", vec![" --- "; head.len()].join("|"))];
    lines.extend(grid[1..].iter().map(row));
    lines.join("\n")
}

/// An HTML `<table>` for a Markdown document: one row per line and no blank lines, so a
/// CommonMark renderer reads the whole table as one HTML block. Cell text is escaped; `<thead>`
/// appears only for a detected `header`; a right-to-left table keeps its columns in reading
/// visual order under `dir="rtl"` (the first cell is the rightmost column, as in GFM).
pub fn grid_html(header: Option<&[String]>, rows: &Grid) -> String {
    let rtl = rows_rtl(header.into_iter().chain(rows.iter().map(Vec::as_slice)));
    let esc = |x: &str| x.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;");
    let row = |r: &[String], tag: &str| {
        // grid columns run left to right; under dir="rtl" the first cell is the rightmost
        let cell = |c: &String| format!("<{tag}>{}</{tag}>", esc(c));
        let cells: Vec<String> = if rtl { r.iter().rev().map(cell).collect() } else { r.iter().map(cell).collect() };
        format!("<tr>{}</tr>", cells.concat())
    };
    if header.is_none() && rows.is_empty() {
        return String::new();
    }
    let mut lines = vec![if rtl { "<table dir=\"rtl\">".to_string() } else { "<table>".to_string() }];
    if let Some(h) = header {
        lines.push(format!("<thead>{}</thead>", row(h, "th")));
    }
    if !rows.is_empty() {
        lines.push("<tbody>".to_string());
        lines.extend(rows.iter().map(|r| row(r, "td")));
        lines.push("</tbody>".to_string());
    }
    lines.push("</table>".to_string());
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
    format: TableFormat,
    budget: &mut Budget,
) -> Option<Vec<RuledTable>> {
    if page.rules.is_empty() {
        return Some(Vec::new());
    }
    // Baseline discovery is shared by GFM and HTML; span work is charged separately.
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
        let mut grid = if bands.len() as f64 >= p.band_rows * rows.len() as f64 && bands.len() >= 2 {
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
            keep_filled(grid, &cols, false)
        } else {
            for r in rows.iter_mut() {
                r.multi = r.cells.len() >= 2;
            }
            build_grid(&rows, &cols, p)
        };
        if grid.rows.len() < 2 || grid.rows[0].len() < 2 || numbered_list(&grid.rows, p) {
            continue;
        }
        if format == TableFormat::Html {
            grid.spans = ruled_html(page, &members, (x0, y0, x1, y1), p, budget).ok()?;
        }
        for &i in &members {
            used[i] = true;
        }
        out.push((members, grid, (x0, y0, x1, y1)));
    }
    Some(out)
}

/// Conservative closed rectangular topology. Partial edges invalidate the entire candidate.
/// Limits bound both the Cartesian grid and edge/rule comparisons.
fn ruled_html(
    page: &Page,
    ids: &[usize],
    bbox: (f64, f64, f64, f64),
    p: &Params,
    budget: &mut Budget,
) -> Result<Option<String>, ()> {
    macro_rules! charge {
        ($n:expr) => {
            if !budget.spend($n) {
                return Err(());
            }
        };
    }
    macro_rules! applicable {
        ($e:expr) => {
            match $e {
                Some(value) => value,
                None => return Ok(None),
            }
        };
    }
    fn sort_work(n: usize) -> usize {
        n.saturating_mul((usize::BITS - n.max(1).leading_zeros()) as usize + 1)
    }
    let (x0, y0, x1, y1) = bbox;
    if page.rules_truncated == Some(true) || page.rules.len() > 2048 {
        return Ok(None);
    }
    let mut h = Vec::new();
    let mut v = Vec::new();
    for q in &page.rules {
        charge!(1);
        if q.boxed {
            // Shading boundaries participate in discovery; do not erase their rows.
            return Ok(None);
        }
        // cp_slice counts the full string for each of the three channels.
        charge!(q.color.len().saturating_mul(3));
        if luminance(&q.color) > p.rule_max_lum {
            continue;
        }
        if q.x0 < x0 - 0.5 || q.x1 > x1 + 0.5 || q.y0 < y0 - 0.5 || q.y1 > y1 + 0.5 {
            continue;
        }
        if (q.y1 - q.y0).abs() <= 1.0 && q.x1 > q.x0 {
            h.push(((q.y0 + q.y1) / 2., q.x0, q.x1));
        } else if (q.x1 - q.x0).abs() <= 1.0 && q.y1 > q.y0 {
            v.push(((q.x0 + q.x1) / 2., q.y0, q.y1));
        }
    }
    let axis = |mut a: Vec<f64>| {
        a.sort_by(|a, b| a.total_cmp(b));
        a.dedup_by(|a, b| (*a - *b).abs() <= 0.5);
        a
    };
    charge!(sort_work(v.len() + 2).saturating_add(sort_work(h.len() + 2)));
    let xs = axis(v.iter().map(|q| q.0).chain([x0, x1]).collect());
    let ys = axis(h.iter().map(|q| q.0).chain([y0, y1]).collect());
    let (nc, nr) = (xs.len() - 1, ys.len() - 1);
    if nc < 2 || nr < 2 || nc > 64 || nr > 64 || nc * nr > 1024 {
        return Ok(None);
    }
    // Return absent, complete, or ambiguous. Never interpret a broken edge as a merge.
    let edge = |rules: &[(f64, f64, f64)], pos: f64, a: f64, b: f64, budget: &mut Budget| -> Result<Option<bool>, ()> {
        if !budget.spend(rules.len()) {
            return Err(());
        }
        let mut seg: Vec<_> = rules
            .iter()
            .filter(|q| (q.0 - pos).abs() <= 0.5 && q.2 > a + 0.5 && q.1 < b - 0.5)
            .map(|q| (q.1, q.2))
            .collect();
        if seg.is_empty() {
            return Ok(Some(false));
        }
        if !budget.spend(sort_work(seg.len()) + seg.len()) {
            return Err(());
        }
        seg.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut end = a;
        for (lo, hi) in seg {
            if lo > end + 0.5 {
                return Ok(None);
            }
            end = end.max(hi);
        }
        Ok(if end < b - 0.5 { None } else { Some(true) })
    };
    charge!(nc * nr * 4);
    let mut right = vec![false; nc * nr];
    let mut down = right.clone();
    for r in 0..nr {
        for c in 0..nc {
            if r == 0 && !applicable!(edge(&h, ys[0], xs[c], xs[c + 1], budget)?) {
                return Ok(None);
            }
            if c == 0 && !applicable!(edge(&v, xs[0], ys[r], ys[r + 1], budget)?) {
                return Ok(None);
            }
            right[r * nc + c] = applicable!(edge(&v, xs[c + 1], ys[r], ys[r + 1], budget)?);
            down[r * nc + c] = applicable!(edge(&h, ys[r + 1], xs[c], xs[c + 1], budget)?);
            if (c == nc - 1 && !right[r * nc + c]) || (r == nr - 1 && !down[r * nc + c]) {
                return Ok(None);
            }
        }
    }
    let mut owner = vec![usize::MAX; nc * nr];
    let mut rects = Vec::new();
    for i in 0..nc * nr {
        if owner[i] != usize::MAX {
            continue;
        }
        let id = rects.len();
        let mut todo = vec![i];
        owner[i] = id;
        let (mut ra, mut rb, mut ca, mut cb) = (i / nc, i / nc, i % nc, i % nc);
        let mut count = 0;
        while let Some(j) = todo.pop() {
            charge!(5);
            let (r, c) = (j / nc, j % nc);
            count += 1;
            ra = ra.min(r);
            rb = rb.max(r);
            ca = ca.min(c);
            cb = cb.max(c);
            for k in [
                if c + 1 < nc && !right[j] { Some(j + 1) } else { None },
                if c > 0 && !right[j - 1] { Some(j - 1) } else { None },
                if r + 1 < nr && !down[j] { Some(j + nc) } else { None },
                if r > 0 && !down[j - nc] { Some(j - nc) } else { None },
            ]
            .into_iter()
            .flatten()
            {
                if owner[k] == usize::MAX {
                    owner[k] = id;
                    todo.push(k);
                }
            }
        }
        if count != (rb - ra + 1) * (cb - ca + 1) {
            return Ok(None);
        }
        for r in ra..=rb {
            for c in ca..=cb {
                charge!(1);
                if (c < cb && right[r * nc + c]) || (r < rb && down[r * nc + c]) {
                    return Ok(None);
                }
            }
        }
        rects.push((ra, rb, ca, cb));
    }
    charge!(rects.len() + sort_work(ids.len()));
    let mut texts = vec![String::new(); rects.len()];
    // Baseline order, with Line::text retaining each line's bidi ordering.
    let mut ordered = ids.to_vec();
    ordered.sort_by(|a, b| page.lines[*a].base().total_cmp(&page.lines[*b].base()));
    for i in ordered {
        let line = &page.lines[i];
        let mut words = std::collections::BTreeMap::<usize, Vec<Word>>::new();
        charge!(sort_work(line.words.len()));
        for w in &line.words {
            charge!(xs.len().ilog2() as usize + ys.len().ilog2() as usize + w.text.len() + 1);
            let x = (w.x0 + w.x1) / 2.;
            let y = (w.y0 + w.y1) / 2.;
            if x <= x0 || x >= x1 || y <= y0 || y >= y1 {
                return Ok(None);
            }
            let c = xs.partition_point(|a| *a < x).saturating_sub(1).min(nc - 1);
            let r = ys.partition_point(|a| *a < y).saturating_sub(1).min(nr - 1);
            let id = owner[r * nc + c];
            let (ra, rb, ca, cb) = rects[id];
            if w.x0 <= xs[ca] || w.x1 >= xs[cb + 1] || w.y0 <= ys[ra] || w.y1 >= ys[rb + 1] {
                return Ok(None);
            }
            words.entry(id).or_default().push(w.clone());
        }
        for (i, ws) in words {
            if ws.is_empty() {
                continue;
            }
            if !texts[i].is_empty() {
                // Multiple source fragments need the established grid assembler's
                // wrapping/bidi evidence. Decline spans rather than invent a join.
                return Ok(None);
            }
            charge!(sort_work(ws.len()));
            for w in &ws {
                charge!(w.text.len().saturating_mul(4));
            }
            texts[i].push_str(&line_text(ws));
        }
    }
    for text in &texts {
        charge!(text.len());
    }
    let rtl = rows_rtl(std::iter::once(texts.as_slice()));
    let mut out = if rtl { "<table dir=\"rtl\">\n<tbody>\n".to_string() } else { "<table>\n<tbody>\n".to_string() };
    for r in 0..nr {
        charge!(rects.len() + 1);
        out.push_str("<tr>");
        let mut cells: Vec<_> = rects.iter().enumerate().filter(|(_, q)| q.0 == r).collect();
        if rtl {
            cells.reverse();
        }
        for (i, &(ra, rb, ca, cb)) in cells {
            charge!(texts[i].len().saturating_mul(24) + 64);
            out.push_str("<td");
            if rb > ra {
                out.push_str(&format!(" rowspan=\"{}\"", rb - ra + 1));
            }
            if cb > ca {
                out.push_str(&format!(" colspan=\"{}\"", cb - ca + 1));
            }
            out.push('>');
            out.push_str(
                &texts[i].replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;"),
            );
            out.push_str("</td>");
        }
        out.push_str("</tr>\n");
    }
    out.push_str("</tbody>\n</table>");
    Ok(Some(out))
}

#[cfg(test)]
mod span_tests {
    use super::*;
    use crate::model::Rule;
    // Source topology: 3x3 grid, upper-left 2x2 rectangle merged; other cells empty.
    fn fixture() -> Page {
        let mut page = Page {
            number: None,
            images: vec![],
            images_truncated: false,
            width: 300.,
            height: 300.,
            lines: vec![],
            rules: vec![],
            rules_truncated: Some(false),
            glyphs: vec![],
            fonts: vec![],
            extra_glyphs: vec![],
            glyphs_truncated: false,
        };
        for (x0, y0, x1, y1) in [
            (0., 0., 300., 0.),
            (0., 300., 300., 300.),
            (0., 0., 0., 300.),
            (300., 0., 300., 300.),
            (200., 0., 200., 300.),
            (100., 200., 100., 300.),
            (0., 200., 300., 200.),
            (200., 100., 300., 100.),
        ] {
            page.rules.push(Rule { x0, y0, x1, y1, color: "#000000".into(), boxed: false });
        }
        page
    }
    #[test]
    fn actual_work_and_exhaustion() {
        let mut page = fixture();
        let mut budget = Budget::new(100_000.);
        assert!(ruled_html(&page, &[], (0., 0., 300., 300.), &Params::default(), &mut budget).unwrap().is_some());
        assert!(budget.used < 10_000);
        let mut tiny = Budget::new(1.);
        assert!(ruled_html(&page, &[], (0., 0., 300., 300.), &Params::default(), &mut tiny).is_err());
        page.rules[0].boxed = true;
        let mut tiny = Budget::new(1.);
        assert_eq!(ruled_html(&page, &[], (0., 0., 300., 300.), &Params::default(), &mut tiny), Ok(None));
        assert_eq!(tiny.used, 1);
    }
    #[test]
    fn oversized_color_exhausts_budget_before_luminance_scan() {
        let mut page = fixture();
        page.rules[0].color = format!("#000000{}", "x".repeat(100_000));
        let mut budget = Budget::new(100_000.);
        assert!(ruled_html(&page, &[], (0., 0., 300., 300.), &Params::default(), &mut budget).is_err());
        assert_eq!(budget.used, 1 + 3 * page.rules[0].color.len() as u64);
    }
    #[test]
    fn combined_empty_geometry_and_rejection() {
        let mut page = fixture();
        let render = |p: &Page| {
            ruled_html(p, &[], (0., 0., 300., 300.), &Params::default(), &mut Budget::new(f64::INFINITY)).unwrap()
        };
        let html = render(&page).unwrap();
        assert!(html.contains("rowspan=\"2\" colspan=\"2\""));
        assert_eq!(html.matches("<td").count(), 6);
        page.rules[7].x0 = 220.; // partial broken edge
        assert!(render(&page).is_none());
        page = fixture();
        page.rules.remove(0); // missing outer border
        assert!(render(&page).is_none());
        page = fixture();
        page.rules[7].x0 = 100.; // internal wall in a connected rectangle
        assert!(render(&page).is_none());
        page = fixture();
        page.rules_truncated = Some(true);
        assert!(render(&page).is_none());
        page = fixture();
        page.rules.resize(2049, page.rules[0].clone());
        assert!(render(&page).is_none());
    }
}

#[cfg(test)]
mod tests {
    use super::ColumnLookup;

    /// The linear scans `build_grid` used before the binary search.
    fn reference(cols: &[(f64, f64)], mid: f64) -> usize {
        if let Some(k) = cols.iter().position(|&(a, b)| a - 1.0 <= mid && mid <= b + 1.0) {
            return k;
        }
        let d = |i: usize| ((cols[i].0 + cols[i].1) / 2.0 - mid).abs();
        let mut best = 0;
        for i in 1..cols.len() {
            if d(i) < d(best) {
                best = i;
            }
        }
        best
    }

    #[test]
    fn column_lookup_matches_linear_scan() {
        let mut seed: u64 = 0x9e3779b97f4a7c15;
        let mut next = |m: u64| {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed % m
        };
        for _ in 0..4000 {
            // ordered columns: touching (ruled), 1-2 point gaps (slack overlaps), zero width,
            // and duplicates of the same interval
            let n = 1 + next(8) as usize;
            let mut x = next(50) as f64;
            let mut cols = Vec::new();
            for _ in 0..n {
                let w = [0.0, 0.5, 3.0, 10.0][next(4) as usize];
                cols.push((x, x + w));
                x += w + [0.0, 0.0, 1.0, 1.5, 2.0, 7.0][next(6) as usize];
            }
            let lookup = ColumnLookup::new(&cols);
            assert!(lookup.centers.is_some());
            for q in 0..40 {
                let mid = if q == 0 { f64::NAN } else { next(800) as f64 / 8.0 - 10.0 };
                assert_eq!(lookup.column(mid), reference(&cols, mid), "{cols:?} {mid}");
            }
            // far away, distances round to ties: the first of the tied columns wins
            for mid in [2e20, -2e20, 1e300, -1e300, f64::MAX, f64::MIN] {
                assert_eq!(lookup.column(mid), reference(&cols, mid), "{cols:?} {mid}");
            }
            // huge columns, where neighbouring centres and 1-point slack round together
            let big: Vec<(f64, f64)> = cols.iter().map(|&(a, b)| (a * 1e15, b * 1e15)).collect();
            let lookup = ColumnLookup::new(&big);
            for _ in 0..20 {
                let mid = (next(800) as f64 / 8.0 - 10.0) * 1e15;
                assert_eq!(lookup.column(mid), reference(&big, mid), "{big:?} {mid}");
            }
            // unordered lists take the scans
            let mut rev = cols.clone();
            rev.reverse();
            let lookup = ColumnLookup::new(&rev);
            let mid = next(800) as f64 / 8.0;
            assert_eq!(lookup.column(mid), reference(&rev, mid));
        }
    }
}
