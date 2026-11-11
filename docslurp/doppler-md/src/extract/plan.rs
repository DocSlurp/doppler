//! The images plan: what image extraction needs from a conversion (the report's images, with
//! exact coordinates), written by one run (`--images-plan-out FILE`) and read by a later
//! `--images-dir` run (`--images-plan FILE`), which then renders and encodes without running
//! pdftotext or converting the document again. Extraction from a plan writes the same files
//! and manifest as extraction after a conversion of the same document with the same settings.
//!
//! Text, one line per image after a header line; fields are tab-separated:
//! `id-hex page x0 y0 x1 y1 px-w px-h flags fate obj`, where `id-hex` is the id's UTF-8 bytes in
//! hex, the box is four `f64` bit patterns in hex (exact), `flags` is three of `0`/`1` (mask,
//! vector, text), `fate` the report's word, and `obj` `num.gen` or `-`.

use std::fmt::Write as _;

use crate::images::{Fate, Note};

const HEADER: &str = "doppler-images-plan\t1";
/// A plan larger than this is refused.
pub const MAX_PLAN_BYTES: u64 = 64 << 20;

const FATES: [Fate; 10] = [
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
];

/// The plan of `notes`.
pub fn write(notes: &[Note]) -> String {
    let mut out = String::with_capacity(64 + notes.len() * 128);
    out.push_str(HEADER);
    out.push('\n');
    for n in notes {
        for b in n.id.bytes() {
            let _ = write!(out, "{b:02x}");
        }
        let (x0, y0, x1, y1) = n.bbox;
        let _ = write!(
            out,
            "\t{}\t{:016x}\t{:016x}\t{:016x}\t{:016x}\t{}\t{}\t{}{}{}\t{}\t",
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
            n.fate.as_str(),
        );
        match n.obj {
            Some((num, gen_)) => {
                let _ = write!(out, "{num}.{gen_}");
            }
            None => out.push('-'),
        }
        out.push('\n');
    }
    out
}

/// The notes of a plan; `Err` for anything that is not a plan this module wrote.
pub fn read(text: &str) -> Result<Vec<Note>, String> {
    let mut lines = text.split('\n');
    if lines.next() != Some(HEADER) {
        return Err("not an images plan".into());
    }
    let mut notes = Vec::new();
    for (k, line) in lines.enumerate() {
        if line.is_empty() {
            continue;
        }
        if notes.len() >= crate::MAX_DOC_IMAGES {
            return Err("images plan: too many images".into());
        }
        notes.push(parse_line(line).ok_or_else(|| format!("images plan: bad line {}", k + 2))?);
    }
    if !text.ends_with('\n') {
        return Err("images plan: truncated".into());
    }
    Ok(notes)
}

fn parse_line(line: &str) -> Option<Note> {
    let f: Vec<&str> = line.split('\t').collect();
    if f.len() != 11 {
        return None;
    }
    let id = unhex(f[0])?;
    let page: u32 = f[1].parse().ok()?;
    let bits = |s: &str| (s.len() == 16).then(|| u64::from_str_radix(s, 16).ok()).flatten().map(f64::from_bits);
    let bbox = (bits(f[2])?, bits(f[3])?, bits(f[4])?, bits(f[5])?);
    let px = (f[6].parse().ok()?, f[7].parse().ok()?);
    let flags = f[8].as_bytes();
    if flags.len() != 3 || !flags.iter().all(|b| *b == b'0' || *b == b'1') {
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
        text: flags[2] == b'1',
        file: None,
        degraded: false,
    })
}

fn unhex(s: &str) -> Option<String> {
    if s.len() % 2 != 0 || s.len() > 2048 {
        return None;
    }
    let bytes: Option<Vec<u8>> = (0..s.len()).step_by(2).map(|i| u8::from_str_radix(s.get(i..i + 2)?, 16).ok()).collect();
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
            text: true,
            file: None,
            degraded: false,
        }
    }

    #[test]
    fn round_trips_exactly() {
        let notes = vec![note("p3.1", false, Some((12, 0))), note("p3.g1", true, None), note("odd\tid", false, None)];
        let back = read(&write(&notes)).unwrap();
        assert_eq!(back.len(), 3);
        for (a, b) in notes.iter().zip(&back) {
            assert_eq!(a.id, b.id);
            assert_eq!(a.page, b.page);
            assert_eq!(a.bbox.0.to_bits(), b.bbox.0.to_bits());
            assert_eq!(a.bbox.3.to_bits(), b.bbox.3.to_bits());
            assert_eq!((a.px, a.mask, a.vector, a.text, a.fate, a.obj), (b.px, b.mask, b.vector, b.text, b.fate, b.obj));
        }
    }

    #[test]
    fn refuses_anything_else() {
        assert!(read("").is_err());
        assert!(read("doppler-images-plan\t1\nzz\n").is_err());
        let good = write(&[note("p3.1", false, None)]);
        assert!(read(&good[..good.len() - 1]).is_err());
        assert!(read(&good.replace("block", "nope")).is_err());
    }
}
