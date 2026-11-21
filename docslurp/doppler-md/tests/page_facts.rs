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
    let supported = Command::new(&pdftotext)
        .arg("-h")
        .output()
        .is_ok_and(|usage| String::from_utf8_lossy(&usage.stderr).contains("-page-facts-fd"));
    if !supported || !facts.is_file() {
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
/// table, and a rotated page with an invisible text layer in an inset under a visible heading.
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
        format!("q 420 0 0 240 54 300 cm /Im1 Do Q\n{}q\n{}Q\n", text(710, ""), text(500, "3 Tr ")),
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
    encode_pdf(&objects)
}

fn encode_pdf(objects: &[String]) -> Vec<u8> {
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

#[test]
fn text_render_modes_in_page_facts_and_page_info() {
    let Some((pdftotext, facts_exe)) = tools() else { return };
    let scratch = Scratch::new("render-modes");
    let pdf = scratch.0.join("render-modes.pdf");
    let cases = [
        ("Tr 3", "/F1 12 Tf 3 Tr", "", "", "(WWWW) Tj", 4u64, 1),
        ("Tr 7 with subsequent paint through the clip", "/F1 12 Tf 7 Tr", "", "0 0 612 792 re f", "(WWWW) Tj", 4, 1),
        ("Tr 0", "/F1 12 Tf 0 Tr", "", "", "(WWWW) Tj", 0, 0),
        ("Tr 1", "/F1 12 Tf 1 Tr", "", "", "(WWWW) Tj", 0, 0),
        ("Tr 2", "/F1 12 Tf 2 Tr", "", "", "(WWWW) Tj", 0, 0),
        ("Tr 4", "/F1 12 Tf 4 Tr", "", "", "(WWWW) Tj", 0, 0),
        ("Tr 5", "/F1 12 Tf 5 Tr", "", "", "(WWWW) Tj", 0, 0),
        ("Tr 6", "/F1 12 Tf 6 Tr", "", "", "(WWWW) Tj", 0, 0),
        ("pattern-filled Tr 0", "/F1 12 Tf 0 Tr", "/Pattern cs /P1 scn", "", "(WWWW) Tj", 0, 0),
        ("clip-only Tr 7 with a pattern selected", "/F1 12 Tf 7 Tr", "/Pattern cs /P1 scn", "", "(WWWW) Tj", 4, 1),
        ("Type 3 Tr 7", "/T3 12 Tf 7 Tr", "", "", "(WWWW) Tj", 0, 0),
        ("Type 3 Tr 3 (legacy classification)", "/T3 12 Tf 3 Tr", "", "", "(WWWW) Tj", 4, 1),
        ("Tr 7 then Tr 0", "/F1 12 Tf 7 Tr", "", "", "(WW) Tj 0 Tr (WW) Tj", 2, 1),
        ("Tr 0 then Tr 7", "/F1 12 Tf 0 Tr", "", "", "(WW) Tj 7 Tr (WW) Tj", 2, 0),
        ("Tr 3 then Tr 0", "/F1 12 Tf 3 Tr", "", "", "(WW) Tj 0 Tr (WW) Tj", 2, 1),
        ("Tr 0 then Tr 3", "/F1 12 Tf 0 Tr", "", "", "(WW) Tj 3 Tr (WW) Tj", 2, 0),
        ("q/Q restores painting", "/F1 12 Tf", "0 Tr q 7 Tr Q", "", "(WWWW) Tj", 0, 0),
        ("Form restores painting", "/F1 12 Tf", "0 Tr /Fm Do", "", "(WWWW) Tj", 0, 0),
    ];
    let kids = (9..9 + 2 * cases.len()).step_by(2).map(|id| format!("{id} 0 R")).collect::<Vec<_>>().join(" ");
    let stream =
        |dict: &str, content: &str| format!("<< {dict} /Length {} >>\nstream\n{content}endstream", content.len());
    let mut objects = vec![
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        format!("<< /Type /Pages /Kids [{kids}] /Count {} >>", cases.len()),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
        stream(
            "/Type /Pattern /PatternType 1 /PaintType 1 /TilingType 1 /BBox [0 0 4 4] /XStep 4 /YStep 4 /Resources << >>",
            "0 g 0 0 3 4 re f\n",
        ),
        "<< /Type /Font /Subtype /Type3 /FontBBox [0 0 600 700] /FontMatrix [0.001 0 0 0.001 0 0] /CharProcs << /W 6 0 R >> /Encoding /WinAnsiEncoding /FirstChar 87 /LastChar 87 /Widths [600] /Resources << >> >>".to_string(),
        stream("", "600 0 0 0 600 700 d1 0 0 500 650 re f\n"),
        stream(
            "/Type /XObject /Subtype /Image /Width 1 /Height 1 /ColorSpace /DeviceGray /BitsPerComponent 8 /Filter /ASCIIHexDecode",
            "41>\n",
        ),
        stream("/Type /XObject /Subtype /Form /BBox [0 0 612 792] /Resources << >>", "BT 7 Tr ET\n"),
    ];
    for (_, text_state, before, after, shows, _, _) in cases {
        let stream_id = objects.len() + 2;
        objects.push(format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 3 0 R /T3 5 0 R >> /Pattern << /P1 4 0 R >> /XObject << /Im 7 0 R /Fm 8 0 R >> >> /Contents {stream_id} 0 R >>"
        ));
        objects.push(stream(
            "",
            &format!("q 612 0 0 792 0 0 cm /Im Do Q q {before} BT {text_state} 72 700 Td {shows} ET {after} Q\n"),
        ));
    }
    std::fs::write(&pdf, encode_pdf(&objects)).unwrap();

    let facts_file = scratch.0.join("facts.jsonl");
    let out = md(
        &pdftotext,
        &["--page-info", "--report", "--page-facts", facts_file.to_str().unwrap(), "--scan-max-words", "0"],
        &pdf,
    );
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let facts = pdfpagefacts(&facts_exe, &["-scan-max-words", "0"], &pdf);
    assert_eq!(std::fs::read(facts_file).unwrap(), facts);
    let pages: Vec<serde_json::Value> = std::str::from_utf8(&facts)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
        .filter(|line| line["type"] == "page")
        .collect();
    let stderr = std::str::from_utf8(&out.stderr).unwrap();
    let report: serde_json::Value = serde_json::from_str(stderr.lines().last().unwrap()).unwrap();
    let info = report["page_info"].as_array().unwrap();

    let math_out = md(&pdftotext, &["--math", "--page-info", "--report"], &pdf);
    assert!(math_out.status.success(), "{}", String::from_utf8_lossy(&math_out.stderr));
    let math_stderr = std::str::from_utf8(&math_out.stderr).unwrap();
    let math_report: serde_json::Value = serde_json::from_str(math_stderr.lines().last().unwrap()).unwrap();
    assert_eq!(math_report["page_info"], report["page_info"]);
    let ocr_pages: Vec<_> = math_report["degraded"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|region| region["reason"] == "ocr_text_layer")
        .map(|region| region["page"].as_u64().unwrap())
        .collect();
    let expected_ocr: Vec<_> =
        cases.iter().enumerate().filter(|(_, case)| case.5 == 4).map(|(i, _)| i as u64 + 1).collect();
    assert_eq!(ocr_pages, expected_ocr);

    for (glyphs, compact) in [(false, false), (true, false), (true, true)] {
        let mut command = Command::new(&pdftotext);
        command.args(["-bbox-layout", "-wordmeta"]);
        if glyphs {
            command.arg("-glyphs");
        }
        if compact {
            command.arg("-compact-layout");
        }
        let raw = command.arg(&pdf).arg("-").output().unwrap();
        assert!(raw.status.success(), "{}", String::from_utf8_lossy(&raw.stderr));
        let xml = std::str::from_utf8(&raw.stdout).unwrap();
        let words: Vec<_> = xml.lines().filter(|line| line.trim_start().starts_with("<word ")).collect();
        assert_eq!(words.len(), cases.len());
        for (word, (label, _, _, _, _, hidden_chars, starts_hidden)) in words.iter().zip(cases) {
            assert!(word.contains(&format!(r#" invisible="{starts_hidden}""#)), "{label}: {word}");
            if glyphs {
                let shown = u8::from(hidden_chars < 4);
                assert!(word.contains(&format!(r#" shown="{shown}""#)), "{label}: {word}");
            } else {
                assert!(!word.contains(" shown="), "{label}: {word}");
            }
        }
    }

    assert_eq!(pages.len(), cases.len());
    assert_eq!(info.len(), cases.len());
    for ((page, info), (label, _, _, _, _, hidden_chars, starts_hidden)) in pages.iter().zip(info).zip(cases) {
        let hidden_word = u64::from(hidden_chars == 4);
        let text = &page["text"];
        assert_eq!(text["visibleCharCount"], 4 - hidden_chars, "{label}");
        assert_eq!(text["invisibleCharCount"], hidden_chars, "{label}");
        assert_eq!(text["visibleWordCount"], 1 - hidden_word, "{label}");
        assert_eq!(text["invisibleWordCount"], hidden_word, "{label}");
        assert_eq!(text["textLayer"], if hidden_word == 1 { "invisible" } else { "visible" }, "{label}");
        assert_eq!(page["images"]["scan"], if hidden_word == 1 { "single" } else { "none" }, "{label}");
        assert_eq!(page["images"]["fullPageImage"], true, "{label}");
        // Page info reports the word metadata, which tracks the starting render mode.
        assert_eq!(info["words"], 1, "{label}");
        assert_eq!(info["invisible"], starts_hidden, "{label}");
        assert_eq!(info["text"], if starts_hidden == 1 { "invisible" } else { "visible" }, "{label}");
    }
}

/// One page: prose, a booktabs table (heavier top and bottom rules, three columns, a header and
/// three rows) and a footnote rule.
fn make_booktabs_pdf() -> Vec<u8> {
    let row = |y: i32, cells: [&str; 3]| -> String {
        [80, 200, 320].iter().zip(cells).map(|(x, t)| format!("BT /F1 10 Tf {x} {y} Td ({t}) Tj ET\n")).collect()
    };
    let mut c = String::new();
    for k in 0..3 {
        c += &format!("BT /F1 10 Tf 72 {} Td (Revenue rose in every region again this quarter.) Tj ET\n", 740 - 12 * k);
    }
    c += "0.8 w 72 650 m 400 650 l S\n";
    c += &row(638, ["Region", "Growth", "Margin"]);
    c += "0.5 w 72 630 m 400 630 l S\n";
    c += &row(618, ["North", "12.5", "31"]);
    c += &row(604, ["South", "9.1", "28"]);
    c += &row(590, ["West", "14.2", "35"]);
    c += "0.8 w 72 582 m 400 582 l S\n0.4 w 72 120 m 160 120 l S\n";
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>".to_string(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
        format!("<< /Length {} >>\nstream\n{c}endstream", c.len()),
    ];
    let mut pdf: Vec<u8> = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::new();
    for (i, o) in objects.iter().enumerate() {
        offsets.push(pdf.len());
        pdf.extend(format!("{} 0 obj\n{o}\nendobj\n", i + 1).bytes());
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
    for (level, extra) in [
        ("basic", vec![]),
        ("full", vec![]),
        ("basic", vec!["--page-facts-include", "vectors,forms"]),
        ("basic", vec!["--page-facts-include", "mathtables"]),
        ("full", vec!["--page-facts-include", "scan-regions"]),
    ] {
        for range in [vec![], vec!["-f", "2", "-l", "3"]] {
            for mode in [vec!["--stream", "--markers"], vec![]] {
                let tag = extra.last().map_or(String::new(), |e| e.replace(',', "_"));
                let f = s.0.join(format!("facts-{level}-{tag}-{}-{}.jsonl", range.len(), mode.len()));
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
                if extra.last() == Some(&"scan-regions") && range.is_empty() {
                    assert_eq!(lines[4]["images"]["scanRegions"]["imageIndices"], serde_json::json!([1]));
                    assert_eq!(lines[4]["images"]["scanRegions"]["truncated"], false);
                }
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

/// The context pages read around a range (`--context`) get no facts: for every range, edge and
/// context size the facts file is pdfpagefacts' for the range alone (doc line, the range's page
/// lines, run-line counts), and the Markdown is the same as without --page-facts.
#[test]
fn context_pages_get_no_facts() {
    let Some((pdftotext, facts_exe)) = tools() else { return };
    let s = Scratch::new("context");
    let pdf = s.0.join("doc.pdf");
    std::fs::write(&pdf, make_pdf()).unwrap();
    let ranges: [(Option<&str>, Option<&str>); 5] =
        [(Some("2"), Some("3")), (Some("1"), Some("2")), (Some("3"), Some("4")), (None, Some("2")), (Some("3"), None)];
    for (first, last) in ranges {
        for context in ["0", "1", "3", "16"] {
            let f = s.0.join(format!("facts-{first:?}-{last:?}-{context}.jsonl"));
            let mut range: Vec<&str> = Vec::new();
            if let Some(n) = first {
                range.extend(["-f", n]);
            }
            if let Some(n) = last {
                range.extend(["-l", n]);
            }
            let mut args = range.clone();
            args.extend(["--context", context, "--page-facts", f.to_str().unwrap()]);
            let with = md(&pdftotext, &args, &pdf);
            assert_eq!(with.status.code(), Some(0), "{}", String::from_utf8_lossy(&with.stderr));
            let mut plain = range.clone();
            plain.extend(["--context", context]);
            let without = md(&pdftotext, &plain, &pdf);
            assert_eq!(with.stdout, without.stdout, "Markdown changed by --page-facts ({range:?} context {context})");
            let expected = pdfpagefacts(&facts_exe, &range, &pdf);
            let got = std::fs::read(&f).unwrap();
            assert_eq!(
                String::from_utf8_lossy(&got),
                String::from_utf8_lossy(&expected),
                "facts for {range:?} with context {context}"
            );
        }
    }
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
fn incomplete_content_prevents_image_plan_and_body_estimate() {
    use std::os::unix::fs::PermissionsExt;

    let s = Scratch::new("incomplete-content");
    let pdf = s.0.join("doc.pdf");
    std::fs::write(&pdf, make_pdf()).unwrap();
    let producer = s.0.join("text.sh");
    let plan = s.0.join("image-plan.json");
    for status in [0, 7] {
        std::fs::write(
            &producer,
            format!("#!/bin/sh\nprintf '%s\\n' '<doc><page contentTruncated=\"1\"></page></doc>'\nexit {status}\n"),
        )
        .unwrap();
        std::fs::set_permissions(&producer, std::fs::Permissions::from_mode(0o755)).unwrap();

        let output = md(&producer, &["--markers", "--images-plan-out", plan.to_str().unwrap()], &pdf);
        assert_eq!(output.status.code(), Some(1));
        assert!(String::from_utf8_lossy(&output.stderr).contains("output is incomplete"));
        assert!(std::fs::read(&plan).unwrap_or_default().is_empty());
        let _ = std::fs::remove_file(&plan);

        let output = md(&producer, &["--estimate-body"], &pdf);
        assert_eq!(output.status.code(), Some(1));
        assert!(output.stdout.is_empty());
    }
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

/// The mathtables family reaches the facts file: at full a booktabs table (vectors.ruleTables,
/// tableBits bit 2 `booktabs`); with `--page-facts-include mathtables` at basic only the three
/// packed words are added, the same as pdfpagefacts writes, and the Markdown does not change.
#[test]
fn math_table_facts_reach_the_page_lines() {
    let Some((pdftotext, facts_exe)) = tools() else { return };
    let s = Scratch::new("mathtables");
    let pdf = s.0.join("booktabs.pdf");
    std::fs::write(&pdf, make_booktabs_pdf()).unwrap();
    let full = String::from_utf8(pdfpagefacts(&facts_exe, &["-level", "full"], &pdf)).unwrap();
    let page: serde_json::Value = serde_json::from_str(full.lines().nth(1).unwrap()).unwrap();
    let tables = page["vectors"]["ruleTables"].as_array().unwrap();
    assert_eq!(tables.len(), 1, "{tables:?}");
    assert_eq!(tables[0]["style"], "booktabs");
    assert_eq!(
        (tables[0]["rows"].as_u64(), tables[0]["columns"].as_u64(), tables[0]["hRules"].as_u64()),
        (Some(4), Some(3), Some(3))
    );
    assert_eq!(page["tableBits"].as_u64(), Some(1 << 2));
    assert_eq!(page["tableFlags"], serde_json::json!(["booktabs"]));
    assert_eq!(page["mathBits"].as_u64(), Some(0));

    let f = s.0.join("packed.jsonl");
    let with = md(&pdftotext, &["--page-facts", f.to_str().unwrap(), "--page-facts-include", "mathtables"], &pdf);
    assert_eq!(with.status.code(), Some(0), "{}", String::from_utf8_lossy(&with.stderr));
    assert_eq!(with.stdout, md(&pdftotext, &[], &pdf).stdout, "Markdown changed by mathtables facts");
    let got = std::fs::read(&f).unwrap();
    assert_eq!(got, pdfpagefacts(&facts_exe, &["-include", "mathtables"], &pdf));
    let packed: serde_json::Value =
        serde_json::from_str(String::from_utf8(got).unwrap().lines().nth(1).unwrap()).unwrap();
    assert_eq!(packed["tableBits"], page["tableBits"]);
    assert_eq!(packed["encodingCounts"], page["encodingCounts"]);
    assert!(packed.get("vectors").is_none() && packed.get("tableFlags").is_none(), "{packed}");
}

#[test]
fn bar_geometry_agrees_in_markdown_and_page_facts() {
    let Some((pdftotext, facts)) = tools() else {
        return;
    };
    let scratch = Scratch::new("bar-geometry");
    let generator =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/docslurp/page-facts/bar_geometry_fixtures.py");
    let output = Command::new("python3").arg(generator).arg(&scratch.0).output();
    let Ok(output) = output else {
        toolchain::skip("bar_geometry", "python3 for the vector PDF fixtures");
        return;
    };
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));

    for (name, chart, table) in [
        ("labeled-crossings-behind", true, false),
        ("gridless-horizontal-endpoints", true, false),
        ("gridless-vertical-endpoints", true, false),
        ("gridless-off-scale-endpoints", false, false),
        ("table-with-calibrated-endpoints", false, true),
        ("labeled-crossings-front", true, false),
        ("vertical-labeled", true, false),
        ("one-crossing", true, false),
        ("varying-thickness", true, false),
        ("vertical-single-crossing", true, false),
        ("vertical-uneven-bars", true, false),
        ("one-crossing-off-axis", false, false),
        ("uneven-bars-single-crossing", false, false),
        ("single-rule-forms", false, true),
        ("rounded-staircase", true, false),
        ("chart-with-category-dividers", true, false),
        ("data-bars-in-edge-column", false, true),
        ("chart-with-one-sided-labels", true, false),
        ("scale-contaminated", true, false),
        ("mixed-separate-table", true, true),
        ("unlabeled-in-cell-bars", false, true),
        ("in-cell-bars-with-crossings", false, true),
        ("shaded-cell-boundaries", false, true),
        ("off-axis-numeric-decoration", false, false),
        ("thin-mark-decoration", false, false),
        ("rule-fragment-decoration", false, false),
    ] {
        let pdf = scratch.0.join(format!("{name}.pdf"));
        let result = Command::new(PDFTOMARKDOWN)
            .args(["--markers", "--html", "--report"])
            .arg(&pdf)
            .env("PDFTOTEXT", &pdftotext)
            .output()
            .unwrap();
        assert!(result.status.success(), "{name}: {}", String::from_utf8_lossy(&result.stderr));
        let markdown = String::from_utf8(result.stdout).unwrap();
        assert_eq!(markdown.matches("/figure-").count(), usize::from(chart), "{name}: {markdown}");
        assert_eq!(markdown.matches("<table>").count(), usize::from(table), "{name}: {markdown}");
        if name == "mixed-separate-table" {
            let cells: Vec<_> = markdown.lines().filter(|line| line.starts_with("<tr>")).collect();
            assert_eq!(
                cells,
                (1..=5).rev().map(|i| format!("<tr><td>Sensor {i}</td><td>{}</td></tr>", 29 + i)).collect::<Vec<_>>()
            );
            let diagnostics = String::from_utf8(result.stderr).unwrap();
            let report: serde_json::Value = serde_json::from_str(diagnostics.lines().last().unwrap()).unwrap();
            let figures: Vec<_> = report["images"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|image| {
                    image["kind"] == "vector" && matches!(image["placed"].as_str(), Some("block" | "inline"))
                })
                .collect();
            assert_eq!(figures.len(), 1, "{report}");
            let bbox = figures[0]["bbox"].as_array().unwrap();
            let left = bbox[0].as_f64().unwrap();
            let right = bbox[2].as_f64().unwrap();
            assert!(130.0 < left && left < 160.0 && 330.0 < right && right < 370.0, "{report}");
        }
        if name == "data-bars-in-edge-column" {
            let cells: Vec<_> = markdown.lines().filter(|line| line.starts_with("<tr>")).collect();
            assert_eq!(
                cells,
                (1..=4)
                    .rev()
                    .map(|i| format!("<tr><td>Quarter {i}</td><td>{}</td></tr>", (i + 1) * 10))
                    .collect::<Vec<_>>()
            );
        }
        if name == "in-cell-bars-with-crossings" {
            assert!(markdown.contains("<td>North</td><td>Alder</td>"), "{markdown}");
            assert!(markdown.contains("<td>40</td>"), "{markdown}");
        }
        let result = Command::new(&facts).args(["-level", "full"]).arg(&pdf).output().unwrap();
        assert!(result.status.success(), "{name}: {}", String::from_utf8_lossy(&result.stderr));
        let json = String::from_utf8(result.stdout).unwrap();
        assert!(json.contains(&format!("\"figureClusters\":{}", usize::from(chart))), "{name}: {json}");
    }
}

#[test]
fn endpoint_crops_include_trailing_ticks_and_shared_legend() {
    let Some((pdftotext, facts)) = tools() else {
        return;
    };
    let scratch = Scratch::new("bar-axis-crop");
    let generator =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/docslurp/page-facts/bar_geometry_fixtures.py");
    let Ok(output) = Command::new("python3").arg(generator).arg(&scratch.0).output() else {
        toolchain::skip("bar_axis_crop", "python3 for the vector PDF fixtures");
        return;
    };
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));

    for (name, count) in [("shared-legend-panels", 1), ("independent-legend-panels", 2)] {
        let pdf = scratch.0.join(format!("{name}.pdf"));
        let result = Command::new(PDFTOMARKDOWN)
            .args(["--markers", "--report"])
            .arg(&pdf)
            .env("PDFTOTEXT", &pdftotext)
            .output()
            .unwrap();
        assert!(result.status.success(), "{name}: {}", String::from_utf8_lossy(&result.stderr));
        let stderr = String::from_utf8(result.stderr).unwrap();
        let report: serde_json::Value = serde_json::from_str(stderr.lines().last().unwrap()).unwrap();
        let crops: Vec<_> = report["images"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|image| image["kind"] == "vector" && image["placed"] == "block")
            .map(|image| &image["bbox"])
            .collect();
        assert_eq!(crops.len(), count, "{name}: {report}");
        for (i, crop) in crops.iter().enumerate() {
            let edge = |j: usize| crop[j].as_f64().unwrap();
            assert!(edge(0) <= 133.0 + 305.0 * i as f64, "{name}: {crop}");
            let far_tick = if count == 1 || i == 1 { 650.0 } else { 345.0 };
            assert!(edge(2) >= far_tick, "{name}: {crop}");
            assert!((524.0..555.0).contains(&edge(3)), "{name}: {crop}");
        }
        let collected = pdfpagefacts(&facts, &["-level", "full"], &pdf);
        let record: serde_json::Value =
            serde_json::from_str(String::from_utf8(collected).unwrap().lines().nth(1).unwrap()).unwrap();
        assert_eq!(record["vectors"]["figureClusters"], 2, "{name}: {record}");
    }
}

#[test]
fn closed_components_keep_semantic_cells_and_exclude_outside_text() {
    let Some((pdftotext, _)) = tools() else {
        return;
    };
    let scratch = Scratch::new("closed-components");
    let generator =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/docslurp/page-facts/bar_geometry_fixtures.py");
    let generator = Command::new("python3").arg(generator).arg(&scratch.0).output().unwrap();
    assert!(generator.status.success(), "{}", String::from_utf8_lossy(&generator.stderr));

    let result = Command::new(PDFTOMARKDOWN)
        .args(["--stream", "--markers", "--page-markers", "--html"])
        .arg(scratch.0.join("closed-components.pdf"))
        .env("PDFTOTEXT", pdftotext)
        .output()
        .unwrap();
    assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
    let markdown = String::from_utf8(result.stdout).unwrap();
    let expected = concat!(
        "<table>\n<tbody>\n",
        "<tr><td>Range name</td><td></td><td>Net total</td></tr>\n",
        "<tr><td>Ash</td><td></td><td>11</td></tr>\n",
        "<tr><td>Fir</td><td></td><td>24</td></tr>\n",
        "<tr><td>Yew</td><td></td><td>52</td></tr>\n",
        "</tbody>\n</table>",
    );
    assert_eq!(markdown.matches(expected).count(), 12, "{markdown}");
    let text: Vec<_> = markdown.split(expected).collect();
    assert!(text.iter().any(|part| part.contains("10") && part.contains("30")), "{markdown}");
    assert_eq!(markdown.matches("<table>").count(), 12, "{markdown}");
    assert!(!markdown.contains("/figure-"), "{markdown}");
}
