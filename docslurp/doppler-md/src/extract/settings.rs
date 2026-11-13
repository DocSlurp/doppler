//! The `--images-*` flags of `pdftomarkdown`.

use std::path::PathBuf;
use std::time::Duration;

/// The smallest side of a picture or figure that gets a file.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MinSide {
    /// output pixels at the extraction dpi
    Px(f64),
    /// page points
    Pt(f64),
}

/// What happens to a marked figure that repeats an earlier one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Repeats {
    /// no file
    Skip,
    /// no file; the manifest points to the first one's (`repeat_of`)
    Link,
    /// a file of its own
    Keep,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RasterFormat {
    Webp,
    Png,
}

/// What becomes of a full-page scan (`--scan-pages`; the producer's `scan=` on `<page>`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScanPages {
    /// the default: only a scan whose own bytes can be copied (one upright image the producer
    /// marks `copyable="1"`, `copy="jpeg"` or `copy="bilevel"`) is a page scan, as in `image`;
    /// any other scan is placed as the page's backdrop, with no marker and no file (its page
    /// image comes from the host's page rendering). Nothing is decoded or rendered for scans
    Copy,
    /// the page's Markdown starts with `![page N scan](link)`; the file is the scan's own
    /// pixels where its dictionary allows (a `.jpg` copy, a bilevel `.png`, else decoded once),
    /// rendered otherwise (tiles, layers, a turned image)
    Image,
    /// placed as any picture (a backdrop under text, or a marker), and never extracted
    Skip,
    /// as `image`, but always rendered at `--scan-dpi` (`--raster-format`)
    Render,
}

impl ScanPages {
    pub fn as_str(self) -> &'static str {
        match self {
            ScanPages::Copy => "copy",
            ScanPages::Image => "image",
            ScanPages::Skip => "skip",
            ScanPages::Render => "render",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VectorFormat {
    Svg,
    Webp,
    Png,
}

/// Image extraction settings; `dir` is `None` when it is off.
#[derive(Clone, Debug)]
pub struct Settings {
    pub dir: Option<PathBuf>,
    /// `--images-resume DIR`: `dir` is an earlier run's folder whose resumable images are
    /// extracted now (see [`super::resume`])
    pub resume: bool,
    pub min_side: MinSide,
    pub repeats: Repeats,
    pub raster_format: RasterFormat,
    pub raster_quality: u8,
    pub raster_lossless: bool,
    /// `--report-timing`: the report gains `images_ms` and `images_cpu_ms` (they change from
    /// run to run, so they are left out of the deterministic report by default)
    pub report_timing: bool,
    /// libwebp effort: lossy method, lossless method and lossless quality
    pub raster_method: u8,
    pub lossless_method: u8,
    pub lossless_quality: u8,
    pub vector_format: VectorFormat,
    pub svg_precision: u8,
    pub svg_max_bytes: u64,
    pub dpi: u32,
    pub max_images: usize,
    pub max_bytes: u64,
    /// one file larger is not written (that image gets `error: file_bytes`)
    pub max_file_bytes: u64,
    pub timeout: Duration,
    /// pdfregions `-confine`: `require`, `auto` or `off`
    pub confine: &'static str,
    /// only figures on these pages (1-based, inclusive) are extracted
    pub pages: Option<(u32, u32)>,
    /// `--images-resume-pages`: a resume retries only the resumable images on these pages
    /// (1-based, inclusive); the others keep their entries, still resumable. Not part of the
    /// settings a manifest is bound to: it narrows what one resume does, not what the images are.
    pub resume_pages: Option<(u32, u32)>,
    /// `--images-link-prefix`: put before each image path in the Markdown links
    pub link_prefix: String,
    /// `--images-batch-pages`: pages per extraction batch (one renderer each, its own budgets)
    pub batch_pages: u32,
    /// `--images-doc-max-bytes`: most bytes written for the whole document
    pub doc_max_bytes: u64,
    /// `--images-deadline`: time for the whole extraction; none given: [`Settings::deadline_for`]
    /// scales it with the converted pages
    pub deadline: Option<Duration>,
    /// `--scan-pages`
    pub scan_pages: ScanPages,
    /// `--scan-dpi`: resolution of rendered scans
    pub scan_dpi: u32,
    /// `--scan-max-dpi`: a decoded scan finer than this is scaled down to it
    pub scan_max_dpi: u32,
    /// `--scan-max`, `--scan-max-bytes`: scan files and bytes per batch, apart from the
    /// pictures' and figures' own budget
    pub scan_max: usize,
    pub scan_max_bytes: u64,
    /// an extraction flag (one that needs `--images-dir`) was given
    other_flags: bool,
    /// a flag that shapes the Markdown links (`--images-link-prefix`, `--images-repeats`,
    /// `--raster-format`, `--vector-format`) was given
    link_flags: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            dir: None,
            resume: false,
            min_side: MinSide::Px(24.0),
            repeats: Repeats::Skip,
            raster_format: RasterFormat::Webp,
            raster_quality: 90,
            raster_lossless: false,
            report_timing: false,
            raster_method: 4,
            lossless_method: 4,
            lossless_quality: 50,
            vector_format: VectorFormat::Svg,
            svg_precision: 1,
            svg_max_bytes: 512 << 10,
            dpi: 150,
            max_images: 500,
            max_bytes: 256 << 20,
            max_file_bytes: 8 << 20,
            timeout: Duration::from_secs(120),
            confine: "auto",
            pages: None,
            resume_pages: None,
            link_prefix: super::layout::DEFAULT_PREFIX.into(),
            batch_pages: 64,
            doc_max_bytes: 4 << 30,
            deadline: None,
            scan_pages: ScanPages::Copy,
            scan_dpi: 200,
            scan_max_dpi: 300,
            scan_max: 1000,
            scan_max_bytes: 1 << 30,
            other_flags: false,
            link_flags: false,
        }
    }
}

/// Every flag that takes a value.
const WITH_VALUE: &[&str] = &[
    "--images-dir",
    "--images-resume",
    "--images-min-side",
    "--images-repeats",
    "--raster-format",
    "--raster-quality",
    "--raster-method",
    "--lossless-method",
    "--lossless-quality",
    "--vector-format",
    "--svg-precision",
    "--svg-max-bytes",
    "--images-dpi",
    "--images-max",
    "--images-max-bytes",
    "--images-max-file-bytes",
    "--images-timeout",
    "--images-confine",
    "--images-pages",
    "--images-resume-pages",
    // links (also without --images-dir) and batches
    "--images-link-prefix",
    "--images-batch-pages",
    "--images-doc-max-bytes",
    "--images-deadline",
    // page scans
    "--scan-pages",
    "--scan-dpi",
    "--scan-max-dpi",
    "--scan-max",
    "--scan-max-bytes",
];

/// The flags that shape the Markdown links: valid with `--markers` alone.
const LINK_FLAGS: &[&str] =
    &["--images-link-prefix", "--images-repeats", "--raster-format", "--vector-format", "--scan-pages"];

/// The default `--images-deadline`: 120 s plus half a second per converted page.
pub const DEADLINE_BASE: Duration = Duration::from_secs(120);
pub const DEADLINE_PER_PAGE: Duration = Duration::from_millis(500);
/// The default `--images-deadline` never exceeds this (an hour).
pub const DEADLINE_CAP: Duration = Duration::from_secs(3600);

impl Settings {
    /// The time the whole extraction gets: `--images-deadline` when given, else 120 s plus
    /// 0.5 s per converted page, at most 3600 s.
    pub fn deadline_for(&self, pages: usize) -> Duration {
        self.deadline.unwrap_or_else(|| {
            let pages = u32::try_from(pages).unwrap_or(u32::MAX);
            DEADLINE_BASE.saturating_add(DEADLINE_PER_PAGE.saturating_mul(pages)).min(DEADLINE_CAP)
        })
    }

    /// Whether `name` is one of these flags, and whether it takes a value.
    pub fn takes(name: &str) -> Option<bool> {
        if name == "--raster-lossless" || name == "--report-timing" {
            Some(false)
        } else if WITH_VALUE.contains(&name) {
            Some(true)
        } else {
            None
        }
    }

    /// Applies flag `name` (one [`Settings::takes`] knows) with its value.
    pub fn apply(&mut self, name: &str, value: Option<&str>) -> Result<(), String> {
        if name == "--raster-lossless" {
            self.raster_lossless = true;
            self.other_flags = true;
            return Ok(());
        }
        if name == "--report-timing" {
            self.report_timing = true;
            self.other_flags = true;
            return Ok(());
        }
        let v = value.ok_or_else(|| format!("{name} needs a value"))?;
        let bad = || format!("{name}: bad value {v:?}");
        if LINK_FLAGS.contains(&name) {
            self.link_flags = true;
        } else if name != "--images-dir" && name != "--images-resume" {
            self.other_flags = true;
        }
        match name {
            "--images-dir" | "--images-resume" => {
                if v.is_empty() {
                    return Err(bad());
                }
                if self.dir.is_some() {
                    return Err("give one --images-dir or --images-resume".into());
                }
                self.dir = Some(PathBuf::from(v));
                self.resume = name == "--images-resume";
            }
            "--images-min-side" => self.min_side = parse_min_side(v).ok_or_else(bad)?,
            "--images-repeats" => {
                self.repeats = match v {
                    "skip" => Repeats::Skip,
                    "link" => Repeats::Link,
                    "keep" => Repeats::Keep,
                    _ => return Err(format!("{name} takes skip, link or keep, not {v:?}")),
                }
            }
            "--raster-format" => {
                self.raster_format = match v {
                    "webp" => RasterFormat::Webp,
                    "png" => RasterFormat::Png,
                    _ => return Err(format!("{name} takes webp or png, not {v:?}")),
                }
            }
            "--raster-quality" => self.raster_quality = int(v, 0, 100).ok_or_else(bad)? as u8,
            "--raster-method" => self.raster_method = int(v, 0, 6).ok_or_else(bad)? as u8,
            "--lossless-method" => self.lossless_method = int(v, 0, 6).ok_or_else(bad)? as u8,
            "--lossless-quality" => self.lossless_quality = int(v, 0, 100).ok_or_else(bad)? as u8,
            "--vector-format" => {
                self.vector_format = match v {
                    "svg" => VectorFormat::Svg,
                    "webp" => VectorFormat::Webp,
                    "png" => VectorFormat::Png,
                    _ => return Err(format!("{name} takes svg, webp or png, not {v:?}")),
                }
            }
            "--svg-precision" => self.svg_precision = int(v, 1, 2).ok_or_else(bad)? as u8,
            "--svg-max-bytes" => self.svg_max_bytes = bytes(v, 1, 64 << 20).ok_or_else(bad)?,
            "--images-dpi" => self.dpi = int(v, 36, 600).ok_or_else(bad)? as u32,
            "--images-max" => self.max_images = int(v, 0, 5000).ok_or_else(bad)? as usize,
            "--images-max-bytes" => self.max_bytes = bytes(v, 1, 64 << 30).ok_or_else(bad)?,
            "--images-max-file-bytes" => self.max_file_bytes = bytes(v, 1, 64 << 30).ok_or_else(bad)?,
            "--images-confine" => {
                self.confine = match v {
                    "require" => "require",
                    "auto" => "auto",
                    "off" => "off",
                    _ => return Err(format!("{name} takes require, auto or off, not {v:?}")),
                }
            }
            "--images-pages" => self.pages = Some(page_range(v).ok_or_else(bad)?),
            "--images-resume-pages" => self.resume_pages = Some(page_range(v).ok_or_else(bad)?),
            "--images-timeout" => self.timeout = Duration::from_secs(int(v, 1, 86400).ok_or_else(bad)?),
            "--images-link-prefix" => {
                super::layout::check_prefix(v)?;
                self.link_prefix = v.to_string();
            }
            "--images-batch-pages" => self.batch_pages = int(v, 1, 100_000).ok_or_else(bad)? as u32,
            "--images-doc-max-bytes" => self.doc_max_bytes = bytes(v, 1, 1 << 40).ok_or_else(bad)?,
            "--images-deadline" => self.deadline = Some(Duration::from_secs(int(v, 1, 604_800).ok_or_else(bad)?)),
            "--scan-pages" => {
                self.scan_pages = match v {
                    "copy" => ScanPages::Copy,
                    "image" => ScanPages::Image,
                    "skip" => ScanPages::Skip,
                    "render" => ScanPages::Render,
                    _ => return Err(format!("{name} takes copy, image, skip or render, not {v:?}")),
                }
            }
            "--scan-dpi" => self.scan_dpi = int(v, 36, 300).ok_or_else(bad)? as u32,
            "--scan-max-dpi" => self.scan_max_dpi = int(v, 36, 1200).ok_or_else(bad)? as u32,
            "--scan-max" => self.scan_max = int(v, 0, 100_000).ok_or_else(bad)? as usize,
            "--scan-max-bytes" => self.scan_max_bytes = bytes(v, 1, 64 << 30).ok_or_else(bad)?,
            _ => return Err(format!("unknown option {name}")),
        }
        Ok(())
    }

    /// The flags given make sense together. The link flags are checked by the caller (they
    /// need markers).
    pub fn check(&self) -> Result<(), String> {
        if self.other_flags && self.dir.is_none() {
            return Err("the --images-*, --raster-*, --lossless-*, --vector-*, --svg-* and --scan-* options need \
                 --images-dir (except --images-link-prefix, --images-repeats, --raster-format, --vector-format \
                 and --scan-pages, which need --markers)"
                .into());
        }
        if self.resume_pages.is_some() && !self.resume {
            return Err("--images-resume-pages needs --images-resume".into());
        }
        Ok(())
    }

    /// Whether a resume retries the resumable images of `page` (`--images-resume-pages`; all
    /// pages without it).
    pub fn resumes_page(&self, page: u32) -> bool {
        self.resume_pages.is_none_or(|(a, b)| (a..=b).contains(&page))
    }

    /// Whether a flag that shapes the links was given.
    pub fn link_flags(&self) -> bool {
        self.link_flags
    }

    /// The links these settings give the Markdown.
    pub fn link_style(&self) -> super::layout::LinkStyle {
        super::layout::LinkStyle {
            prefix: self.link_prefix.clone(),
            raster: self.raster_format,
            vector: self.vector_format,
            repeats: self.repeats,
            scans: self.scan_pages,
        }
    }

    /// What shapes the files and the manifest's entries, for the manifest's settings digest
    /// (a resume must make the same files an uninterrupted run would): the links, the formats
    /// and their encoder settings, the resolutions, the smallest side and the page range. The
    /// budgets, batch size, time limits and confinement are left out: a resume may change them.
    pub fn output_canonical(&self) -> String {
        format!(
            "{}files min_side={:?} raster_quality={} raster_lossless={} raster_method={} lossless_method={} \
             lossless_quality={} svg_precision={} svg_max_bytes={} max_file_bytes={} dpi={} pages={:?} scan_dpi={} \
             scan_max_dpi={}\n",
            self.link_style().canonical(),
            self.min_side,
            self.raster_quality,
            self.raster_lossless,
            self.raster_method,
            self.lossless_method,
            self.lossless_quality,
            self.svg_precision,
            self.svg_max_bytes,
            self.max_file_bytes,
            self.dpi,
            self.pages,
            self.scan_dpi,
            self.scan_max_dpi
        )
    }

    /// The WebP encoder effort.
    pub fn effort(&self) -> super::encode::Effort {
        super::encode::Effort {
            lossy_method: self.raster_method,
            lossless_method: self.lossless_method,
            lossless_quality: self.lossless_quality,
        }
    }

    /// The smallest side, in page points.
    pub fn min_side_pt(&self) -> f64 {
        match self.min_side {
            MinSide::Pt(v) => v,
            MinSide::Px(v) => v * 72.0 / f64::from(self.dpi),
        }
    }
}

/// `A-B` or `N` (1-based, inclusive, A <= B).
fn page_range(v: &str) -> Option<(u32, u32)> {
    let (a, b) = v.split_once('-').unwrap_or((v, v));
    let (a, b) = (int(a, 1, i32::MAX as u64)?, int(b, 1, i32::MAX as u64)?);
    (a <= b).then_some((a as u32, b as u32))
}

fn int(v: &str, lo: u64, hi: u64) -> Option<u64> {
    if v.is_empty() || v.len() > 12 || !v.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    v.parse::<u64>().ok().filter(|n| (lo..=hi).contains(n))
}

/// `N`, `N[px]` or `N.Mpt`.
fn parse_min_side(v: &str) -> Option<MinSide> {
    let num = |s: &str| -> Option<f64> {
        if s.is_empty() || s.len() > 12 || !s.bytes().all(|b| b.is_ascii_digit() || b == b'.') {
            return None;
        }
        s.parse::<f64>().ok().filter(|x| x.is_finite() && (0.0..=100_000.0).contains(x))
    };
    if let Some(n) = v.strip_suffix("pt") {
        num(n).map(MinSide::Pt)
    } else {
        num(v.strip_suffix("px").unwrap_or(v)).map(MinSide::Px)
    }
}

/// `N` bytes, or with a `K`/`KiB`, `M`/`MiB`, `G`/`GiB` suffix (powers of 1024).
fn bytes(v: &str, lo: u64, hi: u64) -> Option<u64> {
    let (n, mult) =
        [("KiB", 1u64 << 10), ("MiB", 1 << 20), ("GiB", 1 << 30), ("K", 1 << 10), ("M", 1 << 20), ("G", 1 << 30)]
            .iter()
            .find_map(|(s, m)| v.strip_suffix(s).map(|n| (n, *m)))
            .unwrap_or((v, 1));
    int(n, 0, u64::MAX).and_then(|n| n.checked_mul(mult)).filter(|n| (lo..=hi).contains(n))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with(flags: &[(&str, Option<&str>)]) -> Result<Settings, String> {
        let mut s = Settings::default();
        for (k, v) in flags {
            s.apply(k, *v)?;
        }
        s.check()?;
        Ok(s)
    }

    #[test]
    fn defaults() {
        let s = with(&[("--images-dir", Some("out"))]).unwrap();
        assert_eq!(s.min_side, MinSide::Px(24.0));
        assert!((s.min_side_pt() - 24.0 * 72.0 / 150.0).abs() < 1e-9);
        assert_eq!(
            (s.repeats, s.raster_format, s.vector_format),
            (Repeats::Skip, RasterFormat::Webp, VectorFormat::Svg)
        );
        assert_eq!((s.raster_quality, s.svg_precision, s.dpi, s.max_images), (90, 1, 150, 500));
        assert_eq!((s.svg_max_bytes, s.max_bytes, s.timeout.as_secs()), (512 << 10, 256 << 20, 120));
        assert_eq!(s.max_file_bytes, 8 << 20);
        assert_eq!(s.effort(), super::super::encode::Effort::default());
        assert_eq!(
            (s.link_prefix.as_str(), s.batch_pages, s.doc_max_bytes, s.deadline),
            ("images/", 64, 4 << 30, None)
        );
        assert_eq!(
            (s.scan_pages, s.scan_dpi, s.scan_max_dpi, s.scan_max, s.scan_max_bytes),
            (ScanPages::Copy, 200, 300, 1000, 1 << 30)
        );
    }

    #[test]
    fn values() {
        let s = with(&[
            ("--images-dir", Some("out")),
            ("--images-min-side", Some("10.5pt")),
            ("--images-repeats", Some("link")),
            ("--raster-format", Some("png")),
            ("--raster-quality", Some("75")),
            ("--raster-lossless", None),
            ("--vector-format", Some("webp")),
            ("--svg-precision", Some("2")),
            ("--svg-max-bytes", Some("1MiB")),
            ("--images-dpi", Some("300")),
            ("--images-max", Some("20")),
            ("--images-max-bytes", Some("10M")),
            ("--images-timeout", Some("5")),
            ("--images-max-file-bytes", Some("2M")),
            ("--images-confine", Some("require")),
            ("--images-pages", Some("3-19")),
            ("--raster-method", Some("2")),
            ("--lossless-method", Some("1")),
            ("--lossless-quality", Some("25")),
            ("--images-link-prefix", Some("a/b/")),
            ("--images-batch-pages", Some("8")),
            ("--images-doc-max-bytes", Some("1G")),
            ("--images-deadline", Some("30")),
            ("--scan-pages", Some("render")),
            ("--scan-dpi", Some("250")),
            ("--scan-max-dpi", Some("400")),
            ("--scan-max", Some("7")),
            ("--scan-max-bytes", Some("5M")),
        ])
        .unwrap();
        assert_eq!(
            (s.scan_pages, s.scan_dpi, s.scan_max_dpi, s.scan_max, s.scan_max_bytes),
            (ScanPages::Render, 250, 400, 7, 5 << 20)
        );
        assert_eq!(s.link_style().scans, ScanPages::Render);
        assert_eq!((s.link_prefix.as_str(), s.batch_pages, s.doc_max_bytes), ("a/b/", 8, 1 << 30));
        assert_eq!(s.deadline, Some(Duration::from_secs(30)));
        assert_eq!(s.link_style().prefix, "a/b/");
        assert_eq!(s.min_side, MinSide::Pt(10.5));
        assert_eq!(s.min_side_pt(), 10.5);
        assert_eq!(
            (s.repeats, s.raster_format, s.vector_format),
            (Repeats::Link, RasterFormat::Png, VectorFormat::Webp)
        );
        assert!(s.raster_lossless);
        assert_eq!((s.raster_quality, s.svg_precision, s.dpi, s.max_images), (75, 2, 300, 20));
        assert_eq!((s.svg_max_bytes, s.max_bytes, s.timeout.as_secs()), (1 << 20, 10 << 20, 5));
        assert_eq!(s.max_file_bytes, 2 << 20);
        assert_eq!(s.confine, "require");
        assert_eq!(s.pages, Some((3, 19)));
        assert_eq!((s.raster_method, s.lossless_method, s.lossless_quality), (2, 1, 25));
        let e = s.effort();
        assert_eq!((e.lossy_method, e.lossless_method, e.lossless_quality), (2, 1, 25));
        assert_eq!(with(&[("--images-dir", Some("o")), ("--images-pages", Some("7"))]).unwrap().pages, Some((7, 7)));
        assert_eq!(
            with(&[("--images-dir", Some("o")), ("--images-min-side", Some("30px"))]).unwrap().min_side,
            MinSide::Px(30.0)
        );
    }

    #[test]
    fn deadline_scales_with_pages() {
        let s = with(&[("--images-dir", Some("out"))]).unwrap();
        assert_eq!(s.deadline_for(0), Duration::from_secs(120));
        assert_eq!(s.deadline_for(1), Duration::from_millis(120_500));
        assert_eq!(s.deadline_for(100), Duration::from_secs(170));
        assert_eq!(s.deadline_for(6960), Duration::from_secs(3600));
        assert_eq!(s.deadline_for(10_000), Duration::from_secs(3600), "capped at an hour");
        assert_eq!(s.deadline_for(usize::MAX), Duration::from_secs(3600));
        // an explicit value overrides, below or above the default
        let s = with(&[("--images-dir", Some("out")), ("--images-deadline", Some("30"))]).unwrap();
        assert_eq!((s.deadline_for(1), s.deadline_for(10_000)), (Duration::from_secs(30), Duration::from_secs(30)));
        let s = with(&[("--images-dir", Some("out")), ("--images-deadline", Some("7200"))]).unwrap();
        assert_eq!(s.deadline_for(10_000), Duration::from_secs(7200));
    }

    #[test]
    fn resume_pages_narrow_a_resume_only() {
        let s = with(&[("--images-resume", Some("out")), ("--images-resume-pages", Some("3-4"))]).unwrap();
        assert_eq!(s.resume_pages, Some((3, 4)));
        assert_eq!(
            (s.resumes_page(2), s.resumes_page(3), s.resumes_page(4), s.resumes_page(5)),
            (false, true, true, false)
        );
        let s = with(&[("--images-resume", Some("out")), ("--images-resume-pages", Some("7"))]).unwrap();
        assert_eq!(s.resume_pages, Some((7, 7)));
        assert!(with(&[("--images-resume", Some("out"))]).unwrap().resumes_page(1_000_000));
        // not with --images-dir, and not part of the settings a manifest is bound to
        assert!(with(&[("--images-dir", Some("out")), ("--images-resume-pages", Some("3-4"))]).is_err());
        let a = with(&[("--images-resume", Some("out"))]).unwrap();
        let b = with(&[("--images-resume", Some("out")), ("--images-resume-pages", Some("3-4"))]).unwrap();
        assert_eq!(a.output_canonical(), b.output_canonical());
    }

    #[test]
    fn bad_values() {
        for (k, v) in [
            ("--images-min-side", "-1"),
            ("--images-min-side", "12em"),
            ("--images-min-side", "nan"),
            ("--images-repeats", "drop"),
            ("--raster-format", "jpeg"),
            ("--raster-quality", "101"),
            ("--raster-method", "7"),
            ("--raster-method", "-1"),
            ("--raster-method", "fast"),
            ("--lossless-method", "7"),
            ("--lossless-method", ""),
            ("--lossless-quality", "101"),
            ("--lossless-quality", "2.5"),
            ("--vector-format", "pdf"),
            ("--svg-precision", "3"),
            ("--svg-precision", "0"),
            ("--svg-max-bytes", "0"),
            ("--images-dpi", "35"),
            ("--images-dpi", "601"),
            ("--images-max", "5001"),
            ("--images-max-bytes", "1T"),
            ("--images-timeout", "0"),
            ("--images-confine", "yes"),
            ("--images-pages", "5-2"),
            ("--images-pages", "0-2"),
            ("--images-pages", "1-"),
            ("--images-resume-pages", "5-2"),
            ("--images-resume-pages", "0"),
            ("--images-dir", ""),
            ("--images-link-prefix", "../x/"),
            ("--images-link-prefix", "x"),
            ("--images-link-prefix", "a b/"),
            ("--images-batch-pages", "0"),
            ("--images-doc-max-bytes", "0"),
            ("--images-deadline", "0"),
            ("--scan-pages", "decode"),
            ("--scan-pages", "Copy"),
            ("--scan-dpi", "301"),
            ("--scan-dpi", "35"),
            ("--scan-max-dpi", "0"),
            ("--scan-max-bytes", "0"),
        ] {
            assert!(with(&[("--images-dir", Some("o")), (k, Some(v))]).is_err(), "{k} {v}");
        }
        assert!(with(&[("--images-dpi", None)]).is_err());
        // the others need --images-dir
        assert!(with(&[("--images-dpi", Some("100"))]).is_err());
        assert!(with(&[("--raster-lossless", None)]).is_err());
        assert!(with(&[("--raster-method", Some("2"))]).is_err());
        assert!(with(&[("--images-batch-pages", Some("2"))]).is_err());
        assert!(with(&[("--scan-dpi", Some("200"))]).is_err());
        assert!(with(&[("--scan-pages", Some("skip"))]).unwrap().link_flags());
        // the link flags do not need it (the caller checks for markers)
        let s = with(&[("--images-link-prefix", Some("")), ("--raster-format", Some("png"))]).unwrap();
        assert!(s.link_flags() && s.link_prefix.is_empty());
    }
}
