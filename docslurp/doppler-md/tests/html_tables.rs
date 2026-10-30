//! HTML table output (`TableFormat::Html`): the `grid_html` writer and its use in page layout.
//! Prose stays Markdown, only table blocks change, and the default (GFM) output is unchanged.

use std::path::Path;

use doppler_md::tables::{Grid, grid_html, grid_markdown};
use doppler_md::{Options, Params, TableFormat, to_markdown_report, to_markdown_report_with};

fn grid(rows: &[&[&str]]) -> Grid {
    rows.iter().map(|r| r.iter().map(|c| c.to_string()).collect()).collect()
}

fn cells(row: &[&str]) -> Vec<String> {
    row.iter().map(|c| c.to_string()).collect()
}

const HTML: Options = Options { tables: TableFormat::Html };

/// A CommonMark type-6 HTML block that starts with `<table` ends at the first blank line.
fn assert_one_html_block(table: &str) {
    assert!(table.starts_with("<table") && table.ends_with("</table>"), "{table}");
    assert!(table.lines().all(|l| !l.trim().is_empty()), "blank line inside {table:?}");
}

// --- grid_html -------------------------------------------------------------------------------

#[test]
fn no_header_means_no_thead() {
    let html = grid_html(None, &grid(&[&["Apples", "1.0"], &["Pears", "2.0"]]));
    assert_eq!(
        html,
        "<table>\n<tbody>\n<tr><td>Apples</td><td>1.0</td></tr>\n<tr><td>Pears</td><td>2.0</td></tr>\n</tbody>\n</table>"
    );
    assert_one_html_block(&html);
}

#[test]
fn detected_header_is_thead() {
    let header = cells(&["Fruit", "Price"]);
    let html = grid_html(Some(&header), &grid(&[&["Apples", "1.0"]]));
    assert_eq!(
        html,
        concat!(
            "<table>\n<thead><tr><th>Fruit</th><th>Price</th></tr></thead>\n",
            "<tbody>\n<tr><td>Apples</td><td>1.0</td></tr>\n</tbody>\n</table>"
        )
    );
}

#[test]
fn header_only_has_no_tbody() {
    let header = cells(&["Fruit", "Price"]);
    assert_eq!(
        grid_html(Some(&header), &Grid::new()),
        "<table>\n<thead><tr><th>Fruit</th><th>Price</th></tr></thead>\n</table>"
    );
}

#[test]
fn empty_grid_is_empty() {
    assert_eq!(grid_html(None, &Grid::new()), "");
    assert_eq!(grid_markdown(&Grid::new()), ""); // the same as GFM, so the block is dropped
}

#[test]
fn empty_cells_stay_cells() {
    let header = cells(&["", "2025"]);
    assert_eq!(
        grid_html(Some(&header), &grid(&[&["Tax", ""]])),
        concat!(
            "<table>\n<thead><tr><th></th><th>2025</th></tr></thead>\n",
            "<tbody>\n<tr><td>Tax</td><td></td></tr>\n</tbody>\n</table>"
        )
    );
}

#[test]
fn cells_are_escaped_once() {
    let html = grid_html(None, &grid(&[&["AT&T", "a<b>c"], &["say \"hi\"", "&amp; | \\"]]));
    assert!(html.contains("<tr><td>AT&amp;T</td><td>a&lt;b&gt;c</td></tr>"), "{html}");
    // a cell that literally reads `&amp;` keeps reading `&amp;`; pipes and backslashes are not
    // special in HTML (unlike GFM)
    assert!(html.contains("<tr><td>say &quot;hi&quot;</td><td>&amp;amp; | \\</td></tr>"), "{html}");
    // markup in a cell cannot close the block or the table
    let html = grid_html(None, &grid(&[&["</table>", "<!-- x -->"], &["b", "c"]]));
    assert!(html.contains("<td>&lt;/table&gt;</td><td>&lt;!-- x --&gt;</td>"), "{html}");
    assert_eq!(html.matches("</table>").count(), 1);
}

#[test]
fn rtl_table_reverses_cells_like_gfm() {
    // grid columns are geometric (left to right): the rightmost column is read first
    let rows = grid(&[&["סכום", "פירוט"], &["₪6,609.10", "עסקאות בארץ"]]);
    let html = grid_html(None, &rows);
    assert_eq!(
        html,
        concat!(
            "<table dir=\"rtl\">\n<tbody>\n<tr><td>פירוט</td><td>סכום</td></tr>\n",
            "<tr><td>עסקאות בארץ</td><td>₪6,609.10</td></tr>\n</tbody>\n</table>"
        )
    );
    assert_eq!(grid_markdown(&rows), "| פירוט | סכום |\n| --- | --- |\n| עסקאות בארץ | ₪6,609.10 |");
}

#[test]
fn rtl_test_counts_the_header() {
    // the body alone is mostly left to right; the header tips the table to right to left,
    // exactly as GFM decides it with the header as its first row
    let header = cells(&["סכום כולל", "פירוט החיוב"]);
    let rows = grid(&[&["12", "AB"]]);
    let html = grid_html(Some(&header), &rows);
    assert_eq!(
        html,
        concat!(
            "<table dir=\"rtl\">\n<thead><tr><th>פירוט החיוב</th><th>סכום כולל</th></tr></thead>\n",
            "<tbody>\n<tr><td>AB</td><td>12</td></tr>\n</tbody>\n</table>"
        )
    );
    let mut with_header = vec![header];
    with_header.extend(rows);
    assert!(grid_markdown(&with_header).starts_with("| פירוט החיוב | סכום כולל |"));
}

#[test]
fn mostly_left_to_right_table_has_no_dir() {
    let html = grid_html(None, &grid(&[&["Account", "שם"], &["Balance", "12"]]));
    assert!(html.starts_with("<table>\n"), "{html}");
    assert!(html.contains("<tr><td>Account</td><td>שם</td></tr>"), "{html}");
}

#[test]
fn ragged_rows_keep_their_cells() {
    let html = grid_html(None, &grid(&[&["a", "b", "c"], &["d"]]));
    assert!(html.contains("<tr><td>a</td><td>b</td><td>c</td></tr>\n<tr><td>d</td></tr>"), "{html}");
}

// --- page layout -------------------------------------------------------------------------------

fn word(x0: f64, x1: f64, y: f64, text: &str) -> String {
    font_word(x0, x1, y, text, "Serif")
}

fn font_word(x0: f64, x1: f64, y: f64, text: &str, font: &str) -> String {
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

/// GFM and HTML pages for one input, after checking GFM is the default output.
fn both(xhtml: &str) -> (Vec<String>, Vec<String>) {
    let p = Params::default();
    let (gfm, gr) = to_markdown_report_with(xhtml, &p, &Options { tables: TableFormat::Gfm });
    let (default, dr) = to_markdown_report(xhtml, &p);
    assert_eq!(gfm, default);
    assert_eq!(gr.to_json(), dr.to_json());
    let (html, hr) = to_markdown_report_with(xhtml, &p, &HTML);
    assert_eq!(hr.degraded, dr.degraded);
    assert!(hr.doc_work >= dr.doc_work);
    (gfm, html)
}

/// Blocks of a page with each table replaced by a marker, and the tables.
fn split_tables(page: &str, is_table: impl Fn(&str) -> bool) -> (Vec<String>, Vec<String>) {
    let mut prose = Vec::new();
    let mut tables = Vec::new();
    for b in page.split("\n\n") {
        if is_table(b) {
            prose.push("<table>".to_string());
            tables.push(b.to_string());
        } else {
            prose.push(b.to_string());
        }
    }
    (prose, tables)
}

/// The prose around the tables is identical; returns the HTML tables.
fn same_prose(gfm: &str, html: &str) -> Vec<String> {
    let (gp, gt) =
        split_tables(gfm, |b| b.starts_with("| ") && b.lines().nth(1).is_some_and(|l| l.starts_with("| ---")));
    let (hp, ht) = split_tables(html, |b| b.starts_with("<table"));
    assert_eq!(gp, hp, "prose differs:\n{gfm}\n---\n{html}");
    assert_eq!(gt.len(), ht.len());
    assert!(!ht.is_empty());
    for t in &ht {
        assert_one_html_block(t);
    }
    ht
}

#[test]
fn gutter_table_without_header() {
    let mut body = line(&word(20.0, 70.0, 84.0, "Stock"));
    for (y, label, amount) in [(100.0, "Apples", "1.0"), (120.0, "Pears", "2.0"), (140.0, "Plums", "3.0")] {
        body += &line(&(word(20.0, 60.0, y, label) + &word(160.0, 190.0, y, amount)));
        if y == 100.0 {
            body += &line(&word(160.0, 195.0, 112.0, "crates"));
        }
    }
    body += &line(&word(20.0, 300.0, 200.0, "Prices are per crate &amp; final."));
    let (gfm, html) = both(&page(&body));
    assert_eq!(
        gfm,
        [
            "Stock\n\n| Apples | 1.0 crates |\n| --- | --- |\n| Pears | 2.0 |\n| Plums | 3.0 |\n\nPrices are per crate & final."
        ]
    );
    let tables = same_prose(&gfm[0], &html[0]);
    assert_eq!(
        tables,
        [concat!(
            "<table>\n<tbody>\n<tr><td>Apples</td><td>1.0 crates</td></tr>\n",
            "<tr><td>Pears</td><td>2.0</td></tr>\n<tr><td>Plums</td><td>3.0</td></tr>\n</tbody>\n</table>"
        )]
    );
    // prose is Markdown text, not escaped HTML
    assert!(html[0].ends_with("\n\nPrices are per crate & final."));
}

#[test]
fn ruled_tables_with_detected_headers() {
    let mut body = line(&font_word(20.0, 100.0, 70.0, "Overview", "Serif-Bold"));
    for (top, title) in [(100.0, "First stock"), (180.0, "Second stock")] {
        body += &line(&word(20.0, 100.0, top - 14.0, title));
        for y in [top, top + 20.0, top + 40.0] {
            body += &rule(20.0, 220.0, y, y + 0.5);
        }
        body += &rule(120.0, 120.5, top, top + 40.0);
        for (i, (label, amount)) in [("A &lt;b&gt;", "1.0"), ("Pears", "2.0")].into_iter().enumerate() {
            let y = top + 4.0 + 20.0 * i as f64;
            body += &line(&(word(25.0, 70.0, y, label) + &word(125.0, 160.0, y, amount)));
        }
    }
    let (gfm, html) = both(&page(&body));
    let tables = same_prose(&gfm[0], &html[0]);
    assert_eq!(html[0].split("\n\n").next(), Some("Overview"));
    for (t, title) in tables.iter().zip(["First stock", "Second stock"]) {
        assert_eq!(
            *t,
            format!(
                concat!(
                    "<table>\n<thead><tr><th>{}</th><th></th></tr></thead>\n<tbody>\n",
                    "<tr><td>A &lt;b&gt;</td><td>1.0</td></tr>\n<tr><td>Pears</td><td>2.0</td></tr>\n</tbody>\n</table>"
                ),
                title
            )
        );
    }
}

#[test]
fn rtl_gutter_table_reads_right_to_left() {
    // Hebrew labels on the right, amounts on the left (visual positions)
    let mut body = String::new();
    for (y, amount, label) in
        [(100.0, "₪6,609.10", "עסקאות"), (120.0, "₪1,400.21", "חיובים"), (140.0, "₪8,009.31", "סה״כ")]
    {
        body += &line(&(word(20.0, 80.0, y, amount) + &word(200.0, 250.0, y, label)));
    }
    let (gfm, html) = both(&page(&body));
    let tables = same_prose(&gfm[0], &html[0]);
    assert!(gfm[0].starts_with("| עסקאות | ₪6,609.10 |"), "{}", gfm[0]);
    assert_eq!(
        tables,
        [concat!(
            "<table dir=\"rtl\">\n<tbody>\n<tr><td>עסקאות</td><td>₪6,609.10</td></tr>\n",
            "<tr><td>חיובים</td><td>₪1,400.21</td></tr>\n<tr><td>סה״כ</td><td>₪8,009.31</td></tr>\n</tbody>\n</table>"
        )]
    );
}

#[test]
fn line_breaks_in_word_text_cannot_split_the_html_block() {
    // Referenced and literal line/paragraph separators in a word become spaces and other
    // controls are removed (model::word_text), so no cell can put a blank line in the table.
    // (`&#x85;` is the HTML5 Windows-1252 remap to an ellipsis, not NEL.)
    let breaks = [
        ("a&#10;&#10;b", "a  b"),
        ("c&#x2028;&#8233;d", "c  d"),
        ("e&#13;&#10;&#13;&#10;f", "e    f"),
        ("g\u{2028}\u{2029}h", "g  h"),
        ("i\r\r\u{85}j", "i   j"),
        ("k&#x85;&#11;&#12;l", "k\u{2026}l"),
    ];
    let mut body = String::new();
    for (i, (text, _)) in breaks.iter().enumerate() {
        let y = 100.0 + 20.0 * i as f64;
        body += &line(&(word(20.0, 60.0, y, text) + &word(160.0, 190.0, y, &format!("{i}.5"))));
    }
    let (gfm, html) = both(&page(&body));
    let tables = same_prose(&gfm[0], &html[0]);
    assert_eq!(tables.len(), 1);
    assert_eq!(tables[0].lines().count(), 4 + breaks.len(), "{}", tables[0]);
    for (i, (_, want)) in breaks.iter().enumerate() {
        let row = format!("<tr><td>{want}</td><td>{i}.5</td></tr>");
        assert!(tables[0].contains(&row), "{row} in {}", tables[0]);
    }
}

#[test]
fn a_word_with_a_literal_newline_is_skipped() {
    // the parser's `<word>(.*?)</word>` does not span a newline, so such a word never
    // reaches a cell
    let mut body = String::new();
    for (i, label) in ["Apples", "Pears", "Plums"].into_iter().enumerate() {
        let y = 100.0 + 20.0 * i as f64;
        let mut words = word(20.0, 60.0, y, label) + &word(160.0, 190.0, y, &format!("{i}.5"));
        if i == 1 {
            words += &word(192.0, 200.0, y, "Xq\n\nZw");
        }
        body += &line(&words);
    }
    let (gfm, html) = both(&page(&body));
    let tables = same_prose(&gfm[0], &html[0]);
    assert!(tables[0].contains("<tr><td>Pears</td><td>1.5</td></tr>"), "{}", tables[0]);
    assert!(!html[0].contains("Xq") && !html[0].contains("Zw"), "{}", html[0]);
}

fn fixture(name: &str) -> String {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    std::fs::read_to_string(dir.join(format!("{name}.xhtml"))).unwrap()
}

#[test]
fn fixtures_change_only_their_tables() {
    for name in ["ruled_table", "gutter_table", "rtl_table"] {
        let (gfm, html) = both(&fixture(name));
        assert_eq!(gfm.len(), html.len());
        let mut tables = 0;
        for (g, h) in gfm.iter().zip(&html) {
            if g.contains("\n| --- |") {
                tables += same_prose(g, h).len();
            } else {
                assert_eq!(g, h, "{name}: a page without tables is unchanged");
            }
        }
        assert!(tables > 0, "{name} has a table");
    }
}

/// Closed 3x3 source geometry, with the upper-left 2x2 cells merged.
#[test]
fn public_combined_span_preserves_cells_and_pages() {
    let mut body = String::new();
    for (x0, y0, x1, y1) in [
        (20., 100., 320., 100.),
        (20., 400., 320., 400.),
        (20., 100., 20., 400.),
        (320., 100., 320., 400.),
        (220., 100., 220., 400.),
        (120., 300., 120., 400.),
        (20., 300., 320., 300.),
        (220., 200., 320., 200.),
    ] {
        body += &rule(x0, x1, 100. + (y0 - 100.) / 5., 100. + (y1 - 100.) / 5.);
    }
    for (x, y, t) in [
        (30., 110., "Merged"),
        (230., 110., "RightOne"),
        (230., 210., "RightTwo"),
        (30., 310., "BottomOne"),
        (130., 310., "BottomTwo"),
        (230., 310., "BottomThree"),
    ] {
        body += &line(&word(x, x + 50., 100. + (y - 100.) / 5., t));
    }
    let input = page(&body) + &page(&line(&word(20., 100., 100., "NextPage")));
    let (_, html) = both(&input);
    let low = Params { max_page_work: 1., ..Params::default() };
    let (plain, report) = to_markdown_report_with(&input, &low, &HTML);
    assert_eq!(report.degraded[0], (0, doppler_md::Degraded::PageWork));
    assert!(!plain[0].contains("<table"));
    assert!(plain[0].contains("Merged"));
    let single = page(&body);
    let (_, one) = to_markdown_report_with(&single, &Params::default(), &HTML);
    let doc = Params { max_doc_work: one.doc_work as f64, ..Params::default() };
    let (_, report) = to_markdown_report_with(&(single.clone() + &single), &doc, &HTML);
    assert_eq!(report.degraded, vec![(1, doppler_md::Degraded::DocWork)]);
    let (_, gfm) = to_markdown_report(&single, &Params::default());
    assert!(one.page_work_max > gfm.page_work_max);
    let capped = Params { max_page_work: gfm.page_work_max as f64, ..Params::default() };
    assert!(to_markdown_report(&single, &capped).1.degraded.is_empty());
    assert!(!to_markdown_report_with(&single, &capped, &HTML).1.degraded.is_empty());
    let dense = page(&(body.clone() + &line(&word(30., 80., 102., "Extra")).repeat(20_000)));
    assert!(!to_markdown_report_with(&dense, &Params::default(), &HTML).1.degraded.is_empty());

    assert_eq!(html.len(), 2);
    assert!(html[0].contains("rowspan=\"2\" colspan=\"2\""), "{}", html[0]);
    assert_eq!(html[0].matches("<td").count(), 6);
    for t in ["Merged", "RightOne", "RightTwo", "BottomOne", "BottomTwo", "BottomThree"] {
        assert_eq!(html[0].matches(t).count(), 1, "{}", html[0]);
    }
    assert_eq!(html[1], "NextPage");
    // Repeated overlapping rule/line candidates must consume more shared work.
    let two = page(&(body.clone() + &body));
    let (_, report) = to_markdown_report_with(&two, &Params::default(), &HTML);
    assert!(report.doc_work > one.doc_work);
    // Do not assign a crossing or exact-boundary word by its midpoint alone.
    for (a, b) in [(210., 240.), (220., 250.)] {
        let ambiguous = body.clone() + &line(&word(a, b, 110., "Boundary"));
        let (_, output) = both(&page(&ambiguous));
        assert!(!output[0].contains("rowspan="));
        assert!(output[0].contains("Boundary"));
    }
    // Detached fragments and wrapping must use established grid text assembly.
    for text in ["en", "純利益", "123", "العربية"] {
        let fragmented = body.clone() + &line(&word(35., 75., 115., text));
        let (_, output) = both(&page(&fragmented));
        assert!(!output[0].contains("rowspan="));
    }
    // Split the merged rectangle horizontally: colspan only.
    let (_, cs) = both(&page(&(body.clone() + &rule(20., 220., 120., 120.))));
    assert!(cs[0].contains("colspan=\"2\""));
    assert!(!cs[0].contains("rowspan="));
    // Split it vertically: rowspan only.
    let (_, rs) = both(&page(&(body.clone() + &rule(120., 120., 100., 140.))));
    assert!(rs[0].contains("rowspan=\"2\""));
    assert!(!rs[0].contains("colspan="));
    // Partial internal wall is ambiguous and must fall back.
    let (_, ambiguous) = both(&page(&(body.clone() + &rule(40., 100., 120., 120.))));
    assert!(!ambiguous[0].contains("rowspan="));
    assert!(!ambiguous[0].contains("colspan="));
    let truncated = page(&body).replacen("<page ", "<page rulesTruncated=\"1\" ", 1);
    let (_, rejected) = both(&truncated);
    assert!(!rejected[0].contains("rowspan="));
    assert!(!rejected[0].contains("colspan="));
    for output in [&ambiguous[0], &rejected[0]] {
        for text in ["Merged", "RightOne", "RightTwo", "BottomOne", "BottomTwo", "BottomThree"] {
            assert_eq!(output.matches(text).count(), 1, "{output}");
        }
    }

    // An external title deliberately selects legacy header semantics.
    let header = line(&word(20., 100., 86., "ExternalTitle")) + &body.replace("RightOne", "1.0");
    let (_, headed) = both(&page(&header));
    assert!(headed[0].contains("<thead>"), "{}", headed[0]);
    assert!(!headed[0].contains("rowspan="));

    let mut rtl = input.clone();
    for (a, b) in [
        ("Merged", "מיזוג"),
        ("RightOne", "ראשון"),
        ("RightTwo", "שני"),
        ("BottomOne", "אחד"),
        ("BottomTwo", "שתיים"),
        ("BottomThree", "שלוש"),
    ] {
        rtl = rtl.replace(a, b);
    }
    let (_, rtl) = both(&rtl);
    assert!(rtl[0].contains("dir=\"rtl\""));
    assert!(rtl[0].find("ראשון").unwrap() < rtl[0].find("מיזוג").unwrap());
    assert!(rtl[0].find("שלוש").unwrap() < rtl[0].find("שתיים").unwrap());

    // Empty geometric cells retain their positions.
    let empty = input.replace("BottomTwo", "");
    let (_, e) = both(&empty);
    assert_eq!(e[0].matches("<td").count(), 6);
    assert!(e[0].contains("<td></td>"));
    // Hostile text is escaped in the actual span renderer.
    let hostile = input.replace("Merged", "&lt;/table&gt;&amp;");
    let (_, h) = both(&hostile);
    assert!(h[0].contains("&lt;/table&gt;&amp;"));
    assert_eq!(h[0].matches("</table>").count(), 1);
}
