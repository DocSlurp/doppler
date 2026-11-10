//! Image markers: `<image>` records from `pdftotext -images` become `![image](doppler-image:pN.M)`
//! between paragraphs or inside a line, only with `Options::markers`.

use doppler_md::{Options, Params, to_markdown_report_with};

fn word(x0: f64, x1: f64, y: f64, text: &str) -> String {
    format!(
        "<word xMin=\"{x0}\" xMax=\"{x1}\" yMin=\"{y}\" yMax=\"{}\" fontSize=\"10\" fontName=\"Serif\" baseline=\"{}\">{text}</word>",
        y + 10.0,
        y + 8.0
    )
}

fn line(y: f64, text: &str) -> String {
    let mut x = 72.0;
    let mut words = String::new();
    for w in text.split(' ') {
        let end = x + 5.0 * w.len() as f64;
        words += &word(x, end, y, w);
        x = end + 3.0;
    }
    format!("<block><line>{words}</line></block>")
}

fn image(x0: f64, y0: f64, x1: f64, y1: f64) -> String {
    format!("<image xMin=\"{x0}\" yMin=\"{y0}\" xMax=\"{x1}\" yMax=\"{y1}\" width=\"600\" height=\"300\"/>")
}

fn page(number: u32, body: &str) -> String {
    format!("<page width=\"612\" height=\"792\" number=\"{number}\">{body}</page>")
}

fn convert(xhtml: &str, markers: bool) -> (String, doppler_md::Report) {
    let o = Options { markers, ..Options::default() };
    let (pages, report) = to_markdown_report_with(xhtml, &Params::default(), &o);
    (pages.join("\u{c}"), report)
}

fn sample() -> String {
    let body = line(100.0, "A paragraph above the figure ends here.")
        + &image(72.0, 130.0, 400.0, 300.0)
        + &line(320.0, "Figure 1: the caption below it.")
        + &format!(
            "<block><line>{}{}{}{}{}{}{}{}</line></block>",
            word(72.0, 82.0, 360.0, "An"),
            word(85.0, 115.0, 360.0, "inline"),
            word(118.0, 138.0, 360.0, "icon"),
            word(141.0, 161.0, 360.0, "sits"),
            word(180.0, 190.0, 360.0, "in"),
            word(193.0, 208.0, 360.0, "the"),
            word(211.0, 241.0, 360.0, "middle"),
            word(244.0, 249.0, 360.0, ".")
        )
        + &image(165.0, 361.0, 175.0, 371.0);
    page(7, &body)
}

#[test]
fn markers_are_off_unless_asked() {
    let (md, report) = convert(&sample(), false);
    assert!(!md.contains("doppler-image"), "{md}");
    assert!(report.images.is_empty());
}

#[test]
fn a_picture_between_paragraphs_is_its_own_paragraph_in_reading_order() {
    let (md, _) = convert(&sample(), true);
    let at = |s: &str| md.find(s).unwrap_or_else(|| panic!("{s:?} missing from {md}"));
    assert!(at("A paragraph above") < at("![image](doppler-image:p7.1)"), "{md}");
    assert!(at("![image](doppler-image:p7.1)") < at("Figure 1: the caption"), "{md}");
    assert!(md.contains("\n\n![image](doppler-image:p7.1)\n\n"), "{md}");
}

#[test]
fn a_text_sized_picture_sits_inside_its_line() {
    let (md, report) = convert(&sample(), true);
    assert!(md.contains("An inline icon sits ![image](doppler-image:p7.2) in the middle ."), "{md}");
    let fates: Vec<&str> = report.images.iter().map(|n| n.fate.as_str()).collect();
    assert_eq!(fates, ["block", "inline"]);
    assert_eq!(report.images[0].id, "p7.1");
    assert_eq!(report.images[0].page, 7);
    assert!(report.to_json().contains("\"images\":[{\"id\":\"p7.1\",\"page\":7,"), "{}", report.to_json());
}

#[test]
fn page_backgrounds_slivers_and_header_logos_get_no_marker() {
    let body = line(100.0, "Scanned text over its own image here.")
        + &image(0.0, 0.0, 612.0, 792.0)
        + &image(10.0, 300.0, 13.0, 303.0)
        + &image(100.0, 5.0, 200.0, 40.0)
        + &image(72.0, 500.0, 300.0, 600.0)
        + &image(72.5, 500.5, 300.0, 600.0);
    let (md, report) = convert(&page(2, &body), true);
    assert_eq!(md.matches("doppler-image").count(), 1, "{md}");
    let fates: Vec<&str> = report.images.iter().map(|n| n.fate.as_str()).collect();
    assert_eq!(fates, ["background", "small", "furniture", "block", "duplicate"]);
}

#[test]
fn ids_use_the_page_index_without_a_page_number() {
    let xhtml = "<page width=\"612\" height=\"792\">".to_string() + &line(100.0, "Only text here.") + &image(72.0, 200.0, 300.0, 300.0) + "</page>";
    let (md, _) = convert(&xhtml, true);
    assert!(md.contains("doppler-image:p1.1"), "{md}");
}
