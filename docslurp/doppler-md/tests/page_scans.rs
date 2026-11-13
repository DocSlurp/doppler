//! Full-page scans (`page-scan`): placement from the producer's `scan=`/`scanImage=` and
//! `copy=`/`orient=` attributes, the `![page N scan](...)` first line, `--scan-pages`, and (with
//! the real pdftotext and pdfregions) extraction that copies instead of rendering: a JPEG byte
//! for byte, a bilevel scan as a 1-bit PNG, other samples decoded once, and a scan budget of
//! its own.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use doppler_md::extract::layout::LinkStyle;
use doppler_md::extract::settings::ScanPages;
use doppler_md::images::Fate;
use doppler_md::{Converter, Options, Params, ParseReport, Report};

fn word(x0: f64, y: f64, text: &str, invisible: bool) -> String {
    let x1 = x0 + 6.0 * text.len() as f64;
    format!(
        "<word xMin=\"{x0}\" xMax=\"{x1}\" yMin=\"{y}\" yMax=\"{}\" fontSize=\"10\" fontName=\"Serif\" baseline=\"{}\"{}>{text}</word>",
        y + 10.0,
        y + 8.0,
        if invisible { " invisible=\"1\"" } else { "" }
    )
}

/// `n` lines of five words each.
fn lines(n: usize, invisible: bool) -> String {
    (0..n)
        .map(|k| {
            let y = 80.0 + 14.0 * k as f64;
            let words: String =
                (0..5).map(|i| word(72.0 + 60.0 * i as f64, y, &format!("w{k}x{i}"), invisible)).collect();
            format!("<block><line>{words}</line></block>")
        })
        .collect()
}

/// An image record as pdftotext -images writes it (`extra`: copy=, orient=, kind=).
fn image(b: (f64, f64, f64, f64), px: (u32, u32), obj: u32, extra: &str) -> String {
    format!(
        "<image xMin=\"{}\" yMin=\"{}\" xMax=\"{}\" yMax=\"{}\" width=\"{}\" height=\"{}\" ref=\"{obj} 0\"{extra}/>",
        b.0, b.1, b.2, b.3, px.0, px.1
    )
}

const FULL: (f64, f64, f64, f64) = (0.0, 0.0, 612.0, 792.0);
const DCT_DICT: &str = " filter=\"dct\" bpc=\"8\" cs=\"gray\" copy=\"jpeg\"";
/// what pdftotext writes for a scan with a copy class drawn upright, alone on its page
const EXACT: &str = " orient=\"0\" overlaid=\"0\" copyable=\"1\"";
const DCT: &str = " filter=\"dct\" bpc=\"8\" cs=\"gray\" copy=\"jpeg\" orient=\"0\" overlaid=\"0\" copyable=\"1\"";

fn page(number: u32, scan: Option<(&str, usize)>, body: &str) -> String {
    let scan = scan.map(|(k, i)| format!(" scan=\"{k}\" scanImage=\"{i}\"")).unwrap_or_default();
    format!("<page width=\"612\" height=\"792\" number=\"{number}\"{scan}><flow>{body}</flow></page>")
}

fn convert(xhtml: &str, scans: ScanPages) -> (Vec<String>, Report) {
    let p = Params::default();
    let o = Options { markers: true, ..Options::default() };
    let mut parse = ParseReport::default();
    let pages = doppler_md::model::parse_with(xhtml, &mut parse, false);
    let mut out = Vec::new();
    let mut c = Converter::new(&p, o, None);
    c.set_link_style(LinkStyle { scans, ..LinkStyle::default() });
    for pg in pages {
        c.push(pg, &mut |_, md| out.push(md));
    }
    let report = c.finish(parse, &mut |_, md| out.push(md));
    (out, report)
}

fn doc(pages: &[String]) -> String {
    format!("<doc>{}</doc>", pages.concat())
}

#[test]
fn a_scan_without_text_is_a_page_scan_on_every_page() {
    // the same scan image on three pages used to be a watermark (repeated backdrop) or a block
    let pages: Vec<String> =
        (1..=3).map(|n| page(n, Some(("single", 1)), &image(FULL, (2550, 3300), 7 + n, DCT))).collect();
    let (md, report) = convert(&doc(&pages), ScanPages::Image);
    for (k, m) in md.iter().enumerate() {
        let n = k + 1;
        assert_eq!(m, &format!("![page {n} scan](images/page-{n:04}/image-001.jpg)"));
    }
    assert!(report.images.iter().all(|n| n.fate == Fate::PageScan), "{:?}", report.images);
    assert!(report.to_json().contains("\"placed\":\"page-scan\",\"scan\":\"jpeg\""), "{}", report.to_json());
}

#[test]
fn an_ocr_layer_follows_the_scan_marker() {
    let body = image(FULL, (2550, 3300), 9, DCT) + &lines(6, true);
    let (md, report) = convert(&doc(&[page(4, Some(("single", 1)), &body)]), ScanPages::Image);
    let first = md[0].lines().next().unwrap();
    assert_eq!(first, "![page 4 scan](images/page-0004/image-001.jpg)", "{}", md[0]);
    assert!(md[0].contains("w0x0"), "the OCR text stays: {}", md[0]);
    assert!(md[0].find("w0x0").unwrap() > first.len());
    // before, an image under kept text was a background
    assert_eq!(report.images[0].fate, Fate::PageScan);
}

#[test]
fn a_backdrop_under_two_hundred_words_stays_a_background() {
    // the producer says no scan (more than 20 visible words): today's placement
    let body = image(FULL, (2550, 3300), 9, DCT) + &lines(40, false);
    let (md, report) = convert(&doc(&[page(1, None, &body)]), ScanPages::Image);
    assert_eq!(report.images[0].fate, Fate::Background);
    assert!(!md[0].contains("scan]("), "{}", md[0]);
    assert!(!md[0].contains("](images/"), "{}", md[0]);
}

#[test]
fn a_layered_scan_groups_its_layers_and_is_rendered() {
    // a JPX background with a full-page JBIG2 stencil text layer on it (MRC)
    let body = image(FULL, (2550, 3300), 5, " filter=\"jpx\" bpc=\"8\" cs=\"rgb\" copy=\"decode\"")
        + &image(FULL, (2550, 3300), 6, " kind=\"mask\" filter=\"jbig2\" bpc=\"1\" cs=\"none\" copy=\"bilevel\"");
    let (md, report) = convert(&doc(&[page(2, Some(("layered", 1)), &body)]), ScanPages::Image);
    assert_eq!(md[0], "![page 2 scan](images/page-0002/image-001.webp)");
    let n = &report.images;
    assert_eq!((n[0].fate, n[0].members), (Fate::PageScan, 2));
    assert_eq!((n[1].fate, n[1].group.as_deref()), (Fate::Grouped, Some("p2.1")));
    assert_eq!(n[0].scan.map(|s| s.as_str()), Some("render"));
}

#[test]
fn tiled_strips_are_one_scan_of_their_union() {
    let body: String = (0..4)
        .map(|k| image((0.0, 198.0 * k as f64, 612.0, 198.0 * (k + 1) as f64), (2550, 825), 20 + k, DCT))
        .collect();
    let (md, report) = convert(&doc(&[page(6, Some(("tiled", 1)), &body)]), ScanPages::Image);
    assert_eq!(md[0], "![page 6 scan](images/page-0006/image-001.webp)");
    let n = &report.images;
    assert_eq!((n[0].fate, n[0].members, n[0].bbox), (Fate::PageScan, 4, FULL));
    assert!(n[1..].iter().all(|x| x.fate == Fate::Grouped && x.group.as_deref() == Some("p6.1")));
}

#[test]
fn turned_or_unreferenced_scans_are_rendered() {
    let turned = image(FULL, (2550, 3300), 9, &format!("{DCT_DICT} orient=\"90\" overlaid=\"0\" copyable=\"1\""));
    let (md, _) = convert(&doc(&[page(1, Some(("single", 1)), &turned)]), ScanPages::Image);
    assert_eq!(md[0], "![page 1 scan](images/page-0001/image-001.webp)");
    // its page did not pass the copy allow-list (no copyable="1": a page number, a redaction
    // box, a clip), the overpaint estimate saw something over it, or a producer that does not
    // say: rendered, never copied
    for extra in [
        " orient=\"0\" overlaid=\"0\"",
        " orient=\"0\" overlaid=\"1\" copyable=\"1\"",
        " orient=\"0\" copyable=\"1\"",
        " overlaid=\"0\" copyable=\"1\"",
        " orient=\"0\" overlaid=\"0\" copyable=\"0\"",
        "",
    ] {
        let im = image(FULL, (2550, 3300), 9, &format!("{DCT_DICT}{extra}"));
        let (md, report) = convert(&doc(&[page(1, Some(("single", 1)), &im)]), ScanPages::Image);
        assert_eq!(md[0], "![page 1 scan](images/page-0001/image-001.webp)", "{extra}");
        assert_eq!(report.images[0].scan.map(|s| s.as_str()), Some("render"), "{extra}");
    }
    // bilevel and decodable scans: .png and the raster format
    let ccitt =
        image(FULL, (2550, 3300), 9, &format!(" filter=\"ccitt\" bpc=\"1\" cs=\"gray\" copy=\"bilevel\"{EXACT}"));
    let flate =
        image(FULL, (2550, 3300), 10, &format!(" filter=\"flate\" bpc=\"8\" cs=\"rgb\" copy=\"decode\"{EXACT}"));
    let (md, report) =
        convert(&doc(&[page(1, Some(("single", 1)), &ccitt), page(2, Some(("single", 1)), &flate)]), ScanPages::Image);
    assert_eq!(
        md,
        ["![page 1 scan](images/page-0001/image-001.png)", "![page 2 scan](images/page-0002/image-001.webp)"]
    );
    assert_eq!(report.images[1].scan.map(|s| s.as_str()), Some("decode"));
}

#[test]
fn a_rebuilt_cross_reference_table_makes_scans_render_only() {
    // pdftotext marks the page where it found the table rebuilt (xrefRebuilt="1"): from there
    // on no image is copyable or plain, even with copyable="1" (a hostile or older producer);
    // the pages before keep their copy class, and the report says the document was rebuilt
    let p1 = page(1, Some(("single", 1)), &image(FULL, (2550, 3300), 9, DCT));
    let p2 = image(FULL, (2550, 3300), 10, DCT);
    let p2 = format!(
        "<page width=\"612\" height=\"792\" number=\"2\" scan=\"single\" scanImage=\"1\" xrefRebuilt=\"1\"><flow>{p2}</flow></page>"
    );
    let p3 = page(3, Some(("single", 1)), &image(FULL, (2550, 3300), 11, DCT));
    let (md, report) = convert(&doc(&[p1.clone(), p2, p3]), ScanPages::Image);
    assert_eq!(md[0], "![page 1 scan](images/page-0001/image-001.jpg)");
    assert_eq!(md[1], "![page 2 scan](images/page-0002/image-001.webp)");
    assert_eq!(md[2], "![page 3 scan](images/page-0003/image-001.webp)", "the mark holds for later pages");
    assert!(report.parse.xref_rebuilt);
    // the end-of-document mark alone: the report says so (extraction then copies nothing)
    let (md, report) = convert(&format!("<doc>{p1}<xref rebuilt=\"1\"/></doc>"), ScanPages::Image);
    assert_eq!(md[0], "![page 1 scan](images/page-0001/image-001.jpg)");
    assert!(report.parse.xref_rebuilt);
    let (_, report) = convert(&format!("<doc>{p1}<xref rebuilt=\"0\"/></doc>"), ScanPages::Image);
    assert!(!report.parse.xref_rebuilt);
}

#[test]
fn scan_pages_skip_and_render() {
    let body = image(FULL, (2550, 3300), 9, DCT);
    let xhtml =
        doc(&[page(1, Some(("single", 1)), &body), page(2, Some(("single", 1)), &(body.clone() + &lines(3, true)))]);
    // skip: today's placement (a background under text; without text a block marker, or a
    // watermark when it repeats on enough pages)
    let (md, report) = convert(&xhtml, ScanPages::Skip);
    assert!(!md[1].contains("](images/"), "{}", md[1]);
    assert_eq!((report.images[0].fate, report.images[1].fate), (Fate::Watermark, Fate::Background));
    assert!(report.images.iter().all(|n| n.scan.is_some()), "still known as scans: not extracted");
    let (md, report) = convert(&doc(&[page(1, Some(("single", 1)), &body)]), ScanPages::Skip);
    assert_eq!(md[0], "![image](images/page-0001/image-001.webp)");
    assert_eq!(report.images[0].fate, Fate::Block);
    // render: a page scan, always the raster format
    let (md, report) = convert(&xhtml, ScanPages::Render);
    assert_eq!(md[0], "![page 1 scan](images/page-0001/image-001.webp)");
    assert!(md[1].starts_with("![page 2 scan](images/page-0002/image-001.webp)\n\n"), "{}", md[1]);
    assert!(report.images.iter().all(|n| n.fate == Fate::PageScan));
}

#[test]
fn hostile_scan_attributes_are_ignored() {
    // a scanImage past the list, zero, not a number, on a stencil mask, or an unknown kind
    for (kind, k, extra) in [
        ("single", "2", ""),
        ("single", "0", ""),
        ("single", "1x", ""),
        ("single", "99999999", ""),
        ("whole", "1", ""),
        ("single", "1", " kind=\"mask\""),
    ] {
        let body = image(FULL, (2550, 3300), 9, &format!("{DCT}{extra}"));
        let xhtml = format!(
            "<doc><page width=\"612\" height=\"792\" number=\"1\" scan=\"{kind}\" scanImage=\"{k}\"><flow>{body}</flow></page></doc>"
        );
        let (md, report) = convert(&xhtml, ScanPages::Image);
        assert!(!md[0].contains("scan]("), "{kind} {k} {extra}: {}", md[0]);
        assert!(report.images.iter().all(|n| n.fate != Fate::PageScan && n.scan.is_none()));
    }
}

// --- the real tools ---------------------------------------------------------------------------

const PDFTOMARKDOWN: &str = env!("CARGO_BIN_EXE_pdftomarkdown");

/// pdftotext and pdfregions of this tree (`$PDFTOTEXT`, `$PDFREGIONS` or ../../build/utils)
/// and python3 for the fixture writer; `None` (the test passes, skipped) without them.
fn tools() -> Option<(PathBuf, PathBuf)> {
    let find = |var: &str, name: &str| match std::env::var_os(var).filter(|p| !p.is_empty()) {
        Some(p) => PathBuf::from(p),
        None => Path::new(env!("CARGO_MANIFEST_DIR")).join("../../build/utils").join(name),
    };
    let (t, r) = (find("PDFTOTEXT", "pdftotext"), find("PDFREGIONS", "pdfregions"));
    let usage = Command::new(&r).arg("-h").output().ok()?;
    let copy = String::from_utf8_lossy(&usage.stderr).contains("-max-copy-bytes");
    let python = Command::new("python3").arg("-c").arg("pass").status().is_ok_and(|s| s.success());
    (t.is_file() && copy && python).then_some((t, r))
}

struct Scratch(PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn fixtures(tag: &str) -> Option<(PathBuf, PathBuf, Scratch)> {
    let Some((t, r)) = tools() else {
        eprintln!("skipped: needs this tree's pdftotext and pdfregions (copy jobs) and python3");
        return None;
    };
    let d = std::env::temp_dir().join(format!("page-scans-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/scan_pdfs.py");
    let ok = Command::new("python3").arg("-I").arg(script).arg(&d).status().unwrap().success();
    assert!(ok, "fixture writer failed");
    Some((t, r, Scratch(d)))
}

fn run(t: &Path, r: &Path, args: &[&str]) -> Output {
    Command::new(PDFTOMARKDOWN).args(args).env("PDFTOTEXT", t).env("PDFREGIONS", r).output().unwrap()
}

fn manifest(dir: &Path) -> serde_json::Value {
    serde_json::from_slice(&std::fs::read(dir.join("manifest.json")).unwrap()).unwrap()
}

fn entry<'a>(m: &'a serde_json::Value, id: &str) -> &'a serde_json::Value {
    m["images"].as_array().unwrap().iter().find(|e| e["id"] == id).unwrap_or_else(|| panic!("{id} missing: {m}"))
}

#[test]
fn whole_scanned_documents_are_copied_not_rendered() {
    let Some((t, r, s)) = fixtures("copy") else { return };
    let pdf = s.0.join("scans.pdf");
    let out = s.0.join("out");
    let res = run(&t, &r, &["--images-dir", out.to_str().unwrap(), "--scan-max-dpi", "72", pdf.to_str().unwrap()]);
    assert_eq!(res.status.code(), Some(0), "{}", String::from_utf8_lossy(&res.stderr));
    let md = String::from_utf8(res.stdout.clone()).unwrap();
    let pages: Vec<&str> = md.split('\u{c}').collect();
    assert_eq!(pages.len(), 7, "{md}");
    assert_eq!(pages[0], "![page 1 scan](images/page-0001/image-001.jpg)");
    assert!(pages[1].starts_with("![page 2 scan](images/page-0001/image-001.jpg)\n\nScanned line 0"), "{}", pages[1]);
    assert_eq!(pages[2], "![page 3 scan](images/page-0003/image-001.png)");
    assert_eq!(pages[3], "![page 4 scan](images/page-0004/image-001.webp)");
    assert_eq!(pages[4], "![page 5 scan](images/page-0005/image-001.webp)", "turned: rendered");
    assert_eq!(pages[5], "![page 6 scan](images/page-0006/image-001.webp)", "tiled: rendered");
    assert!(pages[6].contains("![image](images/page-0007/image-001.webp)"), "{}", pages[6]);
    // the same Markdown without extraction
    let plain = run(&t, &r, &["--markers", pdf.to_str().unwrap()]);
    assert_eq!(plain.stdout, res.stdout);
    // the JPEG is the embedded stream, byte for byte
    let jpg = std::fs::read(out.join("page-0001/image-001.jpg")).unwrap();
    assert_eq!(jpg, std::fs::read(s.0.join("scan.jpg")).unwrap());
    // the CCITT scan: a 1-bit PNG at its own size
    let png = std::fs::read(out.join("page-0003/image-001.png")).unwrap();
    let dec = png::Decoder::new(std::io::Cursor::new(png)).read_info().unwrap();
    let info = dec.info();
    assert_eq!((info.width, info.height, info.bit_depth), (800, 1000, png::BitDepth::One));
    let m = manifest(&out);
    assert_eq!(m["complete"], true);
    assert_eq!(m["scan_pages"], "image");
    for (id, source, format) in [
        ("p1.1", "copy", "jpg"),
        ("p3.1", "decode", "png"),
        ("p4.1", "decode", "webp"),
        ("p5.1", "render", "webp"),
        ("p6.1", "render", "webp"),
        ("p7.1", "render", "webp"),
    ] {
        let e = entry(&m, id);
        assert_eq!(
            (e["status"].as_str(), e["source"].as_str(), e["format"].as_str()),
            (Some("extracted"), Some(source), Some(format)),
            "{e}"
        );
    }
    assert_eq!(entry(&m, "p1.1")["fate"], "page-scan");
    // the decoded RGB scan was scaled to --scan-max-dpi (72: one pixel a point)
    assert_eq!((entry(&m, "p4.1")["width"].as_u64(), entry(&m, "p4.1")["height"].as_u64()), (Some(612), Some(792)));
    // a rendered scan at --scan-dpi (200 by default)
    assert_eq!(entry(&m, "p5.1")["width"], 1700);
    // page 2 repeats page 1's object: its link is page 1's file
    assert_eq!(
        (entry(&m, "p2.1")["reason"].as_str(), entry(&m, "p2.1")["repeat_of"].as_str()),
        (Some("repeat"), Some("p1.1"))
    );
    assert_eq!(m["totals"]["scans"], 5);
    // the default --images-deadline: 120 s plus 0.5 s for each of the 7 converted pages
    assert_eq!(m["deadline_ms"], 123_500);
}

#[test]
fn batches_stopped_by_a_budget_count_toward_the_abort() {
    // one page a batch and no scan budget: the scans of pages 1 and 3 each stop their batch
    // (budget), two failed batches in a row, so the rest is aborted without another renderer
    let Some((t, r, s)) = fixtures("abort") else { return };
    let pdf = s.0.join("scans.pdf");
    let out = s.0.join("out");
    let args =
        ["--images-dir", out.to_str().unwrap(), "--images-batch-pages", "1", "--scan-max", "0", pdf.to_str().unwrap()];
    let res = run(&t, &r, &args);
    assert_eq!(res.status.code(), Some(5), "{}", String::from_utf8_lossy(&res.stderr));
    let m = manifest(&out);
    let statuses: Vec<&str> = m["batches"].as_array().unwrap().iter().map(|b| b["status"].as_str().unwrap()).collect();
    assert_eq!(statuses, ["budget", "budget", "aborted", "aborted", "aborted", "aborted"], "{m}");
    for id in ["p1.1", "p3.1"] {
        assert_eq!(entry(&m, id)["reason"], "budget", "{m}");
    }
    for id in ["p4.1", "p5.1", "p6.1", "p7.1"] {
        let e = entry(&m, id);
        assert_eq!((e["status"].as_str(), e["reason"].as_str()), (Some("not-extracted"), Some("aborted")), "{e}");
    }
    assert_eq!(m["totals"]["extracted"], 0);
    // one batch stopped by a budget is not an abort: with no pictures' budget, only page 7's
    // batch stops (the scans have their own budget) and nothing is aborted
    let out = s.0.join("figures");
    let args = [
        "--images-dir",
        out.to_str().unwrap(),
        "--images-batch-pages",
        "1",
        "--images-max",
        "0",
        pdf.to_str().unwrap(),
    ];
    let res = run(&t, &r, &args);
    assert_eq!(res.status.code(), Some(5), "{}", String::from_utf8_lossy(&res.stderr));
    let m = manifest(&out);
    assert!(m["batches"].as_array().unwrap().iter().all(|b| b["status"] != "aborted"), "{m}");
    assert_eq!(entry(&m, "p7.1")["reason"], "budget", "{m}");
}

#[test]
fn scans_have_their_own_budget() {
    let Some((t, r, s)) = fixtures("budget") else { return };
    let pdf = s.0.join("scans.pdf");
    // the pictures' budget of one file does not touch the scans
    let out = s.0.join("a");
    let res = run(&t, &r, &["--images-dir", out.to_str().unwrap(), "--images-max", "1", pdf.to_str().unwrap()]);
    assert_eq!(res.status.code(), Some(0), "{}", String::from_utf8_lossy(&res.stderr));
    let m = manifest(&out);
    assert_eq!((m["totals"]["scans"].as_u64(), m["totals"]["extracted"].as_u64()), (Some(5), Some(6)), "{m}");
    // two scans per batch: the rest of the scans are not extracted (budget), the picture is
    let out = s.0.join("b");
    let res = run(&t, &r, &["--images-dir", out.to_str().unwrap(), "--scan-max", "2", pdf.to_str().unwrap()]);
    assert_eq!(res.status.code(), Some(5), "{}", String::from_utf8_lossy(&res.stderr));
    let m = manifest(&out);
    assert_eq!(m["complete"], false);
    assert_eq!(m["totals"]["scans"], 2);
    assert_eq!(entry(&m, "p7.1")["status"], "extracted");
    let budget = m["images"].as_array().unwrap().iter().filter(|e| e["reason"] == "budget").count();
    assert_eq!(budget, 3, "{m}");
    assert_eq!(m["batches"][0]["status"], "budget");
}

#[test]
fn scan_pages_skip_extracts_no_scan_and_render_renders_them() {
    let Some((t, r, s)) = fixtures("modes") else { return };
    let pdf = s.0.join("scans.pdf");
    let out = s.0.join("skip");
    let res = run(&t, &r, &["--images-dir", out.to_str().unwrap(), "--scan-pages", "skip", pdf.to_str().unwrap()]);
    let m = manifest(&out);
    assert_eq!(res.status.code(), Some(0), "{}", String::from_utf8_lossy(&res.stderr));
    assert_eq!(m["totals"]["scans"], 0);
    assert!(
        m["images"]
            .as_array()
            .unwrap()
            .iter()
            .all(|e| e["fate"] != "page-scan" && (e["id"] == "p7.1" || e["status"] != "extracted")),
        "{m}"
    );
    // a scan placed as a block picture keeps its link but gets no file
    assert_eq!(
        (entry(&m, "p3.1")["fate"].as_str(), entry(&m, "p3.1")["reason"].as_str()),
        (Some("block"), Some("scan-pages"))
    );
    let out = s.0.join("render");
    let res = run(&t, &r, &["--images-dir", out.to_str().unwrap(), "--scan-pages", "render", pdf.to_str().unwrap()]);
    assert_eq!(res.status.code(), Some(0), "{}", String::from_utf8_lossy(&res.stderr));
    let md = String::from_utf8(res.stdout).unwrap();
    assert!(md.starts_with("![page 1 scan](images/page-0001/image-001.webp)"), "{md}");
    let m = manifest(&out);
    let scans: Vec<&serde_json::Value> =
        m["images"].as_array().unwrap().iter().filter(|e| e["fate"] == "page-scan").collect();
    assert_eq!(scans.len(), 6);
    assert!(scans.iter().all(|e| e["source"] == "render" && e["status"] == "extracted"), "{m}");
}

#[test]
fn scans_with_anything_over_them_are_rendered_not_copied() {
    // a copy would show what a redaction box, a white-out or a clip hides on the page
    let Some((t, r, s)) = fixtures("redacted") else { return };
    let pdf = s.0.join("redacted.pdf");
    let out = s.0.join("out");
    let res = run(&t, &r, &["--images-dir", out.to_str().unwrap(), pdf.to_str().unwrap()]);
    assert_eq!(res.status.code(), Some(0), "{}", String::from_utf8_lossy(&res.stderr));
    let m = manifest(&out);
    for id in ["p1.1", "p2.1", "p3.1", "p4.1"] {
        let e = entry(&m, id);
        assert_eq!(
            (e["fate"].as_str(), e["source"].as_str(), e["format"].as_str()),
            (Some("page-scan"), Some("render"), Some("webp")),
            "{e}"
        );
    }
    assert_eq!(
        (entry(&m, "p5.1")["source"].as_str(), entry(&m, "p5.1")["format"].as_str()),
        (Some("copy"), Some("jpg"))
    );
    // a visible page number over an OCR'd scan: rendered too (the copy allow-list's accepted
    // cost: only invisible text may share a copied scan's page)
    let e = entry(&m, "p6.1");
    assert_eq!((e["source"].as_str(), e["format"].as_str()), (Some("render"), Some("webp")), "{e}");
    // the rendered files show the black boxes: the middle of the band (x 300, y 400 in PDF
    // space, so 392 points from the top) is black, where the scan itself is not
    for page in [1, 2] {
        let e = entry(&m, &format!("p{page}.1"));
        let bytes = std::fs::read(out.join(e["path"].as_str().unwrap())).unwrap();
        let img = webp::Decoder::new(&bytes).decode().expect("a WebP");
        let (w, h) = (img.width() as usize, img.height() as usize);
        let channels = img.len() / (w * h);
        let (x, y) = (w * 300 / 612, h * 392 / 792);
        let px = &img[(y * w + x) * channels..(y * w + x) * channels + 3];
        assert!(px.iter().all(|&v| v < 16), "page {page}: the redaction box is in the file: {px:?}");
    }
}

#[test]
fn a_spent_figure_budget_does_not_stop_the_scans_after_it() {
    // two pictures on page 1 with a figure budget of one, then two scans in the same batch
    let Some((t, r, s)) = fixtures("fig-budget") else { return };
    let pdf = s.0.join("budget.pdf");
    let out = s.0.join("out");
    let res = run(&t, &r, &["--images-dir", out.to_str().unwrap(), "--images-max", "1", pdf.to_str().unwrap()]);
    assert_eq!(res.status.code(), Some(5), "{}", String::from_utf8_lossy(&res.stderr));
    let m = manifest(&out);
    let pictures: Vec<&serde_json::Value> =
        m["images"].as_array().unwrap().iter().filter(|e| e["page"] == 1 && e["fate"] == "block").collect();
    assert_eq!(pictures.len(), 2, "{m}");
    assert_eq!(pictures.iter().filter(|e| e["status"] == "extracted").count(), 1, "{m}");
    assert_eq!(pictures.iter().filter(|e| e["reason"] == "budget").count(), 1, "{m}");
    for id in ["p2.1", "p3.1"] {
        assert_eq!(entry(&m, id)["status"], "extracted", "{m}");
    }
    assert_eq!(m["totals"]["scans"], 2);
    assert_eq!(m["batches"][0]["status"], "budget");
}

#[test]
fn a_document_whose_cross_reference_table_was_rebuilt_copies_no_scan() {
    // page 3's content entry is broken: pdftotext rebuilds the table on page 3, after pages 1
    // (a JPEG) and 2 (a Flate scan to decode) were found copyable. A rebuilt table may resolve a
    // reference to another object body than the page draws, so nothing is copied: the JPEG
    // (linked as .jpg before the mark) is not extracted, the Flate scan is rendered into its
    // link, and page 3 is rendered
    let Some((t, r, s)) = fixtures("rebuilt") else { return };
    let pdf = s.0.join("rebuilt.pdf");
    let out = s.0.join("out");
    let res = run(&t, &r, &["--images-dir", out.to_str().unwrap(), pdf.to_str().unwrap()]);
    assert!(matches!(res.status.code(), Some(0 | 5)), "{}", String::from_utf8_lossy(&res.stderr));
    let md = String::from_utf8(res.stdout.clone()).unwrap();
    let pages: Vec<&str> = md.split('\u{c}').collect();
    assert_eq!(
        pages,
        [
            "![page 1 scan](images/page-0001/image-001.jpg)",
            "![page 2 scan](images/page-0002/image-001.webp)",
            "![page 3 scan](images/page-0003/image-001.webp)"
        ]
    );
    let m = manifest(&out);
    let e = entry(&m, "p1.1");
    assert_eq!(
        (e["status"].as_str(), e["reason"].as_str(), e["error"].as_str()),
        (Some("not-extracted"), Some("copy"), Some("xref")),
        "{e}"
    );
    assert!(!out.join("page-0001/image-001.jpg").exists());
    for id in ["p2.1", "p3.1"] {
        let e = entry(&m, id);
        assert_eq!((e["status"].as_str(), e["source"].as_str()), (Some("extracted"), Some("render")), "{e}");
    }
}

#[test]
fn a_scan_past_the_copy_cap_is_rendered_not_decoded() {
    // 4200 x 4200 samples (17.6 M) would be decoded whole; the render at --scan-dpi is cheaper
    let Some((t, r, s)) = fixtures("huge") else { return };
    let pdf = s.0.join("huge.pdf");
    let out = s.0.join("out");
    let res = run(&t, &r, &["--images-dir", out.to_str().unwrap(), pdf.to_str().unwrap()]);
    assert_eq!(res.status.code(), Some(0), "{}", String::from_utf8_lossy(&res.stderr));
    assert_eq!(String::from_utf8_lossy(&res.stdout), "![page 1 scan](images/page-0001/image-001.webp)");
    let m = manifest(&out);
    let e = entry(&m, "p1.1");
    assert_eq!((e["status"].as_str(), e["source"].as_str()), (Some("extracted"), Some("render")), "{e}");
    assert_eq!((e["width"].as_u64(), e["height"].as_u64()), (Some(1700), Some(2200)), "{e}");
}

#[test]
fn an_encode_that_ends_past_the_deadline_is_dropped() {
    // the lossless encode at the highest effort takes seconds; the document deadline is one
    let Some((t, r, s)) = fixtures("slow") else { return };
    let pdf = s.0.join("slow.pdf");
    let out = s.0.join("out");
    let args = [
        "--images-dir",
        out.to_str().unwrap(),
        "--images-deadline",
        "1",
        "--raster-lossless",
        "--lossless-method",
        "6",
        "--lossless-quality",
        "100",
        pdf.to_str().unwrap(),
    ];
    let res = run(&t, &r, &args);
    assert_eq!(res.status.code(), Some(5), "{}", String::from_utf8_lossy(&res.stderr));
    let m = manifest(&out);
    assert_eq!(m["complete"], false);
    let e = entry(&m, "p1.1");
    assert_eq!((e["status"].as_str(), e["reason"].as_str()), (Some("not-extracted"), Some("deadline")), "{e}");
    assert!(!out.join("page-0001/image-001.webp").exists());
    // an explicit --images-deadline is the effective one, in the manifest
    assert_eq!(m["deadline_ms"], 1000);
}

/// The scan thresholds pass to pdftotext and join the images plan's settings digest: a plan
/// made with other thresholds is refused (exit 2, nothing written); the same values spelled
/// otherwise are not other thresholds.
#[test]
fn scan_thresholds_change_the_scans_and_bind_the_plan() {
    let Some((t, r, s)) = fixtures("thresholds") else { return };
    let pdf = s.0.join("scans.pdf");
    let pdf = pdf.to_str().unwrap();
    // page 7: 54 visible words over a picture of 0.103 of the page, a scan only when both
    // thresholds allow it
    let md = |extra: &[&str]| {
        let mut a = vec!["--markers"];
        a.extend(extra);
        a.push(pdf);
        let out = run(&t, &r, &a);
        assert_eq!(out.status.code(), Some(0), "{}", String::from_utf8_lossy(&out.stderr));
        String::from_utf8(out.stdout).unwrap().split('\u{c}').map(String::from).collect::<Vec<_>>()
    };
    assert!(!md(&[])[6].starts_with("![page 7 scan]"));
    assert!(!md(&["--scan-min-coverage", "0.1"])[6].starts_with("![page 7 scan]"), "too many words");
    assert!(!md(&["--scan-max-words", "100"])[6].starts_with("![page 7 scan]"), "too small");
    let both = md(&["--scan-min-coverage", "0.1", "--scan-max-words", "100"]);
    assert!(both[6].starts_with("![page 7 scan](images/page-0007/image-001."), "{}", both[6]);

    let plan = s.0.join("t.plan");
    let plan_s = plan.to_str().unwrap();
    let thresholds = ["--scan-min-coverage", "0.1", "--scan-max-words", "100", "--scan-layer-overlap", "0.4"];
    let mut a = vec!["--markers", "--images-plan-out", plan_s];
    a.extend(thresholds);
    a.push(pdf);
    let one = run(&t, &r, &a);
    assert_eq!(one.status.code(), Some(0), "{}", String::from_utf8_lossy(&one.stderr));
    let extract = |k: usize, extra: &[&str]| {
        let d = s.0.join(format!("x-{k}"));
        let mut a = vec!["--images-plan", plan_s, "--images-dir", d.to_str().unwrap()];
        a.extend(extra);
        a.push(pdf);
        (run(&t, &r, &a), d)
    };
    let refusals: [&[&str]; 4] = [
        &[],
        &["--scan-min-coverage", "0.1", "--scan-max-words", "100"],
        &["--scan-min-coverage", "0.11", "--scan-max-words", "100", "--scan-layer-overlap", "0.4"],
        &["--scan-min-coverage", "0.1", "--scan-max-words", "101", "--scan-layer-overlap", "0.4"],
    ];
    for (k, extra) in refusals.iter().enumerate() {
        let (out, d) = extract(k, extra);
        let err = String::from_utf8_lossy(&out.stderr);
        assert_eq!(out.status.code(), Some(2), "{k}: {err}");
        assert!(err.contains("settings") && err.contains("--scan-layer-overlap"), "{k}: {err}");
        assert!(!d.exists(), "{k}");
    }
    // the same values, spelled otherwise and in another order
    let (out, d) =
        extract(9, &["--scan-layer-overlap", ".40", "--scan-max-words", "100", "--scan-min-coverage", "0.10"]);
    assert_eq!(out.status.code(), Some(0), "{}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(entry(&manifest(&d), "p7.1")["fate"], "page-scan");
}
