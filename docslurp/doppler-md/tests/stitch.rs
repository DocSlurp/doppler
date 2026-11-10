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
