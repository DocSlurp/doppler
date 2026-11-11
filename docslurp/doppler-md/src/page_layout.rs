//! Assignment and reading order for one page, before Markdown serialization.
//!
//! Geometry carries owned table placements. Replacing member lines consumes a collection;
//! each overlapping region gets its own collection, so no membership flags outlive a region.

use std::collections::HashSet;
use std::rc::Rc;

use crate::TableFormat;
use crate::images::Figure;
use crate::layout::{Budget, Elem, regions_within};
use crate::model::{Line, Page, Word};
use crate::params::{Params, UseBlocks};
use crate::patterns::data_cell;
use crate::prose::{Paragraph, SENT_END, Seq, SeqItem, group_within};
use crate::pyc::{Counter, char_len, fcmp, median, pmax, pmin, rstrip, split_count, strip};
use crate::stitch::{Carry, stitch};
use crate::tables::{Table, find_tables_within, grid_html, grid_markdown_with, ruled_tables};

#[derive(Clone, Debug)]
pub(crate) struct PlacedTable {
    pub table: Rc<Table>,
    pub x0: f64,
    pub x1: f64,
    pub y0: f64,
    /// the bottom edge (a gutter table's last line, a ruled table's last rule)
    pub y1: f64,
    pub header: Option<Vec<String>>,
}

pub(crate) enum FlowItem {
    Paragraph(Paragraph),
    Table(PlacedTable),
    /// a rotated region, already laid out and rendered in its own frame
    Rendered(Rc<Vec<String>>),
}

pub(crate) enum Block {
    /// `small`: set in print smaller than the body's (by `aside_size`); `lines`: how many
    Text {
        text: String,
        heading: u8,
        list: bool,
        caption: bool,
        small: bool,
        lines: usize,
    },
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
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn within(
        page: &Page,
        drop: &[bool],
        body: f64,
        body_color: &str,
        p: &Params,
        format: TableFormat,
        budget: &mut Budget,
        figures: &[Figure],
    ) -> Option<Self> {
        // the labels of a vector figure are written after its marker, not in the flow
        let labelled: HashSet<usize> = figures.iter().flat_map(|f| f.labels.iter().copied()).collect();
        let ids: Vec<usize> = (0..page.lines.len()).filter(|&i| !drop[i] && !labelled.contains(&i)).collect();
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
            let sub = PageLayout::within(&g.page, &no_drop, body, body_color, p, format, budget, &[])?;
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
                    let table = PlacedTable { table: Rc::new(table), x0, x1, y0, y1, header: None };
                    (members, Elem { line: None, table: Some(Placed::Table(Rc::new(table))), x0, x1, y0, y1 })
                },
            );
            elements = replace_members(elements, tables);
        }
        elements.extend(rendered);
        // a picture between paragraphs: a barrier of its own, ordered by where it lies
        elements.extend(figures.iter().map(|f| Elem {
            line: None,
            table: Some(Placed::Rendered(Rc::new(figure_parts(page, f)))),
            x0: f.x0,
            x1: f.x1,
            y0: f.y0,
            y1: f.y1,
        }));
        let mut blocks: Vec<Block> = Vec::new();
        let mut tail: Option<Tail> = None;
        for region in regions_within(&elements, &page.lines, body, p, 0, budget)? {
            // A narrow gutter can place the same line in two regions. Each region assigns
            // its own copy: a table in the left region cannot consume right-region prose.
            let seq = assign_region(page, region, body, p, budget)?;
            let mut items = group_within(&seq, body, body_color, p, use_blocks, budget)?;
            let interrupted = p.join_interrupted_paragraphs;
            if interrupted {
                join_within_column(&mut items, body, p, budget)?;
            }
            let open = tail.take();
            let folded = if p.join_column_breaks {
                join_column_break(&mut blocks, open.as_ref(), &mut items, body, p)
            } else {
                None
            };
            if items.is_empty() {
                // the whole region continued the paragraph before it, which may run on
                tail = folded;
                continue;
            }
            let carried = if interrupted && p.join_column_breaks { folded.or(open) } else { None };
            // a region of asides alone (figure labels, a picture, a caption) leaves the paragraph
            // before it open: its blocks go after that paragraph's
            if let Some(t) = &carried
                && let Some(strong) = asides(&items, reference(t.last.size(), body), p)
            {
                blocks.extend(finish_region(items, p));
                tail = carried.map(|t| Tail { interrupted: true, strong: t.strong || strong, ..t });
                continue;
            }
            let last = if interrupted { region_tail(&items, body, p) } else { plain_tail(&items) };
            // the blocks of the asides after the paragraph
            let after = last.as_ref().map_or(0, |(k, _)| items.len() - 1 - k);
            // a region beside the open paragraph's column (another column's text, a figure's)
            // that ends in nothing that could go on leaves it open too
            let beside = match &carried {
                Some(t) if last.as_ref().is_none_or(|(_, own)| own.lines < 2) => beside(&items, t, budget)?,
                _ => false,
            };
            blocks.extend(finish_region(items, p));
            if beside {
                tail = carried.map(|t| Tail { beside: true, ..t });
                continue;
            }
            // the paragraph and the asides after it are the last blocks: a paragraph is only
            // absorbed into a table that follows it
            tail = last.and_then(|(_, t)| {
                let block = blocks.len().checked_sub(1 + after)?;
                matches!(blocks[block], Block::Text { heading: 0, list: false, caption: false, .. })
                    .then_some(Tail { block, ..t })
            });
        }
        Some(Self { blocks })
    }

    /// Mark what this page continues from the previous one (see [`crate::stitch`]) and say what it
    /// leaves open.
    pub(crate) fn stitch(&mut self, prev: &Carry, page_height: f64, p: &Params, seal: &str) -> Carry {
        stitch(&mut self.blocks, prev, page_height, p, seal)
    }

    pub(crate) fn markdown(self, tables: TableFormat, math: bool) -> String {
        render_blocks(self.blocks, tables, math)
            .into_iter()
            .filter(|s| !strip(s).is_empty())
            .collect::<Vec<_>>()
            .join("\n\n")
    }
}

/// A figure's marker, then the text of its labels (top to bottom, left to right) as one
/// paragraph.
fn figure_parts(page: &Page, f: &Figure) -> Vec<String> {
    let mut parts = vec![f.marker.to_string()];
    let mut labels: Vec<&Line> = f.labels.iter().filter_map(|&i| page.lines.get(i).map(|l| &**l)).collect();
    labels.sort_by(|a, b| crate::pyc::fcmp(a.y0 + a.y1, b.y0 + b.y1).then(crate::pyc::fcmp(a.x0, b.x0)));
    let text: Vec<&str> = labels.iter().map(|l| strip(l.text())).filter(|t| !t.is_empty()).collect();
    if !text.is_empty() {
        parts.push(text.join(" "));
    }
    parts
}

/// The last paragraph of a region, which the next region's first paragraph may continue.
struct Tail {
    /// index of its block in the page's blocks (set once the block exists)
    block: usize,
    /// its last line, and how many lines it has
    last: Rc<Line>,
    lines: usize,
    /// the left and right edges of its lines
    left: f64,
    right: f64,
    /// the lowest edge of its column: its last line, or the asides below it
    bottom: f64,
    /// asides (pictures, smaller print) come between it and whatever continues it
    interrupted: bool,
    /// and one of them shows a figure or a footnote, not just smaller print
    strong: bool,
    /// regions beside its column come between it and whatever continues it
    beside: bool,
}

/// Whether all of a region's lines and tables lie left or right of the paragraph `t`'s column
/// (charging the lines visited).
fn beside(items: &[FlowItem], t: &Tail, budget: &mut Budget) -> Option<bool> {
    let out = |x0: f64, x1: f64| x1 <= t.left || x0 >= t.right;
    for item in items {
        let all = match item {
            FlowItem::Paragraph(para) => {
                if !budget.spend(para.lines.len()) {
                    return None;
                }
                para.lines.iter().all(|l| out(l.x0, l.x1))
            }
            FlowItem::Table(tab) => out(tab.x0, tab.x1),
            FlowItem::Rendered(_) => false,
        };
        if !all {
            return Some(false);
        }
    }
    Some(true)
}

/// The lowest edge of the lines and tables of a region's items (pictures have no extent here).
fn bottom_of(items: &[FlowItem]) -> f64 {
    items
        .iter()
        .map(|item| match item {
            FlowItem::Paragraph(para) => para.lines.iter().map(|l| l.y1).fold(f64::NEG_INFINITY, pmax),
            FlowItem::Table(t) => t.y1,
            FlowItem::Rendered(_) => f64::NEG_INFINITY,
        })
        .fold(f64::NEG_INFINITY, pmax)
}

fn plain(para: &Paragraph) -> bool {
    para.level == 0 && para.kind.is_none() && !para.lines.is_empty()
}

/// The region's last item when it is a plain paragraph, with its index.
fn plain_tail(items: &[FlowItem]) -> Option<(usize, Tail)> {
    match items.last()? {
        FlowItem::Paragraph(para) if plain(para) => Tail::of(para).map(|t| (items.len() - 1, t)),
        _ => None,
    }
}

impl Tail {
    fn of(para: &Paragraph) -> Option<Tail> {
        let last = para.lines.last()?.clone();
        let right = para.lines.iter().map(|l| l.x1).fold(f64::NEG_INFINITY, pmax);
        let bottom = last.y1;
        let left = left_of(&para.lines);
        Some(Tail {
            block: 0,
            last,
            lines: para.lines.len(),
            left,
            right,
            bottom,
            interrupted: false,
            strong: false,
            beside: false,
        })
    }
}

/// An end of sentence: a paragraph ending in one is not cut short.
const TERMINAL: [char; 9] = ['.', '!', '?', ':', ';', '"', '”', '’', '…'];

/// The text evidence that `next` continues the paragraph `text` whose last line is `a` (of `lines`
/// lines, running to `right`): the earlier one has no end of sentence and its last line runs to
/// its right edge (the line was cut by the layout, not by the paragraph's end); the later one
/// starts in lowercase in the same size, weight and colour; both are plain paragraphs (no
/// heading, list or caption).
fn continues(text: &str, a: &Line, lines: usize, right: f64, next: &Paragraph, p: &Params) -> bool {
    let Some(b) = next.lines.first() else { return false };
    let size = pmax(a.size(), 1.0);
    plain(next)
        && strip(&next.text).chars().next().is_some_and(crate::pyc::is_lower)
        && !rstrip(text).ends_with(TERMINAL)
        && lines >= 2
        && a.x1 >= right - 2.0 * size
        && !crate::prose::style_change(a, b, p)
}

/// Content that can sit inside a paragraph without ending it: a picture, or print smaller than
/// the paragraph's `size` (figure labels, a caption, a footnote). `Some(true)` when the item by
/// itself shows a figure: a picture, or print so small that only figure labels use it.
fn aside(item: &FlowItem, size: f64, p: &Params) -> Option<bool> {
    match item {
        FlowItem::Rendered(parts) => (parts.len() == 1 && crate::images::is_marker(&parts[0])).then_some(true),
        FlowItem::Paragraph(para) if !para.lines.is_empty() => {
            let s = median(para.lines.iter().map(|l| l.size()).collect());
            (s <= p.aside_size * size).then_some(s <= p.aside_tiny * size)
        }
        _ => None,
    }
}

/// A footnote opens with its mark: a number of up to three digits and a space, a symbol, or a
/// superscript digit.
pub(crate) fn footnote_marked(text: &str) -> bool {
    let t = strip(text);
    let digits = t.chars().take_while(char::is_ascii_digit).count();
    if (1..=3).contains(&digits) {
        return t[digits..].starts_with(' ');
    }
    t.starts_with(['*', '†', '‡', '§', '¶', '⁰', '¹', '²', '³', '⁴', '⁵', '⁶', '⁷', '⁸', '⁹'])
}

/// Whether `items` are all asides of a paragraph of `size`, and if so whether one is strong
/// evidence of an interruption.
fn asides(items: &[FlowItem], size: f64, p: &Params) -> Option<bool> {
    let mut strong = false;
    for item in items {
        strong |= aside(item, size, p)?;
    }
    Some(strong)
}

/// The region's earliest plain paragraph that only asides follow (their blocks will follow its
/// block), with its index in `items`. There, at the bottom of the column, a footnote is strong
/// evidence too. One pass from the end, keeping the extremes of the sizes below.
fn region_tail(items: &[FlowItem], body: f64, p: &Params) -> Option<(usize, Tail)> {
    let mut best: Option<(usize, Tail)> = None;
    let (mut largest, mut smallest, mut bottom) = (f64::NEG_INFINITY, f64::INFINITY, f64::NEG_INFINITY);
    let (mut picture, mut footnote, mut rest) = (false, false, false);
    for (k, item) in items.iter().enumerate().rev() {
        match item {
            FlowItem::Paragraph(para) if !para.lines.is_empty() => {
                if plain(para)
                    && let Some(t) = Tail::of(para)
                {
                    let size = reference(t.last.size(), body);
                    if largest > p.aside_size * size {
                        break;
                    }
                    let t = if rest {
                        let strong = picture || footnote || smallest <= p.aside_tiny * size;
                        Tail { bottom: pmax(t.bottom, bottom), interrupted: true, strong, ..t }
                    } else {
                        t
                    };
                    best = Some((k, t));
                }
                let s = median(para.lines.iter().map(|l| l.size()).collect());
                largest = pmax(largest, s);
                smallest = pmin(smallest, s);
                bottom = para.lines.iter().map(|l| l.y1).fold(bottom, pmax);
                footnote |= footnote_marked(&para.text);
            }
            FlowItem::Rendered(_) if aside(item, 1.0, p).is_some() => picture = true,
            _ => break,
        }
        rest = true;
    }
    best
}

/// The size asides are measured against: the paragraph's, but never more than the body's (the
/// body text after a large-print paragraph does not interrupt it).
fn reference(size: f64, body: f64) -> f64 {
    pmax(pmin(size, body), 1.0)
}

/// A paragraph that runs down a column, is interrupted by a figure (a picture, its labels, its
/// caption) and goes on below it in the same column is one paragraph, followed by the figure.
/// The evidence is that of [`continues`], strong asides between the two halves (see [`asides`]),
/// and the two halves share the column: the same left and right edges, and the later one starts
/// at the left edge, below the earlier one.
fn join_within_column(items: &mut Vec<FlowItem>, body: f64, p: &Params, budget: &mut Budget) -> Option<()> {
    let mut i = 0;
    while i < items.len() {
        let FlowItem::Paragraph(a) = &items[i] else {
            i += 1;
            continue;
        };
        let Some(la) = a.lines.last().filter(|_| plain(a)) else {
            i += 1;
            continue;
        };
        let size = pmax(la.size(), 1.0);
        let small = reference(la.size(), body);
        let mut j = i + 1;
        let mut strong = false;
        while let Some(s) = items.get(j).and_then(|it| aside(it, small, p)) {
            strong |= s;
            j += 1;
        }
        // each aside visited (none on pages without asides, so their work is unchanged)
        if j > i + 1 && !budget.spend(j - i - 1) {
            return None;
        }
        let joins = strong
            && j > i + 1
            && match items.get(j) {
                Some(FlowItem::Paragraph(b)) => same_column_below(a, b, size, p),
                _ => false,
            };
        if !joins {
            i = j;
            continue;
        }
        let FlowItem::Paragraph(b) = items.remove(j) else { return None };
        if let FlowItem::Paragraph(a) = &mut items[i] {
            crate::prose::append_line(&mut a.text, strip(&b.text), p);
            a.lines.extend(b.lines);
        }
        // the joined paragraph may be interrupted again
    }
    Some(())
}

fn same_column_below(a: &Paragraph, b: &Paragraph, size: f64, p: &Params) -> bool {
    let Some(la) = a.lines.last() else { return false };
    let right = a.lines.iter().map(|l| l.x1).fold(f64::NEG_INFINITY, pmax);
    continues(&a.text, la, a.lines.len(), right, b, p) && below_in_column(left_of(&a.lines), right, la, b, size)
}

fn left_of(lines: &[Rc<Line>]) -> f64 {
    lines.iter().map(|l| l.x0).fold(f64::INFINITY, pmin)
}

/// `b` starts below `last`, the last line of a paragraph spanning `left..right`, at its left
/// edge, and keeps within its edges: the same column.
fn below_in_column(left: f64, right: f64, last: &Line, b: &Paragraph, size: f64) -> bool {
    let Some(fb) = b.lines.first() else { return false };
    let lb0 = left_of(&b.lines);
    let lb1 = b.lines.iter().map(|l| l.x1).fold(f64::NEG_INFINITY, pmax);
    // the column's later lines may start left of its earlier ones, under a margin label
    fb.y0 > last.y1
        && lb0 >= left - 4.0 * size
        && lb0 <= left + 2.0 * size
        && fb.x0 <= left + size
        && lb1 <= right + 2.0 * size
        && (b.lines.len() == 1 || (right - lb1).abs() <= 2.0 * size)
}

/// A paragraph that runs to the bottom of one column and goes on at the top of the next was
/// split by the column break. When the first paragraph of the region just grouped continues
/// the last one of the region before it, fold it into that block (dropping it from `items`),
/// and return what the folded paragraph leaves as the tail.
///
/// The evidence is that of [`continues`]; the later one starts higher up the page; and the
/// earlier column goes down as far as the later region's content does (within three lines),
/// as a column does that runs out of room, not one that something else cut short. Asides
/// (see [`asides`]) may come between: below the earlier paragraph, in regions of their own,
/// or above the later one; they stay where they are, after the joined paragraph, and then one
/// of them must be strong evidence. Past asides the later paragraph may also go on below the
/// earlier one in its column (asides that span the page cut the column into regions).
fn join_column_break(
    blocks: &mut [Block],
    tail: Option<&Tail>,
    items: &mut Vec<FlowItem>,
    body: f64,
    p: &Params,
) -> Option<Tail> {
    let t = tail?;
    let a = &t.last;
    let size = pmax(a.size(), 1.0);
    let small = reference(a.size(), body);
    // leading asides only when asides may interrupt
    let k = if p.join_interrupted_paragraphs { items.iter().position(|it| aside(it, small, p).is_none())? } else { 0 };
    let lead = asides(&items[..k], small, p)?;
    let Some(FlowItem::Paragraph(next)) = items.get(k) else { return None };
    let Block::Text { text, .. } = &blocks[t.block] else { return None };
    let b = next.lines.first()?;
    let interrupted = t.interrupted || k > 0;
    let strong = !interrupted || t.strong || lead;
    let below = p.join_interrupted_paragraphs && below_in_column(t.left, t.right, a, next, size);
    // a column break; asides that cut the column into regions above and below them; or the
    // next line of the column, which a region boundary (another column's text beside it) cut off
    let geometry = (!t.beside && b.y0 < a.y0 && t.bottom >= bottom_of(items) - 3.0 * size && strong)
        || (interrupted && below && strong)
        || (below && b.y0 - a.y1 <= 0.5 * size);
    let continues = continues(text, a, t.lines, t.right, next, p) && geometry;
    if !continues {
        return None;
    }
    let FlowItem::Paragraph(next) = items.remove(k) else { return None };
    if let Block::Text { text, .. } = &mut blocks[t.block] {
        crate::prose::append_line(text, strip(&next.text), p);
    }
    // the paragraph may run on into a third column
    Tail::of(&next).map(|folded| Tail { block: t.block, ..folded })
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
            let y1 = members.iter().map(|&i| page.lines[i].y1).fold(f64::NEG_INFINITY, pmax);
            let table = PlacedTable { table: Rc::new(grid), x0, x1, y0, y1, header: None };
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
                Some(Placed::Table(t)) => Seq::Table { table: t.table.clone(), x0: t.x0, x1: t.x1, y1: t.y1 },
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
                    let body_follows = matches!(items.peek(), Some(FlowItem::Paragraph(n))
                        if n.level == 0 && (n.kind.is_some() || char_len(&n.text) as f64 >= p.minor_heading_body));
                    // a numbered section heading that opens body text after a table ends it: it is
                    // the next section's heading, not the table's caption
                    let section_after_table = p.section_headings_after_tables
                        && previous_table
                        && !matches!(items.peek(), Some(FlowItem::Table(_)))
                        && body_follows
                        && section_number(&it.text);
                    let caption = minor && p.captions && near_table && !section_after_table;
                    let mut heading = it.level;
                    if minor && !caption {
                        if !body_follows {
                            heading = 0;
                        }
                    }
                    let small = it.ratio <= p.aside_size;
                    let lines = it.lines.len();
                    out.push(Block::Text { text: it.text, heading, list: it.kind.is_some(), caption, small, lines });
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
                        if header.is_none() && t.table.header_row && t.table.rows.len() > 1 {
                            grid_html(Some(&t.table.rows[0]), &t.table.rows[1..].to_vec())
                        } else if header.is_none() {
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

/// Text that starts with a multi-level section number (`4.6.3`, `2.1.`): digits with at least one
/// dot between digit groups, then a space.
fn section_number(text: &str) -> bool {
    let first = strip(text).split_whitespace().next().unwrap_or("");
    let core = first.strip_suffix('.').unwrap_or(first);
    let groups: Vec<&str> = core.split('.').collect();
    groups.len() >= 2
        && groups.iter().all(|g| !g.is_empty() && g.len() <= 3 && g.chars().all(|c| c.is_ascii_digit()))
        && strip(text).split_whitespace().nth(1).is_some()
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
