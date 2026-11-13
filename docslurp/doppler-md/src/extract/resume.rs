//! `pdftomarkdown --images-resume DIR`: finish an earlier extraction whose batches stopped
//! for a reason that more time or budget can fix.
//!
//! The earlier run's `manifest.jsonl` (schema `doppler-images/3`, read line by line and refused
//! unless whole: see [`super::manifest`]) must name the same PDF (`source_sha256`) and the
//! same settings (`settings_sha256`: the conversion, the page range, the links, the formats and
//! their encoder settings; see `Settings::output_canonical`), and list exactly the images this
//! conversion (or images plan) finds, at the same paths. Then only the images it lists as
//! `not-extracted` with a resumable reason ([`RESUMABLE`]: `budget`, `doc_budget`, `deadline`,
//! `timeout`, `aborted`) are extracted again, in fresh batches with their own budgets, under the
//! document ceilings less what DIR already holds; every other entry is kept as it was. Files go
//! to the paths the Markdown already links (so the Markdown never changes), are created new
//! (an existing file is never replaced) and the manifest is replaced atomically, with `resumes`
//! one more; the earlier batch lines of batches with no retried page are kept. A resume with
//! nothing to do changes nothing.

use std::collections::HashMap;

use super::manifest::{Kind, Line, NAME, Reader};
use super::{Batch, Entry, STOPS, Status};

/// The reasons an image may be extracted by a resume: its batch stopped (a budget, a time
/// limit, the abort rule), not anything about the image itself.
pub const RESUMABLE: [&str; 5] = ["budget", "doc_budget", "deadline", "timeout", "aborted"];

/// The largest manifest read back (its entries are held for the resume).
pub const MAX_MANIFEST_BYTES: u64 = 256 << 20;

/// Every reason the manifest may give, as the `&'static str` the entries hold.
const REASONS: [&str; 16] = [
    "budget",
    "doc_budget",
    "deadline",
    "timeout",
    "aborted",
    "renderer",
    "bbox",
    "copy",
    "encode",
    "error",
    "file_bytes",
    "id",
    "min_side",
    "pages",
    "repeat",
    "scan-pages",
];
const FORMATS: [&str; 4] = ["webp", "png", "svg", "jpg"];
const SOURCES: [&str; 3] = ["render", "copy", "decode"];

/// An earlier run's manifest: each image's entry and path, by id, its batch lines (a resume
/// carries forward those of batches it does not run again) and its resume count.
pub struct Previous {
    pub(super) entries: HashMap<String, (Entry, Option<String>)>,
    pub(super) batches: Vec<Batch>,
    pub resumes: u64,
}

fn known(set: &[&'static str], v: &str) -> Option<&'static str> {
    set.iter().copied().find(|s| *s == v)
}

/// Reads DIR's `manifest.jsonl` from `r` one line at a time, refusing one that is not
/// `doppler-images/3`, is truncated or altered (end line, counts, hash), names another PDF or
/// other settings, or is not one this program wrote.
pub fn read(r: impl std::io::BufRead, source_sha256: &str, settings_sha256: &str) -> Result<Previous, String> {
    let mut lines = Reader::new(r);
    let mut resumes = 0;
    let mut entries = HashMap::new();
    let mut batches = Vec::new();
    let bad = |id: &str, what: &str| format!("{NAME}: image {id:?}: {what}");
    // the first problem with a line's contents is reported once the whole file has been read
    // and checked (end line, counts, hash): a damaged file is refused as damaged
    let mut first: Option<String> = None;
    while let Some(Line { kind, value: im }) = lines.next()? {
        if first.is_some() {
            continue;
        }
        let checked = (|| -> Result<(), String> {
            match kind {
                Kind::Header => {
                    if im["source_sha256"].as_str() != Some(source_sha256) {
                        return Err(format!("{NAME} was written for another PDF"));
                    }
                    if im["settings_sha256"].as_str() != Some(settings_sha256) {
                        return Err(format!(
                            "{NAME} was written with other settings (conversion, page range, links, formats \
                         or scan thresholds)"
                        ));
                    }
                    resumes = im["resumes"].as_u64().ok_or_else(|| format!("{NAME}: header without resumes"))?;
                }
                Kind::Image => {
                    let id = im["id"].as_str().ok_or_else(|| format!("{NAME}: an image without an id"))?;
                    let reason = im["reason"].as_str();
                    let status = match im["status"].as_str() {
                        Some("extracted") => Status::Extracted,
                        Some("unlinked") => Status::Unlinked,
                        Some("not-extracted") => Status::NotExtracted(
                            reason.and_then(|r| known(&REASONS, r)).ok_or_else(|| bad(id, "bad reason"))?,
                        ),
                        _ => return Err(bad(id, "bad status")),
                    };
                    let text = |k: &str| im[k].as_str().map(str::to_string);
                    let num = |k: &str| im[k].as_u64().unwrap_or(0);
                    let format = match im["format"].as_str() {
                        Some(f) => Some(known(&FORMATS, f).ok_or_else(|| bad(id, "bad format"))?),
                        None => None,
                    };
                    let source = match im["source"].as_str() {
                        Some(f) => Some(known(&SOURCES, f).ok_or_else(|| bad(id, "bad source"))?),
                        None => None,
                    };
                    let entry = Entry {
                        status,
                        error: text("error"),
                        // an extracted entry's format and source are its file's; others are not kept
                        format: if status == Status::Extracted { format } else { None },
                        source,
                        bytes: num("bytes"),
                        sha256: text("sha256"),
                        width: u32::try_from(num("width")).map_err(|_| bad(id, "bad width"))?,
                        height: u32::try_from(num("height")).map_err(|_| bad(id, "bad height"))?,
                        repeat_of: text("repeat_of"),
                        fallback: text("fallback"),
                    };
                    if entries.insert(id.to_string(), (entry, text("path"))).is_some() {
                        return Err(bad(id, "listed twice"));
                    }
                }
                Kind::Batch => {
                    let bad = || format!("{NAME}: a bad batch line");
                    let page =
                        |v: &serde_json::Value| v.as_u64().and_then(|p| u32::try_from(p).ok()).filter(|p| *p > 0);
                    let pages = match im["pages"].as_array().map(Vec::as_slice) {
                        Some([a, b]) => match (page(a), page(b)) {
                            (Some(a), Some(b)) if a <= b => (a, b),
                            _ => return Err(bad()),
                        },
                        _ => return Err(bad()),
                    };
                    let status = im["status"]
                        .as_str()
                        .and_then(|v| if v == "ok" { Some("ok") } else { known(&STOPS, v) })
                        .ok_or_else(bad)?;
                    let cut_page = match &im["cut_page"] {
                        serde_json::Value::Null => None,
                        v => Some(page(v).ok_or_else(bad)?),
                    };
                    batches.push(Batch { pages, status, cut_page });
                }
                Kind::End => {}
            }
            Ok(())
        })();
        first = checked.err();
    }
    match first {
        Some(e) => Err(e),
        None => Ok(Previous { entries, batches, resumes }),
    }
}

impl Entry {
    /// Not extracted for a reason a resume may retry ([`RESUMABLE`]).
    pub(super) fn resumable(&self) -> bool {
        matches!(self.status, Status::NotExtracted(r) if RESUMABLE.contains(&r))
    }
}

#[cfg(test)]
mod tests {
    use super::super::manifest::Writer;
    use super::*;

    fn manifest(schema: &str, source: &str, settings: &str) -> String {
        let mut w = Writer::new(Vec::new());
        w.header(&format!(
            "\"schema\":\"{schema}\",\"source_sha256\":\"{source}\",\"settings_sha256\":\"{settings}\",\"resumes\":2"
        ))
        .unwrap();
        w.image(
            "\"id\":\"p1.1\",\"status\":\"extracted\",\"path\":\"page-0001/image-001.webp\",\"format\":\"webp\",\
             \"source\":\"render\",\"bytes\":7,\"sha256\":\"ab\",\"width\":3,\"height\":4",
        )
        .unwrap();
        w.image(
            "\"id\":\"p2.1\",\"status\":\"not-extracted\",\"reason\":\"budget\",\"path\":\"page-0002/image-001.webp\",\
             \"format\":\"webp\",\"bytes\":0",
        )
        .unwrap();
        w.image("\"id\":\"p3.1\",\"status\":\"not-extracted\",\"reason\":\"min_side\",\"path\":null").unwrap();
        w.batch("\"pages\":[1,64],\"status\":\"budget\",\"cut_page\":2").unwrap();
        let out = w.finish("\"complete\":false,\"resumable\":true,\"totals\":{\"images\":3,\"batches\":1}").unwrap();
        String::from_utf8(out).unwrap()
    }

    /// The same lines with a fresh hash (a forged but whole manifest).
    fn rehash(text: &str) -> String {
        let mut lines: Vec<&str> = text.lines().collect();
        let end = lines.pop().unwrap();
        let body: String = lines.iter().map(|l| format!("{l}\n")).collect();
        use sha2::Digest;
        let sum: String = sha2::Sha256::digest(body.as_bytes()).iter().map(|b| format!("{b:02x}")).collect();
        let at = end.rfind("\"sha256\":\"").unwrap() + 10;
        format!("{body}{}{sum}\"}}\n", &end[..at])
    }

    #[test]
    fn reads_a_manifest_bound_to_the_same_pdf_and_settings() {
        let p = read(manifest("doppler-images/3", "aa", "ss").as_bytes(), "aa", "ss").unwrap();
        assert_eq!(p.resumes, 2);
        let (e, path) = &p.entries["p1.1"];
        assert_eq!(
            (e.status, e.format, e.bytes, path.as_deref()),
            (Status::Extracted, Some("webp"), 7, Some("page-0001/image-001.webp"))
        );
        assert!(p.entries["p2.1"].0.resumable());
        assert!(!p.entries["p3.1"].0.resumable());
        assert_eq!(p.entries["p2.1"].0.format, None);
        // the batch lines are kept (a resume carries forward those it does not run again)
        let b: Vec<_> = p.batches.iter().map(|b| (b.pages, b.status, b.cut_page)).collect();
        assert_eq!(b, [((1, 64), "budget", Some(2))]);
    }

    #[test]
    fn refuses_another_schema_pdf_or_settings() {
        let ok = manifest("doppler-images/3", "aa", "ss");
        assert!(read(manifest("doppler-images/2", "aa", "ss").as_bytes(), "aa", "ss").is_err());
        assert!(read(manifest("doppler-images/3", "bb", "ss").as_bytes(), "aa", "ss").is_err());
        assert!(read(manifest("doppler-images/3", "aa", "tt").as_bytes(), "aa", "ss").is_err());
        assert!(read(&b"not json\n"[..], "aa", "ss").is_err());
        // version 2: one JSON object
        let v2 = "{\"version\":2,\"source_sha256\":\"aa\",\"settings_sha256\":\"ss\",\"resumes\":0,\n\"batches\":[\n],\n\"images\":[\n]}\n";
        assert!(read(v2.as_bytes(), "aa", "ss").is_err());
        // altered without a new hash; truncated
        let forged = ok.replace("\"min_side\"", "\"forged\"");
        assert!(read(forged.as_bytes(), "aa", "ss").err().unwrap().contains("sha256"));
        let cut = &ok[..ok.trim_end().rfind('\n').unwrap() + 1];
        assert!(read(cut.as_bytes(), "aa", "ss").err().unwrap().contains("no end line"));
        // whole (hash recomputed) but not one this program wrote
        let odd = rehash(&forged);
        assert!(read(odd.as_bytes(), "aa", "ss").err().unwrap().contains("bad reason"));
        let twice = rehash(&ok.replace("\"p3.1\"", "\"p2.1\""));
        assert!(read(twice.as_bytes(), "aa", "ss").err().unwrap().contains("twice"));
        assert!(read(rehash(&ok).as_bytes(), "aa", "ss").is_ok());
        // a batch line this program would not write
        let batch = "\"pages\":[1,64],\"status\":\"budget\",\"cut_page\":2";
        for bad in [
            "\"pages\":[2,1],\"status\":\"budget\",\"cut_page\":2",
            "\"pages\":[0,64],\"status\":\"budget\",\"cut_page\":2",
            "\"pages\":[1],\"status\":\"budget\",\"cut_page\":2",
            "\"pages\":[1,64],\"status\":\"forged\",\"cut_page\":2",
            "\"pages\":[1,64],\"status\":\"budget\",\"cut_page\":\"2\"",
        ] {
            let e = read(rehash(&ok.replace(batch, bad)).as_bytes(), "aa", "ss").err().unwrap();
            assert!(e.contains("batch"), "{bad}: {e}");
        }
    }
}
