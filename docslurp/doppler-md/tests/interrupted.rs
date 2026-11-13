//! A paragraph interrupted by a figure (a picture, its labels, its caption) or a footnote goes
//! on after it: one paragraph, the interrupting content after it.

use doppler_md::{Options, Params, to_markdown_report_with};

fn word(x0: f64, x1: f64, y: f64, size: f64, text: &str) -> String {
    format!(
        "<word xMin=\"{x0}\" xMax=\"{x1}\" yMin=\"{y}\" yMax=\"{}\" fontSize=\"{size}\" fontName=\"Serif\" baseline=\"{}\">{text}</word>",
        y + size,
        y + 0.8 * size
    )
}

/// A line of `text` starting at `x0`; with `width` its last word runs to `x0 + width` (a full,
/// justified line), else the words keep their natural widths.
fn line(x0: f64, y: f64, size: f64, text: &str, width: Option<f64>) -> String {
    let words: Vec<&str> = text.split(' ').collect();
    let mut x = x0;
    let mut out = String::new();
    for (i, w) in words.iter().enumerate() {
        let natural = 0.5 * size * w.chars().count() as f64;
        let end = match width {
            Some(width) if i == words.len() - 1 => (x0 + width).max(x + natural),
            _ => x + natural,
        };
        out += &word(x, end, y, size, w);
        x = end + 0.3 * size;
    }
    format!("<block><line>{out}</line></block>")
}

/// A full body line of a 200 pt column at `x0`.
fn body(x0: f64, y: f64, first: &str, last: &str) -> String {
    line(x0, y, 10.0, &format!("{first} alpha beta gamma delta epsilon {last}"), Some(200.0))
}

/// `n` full body lines from `y`, the first starting with `first`, the last ending in `last`.
fn para(x0: f64, y: f64, n: usize, first: &str, last: &str) -> String {
    (0..n)
        .map(|i| {
            let f = if i == 0 { first } else { "word" };
            let l = if i == n - 1 { last } else { "tail" };
            body(x0, y + 12.0 * i as f64, f, l)
        })
        .collect()
}

/// Figure labels in tiny print and a caption in small print, centred in a column at `x0`.
fn figure_text(x0: f64, y: f64) -> String {
    line(x0 + 60.0, y, 5.0, "0 10 20 30", None)
        + &line(x0 + 60.0, y + 8.0, 5.0, "training error (%)", None)
        + &line(x0, y + 20.0, 9.0, "Figure 1. Error of the two models over training.", None)
}

fn image(x0: f64, y0: f64, x1: f64, y1: f64) -> String {
    format!("<image xMin=\"{x0}\" yMin=\"{y0}\" xMax=\"{x1}\" yMax=\"{y1}\" width=\"600\" height=\"300\"/>")
}

fn page(body: &str) -> String {
    format!("<page width=\"612\" height=\"792\" number=\"1\">{body}</page>")
}

fn convert_with(xhtml: &str, join: bool, markers: bool) -> Vec<String> {
    let p = Params { join_interrupted_paragraphs: join, ..Params::default() };
    let o = Options { markers, ..Options::default() };
    let (pages, _) = to_markdown_report_with(xhtml, &p, &o);
    pages.join("\n\n").split("\n\n").map(str::to_string).collect()
}

fn convert(xhtml: &str, join: bool) -> Vec<String> {
    convert_with(xhtml, join, false)
}

/// A paragraph of five lines, the figure text below it, and `next` below that, in one column.
fn in_column(last: &str, next: &str) -> String {
    page(&(para(72.0, 100.0, 5, "Start", last) + &figure_text(72.0, 170.0) + &para(72.0, 215.0, 3, next, "end.")))
}

#[test]
fn a_paragraph_that_goes_on_below_a_figure_in_its_column_is_one_paragraph_followed_by_the_figure() {
    let xhtml = in_column("which", "continues");
    let split = convert(&xhtml, false);
    let joined = convert(&xhtml, true);
    assert!(split.iter().any(|b| b.starts_with("continues")), "{split:?}");
    assert!(joined[0].starts_with("Start") && joined[0].contains("which continues alpha"), "{joined:?}");
    assert!(joined[0].ends_with("end."), "{joined:?}");
    // the figure text is kept, after the joined paragraph
    assert!(joined[1..].iter().any(|b| b.contains("Figure 1.")), "{joined:?}");
    assert_eq!(joined.len() + 1, split.len(), "{split:?} {joined:?}");
}

#[test]
fn a_hyphenated_word_is_made_whole_across_the_figure() {
    let joined = convert(&in_column("exam-", "ple"), true);
    assert!(joined[0].contains("example alpha"), "{joined:?}");
}

#[test]
fn a_finished_sentence_or_a_capital_keeps_the_halves_apart() {
    for (last, next) in [("done.", "continues"), ("which", "Continues"), ("which:", "continues")] {
        let joined = convert(&in_column(last, next), true);
        assert!(joined.iter().any(|b| b.starts_with(next)), "{last:?} {next:?}: {joined:?}");
    }
}

#[test]
fn smaller_print_alone_is_not_a_figure() {
    // a caption-sized block (a quotation or listing in small print) with no picture and no
    // figure labels can sit inside a sentence: the text after it is not moved before it
    let xhtml = page(
        &(para(72.0, 100.0, 5, "Start", "which")
            + &line(72.0, 170.0, 9.0, "a listing set in smaller print", None)
            + &line(72.0, 181.0, 9.0, "over two lines of its own", None)
            + &para(72.0, 205.0, 3, "continues", "end.")),
    );
    let joined = convert(&xhtml, true);
    assert!(joined.iter().any(|b| b.starts_with("continues")), "{joined:?}");
}

#[test]
fn body_sized_display_text_is_not_an_interruption() {
    let xhtml = page(
        &(para(72.0, 100.0, 5, "Start", "where")
            + &line(140.0, 172.0, 10.0, "x = y + z", None)
            + &para(72.0, 195.0, 3, "continues", "end.")),
    );
    let joined = convert(&xhtml, true);
    assert!(joined.iter().any(|b| b.starts_with("continues")), "{joined:?}");
}

#[test]
fn the_later_half_must_share_the_column() {
    // indented and narrower below the figure: a quotation or a nested block, not the same text
    let xhtml = page(
        &(para(72.0, 100.0, 5, "Start", "which")
            + &figure_text(72.0, 170.0)
            + &line(110.0, 215.0, 10.0, "continues alpha beta gamma delta epsilon tail", Some(120.0))
            + &line(110.0, 227.0, 10.0, "word alpha beta gamma delta epsilon end.", Some(120.0))),
    );
    let joined = convert(&xhtml, true);
    assert!(joined.iter().any(|b| b.starts_with("continues")), "{joined:?}");
}

#[test]
fn a_picture_between_the_halves_is_a_figure() {
    let xhtml = page(
        &(para(72.0, 100.0, 5, "Start", "which")
            + &image(100.0, 170.0, 240.0, 260.0)
            + &para(72.0, 275.0, 3, "continues", "end.")),
    );
    let split = convert_with(&xhtml, false, true);
    let joined = convert_with(&xhtml, true, true);
    assert!(split.iter().any(|b| b.starts_with("continues")), "{split:?}");
    assert!(joined[0].contains("which continues alpha"), "{joined:?}");
    assert!(joined[1].starts_with("![image](images/page-0001/image-"), "{joined:?}");
    // without markers the picture is not seen: a gap alone is no interruption
    let plain = convert_with(&xhtml, true, false);
    assert!(plain.iter().any(|b| b.starts_with("continues")), "{plain:?}");
}

/// Two columns: the left one ends with `last` and then `below` at its bottom; the right one
/// starts with `top` and then a paragraph opening with `next`.
fn two_columns(last: &str, below: &str, top: &str, next: &str) -> String {
    let left = para(72.0, 100.0, 12, "Start", last) + below;
    let right = top.to_string() + &para(330.0, if top.is_empty() { 100.0 } else { 145.0 }, 8, next, "end.");
    page(&(left + &right))
}

#[test]
fn a_footnote_at_the_bottom_of_the_column_does_not_hide_the_column_break() {
    let footnote = line(72.0, 250.0, 8.0, "1 A note on the method used here.", None);
    let xhtml = two_columns("which", &footnote, "", "continues");
    let split = convert(&xhtml, false);
    let joined = convert(&xhtml, true);
    assert!(split.iter().any(|b| b.starts_with("continues")), "{split:?}");
    assert!(joined[0].contains("which continues alpha") && joined[0].ends_with("end."), "{joined:?}");
    assert_eq!(joined[1], "1 A note on the method used here.", "{joined:?}");
}

#[test]
fn a_figure_at_the_top_of_the_next_column_does_not_hide_the_column_break() {
    let xhtml = two_columns("which", "", &figure_text(330.0, 100.0), "continues");
    let joined = convert(&xhtml, true);
    assert!(joined[0].contains("which continues alpha"), "{joined:?}");
    assert!(joined[1..].iter().any(|b| b.contains("Figure 1.")), "{joined:?}");
    let split = convert(&xhtml, false);
    assert!(split.iter().any(|b| b.starts_with("continues")), "{split:?}");
}

#[test]
fn small_print_at_the_bottom_of_the_column_that_is_not_a_footnote_keeps_the_columns_apart() {
    let note = line(72.0, 250.0, 8.0, "Note on the method used here.", None);
    let joined = convert(&two_columns("which", &note, "", "continues"), true);
    assert!(joined.iter().any(|b| b.starts_with("continues")), "{joined:?}");
}

#[test]
fn a_capital_after_the_footnote_keeps_the_columns_apart() {
    let footnote = line(72.0, 250.0, 8.0, "1 A note on the method used here.", None);
    let joined = convert(&two_columns("which", &footnote, "", "Continues"), true);
    assert!(joined.iter().any(|b| b.starts_with("Continues")), "{joined:?}");
}

/// Two lines of a 164 pt column at the top of the page, then its next lines beside another
/// column's text, which cuts the column into regions; the column's third line starts with
/// `next` after `gap` points.
fn cut_column(last: &str, next: &str, gap: f64) -> String {
    let column = |y: f64, first: &str, end: &str| {
        line(42.0, y, 10.0, &format!("{first} alpha beta gamma delta epsilon {end}"), Some(164.0))
    };
    let mut body = column(44.0, "Form", "tail") + &column(56.0, "word", last);
    for i in 0..6 {
        let y = 68.0 + gap + 12.0 * i as f64;
        let end = if i == 5 { "end." } else { "tail" };
        body += &column(y, if i == 0 { next } else { "word" }, end);
        body += &line(300.0, y, 10.0, &format!("right side text alpha beta gamma {end}"), Some(200.0));
    }
    page(&body)
}

#[test]
fn the_next_line_of_a_column_cut_off_by_a_region_boundary_goes_on_the_paragraph() {
    let xhtml = cut_column("ac-", "count", 0.0);
    let split = convert(&xhtml, false);
    let joined = convert(&xhtml, true);
    assert!(split.iter().any(|b| b.starts_with("count")), "{split:?}");
    assert!(joined[0].contains("epsilon account alpha") && joined[0].ends_with("end."), "{joined:?}");
    assert!(joined[1].starts_with("right side text"), "{joined:?}");
}

#[test]
fn a_column_cut_by_a_region_boundary_stays_apart_after_a_gap_or_a_capital() {
    for (next, gap) in [("count", 8.0), ("Count", 0.0)] {
        let joined = convert(&cut_column("which", next, gap), true);
        assert!(joined.iter().any(|b| b.starts_with(next)), "{next:?} {gap}: {joined:?}");
    }
}
