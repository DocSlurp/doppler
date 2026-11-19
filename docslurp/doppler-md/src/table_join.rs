//! Bounded, opt-in joining of tables across page slots.
use std::ops::Range;
use std::rc::Rc;

use crate::layout::Budget;
use crate::page_layout::{Block, PageLayout, PlacedTable};
use crate::tables::numeric;

/// A joined section, in emitted document page coordinates.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JoinedTable {
    pub start_page: u32,
    pub end_page: u32,
    pub rows: usize,
}

pub(crate) struct JoinPage {
    pub index: usize,
    pub number: u32,
    pub width: f64,
    pub height: f64,
    pub layout: PageLayout,
    pub notes: Range<usize>,
    pub scan_marker: Option<String>,
}

pub(crate) struct TableJoiner {
    pages: Vec<JoinPage>,
    tail: usize,
    max_pages: usize,
    reports: Vec<JoinedTable>,
    dropped: usize,
}

const MAX_REPORTS: usize = 100_000;
const MAX_ROWS: usize = 4096;
const MAX_CELL_BYTES: usize = 262_144;

fn continuation(text: &str) -> bool {
    let text = text.trim().to_ascii_lowercase();
    matches!(text.as_str(), "(continued)" | "continued" | "continued on next page")
        || (text.starts_with("table ") && (text.ends_with("(continued)") || text.ends_with("(cont.)")))
}

fn continuation_block(block: &Block) -> bool {
    matches!(block, Block::Text { text, lines: 1, .. } if text.len() <= 100 && continuation(text))
}

fn continuation_header(table: &PlacedTable) -> bool {
    let Some(header) = &table.header else { return false };
    let mut filled = header.iter().map(|s| s.trim()).filter(|s| !s.is_empty());
    filled.next().is_some_and(continuation) && filled.next().is_none()
}

fn start(page: &JoinPage) -> Option<usize> {
    let blocks = &page.layout.blocks;
    let index = usize::from(blocks.first().is_some_and(continuation_block));
    match blocks.get(index)? {
        Block::Table(t) if t.y0 <= 0.35 * page.height => Some(index),
        _ => None,
    }
}

fn end(page: &JoinPage) -> Option<usize> {
    let blocks = &page.layout.blocks;
    let skip = usize::from(blocks.last().is_some_and(continuation_block));
    let i = blocks.len().checked_sub(1 + skip)?;
    match &blocks[i] {
        Block::Table(t) if t.y1 >= 0.6 * page.height => Some(i),
        _ => None,
    }
}

/// Align one table with another only when each smaller column has one unambiguous,
/// ordered counterpart. Equal widths require matching columns in order.
fn column_map(small: &[(f64, f64)], wide: &[(f64, f64)]) -> Option<Vec<usize>> {
    fn matches((a, b): (f64, f64), (x, y): (f64, f64)) -> bool {
        (a - x).abs() <= 4.0 || (b - y).abs() <= 4.0 || ((a + b) - (x + y)).abs() <= 8.0
    }
    if !(2..=64).contains(&wide.len()) || small.len() < 2 || small.len() > wide.len() {
        return None;
    }
    if small.len() == wide.len() {
        return small.iter().zip(wide).all(|(&a, &b)| matches(a, b)).then(|| (0..wide.len()).collect());
    }
    let mut indices = Vec::with_capacity(small.len());
    for &column in small {
        let mut candidates = wide.iter().enumerate().filter(|(_, other)| matches(column, **other));
        let (index, _) = candidates.next()?;
        if candidates.next().is_some() || indices.last().is_some_and(|&previous| previous >= index) {
            return None;
        }
        indices.push(index);
    }
    Some(indices)
}

fn pad(row: Vec<String>, map: &[usize], width: usize) -> Vec<String> {
    let mut padded = vec![String::new(); width];
    for (value, &at) in row.into_iter().zip(map) {
        padded[at] = value;
    }
    padded
}

fn same_header(a: &[String], b: &[String]) -> bool {
    !a.is_empty()
        && a.len() == b.len()
        && a.iter().zip(b).all(|(x, y)| {
            let mut x = x.split_whitespace();
            let mut y = y.split_whitespace();
            loop {
                match (x.next(), y.next()) {
                    (None, None) => break true,
                    (Some(s), Some(t)) if s.eq_ignore_ascii_case(t) => {}
                    _ => break false,
                }
            }
        })
}

fn profile(rows: &[Vec<String>], col: usize) -> Option<(bool, bool)> {
    let mut seen = 0;
    let mut numbers = 0;
    let mut decimals = 0;
    for row in rows {
        let Some(cell) = row.get(col).map(|s| s.trim()).filter(|s| !s.is_empty()) else { continue };
        seen += 1;
        if numeric(cell) {
            numbers += 1;
            decimals += usize::from(cell.contains('.'));
        }
    }
    (seen != 0).then_some((numbers * 2 >= seen, decimals > 0))
}

fn table_bytes(table: &PlacedTable) -> usize {
    table.table.rows.iter().flatten().map(|s| s.len()).sum::<usize>()
        + table.header.iter().flatten().map(|s| s.len()).sum::<usize>()
}

struct JoinPlan {
    skip_rows: usize,
    /// Matched columns (index in the wider table of each column in the narrower one).
    columns: Vec<usize>,
    widen_anchor: bool,
}

fn compatible(a: &PlacedTable, b: &PlacedTable, edge_cue: bool, budget: &mut Budget) -> Option<JoinPlan> {
    if a.ruled != b.ruled
        || a.table.spans.is_some()
        || b.table.spans.is_some()
        || a.table.rows.len().saturating_add(b.table.rows.len()) > MAX_ROWS
        || a.table.rows.iter().any(|r| r.len() != a.table.cols.len())
        || b.table.rows.iter().any(|r| r.len() != b.table.cols.len())
        || a.header.as_ref().is_some_and(|r| r.len() != a.table.cols.len())
        || b.header.as_ref().is_some_and(|r| r.len() != b.table.cols.len())
    {
        return None;
    }
    let a_width = a.table.cols.len();
    let b_width = b.table.cols.len();
    let widen_anchor = a_width < b_width;
    let columns = if widen_anchor {
        column_map(&a.table.cols, &b.table.cols)?
    } else {
        column_map(&b.table.cols, &a.table.cols)?
    };
    let bytes = table_bytes(a).saturating_add(table_bytes(b));
    let cells = a_width.max(b_width) * (a.table.rows.len() + b.table.rows.len());
    let work = bytes.saturating_add(cells);
    if bytes > MAX_CELL_BYTES || !budget.spend(work + 1) {
        return None;
    }
    let header = a.header.as_ref().or_else(|| a.table.rows.first())?;
    let headers_match = |new: &[String]| {
        if widen_anchor {
            same_header(&pad(header.clone(), &columns, b_width), new)
        } else if a_width != b_width {
            same_header(header, &pad(new.to_vec(), &columns, a_width))
        } else {
            same_header(header, new)
        }
    };
    let repeated = match &b.header {
        Some(h) if !continuation_header(b) => {
            if !headers_match(h) {
                return None;
            }
            false
        }
        _ => {
            let has_header = a.header.is_some() || a.table.header_row || !header.iter().any(|c| numeric(c));
            has_header && b.table.rows.first().is_some_and(|r| headers_match(r))
        }
    };
    // A separate header above was either matched or explicitly marked as a
    // continuation. Without one, aligned columns alone can belong to unrelated tables.
    if b.header.is_none() && !repeated && !edge_cue {
        return None;
    }
    let skip = usize::from(repeated);
    let left = &a.table.rows[usize::from(a.header.is_none())..];
    let right = &b.table.rows[skip..];
    if right.is_empty()
        || columns.iter().enumerate().any(|(small, &wide)| {
            let (old, new) = if widen_anchor { (small, wide) } else { (wide, small) };
            match (profile(left, old), profile(right, new)) {
                (Some(a), Some(b)) => a != b,
                _ => false,
            }
        })
    {
        return None;
    }
    Some(JoinPlan { skip_rows: skip, columns, widen_anchor })
}

impl TableJoiner {
    pub fn new(max_pages: usize) -> Self {
        Self { pages: Vec::new(), tail: 0, max_pages, reports: Vec::new(), dropped: 0 }
    }

    pub fn push(&mut self, mut page: JoinPage, budget: &mut Budget) -> Vec<JoinPage> {
        let head = start(&page);
        let tail = end(&page);
        let continued = self.pages.last().zip(head).and_then(|(prev, at)| {
            let Block::Table(a) = &self.pages[0].layout.blocks[self.tail] else { unreachable!() };
            let Block::Table(b) = &page.layout.blocks[at] else { unreachable!() };
            let edge_cue = at != 0 || prev.layout.blocks.last().is_some_and(continuation_block);
            (self.pages.len() < self.max_pages
                && prev.number.checked_add(1) == Some(page.number)
                && (prev.width - page.width).abs() <= 0.5
                && (prev.height - page.height).abs() <= 0.5)
                .then(|| compatible(a, b, edge_cue, budget))
                .flatten()
                .map(|skip| (at, skip))
        });
        if let Some((head, plan)) = continued {
            let same_table_at_end = tail == Some(head);
            let Block::Table(table) = page.layout.blocks.remove(head) else { unreachable!() };
            let Block::Table(anchor) = &mut self.pages[0].layout.blocks[self.tail] else { unreachable!() };
            let table = Rc::unwrap_or_clone(table.table);
            let anchor_grid = Rc::make_mut(&mut anchor.table);
            let rows = table.rows.into_iter().skip(plan.skip_rows);
            if plan.widen_anchor {
                let width = table.cols.len();
                for row in &mut anchor_grid.rows {
                    *row = pad(std::mem::take(row), &plan.columns, width);
                }
                anchor.header = anchor.header.take().map(|h| pad(h, &plan.columns, width));
                anchor_grid.cols = table.cols;
                anchor_grid.rows.extend(rows);
            } else if anchor_grid.cols.len() != table.cols.len() {
                let width = anchor_grid.cols.len();
                anchor_grid.rows.extend(rows.map(|r| pad(r, &plan.columns, width)));
            } else {
                anchor_grid.rows.extend(rows);
            }

            let previous = self.pages.last_mut().unwrap();
            if previous.layout.blocks.last().is_some_and(continuation_block) {
                previous.layout.blocks.pop();
            }
            if head != 0 {
                page.layout.blocks.remove(0);
            }
            if same_table_at_end {
                self.pages.push(page);
                return Vec::new();
            }
            self.pages.push(page);
            let mut out = self.flush();
            if tail.is_some()
                && let Some(t) = out.last().and_then(end)
            {
                self.tail = t;
                self.pages.push(out.pop().unwrap());
            }
            return out;
        }

        let mut out = self.flush();
        if let Some(tail) = tail {
            self.tail = tail;
            self.pages.push(page);
        } else {
            out.push(page);
        }
        out
    }

    pub fn flush(&mut self) -> Vec<JoinPage> {
        if self.pages.len() >= 2 {
            let first = &self.pages[0];
            let Block::Table(table) = &first.layout.blocks[self.tail] else { unreachable!() };
            if self.reports.len() < MAX_REPORTS {
                self.reports.push(JoinedTable {
                    start_page: first.number,
                    end_page: self.pages.last().unwrap().number,
                    rows: table.table.rows.len() + usize::from(table.header.is_some()),
                });
            } else {
                self.dropped += 1;
            }
        }
        std::mem::take(&mut self.pages)
    }

    pub fn reports(self) -> (Vec<JoinedTable>, usize) {
        (self.reports, self.dropped)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sparse_columns_require_unique_ordered_matches() {
        let wide = [(10., 30.), (100., 120.), (200., 220.)];
        assert_eq!(column_map(&[(10., 36.), (209., 220.)], &wide), Some(vec![0, 2]));
        assert_eq!(column_map(&[(10., 36.), (130., 152.)], &wide), None);
        assert_eq!(column_map(&[(10., 120.), (200., 220.)], &wide), None);
        assert_eq!(pad(vec!["a".to_string(), "b".to_string()], &[0, 2], 3), ["a", "", "b"]);
    }
}
