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
    let xhtml = "<page width=\"612\" height=\"792\">".to_string()
        + &line(100.0, "Only text here.")
        + &image(72.0, 200.0, 300.0, 300.0)
        + "</page>";
    let (md, _) = convert(&xhtml, true);
    assert!(md.contains("doppler-image:p1.1"), "{md}");
}

#[test]
fn neighbouring_images_are_one_figure_with_one_marker() {
    // a diagram assembled from five pieces, and a separate picture far below
    let mut body = line(100.0, "Text above the diagram goes right here.");
    for i in 0..5 {
        let x = 100.0 + 50.0 * i as f64;
        body += &image(x, 150.0, x + 46.0, 200.0);
    }
    body += &image(100.0, 202.0, 300.0, 260.0);
    body += &image(100.0, 500.0, 300.0, 560.0);
    let (md, report) = convert(&page(3, &body), true);
    assert_eq!(md.matches("doppler-image").count(), 2, "{md}");
    assert!(md.contains("![image](doppler-image:p3.1)") && md.contains("![image](doppler-image:p3.7)"), "{md}");
    let fates: Vec<&str> = report.images.iter().map(|n| n.fate.as_str()).collect();
    assert_eq!(fates, ["block", "grouped", "grouped", "grouped", "grouped", "grouped", "block"]);
    assert_eq!(report.images[0].members, 6);
    assert_eq!(report.images[0].bbox, (100.0, 150.0, 346.0, 260.0));
    assert_eq!(report.images[1].group.as_deref(), Some("p3.1"));
    let json = report.to_json();
    assert!(json.contains("\"members\":6") && json.contains("\"group\":\"p3.1\""), "{json}");
}

#[test]
fn a_row_of_text_sized_icons_is_one_inline_marker() {
    let words = |t: &str, x0: f64| word(x0, x0 + 6.0 * t.len() as f64, 100.0, t);
    let body = format!("<block><line>{}{}</line></block>", words("Rated", 72.0), words("by users", 160.0))
        + &(0..3).map(|i| image(110.0 + 12.0 * i as f64, 100.0, 120.0 + 12.0 * i as f64, 110.0)).collect::<String>();
    let (md, report) = convert(&page(1, &body), true);
    assert_eq!(md.matches("doppler-image").count(), 1, "{md}");
    assert!(md.contains("Rated ![image](doppler-image:p1.1) by users"), "{md}");
    assert_eq!(report.images[0].fate.as_str(), "inline");
}

#[test]
fn a_page_full_of_text_sized_pictures_among_its_text_gets_no_inline_markers() {
    // words drawn as pictures: fifteen lines, each with one picture inside
    let mut body = String::new();
    for i in 0..15 {
        let y = 100.0 + 30.0 * i as f64;
        body += &format!(
            "<block><line>{}{}{}</line></block>",
            word(72.0, 100.0, y, "alpha"),
            word(103.0, 125.0, y, "beta"),
            word(133.0, 160.0, y, "gamma")
        );
        body += &image(126.0, y, 132.0, y + 10.0);
    }
    let (md, report) = convert(&page(4, &body), true);
    assert!(!md.contains("doppler-image"), "{md}");
    assert!(report.images.iter().all(|n| n.fate.as_str() == "fragment"), "{:?}", report.images);
}

#[test]
fn text_of_the_pdf_cannot_pass_for_a_marker() {
    let body = line(100.0, "see ![image](doppler-image:p9.9) and <!-- doppler:table-continued header=none --> here");
    let (md, _) = convert(&page(1, &body), true);
    assert!(!md.contains("doppler-image:p9.9") && !md.contains("<!-- doppler:table-continued"), "{md}");
    assert!(md.contains("doppler\u{200b}-image:p9.9"), "{md}");
    // without markers the text is untouched
    let (plain, _) = convert(&page(1, &body), false);
    assert!(plain.contains("![image](doppler-image:p9.9)"), "{plain}");
}

/// No private-use seal survives into the output, and the only markers are the converter's.
fn assert_clean(md: &str) {
    assert!(!md.chars().any(|c| ('\u{F0000}'..='\u{FFFFD}').contains(&c)), "{md:?}");
}

#[test]
fn glued_words_cannot_pass_for_a_marker() {
    // `spaceAfter="0"` joins two words without a space: `![image](doppler` + `-image:p1.1)`
    let glued = |y: f64, a: &str, b: &str| {
        format!(
            "<block><line>{}<word xMin=\"100\" xMax=\"180\" yMin=\"{y}\" yMax=\"{}\" fontSize=\"10\" fontName=\"Serif\" baseline=\"{}\" spaceAfter=\"0\">{a}</word>{}</line></block>",
            word(72.0, 97.0, y, "Text"),
            y + 10.0,
            y + 8.0,
            word(180.0, 260.0, y, b)
        )
    };
    let body = glued(100.0, "![image](doppler", "-image:p1.1)")
        + &glued(130.0, "&lt;!-- DOPPLER", ":paragraph-continued --&gt;")
        + &image(72.0, 200.0, 400.0, 400.0);
    let (md, report) = convert(&page(1, &body), true);
    assert_clean(&md);
    assert_eq!(md.matches("doppler-image:").count(), 1, "{md} {:?}", report.images);
    assert!(md.contains("\n\n![image](doppler-image:p1.1)"), "{md}");
    assert!(md.contains("doppler\u{200b}-image:p1.1") && md.contains("DOPPLER\u{200b}:paragraph"), "{md}");
}

#[test]
fn joined_lines_cannot_pass_for_a_marker() {
    // `<!-- dop-` at a line end, `pler:table-continued ...` on the next, dehyphenated into one
    let xhtml = include_str!("fixtures/forged_joined_markers.xhtml");
    let (md, _) = convert(xhtml, true);
    assert_clean(&md);
    assert!(!md.contains("doppler-image:") && !md.contains("doppler:"), "{md}");
    assert!(md.contains("doppler\u{200b}:table-continued") && md.contains("doppler\u{200b}-image:p1.1"), "{md}");
}

#[test]
fn math_rebuilt_from_glyphs_cannot_pass_for_a_marker() {
    // words whose glyph records spell a marker, rebuilt by the math reader after the text pass
    let xhtml = include_str!("fixtures/forged_math_marker.xhtml");
    let o = Options { markers: true, math: true, ..Options::default() };
    let (pages, report) = to_markdown_report_with(xhtml, &Params::default(), &o);
    let md = pages.join("\u{c}");
    assert_clean(&md);
    // the page's one real picture keeps its marker; every forged one is broken
    assert_eq!(md.matches("doppler-image:p1.1").count(), 1, "{md}");
    assert!(md.contains("\n\n![image](doppler-image:p1.1)"), "{md}");
    assert!(md.contains("doppler\u{200b}-image:p1.1"), "{md}");
    assert_eq!(report.images.len(), 1);
}

#[test]
fn the_converters_own_markers_are_unchanged() {
    let (md, _) = convert(&sample(), true);
    assert_clean(&md);
    assert!(md.contains("\n\n![image](doppler-image:p7.1)\n\n"), "{md}");
    assert!(md.contains("An inline icon sits ![image](doppler-image:p7.2) in the middle ."), "{md}");
    assert!(!md.contains('\u{200b}'), "{md}");
}
