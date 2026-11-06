//! Assignment and reading order checked through the conversion interface. Expected output
//! and work counts were verified against the baseline Rust converter (Python v24 parity); work
//! counts now also include gutter-table runs (2 units per word), which Python does not charge.

use doppler_md::{Degraded, Params, to_markdown_report};

fn word(x0: f64, x1: f64, y: f64, text: &str, font: &str) -> String {
    format!(
        "<word xMin=\"{x0}\" xMax=\"{x1}\" yMin=\"{y}\" yMax=\"{}\" fontSize=\"10\" fontName=\"{font}\" baseline=\"{}\">{text}</word>",
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

fn rule(x0: f64, x1: f64, y0: f64, y1: f64) -> String {
    format!("<rule xMin=\"{x0}\" xMax=\"{x1}\" yMin=\"{y0}\" yMax=\"{y1}\"/>")
}

#[test]
fn adjacent_ruled_tables_own_their_titles_without_changing_an_earlier_heading() {
    let mut body = line(&word(20.0, 100.0, 70.0, "Overview", "Serif-Bold"));
    for (top, title) in [(100.0, "First stock"), (180.0, "Second stock")] {
        body += &line(&word(20.0, 100.0, top - 14.0, title, "Serif"));
        for y in [top, top + 20.0, top + 40.0] {
            body += &rule(20.0, 220.0, y, y + 0.5);
        }
        body += &rule(120.0, 120.5, top, top + 40.0);
        for (i, (label, amount)) in [("Apples", "1.0"), ("Pears", "2.0")].into_iter().enumerate() {
            let y = top + 4.0 + 20.0 * i as f64;
            body += &line(&(word(25.0, 70.0, y, label, "Serif") + &word(125.0, 160.0, y, amount, "Serif")));
        }
    }
    let (md, report) = to_markdown_report(&page(&body), &Params::default());
    // Overview's original neighbour is the title paragraph. Absorbing that title must
    // neither italicize Overview as a caption nor duplicate either title in prose.
    assert_eq!(
        md,
        [concat!(
            "Overview\n\n| First stock |  |\n| --- | --- |\n| Apples | 1.0 |\n| Pears | 2.0 |\n\n",
            "| Second stock |  |\n| --- | --- |\n| Apples | 1.0 |\n| Pears | 2.0 |"
        )]
    );
    assert!(report.degraded.is_empty());
    assert_eq!((report.doc_work, report.page_work_max), (12, 5));
}

#[test]
fn wrapped_cells_keep_their_continuation_once() {
    let mut body = line(&word(20.0, 70.0, 84.0, "Stock", "Serif"));
    for (y, label, amount) in [(100.0, "Apples", "1.0"), (120.0, "Pears", "2.0"), (140.0, "Plums", "3.0")] {
        body += &line(&(word(20.0, 60.0, y, label, "Serif") + &word(160.0, 190.0, y, amount, "Serif")));
        if y == 100.0 {
            body += &line(&word(160.0, 195.0, 112.0, "crates", "Serif"));
        }
    }
    let (md, report) = to_markdown_report(&page(&body), &Params::default());
    assert_eq!(md, ["Stock\n\n| Apples | 1.0 crates |\n| --- | --- |\n| Pears | 2.0 |\n| Plums | 3.0 |"]);
    assert!(report.degraded.is_empty());
    // gutter-table runs also charge their words twice (columns, then grid): 5 + 2 x 7
    assert_eq!((report.doc_work, report.page_work_max), (24, 19));
}

fn overlapping_regions() -> String {
    let mut body = String::new();
    for (i, (label, amount)) in [("A", "10"), ("B", "20"), ("C", "30")].into_iter().enumerate() {
        let y = 100.0 + 14.0 * i as f64;
        body += &line(&(word(20.0, 40.0, y, label, "Serif") + &word(80.0, 100.0, y, amount, "Serif")));
        if i == 0 {
            body += &line(&word(100.5, 100.5, y, "shared", "Serif"));
        }
        body += &line(&word(101.0, 400.0, y, &format!("Right column sentence {i}."), "Serif"));
    }
    page(&body)
}

#[test]
fn narrow_gutter_membership_belongs_to_each_overlapping_region() {
    // The sub-two-point gutter intentionally puts the zero-width shared line in both
    // regions. It belongs to a table on the left and prose on the right (Python behavior).
    let p = Params { column_gutter: 0.05, table_align: 2.0, ..Params::default() };
    let (md, report) = to_markdown_report(&overlapping_regions(), &p);
    assert_eq!(
        md,
        [concat!(
            "| A | 10 shared |\n| --- | --- |\n| B | 20 |\n| C | 30 |\n\n",
            "shared Right column sentence 0. Right column sentence 1. Right column sentence 2."
        )]
    );
    assert!(report.degraded.is_empty());
    // 39 layout units as in Python v24, plus 2 x 7 words for the gutter-table run
    assert_eq!((report.doc_work, report.page_work_max), (60, 53));
}

#[test]
fn exhausted_layout_discards_all_partial_assignments() {
    let p = Params { column_gutter: 0.05, table_align: 2.0, max_page_work: 38.0, ..Params::default() };
    let (md, report) = to_markdown_report(&overlapping_regions(), &p);
    assert_eq!(
        md,
        [concat!(
            "A 10\nshared\nRight column sentence 0.\nB 20\nRight column sentence 1.\nC 30\nRight column sentence 2."
        )]
    );
    assert_eq!(report.degraded, [(0, Degraded::PageWork)]);
    assert_eq!((report.doc_work, report.page_work_max), (58, 51));
}
