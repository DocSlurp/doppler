//! `pdftomarkdown` end to end: needs the DocSlurp pdftotext (with `-compact-layout`), from
//! `$PDFTOTEXT` or the build tree (`../../build/utils/pdftotext`); skipped (passing) without one.

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

const PDFTOMARKDOWN: &str = env!("CARGO_BIN_EXE_pdftomarkdown");
const DOPPLER_MD: &str = env!("CARGO_BIN_EXE_doppler-md");

/// The DocSlurp pdftotext, if one is available: its usage lists `-rules` and `-compact-layout`.
fn pdftotext() -> Option<PathBuf> {
    let exe = match std::env::var_os("PDFTOTEXT").filter(|p| !p.is_empty()) {
        Some(p) => PathBuf::from(p),
        None => Path::new(env!("CARGO_MANIFEST_DIR")).join("../../build/utils/pdftotext"),
    };
    let usage = Command::new(&exe).arg("-h").output().ok()?;
    let usage = String::from_utf8_lossy(&usage.stderr);
    (usage.contains("-rules") && usage.contains("-compact-layout")).then_some(exe)
}

/// A scratch directory removed on drop, so a failing assertion does not leave it behind.
struct Scratch(PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A one-page Helvetica PDF: a heading, a paragraph and a three-column table-like block.
fn tiny_pdf() -> Vec<u8> {
    let mut content = String::from("BT /F1 18 Tf 72 720 Td (Parts list) Tj ET\n");
    content += "BT /F1 11 Tf 72 690 Td (These are the parts we keep in stock today.) Tj ET\n";
    let rows = [["Part", "Count", "Price"], ["Bolt", "12", "0.40"], ["Nut", "30", "0.10"], ["Gear", "4", "7.25"]];
    for (i, row) in rows.iter().enumerate() {
        let y = 650 - 16 * i;
        for (x, cell) in [72, 220, 360].iter().zip(row) {
            content += &format!("BT /F1 11 Tf {x} {y} Td ({cell}) Tj ET\n");
        }
    }
    pdf_from_content(&content)
}

fn pdf_from_content(content: &str) -> Vec<u8> {
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> >> \
         /Contents 5 0 R >>"
            .to_string(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
        format!("<< /Length {} >>\nstream\n{content}endstream", content.len()),
    ];
    let mut pdf = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::new();
    for (i, obj) in objects.iter().enumerate() {
        offsets.push(pdf.len());
        pdf.extend_from_slice(format!("{} 0 obj\n{obj}\nendobj\n", i + 1).as_bytes());
    }
    let xref = pdf.len();
    let mut tail = format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1);
    for o in offsets {
        tail += &format!("{o:010} 00000 n \n");
    }
    tail += &format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objects.len() + 1);
    pdf.extend_from_slice(tail.as_bytes());
    pdf
}

fn run(pdftotext: &Path, args: &[&str]) -> Output {
    Command::new(PDFTOMARKDOWN).args(args).env("PDFTOTEXT", pdftotext).output().unwrap()
}

#[test]
fn pdftomarkdown_matches_the_pipe() {
    let Some(pdftotext) = pdftotext() else {
        eprintln!("skipped: set PDFTOTEXT to the DocSlurp pdftotext (or build it in ../../build)");
        return;
    };
    let scratch = Scratch(std::env::temp_dir().join(format!("pdftomarkdown-test-{}", std::process::id())));
    let dir = &scratch.0;
    std::fs::create_dir_all(dir).unwrap();
    let pdf = dir.join("tiny.pdf");
    std::fs::write(&pdf, tiny_pdf()).unwrap();
    let pdf_s = pdf.to_str().unwrap();

    // (a) --strict: the same bytes as `pdftotext -bbox-layout -wordmeta -rules FILE - | doppler-md`
    let xhtml =
        Command::new(&pdftotext).args(["-q", "-bbox-layout", "-wordmeta", "-rules", pdf_s, "-"]).output().unwrap();
    assert!(xhtml.status.success());
    let mut md = Command::new(DOPPLER_MD).stdin(Stdio::piped()).stdout(Stdio::piped()).spawn().unwrap();
    std::io::Write::write_all(md.stdin.as_mut().unwrap(), &xhtml.stdout).unwrap();
    let piped = md.wait_with_output().unwrap();
    assert!(piped.status.success());
    let one = run(&pdftotext, &["--strict", pdf_s]);
    assert_eq!(one.status.code(), Some(0), "{}", String::from_utf8_lossy(&one.stderr));
    assert_eq!(String::from_utf8_lossy(&one.stdout), String::from_utf8_lossy(&piped.stdout));
    // the default (compact coordinates) converts the same simple page the same way
    let compact = run(&pdftotext, &[pdf_s]);
    assert_eq!(compact.status.code(), Some(0), "{}", String::from_utf8_lossy(&compact.stderr));
    let gfm = String::from_utf8_lossy(&compact.stdout);
    assert!(gfm.contains("# Parts list") && gfm.contains("| Bolt | 12 | 0.40 |"), "{gfm}");

    // (b) --html (and --report, page range) convert
    let html = run(&pdftotext, &["--html", "--report", "-f", "1", "-l", "1", pdf_s]);
    assert_eq!(html.status.code(), Some(0), "{}", String::from_utf8_lossy(&html.stderr));
    assert!(String::from_utf8_lossy(&html.stderr).contains("\"page_found\":true"));
    let html = String::from_utf8_lossy(&html.stdout);
    assert!(html.contains("# Parts list") && html.contains("<tr><td>Bolt</td><td>12</td><td>0.40</td></tr>"), "{html}");

    // (c) a missing file: pdftotext fails
    let missing = run(&pdftotext, &[dir.join("missing.pdf").to_str().unwrap()]);
    assert_eq!(missing.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&missing.stderr).contains("could not open"));

    // (d) a "pdftotext" that writes no <page>: not the fork
    if Path::new("/bin/true").is_file() {
        let bogus = run(Path::new("/bin/true"), &[pdf_s]);
        assert_eq!(bogus.status.code(), Some(2));
        assert!(String::from_utf8_lossy(&bogus.stderr).contains("/bin/true"));
    }

    // (e) bad arguments, including page ranges pdftotext would misread or reject
    for args in [
        &[][..],
        &["-f"],
        &["-f", "x", pdf_s],
        &["-f", "0", pdf_s],
        &["-f", "4294967295", pdf_s],
        &["-f", "2", "-l", "1", pdf_s],
        &["--bogus", pdf_s],
        &[pdf_s, pdf_s],
    ] {
        assert_eq!(run(&pdftotext, args).status.code(), Some(2), "{args:?}");
    }

    // (f) `--` ends the options; poppler's -upw spelling is accepted
    std::fs::write(dir.join("-dash.pdf"), tiny_pdf()).unwrap();
    let dashed = Command::new(PDFTOMARKDOWN)
        .args(["-upw", "", "--", "-dash.pdf"])
        .current_dir(dir)
        .env("PDFTOTEXT", &pdftotext)
        .output()
        .unwrap();
    assert_eq!(dashed.status.code(), Some(0), "{}", String::from_utf8_lossy(&dashed.stderr));
    assert_eq!(dashed.stdout, compact.stdout);
}

#[test]
fn pdftotext_not_found() {
    let out = Command::new(PDFTOMARKDOWN).arg("x.pdf").env("PDFTOTEXT", "/nonexistent/pdftotext").output().unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("/nonexistent/pdftotext"));
}

#[test]
fn real_pdf_combined_span() {
    let Some(pt) = pdftotext() else {
        return;
    };
    // Source: 3 columns, 3 rows, upper-left 2x2 merged. Six labeled cells.
    let mut content = String::from("0.5 w\n");
    for (x0, y0, x1, y1) in [
        (20, 700, 320, 700),
        (20, 640, 320, 640),
        (20, 700, 20, 640),
        (320, 700, 320, 640),
        (220, 700, 220, 640),
        (120, 660, 120, 640),
        (20, 660, 320, 660),
        (220, 680, 320, 680),
    ] {
        content += &format!("{x0} {y0} m {x1} {y1} l S\n");
    }
    for (x, y, t) in [
        (30, 686, "Merged"),
        (230, 686, "RightOne"),
        (230, 666, "RightTwo"),
        (30, 646, "BottomOne"),
        (130, 646, "BottomTwo"),
        (230, 646, "BottomThree"),
    ] {
        content += &format!("BT /F1 10 Tf {x} {y} Td ({t}) Tj ET\n");
    }
    let scratch = Scratch(std::env::temp_dir().join(format!("span-pdf-{}", std::process::id())));
    std::fs::create_dir_all(&scratch.0).unwrap();
    let pdf = scratch.0.join("spans.pdf");
    std::fs::write(&pdf, pdf_from_content(&content)).unwrap();
    let out = run(&pt, &["--html", pdf.to_str().unwrap()]);
    assert!(out.status.success());
    let html = String::from_utf8(out.stdout).unwrap();
    assert!(html.contains("rowspan=\"2\" colspan=\"2\""), "{html}");
    assert_eq!(html.matches("<td").count(), 6);
    for t in ["Merged", "RightOne", "RightTwo", "BottomOne", "BottomTwo", "BottomThree"] {
        assert_eq!(html.matches(t).count(), 1, "{html}");
    }
}

#[test]
fn stream_and_window_match_the_default_on_a_short_document() {
    let Some(pdftotext) = pdftotext() else {
        eprintln!("skipped: set PDFTOTEXT to the DocSlurp pdftotext (or build it in ../../build)");
        return;
    };
    let scratch = Scratch(std::env::temp_dir().join(format!("pdftomarkdown-stream-{}", std::process::id())));
    std::fs::create_dir_all(&scratch.0).unwrap();
    let pdf = scratch.0.join("tiny.pdf");
    std::fs::write(&pdf, tiny_pdf()).unwrap();
    let pdf = pdf.to_str().unwrap();
    let want = run(&pdftotext, &[pdf]);
    assert!(want.status.success());
    for args in [&["--stream"][..], &["--window", "0"], &["--window", "3"]] {
        let got = run(&pdftotext, &[args, &[pdf]].concat());
        assert!(got.status.success(), "{args:?}");
        assert_eq!(got.stdout, want.stdout, "{args:?}");
    }
    let bad = run(&pdftotext, &["--window", "x", pdf]);
    assert_eq!(bad.status.code(), Some(2));
}
