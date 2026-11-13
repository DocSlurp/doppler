//! Vector figure markers: `<graphic>` clusters from `pdftotext -images` (drawing that is not
//! axis-aligned) become `![image](doppler-image:pN.gK)` with the figure's labels after it, only
//! with `Options::markers`. Tables, shading, underlines, frames, hatching, leaders and drawing
//! among prose get no marker.

use doppler_md::{Options, Params, to_markdown_report_with};

fn word(x0: f64, x1: f64, y: f64, text: &str) -> String {
    format!(
        "<word xMin=\"{x0}\" xMax=\"{x1}\" yMin=\"{y}\" yMax=\"{}\" fontSize=\"10\" fontName=\"Serif\" baseline=\"{}\">{text}</word>",
        y + 10.0,
        y + 8.0
    )
}

/// One line of words starting at `x`.
fn line_at(x: f64, y: f64, text: &str) -> String {
    let mut x = x;
    let mut words = String::new();
    for w in text.split(' ') {
        let end = x + 5.0 * w.len() as f64;
        words += &word(x, end, y, w);
        x = end + 3.0;
    }
    format!("<block><line>{words}</line></block>")
}

fn line(y: f64, text: &str) -> String {
    line_at(72.0, y, text)
}

#[allow(clippy::too_many_arguments)]
fn graphic(x0: f64, y0: f64, x1: f64, y1: f64, paths: u32, segments: u32, curves: u32, directions: u32) -> String {
    format!(
        "<graphic xMin=\"{x0}\" yMin=\"{y0}\" xMax=\"{x1}\" yMax=\"{y1}\" paths=\"{paths}\" segments=\"{segments}\" curves=\"{curves}\" directions=\"{directions}\"/>"
    )
}

fn rule(x0: f64, y0: f64, x1: f64, y1: f64) -> String {
    format!("<rule xMin=\"{x0}\" yMin=\"{y0}\" xMax=\"{x1}\" yMax=\"{y1}\" color=\"#000000\"/>")
}

fn page(number: u32, body: &str) -> String {
    format!("<page width=\"612\" height=\"792\" number=\"{number}\">{body}</page>")
}

fn convert(xhtml: &str, markers: bool) -> (String, doppler_md::Report) {
    let o = Options { markers, ..Options::default() };
    let (pages, report) = to_markdown_report_with(xhtml, &Params::default(), &o);
    (pages.join("\u{c}"), report)
}

fn vector_fates(report: &doppler_md::Report) -> Vec<(String, &'static str)> {
    report.images.iter().filter(|n| n.vector).map(|n| (n.id.clone(), n.fate.as_str())).collect()
}

/// A line chart: two data lines (one graphic each), axes as rules, tick labels and an axis
/// title around it, a paragraph above and a caption below.
fn chart() -> String {
    page(3, &chart_body())
}

fn chart_body() -> String {
    line(100.0, "A paragraph above the chart ends with a full stop here.")
        // the plot area 100..400 x 150..330, its data lines and axes
        + &graphic(102.0, 160.0, 398.0, 320.0, 1, 180, 0, 6)
        + &graphic(102.0, 170.0, 398.0, 325.0, 1, 150, 0, 5)
        + &rule(100.0, 330.0, 400.0, 330.5)
        + &rule(100.0, 150.0, 100.5, 330.0)
        // tick labels just outside the axes, and the axis title below them
        + &line_at(85.0, 145.0, "20")
        + &line_at(85.0, 320.0, "0")
        + &line_at(100.0, 334.0, "0")
        + &line_at(390.0, 334.0, "6")
        + &line_at(220.0, 346.0, "iter. (1e4)")
        // a label drawn inside the plot
        + &line_at(300.0, 200.0, "56-layer")
        + &line(380.0, "Figure 1: Training error of the two networks over the iterations of training.")
}

#[test]
fn graphics_are_ignored_without_markers() {
    let (with, report) = convert(&chart(), false);
    assert!(!with.contains("](images/page-"), "{with}");
    assert!(report.images.is_empty());
    // the same page without its graphic records converts the same
    let plain: String = chart()
        .split("<graphic")
        .enumerate()
        .map(|(i, part)| if i == 0 { part.to_string() } else { part.split_once("/>").unwrap().1.to_string() })
        .collect();
    assert!(!plain.contains("<graphic"));
    assert_eq!(convert(&plain, false).0, with);
}

#[test]
fn a_chart_gets_one_marker_with_its_labels_after_it() {
    let (md, report) = convert(&chart(), true);
    let at = |s: &str| md.find(s).unwrap_or_else(|| panic!("{s:?} missing from {md}"));
    assert_eq!(md.matches("](images/page-").count(), 1, "{md}");
    assert!(md.contains("\n\n![image](images/page-0003/figure-001.svg)\n\n"), "{md}");
    assert!(at("A paragraph above") < at("![image](images/page-0003/figure-001.svg)"), "{md}");
    // labels follow the marker, once, top to bottom; the caption stays outside the figure
    let labels =
        at("![image](images/page-0003/figure-001.svg)") + "![image](images/page-0003/figure-001.svg)\n\n".len();
    assert!(md[labels..].starts_with("20 56-layer 0 0 6 iter. (1e4)"), "{md}");
    assert_eq!(md.matches("56-layer").count(), 1, "{md}");
    assert!(at("iter. (1e4)") < at("Figure 1: Training error"), "{md}");
    // the two data lines touch: one drawing of two graphics
    assert_eq!(vector_fates(&report), [("p3.g1".to_string(), "block")]);
    let note = &report.images[0];
    assert_eq!(note.members, 2);
    assert_eq!(note.group, None);
    // the figure's box takes in the axes and the labels around them, not the caption
    let (x0, y0, x1, y1) = note.bbox;
    assert!(x0 <= 85.0 && y0 <= 145.0 && x1 >= 400.0 && (356.0..380.0).contains(&y1), "{:?}", note.bbox);
    let json = report.to_json();
    assert!(json.contains("{\"id\":\"p3.g1\",\"page\":3,"), "{json}");
    assert!(json.contains("\"kind\":\"vector\",\"placed\":\"block\",\"members\":2"), "{json}");
}

#[test]
fn a_ruled_table_with_shaded_rows_is_no_figure() {
    // only rules and boxes: pdftotext reports no graphic for axis-aligned drawing
    let mut body = line(100.0, "A table follows.");
    for k in 0..5 {
        let y = 130.0 + 20.0 * k as f64;
        body += &rule(72.0, y, 400.0, y + 0.5);
        body += &line_at(80.0, y + 4.0, &format!("row{k}"));
        body += &line_at(250.0, y + 4.0, &format!("{k}.5"));
    }
    body += "<box xMin=\"72\" yMin=\"150\" xMax=\"400\" yMax=\"170\" color=\"#dddddd\"/>";
    let (md, report) = convert(&page(1, &body), true);
    assert!(!md.contains("](images/page-"), "{md}");
    assert!(vector_fates(&report).is_empty());
}

#[test]
fn hatching_leaders_braces_and_small_marks_get_no_marker() {
    let body = line(100.0, "Some text on a form page with a hatched box and a dotted leader.")
        // hatching: many parallel diagonal strokes (one direction, no curves)
        + &graphic(300.0, 200.0, 400.0, 260.0, 40, 40, 0, 1)
        // a dotted leader: many round dots in a row, thin
        + &graphic(100.0, 300.0, 400.0, 302.0, 60, 240, 240, 0)
        // a brace: tall and thin, one path
        + &graphic(450.0, 400.0, 456.0, 460.0, 1, 30, 30, 2)
        // a check mark: rich but tiny
        + &graphic(100.0, 500.0, 108.0, 508.0, 2, 30, 0, 4);
    let (md, report) = convert(&page(2, &body), true);
    assert!(!md.contains("](images/page-"), "{md}");
    let fates: Vec<&str> = vector_fates(&report).iter().map(|f| f.1).collect();
    assert_eq!(fates, ["small", "small", "small", "small"]);
}

#[test]
fn drawing_among_prose_and_in_the_header_gets_no_marker() {
    let mut body = String::new();
    // a vector logo in the header band
    body += &graphic(72.0, 20.0, 130.0, 60.0, 30, 400, 120, 8);
    // a decorated panel of prose: rich drawing behind several long lines
    body += &graphic(72.0, 300.0, 500.0, 420.0, 20, 200, 80, 6);
    for k in 0..5 {
        body += &line(310.0 + 20.0 * k as f64, "These are long lines of running prose that sit in the panel area.");
    }
    let (md, report) = convert(&page(4, &body), true);
    assert!(!md.contains("](images/page-"), "{md}");
    let fates: Vec<&str> = vector_fates(&report).iter().map(|f| f.1).collect();
    assert_eq!(fates, ["furniture", "text"]);
    assert!(md.contains("These are long lines"), "{md}");
}

#[test]
fn pictures_inside_a_diagram_are_marked_with_it() {
    let body = line(100.0, "A diagram with two small bitmaps in it follows.")
        + "<image xMin=\"150\" yMin=\"170\" xMax=\"190\" yMax=\"200\" width=\"80\" height=\"60\"/>"
        + &graphic(100.0, 150.0, 450.0, 350.0, 30, 120, 40, 5)
        + "<image xMin=\"300\" yMin=\"250\" xMax=\"340\" yMax=\"280\" width=\"80\" height=\"60\"/>";
    let (md, report) = convert(&page(5, &body), true);
    assert_eq!(md.matches("](images/page-").count(), 1, "{md}");
    assert!(md.contains("![image](images/page-0005/figure-001.svg)"), "{md}");
    let fates: Vec<(String, &str)> = report.images.iter().map(|n| (n.id.clone(), n.fate.as_str())).collect::<Vec<_>>();
    assert!(fates.contains(&("p5.g1".to_string(), "block")), "{fates:?}");
    for n in report.images.iter().filter(|n| !n.vector) {
        assert_eq!(n.fate.as_str(), "grouped", "{fates:?}");
        assert_eq!(n.group.as_deref(), Some("p5.g1"), "{fates:?}");
    }
}

#[test]
fn drawing_over_a_larger_picture_is_part_of_the_picture() {
    let body = line(100.0, "A bitmap plot with vector axes drawn over it follows.")
        + "<image xMin=\"100\" yMin=\"150\" xMax=\"450\" yMax=\"400\" width=\"700\" height=\"500\"/>"
        + &graphic(120.0, 170.0, 300.0, 300.0, 10, 100, 30, 5);
    let (md, report) = convert(&page(6, &body), true);
    assert_eq!(md.matches("](images/page-").count(), 1, "{md}");
    assert!(md.contains("![image](images/page-0006/image-001.webp)"), "{md}");
    let v: Vec<_> = report.images.iter().filter(|n| n.vector).collect();
    assert_eq!(v.len(), 1);
    assert_eq!(v[0].fate.as_str(), "grouped");
    assert_eq!(v[0].group.as_deref(), Some("p6.1"));
}

#[test]
fn malformed_and_hostile_graphics_are_skipped_without_panic() {
    let mut body = line(100.0, "Text.");
    body += "<graphic xMin=\"400\" yMin=\"100\" xMax=\"100\" yMax=\"300\" paths=\"5\" segments=\"500\" curves=\"50\" directions=\"8\"/>";
    body += "<graphic xMin=\"nan\" yMin=\"100\" xMax=\"200\" yMax=\"300\" paths=\"5\" segments=\"500\"/>";
    body += "<graphic xMin=\"100\" yMin=\"100\" xMax=\"200\" yMax=\"300\" paths=\"-3\" segments=\"99999999999999999999999\" curves=\"x\" directions=\"4000000000\"/>";
    body += "<graphic xMin=\"100\" yMin=\"100\"";
    // many graphics on one page stay bounded
    for k in 0..3000 {
        let x = (k % 60) as f64 * 9.0;
        let y = 120.0 + (k / 60) as f64 * 12.0;
        body += &graphic(x, y, x + 4.0, y + 4.0, 1, 8, 4, 3);
    }
    let (_, report) = convert(&page(7, &body), true);
    assert!(report.parse.graphics_skipped >= 2, "{}", report.parse.graphics_skipped);
    assert!(report.images.iter().filter(|n| n.vector).count() <= 1024);
}

#[test]
fn text_cannot_forge_a_vector_marker() {
    let body = line(60.0, "see ![image](doppler-image:p8.g1) here") + &chart_body();
    let (md, report) = convert(&page(8, &body), true);
    assert!(!md.contains("see ![image](images/page-0008/figure-001.svg)"), "{md}");
    assert_eq!(md.matches("![image](images/page-0008/figure-001.svg)").count(), 1, "{md}");
    assert_eq!(vector_fates(&report), [("p8.g1".to_string(), "block")]);
}

/// The council's page: 1024 rich drawings among 19,200 short rules and 19,200 tiny lines.
fn crowded_page() -> String {
    let mut out = String::from("<page width=\"612\" height=\"792\" number=\"1\">");
    for i in 0..19_200 {
        let (x, y) = (80.0 + (i % 400) as f64 * 1.2, 100.0 + ((i / 400) % 500) as f64 * 1.2);
        out += &format!(
            "<rule xMin=\"{x:.2}\" yMin=\"{y:.2}\" xMax=\"{:.2}\" yMax=\"{:.2}\" color=\"#000000\"/>",
            x + 1.0,
            y + 0.3
        );
    }
    for li in 0..19_200 {
        let y = 80.0 + (li % 600) as f64;
        out += &format!(
            "<block><line><word xMin=\"600\" xMax=\"604\" yMin=\"{y}\" yMax=\"{}\" fontSize=\"2\" fontName=\"F\" baseline=\"{}\">ab</word></line></block>",
            y + 2.0,
            y + 1.6
        );
    }
    for i in 0..1024 {
        let (x, y) = (10.0 + (i % 32) as f64 * 18.5, 80.0 + (i / 32) as f64 * 20.0);
        out += &format!(
            "<graphic xMin=\"{x:.2}\" yMin=\"{y:.2}\" xMax=\"{:.2}\" yMax=\"{:.2}\" paths=\"50\" segments=\"100\" curves=\"50\" directions=\"8\"/>",
            x + 8.0,
            y + 8.0
        );
    }
    out + "</page>"
}

#[test]
fn placing_drawings_is_charged_and_bounded() {
    let xhtml = crowded_page();
    let p = Params::default();
    let run = |markers: bool| {
        let start = std::time::Instant::now();
        let (_, report) = to_markdown_report_with(&xhtml, &p, &Options { markers, ..Options::default() });
        (start.elapsed().as_secs_f64(), report)
    };
    let (off_time, off) = run(false);
    let (on_time, on) = run(true);
    // placing has a page budget of its own, charged to the document, and the layout of the
    // figures another: at most two pages' more
    let extra = on.doc_work as f64 - off.doc_work as f64;
    assert!(extra > p.max_page_work && extra <= 2.0 * p.max_page_work, "{extra}");
    assert_eq!(on.images_budget_pages, vec![0]);
    assert!(on.to_json().contains("\"images_budget_pages\":[1]"));
    // it used to take 15-25 times the page without markers
    if !cfg!(debug_assertions) {
        assert!(on_time <= 6.0 * off_time + 0.3, "{on_time} vs {off_time}");
    }
}

// ---- Bar charts: filled boxes (`<box>`), never `<graphic>` drawing (DocSlurp/doppler#38) ----

fn bx(x0: f64, y0: f64, x1: f64, y1: f64, color: &str) -> String {
    format!("<box xMin=\"{x0}\" yMin=\"{y0}\" xMax=\"{x1}\" yMax=\"{y1}\" color=\"{color}\"/>")
}

/// Axes, ticks and tick labels around a plot area 100..400 x 150..330 (the white plot background too).
fn axes() -> String {
    bx(0.0, 0.0, 612.0, 792.0, "#ffffff")
        + &bx(100.0, 150.0, 400.0, 330.0, "#ffffff")
        + &rule(100.0, 330.0, 400.0, 330.5)
        + &rule(100.0, 150.0, 100.5, 330.0)
        + &rule(130.0, 330.0, 130.5, 334.0)
        + &rule(96.0, 250.0, 100.0, 250.5)
        + &line_at(80.0, 245.0, "50")
        + &line_at(120.0, 336.0, "London")
}

fn markers(body: &str) -> Vec<(String, &'static str)> {
    let (_, report) = convert(&page(2, &(line(100.0, "Bookings rose in every region this year.") + body)), true);
    vector_fates(&report).into_iter().filter(|(_, fate)| *fate == "block").collect()
}

#[test]
fn vertical_horizontal_stacked_and_grouped_bars_are_figures() {
    let vertical = axes() + &bx(110.0, 250.0, 140.0, 330.0, "#2a9d8f") + &bx(160.0, 200.0, 190.0, 330.0, "#2a9d8f") + &bx(210.0, 280.0, 240.0, 330.0, "#2a9d8f");
    assert_eq!(markers(&vertical).len(), 1, "vertical bars");
    // horizontal bars share the left axis
    let horizontal = axes() + &bx(100.5, 160.0, 300.0, 180.0, "#7b61ff") + &bx(100.5, 200.0, 220.0, 220.0, "#7b61ff") + &bx(100.5, 240.0, 380.0, 260.0, "#7b61ff");
    assert_eq!(markers(&horizontal).len(), 1, "horizontal bars");
    // stacked: a second segment on each bar; one figure covering both
    let stacked = vertical.clone() + &bx(110.0, 220.0, 140.0, 250.0, "#e76f51") + &bx(160.0, 170.0, 190.0, 200.0, "#e76f51") + &bx(210.0, 260.0, 240.0, 280.0, "#e76f51");
    let found = markers(&stacked);
    assert_eq!(found.len(), 1, "stacked bars");
    // grouped: pairs of bars, small gaps inside a group and larger between groups
    let grouped = axes() + &bx(110.0, 250.0, 125.0, 330.0, "#2a9d8f") + &bx(126.0, 230.0, 141.0, 330.0, "#e76f51")
        + &bx(170.0, 200.0, 185.0, 330.0, "#2a9d8f") + &bx(186.0, 260.0, 201.0, 330.0, "#e76f51");
    assert_eq!(markers(&grouped).len(), 1, "grouped bars");
}

#[test]
fn shaded_tables_zebra_rows_highlights_and_borders_are_not_bar_charts() {
    // a shaded table: cells of different widths on one baseline, each holding words, on ruled lines
    let table = rule(72.0, 300.0, 540.0, 300.5) + &bx(72.0, 280.0, 200.0, 300.0, "#dddddd") + &bx(200.0, 280.0, 400.0, 300.0, "#dddddd") + &bx(400.0, 280.0, 540.0, 300.0, "#dddddd")
        + &line_at(80.0, 284.0, "Region") + &line_at(210.0, 284.0, "Revenue") + &line_at(410.0, 284.0, "Growth");
    assert!(markers(&table).is_empty(), "shaded table");
    // zebra stripes: rows of one length on a shared left edge, beside a rule
    let zebra = rule(72.0, 200.0, 72.5, 400.0) + &(0..6).map(|i| bx(72.0, 200.0 + 30.0 * i as f64, 540.0, 215.0 + 30.0 * i as f64, "#f2f2f2")).collect::<String>();
    assert!(markers(&zebra).is_empty(), "zebra rows");
    // highlighted text: boxes behind words
    let highlight = rule(72.0, 400.0, 540.0, 400.5) + &bx(72.0, 380.0, 150.0, 400.0, "#ffff00") + &bx(160.0, 370.0, 260.0, 400.0, "#ffff00") + &bx(270.0, 390.0, 300.0, 400.0, "#ffff00")
        + &line_at(75.0, 385.0, "launch") + &line_at(165.0, 380.0, "expansion") + &line_at(272.0, 391.0, "go");
    assert!(markers(&highlight).is_empty(), "highlighted text");
    // a page border and a white background
    let border = bx(20.0, 20.0, 592.0, 772.0, "#000000") + &bx(0.0, 0.0, 612.0, 792.0, "#ffffff");
    assert!(markers(&border).is_empty(), "page border");
    // bars without any axis or tick are decoration (a progress strip), not a chart
    let no_axis = bx(110.0, 250.0, 140.0, 330.0, "#2a9d8f") + &bx(160.0, 200.0, 190.0, 330.0, "#2a9d8f") + &bx(210.0, 280.0, 240.0, 330.0, "#2a9d8f");
    assert!(markers(&no_axis).is_empty(), "no axis");
}

#[test]
fn many_boxes_stay_bounded() {
    // 20000 boxes on a few baselines: the converter finishes and finds at most a few runs
    let mut body = axes();
    for i in 0..20000 {
        let x = 100.0 + (i % 290) as f64;
        body += &bx(x, 330.0 - (i % 170) as f64 - 1.0, x + 0.8, 330.0, "#2a9d8f");
    }
    let started = std::time::Instant::now();
    let found = markers(&body);
    assert!(found.len() <= 4);
    assert!(started.elapsed().as_secs() < 20);
}
