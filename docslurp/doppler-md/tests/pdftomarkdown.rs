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
    for args in [&["--stream"][..], &["--window", "0"]] {
        let got = run(&pdftotext, &[args, &[pdf]].concat());
        assert!(got.status.success(), "{args:?}");
        assert_eq!(got.stdout, want.stdout, "{args:?}");
    }
    let bad = run(&pdftotext, &["--window", "x", pdf]);
    assert_eq!(bad.status.code(), Some(2));
}

/// A stand-in pdftotext that writes three pages of doppler XHTML, then exits 1.
#[cfg(unix)]
fn failing_pdftotext(dir: &Path) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let page = |t: &str| {
        format!(
            "<page width=\"612\" height=\"792\"><flow><block><line><word xMin=\"72\" yMin=\"290\" \
             xMax=\"200\" yMax=\"300\" fontName=\"F\" fontSize=\"10\" baseline=\"298\">{t}</word></line>\
             </block></flow></page>\n"
        )
    };
    let xhtml = format!("<doc>\n{}{}{}", page("first"), page("second"), page("third"));
    let exe = dir.join("pdftotext");
    std::fs::write(&exe, format!("#!/bin/sh\ncat <<'EOF'\n{xhtml}EOF\nexit 1\n")).unwrap();
    std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755)).unwrap();
    exe
}

#[cfg(unix)]
#[test]
fn a_failing_pdftotext_fails_the_run_and_only_a_window_writes_early_pages() {
    let scratch = Scratch(std::env::temp_dir().join(format!("pdftomarkdown-fail-{}", std::process::id())));
    std::fs::create_dir_all(&scratch.0).unwrap();
    let fake = failing_pdftotext(&scratch.0);
    let whole = run(&fake, &["x.pdf"]);
    assert_eq!(whole.status.code(), Some(1));
    assert!(whole.stdout.is_empty(), "{}", String::from_utf8_lossy(&whole.stdout));
    // --window 0 writes a page once the next one opens: "third" waits for the end, which fails
    let streamed = run(&fake, &["--window", "0", "--body-pages", "1", "x.pdf"]);
    assert_eq!(streamed.status.code(), Some(1));
    assert_eq!(String::from_utf8_lossy(&streamed.stdout), "first\x0csecond");
}

/// An `n`-page Helvetica PDF: three cover pages in large print, then chapters of six pages,
/// each with its own running header, a document-wide footer, a page number and body lines.
fn book_pdf(n: usize) -> Vec<u8> {
    let mut objects = vec![
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        format!(
            "<< /Type /Pages /Kids [{}] /Count {n} >>",
            (0..n).map(|i| format!("{} 0 R", 4 + 2 * i)).collect::<Vec<_>>().join(" ")
        ),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
    ];
    for i in 0..n {
        let mut c = String::new();
        if i < 3 {
            c += &format!("BT /F1 26 Tf 72 500 Td (Cover page {i} of a generated volume) Tj ET\n");
        } else {
            let ch = (i - 3) / 6;
            c += &format!("BT /F1 10 Tf 72 760 Td (Chapter {ch} running head) Tj ET\n");
            if (i - 3) % 6 == 0 {
                c += &format!("BT /F1 16 Tf 72 700 Td (Heading of chapter {ch}) Tj ET\n");
            }
            for k in 0..6 {
                c += &format!(
                    "BT /F1 10 Tf 72 {} Td (Body line {k} of page {i}, with words enough to read as prose.) Tj ET\n",
                    660 - 14 * k
                );
            }
            c += "BT /F1 8 Tf 72 40 Td (Generated Press Confidential) Tj ET\n";
            c += &format!("BT /F1 8 Tf 300 28 Td ({}) Tj ET\n", i + 1);
        }
        objects.push(format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 3 0 R >> >> /Contents {} 0 R >>",
            5 + 2 * i
        ));
        objects.push(format!("<< /Length {} >>\nstream\n{c}endstream", c.len()));
    }
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

#[test]
fn ranges_with_context_and_the_estimated_body_match_a_whole_run() {
    let Some(pdftotext) = pdftotext() else {
        eprintln!("skipped: set PDFTOTEXT to the DocSlurp pdftotext (or build it in ../../build)");
        return;
    };
    let scratch = Scratch(std::env::temp_dir().join(format!("pdftomarkdown-context-{}", std::process::id())));
    std::fs::create_dir_all(&scratch.0).unwrap();
    let pdf = scratch.0.join("book.pdf");
    // six whole chapters: every running header repeats
    let n = 39;
    std::fs::write(&pdf, book_pdf(n)).unwrap();
    let pdf = pdf.to_str().unwrap();
    for window in [&["--window", "4", "--body-pages", "32"][..], &["--window", "2", "--body-pages", "8"]] {
        let whole = run(&pdftotext, &[window, &[pdf]].concat());
        assert!(whole.status.success());
        let whole = String::from_utf8(whole.stdout).unwrap();
        let pages: Vec<&str> = whole.split('\x0c').collect();
        assert_eq!(pages.len(), n);
        // running headers and the footer are gone, the covers did not set the body size
        assert!(!whole.contains("running head") && !whole.contains("Confidential"), "{window:?}");
        assert!(whole.contains("# Heading of chapter 1"), "{window:?}");
        let est = run(&pdftotext, &[window, &["--estimate-body", pdf]].concat());
        assert!(est.status.success());
        let est: serde_json::Value = serde_json::from_slice(&est.stdout).unwrap();
        let (size, color) = (est["body_size"].to_string(), est["body_color"].as_str().unwrap().to_string());
        assert_eq!(size, "10");
        let mut shards = Vec::new();
        for (f, l) in [(1, 7), (8, 21), (22, 22), (23, 39)] {
            let (fs, ls) = (f.to_string(), l.to_string());
            let args =
                ["--report", "--body-size", &size, "--body-color", &color, "-f", &fs, "-l", &ls, "--context", "16"];
            let r = run(&pdftotext, &[window, &args, &[pdf]].concat());
            assert!(r.status.success(), "{}", String::from_utf8_lossy(&r.stderr));
            let report: serde_json::Value =
                serde_json::from_str(String::from_utf8_lossy(&r.stderr).lines().next().unwrap()).unwrap();
            let range = &report["range"];
            assert_eq!(range["first_page"], f);
            assert_eq!(range["context_before"], (f - 1).min(16));
            assert_eq!(range["context_after"], (n - l).min(16));
            assert_eq!(report["pages"], l - f + 1);
            let why: Vec<&str> = range["identity"].as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
            assert_eq!(why, if f > 1 { vec!["doc_work"] } else { vec![] }, "{f}-{l}");
            shards.push(String::from_utf8(r.stdout).unwrap());
        }
        assert_eq!(shards.join("\x0c"), whole, "{window:?}");
    }
    // without the body and the context, a range says what may differ
    let r = run(&pdftotext, &["--window", "4", "--report", "-f", "20", "-l", "25", "--context", "0", pdf]);
    let err = String::from_utf8_lossy(&r.stderr);
    assert!(err.contains("\"identity\":[\"body\",\"context\",\"doc_work\"]"), "{err}");
    // the context defaults to the window
    let r = run(&pdftotext, &["--window", "4", "--report", "-f", "20", "-l", "25", pdf]);
    let err = String::from_utf8_lossy(&r.stderr);
    assert!(err.contains("\"identity\":[\"body\",\"doc_work\"]"), "{err}");
    // the default --stream window, estimate and default context: the same bytes as a whole run
    let whole = run(&pdftotext, &["--stream", pdf]);
    let est = run(&pdftotext, &["--stream", "--estimate-body", pdf]);
    let est: serde_json::Value = serde_json::from_slice(&est.stdout).unwrap();
    let (size, color) = (est["body_size"].to_string(), est["body_color"].as_str().unwrap().to_string());
    let shards: Vec<String> = [(1, 10), (11, 30), (31, 39)]
        .iter()
        .map(|(f, l)| {
            let (fs, ls) = (f.to_string(), l.to_string());
            let r =
                run(&pdftotext, &["--stream", "--body-size", &size, "--body-color", &color, "-f", &fs, "-l", &ls, pdf]);
            assert!(r.status.success());
            String::from_utf8(r.stdout).unwrap()
        })
        .collect();
    assert_eq!(shards.join("\x0c"), String::from_utf8(whole.stdout).unwrap());
}

/// A stand-in pdftotext that writes `n` pages of doppler XHTML in `size` point print (whatever
/// the page range asked for) and records its arguments in `args.txt` beside it.
#[cfg(unix)]
fn fake_pdftotext(dir: &Path, n: usize, size: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let mut xhtml = String::from("<doc>\n");
    for i in 0..n {
        xhtml += &format!(
            "<page width=\"612\" height=\"792\"><flow><block><line><word xMin=\"72\" yMin=\"290\" \
             xMax=\"200\" yMax=\"300\" fontName=\"F\" fontSize=\"{size}\" color=\"#000000\" \
             baseline=\"298\">Page{i}</word></line></block></flow></page>\n"
        );
    }
    xhtml += "</doc>\n";
    let exe = dir.join("pdftotext");
    let args = dir.join("args.txt");
    std::fs::write(&exe, format!("#!/bin/sh\necho \"$@\" > '{}'\ncat <<'EOF'\n{xhtml}EOF\n", args.display())).unwrap();
    std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755)).unwrap();
    exe
}

#[cfg(unix)]
#[test]
fn estimate_body_succeeds_when_the_input_ends_before_the_sample() {
    let scratch = Scratch(std::env::temp_dir().join(format!("pdftomarkdown-estimate-{}", std::process::id())));
    std::fs::create_dir_all(&scratch.0).unwrap();
    // 10 pages: fewer than the 32 with text of the sample
    let fake = fake_pdftotext(&scratch.0, 10, "11");
    for args in [
        &["--stream", "--estimate-body", "x.pdf"][..],
        // no window: the sample is the whole document
        &["--estimate-body", "x.pdf"],
        // a range shorter than the sample
        &["--stream", "--estimate-body", "-f", "2", "-l", "3", "x.pdf"],
        &["--window", "2", "--body-pages", "200", "--estimate-body", "x.pdf"],
    ] {
        let r = run(&fake, args);
        assert_eq!(r.status.code(), Some(0), "{args:?}: {}", String::from_utf8_lossy(&r.stderr));
        let est: serde_json::Value = serde_json::from_slice(&r.stdout).unwrap();
        assert_eq!(est["body_size"], 11, "{args:?}");
        assert_eq!(est["body_color"], "#000000", "{args:?}");
        assert_eq!(est["body_sample_pages"], 10, "{args:?}");
    }
    // still exit status 2 when nothing is read (not the fork)
    if Path::new("/bin/true").is_file() {
        let r = run(Path::new("/bin/true"), &["--stream", "--estimate-body", "x.pdf"]);
        assert_eq!(r.status.code(), Some(2));
        assert!(String::from_utf8_lossy(&r.stderr).contains("no <page>"));
    }
}

#[cfg(unix)]
#[test]
fn every_estimate_is_accepted_back_as_the_body() {
    // sizes a hostile PDF can set through its text matrix
    for (k, size) in ["0.2", "20000", "-12", "1e308", "12.25"].into_iter().enumerate() {
        let scratch = Scratch(std::env::temp_dir().join(format!("pdftomarkdown-roundtrip-{}-{k}", std::process::id())));
        std::fs::create_dir_all(&scratch.0).unwrap();
        let fake = fake_pdftotext(&scratch.0, 3, size);
        let r = run(&fake, &["--stream", "--estimate-body", "x.pdf"]);
        assert_eq!(r.status.code(), Some(0), "{size}");
        let est: serde_json::Value = serde_json::from_slice(&r.stdout).unwrap();
        let (bs, bc) = (est["body_size"].to_string(), est["body_color"].as_str().unwrap().to_string());
        let back = run(&fake, &["--stream", "--body-size", &bs, "--body-color", &bc, "-f", "2", "-l", "2", "x.pdf"]);
        assert_eq!(back.status.code(), Some(0), "{size} -> {bs}: {}", String::from_utf8_lossy(&back.stderr));
    }
}

#[cfg(unix)]
#[test]
fn a_range_reads_the_window_either_side_by_default() {
    let scratch = Scratch(std::env::temp_dir().join(format!("pdftomarkdown-ctx-{}", std::process::id())));
    std::fs::create_dir_all(&scratch.0).unwrap();
    let fake = fake_pdftotext(&scratch.0, 50, "10");
    let read = |args: &[&str]| {
        let r = run(&fake, &[args, &["-f", "20", "-l", "25", "x.pdf"]].concat());
        assert_eq!(r.status.code(), Some(0), "{args:?}: {}", String::from_utf8_lossy(&r.stderr));
        let a = std::fs::read_to_string(scratch.0.join("args.txt")).unwrap();
        let a: Vec<&str> = a.split_whitespace().collect();
        let at = |f: &str| a[a.iter().position(|x| *x == f).unwrap() + 1].to_string();
        (at("-f"), at("-l"))
    };
    let p = |f: &str, l: &str| (f.to_string(), l.to_string());
    assert_eq!(read(&["--stream"]), p("16", "29"));
    assert_eq!(read(&["--window", "2"]), p("18", "27"));
    assert_eq!(read(&["--window", "2", "--context", "0"]), p("20", "25"));
    assert_eq!(read(&["--stream", "--context", "3"]), p("17", "28"));
    // no window: repetition is judged over the whole document, which context cannot give
    assert_eq!(read(&[]), p("20", "25"));
}

#[test]
fn window_and_body_options_are_bounded() {
    let bad = |args: &[&str]| {
        let r = Command::new(PDFTOMARKDOWN).args(args).arg("x.pdf").env("PDFTOTEXT", "/nonexistent").output().unwrap();
        assert_eq!(r.status.code(), Some(2), "{args:?}");
        let err = String::from_utf8_lossy(&r.stderr).to_string();
        // the usage text is whole, with the window and body options
        assert!(!err.contains("%WINDOW%") && err.contains("--body-pages N"), "{err}");
    };
    bad(&["--window", "65"]);
    bad(&["--body-pages", "257"]);
    bad(&["--body-pages", "0"]);
    bad(&["--context", "65"]);
    bad(&["--body-size", "0.49"]);
    bad(&["--body-size", "1000.5"]);
    bad(&["--body-size", "5e-324"]);
    bad(&["--body-color", &"c".repeat(65)]);
    bad(&["-f", "50", "-l", "40"]);
    // the bounds themselves are accepted (then pdftotext cannot run: status 1)
    for args in [
        &["--window", "64", "--body-pages", "256", "--context", "64"][..],
        &["--body-size", "0.5"],
        &["--body-size", "1000"],
    ] {
        let r = Command::new(PDFTOMARKDOWN).args(args).arg("x.pdf").env("PDFTOTEXT", "/nonexistent").output().unwrap();
        assert_eq!(r.status.code(), Some(1), "{args:?}: {}", String::from_utf8_lossy(&r.stderr));
    }
}
