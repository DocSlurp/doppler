//! `pdftomarkdown --page-facts FILE` / `--page-facts-fd N`: the facts pdftotext writes on the
//! conversion's pass are byte-identical to `pdfpagefacts` for the same range and level, and
//! the Markdown does not change. Needs the DocSlurp pdftotext and pdfpagefacts (`$PDFTOTEXT`,
//! `$PDFPAGEFACTS`, else `../../build/utils/`); skipped (passing) without them, unless
//! `DOPPLER_REQUIRE_INTEGRATION` is set (tests/toolchain).

mod toolchain;

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
        toolchain::skip("page_facts", "the DocSlurp pdftotext with -page-facts-fd and pdfpagefacts");
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

/// The scan thresholds reach pdftotext: the facts (and their doc line) are pdfpagefacts' with
/// the same options, and the Markdown places the page that became a scan as one.
#[test]
fn scan_thresholds_reach_the_facts_and_the_markdown() {
    let Some((pdftotext, facts_exe)) = tools() else { return };
    let s = Scratch::new("thresholds");
    let pdf = s.0.join("doc.pdf");
    std::fs::write(&pdf, make_pdf()).unwrap();
    // page 4's picture covers 200 x 150 of 612 x 792 (0.062) under 40 visible words
    let scan = ["--scan-min-coverage", "0.06", "--scan-max-words", "40", "--scan-layer-overlap", "0.05"];
    let f = s.0.join("thresholds.jsonl");
    // --scan-pages image: page 4's scan has visible words, so the default (copy) would not mark it
    let mut args =
        vec!["--markers", "--scan-pages", "image", "--page-facts", f.to_str().unwrap(), "--page-facts-level", "full"];
    args.extend(scan);
    let out = md(&pdftotext, &args, &pdf);
    assert_eq!(out.status.code(), Some(0), "{}", String::from_utf8_lossy(&out.stderr));
    let reference = pdfpagefacts(
        &facts_exe,
        &["-level", "full", "-scan-min-coverage", "0.06", "-scan-max-words", "40", "-scan-layer-overlap", "0.05"],
        &pdf,
    );
    let got = std::fs::read(&f).unwrap();
    assert_eq!(got, reference, "--page-facts with thresholds != pdfpagefacts with them");
    let lines: Vec<serde_json::Value> =
        String::from_utf8(got).unwrap().lines().map(|l| serde_json::from_str(l).unwrap()).collect();
    assert_eq!(
        lines[0]["scanThresholds"],
        serde_json::json!({"minCoverage": 0.06, "maxVisibleWords": 40, "layerOverlap": 0.05})
    );
    let scans: Vec<&str> = lines[1..5].iter().map(|l| l["images"]["scan"].as_str().unwrap()).collect();
    assert_eq!(scans, ["none", "single", "none", "single"]);
    let md_pages: Vec<String> = String::from_utf8(out.stdout).unwrap().split('\u{c}').map(String::from).collect();
    assert!(md_pages[3].starts_with("![page 4 scan]("), "{}", md_pages[3]);
    // by default page 4 is not a scan, and the doc line has no thresholds
    let d = s.0.join("default.jsonl");
    let out = md(&pdftotext, &["--markers", "--page-facts", d.to_str().unwrap()], &pdf);
    assert_eq!(out.status.code(), Some(0));
    assert!(!String::from_utf8(out.stdout).unwrap().contains("![page 4 scan]"));
    let text = std::fs::read_to_string(&d).unwrap();
    assert!(!text.contains("scanThresholds"), "{text}");
    // the defaults spelled out: the same bytes as none given
    let e = s.0.join("explicit.jsonl");
    let out = md(
        &pdftotext,
        &[
            "--markers",
            "--page-facts",
            e.to_str().unwrap(),
            "--scan-min-coverage",
            "0.85",
            "--scan-max-words",
            "20",
            "--scan-layer-overlap",
            ".5",
        ],
        &pdf,
    );
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(std::fs::read_to_string(&e).unwrap(), text);
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
        vec!["--scan-max-words"],
        vec!["--scan-min-coverage", "0"],
        vec!["--scan-min-coverage", "1.5"],
        vec!["--scan-min-coverage", "nan"],
        vec!["--scan-min-coverage", "1e-1"],
        vec!["--scan-layer-overlap", "-0.5"],
        vec!["--scan-layer-overlap", "inf"],
        vec!["--scan-layer-overlap", ""],
        vec!["--scan-layer-overlap"],
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

#[test]
fn a_closed_facts_reader_keeps_the_markdown() {
    let Some((pdftotext, _)) = tools() else { return };
    let s = Scratch::new("epipe");
    let pdf = s.0.join("doc.pdf");
    std::fs::write(&pdf, make_pdf()).unwrap();
    let err = s.0.join("stderr.txt");
    // descriptor 5 is the write end of a pipe whose reader is gone before pdftotext writes
    let mut child = Command::new("sh")
        .arg("-c")
        .arg(r#"exec "$0" --page-facts-fd 5 --page-facts-level full "$1" 5>&2 2>"$2""#)
        .arg(PDFTOMARKDOWN)
        .arg(&pdf)
        .arg(&err)
        .env("PDFTOTEXT", &pdftotext)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    drop(child.stderr.take());
    let out = child.wait_with_output().unwrap();
    let message = std::fs::read_to_string(&err).unwrap_or_default();
    // the Markdown stands; the failed sidecar is exit status 6
    assert_eq!(out.status.code(), Some(6), "{message}");
    assert_eq!(out.stdout, md(&pdftotext, &[], &pdf).stdout);
    assert!(message.contains("could not write the page facts"), "{message}");
}

#[test]
fn facts_file_appears_before_image_extraction() {
    let Some((pdftotext, _)) = tools() else { return };
    let s = Scratch::new("early");
    let pdf = s.0.join("doc.pdf");
    std::fs::write(&pdf, make_pdf()).unwrap();
    let f = s.0.join("facts.jsonl");
    let dir = s.0.join("images");
    // a renderer that records whether the facts file is already in place, then fails
    let probe = s.0.join("renderer.sh");
    let seen = s.0.join("seen.txt");
    std::fs::write(
        &probe,
        format!("#!/bin/sh\nif [ -f '{}' ]; then echo yes > '{}'; fi\nexit 1\n", f.display(), seen.display()),
    )
    .unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&probe, std::fs::Permissions::from_mode(0o755)).unwrap();
    let out = Command::new(PDFTOMARKDOWN)
        .args(["--page-facts", f.to_str().unwrap(), "--images-dir", dir.to_str().unwrap()])
        .arg(&pdf)
        .env("PDFTOTEXT", &pdftotext)
        .env("PDFREGIONS", &probe)
        .output()
        .unwrap();
    assert!(f.is_file(), "{}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(
        std::fs::read_to_string(&seen).unwrap_or_default().trim(),
        "yes",
        "facts file missing during extraction"
    );
}

#[test]
fn pdftotext_is_bounded_in_time_with_and_without_facts() {
    let Some((pdftotext, _)) = tools() else { return };
    let s = Scratch::new("bound");
    let pdf = s.0.join("doc.pdf");
    std::fs::write(&pdf, make_pdf()).unwrap();
    // a pdftotext that records its arguments, then runs the real one
    let wrapper = s.0.join("pdftotext.sh");
    let argv = s.0.join("argv.txt");
    std::fs::write(
        &wrapper,
        format!("#!/bin/sh\necho \"$@\" >> '{}'\nexec '{}' \"$@\"\n", argv.display(), pdftotext.display()),
    )
    .unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&wrapper, std::fs::Permissions::from_mode(0o755)).unwrap();
    let plain = md(&wrapper, &[], &pdf);
    assert!(plain.status.success(), "{}", String::from_utf8_lossy(&plain.stderr));
    let f = s.0.join("facts.jsonl");
    let with = md(&wrapper, &["--page-facts", f.to_str().unwrap()], &pdf);
    assert!(with.status.success(), "{}", String::from_utf8_lossy(&with.stderr));
    assert_eq!(plain.stdout, with.stdout);
    let runs = std::fs::read_to_string(&argv).unwrap();
    let runs: Vec<&str> = runs.lines().collect();
    assert_eq!(runs.len(), 2, "{runs:?}");
    // the same bound, facts or not
    for run in &runs {
        assert!(run.contains("-timeout 60 -timeout-per-page 1 -timeout-max 3600"), "{run}");
    }
}

#[test]
fn a_pdftotext_timeout_is_named() {
    let Some(_) = tools() else { return };
    let s = Scratch::new("timeout-exit");
    let pdf = s.0.join("doc.pdf");
    std::fs::write(&pdf, make_pdf()).unwrap();
    // a pdftotext that times out (exit status 6)
    let fake = s.0.join("pdftotext.sh");
    std::fs::write(&fake, "#!/bin/sh\nexit 6\n").unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
    let f = s.0.join("facts.jsonl");
    let out = md(&fake, &["--page-facts", f.to_str().unwrap()], &pdf);
    let message = String::from_utf8_lossy(&out.stderr);
    // the conversion failed (1): never pdftomarkdown's own 6, which means the Markdown stands
    assert_eq!(out.status.code(), Some(1), "{message}");
    assert!(out.stdout.is_empty());
    assert!(message.contains("ran past its time limit"), "{message}");
}

#[test]
fn an_unterminated_last_facts_line_is_dropped() {
    let Some(_) = tools() else { return };
    let s = Scratch::new("partial-line");
    let pdf = s.0.join("doc.pdf");
    std::fs::write(&pdf, make_pdf()).unwrap();
    use std::os::unix::fs::PermissionsExt;
    // a pdftotext that stops mid-line on the facts descriptor, as at its time limit while a
    // reader stalled (exit status 6), after <whole> whole lines
    let fake = |name: &str, whole: &str| {
        let p = s.0.join(name);
        std::fs::write(
            &p,
            format!(
                "#!/bin/sh\nwhile [ $# -gt 0 ]; do [ \"$1\" = -page-facts-fd ] && fd=$2; shift; done\n\
                 eval \"printf '%b' '{whole}{{\\\"type\\\":\\\"pa' >&$fd\"\nexit 6\n"
            ),
        )
        .unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
        p
    };
    let f = s.0.join("facts.jsonl");
    let out = md(&fake("one.sh", "{\\\"type\\\":\\\"doc\\\"}\\n"), &["--page-facts", f.to_str().unwrap()], &pdf);
    let message = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success());
    assert!(message.contains("dropped an unterminated last line"), "{message}");
    assert_eq!(std::fs::read(&f).unwrap(), b"{\"type\":\"doc\"}\n");
    // nothing whole: no file at all
    let g = s.0.join("none.jsonl");
    let out = md(&fake("none.sh", ""), &["--page-facts", g.to_str().unwrap()], &pdf);
    let message = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success());
    assert!(message.contains("no whole line"), "{message}");
    assert!(!g.exists());
    assert_eq!(
        std::fs::read_dir(&s.0)
            .unwrap()
            .filter(|e| e.as_ref().unwrap().file_name().to_string_lossy().ends_with(".part"))
            .count(),
        0
    );
}
