//! pdftomarkdown's scan-test thresholds: parsed and checked as pdftotext reads them, passed
//! on to it, and spelled canonically for the images plan's settings digest. Included by
//! pdftomarkdown with `mod scan_thresholds;` (not part of the library API).

use std::ffi::OsString;

/// The scan test's thresholds as given to pdftomarkdown (`--scan-min-coverage`,
/// `--scan-max-words`, `--scan-layer-overlap`), passed on to pdftotext; unset ones keep
/// pdftotext's defaults.
#[derive(Debug, Default, Clone)]
pub struct ScanThresholds {
    min_coverage: Option<(f64, String)>,
    max_words: Option<u32>,
    layer_overlap: Option<(f64, String)>,
}

impl ScanThresholds {
    pub const DEFAULT_MIN_COVERAGE: f64 = 0.85;
    pub const DEFAULT_MAX_WORDS: u32 = 20;
    pub const DEFAULT_LAYER_OVERLAP: f64 = 0.5;

    /// Takes the value of one of the three options; `Ok(false)` for another option.
    pub fn apply(&mut self, flag: &str, value: Option<&str>) -> Result<bool, String> {
        match flag {
            "--scan-min-coverage" => self.min_coverage = Some(parse_share(flag, value)?),
            "--scan-layer-overlap" => self.layer_overlap = Some(parse_share(flag, value)?),
            "--scan-max-words" => match value.and_then(|n| n.parse::<u32>().ok()) {
                Some(n) if n <= i32::MAX as u32 => self.max_words = Some(n),
                _ => return Err("--scan-max-words needs a number".into()),
            },
            _ => return Ok(false),
        }
        Ok(true)
    }

    /// pdftotext's options for them (none for those not given).
    pub fn pdftotext_args(&self) -> Vec<OsString> {
        let mut a: Vec<OsString> = Vec::new();
        if let Some((_, text)) = &self.min_coverage {
            a.extend(["-scan-min-coverage".into(), text.into()]);
        }
        if let Some(n) = self.max_words {
            a.extend(["-scan-max-words".into(), n.to_string().into()]);
        }
        if let Some((_, text)) = &self.layer_overlap {
            a.extend(["-scan-layer-overlap".into(), text.into()]);
        }
        a
    }

    /// The thresholds in effect, for the images plan's settings digest: equal values (0.5 and
    /// .50, or a default spelled out) give the same text.
    pub fn canonical(&self) -> String {
        format!(
            "scan-thresholds {:?} {} {:?}\n",
            self.min_coverage.as_ref().map_or(Self::DEFAULT_MIN_COVERAGE, |c| c.0),
            self.max_words.unwrap_or(Self::DEFAULT_MAX_WORDS),
            self.layer_overlap.as_ref().map_or(Self::DEFAULT_LAYER_OVERLAP, |c| c.0)
        )
    }
}

/// A share of the page in (0, 1] as pdftotext reads it: a plain decimal (digits and at most one
/// point; no sign, exponent, inf or nan), at most 30 bytes (pdftotext's buffer).
fn parse_share(flag: &str, value: Option<&str>) -> Result<(f64, String), String> {
    let err = || format!("{flag} needs a share of the page in (0,1], such as 0.85");
    let v = value.ok_or_else(err)?;
    let plain = v.len() <= 30
        && v.bytes().any(|b| b.is_ascii_digit())
        && v.bytes().all(|b| b.is_ascii_digit() || b == b'.')
        && v.bytes().filter(|&b| b == b'.').count() <= 1;
    match v.parse::<f64>() {
        Ok(x) if plain && x.is_finite() && x > 0.0 && x <= 1.0 => Ok((x, v.to_string())),
        _ => Err(err()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shares_are_plain_decimals_in_the_unit_interval() {
        for ok in ["0.85", ".5", "1", "1.", "00.5", "0.000001"] {
            assert!(parse_share("--x", Some(ok)).is_ok(), "{ok}");
        }
        for bad in
            ["0", "0.0", "-0.5", "+0.5", "1.01", "2", "nan", "inf", "1e-1", "0x1p-1", "", ".", "0.5.", " 0.5", "0.5 "]
        {
            assert!(parse_share("--x", Some(bad)).is_err(), "{bad:?}");
        }
        assert!(parse_share("--x", Some(&format!("0.{}1", "0".repeat(29)))).is_err(), "longer than pdftotext's buffer");
        assert!(parse_share("--x", None).is_err());
    }

    #[test]
    fn canonical_text_is_by_value() {
        let mut a = ScanThresholds::default();
        let mut b = ScanThresholds::default();
        b.apply("--scan-min-coverage", Some("0.850")).unwrap();
        b.apply("--scan-max-words", Some("20")).unwrap();
        b.apply("--scan-layer-overlap", Some(".5")).unwrap();
        assert_eq!(a.canonical(), b.canonical());
        a.apply("--scan-layer-overlap", Some("0.4")).unwrap();
        assert_ne!(a.canonical(), b.canonical());
        assert_eq!(
            b.pdftotext_args(),
            ["-scan-min-coverage", "0.850", "-scan-max-words", "20", "-scan-layer-overlap", ".5"]
                .map(OsString::from)
                .to_vec()
        );
        assert_eq!(b.apply("--scan-pages", Some("image")), Ok(false));
        assert!(b.apply("--scan-max-words", Some("-1")).is_err());
    }
}
