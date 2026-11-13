//! `pdftomarkdown --images-dir` in batches: one renderer per `--images-batch-pages` pages, each
//! with its own budgets; partial results kept (exit 5), hard failures (exit 6), the document
//! ceilings, stdout closed before extraction, and `--page-markers` through pdftotext. Needs the
//! DocSlurp pdftotext with `-images` and python3 for the stand-in renderer
//! (tests/fixtures/fake_pdfregions.py); skipped (passing) without them.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

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

fn manifest(dir: &Path) -> serde_json::Value {
    serde_json::from_str(&std::fs::read_to_string(dir.join("manifest.json")).unwrap()).unwrap()
}

fn status(m: &serde_json::Value, page: u32) -> (String, Option<String>) {
    let e = m["images"].as_array().unwrap().iter().find(|e| e["page"] == page).unwrap();
    (e["status"].as_str().unwrap().to_string(), e["reason"].as_str().map(str::to_string))
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
    // one renderer per batch, each with a fresh budget; two batches stopped by their budget in
    // a row abort the rest without a third renderer (we do not pay for bad inputs)
    assert_eq!(std::fs::read_to_string(&log).unwrap(), "1 2\n3 4\n");
    let m = manifest(&d);
    assert_eq!(m["complete"], false);
    let want = vec![("budget".to_string(), Some(2)), ("budget".to_string(), Some(4)), ("aborted".to_string(), Some(5))];
    assert_eq!(batches(&m), want, "{m}");
    // each batch that ran wrote its first picture; the budget cut the second
    for page in [1, 3] {
        assert_eq!(status(&m, page).0, "extracted", "{page}");
        assert!(d.join(format!("page-{page:04}/image-001.webp")).is_file());
    }
    for page in [2, 4] {
        assert_eq!(status(&m, page), ("not-extracted".into(), Some("budget".into())), "{page}");
    }
    for page in [5, 6] {
        assert_eq!(status(&m, page), ("not-extracted".into(), Some("aborted".into())), "{page}");
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
