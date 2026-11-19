//! Content that runs across a page break is marked, only with `Options::markers`.

use doppler_md::{Options, Params, to_markdown_report_with};

fn word(x0: f64, x1: f64, y: f64, text: &str) -> String {
    format!(
        "<word xMin=\"{x0}\" xMax=\"{x1}\" yMin=\"{y}\" yMax=\"{}\" fontSize=\"10\" fontName=\"Serif\" baseline=\"{}\">{text}</word>",
        y + 10.0,
        y + 8.0
    )
}

fn row(y: f64, cells: [&str; 3]) -> String {
    let xs = [72.0, 230.0, 380.0];
    let mut out = String::new();
    for (x, c) in xs.iter().zip(cells) {
        out += &format!("<block><line>{}</line></block>", word(*x, *x + 6.0 * c.len() as f64, y, c));
    }
    out
}

fn table(top: f64, header: Option<[&str; 3]>, first: usize) -> String {
    let mut y = top;
    let mut out = String::new();
    if let Some(h) = header {
        out += &row(y, h);
        y += 16.0;
    }
    for i in first..first + 8 {
        out += &row(y, [&format!("Item{i}"), &format!("{}", 10 + i), &format!("{}.50", 3 + i)]);
        y += 16.0;
    }
    out
}

fn text(y: f64, t: &str) -> String {
    let mut x = 72.0;
    let mut words = String::new();
    for w in t.split(' ') {
        words += &word(x, x + 5.0 * w.len() as f64, y, w);
        x += 5.0 * w.len() as f64 + 3.0;
    }
    format!("<block><line>{words}</line></block>")
}

fn page(body: &str) -> String {
    format!("<page width=\"612\" height=\"792\">{body}</page>")
}

fn convert(xhtml: &str, markers: bool) -> Vec<String> {
    let o = Options { markers, ..Options::default() };
    to_markdown_report_with(xhtml, &Params::default(), &o).0
}

const HEADER: [&str; 3] = ["Name", "Quantity", "Price"];

#[test]
fn a_table_that_repeats_its_header_on_the_next_page_is_marked_repeated() {
    let xhtml = page(&table(560.0, Some(HEADER), 1)) + &page(&table(60.0, Some(HEADER), 9));
    let pages = convert(&xhtml, true);
    assert!(
        pages[1].starts_with("<!-- doppler:table-continued header=repeated -->\n\n| Name | Quantity | Price |"),
        "{}",
        pages[1]
    );
    assert!(!pages[0].contains("doppler:"), "{}", pages[0]);
}

#[test]
fn a_continuation_without_a_header_inherits_the_header_of_the_table_it_goes_on_from() {
    let xhtml = page(&table(560.0, Some(HEADER), 1)) + &page(&table(60.0, None, 9));
    let pages = convert(&xhtml, true);
    assert!(
        pages[1].starts_with("<!-- doppler:table-continued header=inherited -->\n\n| Name | Quantity | Price |\n| --- | --- | --- |\n| Item9 |"),
        "{}",
        pages[1]
    );
}

#[test]
fn a_continued_note_above_the_table_is_kept_and_does_not_hide_the_continuation() {
    let xhtml = page(&table(560.0, Some(HEADER), 1))
        + &page(&(text(60.0, "Table 2 (continued)") + &table(90.0, Some(HEADER), 9)));
    let pages = convert(&xhtml, true);
    assert!(pages[1].contains("<!-- doppler:table-continued header=repeated -->"), "{}", pages[1]);
    assert!(pages[1].contains("Table 2 (continued)"), "{}", pages[1]);
}

#[test]
fn a_table_that_does_not_line_up_or_is_not_at_the_top_is_a_new_table() {
    // a text paragraph first, then the table: not a continuation
    let xhtml = page(&table(560.0, Some(HEADER), 1))
        + &page(
            &(text(60.0, "A new paragraph begins the next page and it is long enough to be one.")
                + &table(200.0, Some(HEADER), 9)),
        );
    assert!(!convert(&xhtml, true)[1].contains("table-continued"));
}

#[test]
fn a_paragraph_that_goes_on_is_marked_and_a_finished_one_is_not() {
    let open = "The pump keeps running through the whole of the long afternoon until the tank is nearly";
    let page1 = page(&text(700.0, open));
    let page2 = page(&text(60.0, "empty and the float switch finally trips the relay."));
    let marked = convert(&(page1.clone() + &page2), true);
    assert!(marked[1].starts_with("<!-- doppler:paragraph-continued -->\n\nempty and"), "{}", marked[1]);
    let done =
        page(&text(700.0, "The pump keeps running through the whole of the long afternoon until the tank is empty."));
    assert!(!convert(&(done + &page2), true)[1].contains("doppler:"));
}

#[test]
fn nothing_is_marked_unless_asked() {
    let xhtml = page(&table(560.0, Some(HEADER), 1)) + &page(&table(60.0, Some(HEADER), 9));
    assert!(!convert(&xhtml, false)[1].contains("doppler:"));
}

/// A line of `t` in `size` point print.
fn small_text(y: f64, t: &str, size: f64) -> String {
    let mut x = 72.0;
    let mut words = String::new();
    for w in t.split(' ') {
        let width = 0.5 * size * w.len() as f64;
        words += &format!(
            "<word xMin=\"{x}\" xMax=\"{}\" yMin=\"{y}\" yMax=\"{}\" fontSize=\"{size}\" fontName=\"Serif\" baseline=\"{}\">{w}</word>",
            x + width,
            y + size,
            y + 0.8 * size
        );
        x += width + 0.3 * size;
    }
    format!("<block><line>{words}</line></block>")
}

fn convert_with(xhtml: &str, past_notes: bool) -> Vec<String> {
    let o = Options { markers: true, ..Options::default() };
    let p = Params { stitch_past_notes: past_notes, ..Params::default() };
    to_markdown_report_with(xhtml, &p, &o).0
}

#[test]
fn a_footnote_under_the_last_table_does_not_hide_the_continuation() {
    for note in [
        small_text(730.0, "Prices exclude shipping and handling charges.", 8.0),
        text(730.0, "1 Prices exclude shipping and handling charges."),
        small_text(730.0, "* Discontinued items are listed for reference.", 8.0),
    ] {
        let xhtml = page(&(table(560.0, Some(HEADER), 1) + &note)) + &page(&table(60.0, Some(HEADER), 9));
        let pages = convert_with(&xhtml, true);
        assert!(pages[0].contains("Prices exclude") || pages[0].contains("Discontinued"), "{}", pages[0]);
        assert!(
            pages[1].starts_with("<!-- doppler:table-continued header=repeated -->"),
            "{}\n=== {}",
            pages[0],
            pages[1]
        );
        assert!(!convert_with(&xhtml, false)[1].contains("doppler:"), "off");
    }
}

#[test]
fn a_small_note_above_the_next_table_does_not_hide_the_continuation() {
    let xhtml = page(&table(560.0, Some(HEADER), 1))
        + &page(&(small_text(60.0, "Amounts in dollars.", 8.0) + &table(80.0, None, 9)));
    let pages = convert_with(&xhtml, true);
    assert!(pages[1].starts_with("Amounts in dollars."), "{}", pages[1]);
    assert!(
        pages[1].contains("Amounts in dollars.\n\n<!-- doppler:table-continued header=inherited -->\n\n| Name |"),
        "{}",
        pages[1]
    );
    assert!(!convert_with(&xhtml, false)[1].contains("doppler:"));
}

#[test]
fn body_text_after_the_table_or_before_the_next_ends_it() {
    let prose = "The totals above are reconciled every quarter against the ledger of the warehouse.";
    let after = page(&(table(560.0, Some(HEADER), 1) + &text(730.0, prose))) + &page(&table(60.0, Some(HEADER), 9));
    assert!(!convert_with(&after, true)[1].contains("table-continued"));
    let before = page(&table(560.0, Some(HEADER), 1)) + &page(&(text(60.0, prose) + &table(80.0, Some(HEADER), 9)));
    assert!(!convert_with(&before, true)[1].contains("table-continued"));
    // a small note above body text, with no table after it, is no reason either
    let note_then_text = page(&table(560.0, Some(HEADER), 1))
        + &page(&(small_text(60.0, "Amounts in dollars.", 8.0) + &text(75.0, prose) + &table(100.0, None, 9)));
    assert!(!convert_with(&note_then_text, true)[1].contains("table-continued"));
}

#[test]
fn a_title_block_of_small_print_above_the_next_table_starts_a_new_table() {
    let title = ["TABLE 5", "PRINTING OFFICE", "AMOUNTS DUE", "FISCAL YEARS 1974 AND 1973"]
        .iter()
        .enumerate()
        .map(|(i, t)| small_text(40.0 + 18.0 * i as f64, t, 8.0))
        .collect::<String>();
    let xhtml = page(&table(560.0, Some(HEADER), 1)) + &page(&(title + &table(110.0, None, 9)));
    let out = convert_with(&xhtml, true);
    assert!(!out[1].contains("table-continued"), "{}", out[1]);
}

fn join(xhtml: &str, window: Option<usize>) -> (Vec<String>, doppler_md::Report) {
    use doppler_md::model::Parser;
    let params = Params::default();
    let mut converter = doppler_md::Converter::new(&params, Options::default(), window);
    assert!(converter.enable_page_table_joining());
    let mut parser = Parser::new();
    let mut pages = Vec::new();
    {
        let mut convert = |page| {
            converter.push(page, &mut |n, md| {
                assert_eq!(n, pages.len());
                pages.push(md);
            })
        };
        parser.push(xhtml, &mut convert);
    }
    let parse = parser.finish(&mut |page| {
        converter.push(page, &mut |n, md| {
            assert_eq!(n, pages.len());
            pages.push(md);
        })
    });
    let report = converter.finish(parse, &mut |n, md| {
        assert_eq!(n, pages.len());
        pages.push(md);
    });
    (pages, report)
}

#[test]
fn joined_table_keeps_page_slots_and_reports_span() {
    let first = page(&table(560.0, Some(HEADER), 1));
    for header in [Some(HEADER), None] {
        let second = page(&(text(60.0, "Table 2 (continued)") + &table(90.0, header, 9)));
        let source = first.clone() + &second;
        let (pages, report) = join(&source, None);
        assert_eq!(pages.len(), 2);
        assert!(pages[1].is_empty(), "{pages:#?}");
        assert_eq!(pages[0].matches("| Name | Quantity | Price |").count(), 1);
        assert!(pages[0].contains("| Item16 |"), "{pages:#?}");
        assert_eq!(report.joined_tables, [doppler_md::JoinedTable { start_page: 1, end_page: 2, rows: 17 }]);
        let json: serde_json::Value = serde_json::from_str(&report.to_json()).unwrap();
        assert_eq!(json["joinedTables"][0]["endPage"], 2);
        let (streamed, _) = join(&source, Some(4));
        assert_eq!(streamed, pages);
    }
}

#[test]
fn page_table_join_respects_body_barriers_and_bound() {
    let first = page(&table(560.0, Some(HEADER), 1));
    let second = page(&table(60.0, Some(HEADER), 9));
    let source = first.clone() + &second;
    let (pages, report) = join(&source, Some(1));
    assert_eq!(pages, convert(&source, false));
    assert!(report.joined_tables.is_empty());

    let blocked = page(
        &(text(60.0, "A new paragraph begins and makes this a completely different table.")
            + &table(190.0, Some(HEADER), 9)),
    );
    let source = first + &blocked;
    let (pages, report) = join(&source, None);
    assert_eq!(pages, convert(&source, false));
    assert!(report.joined_tables.is_empty());
}

fn long_ruled_table(first: usize, rows: usize, top: f64) -> String {
    let size = 16.0;
    let bottom = top + size * (rows + 1) as f64;
    let mut content = row(top + 2.0, HEADER);
    for n in 0..rows {
        let i = first + n;
        content += &row(
            top + size * (n + 1) as f64 + 2.0,
            [&format!("Item{i}"), &format!("{}", 10 + i), &format!("{}.50", 3 + i)],
        );
    }
    for i in 0..=rows + 1 {
        let y = top + size * i as f64;
        content += &format!(r##"<rule xMin="65" xMax="455" yMin="{y}" yMax="{y}" color="#000000"/>"##);
    }
    for x in [65, 205, 355, 455] {
        content += &format!(r##"<rule xMin="{x}" xMax="{x}" yMin="{top}" yMax="{bottom}" color="#000000"/>"##);
    }
    content
}

#[test]
fn ruled_chain_groups_pages_and_flushes_at_the_window_limit() {
    let first = page(&long_ruled_table(1, 8, 590.0));
    let middle = page(&long_ruled_table(9, 28, 60.0));
    let last = page(&long_ruled_table(37, 8, 60.0));
    let source = first + &middle + &last;
    let (pages, report) = join(&source, None);
    assert_eq!(pages.len(), 3);
    assert!(pages[1].is_empty() && pages[2].is_empty(), "{pages:#?}");
    assert_eq!(report.joined_tables, [doppler_md::JoinedTable { start_page: 1, end_page: 3, rows: 45 }]);
    assert_eq!(join(&source, Some(4)).0, pages);

    let (bounded, report) = join(&source, Some(2));
    assert!(bounded[1].is_empty());
    assert!(bounded[2].contains("| Item37 |"));
    assert_eq!(report.joined_tables, [doppler_md::JoinedTable { start_page: 1, end_page: 2, rows: 37 }]);
}
