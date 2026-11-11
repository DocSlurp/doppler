//! `page_info` in the report: per-page crop size (pdftotext -crop-sizes), word counts, text layer.

use doppler_md::{to_markdown_report, Params};

fn word(text: &str, invisible: bool) -> String {
    format!(
        r##"<word xMin="10" yMin="10" xMax="40" yMax="20" fontName="F" fontSize="10" rotation="0" spaceAfter="1" invisible="{}" color="#000000" baseline="19">{text}</word>"##,
        invisible as u8
    )
}

fn page(attrs: &str, words: &[(&str, bool)]) -> String {
    let ws: String = words.iter().map(|(t, i)| word(t, *i)).collect();
    let body = if ws.is_empty() { String::new() } else { format!(r#"<flow><block xMin="10" yMin="10" xMax="40" yMax="20"><line xMin="10" yMin="10" xMax="40" yMax="20">{ws}</line></block></flow>"#) };
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
        .map(|p| (p["size"].clone(), p["words"].as_u64().unwrap(), p["invisible"].as_u64().unwrap(), p["text"].as_str().unwrap()))
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
