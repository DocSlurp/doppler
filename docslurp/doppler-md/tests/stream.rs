//! Streaming: `model::Parser` and `decode_lossy` over input split anywhere, and `Converter`'s
//! lookahead window.

use std::path::Path;
use std::time::{Duration, Instant};

use doppler_md::model::{Parser, decode_lossy};
use doppler_md::{BodySample, Converter, Options, Params, Window, to_markdown_report_with};

/// Converts `bytes` fed in pieces of `chunk` bytes (split anywhere, even inside a UTF-8
/// sequence) with a `window`: (pages, report JSON).
fn streamed(bytes: &[u8], chunk: usize, window: Option<usize>) -> (Vec<String>, String) {
    let p = Params::default();
    let mut conv = Converter::new(&p, Options::default(), window);
    let mut parser = Parser::new();
    let (mut partial, mut text) = (Vec::new(), String::new());
    let mut out = Vec::new();
    for piece in bytes.chunks(chunk.max(1)) {
        decode_lossy(&mut partial, piece, false, &mut text);
        parser.push(&text, &mut |pg| conv.push(pg, &mut |_, md| out.push(md)));
        text.clear();
    }
    decode_lossy(&mut partial, &[], true, &mut text);
    parser.push(&text, &mut |pg| conv.push(pg, &mut |_, md| out.push(md)));
    let parse = parser.finish(&mut |pg| conv.push(pg, &mut |_, md| out.push(md)));
    let report = conv.finish(parse, &mut |_, md| out.push(md));
    (out, report.to_json())
}

fn whole(bytes: &[u8]) -> (Vec<String>, String) {
    let (md, report) =
        to_markdown_report_with(&String::from_utf8_lossy(bytes), &Params::default(), &Options::default());
    (md, report.to_json())
}

fn fixtures() -> Vec<(String, Vec<u8>)> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let mut out: Vec<(String, Vec<u8>)> = std::fs::read_dir(dir)
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "xhtml"))
        .map(|p| (p.display().to_string(), std::fs::read(&p).unwrap()))
        .collect();
    out.sort();
    out
}

/// Inputs where a split could change what the parser decides: unclosed and truncated tags, a
/// `</page>` inside word text, a newline inside a word, invalid and split UTF-8, lines that
/// stay open across a `<page>`, and content after `</page>`.
fn hostile() -> Vec<Vec<u8>> {
    let w = |x: f64, t: &str| {
        format!(
            "<word xMin=\"{x}\" yMin=\"90\" xMax=\"{}\" yMax=\"100\" fontName=\"F\" fontSize=\"10\" baseline=\"98\">{t}</word>",
            x + 30.0
        )
    };
    let mut v: Vec<Vec<u8>> = vec![
        format!("<doc><page width=\"612\" height=\"792\"><line>{}</line></page><page><line>{}", w(72.0, "a"), w(72.0, "b")),
        format!("<page><line>{}</line></page><line>{}</line><rule xMin=\"1\" yMin=\"2\" xMax=\"3\" yMax=\"4\"/>", w(72.0, "x"), w(80.0, "after")),
        format!("<page><line><word xMin=\"1\" yMin=\"2\" xMax=\"3\" yMax=\"4\">a</page>b</word></line></page><page><line>{}</line>", w(9.0, "z")),
        format!("<page><line><word xMin=\"1\" yMin=\"2\" xMax=\"3\" yMax=\"4\">line\nbreak</word>{}</line></page>", w(5.0, "ok")),
        format!("<page><line>{}<page><line>{}</line></page>", w(1.0, "open"), w(2.0, "next")),
        format!("<page><line>{}</line><wor<pag<word <line {}</line><rule xMin=\"1\"/><box xMin=\"1\" yMin=\"1\" xMax=\"2\" yMax=\"2\"/>", w(3.0, "é漢字"), w(4.0, "y")),
        format!("<page><line>{}</line></page><page><line><word xMin=\"1\" yMin=\"2\"", w(5.0, "tail")),
        "<page><line><word xMin=\"1\" yMin=\"2\" xMax=\"3\" yMax=\"4\">".to_string() + &"<".repeat(300),
    ]
    .into_iter()
    .map(String::into_bytes)
    .collect();
    let mut bad = format!("<page><line>{}</line></page>", w(1.0, "ok")).into_bytes();
    // invalid UTF-8 inside a word, and a truncated sequence at the end of the input
    let at = bad.windows(2).position(|x| x == b"ok").unwrap();
    bad.splice(at..at, [0xff, 0xe6, 0xbc, b'q', 0xf0, 0x9f]);
    bad.extend_from_slice(&[0xe6, 0xbc]);
    v.push(bad);
    v
}

#[test]
fn any_split_converts_like_the_whole_input() {
    let mut inputs: Vec<(String, Vec<u8>)> = fixtures();
    inputs.extend(hostile().into_iter().enumerate().map(|(i, b)| (format!("hostile {i}"), b)));
    for (name, bytes) in &inputs {
        let want = whole(bytes);
        let small = bytes.len() <= 4096;
        for chunk in [1, 2, 3, 5, 7, 16, 17, 64, 1000, 65536] {
            if chunk < 16 && !small {
                continue;
            }
            assert_eq!(streamed(bytes, chunk, None), want, "{name}: chunk {chunk}");
        }
    }
}

#[test]
fn decode_lossy_matches_from_utf8_lossy_at_every_split() {
    let bytes: Vec<u8> = [b"a\xe6\xbc\xa2b\xff\xf0\x9f\x98\x80\xf0\x9f\xe2\x82\xac\xed\xa0\x80z\xc3" as &[u8]].concat();
    let want = String::from_utf8_lossy(&bytes).into_owned();
    for i in 0..=bytes.len() {
        for j in i..=bytes.len() {
            let (mut partial, mut out) = (Vec::new(), String::new());
            for piece in [&bytes[..i], &bytes[i..j], &bytes[j..]] {
                decode_lossy(&mut partial, piece, false, &mut out);
            }
            decode_lossy(&mut partial, &[], true, &mut out);
            assert_eq!(out, want, "split at {i}, {j}");
        }
    }
}

/// `n` pages, each with a running header, a body line and a footer page number.
fn pages(n: usize, header: impl Fn(usize) -> String) -> Vec<u8> {
    let line = |y: f64, t: &str| {
        format!(
            "<line><word xMin=\"72\" yMin=\"{}\" xMax=\"300\" yMax=\"{y}\" fontName=\"F\" fontSize=\"10\" baseline=\"{}\">{t}</word></line>\n",
            y - 10.0,
            y - 2.0
        )
    };
    let mut s = String::from("<doc>\n");
    for i in 0..n {
        s.push_str("<page width=\"612\" height=\"792\">\n<flow>\n<block>\n");
        s.push_str(&line(40.0, &header(i)));
        s.push_str(&line(300.0, &format!("Body text of page {i} with several words in it.")));
        s.push_str(&line(770.0, &format!("{}", i + 1)));
        s.push_str("</block>\n</flow>\n</page>\n");
    }
    s.push_str("</doc>\n");
    s.into_bytes()
}

#[test]
fn a_window_covering_the_document_is_exact() {
    for n in [1, 2, 5, 17] {
        let bytes = pages(n, |i| if i % 3 == 0 { "Annual Report".into() } else { format!("Chapter {i}") });
        let want = whole(&bytes);
        // n - 1 pages either side is the smallest window that covers every page
        for w in [n.saturating_sub(1), 1000] {
            assert_eq!(streamed(&bytes, 4096, Some(w)), want, "{n} pages, window {w}");
        }
    }
}

#[test]
fn window_holds_exactly_window_pages_after_each_push() {
    let p = Params::default();
    let w = 3;
    let mut conv = Converter::new(&p, Options::default(), Some(w));
    let mut parser = Parser::new();
    let text = String::from_utf8(pages(20, |_| "Header".into())).unwrap();
    let (mut parsed, mut written) = (0usize, Vec::new());
    for piece in text.as_bytes().chunks(50) {
        parser.push(std::str::from_utf8(piece).unwrap(), &mut |pg| {
            parsed += 1;
            conv.push(pg, &mut |i, _| written.push(i));
            // fewer would mean writing before the window is complete, more holding pages longer
            assert_eq!(parsed - written.len(), parsed.min(w), "{parsed} parsed, {} written", written.len());
        });
    }
    let parse = parser.finish(&mut |pg| conv.push(pg, &mut |i, _| written.push(i)));
    conv.finish(parse, &mut |i, _| written.push(i));
    assert_eq!(written, (0..20).collect::<Vec<_>>());
}

#[test]
fn running_headers_are_judged_within_the_window() {
    // a header on every page drops however small the window; one that changes every 10 pages
    // drops within its chapter under a window, while the whole document sees each on only
    // 10 of 40 pages (under the 40% share) and keeps them all
    let bytes = pages(40, |i| format!("Part {}", ["one", "two", "three", "four"][i / 10]));
    let header_kept = |md: &[String]| md.iter().filter(|m| m.contains("Part ")).count();
    assert_eq!(header_kept(&whole(&bytes).0), 40);
    assert_eq!(header_kept(&streamed(&bytes, 4096, Some(4)).0), 0);
    let same = pages(40, |_| "Same header".into());
    assert!(streamed(&same, 4096, Some(1)).0.iter().all(|m| !m.contains("Same header")));
}

#[test]
fn unclosed_tag_streamed_in_tiny_pieces_is_linear() {
    // a <word> that never closes keeps the parser waiting: rescans must stay amortized linear
    let mut s = String::from("<page><line><word xMin=\"1\" yMin=\"2\" xMax=\"3\" yMax=\"4\">");
    s.push_str(&"x".repeat(2_000_000));
    let t = Instant::now();
    let (md, _) = streamed(s.as_bytes(), 1, Some(4));
    assert!(t.elapsed() < Duration::from_secs(10), "{:?}", t.elapsed());
    assert_eq!(md.len(), 1);
}

#[test]
fn rejected_tags_streamed_stay_linear() {
    // runs of `<` (or `<word ` lines) that tag_at rejects must not each rescan to the next
    // `>` or `</word>`; the filler makes the buffer grow so that non-final runs happen
    let mut lts = String::from("<page>");
    lts.push_str(&"<".repeat(500_000));
    lts.push('>');
    lts.push_str(&"a".repeat(1_200_000));
    let mut words = String::from("<page>");
    words.push_str(&"<word x>\n".repeat(200_000));
    words.push_str("</word>");
    for s in [lts, words] {
        let t = Instant::now();
        assert_eq!(streamed(s.as_bytes(), 65536, Some(4)), whole(s.as_bytes()));
        assert!(t.elapsed() < Duration::from_secs(1), "{:?}", t.elapsed());
    }
}

#[test]
fn an_unclosed_word_does_not_hold_back_later_pages() {
    // no `</word>` ever follows (the later pages hold only rules), but the newline after the
    // tag already decides it
    let mut s = String::from("<page><line><word xMin=\"1\" yMin=\"2\" xMax=\"3\" yMax=\"4\">a\n");
    for _ in 0..30 {
        s.push_str("<page width=\"612\" height=\"792\"><rule xMin=\"1\" yMin=\"2\" xMax=\"300\" yMax=\"3\"/></page>\n");
    }
    let p = Params::default();
    let mut conv = Converter::new(&p, Options::default(), Some(2));
    let mut parser = Parser::new();
    let mut written = 0;
    for piece in s.as_bytes().chunks(64) {
        parser.push(std::str::from_utf8(piece).unwrap(), &mut |pg| conv.push(pg, &mut |_, _| written += 1));
    }
    assert!(written >= 25, "only {written} pages written before the end of the input");
}

/// Converts whole pages (each a `<page>` element) with `win`: (pages, report).
fn converted(pages: &[String], win: Window, o: Options, p: &Params) -> (Vec<String>, doppler_md::Report) {
    let mut conv = Converter::with(p, o, win);
    let mut parser = Parser::new();
    let mut out = Vec::new();
    let text = format!("<doc>\n{}</doc>\n", pages.concat());
    parser.push(&text, &mut |pg| conv.push(pg, &mut |i, md| out.push((i, md))));
    let parse = parser.finish(&mut |pg| conv.push(pg, &mut |i, md| out.push((i, md))));
    let report = conv.finish(parse, &mut |i, md| out.push((i, md)));
    // pages arrive in order and are numbered from 0
    assert!(out.iter().enumerate().all(|(k, (i, _))| k == *i), "page indices out of order");
    (out.into_iter().map(|(_, md)| md).collect(), report)
}

/// A small deterministic generator (xorshift).
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

/// A generated document of `<page>` elements: a few cover pages in large print, then chapters
/// with their own running header, a document-wide footer, page numbers, headings in larger
/// print and some blank pages.
fn book(rng: &mut Rng) -> Vec<String> {
    let n = 20 + rng.below(40) as usize;
    let covers = rng.below(4) as usize;
    let chapter_len = 3 + rng.below(9) as usize;
    let body = [9.0, 10.0, 11.0][rng.below(3) as usize];
    let line = |y: f64, size: f64, t: &str| {
        format!(
            "<line><word xMin=\"72\" yMin=\"{}\" xMax=\"{}\" yMax=\"{y}\" fontName=\"F\" fontSize=\"{size}\" baseline=\"{}\">{t}</word></line>\n",
            y - size,
            72.0 + 0.5 * size * t.chars().count() as f64,
            y - 2.0
        )
    };
    (0..n)
        .map(|i| {
            let mut s = String::from("<page width=\"612\" height=\"792\">\n<flow>\n<block>\n");
            if i < covers {
                s += &line(300.0, 24.0, &format!("Cover title page {i} in large print"));
                s += &line(340.0, 18.0, "A generated volume of several chapters");
            } else if rng.below(15) != 0 {
                let ch = (i - covers) / chapter_len;
                s += &line(40.0, body, &format!("Chapter {ch} running head"));
                if (i - covers) % chapter_len == 0 {
                    s += &line(120.0, body * 1.6, &format!("Heading of chapter {ch}"));
                }
                for k in 0..(4 + rng.below(8)) {
                    s += &line(
                        160.0 + 24.0 * k as f64,
                        body,
                        &format!("Body line {k} of page {i}, words enough to be prose."),
                    );
                }
                s += &line(760.0, body * 0.8, "Generated Press Confidential");
                s += &line(775.0, body * 0.8, &format!("{}", i + 1));
            }
            s += "</block>\n</flow>\n</page>\n";
            s
        })
        .collect()
}

#[test]
fn a_range_with_context_and_the_body_given_matches_a_whole_run() {
    let p = Params::default();
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    let mut checked = 0;
    for _ in 0..12 {
        let doc = book(&mut rng);
        let n = doc.len();
        for w in [2usize, 4, 8] {
            for body in [None, Some(BodySample::TextPages(6)), Some(BodySample::TextPages(32))] {
                let whole_win = Window { furniture: Some(w), body, ..Window::default() };
                let (want, report) = converted(&doc, whole_win.clone(), Options::default(), &p);
                assert_eq!(want.len(), n);
                for _ in 0..4 {
                    let f = 1 + rng.below(n as u64) as usize;
                    let l = f + rng.below((n - f + 1) as u64) as usize;
                    let ctx = w + rng.below(3) as usize;
                    let read = f.saturating_sub(ctx).max(1);
                    let end = (l + ctx).min(n);
                    let win = Window {
                        body_size: Some(report.body.size),
                        body_color: Some(report.body.color.clone()),
                        skip: f - read,
                        emit: Some(l - f + 1),
                        after: ctx,
                        first_page: read as u32,
                        ..whole_win.clone()
                    };
                    let (got, r) = converted(&doc[read - 1..end], win, Options::default(), &p);
                    assert_eq!(
                        got,
                        want[f - 1..l],
                        "{n} pages, window {w}, body {body:?}, pages {f}-{l}, context {ctx}"
                    );
                    let range = r.range.expect("a range report");
                    let expect: Vec<&str> = if f > 1 { vec!["doc_work"] } else { vec![] };
                    assert_eq!(range.identity, expect, "pages {f}-{l} of {n}");
                    assert_eq!(r.parse.pages, l - f + 1);
                    assert!(r.body.size_given && r.body.color_given);
                    checked += 1;
                }
            }
        }
    }
    assert_eq!(checked, 12 * 3 * 3 * 4);
}

#[test]
fn the_range_report_names_what_can_differ() {
    let p = Params::default();
    let mut rng = Rng(7);
    let doc = book(&mut rng);
    let n = doc.len();
    let w = 4;
    let range = |read: usize, f: usize, l: usize, ctx: usize, given: bool, o: Options, p: &Params| {
        let win = Window {
            furniture: Some(w),
            body_size: given.then_some(10.0),
            body_color: given.then(String::new),
            skip: f - read,
            emit: Some(l - f + 1),
            after: ctx,
            first_page: read as u32,
            ..Window::default()
        };
        converted(&doc[read - 1..(l + ctx).min(n)], win, o, p).1.range.unwrap().identity
    };
    // enough context, the body given: only the document's work budget is unseen
    assert_eq!(range(5, 10, 14, 5, true, Options::default(), &p), ["doc_work"]);
    // the body estimated from the range's own pages
    assert_eq!(range(5, 10, 14, 5, false, Options::default(), &p), ["body", "doc_work"]);
    // context shorter than the window on either side
    assert_eq!(range(8, 10, 14, 2, true, Options::default(), &p), ["context", "doc_work"]);
    // at the start of the document no context is missing and nothing comes before
    assert_eq!(range(1, 1, 5, 4, true, Options::default(), &p), Vec::<&str>::new());
    // context pages are not laid out, so their work is not charged either
    assert_eq!(range(1, 2, 5, 4, true, Options::default(), &p), ["doc_work"]);
    // no furniture window: repetition is judged over the whole document
    let win = Window { skip: 2, emit: Some(3), first_page: 4, ..Window::default() };
    let r = converted(&doc[3..10], win, Options::default(), &p).1.range.unwrap();
    assert_eq!(r.identity, ["body", "furniture", "doc_work"]);
    // continuation markers: a range that starts mid-document without context cannot know
    // what the previous page left open
    let markers = Options { markers: true, ..Options::default() };
    assert_eq!(range(10, 10, 14, 0, true, markers, &p), ["context", "markers", "doc_work"]);
    // with context the leading pages are laid out for it (these pages end in no table)
    assert_eq!(range(5, 10, 14, 5, true, markers, &p), ["doc_work"]);
}

#[test]
fn the_document_work_budget_is_where_a_range_can_still_differ() {
    // a budget that runs out mid-document degrades later pages in a whole run, but a range
    // starting there has spent nothing: the report says so (doc_work), and the range's own
    // doc_work tells a host whether the sum over its ranges stays within the limit
    let mut rng = Rng(11);
    let doc = book(&mut rng);
    let n = doc.len();
    let w = 4;
    let whole_win = Window { furniture: Some(w), ..Window::default() };
    let full = converted(&doc, whole_win.clone(), Options::default(), &Params::default()).1;
    let mut p = Params::default();
    p.max_doc_work = full.doc_work as f64 / 2.0;
    let (want, whole) = converted(&doc, whole_win.clone(), Options::default(), &p);
    assert!(whole.degraded.iter().any(|(_, d)| *d == doppler_md::Degraded::DocWork));
    let (f, l) = (n - 6, n - 1);
    let win = Window {
        body_size: Some(whole.body.size),
        body_color: Some(whole.body.color.clone()),
        skip: w,
        emit: Some(l - f + 1),
        after: w,
        first_page: (f - w) as u32,
        ..whole_win
    };
    let (got, r) = converted(&doc[f - w - 1..(l + w).min(n)], win, Options::default(), &p);
    assert_ne!(got, want[f - 1..l]);
    assert!(r.degraded.is_empty());
    let range = r.range.unwrap();
    assert_eq!(range.identity, ["doc_work"]);
    assert_eq!(range.doc_work_limit, p.max_doc_work);
}

#[test]
fn the_body_sample_holds_pages_until_it_is_complete() {
    // --body-pages N: nothing is written before the N-th page with text; blank pages do not
    // count, and the furniture window bounds what is held after that
    let mut doc: Vec<String> = Vec::new();
    let mut rng = Rng(3);
    let text = book(&mut rng).into_iter().filter(|pg| pg.contains("Body line")).take(12).collect::<Vec<_>>();
    for (i, pg) in text.into_iter().enumerate() {
        if i % 2 == 0 {
            doc.push("<page width=\"612\" height=\"792\"></page>\n".to_string());
        }
        doc.push(pg);
    }
    let p = Params::default();
    let win = Window { furniture: Some(1), body: Some(BodySample::TextPages(5)), ..Window::default() };
    let mut conv = Converter::with(&p, Options::default(), win);
    let mut parser = Parser::new();
    let (mut parsed, mut written) = (0usize, 0usize);
    for pg in &doc {
        parser.push(pg, &mut |page| {
            parsed += 1;
            conv.push(page, &mut |_, _| written += 1);
            // held until the sample is complete, then down to the window
            if parsed < 8 {
                assert_eq!(written, 0, "{parsed} parsed");
            } else {
                assert_eq!(parsed - written, 1, "{parsed} parsed");
            }
        });
    }
    let parse = parser.finish(&mut |page| conv.push(page, &mut |_, _| written += 1));
    let report = conv.finish(parse, &mut |_, _| written += 1);
    assert_eq!(written, doc.len());
    // the 5th page with text is the 8th page read (a blank page before every other one)
    assert_eq!(report.body.sample_pages, 8);
    assert_eq!(report.body.sample_text_pages, 5);
    let json = report.to_json();
    assert!(json.contains("\"body_size\":") && json.contains("\"body_sample_pages\":8"), "{json}");
}
