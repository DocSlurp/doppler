//! `pdftomarkdown --images-resume DIR`: finish an earlier extraction whose batches stopped
//! for a reason that more time or budget can fix.
//!
//! The earlier run's `manifest.json` (version 2) must name the same PDF (`source_sha256`) and the
//! same settings (`settings_sha256`: the conversion, the page range, the links, the formats and
//! their encoder settings; see `Settings::output_canonical`), and list exactly the images this
//! conversion (or images plan) finds, at the same paths. Then only the images it lists as
//! `not-extracted` with a resumable reason ([`RESUMABLE`]: `budget`, `doc_budget`, `deadline`,
//! `timeout`, `aborted`) are extracted again, in fresh batches with their own budgets, under the
//! document ceilings less what DIR already holds; every other entry is kept as it was. Files go
//! to the paths the Markdown already links (so the Markdown never changes), are created new
//! (an existing file is never replaced) and the manifest is replaced atomically, with `resumes`
//! one more. A resume with nothing to do changes nothing.

use std::collections::HashMap;

use super::{Entry, Status};

/// The reasons an image may be extracted by a resume: its batch stopped (a budget, a time
/// limit, the abort rule), not anything about the image itself.
pub const RESUMABLE: [&str; 5] = ["budget", "doc_budget", "deadline", "timeout", "aborted"];

/// The largest manifest read back.
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

/// An earlier run's manifest: each image's entry and path, by id, and its resume count.
pub struct Previous {
    pub(super) entries: HashMap<String, (Entry, Option<String>)>,
    pub resumes: u64,
}

fn known(set: &[&'static str], v: &str) -> Option<&'static str> {
    set.iter().copied().find(|s| *s == v)
}

/// Reads `manifest` (the bytes of DIR's `manifest.json`), refusing one that is not version 2,
/// names another PDF or other settings, or is not one this program wrote.
pub fn read(manifest: &[u8], source_sha256: &str, settings_sha256: &str) -> Result<Previous, String> {
    let m: serde_json::Value = serde_json::from_slice(manifest).map_err(|_| "manifest.json is not JSON".to_string())?;
    if m["version"].as_u64() != Some(2) {
        return Err("manifest.json is not version 2".into());
    }
    if m["source_sha256"].as_str() != Some(source_sha256) {
        return Err("manifest.json was written for another PDF".into());
    }
    match m["settings_sha256"].as_str() {
        None => return Err("manifest.json has no settings digest (written before --images-resume)".into()),
        Some(d) if d != settings_sha256 => {
            return Err("manifest.json was written with other settings (conversion, page range, links, formats \
                 or scan thresholds)"
                .into());
        }
        Some(_) => {}
    }
    let resumes = m["resumes"].as_u64().unwrap_or(0);
    let images = m["images"].as_array().ok_or("manifest.json has no images")?;
    let bad = |id: &str, what: &str| format!("manifest.json: image {id:?}: {what}");
    let mut entries = HashMap::with_capacity(images.len());
    for im in images {
        let id = im["id"].as_str().ok_or("manifest.json: an image without an id")?;
        let reason = im["reason"].as_str();
        let status = match im["status"].as_str() {
            Some("extracted") => Status::Extracted,
            Some("unlinked") => Status::Unlinked,
            Some("not-extracted") => {
                Status::NotExtracted(reason.and_then(|r| known(&REASONS, r)).ok_or_else(|| bad(id, "bad reason"))?)
            }
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
    Ok(Previous { entries, resumes })
}

impl Entry {
    /// Not extracted for a reason a resume may retry ([`RESUMABLE`]).
    pub(super) fn resumable(&self) -> bool {
        matches!(self.status, Status::NotExtracted(r) if RESUMABLE.contains(&r))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest(version: u32, source: &str, settings: Option<&str>) -> String {
        let settings = settings.map(|s| format!(",\"settings_sha256\":\"{s}\"")).unwrap_or_default();
        format!(
            "{{\"version\":{version},\"source_sha256\":\"{source}\"{settings},\"resumes\":2,\"images\":[\
             {{\"id\":\"p1.1\",\"status\":\"extracted\",\"path\":\"page-0001/image-001.webp\",\"format\":\"webp\",\
             \"source\":\"render\",\"bytes\":7,\"sha256\":\"ab\",\"width\":3,\"height\":4}},\
             {{\"id\":\"p2.1\",\"status\":\"not-extracted\",\"reason\":\"budget\",\"path\":\"page-0002/image-001.webp\",\
             \"format\":\"webp\",\"bytes\":0}},\
             {{\"id\":\"p3.1\",\"status\":\"not-extracted\",\"reason\":\"min_side\",\"path\":null}}]}}"
        )
    }

    #[test]
    fn reads_a_manifest_bound_to_the_same_pdf_and_settings() {
        let p = read(manifest(2, "aa", Some("ss")).as_bytes(), "aa", "ss").unwrap();
        assert_eq!(p.resumes, 2);
        let (e, path) = &p.entries["p1.1"];
        assert_eq!(
            (e.status, e.format, e.bytes, path.as_deref()),
            (Status::Extracted, Some("webp"), 7, Some("page-0001/image-001.webp"))
        );
        assert!(p.entries["p2.1"].0.resumable());
        assert!(!p.entries["p3.1"].0.resumable());
        assert_eq!(p.entries["p2.1"].0.format, None);
    }

    #[test]
    fn refuses_another_version_pdf_or_settings() {
        assert!(read(manifest(1, "aa", Some("ss")).as_bytes(), "aa", "ss").is_err());
        assert!(read(manifest(2, "bb", Some("ss")).as_bytes(), "aa", "ss").is_err());
        assert!(read(manifest(2, "aa", Some("tt")).as_bytes(), "aa", "ss").is_err());
        assert!(read(manifest(2, "aa", None).as_bytes(), "aa", "ss").is_err());
        assert!(read(b"not json", "aa", "ss").is_err());
        let odd = manifest(2, "aa", Some("ss")).replace("\"min_side\"", "\"forged\"");
        assert!(read(odd.as_bytes(), "aa", "ss").is_err());
        let twice = manifest(2, "aa", Some("ss")).replace("\"p3.1\"", "\"p2.1\"");
        assert!(read(twice.as_bytes(), "aa", "ss").is_err());
    }
}
