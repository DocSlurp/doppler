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
//! one more; the earlier batch lines that still describe a page no newer line covers are kept
//! (see `merge_batches`). A resume with nothing to do changes nothing.

use std::collections::{BTreeSet, HashMap};

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
/// other settings, or is not one this program would write. A batch line must be well formed
/// and consistent: `ok` with no `cut_page`, a `cut_page` within its own `pages`, a range that
/// holds a page with an image, a `run` no later than the header's `resumes`, and no overlap
/// with another line of the same run. There are at most as many batch lines as pages with
/// images: every line this program keeps has such a page no other line kept claims (see
/// `merge_batches`), whatever `--images-batch-pages` each run used.
pub fn read(r: impl std::io::BufRead, source_sha256: &str, settings_sha256: &str) -> Result<Previous, String> {
    let mut lines = Reader::new(r);
    let mut resumes = 0;
    let mut entries = HashMap::new();
    let mut batches: Vec<Batch> = Vec::new();
    // the pages with an image (the image lines come first)
    let mut pages: BTreeSet<u32> = BTreeSet::new();
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
                    let page = im["page"]
                        .as_u64()
                        .and_then(|p| u32::try_from(p).ok())
                        .filter(|p| *p > 0)
                        .ok_or_else(|| bad(id, "bad page"))?;
                    pages.insert(page);
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
                    let range = match im["pages"].as_array().map(Vec::as_slice) {
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
                    let run = im["run"].as_u64().filter(|r| *r <= resumes).ok_or_else(bad)?;
                    let (a, b) = range;
                    if (status == "ok" && cut_page.is_some()) || cut_page.is_some_and(|c| c < a || c > b) {
                        return Err(format!("{NAME}: a batch line whose cut_page is not one of its pages"));
                    }
                    if pages.range(a..=b).next().is_none() {
                        return Err(format!("{NAME}: a batch line for pages {a}-{b}, which have no image"));
                    }
                    if batches.len() >= pages.len() {
                        return Err(format!("{NAME}: more batch lines than pages with images"));
                    }
                    batches.push(Batch { pages: range, status, cut_page, run });
                }
                Kind::End => {}
            }
            Ok(())
        })();
        first = checked.err();
    }
    if let Some(e) = first {
        return Err(e);
    }
    // the lines of one run never overlap (nor repeat)
    let mut by_run: Vec<&Batch> = batches.iter().collect();
    by_run.sort_by_key(|b| (b.run, b.pages));
    if by_run.windows(2).any(|w| w[0].run == w[1].run && w[1].pages.0 <= w[0].pages.1) {
        return Err(format!("{NAME}: two batch lines of one run overlap"));
    }
    Ok(Previous { entries, batches, resumes })
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
            "\"id\":\"p1.1\",\"page\":1,\"status\":\"extracted\",\"path\":\"page-0001/image-001.webp\",\"format\":\"webp\",\
             \"source\":\"render\",\"bytes\":7,\"sha256\":\"ab\",\"width\":3,\"height\":4",
        )
        .unwrap();
        w.image(
            "\"id\":\"p2.1\",\"page\":2,\"status\":\"not-extracted\",\"reason\":\"budget\",\"path\":\"page-0002/image-001.webp\",\
             \"format\":\"webp\",\"bytes\":0",
        )
        .unwrap();
        w.image("\"id\":\"p3.1\",\"page\":3,\"status\":\"not-extracted\",\"reason\":\"min_side\",\"path\":null").unwrap();
        w.batch("\"pages\":[1,64],\"status\":\"budget\",\"cut_page\":2,\"run\":0").unwrap();
        let out = w.finish("\"complete\":false,\"resumable\":true,\"totals\":{\"images\":3,\"batches\":1}").unwrap();
        String::from_utf8(out).unwrap()
    }

    /// [`manifest`]'s header and images with these batch lines.
    fn with_batches(batches: &[&str]) -> String {
        let base = manifest("doppler-images/3", "aa", "ss");
        let mut w = Writer::new(Vec::new());
        for l in base.lines() {
            let v: serde_json::Value = serde_json::from_str(l).unwrap();
            let fields = &l[l.find(",").unwrap() + 1..l.len() - 1];
            match v["type"].as_str().unwrap() {
                "header" => w.header(fields).unwrap(),
                "image" => w.image(fields).unwrap(),
                _ => {}
            }
        }
        for b in batches {
            w.batch(b).unwrap();
        }
        let end = format!(
            "\"complete\":false,\"resumable\":true,\"totals\":{{\"images\":3,\"batches\":{}}}",
            batches.len()
        );
        String::from_utf8(w.finish(&end).unwrap()).unwrap()
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
        let b: Vec<_> = p.batches.iter().map(|b| (b.pages, b.status, b.cut_page, b.run)).collect();
        assert_eq!(b, [((1, 64), "budget", Some(2), 0)]);
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
        let batch = "\"pages\":[1,64],\"status\":\"budget\",\"cut_page\":2,\"run\":0";
        let refused = |lines: &str| read(rehash(&ok.replace(batch, lines)).as_bytes(), "aa", "ss").err();
        for bad in [
            // malformed
            "\"pages\":[2,1],\"status\":\"budget\",\"cut_page\":2,\"run\":0",
            "\"pages\":[0,64],\"status\":\"budget\",\"cut_page\":2,\"run\":0",
            "\"pages\":[1],\"status\":\"budget\",\"cut_page\":2,\"run\":0",
            "\"pages\":[1,64],\"status\":\"forged\",\"cut_page\":2,\"run\":0",
            "\"pages\":[1,64],\"status\":\"budget\",\"cut_page\":\"2\",\"run\":0",
            "\"pages\":[1,64],\"status\":\"budget\",\"cut_page\":2",
            // inconsistent: ok with a cut_page, a cut_page outside the line, a run after the
            // header's resumes (2), pages with no image
            "\"pages\":[1,64],\"status\":\"ok\",\"cut_page\":2,\"run\":0",
            "\"pages\":[1,2],\"status\":\"budget\",\"cut_page\":3,\"run\":0",
            "\"pages\":[1,64],\"status\":\"budget\",\"cut_page\":2,\"run\":3",
            "\"pages\":[4000000000,4000000000],\"status\":\"budget\",\"cut_page\":null,\"run\":0",
        ] {
            let e = refused(bad).unwrap();
            assert!(e.contains("batch"), "{bad}: {e}");
        }
        // two lines of one run that overlap, or repeat, are refused; lines of two runs may
        let lines = |batches: &[&str]| read(with_batches(batches).as_bytes(), "aa", "ss");
        let line = |a: u32, b: u32, run: u64| format!("\"pages\":[{a},{b}],\"status\":\"ok\",\"cut_page\":null,\"run\":{run}");
        let e = lines(&[batch, &line(2, 3, 0)]).err().unwrap();
        assert!(e.contains("overlap"), "{e}");
        assert!(lines(&[batch, batch]).err().unwrap().contains("overlap"));
        assert_eq!(lines(&[batch, &line(2, 3, 1)]).unwrap().batches.len(), 2);
        // no more batch lines than pages with images (three here)
        let (l1, l2, l3, l4) = (line(1, 1, 0), line(2, 2, 0), line(3, 3, 0), line(1, 3, 1));
        assert_eq!(lines(&[&l1, &l2, &l3]).unwrap().batches.len(), 3);
        assert!(lines(&[&l1, &l2, &l3, &l4]).err().unwrap().contains("more batch lines"));
    }
}
