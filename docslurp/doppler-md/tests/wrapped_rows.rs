//! Borderless tables whose cells wrap onto several lines, with padding between the rows
//! (`wrapped_rows`), and layouts that must not become tables.
//!
//! The XHTML is built here: a word is 5 points per character at size 10, words are 2.5 points
//! apart, and each cell line is its own `<line>`, as pdftotext writes table cells.

use doppler_md::{Options, Params, TableFormat, to_markdown_report_with};

/// A line's words from its left edge and baseline.
fn line(x0: f64, base: f64, text: &str) -> String {
    let mut x = x0;
    let mut words = String::new();
    for w in text.split(' ') {
        let x1 = x + 5.0 * w.chars().count() as f64;
        words += &format!(
            "<word xMin=\"{x}\" xMax=\"{x1}\" yMin=\"{}\" yMax=\"{}\" fontSize=\"10\" fontName=\"Serif\" baseline=\"{base}\" spaceAfter=\"1\">{w}</word>",
            base - 8.0,
            base + 2.0
        );
        x = x1 + 2.5;
    }
    format!("<block><line>{words}</line></block>")
}

fn page(body: &str) -> String {
    format!("<page width=\"612\" height=\"792\">{body}</page>")
}

fn convert(xhtml: &str, on: bool) -> String {
    let p = Params { wrapped_rows: on, ..Params::default() };
    let o = Options { tables: TableFormat::Gfm, ..Options::default() };
    to_markdown_report_with(xhtml, &p, &o).0.join("\u{c}")
}

/// Column left edges of a four-column requirements table.
const XS: [f64; 4] = [72.0, 130.0, 230.0, 500.0];

/// A table whose third column wraps onto `lines[i]` lines in row `i`, rows `pad` points apart
/// from the last line of the row above (line pitch 13).
fn requirements(pad: f64, wraps: &[&[&str]]) -> String {
    let ids = ["RQ-1", "RQ-2", "RQ-3", "RQ-4", "RQ-5"];
    let owners = ["Thermal", "Avionics", "Power", "Structures", "Comms"];
    let status = ["Open", "Closed", "Verified", "Open", "Closed"];
    let mut body = String::new();
    let mut y = 100.0;
    for (k, h) in ["ID", "Owner", "Requirement", "Status"].iter().enumerate() {
        body += &line(XS[k], y, h);
    }
    y += 13.0 + pad;
    for (i, text) in wraps.iter().enumerate() {
        body += &line(XS[0], y, ids[i]);
        body += &line(XS[1], y, owners[i]);
        body += &line(XS[3], y, status[i]);
        for (j, l) in text.iter().enumerate() {
            body += &line(XS[2], y + 13.0 * j as f64, l);
        }
        y += 13.0 * text.len() as f64 + pad;
    }
    body += &line(72.0, y + 20.0, "A closing paragraph follows the table and stays a paragraph.");
    page(&body)
}

const WRAPS: &[&[&str]] = &[
    &["Radiators shall reject four kW of heat", "at end of life with heaters off."],
    &["Software shall boot within thirty seconds."],
    &[
        "Battery discharge shall stay below forty",
        "percent in the longest eclipse season,",
        "including all contingency loads.",
    ],
    &["Structure shall survive twelve g axial."],
    &["Downlink shall reach two Mbps at the", "maximum range of the mission."],
];

const TABLE: &str = "| ID | Owner | Requirement | Status |\n| --- | --- | --- | --- |\n\
| RQ-1 | Thermal | Radiators shall reject four kW of heat at end of life with heaters off. | Open |\n\
| RQ-2 | Avionics | Software shall boot within thirty seconds. | Closed |\n\
| RQ-3 | Power | Battery discharge shall stay below forty percent in the longest eclipse season, including all contingency loads. | Verified |\n\
| RQ-4 | Structures | Structure shall survive twelve g axial. | Open |\n\
| RQ-5 | Comms | Downlink shall reach two Mbps at the maximum range of the mission. | Closed |";

#[test]
fn padded_rows_with_wrapped_cells_are_one_table() {
    let md = convert(&requirements(4.0, WRAPS), true);
    assert!(md.contains(TABLE), "{md}");
    assert!(md.ends_with("\n\nA closing paragraph follows the table and stays a paragraph."), "{md}");
    let before = convert(&requirements(4.0, WRAPS), false);
    assert!(!before.contains(TABLE), "{before}");
}

#[test]
fn wide_padding_takes_the_sparse_pass() {
    // rows more than table_row_gap apart: the dense pass leaves them, the sparse pass reads the
    // gaps from each row's last line
    let md = convert(&requirements(14.0, WRAPS), true);
    assert!(md.contains(TABLE), "{md}");
}

#[test]
fn a_wrapped_line_of_the_last_row_stays_in_it() {
    let mut wraps = WRAPS.to_vec();
    wraps.truncate(3);
    let md = convert(&requirements(4.0, &wraps), true);
    assert!(md.contains("including all contingency loads. | Verified |"), "{md}");
    assert!(!md.contains("\n\nincluding"), "{md}");
}

#[test]
fn hanging_indent_wraps_join_their_cell() {
    let mut body = String::new();
    let mut y = 100.0;
    let rows: [(&str, &[&str], &str, &str); 4] = [
        ("3.1", &["Restated articles of incorporation filed with", "the state registrar."], "8-K", "8/7/20"),
        ("3.2", &["Amended bylaws effective as of August."], "8-K", "8/23/24"),
        (
            "4.1",
            &["Indenture between the registrant and the", "trust company, as trustee, for the notes."],
            "S-3",
            "4/29/13",
        ),
        ("4.2", &["Officer certificate for the global notes."], "8-K", "5/3/13"),
    ];
    for (num, desc, form, date) in rows {
        body += &line(72.0, y, num);
        body += &line(110.0, y, desc[0]);
        for (j, l) in desc.iter().enumerate().skip(1) {
            body += &line(118.0, y + 12.0 * j as f64, l);
        }
        body += &line(400.0, y, form);
        body += &line(460.0, y, date);
        y += 12.0 * desc.len() as f64 + 6.0;
    }
    let md = convert(&page(&body), true);
    assert!(
        md.contains("| 3.1 | Restated articles of incorporation filed with the state registrar. | 8-K | 8/7/20 |"),
        "{md}"
    );
    assert!(
        md.contains(
            "| 4.1 | Indenture between the registrant and the trust company, as trustee, for the notes. | S-3 |"
        ),
        "{md}"
    );
}

#[test]
fn rows_without_padding_are_left_to_the_dense_pass() {
    // rows one line pitch apart: a short row is a row with empty cells, not a wrapped line
    let md_on = convert(&requirements(0.0, WRAPS), true);
    let md_off = convert(&requirements(0.0, WRAPS), false);
    assert_eq!(md_on, md_off);
}

#[test]
fn a_row_with_empty_cells_at_row_pitch_is_its_own_row() {
    // evenly spaced rows: a row with an empty first cell is not folded into the row above
    let mut body = String::new();
    let mut y = 100.0;
    for (a, b, c) in [("Alpha", "Red", "10"), ("Beta", "Green", "20"), ("", "Blue", "30"), ("Delta", "Gray", "40")] {
        if !a.is_empty() {
            body += &line(72.0, y, a);
        }
        body += &line(200.0, y, b);
        body += &line(320.0, y, c);
        y += 18.0;
    }
    let md = convert(&page(&body), true);
    assert_eq!(md, convert(&page(&body), false));
}

#[test]
fn two_column_prose_is_not_a_table() {
    let words = "the quick brown fox jumps over a lazy dog while the";
    let mut body = String::new();
    for i in 0..12 {
        let y = 100.0 + 13.0 * i as f64;
        body += &line(72.0, y, words);
        body += &line(330.0, y, words);
    }
    let on = convert(&page(&body), true);
    assert!(!on.contains('|'), "{on}");
    assert_eq!(on, convert(&page(&body), false));
}

#[test]
fn a_bulleted_list_with_wrapped_items_is_not_a_table() {
    let mut body = String::new();
    let mut y = 100.0;
    for item in [
        ["Check the supply voltage at the main", "connector before power on."],
        ["Inspect the harness for chafing and", "replace damaged sections."],
        ["Record the readings in the test log", "with the date and operator."],
    ] {
        body += &line(72.0, y, "•");
        body += &line(84.0, y, item[0]);
        body += &line(84.0, y + 13.0, item[1]);
        y += 13.0 * 2.0 + 6.0;
    }
    let on = convert(&page(&body), true);
    assert!(!on.contains('|'), "{on}");
}

#[test]
fn key_value_text_with_wrapped_values_is_not_a_table() {
    let mut body = String::new();
    let mut y = 100.0;
    for (k, v) in [
        ("Name:", ["Jane Example of the Example Department,", "Somewhere Office, Building Four"]),
        ("Address:", ["Twelve Long Street, Apartment Five,", "Example City, Example County"]),
        ("Notes:", ["Deliveries go to the side entrance after", "six in the evening on weekdays"]),
    ] {
        body += &line(72.0, y, k);
        body += &line(160.0, y, v[0]);
        body += &line(160.0, y + 13.0, v[1]);
        y += 13.0 * 2.0 + 8.0;
    }
    let on = convert(&page(&body), true);
    assert!(!on.contains('|'), "{on}");
}

#[test]
fn side_headings_beside_paragraphs_are_not_a_table() {
    let mut body = String::new();
    let mut y = 100.0;
    let para = "the scope of this section covers every subsystem on the vehicle";
    for head in ["Purpose", "Scope", "Background"] {
        body += &line(72.0, y, head);
        for j in 0..4 {
            body += &line(160.0, y + 13.0 * j as f64, para);
        }
        y += 13.0 * 4.0 + 10.0;
    }
    let on = convert(&page(&body), true);
    assert!(!on.contains('|'), "{on}");
    assert_eq!(on, convert(&page(&body), false));
}
