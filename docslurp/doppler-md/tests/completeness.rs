//! Producer rule completeness survives parsing, conversion and report serialization.

use doppler_md::model::{ParseReport, parse_report};
use doppler_md::{Degraded, Params, to_markdown_report};
use serde_json::{Value, json};

fn page(marker: &str, body: &str) -> String {
    format!("<page width=\"612\" height=\"792\" {marker}>{body}</page>")
}

fn line(text: &str) -> String {
    format!("<line><word xMin=\"50\" yMin=\"200\" xMax=\"100\" yMax=\"210\">{text}</word></line>")
}

#[test]
fn absent_complete_truncated_and_unrecognized_markers_stay_distinct() {
    let markers =
        ["", "rulesTruncated=\"0\"", "rulesTruncated=\"1\"", "rulesTruncated=\"true\"", "rulesTruncated=\"\""];
    let x: String = markers.iter().map(|m| page(m, "")).collect();
    let mut report = ParseReport::default();
    let pages = parse_report(&x, &mut report);
    assert_eq!(
        pages.iter().map(|p| p.rules_truncated).collect::<Vec<_>>(),
        [None, Some(false), Some(true), None, None]
    );
    assert_eq!(report.rules_truncated_pages, [2]);
    let (markdown, report) = to_markdown_report(&x, &Params::default());
    assert_eq!(markdown, ["", "", "", "", ""]);
    let json: Value = serde_json::from_str(&report.to_json()).unwrap();
    assert_eq!(json["rules_truncated_pages"], json!([3]));
    assert!(report.degraded.is_empty());
}

#[test]
fn incomplete_evidence_does_not_require_any_kept_rules_or_a_closed_page() {
    let x = "<page rulesTruncated=\"1\"><rule xMin=\"0\" yMin=\"nan\" xMax=\"1\" yMax=\"1\"/>";
    let (markdown, report) = to_markdown_report(x, &Params::default());
    assert_eq!(markdown, [""]);
    assert_eq!(report.parse.rules_truncated_pages, [0]);
    assert_eq!(report.parse.rules_skipped, 1);
    assert_eq!(report.parse.page_closes, 0);
    assert!(report.degraded.is_empty());
}

#[test]
fn complete_and_truncated_at_the_producer_cap_have_the_same_layout() {
    // These short rules are kept by the decoder but do not make a table. Exactly at the
    // default cap, only the producer knows whether any further rules were discarded.
    let rules = "<rule xMin=\"1\" yMin=\"1\" xMax=\"2\" yMax=\"1\"/>".repeat(20_000);
    let body = format!("{}{rules}", line("kept"));
    let complete = page("rulesTruncated=\"0\"", &body);
    let truncated = page("rulesTruncated=\"1\"", &body);
    let (complete_md, complete_report) = to_markdown_report(&complete, &Params::default());
    let (truncated_md, truncated_report) = to_markdown_report(&truncated, &Params::default());
    assert_eq!(complete_md, ["kept"]);
    assert_eq!(complete_md, truncated_md);
    assert!(complete_report.parse.rules_truncated_pages.is_empty());
    assert_eq!(truncated_report.parse.rules_truncated_pages, [0]);
    assert!(complete_report.degraded.is_empty() && truncated_report.degraded.is_empty());
    assert_eq!(complete_report.doc_work, truncated_report.doc_work);
    assert_eq!(complete_report.page_work_max, truncated_report.page_work_max);

    // Consumer layout limits remain independent of the producer's marker.
    let p = Params { max_page_rules: 19_999.0, ..Params::default() };
    let (_, report) = to_markdown_report(&truncated, &p);
    assert_eq!(report.degraded, [(0, Degraded::PageRules)]);
    assert_eq!(report.parse.rules_truncated_pages, [0]);
}

#[test]
fn later_document_exhaustion_does_not_hide_upstream_truncation() {
    let x = [
        page("rulesTruncated=\"1\"", &line("alpha")),
        page("rulesTruncated=\"0\"", &line("beta")),
        page("rulesTruncated=\"1\"", &line("gamma")),
    ]
    .concat();
    let p = Params { max_doc_work: 2.0, ..Params::default() };
    let (markdown, report) = to_markdown_report(&x, &p);
    assert_eq!(markdown, ["alpha", "beta", "gamma"]);
    assert_eq!(report.parse.rules_truncated_pages, [0, 2]);
    assert_eq!(report.degraded, [(1, Degraded::DocWork), (2, Degraded::DocWork)]);
    let json: Value = serde_json::from_str(&report.to_json()).unwrap();
    assert_eq!(json["rules_truncated_pages"], json!([1, 3]));
    assert_eq!(json["degraded"], json!([{"page": 2, "reason": "doc_work"}, {"page": 3, "reason": "doc_work"}]));
}

#[test]
fn marker_does_not_change_existing_table_output_or_work() {
    let x = include_str!("fixtures/ruled_table.xhtml");
    let marked = x.replacen("<page ", "<page rulesTruncated=\"1\" ", 1);
    let (original, before) = to_markdown_report(x, &Params::default());
    let (converted, after) = to_markdown_report(&marked, &Params::default());
    assert_eq!(converted, original);
    assert!(converted[0].contains("| --- |"));
    assert_eq!(after.parse.rules_truncated_pages, [0]);
    assert_eq!(after.doc_work, before.doc_work);
    assert_eq!(after.page_work_max, before.page_work_max);
    assert_eq!(after.degraded, before.degraded);
}
