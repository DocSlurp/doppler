//! `pdftomarkdown --images-dir` in batches: one renderer per `--images-batch-pages` pages, each
//! with its own budgets; partial results kept (exit 5), hard failures (exit 6), the document
//! ceilings, stdout closed before extraction, and `--page-markers` through pdftotext. Needs the
//! DocSlurp pdftotext with `-images` and python3 for the stand-in renderer
//! (tests/fixtures/fake_pdfregions.py); skipped (passing) without them.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

mod manifest_file;
use manifest_file::manifest;

const PDFTOMARKDOWN: &str = env!("CARGO_BIN_EXE_pdftomarkdown");

fn pdftotext() -> Option<PathBuf> {
    let exe = match std::env::var_os("PDFTOTEXT").filter(|p| !p.is_empty()) {
        Some(p) => PathBuf::from(p),
        None => Path::new(env!("CARGO_MANIFEST_DIR")).join("../../build/utils/pdftotext"),
    };
    let usage = Command::new(&exe).arg("-h").output().ok()?;
    let usage = String::from_utf8_lossy(&usage.stderr);
    let python = Command::new("python3").arg("-c").arg("pass").status().is_ok_and(|s| s.success());
    (usage.contains("-compact-layout") && usage.contains("-images") && python).then_some(exe)
}

fn fake() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fake_pdfregions.py")
}

struct Scratch(PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// `n` pages of text, page k with its own picture (an image XObject of its own, a box of its
/// own size, so no picture repeats another).
fn make_pdf(n: usize) -> Vec<u8> {
    let font = 3 + 2 * n;
    let mut objects: Vec<Vec<u8>> = vec![
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        format!(
            "<< /Type /Pages /Kids [{}] /Count {n} >>",
            (0..n).map(|k| format!("{} 0 R", 3 + k)).collect::<Vec<_>>().join(" ")
        )
        .into_bytes(),
    ];
    for k in 0..n {
        objects.push(
            format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 {font} 0 R >> /XObject << /Im1 {} 0 R >> >> /Contents {} 0 R >>",
                font + 1 + k,
                3 + n + k
            )
            .into_bytes(),
        );
    }
    for k in 0..n {
        let mut c: String = (0..6)
            .map(|i| {
                format!("BT /F1 11 Tf 72 {} Td (Line {i} of page {} about its picture.) Tj ET\n", 720 - 14 * i, k + 1)
            })
            .collect();
        c += &format!("q {} 0 0 120 72 400 cm /Im1 Do Q\n", 150 + 10 * k);
        objects.push(format!("<< /Length {} >>\nstream\n{c}endstream", c.len()).into_bytes());
    }
    objects.push(b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_vec());
    for k in 0..n {
        let px = [(k * 40) as u8, 0, 255, 255, 255, 0, 0, 0, 255, 255, 255, 0];
        let mut im = format!(
            "<< /Type /XObject /Subtype /Image /Width 2 /Height 2 /ColorSpace /DeviceRGB /BitsPerComponent 8 /Length {} >>\nstream\n",
            px.len()
        )
        .into_bytes();
        im.extend(px);
        im.extend(b"\nendstream");
        objects.push(im);
    }
    let mut pdf = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::new();
    for (i, obj) in objects.iter().enumerate() {
        offsets.push(pdf.len());
        pdf.extend(format!("{} 0 obj\n", i + 1).as_bytes());
        pdf.extend(obj);
        pdf.extend(b"\nendobj\n");
    }
    let xref = pdf.len();
    let mut tail = format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1);
    for o in offsets {
        tail += &format!("{o:010} 00000 n \n");
    }
    tail += &format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objects.len() + 1);
    pdf.extend(tail.as_bytes());
    pdf
}

fn setup(tag: &str, pages: usize) -> Option<(PathBuf, Scratch, String)> {
    let Some(p) = pdftotext() else {
        eprintln!("skipped: needs the DocSlurp pdftotext with -images and python3");
        return None;
    };
    let d = std::env::temp_dir().join(format!("images-batches-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    let pdf = d.join("doc.pdf");
    std::fs::write(&pdf, make_pdf(pages)).unwrap();
    let pdf = pdf.to_str().unwrap().to_string();
    Some((p, Scratch(d), pdf))
}

fn run(pdftotext: &Path, args: &[&str], env: &[(&str, &str)]) -> Output {
    let mut c = Command::new(PDFTOMARKDOWN);
    c.args(args).env("PDFTOTEXT", pdftotext).env("PDFREGIONS", fake()).env("FAKE_PDFREGIONS_MODE", "ok");
    for (k, v) in env {
        c.env(k, v);
    }
    c.output().unwrap()
}

fn status(m: &serde_json::Value, page: u32) -> (String, Option<String>) {
    let e = m["images"].as_array().unwrap().iter().find(|e| e["page"] == page).unwrap();
    (e["status"].as_str().unwrap().to_string(), e["reason"].as_str().map(str::to_string))
}

fn entry(m: &serde_json::Value, page: u32) -> &serde_json::Value {
    m["images"].as_array().unwrap().iter().find(|e| e["page"] == page).unwrap()
}

fn batches(m: &serde_json::Value) -> Vec<(String, Option<u64>)> {
    m["batches"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| (b["status"].as_str().unwrap().to_string(), b["cut_page"].as_u64()))
        .collect()
}

fn ok(s: &str) -> (String, Option<u64>) {
    (s.to_string(), None)
}

#[test]
fn each_batch_has_its_own_renderer_and_budgets() {
    let Some((p, s, pdf)) = setup("budget", 6) else { return };
    let log = s.0.join("runs.log");
    let d = s.0.join("out");
    let r = run(
        &p,
        &["--report", "--images-dir", d.to_str().unwrap(), "--images-batch-pages", "2", "--images-max", "1", &pdf],
        &[("FAKE_PDFREGIONS_LOG", log.to_str().unwrap())],
    );
    let err = String::from_utf8_lossy(&r.stderr);
    assert_eq!(r.status.code(), Some(5), "{err}");
    // one renderer per batch, each with a fresh budget; a batch stopped by the figures' budget
    // alone is not a failure (a resume finishes it): three of them in a row do not abort
    assert_eq!(std::fs::read_to_string(&log).unwrap(), "1 2\n3 4\n5 6\n");
    let m = manifest(&d);
    assert_eq!((&m["complete"], &m["resumable"], &m["resumes"]), (&false.into(), &true.into(), &0.into()));
    let want = vec![("budget".to_string(), Some(2)), ("budget".to_string(), Some(4)), ("budget".to_string(), Some(6))];
    assert_eq!(batches(&m), want, "{m}");
    // each batch wrote its first picture; the budget cut the second, which a resume may extract
    for page in [1, 3, 5] {
        assert_eq!(status(&m, page).0, "extracted", "{page}");
        assert!(d.join(format!("page-{page:04}/image-001.webp")).is_file());
        assert!(entry(&m, page).get("resumable").is_none());
    }
    for page in [2, 4, 6] {
        assert_eq!(status(&m, page), ("not-extracted".into(), Some("budget".into())), "{page}");
        assert_eq!(entry(&m, page)["resumable"], true);
    }
    assert!(err.contains("\"images_complete\":false"), "{err}");
    // the batches of the default size are one here, and nothing is cut
    let d = s.0.join("whole");
    let r = run(&p, &["--images-dir", d.to_str().unwrap(), &pdf], &[]);
    assert_eq!(r.status.code(), Some(0));
    assert_eq!(batches(&manifest(&d)), [ok("ok")]);
    assert_eq!(manifest(&d)["totals"]["extracted"], 6);
}

#[test]
fn a_failed_batch_keeps_the_others_and_two_in_a_row_abort_the_rest() {
    let Some((p, s, pdf)) = setup("fail", 8) else { return };
    let plain = run(&p, &["--markers", &pdf], &[]);
    // the renderer of batch 2 (pages 3-4) stops at page 4 (a limit): page 3's file stays
    let d = s.0.join("one");
    let r = run(
        &p,
        &["--images-dir", d.to_str().unwrap(), "--images-batch-pages", "2", &pdf],
        &[("FAKE_PDFREGIONS_FAIL_PAGES", "4")],
    );
    assert_eq!(r.status.code(), Some(5), "{}", String::from_utf8_lossy(&r.stderr));
    assert_eq!(r.stdout, plain.stdout);
    let m = manifest(&d);
    assert_eq!(batches(&m), [ok("ok"), ("renderer".into(), Some(4)), ok("ok"), ok("ok")], "{m}");
    assert_eq!(status(&m, 3).0, "extracted");
    assert_eq!(status(&m, 4), ("not-extracted".into(), Some("renderer".into())));
    assert_eq!(m["totals"]["extracted"], 7);
    // killed by a signal (batch 1) and a limit (batch 2): two in a row, the rest is aborted
    let log = s.0.join("runs.log");
    let d = s.0.join("two");
    let r = run(
        &p,
        &["--images-dir", d.to_str().unwrap(), "--images-batch-pages", "2", &pdf],
        &[
            ("FAKE_PDFREGIONS_KILL_PAGES", "1"),
            ("FAKE_PDFREGIONS_FAIL_PAGES", "3"),
            ("FAKE_PDFREGIONS_LOG", log.to_str().unwrap()),
        ],
    );
    assert_eq!(r.status.code(), Some(5));
    let m = manifest(&d);
    assert_eq!(
        batches(&m),
        [
            ("renderer".into(), Some(1)),
            ("renderer".into(), Some(3)),
            ("aborted".into(), Some(5)),
            ("aborted".into(), Some(7))
        ],
        "{m}"
    );
    assert_eq!(status(&m, 8), ("not-extracted".into(), Some("aborted".into())));
    assert_eq!(std::fs::read_to_string(&log).unwrap().lines().count(), 2, "no renderer for aborted batches");
    assert_eq!(m["totals"]["extracted"], 0);
    // a manifest and nothing else: the folder is kept
    assert_eq!(std::fs::read_dir(&d).unwrap().count(), 1);
}

#[test]
fn the_document_ceilings_stop_partially() {
    let Some((p, s, pdf)) = setup("ceil", 6) else { return };
    // one byte: the first file is over the document's bytes, and every batch after
    let d = s.0.join("bytes");
    let r = run(
        &p,
        &["--images-dir", d.to_str().unwrap(), "--images-batch-pages", "2", "--images-doc-max-bytes", "1", &pdf],
        &[],
    );
    assert_eq!(r.status.code(), Some(5));
    let m = manifest(&d);
    assert_eq!(
        batches(&m),
        [("doc_budget".into(), Some(1)), ("doc_budget".into(), Some(3)), ("doc_budget".into(), Some(5))],
        "{m}"
    );
    assert_eq!(status(&m, 6), ("not-extracted".into(), Some("doc_budget".into())));
    // the deadline passes while the first batch hangs
    let d = s.0.join("deadline");
    let t = Instant::now();
    let r = run(
        &p,
        &["--images-dir", d.to_str().unwrap(), "--images-batch-pages", "2", "--images-deadline", "1", &pdf],
        &[("FAKE_PDFREGIONS_MODE", "hang")],
    );
    assert!(t.elapsed() < Duration::from_secs(20));
    assert_eq!(r.status.code(), Some(5));
    let m = manifest(&d);
    let b = batches(&m);
    assert_eq!(b[0], ("deadline".into(), Some(1)), "{m}");
    assert!(b[1..].iter().all(|x| x.0 == "deadline"), "{m}");
}

/// EOF on stdout comes when the Markdown is complete, before the images are extracted.
#[test]
fn stdout_closes_before_extraction() {
    let Some((p, s, pdf)) = setup("eof", 2) else { return };
    let d = s.0.join("out");
    let start = Instant::now();
    let mut child = Command::new(PDFTOMARKDOWN)
        .args(["--images-dir", d.to_str().unwrap(), &pdf])
        .env("PDFTOTEXT", &p)
        .env("PDFREGIONS", fake())
        .env("FAKE_PDFREGIONS_SLEEP", "2")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut md = String::new();
    child.stdout.take().unwrap().read_to_string(&mut md).unwrap();
    let eof = start.elapsed();
    let status = child.wait().unwrap();
    let exit = start.elapsed();
    assert_eq!(status.code(), Some(0));
    assert!(md.contains("](images/page-0002/image-001.webp)"), "{md}");
    assert!(exit >= eof + Duration::from_millis(1500), "EOF at {eof:?}, exit at {exit:?}");
    assert!(d.join("page-0002/image-001.webp").is_file());
}

#[test]
fn page_markers_name_the_pdf_pages() {
    let Some((p, _s, pdf)) = setup("pages", 4) else { return };
    let r = run(&p, &["--page-markers", "--report", "-f", "2", "-l", "3", &pdf], &[]);
    assert_eq!(r.status.code(), Some(0));
    let md = String::from_utf8(r.stdout).unwrap();
    let chunks: Vec<&str> = md.split('\u{c}').collect();
    assert_eq!(chunks.len(), 2);
    assert!(chunks[0].starts_with("<!-- doppler:page 2 -->\n\nLine 0 of page 2"), "{md}");
    assert!(chunks[1].starts_with("<!-- doppler:page 3 -->\n\nLine 0 of page 3"), "{md}");
    assert!(String::from_utf8_lossy(&r.stderr).contains("\"page_numbers\":[2,3]"));
    // with markers: the page's marker first, then its picture's link
    let r = run(&p, &["--page-markers", "--markers", "-f", "4", &pdf], &[]);
    let md = String::from_utf8(r.stdout).unwrap();
    assert!(md.starts_with("<!-- doppler:page 4 -->\n\n") && md.contains("](images/page-0004/image-001.webp)"), "{md}");
    // off: the Markdown is as before
    let r = run(&p, &["-f", "2", "-l", "3", &pdf], &[]);
    assert!(!String::from_utf8_lossy(&r.stdout).contains("doppler:page"));
}

#[test]
fn failed_batches_abort_two_in_a_row_or_three_in_all() {
    // one page a batch, the renderer failing on every other page: never two in a row, but the
    // third failure stops the rest (alternating failed and cheap batches must not run forever)
    let Some((p, s, pdf)) = setup("alternate", 8) else { return };
    let log = s.0.join("runs.log");
    let d = s.0.join("out");
    let r = run(
        &p,
        &["--images-dir", d.to_str().unwrap(), "--images-batch-pages", "1", &pdf],
        &[("FAKE_PDFREGIONS_FAIL_PAGES", "1,3,5,7"), ("FAKE_PDFREGIONS_LOG", log.to_str().unwrap())],
    );
    assert_eq!(r.status.code(), Some(5), "{}", String::from_utf8_lossy(&r.stderr));
    let m = manifest(&d);
    let got: Vec<String> = batches(&m).into_iter().map(|b| b.0).collect();
    assert_eq!(got, ["renderer", "ok", "renderer", "ok", "renderer", "aborted", "aborted", "aborted"], "{m}");
    assert_eq!(std::fs::read_to_string(&log).unwrap().lines().count(), 5, "no renderer after the third failure");
    for page in [6, 7, 8] {
        assert_eq!(status(&m, page), ("not-extracted".into(), Some("aborted".into())));
        assert_eq!(entry(&m, page)["resumable"], true);
    }
    // a renderer failure is the image's own: not resumable
    assert!(entry(&m, 1).get("resumable").is_none());
}

/// Every file under `dir` but the manifest: relative path -> (bytes, inode, mtime).
fn files(dir: &Path) -> std::collections::BTreeMap<String, (Vec<u8>, u64, std::time::SystemTime)> {
    use std::os::unix::fs::MetadataExt;
    let mut out = std::collections::BTreeMap::new();
    for sub in std::fs::read_dir(dir).unwrap() {
        let sub = sub.unwrap().path();
        if !sub.is_dir() {
            continue;
        }
        for f in std::fs::read_dir(&sub).unwrap() {
            let f = f.unwrap().path();
            let md = std::fs::metadata(&f).unwrap();
            let rel = f.strip_prefix(dir).unwrap().to_str().unwrap().to_string();
            out.insert(rel, (std::fs::read(&f).unwrap(), md.ino(), md.modified().unwrap()));
        }
    }
    out
}

#[test]
fn a_resume_finishes_what_a_budget_stopped() {
    let Some((p, s, pdf)) = setup("resume", 6) else { return };
    let full = s.0.join("full");
    let r = run(&p, &["--images-dir", full.to_str().unwrap(), &pdf], &[]);
    assert_eq!(r.status.code(), Some(0));
    let markdown = r.stdout;
    let part = s.0.join("part");
    let budget = ["--images-batch-pages", "2", "--images-max", "1"];
    let mut args = vec!["--images-dir", part.to_str().unwrap()];
    args.extend(budget);
    args.push(&pdf);
    let r = run(&p, &args, &[]);
    assert_eq!(r.status.code(), Some(5));
    assert_eq!(r.stdout, markdown);
    let before = files(&part);
    assert_eq!(before.len(), 3);
    // the resume: the same PDF and settings, the budgets left at their defaults
    let r = run(&p, &["--images-resume", part.to_str().unwrap(), "--images-batch-pages", "2", &pdf], &[]);
    assert_eq!(r.status.code(), Some(0), "{}", String::from_utf8_lossy(&r.stderr));
    assert_eq!(r.stdout, markdown, "the Markdown never changes");
    let after = files(&part);
    let whole = files(&full);
    assert_eq!(
        after.iter().map(|(k, v)| (k, &v.0)).collect::<Vec<_>>(),
        whole.iter().map(|(k, v)| (k, &v.0)).collect::<Vec<_>>(),
        "the files of an uninterrupted run, byte for byte"
    );
    for (k, v) in &before {
        assert_eq!((after[k].1, after[k].2), (v.1, v.2), "{k}: never rewritten");
    }
    let m = manifest(&part);
    let f = manifest(&full);
    assert_eq!((&m["complete"], &m["resumable"], &m["resumes"]), (&true.into(), &false.into(), &1.into()));
    // the round trip: the resumed manifest is the uninterrupted run's, line for line, but for
    // `resumes` and the resume's own batches (and so the end line's counts and hash)
    let lines = |d: &Path| -> Vec<String> {
        String::from_utf8(std::fs::read(d.join("manifest.jsonl")).unwrap()).unwrap().lines().map(String::from).collect()
    };
    let (rl, fl) = (lines(&part), lines(&full));
    assert_eq!(
        rl[0],
        fl[0].replace("\"resumes\":0", "\"resumes\":1").replace("\"batch_pages\":64", "\"batch_pages\":2")
    );
    let images = |l: &[String]| l.iter().filter(|l| l.starts_with("{\"type\":\"image\"")).cloned().collect::<Vec<_>>();
    assert_eq!(images(&rl), images(&fl));
    assert!(rl.last().unwrap().starts_with("{\"type\":\"end\",\"complete\":true,\"resumable\":false,"));
    assert_eq!(m["totals"]["extracted"], 6);
    for page in 1..=6 {
        assert_eq!(entry(&m, page)["sha256"], entry(&f, page)["sha256"], "{page}");
        assert_eq!(entry(&m, page)["path"], entry(&f, page)["path"], "{page}");
    }
    // nothing left to do: nothing changes, exit 0
    let manifest_bytes = std::fs::read(part.join("manifest.jsonl")).unwrap();
    let mtime = std::fs::metadata(part.join("manifest.jsonl")).unwrap().modified().unwrap();
    let r = run(&p, &["--images-resume", part.to_str().unwrap(), &pdf], &[]);
    assert_eq!(r.status.code(), Some(0), "{}", String::from_utf8_lossy(&r.stderr));
    assert_eq!(std::fs::read(part.join("manifest.jsonl")).unwrap(), manifest_bytes);
    assert_eq!(std::fs::metadata(part.join("manifest.jsonl")).unwrap().modified().unwrap(), mtime);
    // from an images plan, without converting again
    let plan = s.0.join("doc.plan");
    let r = run(&p, &["--markers", "--images-plan-out", plan.to_str().unwrap(), &pdf], &[]);
    assert_eq!(r.status.code(), Some(0));
    let planned = s.0.join("planned");
    let mut args = vec!["--images-plan", plan.to_str().unwrap(), "--images-dir", planned.to_str().unwrap()];
    args.extend(budget);
    args.push(&pdf);
    assert_eq!(run(&p, &args, &[]).status.code(), Some(5));
    let r =
        run(&p, &["--images-plan", plan.to_str().unwrap(), "--images-resume", planned.to_str().unwrap(), &pdf], &[]);
    assert_eq!(r.status.code(), Some(0), "{}", String::from_utf8_lossy(&r.stderr));
    assert_eq!(
        files(&planned).into_iter().map(|(k, v)| (k, v.0)).collect::<Vec<_>>(),
        whole.into_iter().map(|(k, v)| (k, v.0)).collect::<Vec<_>>()
    );
}

#[test]
fn a_resume_is_refused_unless_bound_to_the_same_pdf_and_settings() {
    let Some((p, s, pdf)) = setup("refuse", 4) else { return };
    let part = s.0.join("part");
    let r = run(&p, &["--images-dir", part.to_str().unwrap(), "--images-max", "1", &pdf], &[]);
    assert_eq!(r.status.code(), Some(5));
    let snapshot = || {
        let mut v: Vec<(String, Vec<u8>)> = files(&part).into_iter().map(|(k, v)| (k, v.0)).collect();
        v.push(("manifest.jsonl".into(), std::fs::read(part.join("manifest.jsonl")).unwrap_or_default()));
        v
    };
    let before = snapshot();
    let resume = |extra: &[&str], file: &str| {
        let mut args = vec!["--images-resume", part.to_str().unwrap()];
        args.extend(extra);
        args.push(file);
        run(&p, &args, &[])
    };
    // another PDF (one page more)
    let other = s.0.join("other.pdf");
    std::fs::write(&other, make_pdf(5)).unwrap();
    let r = resume(&[], other.to_str().unwrap());
    assert_eq!(r.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&r.stderr).contains("another PDF"), "{}", String::from_utf8_lossy(&r.stderr));
    // other settings: a format, a resolution, a page range, the scan test
    for extra in [
        &["--raster-format", "png"][..],
        &["--images-dpi", "300"],
        &["-l", "3"],
        &["--scan-max-words", "5"],
        &["--images-repeats", "keep"],
    ] {
        let r = resume(extra, &pdf);
        assert_eq!(r.status.code(), Some(2), "{extra:?}");
        assert!(String::from_utf8_lossy(&r.stderr).contains("other settings"), "{extra:?}");
    }
    assert_eq!(snapshot(), before, "a refused resume changes nothing");
    // a damaged or foreign manifest: refused (exit 2) and left as it is
    let path = part.join("manifest.jsonl");
    let text = String::from_utf8(std::fs::read(&path).unwrap()).unwrap();
    let lines: Vec<&str> = text.lines().collect();
    let joined = |l: &[&str]| l.iter().map(|l| format!("{l}\n")).collect::<String>();
    let refused = |bytes: &str, why: &str| {
        std::fs::write(&path, bytes).unwrap();
        let r = resume(&[], &pdf);
        let err = String::from_utf8_lossy(&r.stderr).to_string();
        assert_eq!(r.status.code(), Some(2), "{why}: {err}");
        assert!(err.contains(why), "{why}: {err}");
        assert_eq!(std::fs::read(&path).unwrap(), bytes.as_bytes(), "{why}: left as it was");
    };
    // truncated: no end line; the header alone; a line cut short
    refused(&joined(&lines[..lines.len() - 1]), "no end line");
    refused(&joined(&lines[..1]), "no end line");
    refused(&text[..text.len() - 10], "no line break");
    // an image line changed (a status that would let a resume skip it): the hash fails
    let tampered = text.replacen("\"status\":\"not-extracted\"", "\"status\":\"extracted\"", 1);
    assert_ne!(tampered, text);
    refused(&tampered, "sha256");
    // another schema, and a version 2 manifest.json in place of manifest.jsonl
    refused(&text.replacen("doppler-images/3", "doppler-images/4", 1), "schema");
    std::fs::remove_file(&path).unwrap();
    let v2 = "{\"version\":2,\"source_sha256\":\"00\",\"resumes\":0,\n\"batches\":[\n],\n\"images\":[\n]}\n";
    std::fs::write(part.join("manifest.json"), v2).unwrap();
    let r = resume(&[], &pdf);
    assert_eq!(r.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&r.stderr).contains("version 2"), "{}", String::from_utf8_lossy(&r.stderr));
    std::fs::remove_file(part.join("manifest.json")).unwrap();
    // none at all
    let r = resume(&[], &pdf);
    assert_eq!(r.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&r.stderr).contains("no manifest.jsonl"));
    // a folder that does not exist, and --images-dir with --images-resume
    assert_eq!(run(&p, &["--images-resume", s.0.join("none").to_str().unwrap(), &pdf], &[]).status.code(), Some(2));
    let r = run(
        &p,
        &["--images-resume", part.to_str().unwrap(), "--images-dir", s.0.join("x").to_str().unwrap(), &pdf],
        &[],
    );
    assert_eq!(r.status.code(), Some(2));
    // the budget flags may change: a resume with a bigger budget is not refused
    std::fs::write(&path, &text).unwrap();
    let r = resume(&["--images-max", "100", "--images-timeout", "30"], &pdf);
    assert_eq!(r.status.code(), Some(0), "{}", String::from_utf8_lossy(&r.stderr));
}

#[test]
fn a_failed_resume_keeps_the_old_manifest_and_removes_only_its_own_files() {
    let Some((p, s, pdf)) = setup("resume-fail", 4) else { return };
    let part = s.0.join("part");
    let r = run(&p, &["--images-dir", part.to_str().unwrap(), "--images-max", "1", &pdf], &[]);
    assert_eq!(r.status.code(), Some(5));
    let manifest_bytes = std::fs::read(part.join("manifest.jsonl")).unwrap();
    // something put a file where page 4's picture goes: the resume writes pages 2 and 3, then
    // must not replace it (a hard failure, exit 6)
    std::fs::create_dir(part.join("page-0004")).unwrap();
    std::fs::write(part.join("page-0004/image-001.webp"), b"theirs").unwrap();
    let before = files(&part);
    let r = run(&p, &["--images-resume", part.to_str().unwrap(), &pdf], &[]);
    assert_eq!(r.status.code(), Some(6), "{}", String::from_utf8_lossy(&r.stderr));
    assert_eq!(std::fs::read(part.join("manifest.jsonl")).unwrap(), manifest_bytes, "the old manifest stays");
    let after = files(&part);
    assert_eq!(after.keys().collect::<Vec<_>>(), before.keys().collect::<Vec<_>>(), "only the new files went");
    assert_eq!(std::fs::read(part.join("page-0004/image-001.webp")).unwrap(), b"theirs");
    assert!(!part.join("page-0002").exists(), "a page folder the resume made is removed");
}

/// Keeps only `manifest.jsonl` in `dir`: what a host that keeps the extracted files elsewhere
/// (object storage) hands back for a resume.
fn manifest_only(dir: &Path) {
    for e in std::fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        if p.is_dir() {
            std::fs::remove_dir_all(&p).unwrap();
        } else if p.file_name().unwrap() != "manifest.jsonl" {
            std::fs::remove_file(&p).unwrap();
        }
    }
}

#[test]
fn a_resume_needs_only_the_manifest() {
    let Some((p, s, pdf)) = setup("resume-manifest-only", 6) else { return };
    let full = s.0.join("full");
    assert_eq!(run(&p, &["--images-dir", full.to_str().unwrap(), &pdf], &[]).status.code(), Some(0));
    let part = s.0.join("part");
    let r =
        run(&p, &["--images-dir", part.to_str().unwrap(), "--images-batch-pages", "2", "--images-max", "1", &pdf], &[]);
    assert_eq!(r.status.code(), Some(5));
    let kept = files(&part);
    assert_eq!(kept.len(), 3);
    manifest_only(&part);
    let r = run(&p, &["--images-resume", part.to_str().unwrap(), "--images-batch-pages", "2", &pdf], &[]);
    assert_eq!(r.status.code(), Some(0), "{}", String::from_utf8_lossy(&r.stderr));
    // only the new files: the three the first run wrote are not written again
    let new = files(&part);
    assert_eq!(new.len(), 3);
    assert!(new.keys().all(|k| !kept.contains_key(k)), "{:?} vs {:?}", new.keys(), kept.keys());
    let whole = files(&full);
    for (k, v) in &new {
        assert_eq!(v.0, whole[k].0, "{k}");
    }
    let m = manifest(&part);
    assert_eq!((&m["complete"], &m["resumes"]), (&true.into(), &1.into()));
    assert_eq!(m["totals"]["extracted"], 6);
}

#[test]
fn a_manifest_only_resume_counts_the_manifests_bytes_against_the_document_ceiling() {
    let Some((p, s, pdf)) = setup("resume-manifest-bytes", 6) else { return };
    let part = s.0.join("part");
    let r =
        run(&p, &["--images-dir", part.to_str().unwrap(), "--images-batch-pages", "2", "--images-max", "1", &pdf], &[]);
    assert_eq!(r.status.code(), Some(5));
    let m = manifest(&part);
    let held: u64 = m["images"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["status"] == "extracted")
        .map(|e| e["bytes"].as_u64().unwrap())
        .sum();
    assert!(held > 0);
    manifest_only(&part);
    // the ceiling is exactly what the manifest says is held: nothing more fits
    let ceiling = held.to_string();
    let r = run(
        &p,
        &[
            "--images-resume",
            part.to_str().unwrap(),
            "--images-batch-pages",
            "2",
            "--images-doc-max-bytes",
            &ceiling,
            &pdf,
        ],
        &[],
    );
    assert_eq!(r.status.code(), Some(5), "{}", String::from_utf8_lossy(&r.stderr));
    assert!(files(&part).is_empty(), "nothing fits under the ceiling the held bytes fill");
    let m = manifest(&part);
    assert_eq!(m["totals"]["extracted"], 3);
}

#[test]
fn a_resume_page_range_retries_only_its_pages() {
    let Some((p, s, pdf)) = setup("resume-pages", 6) else { return };
    let full = s.0.join("full");
    assert_eq!(run(&p, &["--images-dir", full.to_str().unwrap(), &pdf], &[]).status.code(), Some(0));
    let part = s.0.join("part");
    let r =
        run(&p, &["--images-dir", part.to_str().unwrap(), "--images-batch-pages", "2", "--images-max", "1", &pdf], &[]);
    assert_eq!(r.status.code(), Some(5));
    let markdown = r.stdout;
    let m = manifest(&part);
    let resumable: Vec<u32> = (1..=6).filter(|&pg| entry(&m, pg)["resumable"] == true).collect();
    assert_eq!(resumable, [2, 4, 6], "{m}");
    manifest_only(&part);
    // --images-pages is bound into the manifest's settings: a different range is refused
    let r = run(&p, &["--images-resume", part.to_str().unwrap(), "--images-pages", "3-4", &pdf], &[]);
    assert_eq!(r.status.code(), Some(2));
    // --images-resume-pages narrows the resume only: page 4 is extracted, 2 and 6 stay resumable
    let r = run(&p, &["--images-resume", part.to_str().unwrap(), "--images-resume-pages", "3-4", &pdf], &[]);
    assert_eq!(r.status.code(), Some(5), "{}", String::from_utf8_lossy(&r.stderr));
    assert_eq!(r.stdout, markdown, "the Markdown never changes");
    let got = files(&part);
    let whole = files(&full);
    assert_eq!(got.len(), 1, "{:?}", got.keys());
    let (k, v) = got.iter().next().unwrap();
    assert!(k.starts_with("page-0004/"), "{k}");
    assert_eq!(v.0, whole[k].0);
    let m = manifest(&part);
    assert_eq!((&m["complete"], &m["resumable"], &m["resumes"]), (&false.into(), &true.into(), &1.into()));
    assert_eq!(status(&m, 4).0, "extracted");
    for pg in [2, 6] {
        assert_eq!(entry(&m, pg)["resumable"], true, "{pg}");
    }
    // a range with nothing resumable changes nothing (still 5: pages 2 and 6 remain)
    let before = std::fs::read(part.join("manifest.jsonl")).unwrap();
    let r = run(&p, &["--images-resume", part.to_str().unwrap(), "--images-resume-pages", "3-5", &pdf], &[]);
    assert_eq!(r.status.code(), Some(5));
    assert_eq!(std::fs::read(part.join("manifest.jsonl")).unwrap(), before);
    // the rest, from the manifest alone again
    manifest_only(&part);
    let r = run(&p, &["--images-resume", part.to_str().unwrap(), &pdf], &[]);
    assert_eq!(r.status.code(), Some(0), "{}", String::from_utf8_lossy(&r.stderr));
    let m = manifest(&part);
    assert_eq!((&m["complete"], &m["resumes"]), (&true.into(), &2.into()));
    assert_eq!(m["totals"]["extracted"], 6);
    // --images-resume-pages without --images-resume is a usage error
    let r = run(&p, &["--images-dir", s.0.join("x").to_str().unwrap(), "--images-resume-pages", "1", &pdf], &[]);
    assert_eq!(r.status.code(), Some(2));
}
