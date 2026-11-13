//! `page_info` in the report: per-page crop size (pdftotext -crop-sizes), word counts, text layer.
//! The pdftotext test is skipped without `$PDFTOTEXT`, unless `DOPPLER_REQUIRE_INTEGRATION` is
//! set (tests/toolchain).

mod toolchain;

use doppler_md::{Params, to_markdown_report};

fn word(text: &str, invisible: bool) -> String {
    format!(
        r##"<word xMin="10" yMin="10" xMax="40" yMax="20" fontName="F" fontSize="10" rotation="0" spaceAfter="1" invisible="{}" color="#000000" baseline="19">{text}</word>"##,
        invisible as u8
    )
}

fn page(attrs: &str, words: &[(&str, bool)]) -> String {
    let ws: String = words.iter().map(|(t, i)| word(t, *i)).collect();
    let body = if ws.is_empty() {
        String::new()
    } else {
        format!(
            r#"<flow><block xMin="10" yMin="10" xMax="40" yMax="20"><line xMin="10" yMin="10" xMax="40" yMax="20">{ws}</line></block></flow>"#
        )
    };
    format!(r#"<page width="612" height="792"{attrs}>{body}</page>"#)
}

fn report(pages: &[String]) -> String {
    let doc = format!("<doc>{}</doc>", pages.concat());
    to_markdown_report(&doc, &Params::default()).1.to_json()
}

#[test]
fn absent_without_crop_sizes() {
    let r = report(&[page("", &[("hello", false)])]);
    assert!(!r.contains("page_info"), "{r}");
}

#[test]
fn layers_counts_and_sizes() {
    let r = report(&[
        page(r#" crop="612 792""#, &[]),
        page(r#" crop="595.276 841.89""#, &[("ocr", true), ("layer", true)]),
        page(r#" crop="612 792""#, &[("seen", false)]),
        page(r#" crop="1e+06 5""#, &[("a", false), ("b", true)]),
        page(r#" crop="612&quot;,&quot;x""#, &[("q", false)]),
    ]);
    let v: serde_json::Value = serde_json::from_str(&r).unwrap();
    let info = v["page_info"].as_array().unwrap();
    let got: Vec<(serde_json::Value, u64, u64, &str)> = info
        .iter()
        .map(|p| {
            (
                p["size"].clone(),
                p["words"].as_u64().unwrap(),
                p["invisible"].as_u64().unwrap(),
                p["text"].as_str().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        got,
        vec![
            ("612 792".into(), 0, 0, "none"),
            ("595.276 841.89".into(), 2, 2, "invisible"),
            ("612 792".into(), 1, 0, "visible"),
            ("1e+06 5".into(), 2, 1, "mixed"),
            // not a short number: dropped, the page is still listed
            (serde_json::Value::Null, 1, 0, "visible"),
        ]
    );
    assert_eq!(info[0]["page"], 1);
    assert_eq!(info[4]["page"], 5);
}

/// `pdftomarkdown --page-info -f 2 -l 3`: the entries name the PDF's pages 2 and 3, with their
/// sizes (needs the DocSlurp pdftotext, `$PDFTOTEXT`; skipped without it).
#[test]
fn a_page_range_reports_the_pdfs_page_numbers() {
    use std::path::PathBuf;
    use std::process::Command;
    let Some(exe) = std::env::var_os("PDFTOTEXT").filter(|p| !p.is_empty()).map(PathBuf::from) else {
        toolchain::skip("page_info", "PDFTOTEXT set to the DocSlurp pdftotext");
        return;
    };
    let sizes = [(612, 792), (595, 842), (400, 792)];
    let mut objects = vec![
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R 4 0 R 5 0 R] /Count 3 >>".to_string(),
    ];
    for (w, h) in sizes {
        objects.push(format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {w} {h}] /Resources << /Font << /F1 6 0 R >> >> /Contents 7 0 R >>"));
    }
    objects.push("<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".into());
    let content = "BT /F1 11 Tf 72 300 Td (Quarterly revenue grew again.) Tj ET\n";
    objects.push(format!("<< /Length {} >>\nstream\n{content}endstream", content.len()));
    let mut pdf = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::new();
    for (i, o) in objects.iter().enumerate() {
        offsets.push(pdf.len());
        pdf.extend_from_slice(format!("{} 0 obj\n{o}\nendobj\n", i + 1).as_bytes());
    }
    let xref = pdf.len();
    let mut tail = format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1);
    for o in offsets {
        tail += &format!("{o:010} 00000 n \n");
    }
    tail += &format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objects.len() + 1);
    pdf.extend_from_slice(tail.as_bytes());
    let path = std::env::temp_dir().join(format!("page-info-range-{}.pdf", std::process::id()));
    std::fs::write(&path, pdf).unwrap();
    let info = |args: &[&str]| -> Vec<(u64, String)> {
        let out = Command::new(env!("CARGO_BIN_EXE_pdftomarkdown"))
            .args(["--page-info", "--report"])
            .args(args)
            .arg(&path)
            .env("PDFTOTEXT", &exe)
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(0), "{}", String::from_utf8_lossy(&out.stderr));
        let line = String::from_utf8_lossy(&out.stderr).lines().last().unwrap().to_string();
        let v: serde_json::Value = serde_json::from_str(&line).unwrap();
        v["page_info"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| (p["page"].as_u64().unwrap(), p["size"].as_str().unwrap().to_string()))
            .collect()
    };
    let all = info(&[]);
    let range = info(&["-f", "2", "-l", "3"]);
    let with_markers = info(&["--markers", "-f", "3"]);
    let _ = std::fs::remove_file(&path);
    assert_eq!(all, [(1, "612 792".to_string()), (2, "595 842".to_string()), (3, "400 792".to_string())]);
    assert_eq!(range, all[1..]);
    assert_eq!(with_markers, all[2..]);
}
