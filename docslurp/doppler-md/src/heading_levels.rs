//! Heading levels by rank within the document (`--heading-levels rank`), instead of by the
//! heading's size relative to the body size.
//!
//! The ratio thresholds (`h1_ratio`, `h2_ratio`, `heading_ratio`) turn a body size that is off
//! (front matter in the body sample) into a shift of every heading level. Ranked, a heading's
//! level depends on how many larger heading sizes the document has, not on the body size:
//! the largest heading size is `#`, the next `##`, the rest `###`.
//!
//! Streaming cannot renumber a heading once its page is written, so the ranking is fixed in two
//! steps that a whole run and a `--stream` run take the same way:
//!
//! 1. **Seed.** Once the first [`crate::DEFAULT_BODY_PAGES`] pages with text are read (at most
//!    twice as many pages, or fewer once they hold [`crate::BODY_SAMPLE_MAX_ITEMS`] items, or the
//!    end of the input), and before any page is written, the sizes of their heading-like lines
//!    are ranked: lines outside the top and bottom margin bands (`furniture_band`), with a
//!    letter, at most `heading_max_chars` characters, at least `heading_ratio` times those
//!    pages' own most common size, and not in a run of more than three lines of one size (a
//!    paragraph). Sizes within `style_size` of the largest of a group are one size; the
//!    largest is `#`, and each smaller size one level deeper (at most `###`) unless it is less
//!    than `h2_ratio / heading_ratio` smaller than the one above, which shares its level.
//! 2. **Additions.** A heading whose size is not in the table is added where it falls without
//!    renumbering any: above every size, `#`; close to the size above (as in the seed), its
//!    level; else one level deeper than the size above when the next smaller size is deeper
//!    still (or there is none), else the size above's level.
//!
//! The seed is always the first pages of the document (not the body sample, which
//! `--body-pages` sets), so it is the same in a whole run and a stream; the additions are made
//! in page order in both. Only which paragraphs are headings at all still depends on the body
//! size (`heading_ratio`), and minor headings (bold or coloured body-size lines, `###`) are
//! not ranked.

use crate::model::{Line, Page};
use crate::params::Params;
use crate::prose::has_letter;
use crate::pyc::{char_len, fcmp, strip};

/// Two heading sizes are a level apart when the larger is at least this many times the
/// smaller: the step between the size thresholds of `##` and `###` (`h2_ratio /
/// heading_ratio`, 1.087)
fn step(p: &Params) -> f64 {
    if p.heading_ratio > 0.0 { p.h2_ratio / p.heading_ratio } else { 1.0 }
}

/// One heading size of the document: the sizes it covers and its level.
#[derive(Clone, Debug, PartialEq)]
struct Rank {
    lo: f64,
    hi: f64,
    level: u8,
}

/// The document's heading sizes, largest first, with their levels. Levels never change once
/// given; new sizes are added between them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HeadingTable {
    ranks: Vec<Rank>,
    /// sizes the seed ranked (the rest were added later)
    pub seeded: usize,
}

impl HeadingTable {
    /// The table seeded from the first pages of the document (see the module docs).
    pub fn seed<'a>(pages: impl Iterator<Item = &'a Page> + Clone, p: &Params) -> HeadingTable {
        let (reference, _) = crate::body_size(pages.clone());
        let mut sizes: Vec<f64> = Vec::new();
        for page in pages {
            let band = page.height * p.furniture_band;
            let lines: Vec<&Line> = page
                .lines
                .iter()
                .map(|l| &**l)
                .filter(|l| !l.words.is_empty() && l.y1 > band && l.y0 < page.height - band)
                .collect();
            // runs of lines in one size, each within two sizes of the one before: more than
            // three is a paragraph (as a heading has at most three lines)
            let mut i = 0;
            while i < lines.len() {
                let size = lines[i].size();
                let mut j = i;
                while j + 1 < lines.len() && {
                    let (a, b) = (lines[j], lines[j + 1]);
                    (b.size() - size).abs() <= p.style_size * size && (b.y0 - a.y1).abs() <= 2.0 * size
                } {
                    j += 1;
                }
                if j - i < 3 && size.is_finite() && size >= p.heading_ratio * reference {
                    for l in &lines[i..=j] {
                        let text = strip(l.text());
                        if char_len(text) as f64 <= p.heading_max_chars && has_letter(text) {
                            sizes.push(l.size());
                        }
                    }
                }
                i = j + 1;
            }
        }
        sizes.sort_by(|a, b| fcmp(*b, *a));
        let mut ranks: Vec<Rank> = Vec::new();
        let mut i = 0;
        while i < sizes.len() {
            let hi = sizes[i];
            let mut j = i;
            while j + 1 < sizes.len() && sizes[j + 1] >= hi * (1.0 - p.style_size) {
                j += 1;
            }
            // one line is a size too (a one-line title)
            let level = match ranks.last() {
                None => 1,
                Some(prev) if prev.lo < hi * step(p) => prev.level,
                Some(prev) => (prev.level + 1).min(3),
            };
            ranks.push(Rank { lo: sizes[j], hi, level });
            i = j + 1;
        }
        let seeded = ranks.len();
        HeadingTable { ranks, seeded }
    }

    /// The level of a heading of `size` points, adding the size when it is new.
    pub fn level(&mut self, size: f64, p: &Params) -> u8 {
        if !size.is_finite() {
            return 3;
        }
        let tol = p.style_size;
        if let Some(r) = self.ranks.iter().find(|r| size >= r.lo * (1.0 - tol) && size <= r.hi * (1.0 + tol)) {
            return r.level;
        }
        // the first size below it (ranks are largest first and do not overlap it)
        let at = self.ranks.iter().position(|r| r.hi < size).unwrap_or(self.ranks.len());
        let above = at.checked_sub(1).map(|k| &self.ranks[k]);
        let below = self.ranks.get(at).map(|r| r.level);
        let level = match (above, below) {
            (None, _) => 1,
            // not a step smaller than the size above: its level
            (Some(a), _) if a.lo < size * step(p) => a.level,
            (Some(a), None) => (a.level + 1).min(3),
            (Some(a), Some(b)) if b > a.level => a.level + 1,
            (Some(a), Some(_)) => a.level,
        };
        self.ranks.insert(at, Rank { lo: size, hi: size, level });
        level
    }

    /// Heading sizes known (seeded and added).
    pub fn len(&self) -> usize {
        self.ranks.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ranks.is_empty()
    }
}
