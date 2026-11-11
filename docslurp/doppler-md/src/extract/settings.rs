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
    pub min_side: MinSide,
    pub repeats: Repeats,
    pub raster_format: RasterFormat,
    pub raster_quality: u8,
    pub raster_lossless: bool,
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
    /// an `--images-*` flag other than `--images-dir` was given
    other_flags: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            dir: None,
            min_side: MinSide::Px(24.0),
            repeats: Repeats::Skip,
            raster_format: RasterFormat::Webp,
            raster_quality: 90,
            raster_lossless: false,
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
            other_flags: false,
        }
    }
}

/// Every flag that takes a value.
const WITH_VALUE: &[&str] = &[
    "--images-dir",
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
];

impl Settings {
    /// Whether `name` is one of these flags, and whether it takes a value.
    pub fn takes(name: &str) -> Option<bool> {
        if name == "--raster-lossless" {
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
        let v = value.ok_or_else(|| format!("{name} needs a value"))?;
        let bad = || format!("{name}: bad value {v:?}");
        if name != "--images-dir" {
            self.other_flags = true;
        }
        match name {
            "--images-dir" => {
                if v.is_empty() {
                    return Err(bad());
                }
                self.dir = Some(PathBuf::from(v));
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
            "--images-pages" => {
                let (a, b) = v.split_once('-').unwrap_or((v, v));
                let (a, b) = (int(a, 1, i32::MAX as u64).ok_or_else(bad)?, int(b, 1, i32::MAX as u64).ok_or_else(bad)?);
                if a > b {
                    return Err(bad());
                }
                self.pages = Some((a as u32, b as u32));
            }
            "--images-timeout" => self.timeout = Duration::from_secs(int(v, 1, 86400).ok_or_else(bad)?),
            _ => return Err(format!("unknown option {name}")),
        }
        Ok(())
    }

    /// The flags given make sense together.
    pub fn check(&self) -> Result<(), String> {
        if self.other_flags && self.dir.is_none() {
            return Err(
                "the --images-*, --raster-*, --lossless-*, --vector-* and --svg-* options need --images-dir".into()
            );
        }
        Ok(())
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
        ])
        .unwrap();
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
            ("--images-dir", ""),
        ] {
            assert!(with(&[("--images-dir", Some("o")), (k, Some(v))]).is_err(), "{k} {v}");
        }
        assert!(with(&[("--images-dpi", None)]).is_err());
        // the others need --images-dir
        assert!(with(&[("--images-dpi", Some("100"))]).is_err());
        assert!(with(&[("--raster-lossless", None)]).is_err());
        assert!(with(&[("--raster-method", Some("2"))]).is_err());
    }
}
