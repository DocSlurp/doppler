//! Watermarks are not content: pale, diagonal and repeated large words, and repeated backdrop pictures.

use doppler_md::{Options, Params, to_markdown_report_with};

fn word(x0: f64, y: f64, size: f64, text: &str, color: &str, rot: u8) -> String {
    format!(
        "<word xMin=\"{x0}\" xMax=\"{}\" yMin=\"{y}\" yMax=\"{}\" fontSize=\"{size}\" fontName=\"Serif\" baseline=\"{}\" color=\"{color}\" rotation=\"{rot}\">{text}</word>",
        x0 + 0.5 * size * text.len() as f64,
        y + size,
        y + 0.8 * size
    )
}

fn body() -> String {
    let mut out = String::new();
    for i in 0..6 {
        let y = 100.0 + 14.0 * i as f64;
        out += &format!(
            "<block><line>{}{}{}</line></block>",
            word(72.0, y, 10.0, "Ordinary", "#000000", 0),
            word(112.0, y, 10.0, "body", "#000000", 0),
            word(140.0, y, 10.0, "text.", "#000000", 0)
        );
    }
    out
}

fn page(extra: &str) -> String {
    format!("<page width=\"612\" height=\"792\" number=\"1\">{}{extra}</page>", body())
}

fn convert(xhtml: &str, watermarks: bool) -> (String, usize) {
    let mut p = Params::default();
    p.watermarks = watermarks;
    let (pages, report) = to_markdown_report_with(xhtml, &p, &Options::default());
    (pages.join("\u{c}"), report.watermark_words)
}

#[test]
fn a_large_pale_word_is_removed() {
    let xhtml = page(&format!("<block><line>{}</line></block>", word(150.0, 120.0, 90.0, "DRAFT", "#e6e6e6", 0)));
    let (md, n) = convert(&xhtml, true);
    assert!(!md.contains("DRAFT") && n == 1, "{md}");
    assert!(convert(&xhtml, false).0.contains("DRAFT"));
}

#[test]
fn white_text_on_a_banner_and_small_pale_text_are_content() {
    let banner = "<box xMin=\"60\" xMax=\"500\" yMin=\"30\" yMax=\"80\" color=\"#1a2b4c\"/>";
    let xhtml = format!(
        "<page width=\"612\" height=\"792\">{banner}{}<block><line>{}</line></block><block><line>{}</line></block></page>",
        body(),
        word(72.0, 40.0, 26.0, "Review", "#ffffff", 0),
        word(72.0, 300.0, 10.0, "footnote", "#c0c0c0", 0)
    );
    let (md, n) = convert(&xhtml, true);
    assert!(md.contains("Review") && md.contains("footnote") && n == 0, "{md}");
}

#[test]
fn large_dark_grey_text_is_content_on_its_own() {
    let xhtml = page(&format!("<block><line>{}</line></block>", word(72.0, 300.0, 30.0, "Heading", "#888888", 0)));
    let (md, n) = convert(&xhtml, true);
    assert!(md.contains("Heading") && n == 0, "{md}");
}

#[test]
fn a_diagonal_run_of_large_words_is_removed_but_a_vertical_label_is_not() {
    let diag: String = [("CON", 100.0, 500.0), ("FID", 160.0, 440.0), ("EN", 220.0, 380.0), ("TIAL", 280.0, 320.0)]
        .iter()
        .map(|(t, x, y)| format!("<block><line>{}</line></block>", word(*x, *y, 60.0, t, "#000000", 3)))
        .collect();
    let (md, n) = convert(&page(&diag), true);
    assert!(!md.contains("CON") && !md.contains("TIAL") && n == 4, "{md}");
    // the same size and rotation stacked straight up the page: a rotated label
    let label: String = (0..4)
        .map(|i| {
            format!(
                "<block><line>{}</line></block>",
                word(30.0, 500.0 - 70.0 * i as f64, 60.0, &format!("Lab{i}"), "#000000", 3)
            )
        })
        .collect();
    let (md, n) = convert(&page(&label), true);
    assert!(md.contains("Lab0") && n == 0, "{md}");
}

#[test]
fn a_large_word_at_the_same_place_on_several_pages_is_removed_but_not_on_one() {
    // dark, but drawn across the body text
    let mark = |p: &str| format!("<block><line>{}</line></block>{p}", word(150.0, 90.0, 100.0, "COPY", "#444444", 0));
    let three = page(&mark("")) + &page(&mark("")) + &page(&mark(""));
    let (md, n) = convert(&three, true);
    assert!(!md.contains("COPY") && n == 3, "{md}");
    let (md, n) = convert(&page(&mark("")), true);
    assert!(md.contains("COPY") && n == 0, "{md}");
}

fn image(x0: f64, y0: f64, x1: f64, y1: f64) -> String {
    format!("<image xMin=\"{x0}\" yMin=\"{y0}\" xMax=\"{x1}\" yMax=\"{y1}\" width=\"500\" height=\"200\"/>")
}

#[test]
fn a_backdrop_picture_repeated_on_several_pages_gets_no_marker() {
    let backdrop = image(100.0, 300.0, 500.0, 460.0);
    let xhtml = page(&backdrop) + &page(&backdrop) + &page(&(backdrop.clone() + &image(100.0, 600.0, 300.0, 700.0)));
    let o = Options { markers: true, ..Options::default() };
    let (pages, report) = to_markdown_report_with(&xhtml, &Params::default(), &o);
    assert_eq!(pages.join("").matches("doppler-image").count(), 1, "{pages:?}");
    let fates: Vec<&str> = report.images.iter().map(|n| n.fate.as_str()).collect();
    assert_eq!(fates, ["watermark", "watermark", "watermark", "block"]);
}

#[test]
fn a_scan_repeating_its_page_image_is_a_background_not_a_watermark() {
    let scan = image(0.0, 0.0, 612.0, 792.0);
    let xhtml = page(&scan) + &page(&scan) + &page(&scan);
    let o = Options { markers: true, ..Options::default() };
    let (pages, report) = to_markdown_report_with(&xhtml, &Params::default(), &o);
    assert!(!pages.join("").contains("doppler-image"), "{pages:?}");
    assert!(report.images.iter().all(|n| n.fate.as_str() == "background"), "{:?}", report.images);
}

/// A heading line of words set at `y` in `color`, `size` points.
fn heading(y: f64, size: f64, color: &str, text: &str) -> String {
    let mut x = 72.0;
    let mut words = String::new();
    for t in text.split(' ') {
        words += &word(x, y, size, t, color, 0);
        x += 0.5 * size * t.len() as f64 + 0.3 * size;
    }
    format!("<block><line>{words}</line></block>")
}

/// Body text below the heading area (from y 300), so the headings overlap nothing.
fn low_page(extra: &str) -> String {
    let mut out = String::new();
    for i in 0..6 {
        let y = 300.0 + 14.0 * i as f64;
        out += &format!(
            "<block><line>{}{}{}</line></block>",
            word(72.0, y, 10.0, "Ordinary", "#000000", 0),
            word(112.0, y, 10.0, "body", "#000000", 0),
            word(140.0, y, 10.0, "text.", "#000000", 0)
        );
    }
    format!("<page width=\"612\" height=\"792\">{extra}{out}</page>")
}

#[test]
fn headings_repeated_at_the_same_place_are_content() {
    // a short report: black headings at one spot, some repeated, some differing by a number
    let titles = ["Summary of Findings", "Summary of Findings", "Section 2 Discussion", "Section 3 Discussion"];
    let xhtml: String = titles.iter().map(|t| low_page(&heading(120.0, 20.0, "#000000", t))).collect();
    let (md, n) = convert(&xhtml, true);
    assert_eq!(md.matches("# Summary of Findings").count(), 2, "{md}");
    assert!(md.contains("# Section 2 Discussion") && md.contains("# Section 3 Discussion") && n == 0, "{md}");
    // "(continued)" headings, and a slide deck's title on every slide
    let cont: String = (0..3).map(|_| low_page(&heading(120.0, 20.0, "#1a1a1a", "Results (continued)"))).collect();
    let (md, n) = convert(&cont, true);
    assert_eq!(md.matches("Results (continued)").count(), 3, "{md}");
    assert_eq!(n, 0);
    let deck: String = (0..5).map(|_| low_page(&heading(60.0, 32.0, "#203040", "Quarterly Review"))).collect();
    let (md, n) = convert(&deck, true);
    assert_eq!(md.matches("Quarterly Review").count(), 5, "{md}");
    assert_eq!(n, 0);
}

#[test]
fn a_repeated_word_never_leaves_its_line_in_pieces() {
    // the same large word repeats, but its line has other words that do not: all stay
    let xhtml: String =
        (0..3).map(|i| low_page(&heading(120.0, 20.0, "#cccccc", &format!("Chapter {} Overview", i + 1)))).collect();
    let (md, n) = convert(&xhtml, true);
    assert!(md.contains("Chapter 1 Overview") && md.contains("Chapter 3 Overview") && n == 0, "{md}");
}

#[test]
fn light_coloured_and_lone_grey_headings_are_content() {
    for color in ["#9ec5e8", "#ffd700", "#7fd3c8", "#bbbbbb"] {
        let xhtml = low_page(&heading(120.0, 24.0, color, "Chapter Results Overview"));
        let (md, n) = convert(&xhtml, true);
        assert!(md.contains("Chapter Results Overview") && n == 0, "{color}: {md}");
    }
    // the same grey across the body text is a watermark
    let xhtml = low_page(&format!("<block><line>{}</line></block>", word(80.0, 290.0, 60.0, "DRAFT", "#bbbbbb", 0)));
    let (md, n) = convert(&xhtml, true);
    assert!(!md.contains("DRAFT") && n == 1, "{md}");
}
