//! Rotated text and borderless (whitespace-aligned) tables.
//!
//! The XHTML in tests/fixtures/producer is real DocSlurp pdftotext output (`-bbox-layout
//! -wordmeta -rules`) for the synthetic PDFs that scripts/make_table_pdfs.py draws; all text is
//! made up there. The large-page test builds its XHTML here.

use std::path::Path;
use std::time::{Duration, Instant};

use doppler_md::{Options, Params, TableFormat, to_markdown_report_with};

const HTML: Options = Options { tables: TableFormat::Html, math: false };

fn pages(name: &str) -> Vec<String> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("tests/fixtures/producer/{name}.xhtml"));
    let xhtml = std::fs::read_to_string(path).unwrap();
    to_markdown_report_with(&xhtml, &Params::default(), &HTML).0
}

fn unescape(s: &str) -> String {
    s.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&amp;", "&")
}

/// Every `<table>` in a page as rows of cell texts (`<th>` and `<td>` alike).
fn tables(md: &str) -> Vec<Vec<Vec<String>>> {
    let mut out = Vec::new();
    for chunk in md.split("<table").skip(1) {
        let body = chunk.split("</table>").next().unwrap_or("");
        let rows = body
            .split("<tr>")
            .skip(1)
            .map(|tr| {
                let tr = tr.split("</tr>").next().unwrap_or("");
                tr.split("<t")
                    .skip(1)
                    .filter_map(|c| {
                        let c = c.strip_prefix("d>").or_else(|| c.strip_prefix("h>"))?;
                        Some(unescape(c.split("</t").next().unwrap_or("")))
                    })
                    .collect()
            })
            .collect();
        out.push(rows);
    }
    out
}

fn rows(t: &[&[&str]]) -> Vec<Vec<String>> {
    t.iter().map(|r| r.iter().map(|c| c.to_string()).collect()).collect()
}

const STATIONS: &[&[&str]] = &[
    &["Station", "Depth (m)", "Flow", "Change"],
    &["Alder Creek", "12.5", "1,204", "(37.2)"],
    &["Birch Hollow", "7.25", "88", "4.1"],
    &["Cedar Run", "104.0", "15,730", "(0.6)"],
    &["Dunmore Lake", "3.75", "612", "12.9"],
    &["Elm Fork", "66.1", "9", "(118.4)"],
];

const SHIPMENTS: &[&[&str]] = &[
    &["Item", "Crates", "Weight", "Balance"],
    &["Copper wire", "14", "2,310.5", "(45)"],
    &["Glass panes", "3", "87.25", "1,020"],
    &["Oak boards", "220", "14,002.0", "(7)"],
    &["Wool bales", "41", "506.75", "333"],
];

fn assert_only_table(md: &str, want: &[&[&str]]) {
    let ts = tables(md);
    assert_eq!(ts.len(), 1, "one table expected in:\n{md}");
    assert_eq!(ts[0], rows(want), "table cells in:\n{md}");
}

fn before(md: &str, a: &str, b: &str) -> bool {
    matches!((md.find(a), md.find(b)), (Some(i), Some(j)) if i < j)
}

// --- rotated text ------------------------------------------------------------------------------

#[test]
fn page_rotated_90_ruled_table_is_one_grid() {
    let p = pages("rot90_ruled");
    assert_only_table(&p[0], STATIONS);
    assert!(before(&p[0], "Spring gauging summary", "<table"), "{}", p[0]);
}

#[test]
fn page_rotated_270_borderless_table_is_one_grid() {
    let p = pages("rot270_borderless");
    assert_only_table(&p[0], SHIPMENTS);
    assert!(before(&p[0], "Warehouse shipments by item", "<table"), "{}", p[0]);
}

#[test]
fn rotated_region_on_upright_page_keeps_reading_order() {
    let p = pages("mixed_pages");
    assert_eq!(p.len(), 2);
    let page = &p[0];
    assert_only_table(page, STATIONS);
    assert!(page.starts_with("# Field report"), "{page}");
    assert!(before(page, "compares them with the autumn run.", "<table"), "{page}");
    assert!(before(page, "</table>", "Negative changes are shown"), "{page}");
    // the upright page after it is untouched
    assert_only_table(&p[1], SHIPMENTS);
}

// --- borderless tables -------------------------------------------------------------------------

#[test]
fn wide_spaced_borderless_table_with_ragged_numbers() {
    let p = pages("borderless_sparse");
    assert_only_table(&p[0], STATIONS);
    assert!(before(&p[0], "compares them with the autumn run.", "<table"), "{}", p[0]);
    assert!(before(&p[0], "</table>", "Negative changes are shown"), "{}", p[0]);
}

#[test]
fn decimal_aligned_borderless_table() {
    let p = pages("borderless_decimal");
    assert_only_table(
        &p[0],
        &[
            &["Lot", "Mass", "Spread"],
            &["North", "1.5", "0.125"],
            &["South", "128.25", "12.5"],
            &["East", "64.0", "(3.75)"],
            &["West", "7.125", "100.0"],
        ],
    );
}

#[test]
fn prose_columns_lists_and_number_columns_are_not_tables() {
    let p = pages("negatives");
    assert!(tables(&p[0]).is_empty(), "{}", p[0]);
    for w in ["Photograph the bank", "15,730", "automatic station records."] {
        assert!(p[0].contains(w), "{w} missing from {}", p[0]);
    }
}

// --- synthetic pages ---------------------------------------------------------------------------

/// A page of `lines` words, each its own line, in a grid of `cols` columns, every word rotated
/// by `rot` quarter turns. The geometry follows the producer: for rotation 1 the reading
/// direction is +y and lines advance toward -x, and the baseline is an x coordinate.
fn rotated_grid(rot: u8, cols: usize, nrows: usize) -> String {
    let (w, h) = (612.0, 792.0);
    let mut s = String::from("<doc>\n");
    s.push_str(&format!("<page width=\"{w}\" height=\"{h}\">\n<flow>\n<block>\n"));
    let (pitch, cw) = (720.0 / nrows as f64, 600.0 / cols as f64);
    for r in 0..nrows {
        for c in 0..cols {
            // reading frame: x along the row, y down the rows
            let (u0, u1) = (c as f64 * cw + 4.0, c as f64 * cw + 20.0);
            let (v0, v1) = (r as f64 * pitch + 2.0, r as f64 * pitch + 2.0 + 0.8 * pitch);
            let vb = v0 + 0.65 * pitch;
            let (x0, y0, x1, y1, base) = match rot {
                1 => (w - v1, u0, w - v0, u1, w - vb),
                2 => (w - u1, h - v1, w - u0, h - v0, h - vb),
                3 => (v0, h - u1, v1, h - u0, vb),
                _ => (u0, v0, u1, v1, vb),
            };
            s.push_str(&format!(
                "<line><word xMin=\"{x0}\" yMin=\"{y0}\" xMax=\"{x1}\" yMax=\"{y1}\" fontName=\"F\" fontSize=\"{fs}\" \
                 rotation=\"{rot}\" baseline=\"{base}\">{r}x{c}</word></line>\n",
                fs = 0.8 * pitch
            ));
        }
    }
    s.push_str("</block>\n</flow>\n</page>\n</doc>\n");
    s
}

#[test]
fn large_rotated_page_is_bounded() {
    // 19,800 one-cell lines, just under max_page_lines: the reframed page must hit the same
    // work limit (and plain-line fallback) as an upright one, quickly
    for rot in [1u8, 3] {
        let xhtml = rotated_grid(rot, 60, 330);
        let t = Instant::now();
        let (md, report) = to_markdown_report_with(&xhtml, &Params::default(), &HTML);
        let took = t.elapsed();
        assert!(took < Duration::from_secs(10), "rotation {rot}: {took:?}");
        assert_eq!(md.len(), 1);
        assert!(md[0].contains("329x59"), "rotation {rot}");
        assert!(report.page_work_max as f64 <= Params::default().max_page_work + 20_000.0, "rotation {rot}");
    }
}

#[test]
fn small_rotated_grid_reads_in_its_own_frame() {
    for rot in [1u8, 2, 3] {
        let (md, _) = to_markdown_report_with(&rotated_grid(rot, 3, 4), &Params::default(), &HTML);
        let want: Vec<Vec<String>> = (0..4).map(|r| (0..3).map(|c| format!("{r}x{c}")).collect()).collect();
        assert_eq!(tables(&md[0]), vec![want], "rotation {rot}:\n{}", md[0]);
    }
}

/// One upright page: each row's cells are separate lines on one baseline, `pitch` apart.
fn upright_rows(rows: &[&[(f64, &str)]], pitch: f64) -> String {
    let mut s = String::from("<doc>\n<page width=\"612\" height=\"792\">\n<flow>\n<block>\n");
    for (r, cells) in rows.iter().enumerate() {
        let base = 100.0 + r as f64 * pitch;
        for &(x, t) in cells.iter() {
            let x1 = x + 5.0 * t.chars().count() as f64;
            s.push_str(&format!(
                "<line><word xMin=\"{x}\" yMin=\"{}\" xMax=\"{x1}\" yMax=\"{}\" fontName=\"F\" fontSize=\"10\" \
                 baseline=\"{base}\">{t}</word></line>\n",
                base - 8.0,
                base + 2.0
            ));
        }
    }
    s.push_str("</block>\n</flow>\n</page>\n</doc>\n");
    s
}

#[test]
fn wide_spaced_numbered_list_with_lettered_item_is_not_a_table() {
    let xhtml = upright_rows(
        &[
            &[(72.0, "7."), (110.0, "Gather the samples")],
            &[(72.0, "8."), (110.0, "Label each jar")],
            &[(72.0, "8a."), (110.0, "Seal every lid")],
            &[(72.0, "9."), (110.0, "Ship the crate")],
        ],
        30.0,
    );
    let md = to_markdown_report_with(&xhtml, &Params::default(), &HTML).0;
    assert!(tables(&md[0]).is_empty(), "{}", md[0]);
}

#[test]
fn sparse_pass_can_be_turned_off() {
    let rows: &[&[(f64, &str)]] = &[
        &[(72.0, "Pump"), (200.0, "41"), (300.0, "(3)")],
        &[(72.0, "Valve"), (200.0, "7"), (300.0, "12")],
        &[(72.0, "Hose"), (200.0, "130"), (300.0, "(48)")],
    ];
    let xhtml = upright_rows(rows, 34.0);
    let on = to_markdown_report_with(&xhtml, &Params::default(), &HTML).0;
    assert_eq!(tables(&on[0]).len(), 1, "{}", on[0]);
    let mut p = Params::default();
    p.apply_json(r#"{"sparse_tables": false}"#).unwrap();
    let off = to_markdown_report_with(&xhtml, &p, &HTML).0;
    assert!(tables(&off[0]).is_empty(), "{}", off[0]);
}

/// Three very wide rows of one-word cells, `pitch` apart: every word is its own column.
fn wide_rows(per_row: usize, pitch: f64) -> String {
    let mut s = format!("<doc>\n<page width=\"{}\" height=\"792\">\n", per_row * 60 + 100);
    for r in 0..3 {
        let y = 20.0 + r as f64 * pitch;
        s.push_str("<flow><block><line>");
        for j in 0..per_row {
            let x = 20 + j * 60;
            s.push_str(&format!(
                "<word xMin=\"{x}\" yMin=\"{y}\" xMax=\"{}\" yMax=\"{}\" fontSize=\"9\" baseline=\"{}\">c{r}x{j}</word>",
                x + 20,
                y + 9.0,
                y + 8.0
            ));
        }
        s.push_str("</line></block></flow>\n");
    }
    s.push_str("</page>\n</doc>\n");
    s
}

#[test]
fn wide_rows_table_work_is_bounded_and_charged() {
    // 180,000 cells in 3 rows: grid building must stay near linear, and its work counts
    // against max_page_work
    let words = 3 * 60_000;
    for pitch in [12.0, 35.0] {
        let t = Instant::now();
        let (md, report) = to_markdown_report_with(&wide_rows(60_000, pitch), &Params::default(), &HTML);
        let took = t.elapsed();
        assert!(took < Duration::from_secs(5), "pitch {pitch}: {took:?}");
        assert!(md[0].contains("c2x59999"), "pitch {pitch}");
        assert!(report.page_work_max >= words as u64, "pitch {pitch}: {}", report.page_work_max);
    }
}

// --- borderless false positives and ragged rows -----------------------------------------------

/// A row's cells as (x, text) at 5 points per character.
fn md_of(rows: &[Vec<(f64, String)>], pitch: f64) -> String {
    let borrowed: Vec<Vec<(f64, &str)>> =
        rows.iter().map(|r| r.iter().map(|(x, t)| (*x, t.as_str())).collect()).collect();
    let refs: Vec<&[(f64, &str)]> = borrowed.iter().map(|r| r.as_slice()).collect();
    to_markdown_report_with(&upright_rows(&refs, pitch), &Params::default(), &HTML).0.join("\x0c")
}

fn row(cells: &[(f64, &str)]) -> Vec<(f64, String)> {
    cells.iter().map(|&(x, t)| (x, t.to_string())).collect()
}

#[test]
fn table_of_contents_without_leaders_is_not_a_table() {
    let titles = [
        "Getting started",
        "Field kits",
        "Calibration",
        "Sampling plans",
        "Storage",
        "Shipping",
        "Lab intake",
        "Quality checks",
        "Reporting",
        "Archiving",
        "Glossary",
        "Index",
    ];
    let rows: Vec<_> = titles
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let n = (i + 1).to_string();
            // one-digit numbers sit 13 points before the title, two-digit ones 8 (one cell)
            let page = (3 + 7 * i).to_string();
            vec![(72.0, n.clone()), (90.0, t.to_string()), (530.0 - 5.0 * page.len() as f64, page)]
        })
        .collect();
    let md = md_of(&rows, 24.0);
    assert!(tables(&md).is_empty(), "{md}");
}

#[test]
fn widely_spaced_footnotes_are_not_a_table() {
    let notes = [
        "Readings were taken before the thaw began.",
        "The north gauge was replaced in March.",
        "Values are rounded to the nearest tenth.",
        "Two sites were visited twice that week.",
    ];
    let rows: Vec<_> =
        notes.iter().enumerate().map(|(i, t)| vec![(72.0, (i + 1).to_string()), (90.0, t.to_string())]).collect();
    let md = md_of(&rows, 30.0);
    assert!(tables(&md).is_empty(), "{md}");
}

#[test]
fn verse_numbered_on_every_line_is_not_a_table() {
    let verse = [
        "The ferry waits beside the reeds",
        "while herons count the slow grey tide",
        "and every lamp along the quay",
        "goes out before the bells have died",
        "so morning finds the river wide",
    ];
    let rows: Vec<_> =
        verse.iter().enumerate().map(|(i, t)| vec![(72.0, t.to_string()), (400.0, (i + 1).to_string())]).collect();
    let md = md_of(&rows, 26.0);
    assert!(tables(&md).is_empty(), "{md}");
}

#[test]
fn pleading_paper_line_numbers_are_not_a_table() {
    let rows: Vec<_> = (1..=28)
        .map(|i| {
            let n = i.to_string();
            let x = 50.0 - 5.0 * n.len() as f64; // right-aligned in the margin
            if i % 4 == 0 {
                vec![(x, n)] // a blank pleading line
            } else {
                vec![(x, n), (90.0, format!("The parties agree that clause {i} applies to the lease."))]
            }
        })
        .collect();
    let md = md_of(&rows, 24.0);
    assert!(tables(&md).is_empty(), "{md}");
}

#[test]
fn ragged_wide_spaced_table_is_one_whole_table() {
    // empty cells in the middle column: every row still lands in the shared columns
    let rows = vec![
        row(&[(72.0, "Code"), (160.0, "Form"), (260.0, "Ref"), (360.0, "Filed")]),
        row(&[(72.0, "4.1"), (160.0, "S-3"), (260.0, "4.2"), (360.0, "1/12/21")]),
        row(&[(72.0, "4.2"), (260.0, "4.7"), (360.0, "6/30/21")]),
        row(&[(72.0, "4.3"), (160.0, "8-K"), (260.0, "4.1"), (360.0, "2/2/22")]),
        row(&[(72.0, "4.4"), (260.0, "4.9"), (360.0, "9/9/22")]),
        row(&[(72.0, "4.5"), (360.0, "3/3/23")]),
    ];
    let md = md_of(&rows, 30.0);
    let want: Vec<Vec<String>> = vec![
        vec!["Code", "Form", "Ref", "Filed"],
        vec!["4.1", "S-3", "4.2", "1/12/21"],
        vec!["4.2", "", "4.7", "6/30/21"],
        vec!["4.3", "8-K", "4.1", "2/2/22"],
        vec!["4.4", "", "4.9", "9/9/22"],
        vec!["4.5", "", "", "3/3/23"],
    ]
    .into_iter()
    .map(|r| r.into_iter().map(String::from).collect())
    .collect();
    assert_eq!(tables(&md), vec![want], "{md}");
}

#[test]
fn a_title_above_a_sparse_table_is_never_stuffed_into_one_cell() {
    // a short line just above the table whose words fall between its columns
    let rows: &[&[(f64, &str)]] = &[
        &[(72.0, "Pump"), (220.0, "41.5"), (300.0, "(3.25)")],
        &[(72.0, "Valve"), (220.0, "7.0"), (300.0, "12.75")],
        &[(72.0, "Hose"), (220.0, "130.5"), (300.0, "(48.0)")],
    ];
    let title = "<line><word xMin=\"120\" yMin=\"78\" xMax=\"150\" yMax=\"88\" fontName=\"F\" fontSize=\"10\" \
                 baseline=\"86\">Stock</word><word xMin=\"153\" yMin=\"78\" xMax=\"163\" yMax=\"88\" \
                 fontName=\"F\" fontSize=\"10\" baseline=\"86\">on</word><word xMin=\"166\" yMin=\"78\" \
                 xMax=\"190\" yMax=\"88\" fontName=\"F\" fontSize=\"10\" baseline=\"86\">hand</word></line>\n";
    let xhtml = upright_rows(rows, 30.0).replacen("<block>\n", &format!("<block>\n{title}"), 1);
    let md = to_markdown_report_with(&xhtml, &Params::default(), &HTML).0.join("\x0c");
    assert!(!md.contains("<th>Stock on hand</th>"), "{md}");
    assert!(md.contains("Stock on hand"), "{md}");
    assert_eq!(tables(&md).len(), 1, "{md}");
}

#[test]
fn a_stray_upright_line_inside_a_rotated_table_stays_upright() {
    // a stamp in the middle of a sideways table: the table still forms, the stamp is kept
    let stamp = "<line><word xMin=\"300\" yMin=\"390\" xMax=\"340\" yMax=\"400\" fontName=\"F\" \
                 fontSize=\"10\" rotation=\"0\" baseline=\"398\">COPY</word></line>\n";
    for rot in [1u8, 3] {
        let xhtml = rotated_grid(rot, 3, 4).replacen("<block>\n", &format!("<block>\n{stamp}"), 1);
        let (md, _) = to_markdown_report_with(&xhtml, &Params::default(), &HTML);
        let want: Vec<Vec<String>> = (0..4).map(|r| (0..3).map(|c| format!("{r}x{c}")).collect()).collect();
        assert_eq!(tables(&md[0]), vec![want], "rotation {rot}:\n{}", md[0]);
        assert!(md[0].contains("COPY"), "rotation {rot}:\n{}", md[0]);
    }
}

/// Word texts in the input (`>text</word>`) and their counts.
fn input_words(xhtml: &str) -> std::collections::BTreeMap<String, usize> {
    let mut m = std::collections::BTreeMap::new();
    for chunk in xhtml.split("</word>").filter(|c| c.contains("<word")) {
        *m.entry(chunk.rsplit('>').next().unwrap_or("").to_string()).or_insert(0) += 1;
    }
    m
}

/// A page of lines given in the reading frame of rotation `rot`: each word is (u0, u1, v, text)
/// with u along the line and v its top; boxes are 10 high. Mapped to page coordinates as the
/// producer writes them (an inverted u interval stays inverted).
fn rotated_page(rot: u8, lines: &[Vec<(f64, f64, f64, String)>]) -> String {
    let (w, h) = (612.0, 792.0);
    let mut s = format!("<doc>\n<page width=\"{w}\" height=\"{h}\">\n<flow>\n<block>\n");
    for words in lines {
        s.push_str("<line>");
        for (u0, u1, v0, t) in words {
            let (v1, vb) = (v0 + 10.0, v0 + 8.0);
            let (x0, y0, x1, y1, base) = match rot {
                1 => (w - v1, *u0, w - v0, *u1, w - vb),
                2 => (w - u1, h - v1, w - u0, h - v0, h - vb),
                3 => (*v0, h - u1, v1, h - u0, vb),
                _ => (*u0, *v0, *u1, v1, vb),
            };
            s.push_str(&format!(
                "<word xMin=\"{x0}\" yMin=\"{y0}\" xMax=\"{x1}\" yMax=\"{y1}\" fontName=\"F\" fontSize=\"10\" \
                 rotation=\"{rot}\" baseline=\"{base}\">{t}</word>"
            ));
        }
        s.push_str("</line>\n");
    }
    s.push_str("</block>\n</flow>\n</page>\n</doc>\n");
    s
}

#[test]
fn inverted_word_boxes_in_rotated_text_are_not_duplicated() {
    // two columns of rotated prose and a one-word line whose box the producer wrote inverted
    // (u0 > u1), reaching across the column gutter: every word must come out once
    let mut lines = Vec::new();
    for r in 0..8 {
        for (c, u) in [(0, 40.0), (1, 330.0)] {
            let v = 60.0 + 14.0 * r as f64;
            lines.push(
                (0..6).map(|k| (u + 36.0 * k as f64, u + 36.0 * k as f64 + 30.0, v, format!("c{c}r{r}k{k}"))).collect(),
            );
        }
    }
    lines.push(vec![(560.0, 120.0, 60.0 + 14.0 * 8.0, "flipped".to_string())]);
    for rot in [1u8, 2, 3] {
        let xhtml = rotated_page(rot, &lines);
        let md = to_markdown_report_with(&xhtml, &Params::default(), &HTML).0.join("\x0c");
        for (w, n) in input_words(&xhtml) {
            let got = md.matches(w.as_str()).count();
            assert_eq!(got, n, "rotation {rot}: {w} x{got}\n{md}");
        }
    }
}

#[test]
fn rotated_stray_share_zero_tolerates_no_stray_line() {
    let stamp = "<line><word xMin=\"300\" yMin=\"390\" xMax=\"340\" yMax=\"400\" fontName=\"F\" \
                 fontSize=\"10\" rotation=\"0\" baseline=\"398\">COPY</word></line>\n";
    let xhtml = rotated_grid(1, 3, 4).replacen("<block>\n", &format!("<block>\n{stamp}"), 1);
    let mut p = Params::default();
    p.apply_json(r#"{"rotated_stray_share": 0}"#).unwrap();
    let (md, _) = to_markdown_report_with(&xhtml, &p, &HTML);
    assert!(tables(&md[0]).is_empty(), "{}", md[0]);
}

/// One upright page of rows at the given baselines.
fn rows_at(rows: &[(f64, &[(f64, &str)])]) -> String {
    let mut s = String::from("<doc>\n<page width=\"612\" height=\"792\">\n<flow>\n<block>\n");
    for &(base, cells) in rows {
        for &(x, t) in cells {
            let x1 = x + 5.0 * t.chars().count() as f64;
            s.push_str(&format!(
                "<line><word xMin=\"{x}\" yMin=\"{}\" xMax=\"{x1}\" yMax=\"{}\" fontName=\"F\" fontSize=\"10\" \
                 baseline=\"{base}\">{t}</word></line>\n",
                base - 8.0,
                base + 2.0
            ));
        }
    }
    s.push_str("</block>\n</flow>\n</page>\n</doc>\n");
    s
}

const STOCK: [&[(f64, &str)]; 4] = [
    &[(72.0, "Pump"), (220.0, "41"), (300.0, "(3)")],
    &[(72.0, "Valve"), (220.0, "7"), (300.0, "12")],
    &[(72.0, "Hose"), (220.0, "130"), (300.0, "(48)")],
    &[(72.0, "Filter"), (220.0, "9"), (300.0, "5")],
];

fn stock_then(trailer: &[(f64, &str)], gap: f64) -> Vec<Vec<String>> {
    let mut rows: Vec<(f64, &[(f64, &str)])> =
        STOCK.iter().enumerate().map(|(i, r)| (100.0 + 28.0 * i as f64, *r)).collect();
    rows.push((100.0 + 28.0 * 3.0 + gap, trailer));
    let md = to_markdown_report_with(&rows_at(&rows), &Params::default(), &HTML).0.join("\x0c");
    let ts = tables(&md);
    assert_eq!(ts.len(), 1, "gap {gap}:\n{md}");
    ts.into_iter().next().unwrap()
}

#[test]
fn a_total_row_after_a_wider_gap_joins_the_table() {
    for gap in [36.0, 44.0] {
        let t = stock_then(&[(72.0, "Total"), (220.0, "187"), (300.0, "(34)")], gap);
        assert_eq!(t.len(), 5, "gap {gap}: {t:?}");
        assert_eq!(t[4], ["Total", "187", "(34)"], "gap {gap}");
    }
}

#[test]
fn a_trailing_two_cell_line_after_a_wider_gap_keeps_the_table() {
    // a sign-off line after the gap: the table keeps its four rows and leaves the line out
    let t = stock_then(&[(72.0, "Prepared by"), (220.0, "J. Smith")], 40.0);
    assert_eq!(t.len(), 4, "{t:?}");
    assert_eq!(t[3], ["Filter", "9", "5"]);
}

#[test]
fn a_far_row_that_fits_the_columns_does_not_veto_the_table() {
    let t = stock_then(&[(72.0, "Page"), (220.0, "3")], 300.0);
    assert_eq!(t.len(), 4, "{t:?}");
}

/// Tables in an upright page of evenly spaced rows.
fn tables_of(rows: &[&[(f64, &str)]], pitch: f64) -> Vec<Vec<Vec<String>>> {
    let md = to_markdown_report_with(&upright_rows(rows, pitch), &Params::default(), &HTML).0.join("\x0c");
    tables(&md)
}

#[test]
fn contents_with_a_header_row_or_page_ranges_is_not_a_table() {
    let header: &[&[(f64, &str)]] = &[
        &[(72.0, "Section"), (500.0, "Page")],
        &[(72.0, "Field kits"), (500.0, "3")],
        &[(72.0, "Calibration"), (500.0, "9")],
        &[(72.0, "Sampling plans"), (500.0, "14")],
        &[(72.0, "Storage"), (500.0, "22")],
    ];
    assert!(tables_of(header, 24.0).is_empty());
    let ranges: &[&[(f64, &str)]] = &[
        &[(72.0, "1"), (90.0, "Field kits"), (500.0, "1-4")],
        &[(72.0, "2"), (90.0, "Calibration"), (500.0, "5-11")],
        &[(72.0, "3"), (90.0, "Sampling plans"), (500.0, "12-19")],
        &[(72.0, "4"), (90.0, "Storage"), (500.0, "20-23")],
    ];
    assert!(tables_of(ranges, 24.0).is_empty());
}

#[test]
fn footnotes_with_symbol_or_letter_markers_or_short_notes_are_not_tables() {
    let symbols: &[&[(f64, &str)]] = &[
        &[(72.0, "*"), (90.0, "Readings were taken before the thaw.")],
        &[(72.0, "†"), (90.0, "The north gauge was replaced.")],
        &[(72.0, "‡"), (90.0, "Rounded to the nearest tenth.")],
    ];
    assert!(tables_of(symbols, 30.0).is_empty());
    let letters: &[&[(f64, &str)]] = &[
        &[(72.0, "a"), (90.0, "Adjusted for the dry spell.")],
        &[(72.0, "b"), (90.0, "Two sites visited twice.")],
        &[(72.0, "c"), (90.0, "Estimated from the logs.")],
    ];
    assert!(tables_of(letters, 30.0).is_empty());
    let legal: &[&[(f64, &str)]] = &[
        &[(72.0, "1"), (90.0, "Id.")],
        &[(72.0, "2"), (90.0, "Id. at 4.")],
        &[(72.0, "3"), (90.0, "Ibid.")],
        &[(72.0, "4"), (90.0, "Id. at 9.")],
    ];
    assert!(tables_of(legal, 30.0).is_empty());
}

#[test]
fn pleading_captions_beside_line_numbers_are_not_tables() {
    let two_col: &[&[(f64, &str)]] = &[
        &[(40.0, "1"), (90.0, "JANE ROE,"), (340.0, "Case No. 26-CV-0417")],
        &[(40.0, "2"), (90.0, "Plaintiff,"), (340.0, "COMPLAINT FOR")],
        &[(40.0, "3"), (90.0, "v."), (340.0, "DAMAGES")],
        &[(40.0, "4"), (90.0, "RIVERSIDE LLC,"), (340.0, "JURY DEMANDED")],
        &[(40.0, "5"), (90.0, "Defendant."), (340.0, "Dept. 12")],
    ];
    assert!(tables_of(two_col, 24.0).is_empty());
    let one_col: &[&[(f64, &str)]] = &[
        &[(40.0, "1"), (90.0, "COMPLAINT")],
        &[(40.0, "2"), (90.0, "JANE ROE,")],
        &[(40.0, "3"), (90.0, "Plaintiff,")],
        &[(40.0, "4"), (90.0, "v.")],
        &[(40.0, "5"), (90.0, "RIVERSIDE LLC,")],
        &[(40.0, "6"), (90.0, "Defendant.")],
        &[(40.0, "7"), (90.0, "INTRODUCTION")],
    ];
    assert!(tables_of(one_col, 24.0).is_empty());
}

#[test]
fn real_tables_survive_the_guards() {
    let rank: &[&[(f64, &str)]] = &[
        &[(72.0, "1"), (100.0, "Harbor Mutual Savings Association"), (400.0, "2,450")],
        &[(72.0, "2"), (100.0, "First Prairie Trust Company"), (400.0, "1,980")],
        &[(72.0, "3"), (100.0, "Northgate Cooperative Bank"), (400.0, "1,105")],
    ];
    assert_eq!(tables_of(rank, 30.0).len(), 1, "rank");
    let chronology: &[&[(f64, &str)]] = &[
        &[(72.0, "Mill opened on the river"), (400.0, "1871")],
        &[(72.0, "Footbridge rebuilt"), (400.0, "1902")],
        &[(72.0, "Gauge station added"), (400.0, "1958")],
        &[(72.0, "Towpath reopened"), (400.0, "2011")],
    ];
    assert_eq!(tables_of(chronology, 30.0).len(), 1, "chronology");
    let prices: &[&[(f64, &str)]] = &[
        &[(72.0, "Coffee"), (300.0, "3")],
        &[(72.0, "Tea"), (300.0, "3")],
        &[(72.0, "Scone"), (300.0, "4")],
        &[(72.0, "Pie"), (300.0, "6")],
    ];
    assert_eq!(tables_of(prices, 30.0).len(), 1, "prices");
}

#[test]
fn two_column_checklist_is_not_a_table() {
    // checkbox widgets are form fields, absent from the text layer: only the items remain
    let xhtml = upright_rows(
        &[
            &[(88.0, "Has a valid passport"), (330.0, "Receives child benefit")],
            &[(88.0, "Changing schools"), (330.0, "Owns a registered seal")],
            &[(88.0, "Holds a care certificate"), (330.0, "Updating the ID card")],
        ],
        40.0,
    );
    let md = to_markdown_report_with(&xhtml, &Params::default(), &HTML).0;
    assert!(tables(&md[0]).is_empty(), "{}", md[0]);
    assert!(before(&md[0], "Has a valid passport", "Updating the ID card"), "{}", md[0]);
}

#[test]
fn checkbox_glyph_columns_are_not_a_table() {
    let xhtml = upright_rows(
        &[
            &[(72.0, "☐ Passport"), (330.0, "☐ Utility bill")],
            &[(72.0, "☑ Birth certificate"), (330.0, "☐ Bank statement 2025")],
            &[(72.0, "☐ Driving licence"), (330.0, "☒ Tax return 2024")],
        ],
        40.0,
    );
    let md = to_markdown_report_with(&xhtml, &Params::default(), &HTML).0;
    assert!(tables(&md[0]).is_empty(), "{}", md[0]);
}

#[test]
fn identifier_and_description_columns_stay_a_table() {
    let rows: &[&[(f64, &str)]] = &[
        &[(72.0, "PS_DISTILL"), (200.0, "Converts a stream to a document.")],
        &[(72.0, "CREATE_XPS"), (200.0, "Writes a copy of the document.")],
        &[(72.0, "SAVE_RESULT"), (200.0, "Saves the result to disk.")],
    ];
    let md = to_markdown_report_with(&upright_rows(rows, 34.0), &Params::default(), &HTML).0;
    assert_eq!(tables(&md[0]).len(), 1, "{}", md[0]);
}
