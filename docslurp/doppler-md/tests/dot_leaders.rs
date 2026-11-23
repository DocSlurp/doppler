//! Printed leaders between labels and numeric columns must not seed columns.
use doppler_md::{Options, Params, TableFormat, to_markdown_report_with};

fn word(text: &str, x: usize, y: usize) -> String {
    format!(
        r#"<word xMin="{x}" xMax="{}" yMin="{}" yMax="{y}" fontSize="10" baseline="{y}">{text}</word>"#,
        x + text.len() * 3,
        y - 8
    )
}

fn sample(n: usize) -> String {
    let mut page = String::from(r#"<page width="400" height="700">"#);
    for (i, name) in ["Plates", "Frames", "Braces", "Panels", "Posts", "Gates"].iter().enumerate() {
        let y = 80 + i * 14;
        page += "<block><line>";
        page += &word(name, 15, y);
        for j in 0..n {
            page += &word(".", 56 + j * 5, y);
        }
        page += "</line></block>";
        for (x, value) in [(180, "45.6"), (260, "76")] {
            page += "<block><line>";
            page += &word(value, x, y);
            page += "</line></block>";
        }
    }
    page + "</page>"
}

#[test]
fn dot_runs_connect_labels_to_amounts_without_becoming_cells() {
    for tables in [TableFormat::Html, TableFormat::Gfm] {
        let options = Options { tables, ..Options::default() };
        let extract = |n| to_markdown_report_with(&sample(n), &Params::default(), &options).0.join("");
        let actual = extract(15);
        assert_eq!(actual, extract(0), "{actual}");
        assert!(actual.contains("<td>Plates</td><td>45.6</td>") || actual.contains("| Plates | 45.6 |"), "{actual}");
        assert_ne!(extract(3), actual, "ordinary ellipses should survive");
    }
}

#[test]
fn keep_punctuation_before_text_and_inside_decimals() {
    let plain = sample(12).replace(">45.6</word>", ">open</word>");
    let html = Options { tables: TableFormat::Html, ..Options::default() };
    let md = to_markdown_report_with(&plain, &Params::default(), &html).0.join("");
    assert!(md.matches('.').count() >= 12, "{md}");
    let numeric = to_markdown_report_with(&sample(12), &Params::default(), &html).0.join("");
    assert_eq!(numeric.matches("45.6").count(), 6, "{numeric}");
}
