//! `pdftomarkdown --page-facts FILE` / `--page-facts-fd N`: the facts pdftotext writes on the
//! conversion's pass are byte-identical to `pdfpagefacts` for the same range and level, and
//! the Markdown does not change. Needs the DocSlurp pdftotext and pdfpagefacts (`$PDFTOTEXT`,
//! `$PDFPAGEFACTS`, else `../../build/utils/`); skipped (passing) without them.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const PDFTOMARKDOWN: &str = env!("CARGO_BIN_EXE_pdftomarkdown");

fn tools() -> Option<(PathBuf, PathBuf)> {
    let build = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../build/utils");
    let pdftotext =
        std::env::var_os("PDFTOTEXT").filter(|p| !p.is_empty()).map_or(build.join("pdftotext"), PathBuf::from);
    let facts = std::env::var_os("PDFPAGEFACTS")
        .filter(|p| !p.is_empty())
        .map(PathBuf::from)
        .or_else(|| pdftotext.parent().map(|d| d.join("pdfpagefacts")))
        .unwrap_or(build.join("pdfpagefacts"));
    let usage = Command::new(&pdftotext).arg("-h").output().ok()?;
    if !String::from_utf8_lossy(&usage.stderr).contains("-page-facts-fd") || !facts.is_file() {
        eprintln!("skipped: needs the DocSlurp pdftotext with -page-facts-fd and pdfpagefacts");
        return None;
    }
    Some((pdftotext, facts))
}

struct Scratch(PathBuf);

impl Scratch {
    fn new(tag: &str) -> Scratch {
        let d = std::env::temp_dir().join(format!("page-facts-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        Scratch(d)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Four pages: text, a full-page picture under an invisible text layer (a scan), a ruled
/// table, a rotated page with a picture.
fn make_pdf() -> Vec<u8> {
    let text = |y: i32, mode: &str| {
        (0..5)
            .map(|i| {
                format!(
                    "BT /F1 11 Tf {mode}72 {} Td (Earnings beat the forecast again in quarter {i}.) Tj ET\n",
                    y - 14 * i
                )
            })
            .collect::<String>()
    };
    let grid: String = (0..5)
        .map(|k| format!("72 {y} m 472 {y} l S {x} 400 m {x} 560 l S\n", y = 560 - 40 * k, x = 72 + 100 * k))
        .collect();
    let contents = [
        text(700, ""),
        format!("q 612 0 0 792 0 0 cm /Im1 Do Q\n{}", text(700, "3 Tr ")),
        format!("{grid}{}", text(300, "")),
        format!("q 200 0 0 150 72 420 cm /Im1 Do Q\n{}", text(300, "")),
    ];
    let mut objects = vec![
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R 4 0 R 5 0 R 6 0 R] /Count 4 >>".to_string(),
    ];
    for i in 0..4 {
        let rotate = if i == 3 { "/Rotate 90" } else { "" };
        objects.push(format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] {rotate} /Resources << /Font << /F1 7 0 R >> /XObject << /Im1 8 0 R >> >> /Contents {} 0 R >>",
            9 + i
        ));
    }
    objects.push("<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".into());
    let jpeg = "\u{ff}\u{d8}placeholder\u{ff}\u{d9}";
    objects.push(format!(
        "<< /Type /XObject /Subtype /Image /Width 2550 /Height 3300 /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /DCTDecode /Length {} >>\nstream\n{jpeg}\nendstream",
        jpeg.chars().count()
    ));
    for c in &contents {
        objects.push(format!("<< /Length {} >>\nstream\n{c}endstream", c.len()));
    }
    let mut pdf: Vec<u8> = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::new();
    for (i, o) in objects.iter().enumerate() {
        offsets.push(pdf.len());
        pdf.extend(format!("{} 0 obj\n", i + 1).bytes());
        // latin-1: the placeholder JPEG bytes are single bytes
        pdf.extend(o.chars().map(|c| c as u32 as u8));
        pdf.extend(b"\nendobj\n");
    }
    let xref = pdf.len();
    let mut tail = format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1);
    for o in offsets {
        tail += &format!("{o:010} 00000 n \n");
    }
    tail += &format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objects.len() + 1);
    pdf.extend(tail.bytes());
    pdf
}

fn md(pdftotext: &Path, args: &[&str], pdf: &Path) -> Output {
    Command::new(PDFTOMARKDOWN).args(args).arg(pdf).env("PDFTOTEXT", pdftotext).output().unwrap()
}

fn pdfpagefacts(exe: &Path, args: &[&str], pdf: &Path) -> Vec<u8> {
    let out = Command::new(exe).args(args).arg(pdf).output().unwrap();
    assert_eq!(out.status.code(), Some(0), "{}", String::from_utf8_lossy(&out.stderr));
    out.stdout
}

#[test]
fn facts_file_equals_pdfpagefacts_and_markdown_is_unchanged() {
    let Some((pdftotext, facts_exe)) = tools() else { return };
    let s = Scratch::new("file");
    let pdf = s.0.join("doc.pdf");
    std::fs::write(&pdf, make_pdf()).unwrap();
    for (level, extra) in
        [("basic", vec![]), ("full", vec![]), ("basic", vec!["--page-facts-include", "vectors,forms"])]
    {
        for range in [vec![], vec!["-f", "2", "-l", "3"]] {
            for mode in [vec!["--stream", "--markers"], vec![]] {
                let f = s.0.join(format!("facts-{level}-{}-{}-{}.jsonl", extra.len(), range.len(), mode.len()));
                let fs = f.to_str().unwrap();
                let mut args: Vec<&str> = mode.clone();
                args.extend(&range);
                args.extend(["--page-facts", fs, "--page-facts-level", level]);
                args.extend(&extra);
                let with = md(&pdftotext, &args, &pdf);
                assert_eq!(with.status.code(), Some(0), "{}", String::from_utf8_lossy(&with.stderr));
                let mut plain_args: Vec<&str> = mode.clone();
                plain_args.extend(&range);
                let without = md(&pdftotext, &plain_args, &pdf);
                assert_eq!(
                    with.stdout, without.stdout,
                    "Markdown changed by --page-facts ({level} {range:?} {mode:?})"
                );

                let mut ref_args: Vec<&str> = vec!["-level", level];
                ref_args.extend(&range);
                if !extra.is_empty() {
                    ref_args.extend(["-include", extra[1]]);
                }
                let expected = pdfpagefacts(&facts_exe, &ref_args, &pdf);
                let got = std::fs::read(&f).unwrap();
                assert_eq!(got, expected, "--page-facts != pdfpagefacts ({level} {range:?} {mode:?})");
                // one doc line, the pages asked for, one run line
                let text = String::from_utf8(got).unwrap();
                let lines: Vec<serde_json::Value> = text.lines().map(|l| serde_json::from_str(l).unwrap()).collect();
                assert_eq!(lines.first().unwrap()["type"], "doc");
                assert_eq!(lines.last().unwrap()["type"], "run");
                let pages: Vec<u64> =
                    lines.iter().filter(|l| l["type"] == "page").map(|l| l["page"].as_u64().unwrap()).collect();
                assert_eq!(pages, if range.is_empty() { vec![1, 2, 3, 4] } else { vec![2, 3] });
                use std::os::unix::fs::PermissionsExt;
                assert_eq!(std::fs::metadata(&f).unwrap().permissions().mode() & 0o777, 0o600);
            }
        }
    }
    // the scan page is recognised
    let f = s.0.join("scan.jsonl");
    let out = md(&pdftotext, &["--page-facts", f.to_str().unwrap()], &pdf);
    assert_eq!(out.status.code(), Some(0));
    let text = std::fs::read_to_string(&f).unwrap();
    let p2: serde_json::Value = serde_json::from_str(text.lines().nth(2).unwrap()).unwrap();
    assert_eq!(p2["images"]["scan"], "single");
    assert_eq!(p2["text"]["textLayer"], "invisible");
    // no part files are left behind
    let leftovers: Vec<_> = std::fs::read_dir(&s.0)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().contains(".part"))
        .collect();
    assert!(leftovers.is_empty(), "{leftovers:?}");
}

#[test]
fn facts_fd_passes_a_callers_descriptor() {
    let Some((pdftotext, facts_exe)) = tools() else { return };
    let s = Scratch::new("fd");
    let pdf = s.0.join("doc.pdf");
    std::fs::write(&pdf, make_pdf()).unwrap();
    let f = s.0.join("fd.jsonl");
    // a shell opens descriptor 5 for the caller (std::process cannot without unsafe)
    let out = Command::new("sh")
        .arg("-c")
        .arg(r#"exec "$0" --stream --page-facts-fd 5 --page-facts-level full "$1" 5>"$2""#)
        .arg(PDFTOMARKDOWN)
        .arg(&pdf)
        .arg(&f)
        .env("PDFTOTEXT", &pdftotext)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0), "{}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(std::fs::read(&f).unwrap(), pdfpagefacts(&facts_exe, &["-level", "full"], &pdf));
    // facts never reach stdout
    assert!(!String::from_utf8_lossy(&out.stdout).contains("doppler-page-facts"));
    // a descriptor that is not open: pdftotext refuses it (exit 1, nothing written)
    let bad = md(&pdftotext, &["--page-facts-fd", "57"], &pdf);
    assert_eq!(bad.status.code(), Some(1));
}

#[test]
fn facts_options_are_checked_before_anything_runs() {
    let Some((pdftotext, _)) = tools() else { return };
    let s = Scratch::new("usage");
    let pdf = s.0.join("doc.pdf");
    std::fs::write(&pdf, make_pdf()).unwrap();
    let existing = s.0.join("exists.jsonl");
    std::fs::write(&existing, b"keep").unwrap();
    let e = existing.to_str().unwrap();
    for args in [
        vec!["--page-facts", e],
        vec!["--page-facts-level", "full"],
        vec!["--page-facts", "x.jsonl", "--page-facts-fd", "3"],
        vec!["--page-facts-fd", "2"],
        vec!["--page-facts-fd", "1"],
        vec!["--page-facts", "y.jsonl", "--page-facts-level", "max"],
        vec!["--page-facts", "y.jsonl", "--page-facts-include", "fonts,nope"],
        vec!["--scan-max-words", "-1"],
    ] {
        let out = md(&pdftotext, &args, &pdf);
        assert_eq!(out.status.code(), Some(2), "{args:?}: {}", String::from_utf8_lossy(&out.stderr));
        assert!(out.stdout.is_empty(), "{args:?}");
    }
    assert_eq!(std::fs::read(&existing).unwrap(), b"keep");
    // a symbolic link in the path is refused
    std::os::unix::fs::symlink(&s.0, s.0.join("link")).unwrap();
    let through_link = s.0.join("link/f.jsonl");
    let out = md(&pdftotext, &["--page-facts", through_link.to_str().unwrap()], &pdf);
    assert_eq!(out.status.code(), Some(2));
    assert!(!s.0.join("f.jsonl").exists());
}
