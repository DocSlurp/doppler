use doppler_md::{Params, to_markdown};

fn line(text: &str, y: f64, size: f64) -> String {
    format!(
        "<block><line><word xMin=\"40\" xMax=\"400\" yMin=\"{y}\" yMax=\"{}\" fontSize=\"{size}\" baseline=\"{}\">{text}</word></line></block>",
        y + size,
        y + size * 0.8
    )
}
fn doc(size: f64, vary: bool) -> String {
    (1..=4).map(|n| format!("<page width=\"600\" height=\"800\">{}{}{}{}</page>",
        line(&format!("Section {}", if vary { n } else { 7 }), 50.0, size),
        line("A substantial body paragraph provides ordinary font evidence without depending on specific words or a document identifier.", 50.0 + size + 10.0, 10.0),
        line(&format!("Page {n}"), 765.0, 9.0),
        line("Repeated publication label", 12.0, 9.0))).collect()
}
#[test]
fn prominent_numbered_headings_are_not_running_headers() {
    for size in [14.0, 18.0, 24.0] {
        let pages = to_markdown(&doc(size, true), &Params::default());
        for (i, page) in pages.iter().enumerate() {
            assert!(page.contains(&format!("Section {}", i + 1)), "{page}");
            assert!(!page.contains("Repeated publication label"));
            assert!(!page.contains("Page "));
        }
    }
}
#[test]
fn exact_repeated_headers_still_drop() {
    for (size, vary) in [(24.0, false), (9.0, false), (10.0, false)] {
        let pages = to_markdown(&doc(size, vary), &Params::default());
        assert!(pages.iter().all(|page| !page.contains("Section ")));
    }
}

#[test]
fn heading_preservation_can_be_disabled() {
    let mut p = Params::default();
    p.apply_json(r#"{"furniture_preserve_headings":false}"#).unwrap();
    assert!(to_markdown(&doc(18.0, true), &p).iter().all(|s| !s.contains("Section ")));
}

#[test]
fn differing_numbered_margin_text_survives_large_body_gaps() {
    let source = doc(18.0, true)
        .replace("yMin=\"78\"", "yMin=\"180\"")
        .replace("yMax=\"88\"", "yMax=\"190\"")
        .replace("baseline=\"86\"", "baseline=\"188\"");
    assert!(to_markdown(&source, &Params::default()).iter().all(|s| s.contains("Section ")));
}

#[test]
fn literal_repetitions_are_not_overridden_by_other_numbered_lines() {
    let source = doc(18.0, false)
        .replace("Page 1", "Section 8")
        .replace("Page 2", "Section 8")
        .replace("Page 3", "Section 8")
        .replace("Page 4", "Section 8");
    assert!(to_markdown(&source, &Params::default()).iter().all(|s| !s.contains("Section ")));
    let source = doc(18.0, true).replace("Section 2", "Section 1").replace("Section 4", "Section 3");
    assert!(to_markdown(&source, &Params::default()).iter().all(|s| !s.contains("Section ")));
}
#[test]
fn split_heading_fragments_are_not_deleted_by_numeric_normalization() {
    let source =
        doc(18.0, true).replace("</page>", &format!("{}</page>", line("Adjacent heading fragment", 50.0, 18.0)));
    assert!(to_markdown(&source, &Params::default()).iter().all(|s| s.contains("Section ")));
}

#[test]
fn differing_margin_text_is_not_deleted_by_numeric_normalization() {
    for size in [9.0, 14.0, 18.0] {
        for gap in [10.0, 40.0, 120.0] {
            let source = (1..=3)
                .map(|n| {
                    format!(
                        "<page width=\"600\" height=\"800\">{}{}{}</page>",
                        line(&format!("Record {n}"), 40.0, size),
                        line(
                            "Body text supplies ordinary font evidence and must survive extraction.",
                            40.0 + size + gap,
                            10.0
                        ),
                        line(&format!("Page {n}"), 765.0, 9.0)
                    )
                })
                .collect::<String>();
            for (i, page) in to_markdown(&source, &Params::default()).iter().enumerate() {
                assert!(page.contains(&format!("Record {}", i + 1)), "size={size}, gap={gap}: {page}");
                assert!(!page.contains("Page "));
            }
        }
    }
}
