//! A ruled header can continue into a numeric body with column dividers but no row rules.
use doppler_md::{Options, Params, TableFormat, to_markdown_report_with};

fn rule(x0: usize, y0: usize, x1: usize, y1: usize) -> String {
    format!(r##"<rule xMin="{x0}" yMin="{y0}" xMax="{x1}" yMax="{y1}" color="#000000"/>"##)
}

fn row(y: usize, cells: [&str; 3]) -> String {
    [64, 180, 280].into_iter().zip(cells).map(|(x, text)| format!(
        r#"<block><line><word xMin="{x}" xMax="{}" yMin="{}" yMax="{y}" fontSize="10" baseline="{y}">{text}</word></line></block>"#,
        x + text.len() * 5, y - 8
    )).collect()
}

fn page(interrupt: bool) -> String {
    let mut s = String::from(r#"<page width="500" height="700">"#);
    for y in [90, 104, 120, 270] {
        s += &rule(60, y, 360, y);
    }
    for x in [160, 260] {
        s += &rule(x, 90, x, 120);
        for y in (120..270).step_by(15) {
            if !(interrupt && y == 150) {
                s += &rule(x, y, x, y + 15);
            }
        }
    }
    s += &row(101, ["Trial", "Group", "Group"]);
    s += &row(116, ["Name", "Start", "End"]);
    for i in 0..9 {
        s += &row(151 + 14 * i, ["Gauge", "23", "48"]);
    }
    s += &row(296, ["Outside", "77", "99"]);
    s + "</page>"
}

#[test]
fn recover_numeric_body_only_with_continuing_dividers() {
    for format in [TableFormat::Gfm, TableFormat::Html] {
        let opt = Options { tables: format, ..Options::default() };
        for interrupt in [true, false] {
            let md = to_markdown_report_with(&page(interrupt), &Params::default(), &opt).0.join("");
            let in_table = if format == TableFormat::Html {
                "<td>Gauge</td><td>23</td><td>48</td>"
            } else {
                "| Gauge | 23 | 48 |"
            };
            let header_and_body = if format == TableFormat::Html {
                "<td>End</td></tr>\n<tr><td>Gauge</td>"
            } else {
                "| End |\n| Gauge |"
            };
            assert_eq!(md.contains(header_and_body), !interrupt, "{md}");
            if !interrupt {
                assert_eq!(md.matches(in_table).count(), 9, "{md}");
                assert!(!md.contains("<td>Outside</td>") && !md.contains("| Outside |"), "{md}");
            }
        }
    }
}

#[test]
fn incomplete_evidence_and_prose_do_not_extend_the_header() {
    let original = page(false);
    let prose = original.replace(">23</word>", ">up</word>").replace(">48</word>", ">ok</word>");
    let truncated = original.replacen("<page ", "<page rulesTruncated=\"1\" ", 1);
    let crossed = original.replace("xMin=\"180\" xMax=\"190\"", "xMin=\"150\" xMax=\"190\"");
    assert_ne!(crossed, original);
    let html = Options { tables: TableFormat::Html, ..Options::default() };
    for sample in [&prose, &truncated, &crossed] {
        let md = to_markdown_report_with(sample, &Params::default(), &html).0.join("");
        assert!(!md.contains("<td>End</td></tr>\n<tr><td>Gauge</td>"), "{md}");
        assert_eq!(md.matches("Gauge").count(), 9, "{md}");
    }
}
