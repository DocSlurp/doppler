//! A short label in the margin beside a paragraph (a callout icon's word, a side label) that
//! the producer read as the start of one of its lines is a paragraph of its own before it.

use doppler_md::{Params, to_markdown};

fn word(x0: f64, x1: f64, y: f64, size: f64, font: &str, text: &str) -> String {
    format!(
        "<word xMin=\"{x0}\" xMax=\"{x1}\" yMin=\"{y}\" yMax=\"{}\" fontSize=\"{size}\" fontName=\"{font}\" baseline=\"{}\">{text}</word>",
        y + size,
        y + 0.8 * size
    )
}

/// Words of `text` from `x0`, the last one running to 206 pt (a justified line).
fn words(x0: f64, y: f64, text: &str) -> String {
    let ws: Vec<&str> = text.split(' ').collect();
    let mut x = x0;
    let mut out = String::new();
    for (i, w) in ws.iter().enumerate() {
        let natural = 5.0 * w.chars().count() as f64;
        let end = if i == ws.len() - 1 { 206f64.max(x + natural) } else { x + natural };
        out += &word(x, end, y, 10.0, "Times-Italic", w);
        x = end + 2.75;
    }
    out
}

/// A three-line paragraph at x 78 whose second line starts with `label` (in `size` and
/// `font`) at x 47, `gap` points before the text; the first line ends with `end`, the second
/// starts with `start`.
fn page(end: &str, label: &str, size: f64, font: &str, gap: f64, start: &str) -> String {
    let first = words(78.0, 95.0, &format!("Your refund can be split and {end}"));
    let label_end = 78.0 - gap;
    let second = word(label_end - 6.0 * label.chars().count() as f64, label_end, 107.0, size, font, label)
        + &words(78.0, 106.0, &format!("{start} deposited into up to three"));
    let third = words(78.0, 118.0, "accounts in your name on that form.");
    let lines = [first, second, third].map(|l| format!("<block><line>{l}</line></block>")).concat();
    format!("<page width=\"612\" height=\"792\">{lines}</page>")
}

fn convert(xhtml: &str, labels: bool) -> Vec<String> {
    let p = Params { margin_labels: labels, ..Params::default() };
    to_markdown(xhtml, &p).join("\n\n").split("\n\n").map(str::to_string).collect()
}

fn tip(end: &str, start: &str) -> String {
    page(end, "TIP", 11.25, "Helvetica-Black", 10.0, start)
}

#[test]
fn a_margin_label_inside_a_sentence_goes_before_the_paragraph_and_the_word_is_made_whole() {
    let xhtml = tip("di-", "rectly");
    let before = convert(&xhtml, false);
    assert!(before.concat().contains("di- TIP rectly"), "{before:?}");
    let after = convert(&xhtml, true);
    assert_eq!(after.len(), 2, "{after:?}");
    assert!(after[0].ends_with("TIP"), "{after:?}");
    assert!(after[1].contains("split and directly deposited") && after[1].ends_with("form."), "{after:?}");
}

#[test]
fn a_label_beside_a_lowercase_continuation_is_split_off_too() {
    let after = convert(&tip("then", "is"), true);
    assert!(after[0].ends_with("TIP"), "{after:?}");
    assert!(after[1].contains("and then is deposited"), "{after:?}");
}

#[test]
fn a_label_that_heads_a_new_sentence_stays() {
    // after an end of sentence, or before a capital without a hyphen, the label may head what
    // follows (a run-in "Note:"), so the line is left as it was
    for (end, start) in [("done.", "rectly"), ("then", "Rectly")] {
        let after = convert(&tip(end, start), true);
        assert_eq!(after, convert(&tip(end, start), false), "{end:?} {start:?}");
    }
}

#[test]
fn numbers_bullets_and_text_in_the_same_style_are_not_labels() {
    for xhtml in [
        page("di-", "D.5", 11.25, "Helvetica-Black", 10.0, "rectly"),
        page("di-", "o", 11.25, "Helvetica-Black", 10.0, "rectly"),
        page("di-", "TIP", 10.0, "Times-Italic", 10.0, "rectly"),
        // set at ordinary word spacing: part of the line
        page("di-", "TIP", 11.25, "Helvetica-Black", 2.75, "rectly"),
    ] {
        let after = convert(&xhtml, true);
        assert_eq!(after, convert(&xhtml, false));
    }
}
