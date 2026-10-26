//! Unit tests for the pieces where Python semantics are easiest to get wrong. Expected values
//! were cross-checked against doppler_md.py.

use std::rc::Rc;

use doppler_md::layout::{Elem, bands, regions};
use doppler_md::model::{Line, Word};
use doppler_md::pyc::{self, html_unescape, median, round0, round1};
use doppler_md::tables::{Row, build_grid, gutter_columns, segments};
use doppler_md::{Params, to_markdown};

fn word(x0: f64, x1: f64, y: f64, text: &str) -> Word {
    Word {
        x0,
        y0: y,
        x1,
        y1: y + 10.0,
        text: Rc::from(text),
        bold: false,
        italic: false,
        size: 10.0,
        base: y + 8.0,
        space: true,
        color: Rc::from("#000000"),
    }
}

#[test]
fn median_matches_statistics_median() {
    assert_eq!(median(vec![3.0, 1.0, 2.0]), 2.0);
    assert_eq!(median(vec![4.0, 1.0, 3.0, 2.0]), 2.5); // mean of the two middle values
    assert_eq!(median(vec![7.0]), 7.0);
    assert!(median(vec![]).is_nan()); // Python raises; NaN compares false everywhere
}

#[test]
fn python_rounding() {
    assert_eq!(round0(2.5), 2.0);
    assert_eq!(round0(3.5), 4.0);
    assert_eq!(round0(-0.5), -0.0);
    assert_eq!(round1(0.25), 0.2);
    assert_eq!(round1(0.75), 0.8);
    assert_eq!(round1(2.45), 2.5); // 2.45 is stored as 2.4500000000000001776...
    assert_eq!(round1(72.25), 72.2);
}

#[test]
fn python_strings() {
    assert_eq!(pyc::strip("\u{1f}\u{a0} x \u{3000}"), "x");
    assert_eq!(pyc::split_count(" a\u{2003}b  c "), 3);
    assert_eq!(pyc::py_float(" 1_000.5 "), Some(1000.5));
    assert_eq!(pyc::py_float("1__0"), None);
    assert_eq!(pyc::py_float("nan"), None);
    assert_eq!(pyc::py_hex(" f"), Some(15));
    assert_eq!(pyc::cp_slice("#\u{e9}0a", 1, 3), "\u{e9}0");
}

#[test]
fn float_fast_path_matches_std_parse() {
    let mut cases: Vec<String> = [
        "0",
        "-0",
        "-0.0",
        "5.",
        "0.000001",
        "254.380000",
        "999999999999999",
        "9999999999999999",
        "0.1234567890123456789",
        "123456789012345.6",
        "-12.5e3",
        ".5",
        "-.5",
        "1.2.3",
        "",
        "-",
        ".",
        "00012.50",
        "1e400",
        "1,5",
        "12a",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    // deterministic pseudo-random decimals of every digit count and point position
    let mut x: u64 = 0x9e3779b97f4a7c15;
    for _ in 0..20000 {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        let digits = 1 + (x % 24) as usize;
        let mut s: String = (0..digits).map(|k| char::from(b'0' + ((x >> (k % 60)) % 10) as u8)).collect();
        let point = ((x >> 40) as usize) % (digits + 2);
        if point <= digits {
            s.insert(point, '.');
        }
        if x & 1 == 1 {
            s.insert(0, '-');
        }
        cases.push(s);
    }
    for s in &cases {
        let want = s.parse::<f64>().ok().filter(|v| v.is_finite());
        let got = pyc::py_float(s);
        assert_eq!(got.map(f64::to_bits), want.map(f64::to_bits), "{s:?}");
    }
}

#[test]
fn unescape_matches_html_module() {
    assert_eq!(html_unescape("a &amp; b &lt;c&gt; &quot;&apos;"), "a & b <c> \"'");
    assert_eq!(html_unescape("&#65;&#x42;&#0;&#128;&#xD800;&#1;"), "AB\u{fffd}\u{20ac}\u{fffd}");
    assert_eq!(html_unescape("&ampx &notit; &zzz;"), "&x ¬it; &zzz;");
}

#[test]
fn rtl_line_reads_right_to_left_keeping_ltr_runs() {
    // visual order: [shalom] [2026] [x] [olam]
    let words = vec![
        word(10.0, 30.0, 0.0, "שלום"),
        word(35.0, 55.0, 0.0, "2026"),
        word(60.0, 65.0, 0.0, "x"),
        word(70.0, 90.0, 0.0, "עולם"),
    ];
    let line = Line::new(words, 0, 0);
    assert!(line.rtl());
    assert_eq!(line.text(), "עולם 2026 x שלום");
    // ",28" drawn left of a number in a right-to-left line reads "28,"
    let line = Line::new(vec![word(10.0, 30.0, 0.0, ",28"), word(35.0, 55.0, 0.0, "מספר")], 0, 0);
    assert_eq!(line.text(), "מספר 28,");
    // a mostly Latin line stays left to right
    let line = Line::new(vec![word(10.0, 30.0, 0.0, "hello"), word(35.0, 55.0, 0.0, "שלום")], 0, 0);
    assert!(!line.rtl());
    assert_eq!(line.text(), "hello שלום");
}

fn elem(x0: f64, x1: f64, y0: f64, y1: f64, line: usize) -> Elem {
    Elem { line: Some(line), table: 0, x0, x1, y0, y1 }
}

#[test]
fn bands_split_where_nothing_overlaps() {
    // boxes overlap by less than 20% of their height: still separate bands
    let es = vec![
        elem(0.0, 10.0, 0.0, 10.0, 0),
        elem(20.0, 30.0, 1.0, 11.0, 1),
        elem(0.0, 10.0, 9.0, 19.0, 2),
        elem(0.0, 10.0, 30.0, 40.0, 3),
    ];
    let b = bands(&es);
    let ids: Vec<Vec<usize>> = b.iter().map(|band| band.iter().map(|e| e.line.unwrap()).collect()).collect();
    assert_eq!(ids, vec![vec![0, 1], vec![2], vec![3]]);
}

fn prose_line(x0: f64, x1: f64, y: f64) -> Rc<Line> {
    let n = 6;
    let step = (x1 - x0) / n as f64;
    let words = (0..n).map(|k| word(x0 + k as f64 * step, x0 + (k + 1) as f64 * step - 3.0, y, "word")).collect();
    Rc::new(Line::new(words, 0, 0))
}

#[test]
fn gutter_follows_columns_and_narrows() {
    // three two-column bands; the third left line reaches into the gutter (300 -> 312), which
    // narrows it but keeps it wider than column_gutter x body; then a full-width line.
    let mut lines = Vec::new();
    let mut es = Vec::new();
    for (k, y) in [100.0, 112.0, 124.0].into_iter().enumerate() {
        let lx1 = if k == 2 { 312.0 } else { 300.0 };
        lines.push(prose_line(72.0, lx1, y));
        lines.push(prose_line(330.0, 560.0, y));
    }
    lines.push(prose_line(72.0, 560.0, 140.0));
    for (i, l) in lines.iter().enumerate() {
        es.push(elem(l.x0, l.x1, l.y0, l.y1, i));
    }
    let p = Params::default();
    let got: Vec<Vec<usize>> =
        regions(&es, &lines, 10.0, &p, 0).iter().map(|r| r.iter().map(|e| e.line.unwrap()).collect()).collect();
    assert_eq!(got, vec![vec![0, 2, 4], vec![1, 3, 5], vec![6]]);

    // reaching to 325 leaves a 5pt gutter: the run stops above that band, and two bands with
    // two lines a side are still a column split
    let mut lines2 = lines.clone();
    lines2[4] = prose_line(72.0, 325.0, 124.0);
    let mut es2 = es.clone();
    es2[4] = elem(lines2[4].x0, lines2[4].x1, lines2[4].y0, lines2[4].y1, 4);
    let got: Vec<Vec<usize>> =
        regions(&es2, &lines2, 10.0, &p, 0).iter().map(|r| r.iter().map(|e| e.line.unwrap()).collect()).collect();
    assert_eq!(got, vec![vec![0, 2], vec![1, 3], vec![4, 5, 6]]);
}

#[test]
fn region_recursion_is_capped() {
    // deeply nested gutters: recursion stops at the cap without overflowing the stack
    let mut lines = Vec::new();
    let mut es = Vec::new();
    for col in 0..64 {
        for row in 0..4 {
            let x0 = col as f64 * 40.0;
            let l = prose_line(x0, x0 + 20.0, row as f64 * 12.0);
            es.push(elem(l.x0, l.x1, l.y0, l.y1, lines.len()));
            lines.push(l);
        }
    }
    let r = regions(&es, &lines, 10.0, &Params::default(), 0);
    assert_eq!(r.iter().map(Vec::len).sum::<usize>(), es.len());
}

fn row(base: f64, cells: &[(f64, f64, &str)]) -> Row {
    let cells: Vec<Rc<Line>> =
        cells.iter().map(|&(a, b, t)| Rc::new(Line::new(vec![word(a, b, base - 8.0, t)], 0, 0))).collect();
    let multi = cells.len() >= 2;
    Row { base, ids: vec![], cells, size: 10.0, multi }
}

#[test]
fn table_grid_from_gutters() {
    let p = Params::default();
    let rows = vec![
        row(10.0, &[(72.0, 100.0, "Item"), (240.0, 260.0, "Qty"), (360.0, 390.0, "Price")]),
        row(24.0, &[(72.0, 110.0, "Widget"), (240.0, 250.0, "12"), (360.0, 380.0, "4.50")]),
        row(38.0, &[(72.0, 110.0, "Gadget"), (240.0, 245.0, "7"), (360.0, 385.0, "11.25")]),
        // a continuation line in the first column only, close under the row above
        row(45.0, &[(72.0, 105.0, "(blue)")]),
    ];
    let cols = gutter_columns(&rows, &p);
    assert_eq!(cols, vec![(72.0, 110.0), (240.0, 260.0), (360.0, 390.0)]);
    let grid = build_grid(&rows, &cols, &p);
    assert_eq!(
        grid,
        vec![vec!["Item", "Qty", "Price"], vec!["Widget", "12", "4.50"], vec!["Gadget (blue)", "7", "11.25"],]
    );
}

#[test]
fn huge_coordinates_do_not_allocate_by_width() {
    // xMax=1e9 would make Python allocate a billion-slot coverage list; the run-length scan is
    // proportional to the number of words.
    let p = Params::default();
    let rows = vec![
        row(10.0, &[(72.0, 100.0, "a"), (240.0, 1e9, "b")]),
        row(24.0, &[(72.0, 100.0, "c"), (240.0, 1e9, "d")]),
        row(38.0, &[(72.0, 100.0, "e"), (240.0, 1e9, "f")]),
    ];
    let cols = gutter_columns(&rows, &p);
    assert_eq!(cols, vec![(72.0, 100.0), (240.0, 1e9)]);
    let rows = vec![
        row(10.0, &[(-1e300, 0.0, "a"), (240.0, 1e300, "b")]),
        row(24.0, &[(-1e300, 0.0, "c"), (240.0, 1e300, "d")]),
    ];
    assert_eq!(gutter_columns(&rows, &p).len(), 2);
}

#[test]
fn segments_split_at_wide_gaps() {
    let line = Line::new(vec![word(0.0, 20.0, 0.0, "a"), word(23.0, 40.0, 0.0, "b"), word(60.0, 80.0, 0.0, "c")], 0, 0);
    let segs = segments(&line, &Params::default());
    assert_eq!(segs.iter().map(|s| s.text().to_string()).collect::<Vec<_>>(), vec!["a b", "c"]);
}

#[test]
fn hostile_inputs_do_not_panic() {
    let p = Params::default();
    for s in [
        "",
        "<",
        "<page",
        "<page width=\"x\"><line><word xMin=\"1\">t</word></line>",
        "<line><word xMin=\"1\" yMin=\"1\" xMax=\"2\" yMax=\"2\">orphan</word></line>",
        "<page><rule xMin=\"nan\"/><box xMin=\"1\" yMin=\"1\" xMax=\"1e308\" yMax=\"1e308\" color=\"#\u{e9}\"/></page>",
        "<page width=\"1e308\" height=\"-1\"><flow><block><line><word xMin=\"-1e308\" yMin=\"0\" xMax=\"1e308\" yMax=\"1\" fontSize=\"0\">&#99999999999999;&bogus</word></line></block></flow></page>",
        "<page><line><word xMin=\"1\" yMin=\"1\" xMax=\"0\" yMax=\"0\" fontSize=\"-3\" baseline=\"\">\u{5d0}</word></line></line></page>",
    ] {
        let _ = to_markdown(s, &p);
    }
}
