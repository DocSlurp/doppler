//! Image markers: where pictures sit in the text, so a caller can put them back.
//!
//! The producer (`pdftotext -images`) says where each visible image lands; the image data is
//! never read. A picture between paragraphs gets a marker paragraph of its own, in reading
//! order, and a small picture inside a line of text gets the marker inside that line, in
//! its place among the words. Both are the Markdown image `![image](doppler-image:pN.M)`,
//! where `N` is the PDF page number and `M` is the picture's position in the producer's list
//! for that page (one-based), so the ids stay the same whatever the converter decides about
//! its neighbours. [`Note`] says what happened to each image.

use std::rc::Rc;

use crate::layout::Budget;
use crate::model::{Line, Page, Word};
use crate::params::Params;
use crate::pyc::{fcmp, pmax, pmin};

/// The URL scheme of a marker.
pub const SCHEME: &str = "doppler-image:";

/// The marker of picture `index` (one-based) on page `page`.
pub fn marker(page: u32, index: usize) -> String {
    sealed(page, &index.to_string(), "")
}

/// A marker for picture id `p{page}.{id}` with `seal` after its `doppler` (see [`seal`]).
pub(crate) fn sealed(page: u32, id: &str, seal: &str) -> String {
    format!("![image](doppler{seal}-image:p{page}.{id})")
}

/// Whether a rendered part is a picture marker (sealed or not).
pub(crate) fn is_marker(part: &str) -> bool {
    part.starts_with("![image](doppler") && part.ends_with(')') && part.contains("-image:p")
}

/// A seal for one conversion: four private-use characters (plane 15) drawn from 64 bits of the
/// process's random hash keys. The converter's own markers carry it between `doppler` and the
/// `:` or `-image:` that follows, so no text of the PDF (which cannot know it) can pass for one;
/// [`unseal`] removes it from each page's Markdown, so the markers come out as documented.
pub(crate) fn seal() -> String {
    use std::hash::{BuildHasher, Hasher};
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_u64(0x0064_6f70_706c_6572);
    let bits = h.finish();
    (0..4)
        .map(|k| char::from_u32(0xF0000 + ((bits >> (16 * k)) & 0xFFFF) as u32 % 0xFFFE).unwrap_or('\u{F0000}'))
        .collect()
}

/// A page's Markdown as written: every `doppler:` and `doppler-image:` that does not carry
/// `seal` (text of the PDF, however it was joined: glued words, a hyphen across lines, cells,
/// math rebuilt from glyphs; any case) gets a zero-width space after its `doppler`, then the
/// seal goes, leaving the converter's own markers exactly as documented.
pub(crate) fn unseal(md: &str, seal: &str) -> String {
    let bytes = md.as_bytes();
    let mut out = String::with_capacity(md.len());
    let mut from = 0;
    let at = |i: usize, s: &str| bytes.len() >= i + s.len() && bytes[i..i + s.len()].eq_ignore_ascii_case(s.as_bytes());
    let mut i = 0;
    while i + 7 <= bytes.len() {
        if at(i, "doppler") && (at(i + 7, ":") || at(i + 7, "-image:")) {
            out.push_str(&md[from..i + 7]);
            out.push('\u{200b}');
            from = i + 7;
            i += 7;
        } else {
            i += 1;
        }
    }
    out.push_str(&md[from..]);
    if seal.is_empty() { out } else { out.replace(seal, "") }
}

/// What became of an image.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fate {
    /// a marker paragraph between paragraphs
    Block,
    /// a marker inside a line of text
    Inline,
    /// covers the page under its text (a scan with a text layer): no marker
    Background,
    /// wholly inside the header or footer band: no marker
    Furniture,
    /// the same backdrop picture at the same place on enough pages: no marker
    Watermark,
    /// below the size floors: no marker
    Small,
    /// drawn again at the same place: no marker
    Duplicate,
    /// text-sized, among text, on a page full of them (text drawn as pictures): no marker
    Fragment,
    /// one of several neighbouring images marked together, at the first of them (`group`)
    Grouped,
    /// vector drawing among a mass of words (a form, a callout of prose): no marker
    Text,
}

impl Fate {
    pub fn as_str(self) -> &'static str {
        match self {
            Fate::Block => "block",
            Fate::Inline => "inline",
            Fate::Background => "background",
            Fate::Furniture => "furniture",
            Fate::Watermark => "watermark",
            Fate::Small => "small",
            Fate::Duplicate => "duplicate",
            Fate::Fragment => "fragment",
            Fate::Grouped => "grouped",
            Fate::Text => "text",
        }
    }
}

/// One image of the report.
#[derive(Clone, Debug)]
pub struct Note {
    pub id: String,
    pub page: u32,
    pub bbox: (f64, f64, f64, f64),
    pub px: (u32, u32),
    pub mask: bool,
    /// vector drawing (`<graphic>` clusters), not an image
    pub vector: bool,
    pub fate: Fate,
    /// images marked together at this one (1 when alone); its box is then the union of theirs
    pub members: usize,
    /// for a grouped image, the id of the image that stands for the group
    pub group: Option<String>,
    /// the image XObject (`ref="num gen"` from the producer), when known
    pub obj: Option<(u32, u32)>,
    /// lines of text lie inside the box (a crop of it holds labels: encode it losslessly)
    pub text: bool,
    /// the file written for it by image extraction (`pdftomarkdown --images-dir`)
    pub file: Option<String>,
    /// its page ran out of layout work and was written as plain lines: a block marker then
    /// follows the page's text instead of sitting in reading order
    pub degraded: bool,
}

/// A picture that is its own paragraph: the marker and the box it fills.
#[derive(Clone, Debug)]
pub struct Figure {
    pub marker: Rc<str>,
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
    /// lines of the page drawn inside the figure (labels of a vector figure), written after
    /// the marker instead of in the page's flow
    pub labels: Vec<usize>,
}

/// Decide each image of `page`. Neighbouring images (within about a line of each other, one
/// after another) are one figure: a diagram or a chart assembled from pieces, or a line of
/// words drawn as pictures. A figure is marked once, at its representative: the first of its
/// images. Inline pictures are put into their lines of `page` now; the figures that stand
/// alone come back for the layout to order. The search for each figure's line and its text is
/// charged to `budget`; once that runs out, figures are marked between paragraphs and not
/// checked for text.
#[allow(clippy::too_many_arguments)]
pub fn place(
    page: &mut Page,
    drop: &[bool],
    body: f64,
    p: &Params,
    number: u32,
    watermarks: &[bool],
    seal: &str,
    budget: &mut Budget,
) -> (Vec<Figure>, Vec<Note>) {
    let images = std::mem::take(&mut page.images);
    let mut figures = Vec::new();
    let mut notes: Vec<Note> = Vec::with_capacity(images.len());
    let area = pmax(page.width * page.height, 1.0);
    let band = p.furniture_band * page.height;
    let kept_text = page.lines.iter().zip(drop).any(|(l, d)| !*d && !l.text().trim().is_empty());
    let mut seen: Vec<(f64, f64, f64, f64)> = Vec::new();
    // images that may get a marker: the others are settled here
    let mut candidates: Vec<usize> = Vec::new();
    for (i, im) in images.iter().enumerate() {
        let (w, h) = (im.x1 - im.x0, im.y1 - im.y0);
        let floor = if im.mask { p.image_min_mask } else { p.image_min_size };
        let bbox = (im.x0, im.y0, im.x1, im.y1);
        // a page background comes first: a scan repeats its page image on every page, which is
        // not a watermark
        let fate = if w * h >= p.image_background_share * area && kept_text {
            Some(Fate::Background)
        } else if watermarks.get(i).copied().unwrap_or(false) {
            Some(Fate::Watermark)
        } else if pmin(w, h) < floor {
            Some(Fate::Small)
        } else if seen.iter().any(|s| same_box(*s, bbox)) {
            Some(Fate::Duplicate)
        } else if im.y1 <= band || im.y0 >= page.height - band {
            Some(Fate::Furniture)
        } else {
            seen.push(bbox);
            candidates.push(i);
            None
        };
        notes.push(Note {
            id: format!("p{number}.{}", i + 1),
            page: number,
            bbox,
            px: (im.px_w, im.px_h),
            mask: im.mask,
            vector: false,
            fate: fate.unwrap_or(Fate::Block),
            members: 1,
            group: None,
            obj: im.obj,
            text: false,
            file: None,
            degraded: false,
        });
    }
    let boxes: Vec<(f64, f64, f64, f64)> = images.iter().map(|im| (im.x0, im.y0, im.x1, im.y1)).collect();
    let units: Vec<Vec<usize>> = clusters(&boxes, &candidates, pmax(p.image_group_gap * body, 2.0));
    let bbox_of = |unit: &[usize]| {
        unit.iter().fold((f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY), |b, &i| {
            let im = &images[i];
            (pmin(b.0, im.x0), pmin(b.1, im.y0), pmax(b.2, im.x1), pmax(b.3, im.y1))
        })
    };
    // the line each unit would sit in, when it is text-sized and in one
    let lines: Vec<Option<usize>> = units
        .iter()
        .map(|u| {
            let b = bbox_of(u);
            let short = b.3 - b.1 <= p.image_inline_height * body;
            if short && budget.spend(page.lines.len()) { inline_line(page, drop, b) } else { None }
        })
        .collect();
    // a page with this many pictures inside its text has text drawn as pictures: word-sized
    // fragments are not figures to restitch
    let fragments = lines.iter().flatten().count() as f64 > p.image_inline_max;
    for (unit, line) in units.iter().zip(lines) {
        let rep = unit[0];
        let b = bbox_of(unit);
        let id = notes[rep].id.clone();
        let marker = sealed(number, &(rep + 1).to_string(), seal);
        notes[rep].members = unit.len();
        if unit.len() > 1 {
            notes[rep].bbox = b;
            notes[rep].mask = unit.iter().all(|&i| images[i].mask);
        }
        notes[rep].text = budget.spend(page.lines.len()) && holds_text(page, drop, b);
        notes[rep].fate = match line {
            Some(_) if fragments => Fate::Fragment,
            Some(li) => {
                insert_marker(page, li, b, &marker);
                Fate::Inline
            }
            None => {
                figures.push(Figure {
                    marker: Rc::from(marker),
                    x0: b.0,
                    y0: b.1,
                    x1: b.2,
                    y1: b.3,
                    labels: Vec::new(),
                });
                Fate::Block
            }
        };
        for &i in &unit[1..] {
            notes[i].fate = Fate::Grouped;
            notes[i].group = Some(id.clone());
        }
    }
    page.images = images;
    (figures, notes)
}

/// Candidate boxes (x0, y0, x1, y1) that touch once each is grown by `gap` points,
/// transitively, each group in ascending order of index and the groups by their first box.
pub(crate) fn clusters(boxes: &[(f64, f64, f64, f64)], candidates: &[usize], gap: f64) -> Vec<Vec<usize>> {
    let n = candidates.len();
    let mut parent: Vec<usize> = (0..n).collect();
    fn find(parent: &mut [usize], mut x: usize) -> usize {
        while parent[x] != x {
            parent[x] = parent[parent[x]];
            x = parent[x];
        }
        x
    }
    // sweep by left edge: only images whose grown left edge is within reach can touch
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&a, &b| fcmp(boxes[candidates[a]].0, boxes[candidates[b]].0));
    let mut reach: Vec<usize> = Vec::new();
    for &b in &order {
        let ib = boxes[candidates[b]];
        reach.retain(|&a| boxes[candidates[a]].2 + gap >= ib.0);
        for &a in &reach {
            let ia = boxes[candidates[a]];
            if ia.3 + gap >= ib.1 && ib.3 + gap >= ia.1 {
                let (ra, rb) = (find(&mut parent, a), find(&mut parent, b));
                if ra != rb {
                    parent[ra.max(rb)] = ra.min(rb);
                }
            }
        }
        reach.push(b);
    }
    let mut groups: Vec<Vec<usize>> = Vec::new();
    let mut slot: Vec<Option<usize>> = vec![None; n];
    for k in 0..n {
        let r = find(&mut parent, k);
        match slot[r] {
            Some(g) => groups[g].push(candidates[k]),
            None => {
                slot[r] = Some(groups.len());
                groups.push(vec![candidates[k]]);
            }
        }
    }
    groups
}

/// Whether a kept line of text lies mostly (half its area) inside the box `b`.
pub(crate) fn holds_text(page: &Page, drop: &[bool], b: (f64, f64, f64, f64)) -> bool {
    page.lines.iter().zip(drop).any(|(l, d)| {
        if *d || l.words.is_empty() {
            return false;
        }
        let w = pmin(l.x1, b.2) - pmax(l.x0, b.0);
        let h = pmin(l.y1, b.3) - pmax(l.y0, b.1);
        let area = (l.x1 - l.x0) * (l.y1 - l.y0);
        w > 0.0 && h > 0.0 && area > 0.0 && w * h >= 0.5 * area
    })
}

fn same_box(a: (f64, f64, f64, f64), b: (f64, f64, f64, f64)) -> bool {
    (a.0 - b.0).abs() < 1.0 && (a.1 - b.1).abs() < 1.0 && (a.2 - b.2).abs() < 1.0 && (a.3 - b.3).abs() < 1.0
}

/// The kept line of text that a box sits in: at least half of the shorter of the two heights
/// overlap, and the box lies within the line's horizontal extent (within two line heights of
/// either end). The one with the most overlap.
fn inline_line(page: &Page, drop: &[bool], b: (f64, f64, f64, f64)) -> Option<usize> {
    let mid = (b.0 + b.2) / 2.0;
    let mut best: Option<(usize, f64)> = None;
    for (li, line) in page.lines.iter().enumerate() {
        if drop[li] || line.words.is_empty() || line.rtl() {
            continue;
        }
        let overlap = pmin(line.y1, b.3) - pmax(line.y0, b.1);
        let shorter = pmin(line.y1 - line.y0, b.3 - b.1);
        let slack = 2.0 * pmax(line.size(), 1.0);
        if shorter <= 0.0 || overlap < 0.5 * shorter || mid < line.x0 - slack || mid > line.x1 + slack {
            continue;
        }
        if best.is_none_or(|(_, o)| overlap > o) {
            best = Some((li, overlap));
        }
    }
    best.map(|(li, _)| li)
}

/// Put `marker` into line `li` of `page`, where the box `b` lies among its words.
fn insert_marker(page: &mut Page, li: usize, b: (f64, f64, f64, f64), marker: &str) {
    let line = &page.lines[li];
    let mid = (b.0 + b.2) / 2.0;
    let at = line.words.iter().position(|w| (w.x0 + w.x1) / 2.0 > mid).unwrap_or(line.words.len());
    let neighbour = &line.words[at.saturating_sub(1).min(line.words.len() - 1)];
    let word = Word {
        text: Rc::from(marker),
        x0: b.0,
        x1: b.2,
        y0: pmin(neighbour.y0, b.1),
        y1: pmax(neighbour.y1, b.3),
        bold: false,
        italic: false,
        space: true,
        glyphs: Default::default(),
        ..neighbour.clone()
    };
    let mut words = line.words.clone();
    words.insert(at, word);
    let new = Line::new(words, line.block, line.flow);
    page.lines[li] = Rc::new(new);
}
