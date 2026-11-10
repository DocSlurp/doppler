//! Ruled tables drawn as filled cell boxes (`box_cell_tables`): one filled box per cell between
//! thin dark border rectangles, often with a same-colour box behind each text line inside a
//! cell. Each cell's lines must land in one cell; form fields, single rows of boxes, diagrams
//! and backgrounds must not become tables.

use doppler_md::{Degraded, Options, Params, TableFormat, to_markdown_report, to_markdown_report_with};

fn word(x0: f64, x1: f64, y: f64, text: &str) -> String {
    format!(
        "<word xMin=\"{x0}\" xMax=\"{x1}\" yMin=\"{y}\" yMax=\"{}\" fontSize=\"10\" fontName=\"Sans\" baseline=\"{}\">{text}</word>",
        y + 10.0,
        y + 8.0
    )
}

fn line(words: &str) -> String {
    format!("<block><line>{words}</line></block>")
}

fn page(body: &str) -> String {
    format!("<page width=\"612\" height=\"792\">{body}</page>")
}

fn filled(x0: f64, x1: f64, y0: f64, y1: f64, color: &str) -> String {
    format!("<box xMin=\"{x0}\" xMax=\"{x1}\" yMin=\"{y0}\" yMax=\"{y1}\" color=\"{color}\"/>")
}

fn rule(x0: f64, x1: f64, y0: f64, y1: f64) -> String {
    format!("<rule xMin=\"{x0}\" xMax=\"{x1}\" yMin=\"{y0}\" yMax=\"{y1}\" color=\"#000000\"/>")
}

/// A cell: a white box inside the border lines at x0..x1, y0..y1, with a white box behind each
/// text line (as word processors draw paragraph shading) and borders drawn as thin dark
/// rectangles around it, segment by segment.
fn cell(x0: f64, x1: f64, y0: f64, y1: f64, texts: &[(f64, &str)]) -> String {
    let mut s = filled(x0 + 0.5, x1 - 0.5, y0 + 0.5, y1 - 0.5, "#ffffff");
    for &(y, t) in texts {
        s += &filled(x0 + 5.0, x1 - 5.0, y - 1.0, y + 11.0, "#ffffff");
        s += &line(&word(x0 + 6.0, x0 + 6.0 + 6.0 * t.len() as f64, y, t));
    }
    s += &rule(x0 - 0.5, x1 + 0.5, y0 - 0.5, y0 + 0.5);
    s += &rule(x0 - 0.5, x1 + 0.5, y1 - 0.5, y1 + 0.5);
    s += &rule(x0 - 0.5, x0 + 0.5, y0, y1);
    s += &rule(x1 - 0.5, x1 + 0.5, y0, y1);
    s
}

fn md(x: &str, p: &Params) -> String {
    let (md, report) = to_markdown_report(&page(x), p);
    assert!(report.degraded.is_empty(), "{:?}", report.degraded);
    md.join("\u{c}")
}

fn on(x: &str) -> String {
    md(x, &Params::default())
}

fn off(x: &str) -> String {
    md(x, &Params { box_cell_tables: false, ..Params::default() })
}

fn html(x: &str) -> String {
    let o = Options { tables: TableFormat::Html, ..Options::default() };
    to_markdown_report_with(&page(x), &Params::default(), &o).0.join("\u{c}")
}

/// A header of stacked lines over two rows; the header's lines and a vertically centred cell
/// (between the inner lines of its neighbours) belong to one cell each.
fn stacked_header_table() -> String {
    let mut b = String::new();
    b += &cell(50.0, 150.0, 100.0, 140.0, &[(102.0, "Part"), (114.0, "Serial"), (126.0, "Code")]);
    b += &cell(150.0, 300.0, 100.0, 140.0, &[(114.0, "Component")]);
    b += &cell(300.0, 400.0, 100.0, 140.0, &[(108.0, "Rating"), (120.0, "Class")]);
    for (y, a, c, d) in [(140.0, "Alpha", "Tank", "PE"), (156.0, "Beta", "Strap", "SL")] {
        b += &cell(50.0, 150.0, y, y + 16.0, &[(y + 3.0, a)]);
        b += &cell(150.0, 300.0, y, y + 16.0, &[(y + 3.0, c)]);
        b += &cell(300.0, 400.0, y, y + 16.0, &[(y + 3.0, d)]);
    }
    b
}

#[test]
fn stacked_lines_of_a_cell_stay_in_one_cell() {
    let b = stacked_header_table();
    assert_eq!(
        on(&b),
        "| Part Serial Code | Component | Rating Class |\n| --- | --- | --- |\n| Alpha | Tank | PE |\n| Beta | Strap | SL |"
    );
    // without the pass, the inner line boxes cut the header into a row per line
    assert!(!off(&b).contains("Part Serial Code"));
}

#[test]
fn a_cell_spanning_rows_lands_in_its_first_row() {
    let mut b = String::new();
    b += &cell(50.0, 150.0, 100.0, 116.0, &[(103.0, "Group")]);
    b += &cell(150.0, 300.0, 100.0, 116.0, &[(103.0, "Item")]);
    b += &cell(300.0, 400.0, 100.0, 116.0, &[(103.0, "Code")]);
    // two lines, centred on the two rows beside it
    b += &cell(50.0, 150.0, 116.0, 164.0, &[(128.0, "North"), (140.0, "Yard")]);
    for (y, c, d) in [(116.0, "Tank", "PE"), (132.0, "Strap", "SL"), (148.0, "Valve", "FS")] {
        b += &cell(150.0, 300.0, y, y + 16.0, &[(y + 3.0, c)]);
        b += &cell(300.0, 400.0, y, y + 16.0, &[(y + 3.0, d)]);
    }
    assert_eq!(
        on(&b),
        "| Group | Item | Code |\n| --- | --- | --- |\n| North Yard | Tank | PE |\n|  | Strap | SL |\n|  | Valve | FS |"
    );
    let h = html(&b);
    assert!(h.contains("<tr><td rowspan=\"3\">North Yard</td><td>Tank</td><td>PE</td></tr>"), "{h}");
    assert!(h.contains("<tr><td>Strap</td><td>SL</td></tr>"), "{h}");
}

#[test]
fn an_unfilled_cell_takes_the_rows_of_the_cell_beside_it() {
    let mut b = String::new();
    for (x0, x1, t) in [(50.0, 150.0, "Model"), (150.0, 250.0, "Size"), (250.0, 400.0, "Check")] {
        b += &cell(x0, x1, 100.0, 116.0, &[(103.0, t)]);
    }
    // the middle column has borders but no fill below the header; its value sits lower
    // than the first sub-row of the model cell it belongs to
    for (y, m) in [(116.0, "CC02"), (148.0, "SC01")] {
        b += &cell(50.0, 150.0, y, y + 32.0, &[(y + 10.0, m)]);
        b += &line(&word(160.0, 190.0, y + 13.0, "0.25"));
        b += &cell(250.0, 400.0, y, y + 16.0, &[(y + 3.0, "a = 0.05")]);
        b += &cell(250.0, 400.0, y + 16.0, y + 32.0, &[(y + 19.0, "c = 0.05")]);
    }
    assert_eq!(
        on(&b),
        concat!(
            "| Model | Size | Check |\n| --- | --- | --- |\n| CC02 | 0.25 | a = 0.05 |\n|  |  | c = 0.05 |\n",
            "| SC01 | 0.25 | a = 0.05 |\n|  |  | c = 0.05 |"
        )
    );
}

#[test]
fn an_unfilled_header_row_under_the_top_border_is_the_first_row() {
    let mut b = String::new();
    b += &rule(50.0, 400.0, 80.0, 81.0);
    b += &line(&word(56.0, 90.0, 84.0, "Phase"));
    b += &line(&word(56.0, 90.0, 96.0, "Name"));
    b += &line(&word(156.0, 220.0, 90.0, "Activity"));
    for (y, a, c) in [(110.0, "A", "Plan"), (126.0, "B", "Build")] {
        b += &cell(50.0, 150.0, y, y + 16.0, &[(y + 3.0, a)]);
        b += &cell(150.0, 400.0, y, y + 16.0, &[(y + 3.0, c)]);
    }
    assert_eq!(on(&b), "| Phase Name | Activity |\n| --- | --- |\n| A | Plan |\n| B | Build |");
}

#[test]
fn a_title_across_the_columns_above_the_boxes_is_no_header_row() {
    let mut b = String::new();
    b += &rule(50.0, 400.0, 80.0, 81.0);
    b += &line(&word(56.0, 300.0, 88.0, "Section B applies to joint returns"));
    for (y, a, c) in [(110.0, "A", "Plan"), (126.0, "B", "Build")] {
        b += &cell(50.0, 150.0, y, y + 16.0, &[(y + 3.0, a)]);
        b += &cell(150.0, 400.0, y, y + 16.0, &[(y + 3.0, c)]);
    }
    let out = on(&b);
    assert!(out.starts_with("Section B applies to joint returns\n\n| A | Plan |"), "{out}");
}

#[test]
fn white_cells_on_a_dark_background_are_cells() {
    let mut b = filled(48.0, 402.0, 98.0, 150.0, "#202020");
    for (y, a, c) in [(100.0, "Key", "Value"), (116.0, "Red", "1"), (132.0, "Blue", "2")] {
        b += &filled(50.0, 149.0, y, y + 15.0, "#ffffff");
        b += &filled(151.0, 400.0, y, y + 15.0, "#ffffff");
        b += &line(&word(56.0, 90.0, y + 3.0, a));
        b += &line(&word(156.0, 190.0, y + 3.0, c));
    }
    assert_eq!(on(&b), "| Key | Value |\n| --- | --- |\n| Red | 1 |\n| Blue | 2 |");
}

#[test]
fn banner_and_note_boxes_across_the_width_stay_outside() {
    let mut b = String::new();
    b += &cell(50.0, 400.0, 80.0, 100.0, &[(84.0, "Caution read first")]);
    for (y, a, c) in [(100.0, "1.", "Enter income"), (116.0, "2.", "Enter tax")] {
        b += &cell(50.0, 80.0, y, y + 16.0, &[(y + 3.0, a)]);
        b += &cell(80.0, 400.0, y, y + 16.0, &[(y + 3.0, c)]);
    }
    b += &cell(50.0, 400.0, 132.0, 152.0, &[(136.0, "Keep for your records")]);
    let out = on(&b);
    assert!(out.contains("| 1. | Enter income |\n| --- | --- |\n| 2. | Enter tax |"), "{out}");
    assert!(!out.contains("| Caution") && !out.contains("| Keep"), "{out}");
}

#[test]
fn box_order_does_not_matter() {
    let b = stacked_header_table();
    // the same page with its elements in reverse order
    let mut parts: Vec<String> = Vec::new();
    let mut rest = b.as_str();
    while let Some(i) = rest[1..].find('<').map(|i| i + 1) {
        let (head, tail) = rest.split_at(i);
        parts.push(head.to_string());
        rest = tail;
    }
    parts.push(rest.to_string());
    // keep each block (three elements) together, reverse the rest
    let mut items: Vec<String> = Vec::new();
    let mut i = 0;
    while i < parts.len() {
        if parts[i].starts_with("<block") {
            items.push(parts[i..i + 5].concat());
            i += 5;
        } else {
            items.push(parts[i].clone());
            i += 1;
        }
    }
    items.reverse();
    assert_eq!(on(&items.concat()), on(&b));
}

// ----- negatives: the pass leaves these pages exactly as before

#[test]
fn form_fields_whose_rows_split_differently_are_not_a_table() {
    let mut b = String::new();
    for (y, splits) in
        [(100.0, vec![50.0, 300.0, 400.0]), (124.0, vec![50.0, 120.0, 400.0]), (148.0, vec![50.0, 200.0, 260.0, 400.0])]
    {
        for w in splits.windows(2) {
            b += &filled(w[0], w[1], y, y + 24.0, "#ffffff");
            b += &line(&word(w[0] + 2.0, w[0] + 40.0, y + 1.0, "Label"));
        }
    }
    assert_eq!(on(&b), off(&b));
}

#[test]
fn a_single_row_of_boxes_is_not_a_table() {
    let mut b = String::new();
    for (x0, x1, t) in [(50.0, 150.0, "One"), (150.0, 250.0, "Two"), (250.0, 350.0, "Three"), (350.0, 450.0, "Four")] {
        b += &cell(x0, x1, 100.0, 116.0, &[(103.0, t)]);
    }
    assert_eq!(on(&b), off(&b));
}

#[test]
fn text_running_out_of_the_boxes_is_not_their_table() {
    let mut b = stacked_header_table();
    b += &line(&word(40.0, 520.0, 159.0, "a sentence of body text drawn over the table and past it"));
    assert_eq!(on(&b), off(&b));
}

#[test]
fn separate_diagram_boxes_are_not_a_table() {
    let mut b = String::new();
    for (i, t) in ["Plan", "Build", "Test", "Ship"].into_iter().enumerate() {
        let x = 50.0 + 120.0 * (i % 2) as f64;
        let y = 100.0 + 40.0 * (i / 2) as f64;
        b += &filled(x, x + 90.0, y, y + 20.0, "#ffffff");
        b += &line(&word(x + 5.0, x + 40.0, y + 4.0, t));
    }
    assert_eq!(on(&b), off(&b));
}

#[test]
fn page_backgrounds_and_shaded_rows_are_not_cells() {
    let mut b = filled(0.0, 612.0, 0.0, 792.0, "#ffffff");
    b += &filled(40.0, 560.0, 300.0, 700.0, "#fafafa");
    for (i, y) in [100.0, 140.0, 180.0].into_iter().enumerate() {
        b += &filled(50.0, 400.0, y, y + 14.0, "#eeeeee");
        b += &line(&(word(56.0, 100.0, y + 2.0, &format!("Row{i}")) + &word(300.0, 340.0, y + 2.0, "12")));
    }
    assert_eq!(on(&b), off(&b));
}

// ----- hostile input

#[test]
fn too_many_boxes_skip_the_pass() {
    let mut b = stacked_header_table();
    for i in 0..2100 {
        let x = 20.0 + (i % 50) as f64 * 11.0;
        let y = 300.0 + (i / 50) as f64 * 11.0;
        b += &filled(x, x + 10.0, y, y + 10.0, "#ffffff");
    }
    assert_eq!(on(&b), off(&b));
}

#[test]
fn a_pile_of_nested_boxes_is_bounded() {
    let mut b = stacked_header_table();
    for i in 0..1500 {
        let d = i as f64 * 0.1;
        b += &filled(20.0 + d, 590.0 - d, 300.0 + d, 600.0 - d, if i % 2 == 0 { "#ffffff" } else { "#eeeeee" });
    }
    let (md, report) = to_markdown_report(&page(&b), &Params::default());
    assert_eq!(md.len(), 1);
    assert!(report.degraded.is_empty() || report.degraded == vec![(0, Degraded::PageWork)]);
}

#[test]
fn the_pass_charges_the_page_budget() {
    let b = stacked_header_table();
    let p = Params { max_page_work: 50.0, ..Params::default() };
    let (_, report) = to_markdown_report(&page(&b), &p);
    assert_eq!(report.degraded, vec![(0, Degraded::PageWork)]);
    // and pages over max_page_rules are not analysed at all
    let p = Params { max_page_rules: 10.0, ..Params::default() };
    let (_, report) = to_markdown_report(&page(&b), &p);
    assert_eq!(report.degraded, vec![(0, Degraded::PageRules)]);
}
