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
use crate::pyc::{pmax, pmin};

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

/// Decide each image of `page`. Inline pictures are put into their lines of `page` now; the
/// pictures that stand alone come back as figures for the layout to order.
pub fn place(page: &mut Page, drop: &[bool], body: f64, p: &Params, number: u32) -> (Vec<Figure>, Vec<Note>) {
    let images = std::mem::take(&mut page.images);
    let mut figures = Vec::new();
    let mut notes = Vec::with_capacity(images.len());
    let area = pmax(page.width * page.height, 1.0);
    let band = p.furniture_band * page.height;
    let kept_text = page.lines.iter().zip(drop).any(|(l, d)| !*d && !l.text().trim().is_empty());
    let mut seen: Vec<(f64, f64, f64, f64)> = Vec::new();
    for (i, im) in images.iter().enumerate() {
        let id = format!("p{number}.{}", i + 1);
        let (w, h) = (im.x1 - im.x0, im.y1 - im.y0);
        let floor = if im.mask { p.image_min_mask } else { p.image_min_size };
        let bbox = (im.x0, im.y0, im.x1, im.y1);
        let fate = if pmin(w, h) < floor {
            Fate::Small
        } else if seen.iter().any(|s| same_box(*s, bbox)) {
            Fate::Duplicate
        } else if w * h >= p.image_background_share * area && kept_text {
            Fate::Background
        } else if im.y1 <= band || im.y0 >= page.height - band {
            Fate::Furniture
        } else if h <= p.image_inline_height * body && put_inline(page, drop, im, &marker(number, i + 1)) {
            Fate::Inline
        } else {
            figures.push(Figure { marker: Rc::from(marker(number, i + 1)), x0: im.x0, y0: im.y0, x1: im.x1, y1: im.y1 });
            Fate::Block
        };
        if matches!(fate, Fate::Block | Fate::Inline) {
            seen.push(bbox);
        }
        notes.push(Note { id, page: number, bbox, px: (im.px_w, im.px_h), mask: im.mask, fate });
    }
    page.images = images;
    (figures, notes)
}

fn same_box(a: (f64, f64, f64, f64), b: (f64, f64, f64, f64)) -> bool {
    (a.0 - b.0).abs() < 1.0 && (a.1 - b.1).abs() < 1.0 && (a.2 - b.2).abs() < 1.0 && (a.3 - b.3).abs() < 1.0
}

/// Put `marker` into the kept line of text that the image sits in: at least half of the
/// shorter of the two heights overlap, and the image lies within the line's horizontal extent
/// (within two line heights of either end). Returns whether it did.
fn put_inline(page: &mut Page, drop: &[bool], im: &Image, marker: &str) -> bool {
    let mid = (im.x0 + im.x1) / 2.0;
    let mut best: Option<(usize, f64)> = None;
    for (li, line) in page.lines.iter().enumerate() {
        if drop[li] || line.words.is_empty() || line.rtl() {
            continue;
        }
        let overlap = pmin(line.y1, im.y1) - pmax(line.y0, im.y0);
        let shorter = pmin(line.y1 - line.y0, im.y1 - im.y0);
        let slack = 2.0 * pmax(line.size(), 1.0);
        if shorter <= 0.0 || overlap < 0.5 * shorter || mid < line.x0 - slack || mid > line.x1 + slack {
            continue;
        }
        if best.is_none_or(|(_, o)| overlap > o) {
            best = Some((li, overlap));
        }
    }
    let Some((li, _)) = best else { return false };
    let line = &page.lines[li];
    let at = line.words.iter().position(|w| (w.x0 + w.x1) / 2.0 > mid).unwrap_or(line.words.len());
    let neighbour = &line.words[at.saturating_sub(1).min(line.words.len() - 1)];
    let word = Word {
        text: Rc::from(marker),
        x0: im.x0,
        x1: im.x1,
        y0: pmin(neighbour.y0, im.y0),
        y1: pmax(neighbour.y1, im.y1),
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
    true
}
