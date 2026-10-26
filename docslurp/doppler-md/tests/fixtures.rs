//! Synthetic fixtures (tests/fixtures, made by scripts/make_fixtures.py) against the reference
//! output of doppler_md.py.

use std::path::Path;

use doppler_md::{Params, to_markdown};

fn check(name: &str) {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let xhtml = std::fs::read_to_string(dir.join(format!("{name}.xhtml"))).unwrap();
    let want = std::fs::read_to_string(dir.join(format!("{name}.md"))).unwrap();
    let got = to_markdown(&xhtml, &Params::default()).join("\x0c");
    assert_eq!(got, want, "fixture {name}");
}

#[test]
fn two_columns() {
    check("two_columns");
}

#[test]
fn rtl() {
    check("rtl");
}

#[test]
fn ruled_table() {
    check("ruled_table");
}

#[test]
fn gutter_table() {
    check("gutter_table");
}

#[test]
fn lists_headings() {
    check("lists_headings");
}

#[test]
fn joins() {
    check("joins");
}

#[test]
fn furniture() {
    check("furniture");
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let xhtml = std::fs::read_to_string(dir.join("furniture.xhtml")).unwrap();
    let pages = to_markdown(&xhtml, &Params::default());
    assert_eq!(pages.len(), 3);
    assert!(pages.iter().all(|p| !p.contains("Running Header")), "repeated header dropped");
}

#[test]
fn params_override() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let xhtml = std::fs::read_to_string(dir.join("gutter_table.xhtml")).unwrap();
    let mut p = Params::default();
    p.apply_json(r#"{"tables": false, "unknown_key": 1}"#).unwrap();
    let md = to_markdown(&xhtml, &p).join("\x0c");
    assert!(!md.contains("| --- |"));
    assert!(p.apply_json(r#"{"cell_gap": "wide"}"#).is_err());
    assert!(p.apply_json("[1]").is_err());
}
