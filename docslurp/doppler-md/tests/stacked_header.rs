//! A header printed on several stacked lines is one header row.

use doppler_md::{Options, Params, TableFormat, to_markdown_report_with};

fn word(x0: f64, y: f64, text: &str) -> String {
    format!(
        "<word xMin=\"{x0}\" xMax=\"{}\" yMin=\"{y}\" yMax=\"{}\" fontSize=\"10\" fontName=\"Serif\" baseline=\"{}\">{text}</word>",
        x0 + 6.0 * text.len() as f64,
        y + 10.0,
        y + 8.0
    )
}

fn row(y: f64, cells: [&str; 3]) -> String {
    let xs = [72.0, 230.0, 380.0];
    xs.iter()
        .zip(cells)
        .filter(|(_, c)| !c.is_empty())
        .map(|(x, c)| format!("<block><line>{}</line></block>", word(*x, y, c)))
        .collect()
}

fn page(pitch: f64) -> String {
    let mut body = row(100.0, ["Water", "", "Fracture"]) + &row(112.0, ["tank", "Component", "rating"]);
    for i in 0..6 {
        let y = 112.0 + pitch * (i + 1) as f64;
        body += &row(y, [&format!("T{i}"), &format!("{}", 40 + i), &format!("{}.5", 1 + i)]);
    }
    format!("<page width=\"612\" height=\"792\">{body}</page>")
}

fn convert(xhtml: &str, merge: bool, tables: TableFormat) -> String {
    let mut p = Params::default();
    p.merge_header_rows = merge;
    let o = Options { tables, ..Options::default() };
    to_markdown_report_with(xhtml, &p, &o).0.join("")
}

#[test]
fn tight_header_lines_over_a_looser_body_become_one_header_row() {
    let md = convert(&page(20.0), true, TableFormat::Gfm);
    assert!(
        md.starts_with("| Water tank | Component | Fracture rating |\n| --- | --- | --- |\n| T0 | 40 | 1.5 |"),
        "{md}"
    );
    let before = convert(&page(20.0), false, TableFormat::Gfm);
    assert!(before.contains("| Water |  | Fracture |"), "{before}");
}

#[test]
fn html_tables_get_a_thead_for_the_merged_header() {
    let md = convert(&page(20.0), true, TableFormat::Html);
    assert!(
        md.contains("<thead><tr><th>Water tank</th><th>Component</th><th>Fracture rating</th></tr></thead>"),
        "{md}"
    );
}

#[test]
fn a_body_as_tight_as_the_header_is_left_alone() {
    let md = convert(&page(12.0), true, TableFormat::Gfm);
    assert!(!md.contains("Water tank"), "{md}");
}
