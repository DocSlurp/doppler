//! Tables in real PDFs from several producers (fixtures in `tests/fixtures/tables`, rebuilt by
//! its `gen.sh`): a booktabs table with a spanning header, a label column spanning rows and
//! wrapped cells; a fully ruled LaTeX grid; a whitespace-only LaTeX table whose headers are
//! wider than their columns; bordered and borderless Chromium tables; a LibreOffice Writer
//! table; data tables with ±, ≥, = and letter labels; and a page of matrices that must not
//! become a table. Each is read by the real pdftotext, with and without `--math`: the tables
//! must come out the same either way.
//! Skipped without `$PDFTOTEXT`, unless `DOPPLER_REQUIRE_INTEGRATION` is set (tests/toolchain).

mod toolchain;

use std::path::{Path, PathBuf};
use std::process::Command;

fn convert(exe: &Path, pdf: &str, args: &[&str]) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/tables").join(pdf);
    let out =
        Command::new(env!("CARGO_BIN_EXE_pdftomarkdown")).args(args).arg(&path).env("PDFTOTEXT", exe).output().unwrap();
    assert_eq!(out.status.code(), Some(0), "{pdf}: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8(out.stdout).unwrap()
}

fn pdftotext() -> Option<PathBuf> {
    let exe = std::env::var_os("PDFTOTEXT").filter(|p| !p.is_empty()).map(PathBuf::from);
    if exe.is_none() {
        toolchain::skip("table_pdfs", "PDFTOTEXT set to the DocSlurp pdftotext");
    }
    exe
}

/// The table lines of a page (`|` rows), in order.
fn table(md: &str) -> Vec<&str> {
    md.lines().filter(|l| l.starts_with('|')).collect()
}

fn check(pdf: &str, expected: &[&str]) {
    let Some(exe) = pdftotext() else { return };
    for args in [&[][..], &["--math"][..]] {
        let md = convert(&exe, pdf, args);
        assert_eq!(table(&md), expected, "{pdf} {args:?}:\n{md}");
        assert!(!md.contains('$'), "{pdf} {args:?}: no formula in a table page:\n{md}");
    }
}

#[test]
fn booktabs_with_spans_and_wrapped_cells() {
    check(
        "booktabs.pdf",
        &[
            "| Valley | Orchard | Harvest (t) |  | Notes |",
            "| --- | --- | --- | --- | --- |",
            "|  |  | 2025 | 2026 |  |",
            "| North | Alder Farm | 140 | 210 | New cold store opens in autumn |",
            "|  | Birch Hill | 65 | 95 | Second picking crew |",
            "| South | Cedar Row | 80 | 130 | Record pear yield |",
            "|  | Dune Acres | 45 | 75 | Drip irrigation added this spring |",
            "| All valleys |  | 330 | 510 | Ahead of target |",
        ],
    );
}

#[test]
fn ruled_latex_grid() {
    check(
        "grid.pdf",
        &[
            "| Venue | Spring | Summer | Change |",
            "| --- | --- | --- | --- |",
            "| Harbour Hall | 4,120 | 5,380 | +30.6% |",
            "| City Arena | 9,450 | 11,020 | +16.6% |",
            "| Garden Stage | 1,875 | 2,640 | +40.8% |",
            "| Total | 15,445 | 19,040 | +23.3% |",
        ],
    );
}

#[test]
fn whitespace_table_with_wide_headers() {
    check(
        "norules.pdf",
        &[
            "| House | Crop | Temp (C) | Growth (cm) | Gain (%) |",
            "| --- | --- | --- | --- | --- |",
            "| East | Basil | 24.5 | 31.2 | 18.4 |",
            "| West | Tomato | 22.1 | 48.9 | 12.7 |",
            "| North | Pepper | 23.8 | 27.4 | 15.9 |",
            "| South | Lettuce | 19.6 | 22.3 | 9.8 |",
        ],
    );
}

const LIBRARY: [&str; 6] = [
    "| Branch | Visits | Loans | Change |",
    "| --- | --- | --- | --- |",
    "| Riverside | 12,480 | 8,210 | +14.2% |",
    "| Old Town | 9,315 | 6,040 | +9.6% |",
    "| Hillcrest | 4,870 | 3,155 | +21.3% |",
    "| Total | 26,665 | 17,405 | +13.7% |",
];

#[test]
fn chromium_bordered_and_borderless() {
    check("bordered.pdf", &LIBRARY);
    check("borderless.pdf", &LIBRARY);
}

#[test]
fn libreoffice_writer_table() {
    check(
        "writer.pdf",
        &[
            "| Shop | Bread | Cakes | Change |",
            "| --- | --- | --- | --- |",
            "| Main Street | 2,140 | 860 | +18.5% |",
            "| Station | 1,320 | 410 | +11.0% |",
            "| Market | 980 | 395 | +24.1% |",
        ],
    );
}

#[test]
fn matrices_are_not_tables() {
    let Some(exe) = pdftotext() else { return };
    let md = convert(&exe, "matrix.pdf", &[]);
    assert!(table(&md).is_empty(), "{md}");
    assert!(md.contains("Both segments grow next year."), "{md}");
    let md = convert(&exe, "matrix.pdf", &["--math"]);
    assert!(table(&md).is_empty(), "{md}");
    assert!(md.contains("\\begin{pmatrix}") && md.contains("\\begin{vmatrix}"), "{md}");
}

/// Data tables whose cells hold signs that also occur in math (`±`, `≥`, `=`) and short
/// letter labels stay tables, borderless and ruled: only real math evidence (math-font
/// letters, big-delimiter pieces, operators of analysis, delimiters cut by a gutter) makes a
/// grid read as math.
#[test]
fn signs_and_letter_labels_are_data() {
    let expected = [
        "| Sample | Mean ± SD | n |",
        "| --- | --- | --- |",
        "| A | 12.3 ± 0.4 | 5 |",
        "| B | 11.1 ± 0.6 | 6 |",
        "| C | 9.8 ± 0.2 | 4 |",
        "| D | 10.4 ± 0.3 | 7 |",
        "| Grade | Score |",
        "| --- | --- |",
        "| A | ≥ 90 |",
        "| B | 80-89 |",
        "| C | 70-79 |",
        "| D | < 70 |",
        "| Parameter | Setting |",
        "| --- | --- |",
        "| x | = 5 |",
        "| y | = 10 |",
        "| z | = 2 |",
    ];
    check("signs.pdf", &expected);
    check("signs-ruled.pdf", &expected);
}

/// A 3x3 pmatrix whose big parentheses are TeX extension pieces (top and bottom halves): all
/// three rows are inside the delimiters (the third once fell out, its numbers fused into
/// `0.020.180.80`), and without `--math` the matrix is not a table.
#[test]
fn three_row_matrix_from_extension_pieces() {
    let Some(exe) = pdftotext() else { return };
    let md = convert(&exe, "matrix3.pdf", &["--math"]);
    assert!(
        md.contains(
            "$$P=\\begin{pmatrix}0.82&0.15&0.03\\\\0.10&0.80&0.10\\\\0.02&0.18&0.80\\end{pmatrix},\\pi P=\\pi.$$"
        ),
        "{md}"
    );
    let md = convert(&exe, "matrix3.pdf", &[]);
    assert!(table(&md).is_empty(), "{md}");
}
