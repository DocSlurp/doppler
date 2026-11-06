//! The math reader (`Options::math`) on synthetic `pdftotext -glyphs` XHTML: structure,
//! symbol tables, ActualText LaTeX, the report, default-mode compatibility, and hostile input
//! (malformed records, caps, bounded time, seeded fuzz). All content here is made up for the
//! tests.

use std::fmt::Write as _;
use std::time::{Duration, Instant};

use doppler_md::{Options, Params, TableFormat, to_markdown_report_with};

const MATH: Options = Options { tables: TableFormat::Gfm, math: true };
const PLAIN: Options = Options { tables: TableFormat::Gfm, math: false };

/// One glyph: x, advance, font id, text, glyph name, optional baseline and size overrides.
#[derive(Clone)]
struct G {
    x: f64,
    w: f64,
    f: usize,
    t: &'static str,
    n: Option<&'static str>,
    src: char,
    b: Option<f64>,
}

fn g(x: f64, w: f64, f: usize, t: &'static str) -> G {
    G { x, w, f, t, n: None, src: 't', b: None }
}

/// A word: its glyphs, baseline and size.
struct W {
    gs: Vec<G>,
    base: f64,
    size: f64,
}

fn w(gs: Vec<G>, base: f64, size: f64) -> W {
    W { gs, base, size }
}

const FONTS: [&str; 6] = ["CMR10", "CMMI10", "CMSY10", "CMEX10", "CMR7", "CMMI7"];

/// XHTML of one page with these lines (each a list of words) and rules (x0, y0, x1, y1).
fn page(lines: &[Vec<W>], rules: &[(f64, f64, f64, f64)]) -> String {
    let mut x = String::from("<doc>\n  <page width=\"612\" height=\"792\">\n");
    for &(x0, y0, x1, y1) in rules {
        writeln!(x, "    <rule xMin=\"{x0}\" yMin=\"{y0}\" xMax=\"{x1}\" yMax=\"{y1}\" color=\"#000000\"/>").unwrap();
    }
    for (i, f) in FONTS.iter().enumerate() {
        writeln!(x, "    <font id=\"{i}\" name=\"{f}\" type=\"Type1\" enc=\"Custom\" emb=\"1\" sym=\"1\" cid=\"0\" tu=\"1\" tuMiss=\"0\"/>").unwrap();
    }
    x.push_str("    <flow>\n      <block>\n");
    for line in lines {
        x.push_str("        <line>\n");
        for word in line {
            let x0 = word.gs.first().map_or(0.0, |g| g.x);
            let x1 = word.gs.last().map_or(0.0, |g| g.x + g.w);
            let text: String =
                word.gs.iter().map(|g| g.t).collect::<String>().replace('&', "&amp;").replace('<', "&lt;");
            let font = FONTS[word.gs.first().map_or(0, |g| g.f)];
            writeln!(
                x,
                "          <word xMin=\"{x0}\" yMin=\"{}\" xMax=\"{x1}\" yMax=\"{}\" fontName=\"AAAAAA+{font}\" fontSize=\"{}\" rotation=\"0\" spaceAfter=\"1\" invisible=\"0\" color=\"#000000\" baseline=\"{}\">{text}</word>",
                word.base - 0.75 * word.size,
                word.base + 0.25 * word.size,
                word.size,
                word.base
            )
            .unwrap();
            for gl in &word.gs {
                let u: Vec<String> = gl.t.chars().map(|c| format!("{:x}", c as u32)).collect();
                write!(
                    x,
                    "<g x=\"{}\" w=\"{}\" f=\"{}\" c=\"1\" u=\"{}\" s=\"{}\"",
                    gl.x,
                    gl.w,
                    gl.f,
                    u.join(" "),
                    gl.src
                )
                .unwrap();
                if let Some(b) = gl.b {
                    write!(x, " b=\"{b}\"").unwrap();
                }
                if let Some(n) = gl.n {
                    write!(x, " n=\"{n}\"").unwrap();
                }
                x.push_str("/>");
            }
            x.push('\n');
        }
        x.push_str("        </line>\n");
    }
    x.push_str("      </block>\n    </flow>\n  </page>\n</doc>\n");
    x
}

fn prose(y: f64) -> Vec<W> {
    vec![
        w(vec![g(72.0, 6.0, 0, "S"), g(78.0, 5.0, 0, "o"), g(83.0, 5.0, 0, "m"), g(88.0, 5.0, 0, "e")], y, 10.0),
        w(
            vec![
                g(96.0, 5.0, 0, "t"),
                g(101.0, 5.0, 0, "e"),
                g(106.0, 5.0, 0, "x"),
                g(111.0, 5.0, 0, "t"),
                g(116.0, 3.0, 0, "."),
            ],
            y,
            10.0,
        ),
    ]
}

fn convert(x: &str, o: &Options) -> (String, doppler_md::Report) {
    let (pages, report) = to_markdown_report_with(x, &Params::default(), o);
    (pages.join("\x0c"), report)
}

#[test]
fn inline_scripts() {
    // "Let x_i^2 be" on one prose line: a CMMI x with a CMR7 superscript and a CMMI7 subscript
    let line = vec![
        w(vec![g(72.0, 5.0, 0, "L"), g(77.0, 5.0, 0, "e"), g(82.0, 4.0, 0, "t")], 100.0, 10.0),
        w(vec![g(90.0, 5.7, 1, "x")], 100.0, 10.0),
        w(vec![g(95.7, 4.0, 4, "2")], 96.0, 7.0),
        w(vec![g(95.7, 3.0, 5, "i")], 102.0, 7.0),
        w(vec![g(104.0, 5.0, 0, "b"), g(109.0, 5.0, 0, "e")], 100.0, 10.0),
    ];
    let (md, report) = convert(&page(&[prose(80.0), line, prose(120.0)], &[]), &MATH);
    assert!(md.contains("$x_{i}^{2}$"), "{md}");
    assert!(md.contains("Let $x_{i}^{2}$ be"), "{md}");
    assert_eq!(report.math.iter().map(|m| m.inline).sum::<usize>(), 1);
}

#[test]
fn display_fraction_and_sum() {
    // a centred display: sum (CMEX, raw code text, named) with limits, then a/b over a rule
    let main = vec![
        w(vec![G { x: 280.0, w: 10.0, f: 3, t: "X", n: Some("summationdisplay"), src: 'c', b: None }], 190.0, 10.0),
        w(vec![g(281.0, 3.0, 5, "i"), g(284.0, 4.0, 4, "=")], 214.0, 7.0),
        w(vec![g(283.0, 4.0, 5, "n")], 188.0, 7.0),
        w(vec![g(296.0, 6.0, 1, "x")], 200.0, 10.0),
    ];
    let num = vec![w(vec![g(310.0, 5.0, 1, "a")], 193.0, 10.0)];
    let den = vec![w(vec![g(310.0, 5.0, 1, "b")], 207.0, 10.0)];
    let x = page(&[prose(150.0), main, num, den, prose(250.0)], &[(308.0, 197.3, 317.0, 197.7)]);
    let (md, report) = convert(&x, &MATH);
    assert!(md.contains("$$\\sum_{i=}^{n}x\\frac{a}{b}$$"), "{md}");
    assert_eq!(report.math.iter().map(|m| m.display).sum::<usize>(), 1);
}

#[test]
fn matrix_in_parentheses() {
    // ( a b ; c d ) with big parentheses as CMEX size variants
    let big = |x: f64, t: &'static str, n: &'static str| G { x, w: 7.0, f: 3, t, n: Some(n), src: 't', b: None };
    let rows = [
        vec![
            w(vec![big(270.0, "(", "parenleftbigg")], 190.0, 10.0),
            w(vec![g(280.0, 5.0, 1, "a")], 196.0, 10.0),
            w(vec![g(295.0, 5.0, 1, "b")], 196.0, 10.0),
            w(vec![big(305.0, ")", "parenrightbigg")], 190.0, 10.0),
        ],
        vec![w(vec![g(280.0, 5.0, 1, "c")], 208.0, 10.0), w(vec![g(295.0, 5.0, 1, "d")], 208.0, 10.0)],
    ];
    let x = page(
        &[
            prose(150.0),
            rows[0].iter().map(|w| W { gs: w.gs.clone(), base: w.base, size: w.size }).collect(),
            rows[1].iter().map(|w| W { gs: w.gs.clone(), base: w.base, size: w.size }).collect(),
            prose(250.0),
        ],
        &[],
    );
    let (md, _) = convert(&x, &MATH);
    assert!(md.contains("\\begin{pmatrix}a&b\\\\c&d\\end{pmatrix}"), "{md}");
}

#[test]
fn actual_text_latex() {
    let line = vec![w(vec![G { x: 280.0, w: 40.0, f: 1, t: "\\frac{a}{b}", n: None, src: 'a', b: None }], 200.0, 10.0)];
    let (md, report) = convert(&page(&[prose(150.0), line, prose(250.0)], &[]), &MATH);
    assert!(md.contains("$$\\frac{a}{b}$$"), "{md}");
    assert_eq!(report.math.iter().map(|m| m.actual_text).sum::<usize>(), 1);
    // ActualText that is not LaTeX is not math
    let line =
        vec![w(vec![G { x: 280.0, w: 40.0, f: 0, t: "plain words here", n: None, src: 'a', b: None }], 200.0, 10.0)];
    let (md, _) = convert(&page(&[prose(150.0), line, prose(250.0)], &[]), &MATH);
    assert!(!md.contains('$'), "{md}");
}

#[test]
fn unknown_glyph_stays_text_and_is_reported() {
    // a private-use glyph inside a formula: no LaTeX for it, so the formula keeps its words
    let line = vec![w(vec![g(280.0, 5.0, 1, "x"), g(285.0, 5.0, 2, "\u{e123}"), g(290.0, 5.0, 1, "y")], 200.0, 10.0)];
    let (md, report) = convert(&page(&[prose(150.0), line, prose(250.0)], &[]), &MATH);
    assert!(!md.contains('$'), "{md}");
    assert!(report.to_json().contains("\"mathUncertainPages\":[1]"), "{}", report.to_json());
}

#[test]
fn math_off_ignores_glyph_records() {
    // without Options::math the glyph records and font table change nothing
    let line = vec![w(vec![g(90.0, 5.7, 1, "x")], 100.0, 10.0), w(vec![g(95.7, 4.0, 4, "2")], 96.0, 7.0)];
    let x = page(&[prose(80.0), line, prose(120.0)], &[]);
    let stripped: String = x
        .lines()
        .filter(|l| !l.trim_start().starts_with("<g ") && !l.trim_start().starts_with("<font "))
        .map(|l| format!("{l}\n"))
        .collect();
    assert_eq!(convert(&x, &PLAIN).0, convert(&stripped, &PLAIN).0);
    assert_eq!(convert(&stripped, &MATH).0, convert(&stripped, &PLAIN).0);
}

#[test]
fn malformed_records_are_dropped_and_counted() {
    let x = concat!(
        "<doc><page width=\"612\" height=\"792\">",
        "<font id=\"0\" name=\"CMMI10\"/><font id=\"x\" name=\"bad\"/><font id=\"0\" name=\"dup\"/>",
        "<g x=\"1\" w=\"1\" f=\"0\" u=\"78\" s=\"t\"/>",
        "<flow><block><line>",
        "<word xMin=\"1\" yMin=\"1\" xMax=\"9\" yMax=\"9\" fontSize=\"10\" baseline=\"8\">x</word>",
        "<g x=\"nan\" w=\"1\" f=\"0\" u=\"78\" s=\"t\"/><g x=\"1\" w=\"5\" f=\"99\" u=\"zz 110000 d800 78\" s=\"t\"/>",
        "<g x=\"1\" w=\"5\" f=\"0\" u=\"78\" s=\"t\" n=\"&lt;&amp;\"/>",
        "<word xMin=\"bad\" yMin=\"1\" xMax=\"9\" yMax=\"9\">y</word><g x=\"1\" w=\"5\" f=\"0\" u=\"79\" s=\"t\"/>",
        "<xg x=\"1\" w=\"1\" c=\"12\" n=\"vextendsingle\"/><xg x=\"1\" w=\"1\" b=\"8\" z=\"10\" f=\"0\" c=\"12\" n=\"vextendsingle\"/>",
        "</line></block></flow></page></doc>"
    );
    let (_, report) = convert(x, &MATH);
    let r = &report.parse;
    // the orphan, the NaN, the one after a skipped word, and the xg without b/z
    assert_eq!(r.glyphs_skipped, 4, "{r:?}");
    assert_eq!(r.fonts_skipped, 2, "{r:?}");
    let pages = doppler_md::model::parse_with(x, &mut Default::default(), true);
    assert_eq!(pages[0].glyphs.len(), 2);
    assert_eq!(pages[0].extra_glyphs.len(), 1);
    // invalid code points are dropped; the unknown font id gives no font
    assert_eq!(&*pages[0].glyphs[0].text, "x");
    assert!(pages[0].glyphs[0].font.is_none());
    assert_eq!(pages[0].glyphs[1].name.as_deref(), Some("<&"));
}

fn timed<T>(limit: Duration, f: impl FnOnce() -> T) -> T {
    let t = Instant::now();
    let out = f();
    assert!(t.elapsed() < limit, "took {:?}", t.elapsed());
    out
}

#[test]
fn bounded_time_hostile_pages() {
    let limit = Duration::from_secs(if cfg!(debug_assertions) { 60 } else { 6 });
    // one line of 50,000 math glyphs: over the formula cap, kept as text
    let mut gs = Vec::new();
    for i in 0..50_000 {
        gs.push(g(72.0 + i as f64 * 0.01, 0.01, 1, "x"));
    }
    let x = page(&[vec![W { gs, base: 100.0, size: 10.0 }]], &[]);
    timed(limit, || convert(&x, &MATH));
    // 3,000 stacked math-only lines (one cluster) with a fraction bar between every pair
    let mut lines = Vec::new();
    let mut rules = Vec::new();
    for i in 0..3000 {
        let y = 50.0 + i as f64 * 0.24;
        lines.push(vec![w(vec![g(300.0, 5.0, 1, "a"), g(305.0, 5.0, 2, "+")], y, 10.0)]);
        rules.push((299.0, y + 0.1, 311.0, y + 0.15));
    }
    let x = page(&lines, &rules);
    timed(limit, || convert(&x, &MATH));
    // many delimiters and bars in one formula
    let mut gs = Vec::new();
    for i in 0..1400 {
        gs.push(g(72.0 + i as f64 * 0.3, 0.3, 1, if i % 2 == 0 { "(" } else { ")" }));
    }
    let rules: Vec<_> = (0..1000).map(|i| (72.0, 96.0 - i as f64 * 0.001, 500.0, 96.1 - i as f64 * 0.001)).collect();
    let x = page(&[prose(60.0), vec![W { gs, base: 100.0, size: 10.0 }], prose(140.0)], &rules);
    timed(limit, || convert(&x, &MATH));
    // pages over the line cap are skipped and reported
    let lines: Vec<Vec<W>> =
        (0..6000).map(|i| vec![w(vec![g(300.0, 5.0, 1, "y")], 10.0 + i as f64 * 0.1, 10.0)]).collect();
    let (_, report) = timed(limit, || convert(&page(&lines, &[]), &MATH));
    assert!(report.math[0].skipped);
}

/// A small seeded generator (xorshift) so the fuzz needs no crates.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n.max(1)
    }
}

#[test]
fn fuzz_mutated_math_pages() {
    // random formulas (symbols, scripts, bars, delimiters, pieces), then byte mutations
    const SYMS: [&str; 24] = [
        "x", "y", "2", "+", "=", "(", ")", "[", "]", "|", "∑", "∫", "√", "\u{302}", "−", "⎛", "⎜", "⎝", "\u{fe02}",
        "∞", "α", "\u{338}", "{", "}",
    ];
    let mut rng = Rng(0x9e3779b97f4a7c15);
    let limit = Duration::from_secs(if cfg!(debug_assertions) { 30 } else { 3 });
    for _ in 0..300 {
        let mut lines = Vec::new();
        let mut rules = Vec::new();
        for l in 0..(1 + rng.below(8)) {
            let mut words = Vec::new();
            for _ in 0..(1 + rng.below(6)) {
                let mut gs = Vec::new();
                let x0 = 250.0 + rng.below(100) as f64;
                for k in 0..(1 + rng.below(4)) {
                    let t = SYMS[rng.below(SYMS.len() as u64) as usize];
                    let mut gl = g(x0 + k as f64 * 5.0, rng.below(8) as f64, rng.below(6) as usize, t);
                    if rng.below(5) == 0 {
                        gl.b = Some(150.0 + rng.below(40) as f64);
                    }
                    if rng.below(7) == 0 {
                        gl.n = Some(
                            ["summationdisplay", "parenlefttp", "radicalbig", "vextendsingle", "bardbl", "dotacc"]
                                [rng.below(6) as usize],
                        );
                        gl.src = 'c';
                    }
                    gs.push(gl);
                }
                words.push(W {
                    gs,
                    base: 150.0 + l as f64 * 6.0 + rng.below(4) as f64,
                    size: [10.0, 7.0, 5.0][rng.below(3) as usize],
                });
            }
            lines.push(words);
            if rng.below(2) == 0 {
                let y = 150.0 + rng.below(50) as f64;
                rules.push((250.0 + rng.below(50) as f64, y, 300.0 + rng.below(80) as f64, y + 0.4));
            }
        }
        lines.insert(0, prose(100.0));
        lines.push(prose(260.0));
        let mut x = page(&lines, &rules).into_bytes();
        for _ in 0..rng.below(6) {
            let i = rng.below(x.len() as u64) as usize;
            match rng.below(3) {
                0 => x[i] = b"<>\"/= 0x-.9"[rng.below(11) as usize],
                1 => {
                    x.truncate(i);
                }
                _ => {
                    let j = rng.below(x.len() as u64) as usize;
                    let chunk: Vec<u8> = x[j.min(i)..j.max(i)].to_vec();
                    x.splice(i..i, chunk);
                }
            }
        }
        let s = String::from_utf8_lossy(&x).into_owned();
        let (md, _) = timed(limit, || convert(&s, &MATH));
        // formulas are balanced: an even number of unescaped dollar signs per page
        for p in md.split('\x0c') {
            let dollars = p
                .as_bytes()
                .iter()
                .enumerate()
                .filter(|&(k, &c)| c == b'$' && (k == 0 || p.as_bytes()[k - 1] != b'\\'))
                .count();
            let plain_dollars = convert(&s, &PLAIN).0.matches('$').count();
            assert!(dollars % 2 == 0 || plain_dollars > 0, "unbalanced: {p}");
        }
    }
}

// ---------------------------------------------------------------------------------------
// round 2: every emitted formula is ASCII LaTeX; unknown glyphs never become symbols

/// The LaTeX of every formula in a page's Markdown.
fn formulas(md: &str) -> Vec<String> {
    let mut out = Vec::new();
    let b = md.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'$' && (i == 0 || b[i - 1] != b'\\') {
            let dd = b.get(i + 1) == Some(&b'$');
            let start = i + if dd { 2 } else { 1 };
            let mut j = start;
            while j < b.len() && !(b[j] == b'$' && b[j - 1] != b'\\') {
                j += 1;
            }
            out.push(md[start..j.min(md.len())].to_string());
            i = j + if dd { 2 } else { 1 };
        } else {
            i += 1;
        }
    }
    out
}

fn display_line(gs: Vec<G>) -> String {
    let line = vec![W { gs, base: 200.0, size: 10.0 }];
    convert(&page(&[prose(150.0), line, prose(250.0)], &[]), &MATH).0
}

#[test]
fn unicode_symbols_are_spelled_as_commands() {
    // increment, square root as a symbol, en dash as minus, AMS relations, a thin space
    let md = display_line(vec![
        g(280.0, 5.0, 1, "x"),
        g(285.0, 5.0, 2, "\u{2206}"),
        g(290.0, 5.0, 2, "√"),
        g(295.0, 5.0, 2, "–"),
        g(300.0, 5.0, 2, "⋛"),
        g(305.0, 5.0, 2, "≾"),
        g(310.0, 2.0, 2, "\u{202f}"),
        g(312.0, 5.0, 1, "y"),
        g(317.0, 5.0, 2, "⩽"),
        g(322.0, 5.0, 1, "z"),
    ]);
    let f = formulas(&md);
    assert_eq!(f.len(), 1, "{md}");
    assert!(f[0].is_ascii(), "{}", f[0]);
    for cmd in ["\\Delta", "\\surd", "\\gtreqless", "\\precsim", "\\leqslant"] {
        assert!(f[0].contains(cmd), "{cmd} in {}", f[0]);
    }
}

#[test]
fn every_formula_is_ascii() {
    // a stand-alone combining mark and Latin-1 letters stay spellable
    let md = display_line(vec![
        g(280.0, 5.0, 1, "x"),
        g(290.0, 0.0, 2, "\u{302}"),
        g(300.0, 5.0, 2, "="),
        g(305.0, 6.0, 1, "Ø"),
        g(311.0, 5.0, 2, "°"),
    ]);
    for f in formulas(&md) {
        assert!(f.is_ascii(), "{f}");
    }
}

#[test]
fn garbage_code_points_are_unknown_not_symbols() {
    // C1 controls and currency signs from a mis-encoded font are not math symbols: the
    // formula keeps its words or marks the glyph unknown, and never spells them
    let md = display_line(vec![
        g(280.0, 5.0, 1, "x"),
        g(285.0, 5.0, 2, "\u{95}"),
        g(290.0, 5.0, 2, "¥"),
        g(295.0, 5.0, 1, "y"),
    ]);
    for f in formulas(&md) {
        assert!(f.is_ascii() && !f.contains("yen"), "{f}");
    }
}

#[test]
fn one_unknown_glyph_marks_only_that_glyph() {
    // a long formula with a single private-use glyph keeps its structure; the glyph becomes a
    // visible placeholder, and the page is still reported as uncertain
    let mut gs = Vec::new();
    for (k, t) in ["a", "+", "b", "=", "c", "+", "d", "\u{e123}", "e"].iter().enumerate() {
        gs.push(g(280.0 + 6.0 * k as f64, 5.0, if k % 2 == 0 { 1 } else { 2 }, t));
    }
    let line = vec![W { gs, base: 200.0, size: 10.0 }];
    let (md, report) = convert(&page(&[prose(150.0), line, prose(250.0)], &[]), &MATH);
    let f = formulas(&md);
    assert_eq!(f.len(), 1, "{md}");
    assert!(f[0].contains("\\boxed{?}") && f[0].starts_with("a+b=c+d"), "{}", f[0]);
    assert!(report.to_json().contains("\"mathUncertainPages\":[1]"), "{}", report.to_json());
}

#[test]
fn junk_formulas_stay_text() {
    // an equation number alone, a lone delimiter, and a lone check mark are not formulas
    for t in ["(41)", "[", "√"] {
        let gs: Vec<G> = t.chars().enumerate().map(|(k, _)| g(280.0 + 4.0 * k as f64, 4.0, 2, "")).collect();
        let gs: Vec<G> = gs
            .into_iter()
            .zip(t.chars())
            .map(|(mut gl, c)| {
                gl.t = Box::leak(c.to_string().into_boxed_str());
                gl
            })
            .collect();
        let md = display_line(gs);
        assert!(formulas(&md).is_empty(), "{t}: {md}");
    }
}

#[test]
fn dollar_signs_in_prose_are_escaped() {
    let line = vec![
        w(
            vec![
                g(72.0, 5.0, 0, "c"),
                g(77.0, 5.0, 0, "o"),
                g(82.0, 5.0, 0, "s"),
                g(87.0, 5.0, 0, "t"),
                g(92.0, 5.0, 0, "s"),
            ],
            100.0,
            10.0,
        ),
        w(vec![g(100.0, 5.0, 0, "$"), g(105.0, 5.0, 0, "5")], 100.0, 10.0),
        w(vec![g(115.0, 5.0, 0, "f"), g(120.0, 5.0, 0, "o"), g(125.0, 5.0, 0, "r")], 100.0, 10.0),
        w(vec![g(135.0, 5.7, 1, "x")], 100.0, 10.0),
    ];
    let (md, _) = convert(&page(&[prose(80.0), line, prose(120.0)], &[]), &MATH);
    assert!(md.contains("\\$5 for $x$"), "{md}");
}

#[test]
fn operator_names_with_limits_below() {
    // an upright "lim" with a smaller limit line under it, then the argument
    let main = vec![
        w(vec![g(280.0, 3.0, 0, "l"), g(283.0, 3.0, 0, "i"), g(286.0, 8.0, 0, "m")], 200.0, 10.0),
        w(
            vec![g(298.0, 6.0, 1, "f"), g(304.0, 4.0, 0, "("), g(308.0, 5.0, 1, "x"), g(313.0, 4.0, 0, ")")],
            200.0,
            10.0,
        ),
    ];
    let limit = vec![w(vec![g(280.0, 4.0, 5, "x"), g(284.0, 7.0, 2, "→"), g(291.0, 4.0, 4, "0")], 207.0, 7.0)];
    let (md, _) = convert(&page(&[prose(150.0), main, limit, prose(250.0)], &[]), &MATH);
    assert!(md.contains("\\lim_{x\\to0}f(x)"), "{md}");
}

#[test]
fn operator_names_in_text_fonts() {
    // rank and dim in an upright text font between math symbols
    let line = vec![
        w(
            vec![g(250.0, 4.0, 0, "r"), g(254.0, 5.0, 0, "a"), g(259.0, 5.0, 0, "n"), g(264.0, 5.0, 0, "k")],
            200.0,
            10.0,
        ),
        w(vec![g(272.0, 7.0, 1, "A"), g(280.0, 7.0, 0, "+")], 200.0, 10.0),
        w(vec![g(290.0, 5.0, 0, "d"), g(295.0, 3.0, 0, "i"), g(298.0, 8.0, 0, "m")], 200.0, 10.0),
        w(vec![g(310.0, 7.0, 1, "V"), g(318.0, 7.0, 0, "="), g(326.0, 6.0, 1, "n")], 200.0, 10.0),
    ];
    let (md, _) = convert(&page(&[prose(150.0), line, prose(250.0)], &[]), &MATH);
    assert!(md.contains("$$\\operatorname{rank}A+\\dim V=n$$"), "{md}");
}

/// A paragraph of prose lines at the left margin around `y`.
fn paragraph(y: f64, n: usize) -> Vec<Vec<W>> {
    (0..n)
        .map(|k| {
            let yy = y + 12.0 * k as f64;
            // words across the column with ordinary word gaps
            (0..12)
                .map(|i| {
                    let x = 72.0 + 30.0 * i as f64;
                    w(
                        vec![
                            g(x, 6.0, 0, "w"),
                            g(x + 6.0, 5.0, 0, "o"),
                            g(x + 11.0, 5.0, 0, "r"),
                            g(x + 16.0, 6.0, 0, "d"),
                        ],
                        yy,
                        10.0,
                    )
                })
                .collect()
        })
        .collect()
}

#[test]
fn centred_display_keeps_its_text_words() {
    // "n is prime iff n > 1": centred, with text words between math symbols
    let mut lines = paragraph(100.0, 3);
    lines.push(vec![
        w(vec![g(200.0, 6.0, 1, "n")], 200.0, 10.0),
        w(vec![g(210.0, 3.0, 0, "i"), g(213.0, 4.0, 0, "s")], 200.0, 10.0),
        w(
            vec![
                g(222.0, 5.0, 0, "p"),
                g(227.0, 4.0, 0, "r"),
                g(231.0, 3.0, 0, "i"),
                g(234.0, 8.0, 0, "m"),
                g(242.0, 4.0, 0, "e"),
            ],
            200.0,
            10.0,
        ),
        w(vec![g(252.0, 12.0, 2, "⟺")], 200.0, 10.0),
        w(vec![g(268.0, 6.0, 1, "n"), g(276.0, 7.0, 0, ">"), g(285.0, 5.0, 0, "1")], 200.0, 10.0),
    ]);
    lines.extend(paragraph(230.0, 3));
    let (md, _) = convert(&page(&lines, &[]), &MATH);
    assert!(
        md.contains("$$n\\text{ is prime}\\Longleftrightarrow n>1$$")
            || md.contains("$$n\\text{is prime}\\Longleftrightarrow n>1$$"),
        "{md}"
    );
}

#[test]
fn text_beside_display_rows_joins_them() {
    // a cases display whose conditions are separate text lines on the rows' baselines
    let brace =
        |y: f64, t: &'static str, n: &'static str| G { x: 230.0, w: 7.0, f: 3, t, n: Some(n), src: 't', b: Some(y) };
    let mut lines = paragraph(100.0, 3);
    lines.push(vec![
        w(vec![g(200.0, 6.0, 1, "f"), g(208.0, 7.0, 0, "=")], 206.0, 10.0),
        w(vec![brace(196.0, "{", "bracelefttp")], 196.0, 10.0),
        w(vec![g(240.0, 6.0, 1, "x")], 200.0, 10.0),
    ]);
    lines.push(vec![
        w(vec![g(270.0, 3.0, 0, "i"), g(273.0, 4.0, 0, "f")], 200.0, 10.0),
        w(vec![g(280.0, 6.0, 1, "x"), g(288.0, 7.0, 2, "≥"), g(297.0, 5.0, 0, "0")], 200.0, 10.0),
    ]);
    lines.push(vec![w(vec![brace(206.0, "{", "braceleftmid"), brace(216.0, "{", "braceleftbt")], 206.0, 10.0)]);
    lines.push(vec![w(vec![g(240.0, 5.0, 2, "−"), g(245.0, 6.0, 1, "x")], 212.0, 10.0)]);
    lines.push(vec![
        w(vec![g(270.0, 3.0, 0, "i"), g(273.0, 4.0, 0, "f")], 212.0, 10.0),
        w(vec![g(280.0, 6.0, 1, "x"), g(288.0, 7.0, 0, "<"), g(297.0, 5.0, 0, "0")], 212.0, 10.0),
    ]);
    lines.extend(paragraph(240.0, 3));
    let (md, _) = convert(&page(&lines, &[]), &MATH);
    let f = formulas(&md);
    assert_eq!(f.len(), 1, "{md}");
    assert!(f[0].contains("\\begin{cases}x&\\text{if}x\\ge0\\\\-x&\\text{if}x<0\\end{cases}"), "{}", f[0]);
}

#[test]
fn labelled_arrows_and_relations() {
    // A --f--> B with the arrow drawn as touching minus signs and a head; "def" over an equals
    let main = vec![
        w(vec![g(250.0, 7.0, 1, "A")], 200.0, 10.0),
        w(vec![g(260.0, 6.0, 2, "−"), g(265.0, 6.0, 2, "−"), g(270.0, 8.0, 2, "→")], 200.0, 10.0),
        w(vec![g(281.0, 7.0, 1, "B"), g(292.0, 7.0, 0, "=")], 200.0, 10.0),
        w(vec![g(303.0, 6.0, 1, "C")], 200.0, 10.0),
    ];
    let labels = vec![
        w(vec![g(266.0, 4.0, 5, "f")], 194.0, 7.0),
        w(vec![g(290.0, 4.0, 0, "d"), g(294.0, 3.0, 0, "e"), g(297.0, 3.0, 0, "f")], 193.0, 7.0),
    ];
    let (md, _) = convert(&page(&[prose(150.0), labels, main, prose(250.0)], &[]), &MATH);
    let f = formulas(&md);
    assert_eq!(f.len(), 1, "{md}");
    assert_eq!(f[0], "A\\xrightarrow{f}B\\overset{\\text{def}}{=}C", "{md}");
}
