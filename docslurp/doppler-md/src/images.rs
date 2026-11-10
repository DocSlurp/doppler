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

use crate::model::{Image, Line, Page, Word};
use crate::params::Params;
use crate::pyc::{fcmp, pmax, pmin};

/// The URL scheme of a marker.
pub const SCHEME: &str = "doppler-image:";

/// The marker of picture `index` (one-based) on page `page`.
pub fn marker(page: u32, index: usize) -> String {
    format!("![image]({SCHEME}p{page}.{index})")
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
    /// below the size floors: no marker
    Small,
    /// drawn again at the same place: no marker
    Duplicate,
    /// text-sized, among text, on a page full of them (text drawn as pictures): no marker
    Fragment,
    /// one of several neighbouring images marked together, at the first of them (`group`)
    Grouped,
}

impl Fate {
    pub fn as_str(self) -> &'static str {
        match self {
            Fate::Block => "block",
            Fate::Inline => "inline",
            Fate::Background => "background",
            Fate::Furniture => "furniture",
            Fate::Small => "small",
            Fate::Duplicate => "duplicate",
            Fate::Fragment => "fragment",
            Fate::Grouped => "grouped",
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
    pub fate: Fate,
    /// images marked together at this one (1 when alone); its box is then the union of theirs
    pub members: usize,
    /// for a grouped image, the id of the image that stands for the group
    pub group: Option<String>,
}

/// A picture that is its own paragraph: the marker and the box it fills.
#[derive(Clone, Debug)]
pub struct Figure {
    pub marker: Rc<str>,
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
}

/// Decide each image of `page`. Neighbouring images (within about a line of each other, one
/// after another) are one figure: a diagram or a chart assembled from pieces, or a line of
/// words drawn as pictures. A figure is marked once, at its representative: the first of its
/// images. Inline pictures are put into their lines of `page` now; the figures that stand
/// alone come back for the layout to order.
pub fn place(page: &mut Page, drop: &[bool], body: f64, p: &Params, number: u32) -> (Vec<Figure>, Vec<Note>) {
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
        let fate = if pmin(w, h) < floor {
            Some(Fate::Small)
        } else if seen.iter().any(|s| same_box(*s, bbox)) {
            Some(Fate::Duplicate)
        } else if w * h >= p.image_background_share * area && kept_text {
            Some(Fate::Background)
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
            fate: fate.unwrap_or(Fate::Block),
            members: 1,
            group: None,
        });
    }
    let units: Vec<Vec<usize>> = clusters(&images, &candidates, pmax(p.image_group_gap * body, 2.0));
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
            if b.3 - b.1 <= p.image_inline_height * body { inline_line(page, drop, b) } else { None }
        })
        .collect();
    // a page with this many pictures inside its text has text drawn as pictures: word-sized
    // fragments are not figures to restitch
    let fragments = lines.iter().flatten().count() as f64 > p.image_inline_max;
    for (unit, line) in units.iter().zip(lines) {
        let rep = unit[0];
        let b = bbox_of(unit);
        let id = notes[rep].id.clone();
        let marker = marker(number, rep + 1);
        notes[rep].members = unit.len();
        if unit.len() > 1 {
            notes[rep].bbox = b;
            notes[rep].mask = unit.iter().all(|&i| images[i].mask);
        }
        notes[rep].fate = match line {
            Some(_) if fragments => Fate::Fragment,
            Some(li) => {
                insert_marker(page, li, b, &marker);
                Fate::Inline
            }
            None => {
                figures.push(Figure { marker: Rc::from(marker), x0: b.0, y0: b.1, x1: b.2, y1: b.3 });
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

/// Candidates that touch once each is grown by `gap` points, transitively, each group in
/// ascending order of image index and the groups by their first image.
fn clusters(images: &[Image], candidates: &[usize], gap: f64) -> Vec<Vec<usize>> {
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
    order.sort_by(|&a, &b| fcmp(images[candidates[a]].x0, images[candidates[b]].x0));
    let mut reach: Vec<usize> = Vec::new();
    for &b in &order {
        let ib = &images[candidates[b]];
        reach.retain(|&a| images[candidates[a]].x1 + gap >= ib.x0);
        for &a in &reach {
            let ia = &images[candidates[a]];
            if ia.y1 + gap >= ib.y0 && ib.y1 + gap >= ia.y0 {
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
