//! `--heading-levels rank`: heading levels by the rank of their size in the document, seeded
//! from the first pages and only added to, so a stream with a body sample taken from front matter
//! levels its headings as the whole run does.

use doppler_md::heading_levels::HeadingTable;
use doppler_md::model::{Parser, parse};
use doppler_md::{BodySample, Converter, HeadingLevels, Options, Params, Window};

fn line(y: f64, size: f64, text: &str) -> String {
    let mut x = 72.0;
    let mut words = String::new();
    for w in text.split(' ') {
        let x1 = x + 0.5 * size * w.chars().count() as f64;
        words += &format!(
            r#"<word xMin="{x}" yMin="{}" xMax="{x1}" yMax="{y}" fontName="Serif" fontSize="{size}" baseline="{}">{w}</word>"#,
            y - size,
            y - 0.2 * size
        );
        x = x1 + 0.3 * size;
    }
    format!("<flow><block><line>{words}</line></block></flow>\n")
}

fn page(lines: &[String]) -> String {
    format!("<page width=\"612\" height=\"792\">\n{}</page>\n", lines.concat())
}

const PROSE: &str = "Body text that runs on long enough to be a paragraph and never a heading,";

/// Front matter in 12-point print (long paragraphs), then chapters in 10-point print with
/// 20-point chapter and 14-point section headings: a body sample of the first pages takes 12
/// points for the body, the whole document 10.
fn book() -> Vec<String> {
    let mut pages = Vec::new();
    for i in 0..3 {
        let mut ls = Vec::new();
        for k in 0..12 {
            ls.push(line(120.0 + 30.0 * k as f64, 12.0, &format!("Front {i}.{k} {PROSE} and on to the end")));
        }
        pages.push(page(&ls));
    }
    for ch in 1..=8 {
        for s in 0..3 {
            let mut ls = Vec::new();
            let mut y = 120.0;
            if s == 0 {
                ls.push(line(y, 20.0, &format!("Chapter {ch} Opening")));
                y += 40.0;
            }
            ls.push(line(y, 14.0, &format!("Section {ch}.{s} Details")));
            y += 28.0;
            for k in 0..20 {
                ls.push(line(y + 13.0 * k as f64, 10.0, &format!("Line {k} of section {ch}.{s}: {PROSE}")));
            }
            pages.push(page(&ls));
        }
    }
    pages
}

fn converted(pages: &[String], win: Window) -> (Vec<String>, doppler_md::Report) {
    let p = Params::default();
    let mut conv = Converter::with(&p, Options::default(), win);
    let mut parser = Parser::new();
    let mut out = Vec::new();
    let text = format!("<doc>\n{}</doc>\n", pages.concat());
    parser.push(&text, &mut |pg| conv.push(pg, &mut |_, md| out.push(md)));
    let parse = parser.finish(&mut |pg| conv.push(pg, &mut |_, md| out.push(md)));
    let report = conv.finish(parse, &mut |_, md| out.push(md));
    (out, report)
}

/// The headings of each page: (level, text).
fn headings(pages: &[String]) -> Vec<Vec<(usize, String)>> {
    pages
        .iter()
        .map(|md| {
            md.lines()
                .filter(|l| l.starts_with('#'))
                .map(|l| {
                    let level = l.chars().take_while(|&c| c == '#').count();
                    (level, l[level..].trim().to_string())
                })
                .collect()
        })
        .collect()
}

#[test]
fn a_stream_with_a_front_matter_body_levels_headings_as_the_whole_run() {
    let doc = book();
    for (mode, same) in [(HeadingLevels::Size, false), (HeadingLevels::Rank, true)] {
        let whole = Window { headings: mode, ..Window::default() };
        let stream =
            Window { furniture: Some(4), body: Some(BodySample::TextPages(2)), headings: mode, ..Window::default() };
        let (want, wr) = converted(&doc, whole);
        let (got, sr) = converted(&doc, stream);
        assert_eq!((wr.body.size, sr.body.size), (10.0, 12.0));
        let (want, got) = (headings(&want), headings(&got));
        // the same paragraphs are headings either way (14 points is a heading under both)
        let texts = |h: &Vec<Vec<(usize, String)>>| -> Vec<Vec<String>> {
            h.iter().map(|p| p.iter().map(|(_, t)| t.clone()).collect()).collect()
        };
        assert_eq!(texts(&want), texts(&got));
        assert_eq!(want == got, same, "{mode:?}: {:?} / {:?}", want[3], got[3]);
    }
    // ranked: chapters #, sections ##
    let (md, report) = converted(&doc, Window { headings: HeadingLevels::Rank, ..Window::default() });
    assert_eq!(headings(&md)[3], [(1, "Chapter 1 Opening".to_string()), (2, "Section 1.0 Details".to_string())]);
    assert_eq!(report.heading_sizes, Some((2, 2)));
    assert!(report.to_json().contains("\"heading_sizes\":{\"seeded\":2,\"total\":2}"));
}

#[test]
fn nothing_is_written_before_the_heading_seed() {
    // the seed is the first 16 pages with text, whatever the body sample
    let doc = book();
    let p = Params::default();
    let win = Window {
        furniture: Some(1),
        body: Some(BodySample::TextPages(1)),
        headings: HeadingLevels::Rank,
        ..Window::default()
    };
    let mut conv = Converter::with(&p, Options::default(), win);
    let mut parser = Parser::new();
    let (mut parsed, mut written) = (0usize, 0usize);
    for pg in &doc {
        parser.push(pg, &mut |page| {
            parsed += 1;
            conv.push(page, &mut |_, _| written += 1);
            if parsed < 16 {
                assert_eq!(written, 0, "{parsed} parsed");
            } else {
                assert_eq!(parsed - written, 1, "{parsed} parsed");
            }
        });
    }
}

fn seeded(sizes: &[f64]) -> HeadingTable {
    let lines: Vec<String> =
        sizes.iter().enumerate().map(|(i, &s)| line(120.0 + 40.0 * i as f64, s, "Some Heading")).collect();
    let mut body: Vec<String> = lines;
    for k in 0..30 {
        body.push(line(400.0 + 12.0 * k as f64, 10.0, PROSE));
    }
    let pages = parse(&format!("<doc>{}</doc>", page(&body)));
    HeadingTable::seed(pages.iter(), &Params::default())
}

#[test]
fn ranks_are_by_size_and_new_sizes_never_renumber() {
    let p = Params::default();
    // 24 and 23 are one size (within style_size); 16 is a level below; 12 and 11.5 too close to
    // be a level apart (h2_ratio / heading_ratio)
    let mut t = seeded(&[24.0, 23.0, 16.0, 12.0, 11.5]);
    assert_eq!(t.seeded, 3);
    assert_eq!([t.level(24.0, &p), t.level(16.0, &p), t.level(12.0, &p), t.level(11.6, &p)], [1, 2, 3, 3]);
    // body-size lines are not ranked: a heading of a size not seen is added where it falls
    assert_eq!(t.level(30.0, &p), 1); // above every size
    assert_eq!(t.level(19.0, &p), 2); // between 24 (#) and 16 (##), a level apart from both
    assert_eq!(t.level(15.5, &p), 2); // within the tolerance of 16
    assert_eq!(t.level(13.8, &p), 3); // between 16 (##) and 12 (###)
    assert_eq!(t.level(9.0, &p), 3); // below every size
    // what was given stays
    assert_eq!([t.level(24.0, &p), t.level(16.0, &p), t.level(12.0, &p)], [1, 2, 3]);
    assert_eq!(t.len(), 7);
    // deterministic: the same sizes in the same order give the same table and levels
    let mut u = seeded(&[24.0, 23.0, 16.0, 12.0, 11.5]);
    let order = [30.0, 19.0, 15.5, 13.8, 9.0];
    let a: Vec<u8> = order.iter().map(|&s| u.level(s, &p)).collect();
    assert_eq!(a, [1, 2, 2, 3, 3]);
    assert_eq!(t, u);
    // non-finite sizes are the deepest level and are not added
    assert_eq!(u.level(f64::NAN, &p), 3);
    assert_eq!(u.len(), 7);
    // an empty seed: the first heading is #, the next smaller one a level below
    let mut e = seeded(&[]);
    assert!(e.is_empty());
    assert_eq!([e.level(14.0, &p), e.level(12.0, &p), e.level(20.0, &p)], [1, 2, 1]);
}

#[test]
fn a_range_after_the_first_page_names_the_heading_seed() {
    let doc = book();
    let win = Window {
        furniture: Some(4),
        body_size: Some(10.0),
        body_color: Some(String::new()),
        skip: 4,
        emit: Some(4),
        after: 4,
        first_page: 6,
        headings: HeadingLevels::Rank,
        ..Window::default()
    };
    let r = converted(&doc[5..17], win.clone()).1;
    assert_eq!(r.range.unwrap().identity, ["headings", "doc_work"]);
    let size = Window { headings: HeadingLevels::Size, ..win };
    assert_eq!(converted(&doc[5..17], size).1.range.unwrap().identity, ["doc_work"]);
}
