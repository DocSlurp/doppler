//! A numbered section heading right after a table opens the next section; it is not the table's caption.

use doppler_md::{Params, to_markdown};

fn word(x0: f64, y: f64, text: &str, font: &str) -> String {
    format!(
        "<word xMin=\"{x0}\" xMax=\"{}\" yMin=\"{y}\" yMax=\"{}\" fontSize=\"10\" fontName=\"{font}\" baseline=\"{}\">{text}</word>",
        x0 + 5.5 * text.chars().count() as f64,
        y + 10.0,
        y + 8.0
    )
}

fn line(y: f64, text: &str, font: &str) -> String {
    let mut x = 72.0;
    let mut out = String::new();
    for w in text.split(' ') {
        out += &word(x, y, w, font);
        x += 5.5 * w.chars().count() as f64 + 3.0;
    }
    format!("<block><line>{out}</line></block>")
}

fn page(heading: &str) -> String {
    let mut body = String::new();
    for (i, (a, b, c)) in [
        ("Item", "Mass", "Volume"),
        ("Head", "4.5", "3.9"),
        ("Torso", "30.1", "28.0"),
        ("Arm", "3.6", "3.1"),
        ("Leg", "10.2", "9.8"),
    ]
    .iter()
    .enumerate()
    {
        let y = 100.0 + 16.0 * i as f64;
        body += &format!("<block><line>{}</line></block>", word(72.0, y, a, "Serif"));
        body += &format!("<block><line>{}</line></block>", word(230.0, y, b, "Serif"));
        body += &format!("<block><line>{}</line></block>", word(380.0, y, c, "Serif"));
    }
    body += &line(200.0, heading, "Serif-Bold");
    for i in 0..3 {
        body += &line(
            216.0 + 12.0 * i as f64,
            "The body text of the section runs on over several lines of ordinary prose here",
            "Serif",
        );
    }
    format!("<page width=\"612\" height=\"792\">{body}</page>")
}

fn convert(xhtml: &str, on: bool) -> String {
    let mut p = Params::default();
    p.section_headings_after_tables = on;
    to_markdown(xhtml, &p).join("")
}

#[test]
fn a_numbered_section_heading_after_a_table_stays_a_heading() {
    let xhtml = page("4.6.3.2 Application of Body Volume Data");
    let md = convert(&xhtml, true);
    assert!(md.contains("### 4.6.3.2 Application of Body Volume Data"), "{md}");
    assert!(convert(&xhtml, false).contains("*4.6.3.2 Application of Body Volume Data*"));
}

#[test]
fn a_caption_after_a_table_stays_a_caption() {
    for text in ["Table 4.6-2 Whole-Body Volume of Crewmembers", "1. Source of the data"] {
        let md = convert(&page(text), true);
        assert!(md.contains(&format!("*{text}*")), "{text}: {md}");
    }
}
