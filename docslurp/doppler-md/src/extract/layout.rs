//! The image layout: the only place that maps an image id to a file path and a Markdown link.
//!
//! Pure functions, no I/O. A link is a function of the id and of settings known when the
//! Markdown is written (the link prefix, `--raster-format`, `--vector-format`, the repeats
//! mode), never of what extraction later achieves, so the Markdown is final before any image
//! exists and is the same with or without `--images-dir`.
//!
//! | id | path (relative to the images folder) |
//! |---|---|
//! | picture `pN.M` | `page-{N:04}/image-{M:03}.{webp,png}` |
//! | vector figure `pN.gK` | `page-{N:04}/figure-{K:03}.{svg,webp,png}` |
//! | page scan `pN.M` | `page-{N:04}/image-{M:03}.{jpg,png,webp}`: `.jpg` for a JPEG copied unchanged, `.png` for a bilevel scan, else `--raster-format` |
//!
//! Padding is a minimum width: page 12345 is `page-12345`. A link is the prefix
//! (`--images-link-prefix`, default `images/`) followed by the path.
//!
//! Repeats known at conversion: with `--images-repeats skip` or `link`, a marked picture that
//! draws the same image object (`ref`) at the same size as an earlier marked one links to the
//! earlier one's path (and gets no file of its own), so no link dangles because of a repeat.

use std::collections::HashMap;

use super::settings::{RasterFormat, Repeats, ScanPages, VectorFormat};
use crate::images::{Fate, Note, ScanSource};

/// The default `--images-link-prefix`.
pub const DEFAULT_PREFIX: &str = "images/";
/// The longest `--images-link-prefix`.
pub const MAX_PREFIX: usize = 128;

/// Checks an `--images-link-prefix`: at most 128 bytes of `[A-Za-z0-9._~/-]`, relative (no
/// leading `/`), no empty or `..` segment, and ending in `/` unless empty. The characters keep
/// the link inside its Markdown parentheses (no `)`, space, `<`, `%`, `:` or `\`).
pub fn check_prefix(p: &str) -> Result<(), String> {
    if p.is_empty() {
        return Ok(());
    }
    if p.len() > MAX_PREFIX {
        return Err(format!("--images-link-prefix: longer than {MAX_PREFIX} bytes"));
    }
    if !p.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'~' | b'/' | b'-')) {
        return Err("--images-link-prefix: only A-Z a-z 0-9 . _ ~ / - are allowed".into());
    }
    if p.starts_with('/') {
        return Err("--images-link-prefix: must be relative (no leading /)".into());
    }
    let Some(body) = p.strip_suffix('/') else {
        return Err("--images-link-prefix: must end in / (or be empty)".into());
    };
    if body.split('/').any(|s| s.is_empty() || s == "..") {
        return Err("--images-link-prefix: no empty or .. segment".into());
    }
    Ok(())
}

/// What decides the links of a conversion.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LinkStyle {
    /// checked with [`check_prefix`]
    pub prefix: String,
    pub raster: RasterFormat,
    pub vector: VectorFormat,
    pub repeats: Repeats,
    /// `--scan-pages`: whether scans are placed as scans, and copied or rendered
    pub scans: ScanPages,
}

impl Default for LinkStyle {
    fn default() -> Self {
        LinkStyle {
            prefix: DEFAULT_PREFIX.into(),
            raster: RasterFormat::Webp,
            vector: VectorFormat::Svg,
            repeats: Repeats::Skip,
            scans: ScanPages::Image,
        }
    }
}

impl LinkStyle {
    /// The canonical text of the style, for the images plan's settings digest.
    pub fn canonical(&self) -> String {
        format!(
            "links prefix={:?} raster={:?} vector={:?} repeats={:?} scans={:?}\n",
            self.prefix, self.raster, self.vector, self.repeats, self.scans
        )
    }
}

/// The parts of an image id `pN.M` / `pN.gK`: page, vector flag, index. `None` for anything
/// else (see [`super::outdir::valid_id`]).
pub fn parse_id(id: &str) -> Option<(u32, bool, u32)> {
    if !super::outdir::valid_id(id) {
        return None;
    }
    let (page, n) = id[1..].split_once('.')?;
    let (vector, n) = match n.strip_prefix('g') {
        Some(k) => (true, k),
        None => (false, n),
    };
    Some((page.parse().ok()?, vector, n.parse().ok()?))
}

/// The file extension of a picture or figure under `style`.
pub fn extension(vector: bool, style: &LinkStyle) -> &'static str {
    if vector {
        match style.vector {
            VectorFormat::Svg => "svg",
            VectorFormat::Webp => "webp",
            VectorFormat::Png => "png",
        }
    } else {
        match style.raster {
            RasterFormat::Webp => "webp",
            RasterFormat::Png => "png",
        }
    }
}

/// The file extension of a note's file: a page scan's by how it is taken out (a JPEG copied
/// unchanged is `.jpg`, a bilevel scan `.png`, anything decoded or rendered the raster format),
/// anything else by [`extension`].
pub fn note_extension(n: &Note, style: &LinkStyle) -> &'static str {
    if n.fate == Fate::PageScan {
        match n.scan {
            Some(ScanSource::Jpeg) => return "jpg",
            Some(ScanSource::Bilevel) => return "png",
            _ => {}
        }
    }
    extension(n.vector, style)
}

/// The path of image `id` relative to the images folder, or `None` for an invalid id.
pub fn rel_path(id: &str, style: &LinkStyle) -> Option<String> {
    let (_, vector, _) = parse_id(id)?;
    rel_path_ext(id, extension(vector, style))
}

/// The path of image `id` with extension `ext`, or `None` for an invalid id.
fn rel_path_ext(id: &str, ext: &str) -> Option<String> {
    let (page, vector, n) = parse_id(id)?;
    let what = if vector { "figure" } else { "image" };
    Some(format!("page-{page:04}/{what}-{n:03}.{ext}"))
}

/// The path of a note's file relative to the images folder, or `None` for an invalid id.
pub fn note_path(n: &Note, style: &LinkStyle) -> Option<String> {
    rel_path_ext(&n.id, note_extension(n, style))
}

/// Whether `name` is a path [`rel_path`] makes (`page-` 4..=10 digits, `/`, `image-` or
/// `figure-` 3..=7 digits, a known extension), with no leading zero beyond the padding.
pub fn valid_rel(name: &str) -> bool {
    let Some((dir, file)) = name.split_once('/') else { return false };
    let Some(page) = dir.strip_prefix("page-") else { return false };
    let Some((stem, ext)) = file.rsplit_once('.') else { return false };
    let n = stem.strip_prefix("image-").or_else(|| stem.strip_prefix("figure-"));
    let Some(n) = n else { return false };
    let padded = |s: &str, min: usize, max: usize| {
        s.len() >= min
            && s.len() <= max
            && s.bytes().all(|b| b.is_ascii_digit())
            && (s.len() == min || !s.starts_with('0'))
            && s.bytes().any(|b| b != b'0')
    };
    padded(page, 4, 10) && padded(n, 3, 7) && matches!(ext, "webp" | "png" | "svg" | "jpg")
}

/// What makes two marked figures the same: box size (half points), pixel size and kind.
pub type RepeatKey = (i64, i64, u32, u32, &'static str);

/// `picture`, `mask` or `vector`.
pub fn kind(n: &Note) -> &'static str {
    if n.vector {
        "vector"
    } else if n.mask {
        "mask"
    } else {
        "picture"
    }
}

pub fn repeat_key(n: &Note) -> RepeatKey {
    let (x0, y0, x1, y1) = n.bbox;
    (((x1 - x0) * 2.0).round() as i64, ((y1 - y0) * 2.0).round() as i64, n.px.0, n.px.1, kind(n))
}

/// Whether a note gets a link: a marked image or page scan (its marker is in the Markdown) with
/// a valid id.
pub fn linked(n: &Note) -> bool {
    n.fate.marked() && parse_id(&n.id).is_some()
}

/// Whether a note's box is finite and not empty (only those can repeat one another).
fn sound_box(n: &Note) -> bool {
    let (x0, y0, x1, y1) = n.bbox;
    [x0, y0, x1, y1].iter().all(|v| v.is_finite()) && x1 > x0 && y1 > y0
}

/// A placement's repeat key, image object and file extension.
type FirstKey = (RepeatKey, (u32, u32), &'static str);

/// Assigns links to notes in document order, remembering first placements of image objects
/// across pages. The converter runs one over each page's notes as it writes the page;
/// extraction from an images plan runs one over the plan's notes, with the same result.
#[derive(Debug, Default)]
pub struct Linker {
    pub style: LinkStyle,
    /// first marked placement of each (repeat key, image object, file extension): its id and
    /// path
    first: HashMap<FirstKey, (String, String)>,
}

impl Linker {
    pub fn new(style: LinkStyle) -> Linker {
        Linker { style, first: HashMap::new() }
    }

    /// Sets `link` and `repeat_of` of each note (in order).
    pub fn assign(&mut self, notes: &mut [Note]) {
        for n in notes {
            n.link = None;
            n.repeat_of = None;
            if !linked(n) {
                continue;
            }
            let own = note_path(n, &self.style);
            // a rendered page scan is its page's region, not its image object's pixels alone
            let rendered_scan = n.fate == Fate::PageScan && n.scan.is_none_or(|s| s == ScanSource::Render);
            if self.style.repeats != Repeats::Keep
                && sound_box(n)
                && !rendered_scan
                && let Some(obj) = n.obj
            {
                let key = (repeat_key(n), obj, note_extension(n, &self.style));
                if let Some((id, path)) = self.first.get(&key) {
                    n.repeat_of = Some(id.clone());
                    n.link = Some(path.clone());
                    continue;
                }
                if let Some(p) = &own {
                    self.first.insert(key, (n.id.clone(), p.clone()));
                }
            }
            n.link = own;
        }
    }

    /// The full Markdown link target of marked id `id` whose note has `link` (none: the id's
    /// own path, as for a note past the report's cap). `None` for an invalid id.
    pub fn target(&self, id: &str, link: Option<&str>) -> Option<String> {
        let rel = match link {
            Some(r) => r.to_string(),
            None => rel_path(id, &self.style)?,
        };
        Some(format!("{}{rel}", self.style.prefix))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(id: &str, page: u32, obj: Option<(u32, u32)>) -> Note {
        Note {
            id: id.into(),
            page,
            bbox: (10.0, 10.0, 110.0, 60.0),
            px: (200, 100),
            mask: false,
            vector: id.contains(".g"),
            fate: Fate::Block,
            members: 1,
            group: None,
            obj,
            text: false,
            file: None,
            degraded: false,
            link: None,
            repeat_of: None,
            scan: None,
        }
    }

    #[test]
    fn paths_are_padded_and_typed() {
        let s = LinkStyle::default();
        assert_eq!(rel_path("p12.3", &s).unwrap(), "page-0012/image-003.webp");
        assert_eq!(rel_path("p12.g1", &s).unwrap(), "page-0012/figure-001.svg");
        assert_eq!(rel_path("p12345.1234", &s).unwrap(), "page-12345/image-1234.webp");
        let png = LinkStyle { raster: RasterFormat::Png, vector: VectorFormat::Webp, ..s.clone() };
        assert_eq!(rel_path("p1.1", &png).unwrap(), "page-0001/image-001.png");
        assert_eq!(rel_path("p1.g2", &png).unwrap(), "page-0001/figure-002.webp");
        for bad in ["p0.1", "p1.01", "x", "p1.g", "../p1.1"] {
            assert!(rel_path(bad, &s).is_none(), "{bad}");
        }
        for ok in ["page-0012/image-003.webp", "page-123456789/figure-1234567.svg", "page-0001/image-001.jpg"] {
            assert!(valid_rel(ok), "{ok}");
        }
        for bad in [
            "page-012/image-003.webp",
            "page-00012/image-003.webp",
            "page-0000/image-001.webp",
            "page-0001/image-000.webp",
            "page-0001/image-001.exe",
            "page-0001/../image-001.webp",
            "page-0001/image-001.webp/x",
            "manifest.json",
            "page-0001/IMAGE-001.webp",
            "page-0001/image-0001.webp",
            "page-12345678901/image-001.webp",
        ] {
            assert!(!valid_rel(bad), "{bad}");
        }
    }

    #[test]
    fn prefixes_are_checked() {
        for ok in ["", "images/", "a/b/", "x.y_z~-/", "./images/"] {
            assert!(check_prefix(ok).is_ok(), "{ok:?}");
        }
        for bad in [
            "images",
            "/abs/",
            "../up/",
            "a/../b/",
            "a//b/",
            "a b/",
            "a)b/",
            "http://x/",
            "a%20/",
            "a\\b/",
            "a<b/",
            "\u{e9}/",
            &"a".repeat(128),
            &format!("{}/", "a".repeat(128)),
        ] {
            assert!(check_prefix(bad).is_err(), "{bad:?}");
        }
        assert!(check_prefix(&format!("{}/", "a".repeat(127))).is_ok());
    }

    #[test]
    fn repeats_link_to_the_first_placement() {
        let mut l = Linker::new(LinkStyle::default());
        let mut a = vec![note("p1.1", 1, Some((7, 0))), note("p1.2", 1, None)];
        l.assign(&mut a);
        let mut b = vec![note("p2.1", 2, Some((7, 0))), note("p2.2", 2, Some((8, 0)))];
        l.assign(&mut b);
        assert_eq!(b[0].link.as_deref(), Some("page-0001/image-001.webp"));
        assert_eq!(b[0].repeat_of.as_deref(), Some("p1.1"));
        assert_eq!(b[1].link.as_deref(), Some("page-0002/image-002.webp"));
        assert_eq!(l.target("p2.1", b[0].link.as_deref()).unwrap(), "images/page-0001/image-001.webp");
        assert_eq!(l.target("p9.4", None).unwrap(), "images/page-0009/image-004.webp");
        // keep: every placement its own file
        let mut k = Linker::new(LinkStyle { repeats: Repeats::Keep, ..LinkStyle::default() });
        let mut c = vec![note("p1.1", 1, Some((7, 0))), note("p2.1", 2, Some((7, 0)))];
        k.assign(&mut c);
        assert_eq!(c[1].link.as_deref(), Some("page-0002/image-001.webp"));
        assert!(c[1].repeat_of.is_none());
        // unmarked: no link
        let mut d = vec![note("p3.1", 3, None)];
        d[0].fate = Fate::Background;
        l.assign(&mut d);
        assert!(d[0].link.is_none());
    }

    #[test]
    fn page_scans_take_their_copy_extension() {
        let mut l = Linker::new(LinkStyle::default());
        let mut n = vec![note("p4.1", 4, Some((9, 0))), note("p5.2", 5, Some((10, 0))), note("p6.1", 6, None)];
        for (k, src) in [ScanSource::Jpeg, ScanSource::Bilevel, ScanSource::Decode].into_iter().enumerate() {
            n[k].fate = Fate::PageScan;
            n[k].scan = Some(src);
        }
        l.assign(&mut n);
        assert_eq!(n[0].link.as_deref(), Some("page-0004/image-001.jpg"));
        assert_eq!(n[1].link.as_deref(), Some("page-0005/image-002.png"));
        assert_eq!(n[2].link.as_deref(), Some("page-0006/image-001.webp"));
        // the same object again: a copied scan links to the first copy; a rendered scan of it
        // (turned, tiled, --scan-pages render) and a picture of another extension do not
        let mut again =
            vec![note("p8.1", 8, Some((9, 0))), note("p9.1", 9, Some((9, 0))), note("p10.1", 10, Some((9, 0)))];
        again[0].fate = Fate::PageScan;
        again[0].scan = Some(ScanSource::Jpeg);
        again[1].fate = Fate::PageScan;
        again[1].scan = Some(ScanSource::Render);
        l.assign(&mut again);
        assert_eq!(again[0].link.as_deref(), Some("page-0004/image-001.jpg"));
        assert_eq!(again[0].repeat_of.as_deref(), Some("p4.1"));
        assert_eq!(again[1].link.as_deref(), Some("page-0009/image-001.webp"));
        assert!(again[1].repeat_of.is_none());
        assert_eq!(again[2].link.as_deref(), Some("page-0010/image-001.webp"));
        assert!(again[2].repeat_of.is_none());
        // a scan source on a note placed as a picture (--scan-pages skip) does not change it
        let mut m = vec![note("p7.1", 7, None)];
        m[0].scan = Some(ScanSource::Jpeg);
        l.assign(&mut m);
        assert_eq!(m[0].link.as_deref(), Some("page-0007/image-001.webp"));
    }
}
