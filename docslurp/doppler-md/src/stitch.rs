//! Content that runs across a page break: a table that goes on, a paragraph that goes on.
//!
//! Pages are written separately (a form feed between them), so a page cannot be merged into
//! the next one. With [`crate::Options::markers`] the page that continues something says so,
//! in an HTML comment on its own line directly above the continued block, and a caller that
//! restitches the pages reads those:
//!
//! - `<!-- doppler:table-continued header=repeated -->`: the table below goes on from the last
//!   table of the previous page, and its first row repeats that table's header row. Drop the
//!   first row (and the separator under it) when merging.
//! - `<!-- doppler:table-continued header=inherited -->`: the same, but the page did not repeat
//!   the header; the first row below is the previous table's header, copied so the page stands
//!   alone. Drop it when merging.
//! - `<!-- doppler:table-continued header=none -->`: it goes on, with no header to copy (the
//!   table is written with spans, or the previous table had none): every row is data.
//! - `<!-- doppler:paragraph-continued -->`: the paragraph below goes on from the last
//!   paragraph of the previous page (which has no end of sentence). Join them with a space,
//!   or without one when the previous page's text ends in a hyphen.

use std::rc::Rc;

use crate::page_layout::{Block, PlacedTable};
use crate::params::Params;
use crate::pyc::{pmax, rstrip, strip};

/// Where the last table of a page was, and the header of its columns.
#[derive(Clone, Debug)]
pub(crate) struct TableEdge {
    cols: Vec<(f64, f64)>,
    header: Vec<String>,
    bottom: f64,
    page_height: f64,
}

/// What the previous page left open.
#[derive(Clone, Debug, Default)]
pub(crate) struct Carry {
    table: Option<TableEdge>,
    /// the page ends in a plain paragraph with no end of sentence
    open_paragraph: bool,
}

const TABLE_CONTINUED: &str = "<!-- doppler:table-continued";
const PARAGRAPH_CONTINUED: &str = "<!-- doppler:paragraph-continued -->";

/// Cells compared as text: case, spacing and trailing punctuation do not tell headers apart.
fn norm(cell: &str) -> String {
    cell.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase()
}

fn same_header(a: &[String], b: &[String]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let filled: Vec<(String, String)> =
        a.iter().zip(b).map(|(x, y)| (norm(x), norm(y))).filter(|(x, y)| !x.is_empty() || !y.is_empty()).collect();
    !filled.is_empty() && filled.iter().filter(|(x, y)| x == y).count() * 5 >= filled.len() * 4
}

/// Columns that sit in the same places: as many, and each shares a left edge, a right edge
/// or a centre with its partner, within a tolerance of the width. (A column's extent is that
/// of the words in it, so another page with shorter cells moves one of its edges.)
fn same_columns(a: &[(f64, f64)], b: &[(f64, f64)]) -> bool {
    if a.len() != b.len() || a.is_empty() {
        return false;
    }
    let width = pmax(a[a.len() - 1].1 - a[0].0, 1.0);
    let tol = pmax(8.0, 0.06 * width);
    a.iter().zip(b).all(|(x, y)| {
        (x.0 - y.0).abs() <= tol || (x.1 - y.1).abs() <= tol || ((x.0 + x.1) - (y.0 + y.1)).abs() <= 2.0 * tol
    })
}

fn is_continued_note(text: &str) -> bool {
    let t = text.to_lowercase();
    t.chars().count() <= 100 && (t.contains("continued") || t.contains("(cont") || t.contains("cont'd"))
}

fn header_of(t: &PlacedTable) -> Option<Vec<String>> {
    t.header.clone().or_else(|| t.table.rows.first().cloned())
}

fn open_paragraph(blocks: &[Block]) -> bool {
    match blocks.last() {
        Some(Block::Text { text, heading: 0, list: false, caption: false }) => {
            let t = rstrip(text);
            t.chars().count() >= 40 && !t.ends_with(['.', '!', '?', ':', ';', '"', '”', '’', '…', ')'])
        }
        _ => false,
    }
}

/// Mark what `blocks` (a page's, in reading order) continues from `prev`, and return what this
/// page leaves open for the next one.
pub(crate) fn stitch(blocks: &mut Vec<Block>, prev: &Carry, page_height: f64, p: &Params) -> Carry {
    // the first block, past a leading "(continued)" note on its own
    let mut first = 0;
    if matches!(blocks.first(), Some(Block::Text { text, .. }) if is_continued_note(text)) {
        first = 1;
    }
    let mut marker: Option<(usize, String)> = None;
    if let Some(edge) = &prev.table
        && let Some(Block::Table(t)) = blocks.get_mut(first)
        && t.header.is_none()
        && t.y0 <= 0.35 * page_height
        && edge.bottom >= 0.6 * edge.page_height
        && same_columns(&edge.cols, &t.table.cols)
    {
        let repeated = t.table.rows.first().is_some_and(|row| same_header(row, &edge.header));
        let how = if repeated {
            "repeated"
        } else if t.table.spans.is_none()
            && edge.header.len() == t.table.cols.len()
            && !edge.header.iter().all(|c| strip(c).is_empty())
        {
            t.header = Some(edge.header.clone());
            "inherited"
        } else {
            "none"
        };
        marker = Some((first, format!("{TABLE_CONTINUED} header={how} -->")));
    } else if prev.open_paragraph
        && let Some(Block::Text { text, heading: 0, list: false, caption: false }) = blocks.get(first)
        && strip(text).chars().next().is_some_and(crate::pyc::is_lower)
    {
        marker = Some((first, PARAGRAPH_CONTINUED.to_string()));
    }
    if let Some((at, text)) = marker {
        blocks.insert(at, Block::Rendered(Rc::new(vec![text])));
    }
    let _ = p;
    let table = match blocks.last() {
        Some(Block::Table(t)) => {
            header_of(t).map(|header| TableEdge { cols: t.table.cols.clone(), header, bottom: t.y1, page_height })
        }
        _ => None,
    };
    Carry { table, open_paragraph: open_paragraph(blocks) }
}
