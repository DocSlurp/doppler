//! Assignment and reading order for one page, before Markdown serialization.
//!
//! Geometry carries owned table placements. Replacing member lines consumes a collection;
//! each overlapping region gets its own collection, so no membership flags outlive a region.

use std::collections::HashSet;
use std::rc::Rc;

use crate::TableFormat;
use crate::layout::{Budget, Elem, regions_within};
use crate::model::{Page, Word};
use crate::params::{Params, UseBlocks};
use crate::patterns::data_cell;
use crate::prose::{Paragraph, SENT_END, Seq, SeqItem, group_within};
use crate::pyc::{Counter, char_len, fcmp, median, pmax, pmin, rstrip, split_count, strip};
use crate::tables::{Table, find_tables_within, grid_html, grid_markdown_with, ruled_tables};

#[derive(Clone, Debug)]
pub(crate) struct PlacedTable {
    pub table: Rc<Table>,
    pub x0: f64,
    pub x1: f64,
    pub y0: f64,
    pub header: Option<Vec<String>>,
}

pub(crate) enum FlowItem {
    Paragraph(Paragraph),
    Table(PlacedTable),
    /// a rotated region, already laid out and rendered in its own frame
    Rendered(Rc<Vec<String>>),
}

pub(crate) enum Block {
    Text { text: String, heading: u8, list: bool, caption: bool },
    Table(PlacedTable),
    Rendered(Rc<Vec<String>>),
}

/// What a non-line element carries through region splitting.
#[derive(Clone, Debug)]
pub(crate) enum Placed {
    Table(Rc<PlacedTable>),
    Rendered(Rc<Vec<String>>),
}

type Placement = Elem<Option<Placed>>;

/// A page whose content has its final ownership and reading order.
pub(crate) struct PageLayout {
    blocks: Vec<Block>,
}

impl PageLayout {
    /// Return no partial layout if the shared work budget is exhausted.
    pub(crate) fn within(
        page: &Page,
        drop: &[bool],
        body: f64,
        body_color: &str,
        p: &Params,
        format: TableFormat,
        budget: &mut Budget,
    ) -> Option<Self> {
        let ids: Vec<usize> = (0..page.lines.len()).filter(|&i| !drop[i]).collect();
        let mut per_block: Counter<i64> = Counter::new();
        for &i in &ids {
            per_block.add(page.lines[i].block, 1);
        }
        let use_blocks = match p.use_blocks {
            UseBlocks::On => true,
            UseBlocks::Auto => !per_block.is_empty() && median(per_block.counts().map(|c| c as f64).collect()) >= 1.5,
            UseBlocks::Off => false,
        };
        // lines of a rotation read in their own frame are laid out there (sharing the budget)
        // and come back as one rendered element at their upright extent
        let (ids, groups) = crate::rotation::split(page, &ids, p);
        let mut rendered = Vec::with_capacity(groups.len());
        for g in groups {
            let no_drop = vec![false; g.page.lines.len()];
            let sub = PageLayout::within(&g.page, &no_drop, body, body_color, p, format, budget)?;
            let parts: Vec<String> = render_blocks(sub.blocks, format, p.display_math_paragraphs)
                .into_iter()
                .filter(|s| !strip(s).is_empty())
                .collect();
            let item = Some(Placed::Rendered(Rc::new(parts)));
            rendered.push(Elem { line: None, table: item, x0: g.x0, x1: g.x1, y0: g.y0, y1: g.y1 });
        }
        let mut elements: Vec<Placement> = ids
            .iter()
            .map(|&i| {
                let l = &page.lines[i];
                Elem { line: Some(i), table: None, x0: l.x0, x1: l.x1, y0: l.y0, y1: l.y1 }
            })
            .collect();
        if p.rules && p.tables {
            let tables = ruled_tables(page, &ids, body, p, format, budget)?.into_iter().map(
                |(members, table, (x0, y0, x1, y1))| {
                    let table = PlacedTable { table: Rc::new(table), x0, x1, y0, header: None };
                    (members, Elem { line: None, table: Some(Placed::Table(Rc::new(table))), x0, x1, y0, y1 })
                },
            );
            elements = replace_members(elements, tables);
        }
        elements.extend(rendered);
        let mut blocks = Vec::new();
        for region in regions_within(&elements, &page.lines, body, p, 0, budget)? {
            // A narrow gutter can place the same line in two regions. Each region assigns
            // its own copy: a table in the left region cannot consume right-region prose.
            let seq = assign_region(page, region, body, p, budget)?;
            let items = group_within(&seq, body, body_color, p, use_blocks, budget)?;
            blocks.extend(finish_region(items, p));
        }
        Some(Self { blocks })
    }

    pub(crate) fn markdown(self, tables: TableFormat, math: bool) -> String {
        render_blocks(self.blocks, tables, math)
            .into_iter()
            .filter(|s| !strip(s).is_empty())
            .collect::<Vec<_>>()
            .join("\n\n")
    }
}

/// Replace detected members with their table, keeping the untouched elements in input order
/// and the tables in detection order. There is no shared membership state to clear or reuse.
fn replace_members(
    mut elements: Vec<Placement>,
    tables: impl IntoIterator<Item = (Vec<usize>, Placement)>,
) -> Vec<Placement> {
    let mut members = HashSet::new();
    let mut placed = Vec::new();
    for (ids, table) in tables {
        members.extend(ids);
        placed.push(table);
    }
    if !placed.is_empty() {
        elements.retain(|e| e.line.is_none_or(|i| !members.contains(&i)));
        elements.extend(placed);
    }
    elements
}

fn assign_region(
    page: &Page,
    mut region: Vec<Placement>,
    body: f64,
    p: &Params,
    budget: &mut Budget,
) -> Option<Vec<SeqItem>> {
    if p.tables {
        let ids: Vec<usize> = region.iter().filter_map(|e| e.line).collect();
        let tables = find_tables_within(page, &ids, p, body, budget)?.into_iter().map(|(members, grid)| {
            let y0 = members.iter().map(|&i| page.lines[i].y0).fold(f64::INFINITY, pmin);
            let x0 = members.iter().map(|&i| page.lines[i].x0).fold(f64::INFINITY, pmin);
            let x1 = members.iter().map(|&i| page.lines[i].x1).fold(f64::NEG_INFINITY, pmax);
            let table = PlacedTable { table: Rc::new(grid), x0, x1, y0, header: None };
            // Gutter tables sort at x=0 on a tied top edge, while title tests use their
            // actual extent. These placements no longer participate in region splitting.
            (members, Elem { line: None, table: Some(Placed::Table(Rc::new(table))), x0: 0.0, x1, y0, y1: y0 })
        });
        region = replace_members(region, tables);
    }
    // Python's stable ordering starts with ruled tables, then gutter tables, then lines.
    let (tables, lines): (Vec<_>, Vec<_>) = region.into_iter().partition(|e| e.table.is_some());
    let mut seq: Vec<_> = tables
        .into_iter()
        .chain(lines)
        .map(|e| {
            let item = match e.table {
                Some(Placed::Table(t)) => Seq::Table { table: t.table.clone(), x0: t.x0, x1: t.x1 },
                Some(Placed::Rendered(r)) => Seq::Rendered(r),
                None => Seq::Line(page.lines[e.line.expect("line placement")].clone()),
            };
            SeqItem { item, y0: e.y0, x0: e.x0 }
        })
        .collect();
    seq.sort_by(|a, b| fcmp(a.y0, b.y0).then(fcmp(a.x0, b.x0)));
    Some(seq)
}

/// Resolve title ownership before emitting any blocks. Presentation uses the original
/// neighbouring paragraphs, including a title that the next table will own; absorbing a
/// title must not turn an earlier minor heading into a caption beside that table.
pub(crate) fn finish_region(items: Vec<FlowItem>, p: &Params) -> Vec<Block> {
    let mut out = Vec::with_capacity(items.len());
    let mut items = items.into_iter().peekable();
    let mut previous_table = false;
    while let Some(item) = items.next() {
        match item {
            FlowItem::Table(table) => {
                out.push(Block::Table(table));
                previous_table = true;
            }
            FlowItem::Rendered(parts) => {
                out.push(Block::Rendered(parts));
                previous_table = false;
            }
            FlowItem::Paragraph(it) => {
                // a title the columns do not split stays out of a sparse table: one cell
                // holding a whole line is not a header
                let titled = match items.peek_mut() {
                    Some(FlowItem::Table(tab)) if p.table_titles && table_title(&it, tab, p) => {
                        let split = split_header(&it.lines[0].words, &tab.table.cols);
                        (split.is_some() || !tab.table.sparse).then_some((tab, split))
                    }
                    _ => None,
                };
                if let Some((tab, split)) = titled {
                    let n = tab.table.rows[0].len();
                    tab.header = Some(split.unwrap_or_else(|| {
                        let mut header = vec![String::new(); n];
                        header[0] = it.text;
                        header
                    }));
                } else {
                    let near_table = previous_table || matches!(items.peek(), Some(FlowItem::Table(_)));
                    let minor = it.level != 0 && it.ratio < p.heading_ratio;
                    let caption = minor && p.captions && near_table;
                    let mut heading = it.level;
                    if minor && !caption {
                        let body_follows = matches!(items.peek(), Some(FlowItem::Paragraph(n))
                            if n.level == 0 && (n.kind.is_some() || char_len(&n.text) as f64 >= p.minor_heading_body));
                        if !body_follows {
                            heading = 0;
                        }
                    }
                    out.push(Block::Text { text: it.text, heading, list: it.kind.is_some(), caption });
                }
                previous_table = false;
            }
        }
    }
    out
}

pub(crate) fn render_blocks(blocks: Vec<Block>, tables: TableFormat, math: bool) -> Vec<String> {
    blocks
        .into_iter()
        .flat_map(|block| -> Vec<String> {
            let one = match block {
                Block::Rendered(parts) => return parts.to_vec(),
                Block::Table(t) => match (tables, t.header) {
                    (TableFormat::Html, header) => {
                        if header.is_none() {
                            t.table.spans.clone().unwrap_or_else(|| grid_html(None, &t.table.rows))
                        } else {
                            grid_html(header.as_deref(), &t.table.rows)
                        }
                    }
                    (TableFormat::Gfm, None) => grid_markdown_with(&t.table.rows, math),
                    (TableFormat::Gfm, Some(header)) => {
                        let mut rows = Vec::with_capacity(t.table.rows.len() + 1);
                        rows.push(header);
                        rows.extend(t.table.rows.iter().cloned());
                        grid_markdown_with(&rows, math)
                    }
                },
                Block::Text { text, caption: true, .. } => format!("*{text}*"),
                Block::Text { text, heading, .. } if heading != 0 => format!("{} {text}", "#".repeat(heading as usize)),
                Block::Text { text, list: true, .. } => format!("- {text}"),
                Block::Text { text, .. } => text,
            };
            vec![one]
        })
        .collect()
}

/// Header cells when a line's words fall into two or more of the table's columns.
fn split_header(words: &[Word], cols: &[(f64, f64)]) -> Option<Vec<String>> {
    if cols.len() < 2 {
        return None;
    }
    let mut cells: Vec<Vec<Word>> = vec![Vec::new(); cols.len()];
    for w in words {
        let mid = (w.x0 + w.x1) / 2.0;
        let k = cols.iter().position(|&(a, b)| a - 2.0 <= mid && mid <= b + 2.0)?;
        cells[k].push(w.clone());
    }
    if cells.iter().filter(|c| !c.is_empty()).count() < 2 {
        return None;
    }
    Some(
        cells
            .into_iter()
            .map(|ws| {
                if ws.is_empty() { String::new() } else { strip(crate::model::Line::new(ws, 0, 0).text()).to_string() }
            })
            .collect(),
    )
}

fn table_title(it: &Paragraph, tab: &PlacedTable, p: &Params) -> bool {
    let grid = &tab.table.rows;
    if it.kind.is_some()
        || it.lines.len() != 1
        || it.ratio >= p.heading_ratio
        || grid.first().is_none_or(|r| r.len() < 2)
    {
        return false;
    }
    if char_len(&it.text) as f64 > p.title_max_chars
        || split_count(&it.text) as f64 > p.title_max_words
        || rstrip(&it.text).ends_with(SENT_END)
    {
        return false;
    }
    let l = &it.lines[0];
    if !(tab.x0 - 2.0 <= l.x0 && l.x0 <= tab.x1 && tab.y0 - l.y1 <= p.title_gap * pmax(l.size(), 1.0)) {
        return false;
    }
    grid[0].iter().any(|c| !strip(c).is_empty() && data_cell(strip(c)))
}
