//! The hand-written matchers in `src/patterns.rs` against the original regular expressions
//! (with Python's `\d`/`\s` interpolated as explicit ranges, as the converter used to build them).

use doppler_md::patterns::{data_cell, leading_punct, numbered, numeric_match};
use doppler_md::pyc::{is_decimal, is_space};
use regex::Regex;

fn class(f: fn(char) -> bool) -> String {
    let mut out = String::new();
    let mut start: Option<u32> = None;
    for cp in 0..=0x110000u32 {
        let hit = char::from_u32(cp).is_some_and(f);
        match (hit, start) {
            (true, None) => start = Some(cp),
            (false, Some(a)) => {
                out.push_str(&format!("\\x{{{a:x}}}-\\x{{{:x}}}", cp - 1));
                start = None;
            }
            _ => {}
        }
    }
    out
}

struct Originals {
    numbered: Regex,
    numeric: Regex,
    leading: Regex,
}

fn originals() -> Originals {
    let (d, s) = (class(is_decimal), class(is_space));
    Originals {
        numbered: Regex::new(&format!(
            r"\A(?:\(?[{d}]{{1,3}}(?:\.[{d}]{{1,3}}){{0,3}}[.)]|[{d}]{{1,3}}(?:\.[{d}]{{1,3}}){{1,3}}|\(?[a-z]\)|\([A-Za-z]\)|\(?[ivx]{{1,4}}\)|[IVX]{{1,4}}\.|§[{s}]?[{d}]+)\z"
        ))
        .unwrap(),
        numeric: Regex::new(&format!(
            r"\A(?:[{s}(+\-\u{{2212}}\u{{2013}}]*[$\u{{20ac}}\u{{00a3}}\u{{00a5}}\u{{20a9}}\u{{20b9}}]?[{s}]*[{d}][{d}{s}.,'\u{{2019}}\u{{202f}}\u{{00a0}}]*(?:[.,][{d}]+)?[{s}]*(?:%|\u{{20ac}}|\$|[A-Z]{{3}}|k|m|bn|x)?[){s}]*\z|[{d}]{{1,4}}[/.\-][{d}]{{1,2}}(?:[/.\-][{d}]{{1,4}})?\z|[\u{{2013}}\u{{2014}}\-]\z)"
        ))
        .unwrap(),
        leading: Regex::new(&format!(r"\A([,.;:!?)\]]+)([{d}].*)\z")).unwrap(),
    }
}

fn py_match(re: &Regex, t: &str) -> bool {
    re.is_match(t) || t.strip_suffix('\n').is_some_and(|u| re.is_match(u))
}

fn check(o: &Originals, t: &str) {
    assert_eq!(numbered(t), py_match(&o.numbered, t), "numbered {t:?}");
    assert_eq!(numeric_match(t), py_match(&o.numeric, t), "numeric {t:?}");
    let want = o.leading.captures(t).map(|c| (c.get(1).unwrap().as_str(), c.get(2).unwrap().as_str()));
    assert_eq!(leading_punct(t), want, "leading {t:?}");
}

#[test]
fn matchers_agree_with_original_patterns() {
    let o = originals();
    let fixed = [
        "",
        "1.",
        "1)",
        "(1)",
        "(12.3)",
        "1.2",
        "1.2.3.4",
        "1.2.3.4.5",
        "1.2.3.4.",
        "1234.",
        "(a)",
        "a)",
        "(A)",
        "A)",
        "(iv)",
        "xvi)",
        "IV.",
        "IVXI.",
        "IVXIV.",
        "§ 12",
        "§12",
        "§",
        "§ ",
        "1.\n",
        "1.\n\n",
        "\u{663}.",
        "$1,234.56",
        "(1,234)",
        "- 12 %",
        "−5",
        "–",
        "—",
        "-",
        "12/31/2024",
        "1-2",
        "12345-1",
        "€ 1.5bn",
        "1.5 USD",
        "1.5 usd",
        "3x",
        "10k)",
        "1,5",
        ",28",
        ".,;5 abc",
        ")]28\n",
        ",28\nx",
        "1\u{a0}000",
        "1\u{202f}000,5",
        "\u{a0}1",
        "(+3",
        "££1",
        "1 2 3",
        "1..2",
        "1.,2",
    ];
    for t in fixed {
        check(&o, t);
    }
    let alphabet: Vec<char> = "0123456789\u{663}\u{ff11} \u{a0}\u{2003}\u{202f}\n.,;:!?)](+-\u{2212}\u{2013}\u{2014}$\u{20ac}\u{a3}%'\u{2019}/§ABUSDkmbnxivIVXaz"
        .chars()
        .collect();
    let mut x: u64 = 0x2545f4914f6cdd1d;
    let mut next = || {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        x
    };
    // a narrow alphabet too, so that number-shaped strings are common
    let narrow: Vec<char> = "0123456789.,()-/ \n$%IVi§)".chars().collect();
    for k in 0..400_000 {
        let alpha = if k % 2 == 0 { &alphabet } else { &narrow };
        let len = (next() % 12) as usize;
        let t: String = (0..len).map(|_| alpha[(next() % alpha.len() as u64) as usize]).collect();
        check(&o, &t);
    }
}

/// `DATA_CELL_RE` as `re.search`: `^` becomes `\A` and `$` becomes `\n?\z` (Python's `$`
/// also matches before one trailing newline); the unanchored percentage alternative may match
/// anywhere.
fn data_cell_original() -> Regex {
    let (d, s) = (class(is_decimal), class(is_space));
    Regex::new(&format!(
        r"\A[{s}(+\-\u{{2212}}\u{{2013}}]*[$\u{{20ac}}\u{{00a3}}\u{{00a5}}\u{{20a9}}\u{{20b9}}\u{{0e3f}}][{s}]*[{d}]|\A[(+\-\u{{2212}}\u{{2013}}]?[{d}][{d}{s}.,'\u{{2019}}\u{{202f}}\u{{00a0}}]*[.,][{d}]+[{s}]*(?:%|\u{{20ac}}|\$|[A-Z]{{3}})?\)?\n?\z|\A[+\-\u{{2212}}][{d}]|[{d}][{s}]*%\n?\z|\A[{d}]{{1,2}}[/.\-][{d}]{{1,2}}[/.\-][{d}]{{2,4}}\n?\z"
    ))
    .unwrap()
}

#[test]
fn data_cell_agrees_with_original_pattern() {
    let re = data_cell_original();
    let check = |t: &str| assert_eq!(data_cell(t), re.is_match(t), "data_cell {t:?}");
    for t in [
        "",
        "$5",
        " ( $ 12",
        "€",
        "฿3",
        "1.5",
        "1,234.56",
        "(1.5)",
        "-1.5%",
        "1.5 USD",
        "1.5 US",
        "1.5 USDX",
        "1.5)\n",
        "1.5\n\n",
        "1.",
        ".5",
        "1 . 5",
        "1.5 %)",
        "+3",
        "−3",
        "–3",
        "-x",
        "Total 12 %",
        "12%\n",
        "%",
        " %",
        "5 \u{a0}%",
        "12/31/2024",
        "1-2-24",
        "1.2.123456",
        "123/1/2024",
        "1/2/3",
        "\u{663}.\u{664}",
        "1\u{202f}000,50 €",
        "1'000.5$",
        "Q3 2024",
        "Region",
        "2025 Change 2024",
    ] {
        check(t);
    }
    let wide: Vec<char> = "0123456789\u{663}\u{ff11} \u{a0}\u{2003}\u{202f}\n.,'\u{2019}()+-\u{2212}\u{2013}\u{2014}$\u{20ac}\u{a3}\u{a5}\u{20a9}\u{20b9}\u{e3f}%/USDKkmxaz"
        .chars()
        .collect();
    let narrow: Vec<char> = "0123456789.,()-+/ \n$%USD".chars().collect();
    let mut x: u64 = 0x9e3779b97f4a7c15;
    let mut next = || {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        x
    };
    for k in 0..400_000 {
        let alpha = if k % 2 == 0 { &wide } else { &narrow };
        let len = (next() % 14) as usize;
        let t: String = (0..len).map(|_| alpha[(next() % alpha.len() as u64) as usize]).collect();
        check(&t);
    }
}
