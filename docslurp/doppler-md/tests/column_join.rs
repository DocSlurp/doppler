//! A paragraph that runs out of one column and goes on at the top of the next is one paragraph.

use doppler_md::{Params, to_markdown};

fn word(x0: f64, x1: f64, y: f64, text: &str) -> String {
    format!(
        "<word xMin=\"{x0}\" xMax=\"{x1}\" yMin=\"{y}\" yMax=\"{}\" fontSize=\"10\" fontName=\"Serif\" baseline=\"{}\">{text}</word>",
        y + 10.0,
        y + 8.0
    )
}

/// One 200 pt wide justified line of filler words starting at `x0`; `last` replaces its last word.
fn line(x0: f64, y: f64, first: &str, last: &str) -> String {
    let words = [first, "alpha", "beta", "gamma", "delta", "epsilon", last];
    let mut x = x0;
    let mut out = String::new();
    for (i, w) in words.iter().enumerate() {
        let width = if i == words.len() - 1 { 200.0 + x0 - x } else { 5.0 * w.len() as f64 };
        out += &word(x, x + width, y, w);
        x += width + 3.0;
    }
    format!("<block><line>{out}</line></block>")
}

/// Two columns of nine lines. The left column ends with an unfinished sentence (`left_end`);
/// the right one starts with `right_start`.
fn page(left_end: &str, right_start: &str) -> String {
    let mut body = String::new();
    for i in 0..9 {
        let y = 100.0 + 12.0 * i as f64;
        let first = if i == 0 { "Start" } else { "word" };
        let last = if i == 8 { left_end } else { "tail" };
        body += &line(72.0, y, first, last);
    }
    for i in 0..9 {
        let y = 100.0 + 12.0 * i as f64;
        let first = if i == 0 { right_start } else { "more" };
        let last = if i == 2 { "end." } else { "tail" };
        body += &line(330.0, y, first, last);
    }
    format!("<page width=\"612\" height=\"792\">{body}</page>")
}

fn convert(xhtml: &str, join: bool) -> Vec<String> {
    let mut p = Params::default();
    p.join_column_breaks = join;
    to_markdown(xhtml, &p).join("\n\n").split("\n\n").map(str::to_string).collect()
}

#[test]
fn a_sentence_that_continues_in_lowercase_at_the_top_of_the_next_column_is_joined() {
    let xhtml = page("which", "continues");
    let split = convert(&xhtml, false);
    let joined = convert(&xhtml, true);
    assert_eq!(split.len(), 2, "{split:?}");
    assert_eq!(joined.len(), 1, "{joined:?}");
    assert!(joined[0].contains("which continues alpha"), "{joined:?}");
}

#[test]
fn a_hyphenated_word_is_made_whole_across_the_break() {
    let joined = convert(&page("exam-", "ple"), true);
    assert_eq!(joined.len(), 1, "{joined:?}");
    assert!(joined[0].contains("example alpha"), "{joined:?}");
}

#[test]
fn a_finished_sentence_or_a_capital_keeps_the_columns_apart() {
    for (end, start) in [("done.", "continues"), ("which", "Continues"), ("which:", "continues")] {
        let joined = convert(&page(end, start), true);
        assert_eq!(joined.len(), 2, "{end:?} {start:?}: {joined:?}");
    }
}
