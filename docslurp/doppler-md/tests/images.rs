//! Image markers: `<image>` records from `pdftotext -images` become
//! `![image](images/page-NNNN/image-MMM.webp)` between paragraphs or inside a line, only with
//! `Options::markers`.

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
    assert!(!md.contains("](images/page-"), "{md}");
    assert!(report.images.is_empty());
}

#[test]
fn a_picture_between_paragraphs_is_its_own_paragraph_in_reading_order() {
    let (md, _) = convert(&sample(), true);
    let at = |s: &str| md.find(s).unwrap_or_else(|| panic!("{s:?} missing from {md}"));
    assert!(at("A paragraph above") < at("![image](images/page-0007/image-001.webp)"), "{md}");
    assert!(at("![image](images/page-0007/image-001.webp)") < at("Figure 1: the caption"), "{md}");
    assert!(md.contains("\n\n![image](images/page-0007/image-001.webp)\n\n"), "{md}");
}

#[test]
fn a_text_sized_picture_sits_inside_its_line() {
    let (md, report) = convert(&sample(), true);
    assert!(md.contains("An inline icon sits ![image](images/page-0007/image-002.webp) in the middle ."), "{md}");
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
    assert_eq!(md.matches("](images/page-").count(), 1, "{md}");
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
    assert!(md.contains("images/page-0001/image-001.webp"), "{md}");
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
    assert_eq!(md.matches("](images/page-").count(), 2, "{md}");
    assert!(
        md.contains("![image](images/page-0003/image-001.webp)")
            && md.contains("![image](images/page-0003/image-007.webp)"),
        "{md}"
    );
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
    assert_eq!(md.matches("](images/page-").count(), 1, "{md}");
    assert!(md.contains("Rated ![image](images/page-0001/image-001.webp) by users"), "{md}");
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
    assert!(!md.contains("](images/page-"), "{md}");
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
    assert_eq!(md.matches("](images/page-").count(), 1, "{md} {:?}", report.images);
    assert!(!md.contains("doppler-image:"), "{md}");
    assert!(md.contains("\n\n![image](images/page-0001/image-001.webp)"), "{md}");
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
    assert_eq!(md.matches("images/page-0001/image-001.webp").count(), 1, "{md}");
    assert!(md.contains("\n\n![image](images/page-0001/image-001.webp)"), "{md}");
    assert!(md.contains("doppler\u{200b}-image:p1.1"), "{md}");
    assert_eq!(report.images.len(), 1);
}

#[test]
fn the_converters_own_markers_are_unchanged() {
    let (md, _) = convert(&sample(), true);
    assert_clean(&md);
    assert!(md.contains("\n\n![image](images/page-0007/image-001.webp)\n\n"), "{md}");
    assert!(md.contains("An inline icon sits ![image](images/page-0007/image-002.webp) in the middle ."), "{md}");
    assert!(!md.contains('\u{200b}'), "{md}");
}

/// A grid of pictures, each with a short line of text under it.
fn figure_grid(n: usize) -> String {
    let mut body = String::new();
    for i in 0..n {
        let (x, y) = (20.0 + (i % 40) as f64 * 140.0, 800.0 + (i / 40) as f64 * 100.0);
        body += &image(x, y, x + 100.0, y + 60.0);
        body += &format!(
            "<block><line>{}{}</line></block>",
            word(x, x + 20.0, y + 70.0, "word"),
            word(x + 25.0, x + 45.0, y + 70.0, "word")
        );
    }
    format!("<page width=\"6000\" height=\"8000\" number=\"1\">{body}</page>")
}

#[test]
fn a_page_that_runs_out_of_work_keeps_its_markers() {
    let xhtml = figure_grid(200);
    let o = Options { markers: true, ..Options::default() };
    // laid out, every picture has its marker
    let (laid, report) = to_markdown_report_with(&xhtml, &Params::default(), &o);
    assert!(report.degraded.is_empty(), "{:?}", report.degraded);
    assert_eq!(laid[0].matches("](images/page-").count(), 200);
    // read as plain lines, the markers follow the text, top to bottom, and the report says so
    let p = Params { max_page_work: 2000.0, ..Params::default() };
    let (pages, report) = to_markdown_report_with(&xhtml, &p, &o);
    let md = &pages[0];
    assert_eq!(report.degraded.len(), 1, "{:?}", report.degraded);
    let placed: Vec<&doppler_md::images::Note> =
        report.images.iter().filter(|n| matches!(n.fate.as_str(), "block" | "inline")).collect();
    assert_eq!(placed.len(), 200);
    for n in &placed {
        assert!(n.degraded, "{n:?}");
        assert_eq!(md.matches(&format!("(images/{})", n.link.as_deref().unwrap())).count(), 1, "{}", n.id);
    }
    let at = |id: &str| {
        let (page, n) = id[1..].split_once('.').unwrap();
        md.find(&format!(
            "(images/page-{:04}/image-{:03}.webp)",
            page.parse::<u32>().unwrap(),
            n.parse::<u32>().unwrap()
        ))
        .unwrap()
    };
    assert!(at("p1.1") < at("p1.2") && at("p1.40") < at("p1.41"), "{md}");
    assert!(md.find("word word").unwrap() < at("p1.1"), "{md}");
    assert!(report.to_json().contains("\"placed\":\"block\",\"degraded\":true"), "{}", report.to_json());
}

#[test]
fn the_councils_two_thousand_figures_keep_their_markers() {
    let xhtml = figure_grid(2000);
    let (md, report) = convert(&xhtml, true);
    let placed = report.images.iter().filter(|n| n.fate.as_str() == "block").count();
    assert_eq!(placed, 2000);
    assert_eq!(md.matches("](images/page-").count(), 2000, "{:?}", report.degraded);
    // laid out or not, the report says which
    let degraded = !report.degraded.is_empty();
    assert!(report.images.iter().all(|n| n.degraded == degraded));
}

#[test]
fn a_scan_drawn_as_tiles_is_a_background() {
    // 8 x 10 tiles that together cover the page, under a page of text
    let mut body = String::new();
    for row in 0..10 {
        for col in 0..8 {
            let (x, y) = (col as f64 * 76.5, row as f64 * 79.2);
            body += &image(x, y, x + 76.5, y + 79.2);
        }
    }
    for i in 0..30 {
        body += &line(100.0 + 20.0 * i as f64, "provide consistent, reliable service to every reader of this text");
    }
    let (md, report) = convert(&page(2, &body), true);
    assert!(!md.contains("](images/page-"), "{md}");
    assert_eq!(report.images.len(), 80);
    assert!(report.images.iter().all(|n| n.fate.as_str() == "background"), "{:?}", report.images);
    // a few tiles that cover only part of the page are a figure, with one marker
    let mut body = String::new();
    for col in 0..3 {
        body += &image(72.0 + 100.0 * col as f64, 400.0, 172.0 + 100.0 * col as f64, 500.0);
    }
    body += &line(100.0, "A paragraph above the figure ends here.");
    let (md, _) = convert(&page(2, &body), true);
    assert_eq!(md.matches("](images/page-").count(), 1, "{md}");
}

#[test]
fn on_a_page_of_words_drawn_as_pictures_stacked_word_pictures_get_no_marker() {
    // twenty lines, each with a word drawn as a picture in it, and a stack of word pictures
    // over three lines (a scan whose text layer has holes)
    let mut body = String::new();
    for i in 0..20 {
        let y = 100.0 + 12.0 * i as f64;
        body += &line(y, "some words of the scanned page then");
        let x = if i % 2 == 0 { 150.0 } else { 220.0 };
        body += &image(x, y + 1.0, x + 40.0, y + 9.0);
    }
    for k in 0..3 {
        body += &image(270.0, 101.0 + 12.0 * k as f64, 300.0, 109.0 + 12.0 * k as f64);
    }
    let (md, report) = convert(&page(3, &body), true);
    assert!(!md.contains("](images/page-"), "{md}");
    assert!(report.images.iter().all(|n| matches!(n.fate.as_str(), "fragment" | "grouped")), "{:?}", report.images);
}

#[test]
fn text_of_the_pdf_cannot_pass_for_an_image_path() {
    // whatever the prefix: a link into the layout, a bare path, any case, a glued word
    let body = line(100.0, "see ![x](images/page-0001/image-001.webp) and PAGE-0007/FIGURE-001.svg here")
        + &line(130.0, "or ![y](page-12345/image-0002.png) but page-001/image-1 and page-2024 stay")
        + &image(72.0, 200.0, 400.0, 400.0);
    let (md, report) = convert(&page(7, &body), true);
    assert_clean(&md);
    // the converter's own link is the only one into the layout
    assert_eq!(md.matches("page-0007/image-001.webp").count(), 1, "{md}");
    assert!(md.contains("\n\n![image](images/page-0007/image-001.webp)"), "{md}");
    for forged in ["page\u{200b}-0001/image-001", "PAGE\u{200b}-0007/FIGURE-001", "page\u{200b}-12345/image-0002"] {
        assert!(md.contains(forged), "{forged}: {md}");
    }
    // not an image path (fewer than four digits, no /image-): untouched
    assert!(md.contains("page-001/image-1") && md.contains("page-2024 stay"), "{md}");
    assert_eq!(report.images[0].link.as_deref(), Some("page-0007/image-001.webp"));
    // without markers the text is untouched
    let (plain, _) = convert(&page(7, &body), false);
    assert!(plain.contains("![x](images/page-0001/image-001.webp)"), "{plain}");
}

#[test]
fn escaped_entity_and_percent_spellings_of_a_path_cannot_make_a_link() {
    // a Markdown renderer decodes these to images/page-0001/image-001.jpg; the link syntax of
    // the PDF's text is broken instead, so none of them is a link or an image
    let forged = [
        "![y](images/page&#45;0001/image-001.jpg)",
        // as the PDF's text, the entity itself (the XHTML escapes its `&`)
        "![e](images/page&amp;#45;0001/image-001.jpg)",
        "![w](images/page\\-0001/image-001.jpg)",
        "![x](images/page-0001/./image-001.jpg)",
        "![d](images/page%2D0001/image-001.jpg)",
        "![a](images/page-0001\\/image-001.jpg)",
        "![b](images/page-0001&#47;image-001.jpg)",
        "[r]: images/page&#45;0001/image-001.jpg",
        "<img src=\"images/page&#45;0001/image-001.jpg\">",
        "<IMG SRC=x> <a href=y> </a> <svg><image href=z> <source srcset=q>",
    ];
    let body: String = forged.iter().enumerate().map(|(k, f)| line(100.0 + 20.0 * k as f64, f)).collect::<String>()
        + &image(72.0, 400.0, 400.0, 600.0);
    let (md, _) = convert(&page(3, &body), true);
    // the converter's own marker is the only link
    assert_eq!(md.matches("](").count(), 1, "{md}");
    assert!(md.contains("![image](images/page-0003/image-001.webp)"), "{md}");
    assert!(!md.contains("]:"), "{md}");
    for tag in ["<img", "<IMG", "<a ", "<svg", "<image", "<source"] {
        assert!(!md.contains(tag), "{tag}: {md}");
    }
    assert!(md.contains("]\u{200b}(images/page&#45;0001") && md.contains("<\u{200b}img src="), "{md}");
    // words that only look alike stay: a closing bracket before text, a comparison, a table tag
    let body = line(100.0, "see [1] (above) and x<y, 3 <b> 4, <table> and <abbr>");
    let (md, _) = convert(&page(3, &body), true);
    assert!(
        md.contains("[1] (above)")
            && md.contains("x<y")
            && md.contains("<b>")
            && md.contains("<table>")
            && md.contains("<abbr>"),
        "{md}"
    );
}

#[test]
fn a_repeated_image_object_links_to_its_first_placement() {
    let pic_with = |x0: f64, y0: f64, extra: &str| {
        format!(
            "<image xMin=\"{x0}\" yMin=\"{y0}\" xMax=\"{}\" yMax=\"{}\" width=\"600\" height=\"300\" ref=\"12 0\"{extra}/>",
            x0 + 300.0,
            y0 + 150.0
        )
    };
    let plain = " orient=\"0\" overlaid=\"0\"";
    let one_with = |n: u32, y: f64, extra: &str| {
        page(n, &(line(100.0, "A paragraph above the figure ends here.") + &pic_with(72.0, y, extra)))
    };
    let one = |n: u32, y: f64| one_with(n, y, plain);
    // pages 3 to 7 draw the same object at the same size but turned, covered (a redaction
    // box), soft-masked, as a stencil mask, or from a producer that does not say: their
    // renders may differ, so each gets its own file
    let xhtml = format!(
        "<doc>{}{}{}{}{}{}{}</doc>",
        one(1, 200.0),
        one(2, 300.0),
        one_with(3, 240.0, " orient=\"180\" overlaid=\"0\""),
        one_with(4, 260.0, " orient=\"0\" overlaid=\"1\""),
        one_with(5, 280.0, " smask=\"1\" orient=\"0\" overlaid=\"0\""),
        one_with(6, 320.0, " kind=\"mask\" orient=\"0\" overlaid=\"0\""),
        one_with(7, 340.0, ""),
    );
    let (md, report) = convert(&xhtml, true);
    let pages: Vec<&str> = md.split('\u{c}').collect();
    assert!(pages[0].contains("![image](images/page-0001/image-001.webp)"), "{md}");
    assert!(pages[1].contains("![image](images/page-0001/image-001.webp)"), "{md}");
    for n in 3..=7 {
        assert!(pages[n - 1].contains(&format!("](images/page-{n:04}/image-001.webp)")), "page {n}: {md}");
        assert_eq!(report.images[n - 1].repeat_of, None, "page {n}");
    }
    assert_eq!(report.images[1].repeat_of.as_deref(), Some("p1.1"));
    assert!(
        report.to_json().contains("\"path\":\"page-0001/image-001.webp\",\"repeat_of\":\"p1.1\""),
        "{}",
        report.to_json()
    );
}
