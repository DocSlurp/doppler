//! `pdftomarkdown --images-dir`: needs the DocSlurp pdftotext with `-images` (`$PDFTOTEXT` or
//! `../../build/utils/pdftotext`) and python3 for the stand-in renderer
//! (tests/fixtures/fake_pdfregions.py); skipped (passing) without them.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const PDFTOMARKDOWN: &str = env!("CARGO_BIN_EXE_pdftomarkdown");

fn pdftotext() -> Option<PathBuf> {
    let exe = match std::env::var_os("PDFTOTEXT").filter(|p| !p.is_empty()) {
        Some(p) => PathBuf::from(p),
        None => Path::new(env!("CARGO_MANIFEST_DIR")).join("../../build/utils/pdftotext"),
    };
    let usage = Command::new(&exe).arg("-h").output().ok()?;
    let usage = String::from_utf8_lossy(&usage.stderr);
    let python = Command::new("python3").arg("-c").arg("pass").status().is_ok_and(|s| s.success());
    (usage.contains("-compact-layout") && usage.contains("-images") && python).then_some(exe)
}

fn fake() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fake_pdfregions.py")
}

struct Scratch(PathBuf);

impl Scratch {
    fn new(tag: &str) -> Scratch {
        let d = std::env::temp_dir().join(format!("images-dir-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        Scratch(d)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Two pages of text, each with the same 200 x 150 pt picture (one image XObject), a 10 pt
/// icon, and on page 1 a star drawn with many diagonal strokes (a vector figure).
fn make_pdf() -> Vec<u8> {
    let text = |y: i32| {
        (0..6)
            .map(|i| {
                format!(
                    "BT /F1 11 Tf 72 {} Td (Line {i} of a paragraph about the pictures on this page.) Tj ET\n",
                    y - 14 * i
                )
            })
            .collect::<String>()
    };
    let mut star = String::from("1 w\n");
    for k in 0..36 {
        let a = k as f64 * 10.0_f64.to_radians();
        star += &format!("300 250 m {:.2} {:.2} l S\n", 300.0 + 90.0 * a.cos(), 250.0 + 90.0 * a.sin());
    }
    let page1 = format!("{}q 200 0 0 150 72 420 cm /Im1 Do Q\nq 10 0 0 10 400 700 cm /Im1 Do Q\n{star}", text(720));
    let page2 = format!("{}q 200 0 0 150 300 200 cm /Im1 Do Q\n", text(720));
    let img = "\u{ff}\u{0}\u{0}\u{0}\u{ff}\u{0}\u{0}\u{0}\u{ff}\u{ff}\u{ff}\u{0}";
    let img: Vec<u8> = img.chars().map(|c| c as u8).collect();
    let mut objects: Vec<Vec<u8>> = vec![
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R 4 0 R] /Count 2 >>".to_vec(),
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 5 0 R >> /XObject << /Im1 6 0 R >> >> /Contents 7 0 R >>".to_vec(),
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 5 0 R >> /XObject << /Im1 6 0 R >> >> /Contents 8 0 R >>".to_vec(),
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_vec(),
    ];
    let mut im = format!("<< /Type /XObject /Subtype /Image /Width 2 /Height 2 /ColorSpace /DeviceRGB /BitsPerComponent 8 /Length {} >>\nstream\n", img.len()).into_bytes();
    im.extend(&img);
    im.extend(b"\nendstream");
    objects.push(im);
    for c in [page1, page2] {
        objects.push(format!("<< /Length {} >>\nstream\n{c}endstream", c.len()).into_bytes());
    }
    let mut pdf = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::new();
    for (i, obj) in objects.iter().enumerate() {
        offsets.push(pdf.len());
        pdf.extend(format!("{} 0 obj\n", i + 1).as_bytes());
        pdf.extend(obj);
        pdf.extend(b"\nendobj\n");
    }
    let xref = pdf.len();
    let mut tail = format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1);
    for o in offsets {
        tail += &format!("{o:010} 00000 n \n");
    }
    tail += &format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objects.len() + 1);
    pdf.extend(tail.as_bytes());
    pdf
}

fn run(pdftotext: &Path, args: &[&str], mode: &str) -> Output {
    Command::new(PDFTOMARKDOWN)
        .args(args)
        .env("PDFTOTEXT", pdftotext)
        .env("PDFREGIONS", fake())
        .env("FAKE_PDFREGIONS_MODE", mode)
        .output()
        .unwrap()
}

fn setup(tag: &str) -> Option<(PathBuf, Scratch, String)> {
    let Some(p) = pdftotext() else {
        eprintln!("skipped: needs the DocSlurp pdftotext with -images and python3");
        return None;
    };
    let s = Scratch::new(tag);
    let pdf = s.0.join("doc.pdf");
    std::fs::write(&pdf, make_pdf()).unwrap();
    let pdf = pdf.to_str().unwrap().to_string();
    Some((p, s, pdf))
}

fn files(dir: &Path) -> Vec<String> {
    let mut v: Vec<String> =
        std::fs::read_dir(dir).unwrap().map(|e| e.unwrap().file_name().into_string().unwrap()).collect();
    v.sort();
    v
}

#[test]
fn extracts_files_and_a_deterministic_manifest_without_changing_the_markdown() {
    let Some((p, s, pdf)) = setup("main") else { return };
    let plain = run(&p, &["--markers", &pdf], "ok");
    assert_eq!(plain.status.code(), Some(0));
    let d1 = s.0.join("out1");
    let one = run(&p, &["--report", "--images-dir", d1.to_str().unwrap(), &pdf], "ok");
    let err = String::from_utf8_lossy(&one.stderr);
    assert_eq!(one.status.code(), Some(0), "{err}");
    assert_eq!(one.stdout, plain.stdout, "Markdown changed with --images-dir");
    assert_eq!(files(&d1), ["manifest.json", "p1.1.webp", "p1.g1.svg"], "{err}");
    let manifest = std::fs::read_to_string(d1.join("manifest.json")).unwrap();
    // the 10 pt icon is under 24 px at 150 dpi; page 2's picture repeats page 1's
    assert!(manifest.contains("\"skipped\":\"min_side\""), "{manifest}");
    assert!(manifest.contains("\"skipped\":\"repeat\"") && !manifest.contains("repeat_of"), "{manifest}");
    let m: serde_json::Value = serde_json::from_str(&manifest).unwrap();
    assert_eq!(m["totals"]["written"], 2);
    assert!(err.contains("\"images_written\":2") && err.contains("\"file\":\"p1.1.webp\""), "{err}");
    assert!(err.contains("\"images_confined\":true") && !err.contains("confined=1"), "{err}");
    let svg = std::fs::read_to_string(d1.join("p1.g1.svg")).unwrap();
    assert!(svg.starts_with("<svg xmlns=") && !svg.contains("<g>"), "{svg}");
    let webp = std::fs::read(d1.join("p1.1.webp")).unwrap();
    assert_eq!(&webp[..4], b"RIFF");
    // a second run writes the same bytes
    let d2 = s.0.join("out2");
    assert_eq!(run(&p, &["--images-dir", d2.to_str().unwrap(), &pdf], "ok").status.code(), Some(0));
    for f in files(&d1) {
        assert_eq!(std::fs::read(d1.join(&f)).unwrap(), std::fs::read(d2.join(&f)).unwrap(), "{f}");
    }
    // link: the repeat points to the first file; keep: it gets its own
    let d3 = s.0.join("out3");
    assert_eq!(
        run(&p, &["--images-dir", d3.to_str().unwrap(), "--images-repeats", "link", &pdf], "ok").status.code(),
        Some(0)
    );
    let m3: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(d3.join("manifest.json")).unwrap()).unwrap();
    let p2 = m3["images"].as_array().unwrap().iter().find(|e| e["id"] == "p2.1").unwrap().clone();
    assert_eq!((p2["file"].is_null(), p2["repeat_of"].as_str(), p2["bytes"].as_u64()), (true, Some("p1.1"), Some(0)));
    assert!(!files(&d3).contains(&"p2.1.webp".to_string()));
    let d4 = s.0.join("out4");
    assert_eq!(
        run(&p, &["--images-dir", d4.to_str().unwrap(), "--images-repeats", "keep", &pdf], "ok").status.code(),
        Some(0)
    );
    assert!(files(&d4).contains(&"p2.1.webp".to_string()));
    // a smaller floor in points keeps the icon
    let d5 = s.0.join("out5");
    assert_eq!(
        run(
            &p,
            &["--images-dir", d5.to_str().unwrap(), "--images-min-side", "5pt", "--raster-format", "png", &pdf],
            "ok"
        )
        .status
        .code(),
        Some(0)
    );
    assert!(files(&d5).contains(&"p1.1.png".to_string()), "{:?}", files(&d5));
}

#[test]
fn refuses_existing_and_symlinked_folders_before_running() {
    let Some((p, s, pdf)) = setup("refuse") else { return };
    let exists = s.0.join("exists");
    std::fs::create_dir(&exists).unwrap();
    let r = run(&p, &["--images-dir", exists.to_str().unwrap(), &pdf], "ok");
    assert_eq!(r.status.code(), Some(2));
    assert!(r.stdout.is_empty());
    let target = s.0.join("target");
    std::fs::create_dir(&target).unwrap();
    let link = s.0.join("link");
    std::os::unix::fs::symlink(&target, &link).unwrap();
    // the folder itself, and a parent, as a symbolic link
    for dir in [link.clone(), link.join("out")] {
        let r = run(&p, &["--images-dir", dir.to_str().unwrap(), &pdf], "ok");
        assert_eq!(r.status.code(), Some(2), "{}", String::from_utf8_lossy(&r.stderr));
    }
    assert!(files(&target).is_empty());
    // bad values exit 2 before pdftotext runs
    let r = run(&p, &["--images-dir", s.0.join("x").to_str().unwrap(), "--images-dpi", "9000", &pdf], "ok");
    assert_eq!(r.status.code(), Some(2));
    assert!(!s.0.join("x").exists());
}

#[test]
fn a_failing_renderer_removes_the_folder_and_keeps_the_markdown() {
    let Some((p, s, pdf)) = setup("fail") else { return };
    let plain = run(&p, &["--markers", &pdf], "ok");
    for (mode, extra) in [("crash", vec![]), ("hang", vec!["--images-timeout", "1"]), ("ok", vec!["--images-max", "1"])]
    {
        let d = s.0.join(format!("out-{mode}"));
        let mut args = vec!["--report", "--images-dir", d.to_str().unwrap()];
        args.extend(extra);
        args.push(&pdf);
        let r = run(&p, &args, mode);
        let err = String::from_utf8_lossy(&r.stderr);
        assert_eq!(r.status.code(), Some(5), "{mode}: {err}");
        assert_eq!(r.stdout, plain.stdout, "{mode}");
        assert!(!d.exists(), "{mode}");
        assert!(err.contains("\"images_error\""), "{err}");
    }
}

#[test]
fn bad_records_and_refused_svg_are_recorded_per_image() {
    let Some((p, s, pdf)) = setup("records") else { return };
    let d = s.0.join("mismatch");
    let r = run(&p, &["--images-dir", d.to_str().unwrap(), &pdf], "mismatch");
    assert_eq!(r.status.code(), Some(0), "{}", String::from_utf8_lossy(&r.stderr));
    let m = std::fs::read_to_string(d.join("manifest.json")).unwrap();
    assert!(m.contains("\"error\":\"mismatch\""), "{m}");
    let d = s.0.join("script");
    let r = run(&p, &["--images-dir", d.to_str().unwrap(), &pdf], "script");
    assert_eq!(r.status.code(), Some(0), "{}", String::from_utf8_lossy(&r.stderr));
    let m = std::fs::read_to_string(d.join("manifest.json")).unwrap();
    assert!(m.contains("\"fallback\":\"svg_allowlist\"") && files(&d).contains(&"p1.g1.webp".to_string()), "{m}");
    // a file over --images-max-file-bytes is not written; the rest go on
    let d = s.0.join("filecap");
    let r = run(&p, &["--images-dir", d.to_str().unwrap(), "--images-max-file-bytes", "100", &pdf], "ok");
    assert_eq!(r.status.code(), Some(0), "{}", String::from_utf8_lossy(&r.stderr));
    let m = std::fs::read_to_string(d.join("manifest.json")).unwrap();
    assert!(m.contains("\"error\":\"file_bytes\""), "{m}");
}

/// The real pdfregions (`$PDFREGIONS_REAL`, else `../../build/utils/pdfregions`).
#[test]
fn the_real_renderer_extracts_the_pictures_and_figures() {
    let Some((p, s, pdf)) = setup("real") else { return };
    let real = std::env::var_os("PDFREGIONS_REAL")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../build/utils/pdfregions"));
    if !real.is_file() {
        eprintln!("skipped: no real pdfregions");
        return;
    }
    let plain = run(&p, &["--markers", &pdf], "ok");
    let d = s.0.join("out");
    let r = Command::new(PDFTOMARKDOWN)
        .args(["--report", "--images-dir", d.to_str().unwrap(), &pdf])
        .env("PDFTOTEXT", &p)
        .env("PDFREGIONS", &real)
        .output()
        .unwrap();
    let err = String::from_utf8_lossy(&r.stderr);
    assert_eq!(r.status.code(), Some(0), "{err}");
    assert_eq!(r.stdout, plain.stdout);
    let m = std::fs::read_to_string(d.join("manifest.json")).unwrap();
    assert!(files(&d).contains(&"p1.1.webp".to_string()) && files(&d).contains(&"p1.g1.svg".to_string()), "{m}\n{err}");
    assert!(!m.contains("\"error\""), "{m}");
    assert!(err.contains("\"images_confined\""), "{err}");
    let svg = std::fs::read_to_string(d.join("p1.g1.svg")).unwrap();
    assert!(svg.contains("<path"), "{svg}");
}
