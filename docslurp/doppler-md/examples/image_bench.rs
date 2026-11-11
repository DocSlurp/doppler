//! Measures image extraction's encoders (README, "Extracting images and figures"):
//!
//!     cargo run --release --example image_bench -- svg FIGS.json DIR OUT [PRECISION]
//!     cargo run --release --example image_bench -- raster OUT CROP.png...
//!
//! `svg`: FIGS.json is `[["stem", [x0, y0, x1, y1]], ...]`, each `DIR/<basename(stem)>.raw.svg`
//! a cairo SVG. The root is sized to the box (as pdfregions writes it), then compacted; both
//! are rendered with `rsvg-convert` at 300 dpi and compared pixel by pixel. With `IMAGE_BENCH_PAGE=1`
//! the SVG keeps its own canvas (when the boxes do not match the drawing). Prints bytes, CPU
//! time per figure, the inked pixels (darker than 250 in the original) and the pixels that
//! changed. `raster`: each crop is
//! encoded as lossy WebP (q90), lossless WebP and PNG; prints bytes and CPU time.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

use doppler_md::extract::encode::{Raster, encode};
use doppler_md::extract::svg::compact;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("svg") if args.len() >= 4 => svg(&args[1], Path::new(&args[2]), Path::new(&args[3]), args.get(4)),
        Some("raster") if args.len() >= 3 => raster(Path::new(&args[1]), &args[2..]),
        _ => {
            eprintln!("usage: image_bench svg FIGS.json DIR OUT [PRECISION] | raster OUT CROP.png...");
            std::process::exit(2);
        }
    }
}

/// Sets the root's width, height and viewBox to the box, as pdfregions does.
fn sized(svg: &str, b: [f64; 4]) -> String {
    let start = svg.find("<svg").expect("svg root");
    let end = start + svg[start..].find('>').expect("svg root end");
    let mut head = svg[start..end].to_string();
    for key in ["width", "height", "viewBox"] {
        if let Some(at) = head.find(&format!(" {key}=\"")) {
            let vstart = at + key.len() + 3;
            let vend = vstart + head[vstart..].find('"').unwrap();
            head.replace_range(at..vend + 1, "");
        }
    }
    let (w, h) = (b[2] - b[0], b[3] - b[1]);
    head.push_str(&format!(" width=\"{w}pt\" height=\"{h}pt\" viewBox=\"{} {} {w} {h}\"", b[0], b[1]));
    format!("{}{}{}", &svg[..start], head, &svg[end..])
}

fn render(svg: &Path, png: &Path) -> Option<(u32, u32, Vec<u8>)> {
    let ok = Command::new("rsvg-convert")
        .args(["--dpi-x", "300", "--dpi-y", "300", "-b", "white", "-o"])
        .arg(png)
        .arg(svg)
        .status()
        .ok()?
        .success();
    if !ok {
        return None;
    }
    let dec = png::Decoder::new(std::io::BufReader::new(std::fs::File::open(png).ok()?));
    let mut r = dec.read_info().ok()?;
    let mut buf = vec![0; r.output_buffer_size()?];
    let info = r.next_frame(&mut buf).ok()?;
    buf.truncate(info.buffer_size());
    let rgb = to_rgb(&buf, info.color_type);
    Some((info.width, info.height, rgb))
}

fn to_rgb(buf: &[u8], ct: png::ColorType) -> Vec<u8> {
    match ct {
        png::ColorType::Rgb => buf.to_vec(),
        png::ColorType::Rgba => buf.chunks(4).flat_map(|p| [p[0], p[1], p[2]]).collect(),
        png::ColorType::Grayscale => buf.iter().flat_map(|&g| [g, g, g]).collect(),
        png::ColorType::GrayscaleAlpha => buf.chunks(2).flat_map(|p| [p[0], p[0], p[0]]).collect(),
        png::ColorType::Indexed => panic!("indexed PNG"),
    }
}

fn svg(figs: &str, dir: &Path, out: &Path, precision: Option<&String>) {
    let precision: u8 = precision.map(|p| p.parse().unwrap()).unwrap_or(1);
    std::fs::create_dir_all(out).unwrap();
    let list: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(figs).unwrap()).unwrap();
    let (mut raw_total, mut out_total, mut cpu_total) = (0usize, 0usize, 0f64);
    let (mut px_total, mut changed_total, mut changed16_total) = (0u64, 0u64, 0u64);
    println!("figure\tcairo_bytes\tcompact_bytes\tratio\tms\tink_pixels\tchanged\tchanged>16\tmax_diff");
    for f in list.as_array().unwrap() {
        let stem = f[0].as_str().unwrap();
        let name = Path::new(stem).file_name().unwrap().to_str().unwrap().to_string();
        let b: Vec<f64> = f[1].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
        let raw = std::fs::read_to_string(dir.join(format!("{name}.raw.svg"))).unwrap();
        let orig =
            if std::env::var_os("IMAGE_BENCH_PAGE").is_some() { raw } else { sized(&raw, [b[0], b[1], b[2], b[3]]) };
        let t = Instant::now();
        let reps = 5;
        let mut small = String::new();
        for _ in 0..reps {
            small = match compact(&orig, precision) {
                Ok(s) => s,
                Err(e) => {
                    println!("{name}\tREJECTED: {e}");
                    break;
                }
            };
        }
        if small.is_empty() {
            continue;
        }
        let ms = t.elapsed().as_secs_f64() * 1000.0 / reps as f64;
        let (po, pc): (PathBuf, PathBuf) =
            (out.join(format!("{name}.orig.svg")), out.join(format!("{name}.p{precision}.svg")));
        std::fs::write(&po, &orig).unwrap();
        std::fs::write(&pc, &small).unwrap();
        let ro = render(&po, &out.join(format!("{name}.orig.png")));
        let rc = render(&pc, &out.join(format!("{name}.p{precision}.png")));
        let (pixels, changed, changed16, maxd) = match (ro, rc) {
            (Some((w1, h1, a)), Some((w2, h2, c))) if (w1, h1) == (w2, h2) => {
                let mut ch = 0u64;
                let mut ch16 = 0u64;
                let mut maxd = 0u8;
                for (p, q) in a.chunks(3).zip(c.chunks(3)) {
                    let d = p.iter().zip(q).map(|(x, y)| x.abs_diff(*y)).max().unwrap();
                    maxd = maxd.max(d);
                    if d > 0 {
                        ch += 1;
                    }
                    if d > 16 {
                        ch16 += 1;
                    }
                }
                let ink = a.chunks(3).filter(|p| p.iter().any(|&v| v < 250)).count() as u64;
                (ink, ch, ch16, maxd)
            }
            _ => {
                println!("{name}\trender failed or size differs");
                continue;
            }
        };
        raw_total += orig.len();
        out_total += small.len();
        cpu_total += ms;
        px_total += pixels;
        changed_total += changed;
        changed16_total += changed16;
        println!(
            "{name}\t{}\t{}\t{:.1}%\t{ms:.2}\t{pixels}\t{changed}\t{changed16}\t{maxd}",
            orig.len(),
            small.len(),
            100.0 * small.len() as f64 / orig.len() as f64
        );
    }
    println!(
        "TOTAL\t{raw_total}\t{out_total}\t{:.1}%\t{cpu_total:.1} ms\t{px_total}\t{changed_total} ({:.3}% of ink)\t{changed16_total} ({:.4}% of ink)",
        100.0 * out_total as f64 / raw_total as f64,
        100.0 * changed_total as f64 / px_total as f64,
        100.0 * changed16_total as f64 / px_total as f64
    );
}

fn raster(out: &Path, crops: &[String]) {
    std::fs::create_dir_all(out).unwrap();
    let formats = [("webp_q90", Raster::WebpLossy(90)), ("webp_lossless", Raster::WebpLossless), ("png", Raster::Png)];
    let mut totals = [(0usize, 0f64); 3];
    let mut src_total = 0usize;
    let mut px_total = 0u64;
    println!("crop\tpixels\tsource_png\twebp_q90\tms\twebp_lossless\tms\tpng\tms");
    for c in crops {
        let dec = png::Decoder::new(std::io::BufReader::new(std::fs::File::open(c).unwrap()));
        let mut r = dec.read_info().unwrap();
        let mut buf = vec![0; r.output_buffer_size().unwrap()];
        let info = r.next_frame(&mut buf).unwrap();
        if info.bit_depth != png::BitDepth::Eight || info.color_type == png::ColorType::Indexed {
            continue;
        }
        buf.truncate(info.buffer_size());
        let rgb = to_rgb(&buf, info.color_type);
        let (w, h) = (info.width, info.height);
        let src = std::fs::metadata(c).unwrap().len() as usize;
        src_total += src;
        px_total += u64::from(w) * u64::from(h);
        let mut line = format!("{}\t{}\t{src}", Path::new(c).file_name().unwrap().to_string_lossy(), w * h);
        for (k, (_, f)) in formats.iter().enumerate() {
            let t = Instant::now();
            let data = encode(&rgb, w, h, *f).unwrap();
            let ms = t.elapsed().as_secs_f64() * 1000.0;
            totals[k].0 += data.len();
            totals[k].1 += ms;
            line += &format!("\t{}\t{ms:.1}", data.len());
        }
        println!("{line}");
    }
    let n = crops.len().max(1) as f64;
    println!("TOTAL crops={} pixels={px_total} source_png={src_total}", crops.len());
    for (k, (name, _)) in formats.iter().enumerate() {
        println!(
            "{name}: {} bytes ({:.1}% of PNG), {:.1} ms total, {:.2} ms/crop, {:.1} Mpx/s",
            totals[k].0,
            100.0 * totals[k].0 as f64 / totals[2].0 as f64,
            totals[k].1,
            totals[k].1 / n,
            px_total as f64 / 1e3 / totals[k].1
        );
    }
}
