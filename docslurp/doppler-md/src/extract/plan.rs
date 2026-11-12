//! The images plan: what image extraction needs from a conversion (the report's images, with
//! exact coordinates), written by one run (`--images-plan-out FILE`) and read by a later
//! `--images-dir` run (`--images-plan FILE`), which then renders and encodes without running
//! pdftotext or converting the document again. Extraction from a plan writes the same files
//! and manifest as extraction after a conversion of the same document with the same settings.
//!
//! A plan is bound to what made it: the first lines name the source PDF (SHA-256 and size),
//! the page range (`-f`/`-l`), a SHA-256 of the conversion settings and the converter version.
//! The reading run computes the same [`Binding`] from its own FILE and options and refuses a
//! plan that differs in any of them.
//!
//! Text. The header lines, tab-separated:
//! `doppler-images-plan 4`, `source SHA256-HEX SIZE`, `pages FIRST LAST` (`-` when not given),
//! `settings SHA256-HEX`, `version VERSION`; then one line per image:
//! `id-hex page x0 y0 x1 y1 px-w px-h flags fate obj scan`, where `id-hex` is the id's UTF-8 bytes in
//! hex, the box is four `f64` bit patterns in hex (exact, finite), `flags` is four of `0`/`1`
//! (mask, vector, text, plain), `fate` the report's word, `obj` `num.gen` or `-`, and `scan` how a
//! page scan's file is made (`jpeg`, `bilevel`, `decode`, `render`) or `-`. Ids are image ids
//! (`pN.M`, `pN.gK`), unique, and their page is the line's page, inside the page range.

use std::fmt::Write as _;

use std::collections::HashSet;

use sha2::{Digest, Sha256};

use crate::extract::outdir::valid_id;
use crate::images::{Fate, Note, ScanSource};

const HEADER: &str = "doppler-images-plan\t5";

/// What a plan is bound to: the conversion that wrote it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Binding {
    /// SHA-256 of the source PDF, lowercase hex
    pub source_sha256: String,
    pub source_len: u64,
    /// `-f` and `-l`, when given
    pub first: Option<u32>,
    pub last: Option<u32>,
    /// SHA-256 of the conversion settings ([`settings_digest`]), lowercase hex
    pub settings: String,
    /// the converter's version
    pub version: String,
}

impl Binding {
    /// The first field of `self` that differs from `other` (the plan's), as a message.
    pub fn mismatch(&self, plan: &Binding) -> Option<String> {
        let opt = |v: Option<u32>| v.map_or("-".to_string(), |n| n.to_string());
        if (self.source_sha256.as_str(), self.source_len) != (plan.source_sha256.as_str(), plan.source_len) {
            Some("written for another PDF (SHA-256 or size differs)".into())
        } else if (self.first, self.last) != (plan.first, plan.last) {
            Some(format!(
                "written for pages -f {} -l {}, this run has -f {} -l {}",
                opt(plan.first),
                opt(plan.last),
                opt(self.first),
                opt(self.last)
            ))
        } else if self.settings != plan.settings {
            Some(
                "written with other conversion settings (--params, --strict, --stream/--window, --math, --html, \
                 --images-link-prefix, --images-repeats, --raster-format, --vector-format, --scan-pages, \
                 --scan-max-words)"
                    .into(),
            )
        } else if self.version != plan.version {
            Some(format!("written by version {}, this is {}", plan.version, self.version))
        } else {
            None
        }
    }
}

/// SHA-256 (hex) of the conversion settings that decide the report's images.
pub fn settings_digest(canonical: &str) -> String {
    hex(&Sha256::digest(canonical.as_bytes()))
}

/// SHA-256 (hex) and length of everything `r` reads.
pub fn source_digest(r: &mut impl std::io::Read) -> std::io::Result<(String, u64)> {
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 16];
    let mut len = 0u64;
    loop {
        let n = match r.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        };
        h.update(&buf[..n]);
        len += n as u64;
    }
    Ok((hex(&h.finalize()), len))
}

fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(s, "{b:02x}");
    }
    s
}

fn is_hex64(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}
/// A plan larger than this is refused.
pub const MAX_PLAN_BYTES: u64 = 64 << 20;

const FATES: [Fate; 11] = [
    Fate::Block,
    Fate::Inline,
    Fate::Background,
    Fate::Furniture,
    Fate::Watermark,
    Fate::Small,
    Fate::Duplicate,
    Fate::Fragment,
    Fate::Grouped,
    Fate::Text,
    Fate::PageScan,
];

const SCAN_SOURCES: [ScanSource; 4] = [ScanSource::Jpeg, ScanSource::Bilevel, ScanSource::Decode, ScanSource::Render];

/// The plan of `notes`, bound to `b`.
pub fn write(b: &Binding, notes: &[Note]) -> String {
    let opt = |v: Option<u32>| v.map_or("-".to_string(), |n| n.to_string());
    let mut out = String::with_capacity(256 + notes.len() * 128);
    out.push_str(HEADER);
    let _ = write!(
        out,
        "\nsource\t{}\t{}\npages\t{}\t{}\nsettings\t{}\nversion\t{}\n",
        b.source_sha256,
        b.source_len,
        opt(b.first),
        opt(b.last),
        b.settings,
        b.version
    );
    for n in notes {
        for b in n.id.bytes() {
            let _ = write!(out, "{b:02x}");
        }
        let (x0, y0, x1, y1) = n.bbox;
        let _ = write!(
            out,
            "\t{}\t{:016x}\t{:016x}\t{:016x}\t{:016x}\t{}\t{}\t{}{}{}{}\t{}\t",
            n.page,
            x0.to_bits(),
            y0.to_bits(),
            x1.to_bits(),
            y1.to_bits(),
            n.px.0,
            n.px.1,
            u8::from(n.mask),
            u8::from(n.vector),
            u8::from(n.text),
            u8::from(n.plain),
            n.fate.as_str(),
        );
        match n.obj {
            Some((num, gen_)) => {
                let _ = write!(out, "{num}.{gen_}");
            }
            None => out.push('-'),
        }
        out.push('\t');
        out.push_str(n.scan.map_or("-", ScanSource::as_str));
        out.push('\n');
    }
    out
}

/// The binding and notes of a plan; `Err` for anything that is not a plan this module wrote:
/// a bad header, a malformed or non-finite field, an invalid or repeated id, or an id whose
/// page is not its line's page or lies outside the plan's page range.
pub fn read(text: &str) -> Result<(Binding, Vec<Note>), String> {
    if !text.ends_with('\n') {
        return Err("images plan: truncated".into());
    }
    let mut lines = text.split('\n');
    if lines.next() != Some(HEADER) {
        return Err("not an images plan (version 5)".into());
    }
    let binding = read_binding(&mut lines).ok_or("images plan: bad header")?;
    let mut notes = Vec::new();
    let mut ids = HashSet::new();
    for (k, line) in lines.enumerate() {
        if line.is_empty() {
            continue;
        }
        if notes.len() >= crate::MAX_DOC_IMAGES {
            return Err("images plan: too many images".into());
        }
        let bad = || format!("images plan: bad line {}", k + 6);
        let n = parse_line(line).ok_or_else(bad)?;
        let in_range = binding.first.is_none_or(|f| n.page >= f) && binding.last.is_none_or(|l| n.page <= l);
        if id_page(&n.id) != Some(n.page) || !in_range {
            return Err(format!(
                "images plan: line {}: the id's page is not the line's page or is outside the page range",
                k + 6
            ));
        }
        if !ids.insert(n.id.clone()) {
            return Err(format!("images plan: line {}: repeated id", k + 6));
        }
        notes.push(n);
    }
    Ok((binding, notes))
}

/// The page of a valid image id.
fn id_page(id: &str) -> Option<u32> {
    if !valid_id(id) {
        return None;
    }
    id[1..].split_once('.')?.0.parse().ok()
}

fn read_binding<'a>(lines: &mut impl Iterator<Item = &'a str>) -> Option<Binding> {
    let mut field = |name: &str, n: usize| -> Option<Vec<&'a str>> {
        let f: Vec<&str> = lines.next()?.split('\t').collect();
        (f.len() == n + 1 && f[0] == name).then(|| f[1..].to_vec())
    };
    let page = |s: &str| -> Option<Option<u32>> {
        match s {
            "-" => Some(None),
            s if !s.starts_with('0') && !s.starts_with('+') => s.parse::<u32>().ok().filter(|n| *n >= 1).map(Some),
            _ => None,
        }
    };
    let src = field("source", 2)?;
    let pages = field("pages", 2)?;
    let settings = field("settings", 1)?;
    let version = field("version", 1)?;
    let b = Binding {
        source_sha256: src[0].to_string(),
        source_len: src[1].parse().ok()?,
        first: page(pages[0])?,
        last: page(pages[1])?,
        settings: settings[0].to_string(),
        version: version[0].to_string(),
    };
    (is_hex64(&b.source_sha256) && is_hex64(&b.settings) && !b.version.is_empty()).then_some(b)
}

fn parse_line(line: &str) -> Option<Note> {
    let f: Vec<&str> = line.split('\t').collect();
    if f.len() != 12 {
        return None;
    }
    let id = unhex(f[0])?;
    let page: u32 = f[1].parse().ok()?;
    let bits = |s: &str| {
        (s.len() == 16).then(|| u64::from_str_radix(s, 16).ok()).flatten().map(f64::from_bits).filter(|v| v.is_finite())
    };
    let bbox = (bits(f[2])?, bits(f[3])?, bits(f[4])?, bits(f[5])?);
    let px = (f[6].parse().ok()?, f[7].parse().ok()?);
    let flags = f[8].as_bytes();
    if flags.len() != 4 || !flags.iter().all(|b| *b == b'0' || *b == b'1') {
        return None;
    }
    let fate = *FATES.iter().find(|x| x.as_str() == f[9])?;
    let obj = match f[10] {
        "-" => None,
        s => {
            let (num, gen_) = s.split_once('.')?;
            Some((num.parse().ok()?, gen_.parse().ok()?))
        }
    };
    let scan = match f[11] {
        "-" => None,
        s => Some(*SCAN_SOURCES.iter().find(|x| x.as_str() == s)?),
    };
    Some(Note {
        id,
        page,
        bbox,
        px,
        mask: flags[0] == b'1',
        vector: flags[1] == b'1',
        fate,
        members: 1,
        group: None,
        obj,
        plain: flags[3] == b'1',
        text: flags[2] == b'1',
        file: None,
        degraded: false,
        link: None,
        repeat_of: None,
        scan,
    })
}

fn unhex(s: &str) -> Option<String> {
    if s.len() % 2 != 0 || s.len() > 2048 {
        return None;
    }
    let bytes: Option<Vec<u8>> =
        (0..s.len()).step_by(2).map(|i| u8::from_str_radix(s.get(i..i + 2)?, 16).ok()).collect();
    String::from_utf8(bytes?).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(id: &str, vector: bool, obj: Option<(u32, u32)>) -> Note {
        Note {
            id: id.into(),
            page: 3,
            bbox: (0.1 + 0.2, 72.123456789, 300.0000001, 1e-9),
            px: (640, 480),
            mask: false,
            vector,
            fate: Fate::Block,
            members: 1,
            group: None,
            obj,
            plain: true,
            text: true,
            file: None,
            degraded: false,
            link: None,
            repeat_of: None,
            scan: None,
        }
    }

    fn binding() -> Binding {
        Binding {
            source_sha256: "ab".repeat(32),
            source_len: 1234,
            first: Some(2),
            last: Some(5),
            settings: settings_digest("x"),
            version: "0.1.0".into(),
        }
    }

    #[test]
    fn round_trips_exactly() {
        let mut notes = vec![note("p3.1", false, Some((12, 0))), note("p3.g1", true, None), note("p3.2", false, None)];
        notes[0].fate = Fate::PageScan;
        notes[0].scan = Some(ScanSource::Jpeg);
        let (b, back) = read(&write(&binding(), &notes)).unwrap();
        assert_eq!(b, binding());
        assert_eq!(back.len(), 3);
        for (a, b) in notes.iter().zip(&back) {
            assert_eq!(a.id, b.id);
            assert_eq!(a.page, b.page);
            assert_eq!(a.bbox.0.to_bits(), b.bbox.0.to_bits());
            assert_eq!(a.bbox.3.to_bits(), b.bbox.3.to_bits());
            assert_eq!(
                (a.px, a.mask, a.vector, a.text, a.plain, a.fate, a.obj, a.scan),
                (b.px, b.mask, b.vector, b.text, b.plain, b.fate, b.obj, b.scan)
            );
        }
        let open = Binding { first: None, last: None, ..binding() };
        assert_eq!(read(&write(&open, &notes)).unwrap().0, open);
    }

    #[test]
    fn refuses_anything_else() {
        assert!(read("").is_err());
        assert!(read("doppler-images-plan\t1\n").is_err());
        assert!(read(&write(&binding(), &[note("p3.1", false, None)]).replace("plan\t5", "plan\t4")).is_err());
        let good = write(&binding(), &[note("p3.1", false, None)]);
        assert!(read(&good).is_ok());
        assert!(read(&good[..good.len() - 1]).is_err());
        assert!(read(&good.replace("block", "nope")).is_err());
        assert!(read(&good.replace("\t-\n", "\tcopy\n")).is_err(), "unknown scan source");
        assert!(read(&good.replace("\nversion\t0.1.0\n", "\n")).is_err(), "header line missing");
        assert!(read(&good.replace("pages\t2\t5", "pages\t0\t5")).is_err());
        assert!(read(&good.replace("pages\t2\t5", "pages\t02\t5")).is_err());
        assert!(read(&(good.clone() + "zz\n")).is_err());
    }

    /// The plan lines a hostile or stale writer could make: each is refused.
    #[test]
    fn refuses_bad_image_lines() {
        let b = binding();
        let one = |n: Note| read(&write(&b, &[n]));
        // non-finite coordinates (NaN would otherwise reach the manifest as invalid JSON)
        for v in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let mut n = note("p3.1", false, None);
            n.bbox.0 = v;
            assert!(one(n).is_err(), "{v}");
        }
        // ids that are not image ids
        for id in ["odd\tid", "p3", "p03.1", "../p3.1", "p3.1\"x", "p3.0"] {
            assert!(one(note(id, false, None)).is_err(), "{id:?}");
        }
        // the id's page is not the line's page
        let mut n = note("p999.1", false, None);
        n.page = 3;
        assert!(one(n).is_err());
        // outside the plan's page range (-f 2 -l 5)
        let mut n = note("p6.1", false, None);
        n.page = 6;
        assert!(one(n).is_err());
        let mut n = note("p1.1", false, None);
        n.page = 1;
        assert!(one(n).is_err());
        // a repeated id
        assert!(read(&write(&b, &[note("p3.1", false, None), note("p3.1", false, None)])).is_err());
    }

    #[test]
    fn mismatch_names_the_field() {
        let b = binding();
        assert_eq!(b.mismatch(&b), None);
        let other = |f: &dyn Fn(&mut Binding)| {
            let mut x = binding();
            f(&mut x);
            b.mismatch(&x).unwrap()
        };
        assert!(other(&|x| x.source_sha256 = "cd".repeat(32)).contains("another PDF"));
        assert!(other(&|x| x.source_len = 1).contains("another PDF"));
        assert!(other(&|x| x.first = None).contains("pages"));
        assert!(other(&|x| x.last = Some(6)).contains("pages"));
        assert!(other(&|x| x.settings = settings_digest("y")).contains("settings"));
        assert!(other(&|x| x.version = "9".into()).contains("version"));
    }

    #[test]
    fn source_digest_is_sha256() {
        let (h, n) = source_digest(&mut &b"abc"[..]).unwrap();
        assert_eq!(n, 3);
        assert_eq!(h, "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    }
}
