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
    // `pdf` is relative to tests/fixtures/tables (data/ holds the cell-content cases)
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

/// A page of matrices: with `--math` they are formulas, never tables (without it a matrix may
/// come out as a table, as on master: no table is ever vetoed from its text).
#[test]
fn matrices_are_formulas_with_math() {
    let Some(exe) = pdftotext() else { return };
    let md = convert(&exe, "matrix.pdf", &["--math"]);
    assert!(table(&md).is_empty(), "{md}");
    assert!(md.contains("\\begin{pmatrix}") && md.contains("\\begin{vmatrix}"), "{md}");
    assert!(convert(&exe, "matrix.pdf", &[]).contains("Both segments grow next year."));
}

/// Data tables whose cells hold signs that also occur in math (`±`, `≥`, `=`) and short
/// letter labels stay tables, borderless and ruled: no cell text vetoes a table.
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
/// `0.020.180.80`).
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
}

/// Data tables whose cells hold what a cell-text heuristic could take for math: enumerated row
/// labels (`a) Revenue`, `1)`), footnote marks (`Bayern 1)`), half-open bins (`[0, 10)`),
/// bars (`-q | --quiet`, `Ctrl | C`), math italic letters (`𝑥 | 𝑓(𝑥)`, `Run | 𝑛 | 𝑝 | 𝑡`) and
/// check marks (`√`), borderless and ruled: each is one table, the same with `--math`.
#[test]
fn cell_text_never_vetoes_a_table() {
    let Some(exe) = pdftotext() else { return };
    let cases: [(&str, &str); 10] = [
        ("enum", "| a) Revenue | 1,204 | 1,390 |"),
        ("ruledenum", "| a) Revenue | 1,204 | 1,390 |"),
        ("footnote", "| Bayern 1) | 1.204 | 1.390 |"),
        ("bins2", "| [0, 10) | 412 |"),
        ("flags", "| -q \\| --quiet | 0 |"),
        ("pipe", "| Ctrl \\| C | 12 |"),
        ("mathital", "| 𝑥 | 𝑓(𝑥) | 𝑔(𝑥) |"),
        ("mathitalhdr", "| Run | 𝑛 | 𝑝 | 𝑡 |"),
        ("check", "| Export | √ | √ | √ | √ |"),
        ("numlabels", "| 1) | 15 | A |"),
    ];
    for (name, line) in cases {
        let pdf = format!("data/{name}.pdf");
        let plain = convert(&exe, &pdf, &[]);
        let math = convert(&exe, &pdf, &["--math"]);
        assert_eq!(plain.lines().filter(|l| l.starts_with("| ---")).count(), 1, "{name}:\n{plain}");
        assert!(table(&plain).contains(&line), "{name}: {line}\n{plain}");
        assert_eq!(table(&math), table(&plain), "{name} --math:\n{math}");
    }
}

/// Every table repro keeps at least its tables, in default mode and with `--math` (the counts
/// are those of master or better; with `--math` a matrix set between big delimiters reads as a
/// formula, so it may leave one table fewer, and a table beside it stays a table): tables with
/// fractions, determinants, a pmatrix or brace-grouped rows in their cells, tables beside
/// matrices, matrices themselves (a table without `--math`, as on master), and the cell-content
/// cases. No table may become prose.
#[test]
fn every_repro_keeps_its_tables() {
    let Some(exe) = pdftotext() else { return };
    // (fixture under data/, at least this many tables by default, at least this many with --math)
    let cases: [(&str, usize, usize); 33] = [
        ("rmw_2", 1, 1),
        ("segrules2", 1, 1),
        ("captions3", 3, 3),
        ("absfrac", 1, 1),
        ("absfrac2", 1, 1),
        ("adjmatrix", 1, 1),
        ("bigbracket", 1, 1),
        ("bigvert", 1, 1),
        ("binom", 1, 1),
        ("blocks", 1, 1),
        ("blocks3", 1, 1),
        ("brace", 1, 1),
        ("detcol", 1, 1),
        ("fracparen", 1, 1),
        ("fracparen2", 1, 1),
        ("gates", 1, 1),
        ("gatesbm", 1, 1),
        ("pcell-booktabs", 1, 1),
        ("pcell-grid", 1, 1),
        ("sidebyside2", 1, 1),
        ("sidebmat", 2, 1),
        ("sidematrix", 2, 1),
        ("sidematrix2", 1, 1),
        ("twocol", 2, 1),
        ("m2", 1, 0),
        ("m5", 1, 0),
        ("mx3_0pt", 1, 0),
        ("enum", 1, 1),
        ("footnote", 1, 1),
        ("bins2", 1, 1),
        ("mathital", 1, 1),
        ("check", 1, 1),
        ("numlabels", 1, 1),
    ];
    let count = |md: &str| md.lines().filter(|l| l.starts_with("| ---")).count();
    let mut short = Vec::new();
    for (name, plain_min, math_min) in cases {
        let pdf = format!("data/{name}.pdf");
        let plain = count(&convert(&exe, &pdf, &[]));
        let math = count(&convert(&exe, &pdf, &["--math"]));
        if plain < plain_min || math < math_min {
            short.push(format!("{name}: {plain} (want {plain_min}), --math {math} (want {math_min})"));
        }
    }
    assert!(short.is_empty(), "{short:#?}");
}

/// A table beside a matrix, and booktabs rows with fractions in their cells, keep their label
/// column and every row (master lost the label column of a booktabs table with |frac| cells).
#[test]
fn tables_with_math_cells_keep_their_rows() {
    let Some(exe) = pdftotext() else { return };
    let md = convert(&exe, "data/absfrac.pdf", &[]);
    assert!(table(&md).iter().any(|l| l.starts_with("| Solver | Error | Bound | Steps |")), "{md}");
    assert!(table(&md).iter().any(|l| l.starts_with("| RK4 |")), "{md}");
    let md = convert(&exe, "data/absfrac2.pdf", &[]);
    for row in ["| Euler |", "| Heun |", "| RK4 |"] {
        assert!(table(&md).iter().any(|l| l.starts_with(row)), "{row}\n{md}");
    }
    let md = convert(&exe, "data/gates.pdf", &[]);
    for row in ["| Hadamard |", "| Phase |"] {
        assert!(table(&md).iter().any(|l| l.starts_with(row)), "{row}\n{md}");
    }
    let md = convert(&exe, "data/sidebyside2.pdf", &[]);
    assert!(table(&md).iter().any(|l| l.starts_with("| From | Sunny | Rainy | Total |")), "{md}");
}

/// Tables bracketed by thick rules (2 pt booktabs rules, and rules drawn as filled bars per
/// column) keep one row per line with their numbers apart, in both modes; and three ruled
/// tables, each under its "Table N." caption, stay three tables with the captions outside.
#[test]
fn thick_rules_and_captioned_tables() {
    let Some(exe) = pdftotext() else { return };
    for args in [&[][..], &["--math"][..]] {
        let md = convert(&exe, "data/rmw_2.pdf", args);
        assert!(table(&md).contains(&"| Total | 39695 | 100.0 | 100.0 |"), "{md}");
        assert!(table(&md).contains(&"| Home examined | 493 | 1.2 | 0.8 |"), "{md}");
        let md = convert(&exe, "data/segrules2.pdf", args);
        assert!(
            table(&md).iter().any(|l| l.starts_with("| Total |") && l.contains("| 39695 | 100.0 | 100.0 |")),
            "{md}"
        );
        assert!(table(&md).iter().any(|l| l.contains("Sample size")), "{md}");
        let md = convert(&exe, "data/captions3.pdf", args);
        assert_eq!(md.lines().filter(|l| l.starts_with("| ---")).count(), 3, "{md}");
        assert!(table(&md).iter().all(|l| !l.contains("Table ")), "{md}");
        assert!(table(&md).contains(&"| Plan | Q1 | Q2 | Q3 |"), "{md}");
    }
}

/// A rule under every row and a first-column cell that wraps to three lines: the taller band
/// is that row, not a gap between two tables, so the header and the first row stay in the
/// table (they once fell into the caption's paragraph), in both modes.
#[test]
fn rule_under_every_row_with_a_wrapped_label() {
    let Some(exe) = pdftotext() else { return };
    for args in [&[][..], &["--math"][..]] {
        let md = convert(&exe, "data/allrules_label.pdf", args);
        assert_eq!(md.lines().filter(|l| l.starts_with("| ---")).count(), 1, "{md}");
        assert!(table(&md).contains(&"| Item | 2025 | 2024 |"), "{md}");
        assert!(
            table(&md).iter().any(|l| l.starts_with("| Cost of revenue") && l.ends_with("| 1,204 | 1,033 |")),
            "{md}"
        );
        assert!(table(&md).contains(&"| Total operating expenses | 2,972 | 2,562 |"), "{md}");
    }
}

/// Two tables ruled under every row, stacked with a gap (and, in one case, a heading line
/// between them): two tables, the second one's header its own header, the heading prose, in
/// both modes (they once merged into one table with the heading as a row).
#[test]
fn stacked_row_ruled_tables_stay_apart() {
    let Some(exe) = pdftotext() else { return };
    for args in [&[][..], &["--math"][..]] {
        for name in ["two_stack_gap", "stack_head2", "stack_wrapgap"] {
            let md = convert(&exe, &format!("data/{name}.pdf"), args);
            assert_eq!(md.lines().filter(|l| l.starts_with("| ---")).count(), 2, "{name} {args:?}\n{md}");
        }
        let md = convert(&exe, "data/two_stack_gap.pdf", args);
        assert!(table(&md).contains(&"| Segment | 2025 | 2024 |"), "{md}");
        let md = convert(&exe, "data/stack_head2.pdf", args);
        assert!(md.lines().any(|l| l.starts_with("Cash flow highlights")), "{md}");
        assert!(table(&md).iter().all(|l| !l.contains("Cash flow highlights")), "{md}");
    }
}

/// Ten pages of 200 thin full-width rules with 95 tiny one-word lines between each pair, beside
/// a block of large prose: each band between two rules is read once per rule group (reading
/// every other band again for each band made the ten pages take about 9 s, against 2.7 s).
#[test]
fn many_rule_bands_are_read_once() {
    let Some(exe) = pdftotext() else { return };
    let (rules, per, pitch) = (200usize, 95usize, 1.3f64);
    let band = (per + 1) as f64 * pitch;
    let h = (rules as f64 * band + 100.0).max(300.0 * 24.0 + 100.0);
    let mut ops = String::from("0.3 w\n");
    let mut y = h - 50.0;
    for r in 0..rules {
        ops += &format!("50 {y:.3} m 550 {y:.3} l S\n");
        if r + 1 < rules {
            ops += &format!("BT /F1 {:.2} Tf\n", pitch * 0.8);
            for j in 0..per {
                ops += &format!("1 0 0 1 60 {:.3} Tm (w) Tj\n", y - (j + 1) as f64 * pitch);
            }
            ops += "ET\n";
        }
        y -= band;
    }
    ops += "BT /F1 20 Tf\n";
    let words = "the quarterly results beat every estimate and the team flew to london to open a new office ".repeat(3);
    for j in 0..300 {
        ops += &format!("1 0 0 1 700 {} Tm ({}) Tj\n", h - 50.0 - j as f64 * 24.0, &words[..200]);
    }
    ops += "ET\n";
    // ten pages sharing one content stream (objects 3 and 4), the pages from object 5
    let pages = 10;
    let kids: Vec<String> = (0..pages).map(|k| format!("{} 0 R", 5 + k)).collect();
    let mut objects = vec![
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        format!("<< /Type /Pages /Kids [{}] /Count {pages} >>", kids.join(" ")),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
        format!("<< /Length {} >>\nstream\n{ops}endstream", ops.len()),
    ];
    for _ in 0..pages {
        objects.push(format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 3400 {h:.0}] /Resources << /Font << /F1 3 0 R >> >> /Contents 4 0 R >>"
        ));
    }
    let mut pdf = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::new();
    for (i, o) in objects.iter().enumerate() {
        offsets.push(pdf.len());
        pdf.extend_from_slice(format!("{} 0 obj\n{o}\nendobj\n", i + 1).as_bytes());
    }
    let xref = pdf.len();
    let mut tail = format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1);
    for o in offsets {
        tail += &format!("{o:010} 00000 n \n");
    }
    tail += &format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objects.len() + 1);
    pdf.extend_from_slice(tail.as_bytes());
    let path = std::env::temp_dir().join(format!("rule-bands-{}.pdf", std::process::id()));
    std::fs::write(&path, pdf).unwrap();
    let t = std::time::Instant::now();
    let out = Command::new(env!("CARGO_BIN_EXE_pdftomarkdown")).arg(&path).env("PDFTOTEXT", &exe).output().unwrap();
    let took = t.elapsed();
    let _ = std::fs::remove_file(&path);
    assert_eq!(out.status.code(), Some(0), "{}", String::from_utf8_lossy(&out.stderr));
    assert!(took < std::time::Duration::from_secs(5), "{took:?}");
}
