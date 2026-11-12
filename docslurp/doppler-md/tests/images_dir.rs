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

/// Every file under `dir`, as paths relative to it, sorted.
fn files(dir: &Path) -> Vec<String> {
    fn walk(base: &Path, dir: &Path, out: &mut Vec<String>) {
        for e in std::fs::read_dir(dir).unwrap() {
            let e = e.unwrap();
            let p = e.path();
            if e.file_type().unwrap().is_dir() {
                walk(base, &p, out);
            } else {
                out.push(p.strip_prefix(base).unwrap().to_str().unwrap().to_string());
            }
        }
    }
    let mut v = Vec::new();
    walk(dir, dir, &mut v);
    v.sort();
    v
}

fn manifest(dir: &Path) -> serde_json::Value {
    serde_json::from_str(&std::fs::read_to_string(dir.join("manifest.json")).unwrap()).unwrap()
}

fn entry(m: &serde_json::Value, id: &str) -> serde_json::Value {
    m["images"].as_array().unwrap().iter().find(|e| e["id"] == id).unwrap().clone()
}

const PICTURE: &str = "page-0001/image-001.webp";
const FIGURE: &str = "page-0001/figure-001.svg";

#[test]
fn extracts_files_where_the_links_point_without_changing_the_markdown() {
    let Some((p, s, pdf)) = setup("main") else { return };
    let plain = run(&p, &["--markers", &pdf], "ok");
    assert_eq!(plain.status.code(), Some(0));
    let md = String::from_utf8(plain.stdout.clone()).unwrap();
    // the links are the files' paths; page 2's picture repeats page 1's (one image object)
    assert!(md.contains(&format!("![image](images/{PICTURE})")), "{md}");
    assert!(md.contains(&format!("![image](images/{FIGURE})")), "{md}");
    assert!(md.contains("![image](images/page-0001/image-002.webp)"), "{md}");
    assert_eq!(md.split('\u{c}').nth(1).unwrap().matches(&format!("](images/{PICTURE})")).count(), 1, "{md}");
    let d1 = s.0.join("out1");
    let one = run(&p, &["--report", "--images-dir", d1.to_str().unwrap(), &pdf], "ok");
    let err = String::from_utf8_lossy(&one.stderr);
    assert_eq!(one.status.code(), Some(0), "{err}");
    assert_eq!(one.stdout, plain.stdout, "Markdown changed with --images-dir");
    assert_eq!(files(&d1), ["manifest.json", FIGURE, PICTURE], "{err}");
    let m = manifest(&d1);
    assert_eq!((m["version"].as_u64(), m["complete"].as_bool()), (Some(2), Some(true)));
    // the 10 pt icon is under 24 px at 150 dpi: linked, not extracted
    let icon = entry(&m, "p1.2");
    assert_eq!((icon["status"].as_str(), icon["reason"].as_str()), (Some("not-extracted"), Some("min_side")));
    assert_eq!(icon["link"], "images/page-0001/image-002.webp");
    let p2 = entry(&m, "p2.1");
    assert_eq!(
        (p2["status"].as_str(), p2["reason"].as_str(), p2["repeat_of"].as_str(), p2["path"].as_str()),
        (Some("not-extracted"), Some("repeat"), Some("p1.1"), Some(PICTURE))
    );
    assert_eq!(m["totals"]["extracted"], 2);
    assert!(err.contains("\"images_written\":2") && err.contains(&format!("\"file\":\"{PICTURE}\"")), "{err}");
    assert!(err.contains("\"images_confined\":true") && !err.contains("confined=1"), "{err}");
    assert!(err.contains("\"images_complete\":true") && err.contains("\"images_ms\":{\"render\":"), "{err}");
    let svg = std::fs::read_to_string(d1.join(FIGURE)).unwrap();
    assert!(svg.starts_with("<svg xmlns=") && !svg.contains("<g>"), "{svg}");
    let webp = std::fs::read(d1.join(PICTURE)).unwrap();
    assert_eq!(&webp[..4], b"RIFF");
    // private: 0700 folders, 0600 files, whatever the umask
    use std::os::unix::fs::PermissionsExt;
    let mode = |q: &Path| std::fs::metadata(q).unwrap().permissions().mode() & 0o777;
    assert_eq!((mode(&d1), mode(&d1.join("page-0001"))), (0o700, 0o700));
    for f in files(&d1) {
        assert_eq!(mode(&d1.join(&f)), 0o600, "{f}");
    }
    // a second run writes the same bytes
    let d2 = s.0.join("out2");
    assert_eq!(run(&p, &["--images-dir", d2.to_str().unwrap(), &pdf], "ok").status.code(), Some(0));
    for f in files(&d1) {
        assert_eq!(std::fs::read(d1.join(&f)).unwrap(), std::fs::read(d2.join(&f)).unwrap(), "{f}");
    }
    // link: as skip (the manifest names the first placement either way)
    let d3 = s.0.join("out3");
    let r3 = run(&p, &["--images-dir", d3.to_str().unwrap(), "--images-repeats", "link", &pdf], "ok");
    assert_eq!(r3.status.code(), Some(0));
    assert_eq!(r3.stdout, plain.stdout);
    assert_eq!(entry(&manifest(&d3), "p2.1")["repeat_of"], "p1.1");
    // keep: the repeat has a link and a file of its own
    let d4 = s.0.join("out4");
    let r4 = run(&p, &["--images-dir", d4.to_str().unwrap(), "--images-repeats", "keep", &pdf], "ok");
    assert_eq!(r4.status.code(), Some(0));
    assert!(files(&d4).contains(&"page-0002/image-001.webp".to_string()));
    assert!(String::from_utf8_lossy(&r4.stdout).contains("](images/page-0002/image-001.webp)"));
    assert_eq!(r4.stdout, run(&p, &["--markers", "--images-repeats", "keep", &pdf], "ok").stdout);
    // a smaller floor in points keeps the icon; PNG pictures are .png links and files
    let d5 = s.0.join("out5");
    let r5 = run(
        &p,
        &["--images-dir", d5.to_str().unwrap(), "--images-min-side", "5pt", "--raster-format", "png", &pdf],
        "ok",
    );
    assert_eq!(r5.status.code(), Some(0));
    assert!(files(&d5).contains(&"page-0001/image-002.png".to_string()), "{:?}", files(&d5));
    assert!(String::from_utf8_lossy(&r5.stdout).contains("](images/page-0001/image-001.png)"));
}

/// The manifest, field by field: every link of the Markdown has its entry, every file in the
/// folder is listed, and the hashes are the files'.
#[test]
fn the_manifest_lists_every_link_and_every_file() {
    let Some((p, s, pdf)) = setup("golden") else { return };
    let d = s.0.join("out");
    let r = run(&p, &["--images-dir", d.to_str().unwrap(), &pdf], "ok");
    assert_eq!(r.status.code(), Some(0), "{}", String::from_utf8_lossy(&r.stderr));
    let sha = |bytes: &[u8]| -> String {
        use sha2::Digest;
        sha2::Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
    };
    let m = manifest(&d);
    let text = std::fs::read_to_string(d.join("manifest.json")).unwrap();
    let source = sha(&std::fs::read(&pdf).unwrap());
    let head = format!(
        "{{\"version\":2,\"source_sha256\":\"{source}\",\"link_prefix\":\"images/\",\"dpi\":150,\"min_side_pt\":11.52,\"repeats\":\"skip\",\"scan_pages\":\"image\",\"scan_dpi\":150,\"scan_max_dpi\":300,\"batch_pages\":64,\"complete\":true,\n\"batches\":[\n{{\"pages\":[1,64],\"status\":\"ok\"}}\n],\n\"images\":["
    );
    assert!(text.starts_with(&head), "{text}");
    let file = |f: &str| std::fs::read(d.join(f)).unwrap();
    let webp = file(PICTURE);
    let svg = file(FIGURE);
    let lines = [
        format!(
            "{{\"id\":\"p1.1\",\"page\":1,\"bbox\":[72.00,222.00,272.00,372.00],\"kind\":\"picture\",\"fate\":\"block\",\"obj\":[6,0],\"path\":\"{PICTURE}\",\"link\":\"images/{PICTURE}\",\"status\":\"extracted\",\"format\":\"webp\",\"source\":\"render\",\"bytes\":{},\"sha256\":\"{}\",\"width\":417,\"height\":312}}",
            webp.len(),
            sha(&webp)
        ),
        "{\"id\":\"p1.2\",\"page\":1,\"bbox\":[400.00,82.00,410.00,92.00],\"kind\":\"picture\",\"fate\":\"block\",\"obj\":[6,0],\"path\":\"page-0001/image-002.webp\",\"link\":\"images/page-0001/image-002.webp\",\"status\":\"not-extracted\",\"reason\":\"min_side\",\"format\":\"webp\",\"source\":null,\"bytes\":0,\"sha256\":null,\"width\":0,\"height\":0}".to_string(),
        format!(
            "{{\"id\":\"p1.g1\",\"page\":1,\"bbox\":[210.00,452.00,390.00,632.00],\"kind\":\"vector\",\"fate\":\"block\",\"obj\":null,\"path\":\"{FIGURE}\",\"link\":\"images/{FIGURE}\",\"status\":\"extracted\",\"format\":\"svg\",\"source\":\"render\",\"bytes\":{},\"sha256\":\"{}\",\"width\":180,\"height\":180}}",
            svg.len(),
            sha(&svg)
        ),
        format!(
            "{{\"id\":\"p2.1\",\"page\":2,\"bbox\":[300.00,442.00,500.00,592.00],\"kind\":\"picture\",\"fate\":\"block\",\"obj\":[6,0],\"path\":\"{PICTURE}\",\"link\":\"images/{PICTURE}\",\"status\":\"not-extracted\",\"reason\":\"repeat\",\"repeat_of\":\"p1.1\",\"format\":\"webp\",\"source\":null,\"bytes\":0,\"sha256\":null,\"width\":0,\"height\":0}}"
        ),
    ];
    let body: Vec<&str> = text[head.len()..].lines().take(lines.len() + 1).collect();
    assert_eq!(body[0], "", "{text}");
    for (k, want) in lines.iter().enumerate() {
        assert_eq!(body[k + 1].trim_end_matches(','), want, "line {k}");
    }
    assert!(
        text.ends_with(&format!(
            "\n],\n\"totals\":{{\"images\":4,\"linked\":4,\"extracted\":2,\"not_extracted\":2,\"unlinked\":0,\"bytes\":{},\"repeats\":1,\"fallbacks\":0,\"scans\":0,\"scan_bytes\":0,\"batches\":1,\"batches_cut\":0}}}}\n",
            webp.len() + svg.len()
        )),
        "{text}"
    );
    // every link of the Markdown has one entry (by id), every file one extracted entry
    let md = String::from_utf8(r.stdout).unwrap();
    let links: Vec<&str> = md.split("](").skip(1).map(|t| &t[..t.find(')').unwrap()]).collect();
    assert_eq!(links.len(), 4, "{md}");
    let all = m["images"].as_array().unwrap();
    for l in &links {
        assert!(all.iter().any(|e| e["link"] == *l), "{l}");
    }
    for f in files(&d).iter().filter(|f| *f != "manifest.json") {
        assert_eq!(all.iter().filter(|e| e["status"] == "extracted" && e["path"] == f.as_str()).count(), 1, "{f}");
    }
}

#[test]
fn link_prefixes_and_formats_shape_the_links_with_or_without_extraction() {
    let Some((p, s, pdf)) = setup("prefix") else { return };
    for (k, (args, picture, figure)) in [
        (vec!["--images-link-prefix", ""], "page-0001/image-001.webp", "page-0001/figure-001.svg"),
        (vec!["--images-link-prefix", "a/b/"], "a/b/page-0001/image-001.webp", "a/b/page-0001/figure-001.svg"),
        (
            vec!["--raster-format", "png", "--vector-format", "webp"],
            "images/page-0001/image-001.png",
            "images/page-0001/figure-001.webp",
        ),
        (vec!["--vector-format", "png"], "images/page-0001/image-001.webp", "images/page-0001/figure-001.png"),
    ]
    .into_iter()
    .enumerate()
    {
        let mut a = vec!["--markers"];
        a.extend(&args);
        a.push(&pdf);
        let plain = run(&p, &a, "ok");
        assert_eq!(plain.status.code(), Some(0), "{k}: {}", String::from_utf8_lossy(&plain.stderr));
        let md = String::from_utf8_lossy(&plain.stdout).to_string();
        assert!(
            md.contains(&format!("![image]({picture})")) && md.contains(&format!("![image]({figure})")),
            "{k}: {md}"
        );
        let d = s.0.join(format!("out{k}"));
        let mut a = vec!["--images-dir", d.to_str().unwrap()];
        a.extend(&args);
        a.push(&pdf);
        let r = run(&p, &a, "ok");
        assert_eq!(r.status.code(), Some(0), "{k}: {}", String::from_utf8_lossy(&r.stderr));
        assert_eq!(r.stdout, plain.stdout, "{k}");
        let prefix = &picture[..picture.find("page-").unwrap()];
        for f in [picture, figure] {
            assert!(d.join(&f[prefix.len()..]).is_file(), "{k}: {f} {:?}", files(&d));
        }
        assert_eq!(manifest(&d)["link_prefix"], prefix);
    }
    // bad prefixes, and link options without markers: exit 2 before anything runs
    for bad in ["../x/", "/abs/", "x", "a b/", "a)b/", "a%b/", "http://x/", &format!("{}/", "a".repeat(128))] {
        let r = run(&p, &["--markers", "--images-link-prefix", bad, &pdf], "ok");
        assert_eq!(r.status.code(), Some(2), "{bad:?}");
        assert!(r.stdout.is_empty());
    }
    let r = run(&p, &["--images-link-prefix", "a/", &pdf], "ok");
    assert_eq!(r.status.code(), Some(2));
    // the plan binds the link options
    let plan = s.0.join("p.plan");
    let one =
        run(&p, &["--markers", "--images-link-prefix", "a/", "--images-plan-out", plan.to_str().unwrap(), &pdf], "ok");
    assert_eq!(one.status.code(), Some(0));
    let d = s.0.join("planned");
    let two = run(&p, &["--images-plan", plan.to_str().unwrap(), "--images-dir", d.to_str().unwrap(), &pdf], "ok");
    assert_eq!(two.status.code(), Some(2), "{}", String::from_utf8_lossy(&two.stderr));
    let d = s.0.join("planned2");
    let mut a = vec!["--images-link-prefix", "a/", "--images-plan", plan.to_str().unwrap()];
    a.extend(["--images-dir", d.to_str().unwrap(), &pdf]);
    assert_eq!(run(&p, &a, "ok").status.code(), Some(0));
    assert_eq!(manifest(&d)["link_prefix"], "a/");
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

/// A renderer that crashes, hangs or a batch over its budget: the Markdown, the files written
/// and the manifest stay (exit 5); a renderer that cannot start fails the extraction (exit 6,
/// DIR removed).
#[test]
fn a_failing_renderer_keeps_what_was_written() {
    let Some((p, s, pdf)) = setup("fail") else { return };
    let plain = run(&p, &["--markers", &pdf], "ok");
    for (mode, extra, status, written) in [
        ("crash", vec![], "renderer", 1),
        ("hang", vec!["--images-timeout", "1"], "timeout", 0),
        ("ok", vec!["--images-max", "1"], "budget", 1),
    ] {
        let d = s.0.join(format!("out-{mode}"));
        let mut args = vec!["--report", "--images-dir", d.to_str().unwrap()];
        args.extend(extra);
        args.push(&pdf);
        let r = run(&p, &args, mode);
        let err = String::from_utf8_lossy(&r.stderr);
        assert_eq!(r.status.code(), Some(5), "{mode}: {err}");
        assert_eq!(r.stdout, plain.stdout, "{mode}");
        let m = manifest(&d);
        assert_eq!(m["complete"], false, "{mode}");
        assert_eq!(m["batches"][0]["status"], status, "{mode}: {m}");
        assert_eq!(m["batches"][0]["cut_page"], 1, "{mode}: {m}");
        assert_eq!(m["totals"]["extracted"], written, "{mode}: {m}");
        assert_eq!(files(&d).len(), written + 1, "{mode}");
        assert!(m["images"].as_array().unwrap().iter().any(|e| e["reason"] == status), "{mode}: {m}");
        assert!(err.contains("\"images_complete\":false") && !err.contains("\"images_error\""), "{err}");
    }
    // cannot start: hard failure
    let d = s.0.join("out-missing");
    let r = Command::new(PDFTOMARKDOWN)
        .args(["--report", "--images-dir", d.to_str().unwrap(), &pdf])
        .env("PDFTOTEXT", &p)
        .env("PDFREGIONS", s.0.join("no-such-renderer"))
        .output()
        .unwrap();
    let err = String::from_utf8_lossy(&r.stderr);
    assert_eq!(r.status.code(), Some(6), "{err}");
    assert_eq!(r.stdout, plain.stdout);
    assert!(!d.exists());
    assert!(err.contains("\"images_error\""), "{err}");
}

#[test]
fn bad_records_and_refused_svg_are_recorded_per_image() {
    let Some((p, s, pdf)) = setup("records") else { return };
    let d = s.0.join("mismatch");
    let r = run(&p, &["--images-dir", d.to_str().unwrap(), &pdf], "mismatch");
    assert_eq!(r.status.code(), Some(0), "{}", String::from_utf8_lossy(&r.stderr));
    let p1 = entry(&manifest(&d), "p1.1");
    assert_eq!((p1["reason"].as_str(), p1["error"].as_str()), (Some("error"), Some("mismatch")));
    // a refused SVG: a lossless PNG wrapped in an SVG, at the figure's .svg link
    let d = s.0.join("script");
    let r = run(&p, &["--images-dir", d.to_str().unwrap(), &pdf], "script");
    assert_eq!(r.status.code(), Some(0), "{}", String::from_utf8_lossy(&r.stderr));
    let fig = entry(&manifest(&d), "p1.g1");
    assert_eq!((fig["fallback"].as_str(), fig["status"].as_str()), (Some("svg_allowlist"), Some("extracted")));
    let svg = std::fs::read_to_string(d.join(FIGURE)).unwrap();
    assert!(
        svg.starts_with("<svg xmlns=") && svg.contains("data:image/png;base64,") && !svg.contains("script"),
        "{svg}"
    );
    assert_eq!(files(&d), ["manifest.json", FIGURE, PICTURE]);
    // with --vector-format webp a refused figure is a lossless WebP, as linked
    let d = s.0.join("bigsvg-webp");
    let r = run(&p, &["--images-dir", d.to_str().unwrap(), "--vector-format", "webp", &pdf], "ok");
    assert_eq!(r.status.code(), Some(0));
    assert!(files(&d).contains(&"page-0001/figure-001.webp".to_string()));
    // a file over --images-max-file-bytes is not written; the rest go on
    let d = s.0.join("filecap");
    let r = run(&p, &["--images-dir", d.to_str().unwrap(), "--images-max-file-bytes", "100", &pdf], "ok");
    assert_eq!(r.status.code(), Some(0), "{}", String::from_utf8_lossy(&r.stderr));
    let m = manifest(&d);
    assert_eq!(entry(&m, "p1.1")["reason"], "file_bytes");
    // its repeat still names it: the caller sees that the shared file is missing
    assert_eq!(entry(&m, "p2.1")["repeat_of"], "p1.1");
    // pages: only page 2 is extracted; its picture repeats page 1's, whose file it writes
    let d = s.0.join("pages");
    let r = run(&p, &["--images-dir", d.to_str().unwrap(), "--images-pages", "2-2", &pdf], "ok");
    assert_eq!(r.status.code(), Some(0), "{}", String::from_utf8_lossy(&r.stderr));
    assert_eq!(files(&d), ["manifest.json", PICTURE]);
    let m = manifest(&d);
    assert_eq!(entry(&m, "p1.1")["reason"], "pages");
    let p2 = entry(&m, "p2.1");
    assert_eq!((p2["status"].as_str(), p2["path"].as_str()), (Some("extracted"), Some(PICTURE)), "{m}");
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
    assert!(files(&d).contains(&PICTURE.to_string()) && files(&d).contains(&FIGURE.to_string()), "{m}\n{err}");
    assert!(!m.contains("\"error\"") && m.contains("\"complete\":true"), "{m}");
    assert!(err.contains("\"images_confined\""), "{err}");
    let svg = std::fs::read_to_string(d.join(FIGURE)).unwrap();
    assert!(svg.contains("<path"), "{svg}");
}

fn hex(s: &str) -> String {
    s.bytes().map(|b| format!("{b:02x}")).collect()
}

/// Pass 1 writes a plan (`--images-plan-out`), pass 2 extracts from it (`--images-plan`): the
/// same files and manifest as extraction after a conversion. A plan is bound to its PDF, page
/// range, settings and version, and a tampered plan is refused: exit 2, nothing written.
#[test]
fn a_plan_extracts_the_same_files_and_is_bound_to_its_conversion() {
    let Some((p, s, pdf)) = setup("plan") else { return };
    let plan = s.0.join("doc.plan");
    let plan_s = plan.to_str().unwrap();
    let one = run(&p, &["--stream", "--html", "--markers", "--images-plan-out", plan_s, &pdf], "ok");
    assert_eq!(one.status.code(), Some(0), "{}", String::from_utf8_lossy(&one.stderr));
    assert_eq!(
        one.stdout,
        run(&p, &["--stream", "--html", "--markers", &pdf], "ok").stdout,
        "Markdown changed with --images-plan-out"
    );
    let text = std::fs::read_to_string(&plan).unwrap();
    assert!(text.starts_with("doppler-images-plan\t4\nsource\t"), "{text}");
    let direct = s.0.join("direct");
    assert_eq!(
        run(&p, &["--stream", "--html", "--images-dir", direct.to_str().unwrap(), &pdf], "ok").status.code(),
        Some(0)
    );
    let planned = s.0.join("planned");
    let two = run(
        &p,
        &["--stream", "--html", "--images-plan", plan_s, "--images-dir", planned.to_str().unwrap(), &pdf],
        "ok",
    );
    assert_eq!(two.status.code(), Some(0), "{}", String::from_utf8_lossy(&two.stderr));
    assert!(two.stdout.is_empty());
    assert_eq!(files(&direct), files(&planned));
    for f in files(&direct) {
        assert_eq!(std::fs::read(direct.join(&f)).unwrap(), std::fs::read(planned.join(&f)).unwrap(), "{f}");
    }

    // another PDF (one more byte), other settings or pages, a missing PDF
    let other = s.0.join("other.pdf");
    let mut bytes = std::fs::read(&pdf).unwrap();
    bytes.extend(b"%\n");
    std::fs::write(&other, bytes).unwrap();
    let other = other.to_str().unwrap();
    let missing = s.0.join("missing.pdf");
    let refused = |k: usize, args: &[&str], plan: &str, why: &str| {
        let d = s.0.join(format!("refused-{k}"));
        let mut a = args.to_vec();
        a.extend(["--images-plan", plan, "--images-dir", d.to_str().unwrap()]);
        let r = run(&p, &a, "ok");
        let err = String::from_utf8_lossy(&r.stderr);
        assert_eq!(r.status.code(), Some(2), "{k}: {err}");
        assert!(err.contains(why), "{k}: {err}");
        assert!(r.stdout.is_empty() && !d.exists(), "{k}");
    };
    refused(1, &["--stream", "--html", other], plan_s, "another PDF");
    refused(2, &["--html", &pdf], plan_s, "settings");
    refused(3, &["--stream", "--html", "--strict", &pdf], plan_s, "settings");
    refused(4, &["--stream", "--html", "--params", r#"{"max_page_work": 1000}"#, &pdf], plan_s, "settings");
    refused(5, &["--stream", "--html", "-f", "1", &pdf], plan_s, "pages");
    refused(6, &["--stream", "--html", missing.to_str().unwrap()], plan_s, "missing.pdf");

    // tampered plans
    let first_image = text.lines().nth(5).unwrap().to_string();
    let fields: Vec<&str> = first_image.split('\t').collect();
    let id = String::from_utf8(
        (0..fields[0].len()).step_by(2).map(|i| u8::from_str_radix(&fields[0][i..i + 2], 16).unwrap()).collect(),
    )
    .unwrap();
    let page = fields[1];
    let tampered = [
        // an id naming another page than its line's
        text.replacen(
            &first_image,
            &first_image.replacen(&fields[0], &hex(&id.replacen(&format!("p{page}."), "p999.", 1)), 1),
            1,
        ),
        // a NaN coordinate
        text.replacen(&first_image, &first_image.replacen(fields[2], "7ff8000000000000", 1), 1),
        // a repeated line
        text.replacen(&first_image, &format!("{first_image}\n{first_image}"), 1),
        // not an image id
        text.replacen(&first_image, &first_image.replacen(&fields[0], &hex("../x"), 1), 1),
        // the header of another document
        text.replacen(&text.lines().nth(1).unwrap().to_string(), &format!("source\t{}\t10", "0".repeat(64)), 1),
    ];
    for (k, t) in tampered.iter().enumerate() {
        assert_ne!(t, &text, "{k}");
        let f = s.0.join(format!("tampered-{k}.plan"));
        std::fs::write(&f, t).unwrap();
        refused(10 + k, &["--stream", "--html", &pdf], f.to_str().unwrap(), "--images-plan");
    }
}

/// `--images-plan-out` needs --markers (else the plan would be empty) and a FILE; its file must
/// be new and is reached without following symbolic links.
#[test]
fn plan_out_needs_markers_and_a_new_confined_file() {
    let Some((p, s, pdf)) = setup("planout") else { return };
    let plan = s.0.join("p.plan");
    let r = run(&p, &["--images-plan-out", plan.to_str().unwrap(), &pdf], "ok");
    assert_eq!(r.status.code(), Some(2));
    assert!(r.stdout.is_empty() && !plan.exists());
    let r = run(&p, &["--markers", "--images-plan-out", plan.to_str().unwrap(), "-"], "ok");
    assert_eq!(r.status.code(), Some(2));
    let target = s.0.join("target");
    std::fs::create_dir(&target).unwrap();
    std::os::unix::fs::symlink(&target, s.0.join("link")).unwrap();
    std::fs::write(&plan, b"keep").unwrap();
    for path in [s.0.join("link/p.plan"), s.0.join("link"), plan.clone(), s.0.join("absent/p.plan")] {
        let r = run(&p, &["--markers", "--images-plan-out", path.to_str().unwrap(), &pdf], "ok");
        assert_eq!(r.status.code(), Some(2), "{}: {}", path.display(), String::from_utf8_lossy(&r.stderr));
        assert!(r.stdout.is_empty());
    }
    assert!(files(&target).is_empty());
    assert_eq!(std::fs::read(&plan).unwrap(), b"keep");
    // with --images-dir, markers are implied
    let fresh = s.0.join("fresh.plan");
    let d = s.0.join("out");
    let r = run(&p, &["--images-dir", d.to_str().unwrap(), "--images-plan-out", fresh.to_str().unwrap(), &pdf], "ok");
    assert_eq!(r.status.code(), Some(0), "{}", String::from_utf8_lossy(&r.stderr));
    assert!(std::fs::read_to_string(&fresh).unwrap().lines().count() > 5);
    assert!(!files(&s.0).iter().any(|f| f.ends_with(".part")));
}
