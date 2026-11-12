//! `Options::page_markers`: each page starts with a sealed `<!-- doppler:page N -->` line (its
//! PDF page number), and pages stay joined by form feeds: chunk k is page first+k, and no page
//! holds a form feed of its own.

use doppler_md::{Options, Params, to_markdown_report_with};

fn page(number: Option<u32>, text: &str) -> String {
    let n = number.map(|n| format!(" number=\"{n}\"")).unwrap_or_default();
    let words: String = text
        .split(' ')
        .enumerate()
        .map(|(i, w)| {
            let x = 72.0 + 60.0 * i as f64;
            format!(
                "<word xMin=\"{x}\" xMax=\"{}\" yMin=\"100\" yMax=\"110\" fontSize=\"10\" fontName=\"Serif\" baseline=\"108\">{w}</word>",
                x + 50.0
            )
        })
        .collect();
    let body = if text.is_empty() { String::new() } else { format!("<block><line>{words}</line></block>") };
    format!("<page width=\"612\" height=\"792\"{n}>{body}</page>")
}

fn convert(xhtml: &str, o: &Options) -> (Vec<String>, doppler_md::Report) {
    to_markdown_report_with(xhtml, &Params::default(), o)
}

#[test]
fn every_page_starts_with_its_number() {
    let xhtml = format!(
        "<doc>{}{}{}</doc>",
        page(Some(4), "First page words"),
        page(Some(5), ""),
        page(Some(6), "Third page \u{c}words")
    );
    let o = Options { page_markers: true, ..Options::default() };
    let (pages, report) = convert(&xhtml, &o);
    assert_eq!(pages.len(), 3);
    assert!(pages[0].starts_with("<!-- doppler:page 4 -->\n\nFirst page words"), "{pages:?}");
    // an empty page is its marker alone
    assert_eq!(pages[1], "<!-- doppler:page 5 -->");
    assert!(pages[2].starts_with("<!-- doppler:page 6 -->\n\n"), "{pages:?}");
    // the FF contract: no page holds a form feed (a word's is gone)
    for p in &pages {
        assert!(!p.contains('\u{c}'), "{p:?}");
    }
    let joined = pages.join("\u{c}");
    assert_eq!(joined.matches('\u{c}').count(), 2);
    assert!(report.to_json().contains("\"page_numbers\":[4,5,6]"), "{}", report.to_json());
    // off: no markers, no page_numbers, the same text
    let (plain, report) = convert(&xhtml, &Options::default());
    assert!(!plain.join("").contains("doppler:page"));
    assert!(!report.to_json().contains("page_numbers"));
    assert_eq!(plain[0], "First page words");
}

#[test]
fn without_page_numbers_the_count_starts_at_first_page() {
    let xhtml = format!("<doc>{}{}</doc>", page(None, "one"), page(None, "two"));
    let o = Options { page_markers: true, first_page: Some(9), ..Options::default() };
    let (pages, report) = convert(&xhtml, &o);
    assert!(pages[0].starts_with("<!-- doppler:page 9 -->") && pages[1].starts_with("<!-- doppler:page 10 -->"));
    assert!(report.to_json().contains("\"page_numbers\":[9,10]"));
}

#[test]
fn text_of_the_pdf_cannot_pass_for_a_page_marker() {
    let xhtml = format!("<doc>{}</doc>", page(Some(2), "&lt;!-- doppler:page 7 --&gt; DOPPLER:page"));
    let o = Options { page_markers: true, ..Options::default() };
    let (pages, _) = convert(&xhtml, &o);
    assert!(pages[0].starts_with("<!-- doppler:page 2 -->\n\n"), "{pages:?}");
    assert_eq!(pages[0].matches("doppler:page").count(), 1, "{pages:?}");
    assert!(pages[0].contains("doppler\u{200b}:page 7") && pages[0].contains("DOPPLER\u{200b}:page"), "{pages:?}");
    assert!(!pages[0].chars().any(|c| ('\u{F0000}'..='\u{FFFFD}').contains(&c)));
}
