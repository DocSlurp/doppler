//! Hostile-input behaviour: work limits, the conversion report, and Markdown breakout through
//! word text. Expected Markdown was cross-checked against doppler_md.py (v24).

use std::io::Write;
use std::process::{Command, Stdio};

use doppler_md::tables::grid_markdown;
use doppler_md::{Degraded, Params, to_markdown, to_markdown_report};

fn word(x0: f64, y0: f64, text: &str) -> String {
    format!(
        r#"<word xMin="{x0}" yMin="{y0}" xMax="{}" yMax="{}" fontName="Serif" fontSize="10" baseline="{}">{text}</word>"#,
        x0 + 15.0,
        y0 + 10.0,
        y0 + 8.0
    )
}

fn page(body: &str) -> String {
    format!("<doc><page width=\"9000\" height=\"9000\">\n{body}</page></doc>\n")
}

/// rows x cols one-cell lines (the council's grid repro), below the header band.
fn grid(rows: usize, cols: usize) -> String {
    let mut body = String::new();
    for r in 0..rows {
        for c in 0..cols {
            body += &format!(
                "<flow><block><line>{}</line></block></flow>\n",
                word(20.0 + 40.0 * c as f64, 2000.0 + 12.0 * r as f64, "x")
            );
        }
    }
    page(&body)
}

#[test]
fn breaks_in_word_text_become_spaces() {
    let x = page(&format!("<line>{}</line>\n", word(50.0, 50.0, "a&#10;# b&#13;c&#9;d\u{2028}e&#x2029;f\u{85}g\th")));
    assert_eq!(to_markdown(&x, &Params::default()), vec!["a # b c d e f g h"]);
}

#[test]
fn table_cells_escape_backslash_before_pipe() {
    let g = vec![vec!["a\\".to_string(), "|b".to_string()], vec!["c\\|".to_string(), "d".to_string()]];
    assert_eq!(grid_markdown(&g), "| a\\\\ | \\|b |\n| --- | --- |\n| c\\\\\\| | d |");
}

#[test]
fn report_counts_skipped_words_and_rules() {
    let good = word(50.0, 50.0, "kept");
    let x = format!(
        concat!(
            "<page width=\"612\" height=\"792\">",
            "<rule xMin=\"1\" yMin=\"nan\" xMax=\"2\" yMax=\"3\"/>",
            "<word xMin=\"1\" yMin=\"1\" xMax=\"2\" yMax=\"2\">outside</word>",
            "<line>{}",
            "<word yMin=\"1\" xMax=\"2\" yMax=\"2\">no-xmin</word>",
            "<word xMin=\"inf\" yMin=\"1\" xMax=\"2\" yMax=\"2\">inf</word>",
            "<word xMin=\"1\" yMin=\"1\" xMax=\"2\" yMax=\"2\" fontSize=\"big\">bad-size</word>",
            "<word xMin=\"1\" yMin=\"1\" xMax=\"2\" yMax=\"2\" baseline=\"1e999\">bad-base</word>",
            "</line></page>",
            "<page><line>{}{}"
        ),
        good,
        word(60.0, 60.0, "lost1"),
        word(80.0, 60.0, "lost2")
    );
    let (md, r) = to_markdown_report(&x, &Params::default());
    assert_eq!(md, vec!["kept".to_string(), String::new()]);
    assert!(r.page_found());
    let p = &r.parse;
    assert_eq!((p.pages, p.page_closes, p.lines, p.words), (2, 1, 1, 1));
    assert_eq!((p.words_missing_coord, p.words_bad_number, p.words_outside_line), (1, 3, 1));
    assert_eq!((p.words_unclosed_line, p.unclosed_line_at_eof, p.rules_skipped), (2, true, 1));
    let json = r.to_json();
    assert!(json.contains("\"unclosed_pages\":1") && json.contains("\"bad_number\":3"), "{json}");
}

#[test]
fn input_without_pages_is_reported() {
    for x in
        ["", "plain text", "<html><body><p>not doppler</p></body></html>", "<line><word xMin=\"1\">x</word></line>"]
    {
        let (md, r) = to_markdown_report(x, &Params::default());
        assert!(md.is_empty() && !r.page_found(), "{x:?}");
    }
}

#[test]
fn cli_exits_2_without_pages_and_prints_report() {
    let run = |input: &str, args: &[&str]| {
        let mut c = Command::new(env!("CARGO_BIN_EXE_doppler-md"))
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        c.stdin.take().unwrap().write_all(input.as_bytes()).unwrap();
        let o = c.wait_with_output().unwrap();
        (o.status.code(), String::from_utf8(o.stdout).unwrap(), String::from_utf8(o.stderr).unwrap())
    };
    let (code, out, err) = run("<html>nothing</html>", &[]);
    assert_eq!((code, out.as_str()), (Some(2), ""));
    assert!(err.contains("no <page>"), "{err}");
    let (code, out, err) = run(&page(&format!("<line>{}</line>", word(50.0, 50.0, "hi"))), &["--report"]);
    assert_eq!((code, out.as_str()), (Some(0), "hi"));
    let line = err.lines().next().unwrap();
    assert!(line.starts_with("{\"page_found\":true,\"pages\":1,") && line.ends_with("\"degraded\":[],\"body_size\":10,\"body_color\":\"\",\"body_sample_pages\":1,\"body_sample_text_pages\":1,\"body_given\":[]}"), "{line}");
}

#[test]
fn page_over_line_cap_reads_as_plain_lines() {
    let x = grid(3, 4);
    let p = Params { max_page_lines: 11.0, ..Params::default() };
    let (md, r) = to_markdown_report(&x, &p);
    assert_eq!(md, vec![["x"; 12].join("\n")]);
    assert_eq!(r.degraded, vec![(0, Degraded::PageLines)]);
    // at the cap the page is laid out
    let p = Params { max_page_lines: 12.0, ..Params::default() };
    assert!(to_markdown_report(&x, &p).1.degraded.is_empty());
}

#[test]
fn grid_page_runs_out_of_work_quickly() {
    // 100 x 200 one-cell lines took 4.3 s (and 280 x 280 took 123 s) before the budget
    let x = grid(100, 200);
    let t = std::time::Instant::now();
    let (md, r) = to_markdown_report(&x, &Params::default());
    assert_eq!(r.degraded, vec![(0, Degraded::PageWork)]);
    assert_eq!(md[0].lines().count(), 20_000);
    assert!(t.elapsed().as_secs_f64() < 5.0, "{:?}", t.elapsed());
}

#[test]
fn document_budget_degrades_the_remaining_pages() {
    let one = grid(4, 4);
    let x = format!("{one}{one}{one}");
    // each page charges 16 lines plus its layout work
    let (_, r) = to_markdown_report(&x, &Params::default());
    assert!(r.degraded.is_empty());
    let per_page = r.doc_work / 3;
    let p = Params { max_doc_work: (per_page * 2) as f64, doc_work_per_page: 0.0, ..Params::default() };
    let (md, r) = to_markdown_report(&x, &p);
    assert_eq!(r.degraded, vec![(2, Degraded::DocWork)]);
    assert_eq!(md[2], ["x"; 16].join("\n"));
    // a page that starts within the document budget but runs out inside it
    let p = Params { max_doc_work: (per_page * 2 + 17) as f64, doc_work_per_page: 0.0, ..Params::default() };
    assert_eq!(to_markdown_report(&x, &p).1.degraded, vec![(2, Degraded::DocWork)]);
    let p = Params { max_page_work: 1.0, ..Params::default() };
    assert_eq!(to_markdown_report(&x, &p).1.degraded.len(), 3);
}

#[test]
fn many_fragments_on_one_baseline_are_bounded() {
    // 20k one-word lines on one baseline took 1.3 s (50k: 8 s) in the same-baseline merges
    let body: String = (0..20_000)
        .map(|i| {
            let x0 = 10.0 + 4.0 * i as f64;
            format!(
                r#"<line><word xMin="{x0}" yMin="50" xMax="{}" yMax="60" fontSize="10" baseline="58">x</word></line>"#,
                x0 + 3.0
            )
        })
        .collect();
    let (_, r) = to_markdown_report(&page(&body), &Params::default());
    assert_eq!(r.degraded, vec![(0, Degraded::PageWork)]);
}

#[test]
fn image_notes_are_capped_per_document() {
    // 60 pages of 2,000 images: 120,000 notes, of which the report keeps 100,000
    let mut page = String::from(
        "<page width=\"612\" height=\"792\"><block><line><word xMin=\"72\" xMax=\"100\" yMin=\"100\" yMax=\"110\" fontSize=\"10\" fontName=\"F\" baseline=\"108\">Text</word></line></block>",
    );
    for i in 0..2000 {
        let (x, y) = ((i % 40) as f64 * 15.0, 100.0 + (i / 40) as f64 * 12.0);
        page += &format!(
            "<image xMin=\"{x}\" yMin=\"{y}\" xMax=\"{}\" yMax=\"{}\" width=\"1\" height=\"1\"/>",
            x + 0.5,
            y + 0.5
        );
    }
    page += "</page>";
    let xhtml = page.repeat(60);
    let o = doppler_md::Options { markers: true, ..Default::default() };
    let (_, report) = doppler_md::to_markdown_report_with(&xhtml, &doppler_md::Params::default(), &o);
    assert_eq!(report.images.len(), doppler_md::MAX_DOC_IMAGES);
    assert_eq!(report.images_notes_dropped, 20_000);
    assert!(report.to_json().contains("\"images_notes_dropped\":20000"));
}

/// A two-column key/value page of `rows` rows (each row two one-word lines): its layout work
/// grows with the square of the rows (every band re-follows the gutter), about 18,600 units at
/// 105 rows.
fn key_values(rows: usize) -> String {
    let mut body = String::new();
    for i in 0..rows {
        let y = 50.0 + 12.0 * i as f64;
        body += &format!("<flow><block><line>{}</line></block></flow>\n", word(50.0, y, &format!("k{i}")));
        body += &format!("<flow><block><line>{}</line></block></flow>\n", word(300.0, y, &format!("v{i}")));
    }
    format!("<page width=\"612\" height=\"{}\">\n{body}</page>\n", 12.0 * rows as f64 + 100.0)
}

/// Converts `n` copies of `page` one page at a time (no document string is held): the report.
fn many_pages(page: &str, n: usize, p: &Params, window: Option<usize>) -> doppler_md::Report {
    let mut conv = doppler_md::Converter::new(p, doppler_md::Options::default(), window);
    let mut parse = doppler_md::model::ParseReport::default();
    let doc = format!("<doc>{page}</doc>");
    for _ in 0..n {
        for pg in doppler_md::model::parse_with(&doc, &mut parse, false) {
            conv.push(pg, &mut |_, _| {});
        }
    }
    parse.pages = n;
    parse.page_closes = n;
    conv.finish(parse, &mut |_, _| {})
}

#[test]
fn the_document_budget_grows_with_its_pages() {
    // 2,800 pages of moderate work (about 18,600 units each, near the densest long PDFs'
    // average): 52 million units in all, more than the fixed 50 million budget used to allow
    let one = key_values(105);
    let p = Params::default();
    let r = many_pages(&one, 2800, &p, Some(4));
    assert!(r.doc_work as f64 > 5.0e7, "{}", r.doc_work);
    assert!(r.degraded.is_empty(), "{:?}", &r.degraded[..r.degraded.len().min(3)]);
    assert_eq!(r.doc_work_limit, 5.0e7 + 2.0e4 * 2800.0);
    assert!(r.to_json().contains("\"doc_work_limit\":106000000,"), "{}", r.to_json());
    // (a fixed budget of 50 million, as before, would have read the last pages as plain lines:
    // the fixed-budget cut-off is checked in the test below)
}

#[test]
fn a_hostile_document_stops_at_the_total_cap() {
    // every page uses nearly its whole page budget, and the per-page credit covers that, so
    // only max_doc_work_total stops it: the pages after are plain lines and listed
    let one = key_values(105);
    // a page's work in this run (furniture takes the repeated margin lines out)
    let per = many_pages(&one, 60, &Params::default(), Some(4)).doc_work as f64 / 60.0;
    let p = Params {
        max_page_work: per * 1.05,
        max_doc_work: per,
        doc_work_per_page: per * 2.0,
        max_doc_work_total: per * 20.5,
        ..Params::default()
    };
    let r = many_pages(&one, 60, &p, Some(4));
    let degraded: Vec<usize> = r.degraded.iter().map(|(i, _)| *i).collect();
    let first = degraded.first().copied().expect("degraded pages");
    assert!((19..=21).contains(&first), "{first}");
    assert_eq!(degraded, (first..60).collect::<Vec<_>>());
    assert!(r.degraded.iter().all(|(_, d)| *d == Degraded::DocWork));
    // the page that runs out is charged what it used before it stopped
    assert!(r.doc_work as f64 <= p.max_doc_work_total + per, "{} > {}", r.doc_work, p.max_doc_work_total);
    assert_eq!(r.doc_work_limit, p.max_doc_work_total);
    // the defaults: 50 million, 20,000 per page, at most 250 million (10,000 pages' growth)
    let d = Params::default();
    assert_eq!(d.doc_work_limit(1), 5.0e7 + 2.0e4);
    assert_eq!(d.doc_work_limit(10_000), 2.5e8);
    assert_eq!(d.doc_work_limit(usize::MAX), 2.5e8);
    // without growth, the first page past the fixed budget is where it stops
    let fixed = Params { doc_work_per_page: 0.0, max_doc_work: per * 5.5, ..p.clone() };
    let r = many_pages(&one, 60, &fixed, Some(4));
    let first = r.degraded.first().expect("degraded pages").0;
    assert!((4..=6).contains(&first), "{first}");
}
