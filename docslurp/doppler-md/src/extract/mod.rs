//! Image and graphic extraction for `pdftomarkdown --images-dir DIR`.
//!
//! After the conversion, the report's images say what became of every picture and vector
//! figure. The marked ones (`block`, `inline`) that are large enough and do not repeat an
//! earlier one are rendered by `pdfregions` (docslurp/extraction/PROTOCOL.md): pictures as
//! RGB crops at the extraction dpi, vector figures as SVG. Crops are encoded as WebP (lossy,
//! or lossless when they hold text) or PNG; SVG is checked against an allowlist and compacted,
//! and becomes a lossless raster when it fails or is too large. Files are named from the ids
//! only (`p3.1.webp`, `p3.g1.svg`); `manifest.json`, written last, lists every image.
//!
//! One image failing is recorded in the manifest. A document limit, the renderer failing, or
//! a write error fails the extraction: the caller removes the folder.

pub mod encode;
pub mod outdir;
pub mod regions;
pub mod settings;
pub mod svg;

use std::collections::HashMap;
use std::fmt::Write as _;
use std::time::{Duration, Instant};

use sha2::{Digest, Sha256};

use crate::Report;
use crate::images::{Fate, Note};
use encode::Raster;
use outdir::OutDir;
use regions::{Job, Mode, Outcome, Renderer};
use settings::{RasterFormat, Repeats, Settings, VectorFormat};

/// Extra time past `--images-timeout` before the renderer is killed (it stops itself at the
/// timeout, exit status 3).
const GRACE: Duration = Duration::from_secs(2);
/// Raw SVG larger than this is not compacted: the figure is rendered as a raster.
pub const MAX_SVG_INPUT: usize = 8 << 20;

/// One image of the manifest.
#[derive(Clone, Debug, Default)]
struct Entry {
    file: Option<String>,
    format: Option<&'static str>,
    bytes: u64,
    sha256: Option<String>,
    width: u32,
    height: u32,
    repeat_of: Option<String>,
    fallback: Option<String>,
    error: Option<String>,
    skipped: Option<&'static str>,
}

/// What makes two marked figures the same: box size (half points), pixel size and kind.
type RepeatKey = (i64, i64, u32, u32, &'static str);

fn kind(n: &Note) -> &'static str {
    if n.vector {
        "vector"
    } else if n.mask {
        "mask"
    } else {
        "picture"
    }
}

fn repeat_key(n: &Note) -> RepeatKey {
    let (x0, y0, x1, y1) = n.bbox;
    (((x1 - x0) * 2.0).round() as i64, ((y1 - y0) * 2.0).round() as i64, n.px.0, n.px.1, kind(n))
}

/// The page, vector flag and index of an id, for sorting.
fn id_order(id: &str) -> (u64, bool, u64) {
    let rest = id.trim_start_matches('p');
    let (page, n) = rest.split_once('.').unwrap_or((rest, "0"));
    let vector = n.starts_with('g');
    (page.parse().unwrap_or(0), vector, n.trim_start_matches('g').parse().unwrap_or(0))
}

struct Run<'a> {
    s: &'a Settings,
    notes: &'a [Note],
    entries: Vec<Entry>,
    dir: &'a mut OutDir,
    written: usize,
    total_bytes: u64,
    /// content repeats: (key, sha256 of the rendered payload) -> entry
    seen_payload: HashMap<(RepeatKey, [u8; 32]), usize>,
    /// figures whose SVG failed: rendered again as rasters
    fallbacks: Vec<(usize, String)>,
    /// past it, records are not encoded or compacted any more (`error: timeout`)
    deadline: Instant,
}

/// Extracts the images of `report` into `dir`, sets each note's `file` and the report's
/// `images_written`. `Err` is a document failure: the caller removes the folder.
pub fn extract(report: &mut Report, s: &Settings, renderer: &Renderer, dir: &mut OutDir) -> Result<usize, String> {
    let start = Instant::now();
    let deadline = start + s.timeout + GRACE;
    let notes = &report.images;
    let mut entries: Vec<Entry> = vec![Entry::default(); notes.len()];
    let min_side = s.min_side_pt();
    // first marked figure for each (key, object): later ones are repeats without rendering
    let mut seen_obj: HashMap<(RepeatKey, (u32, u32)), usize> = HashMap::new();
    let mut jobs: Vec<(Job, usize)> = Vec::new();
    for (i, n) in notes.iter().enumerate() {
        if !outdir::valid_id(&n.id) {
            entries[i].error = Some("id".into());
            continue;
        }
        if !matches!(n.fate, Fate::Block | Fate::Inline) {
            entries[i].skipped = Some("fate");
            continue;
        }
        let (x0, y0, x1, y1) = n.bbox;
        let finite = [x0, y0, x1, y1].iter().all(|v| v.is_finite());
        if !finite || x1 <= x0 || y1 <= y0 {
            entries[i].error = Some("bbox".into());
            continue;
        }
        if (x1 - x0).min(y1 - y0) < min_side {
            entries[i].skipped = Some("min_side");
            continue;
        }
        if s.repeats != Repeats::Keep
            && let Some(obj) = n.obj
        {
            let key = (repeat_key(n), obj);
            if let Some(&first) = seen_obj.get(&key) {
                entries[i].repeat_of = Some(notes[first].id.clone());
                entries[i].skipped = Some("repeat");
                continue;
            }
            seen_obj.insert(key, i);
        }
        let mode = if n.vector && s.vector_format == VectorFormat::Svg { Mode::Svg } else { Mode::Raster };
        jobs.push((Job { id: n.id.clone(), page: n.page, bbox: n.bbox, mode, dpi: s.dpi }, i));
    }
    // the renderer takes the jobs by page; the report lists them by page already
    jobs.sort_by_key(|(j, _)| j.page);
    let mut run = Run {
        s,
        notes,
        entries,
        dir,
        written: 0,
        total_bytes: 0,
        seen_payload: HashMap::new(),
        fallbacks: Vec::new(),
        deadline,
    };
    let (job_list, owners): (Vec<Job>, Vec<usize>) = jobs.into_iter().unzip();
    let timeout_arg = s.timeout.as_secs().max(1);
    let mut confined =
        renderer.run(&job_list, deadline, timeout_arg, &mut |k, outcome| run.record(owners[k], outcome))?;
    // figures whose SVG was refused: lossless rasters, in a second pass
    if !run.fallbacks.is_empty() {
        let fb: Vec<(Job, usize)> = run
            .fallbacks
            .iter()
            .map(|(e, _)| {
                let n = &notes[*e];
                (Job { id: n.id.clone(), page: n.page, bbox: n.bbox, mode: Mode::Raster, dpi: s.dpi }, *e)
            })
            .collect();
        let (job_list, owners): (Vec<Job>, Vec<usize>) = fb.into_iter().unzip();
        let left = deadline.saturating_duration_since(Instant::now()).saturating_sub(GRACE).as_secs().max(1);
        let again = renderer.run(&job_list, deadline, left, &mut |k, outcome| run.record(owners[k], outcome))?;
        confined = match (confined, again) {
            (Some(a), Some(b)) => Some(a && b),
            (a, b) => a.or(b),
        };
    }
    let manifest = manifest(&run);
    run.dir.write("manifest.json", manifest.as_bytes())?;
    run.dir.sync()?;
    let written = run.written;
    let files: Vec<Option<String>> = run.entries.iter().map(|e| e.file.clone()).collect();
    for (n, f) in report.images.iter_mut().zip(files) {
        n.file = f;
    }
    report.images_written = Some(written);
    report.images_confined = confined;
    Ok(written)
}

impl Run<'_> {
    /// One record of the renderer for entry `e`. `Err` only for document failures.
    fn record(&mut self, e: usize, outcome: Outcome) -> Result<(), String> {
        let n = &self.notes[e];
        let s = self.s;
        let fallback = self.fallbacks.iter().find(|(f, _)| *f == e).map(|(_, why)| why.clone());
        if fallback.is_some() {
            self.entries[e].fallback = fallback.clone();
        }
        if Instant::now() >= self.deadline {
            self.entries[e].error = Some("timeout".into());
            return Ok(());
        }
        // a marked figure whose rendering is the same as an earlier one's is a repeat
        let payload: &[u8] = match &outcome {
            // the renderer refused an SVG as too large: a raster instead
            Outcome::Error(code) if code == "svgsize" && fallback.is_none() && n.vector => {
                self.fallbacks.push((e, "svg_size".into()));
                return Ok(());
            }
            Outcome::Error(code) => {
                self.entries[e].error = Some(code.clone());
                return Ok(());
            }
            Outcome::Svg(text) if text.len() > MAX_SVG_INPUT => {
                self.fallbacks.push((e, "svg_input".into()));
                return Ok(());
            }
            Outcome::Rgb { data, .. } => data,
            Outcome::Svg(text) => text.as_bytes(),
        };
        let key = (repeat_key(n), <[u8; 32]>::from(Sha256::digest(payload)));
        if s.repeats != Repeats::Keep
            && let Some(&first) = self.seen_payload.get(&key)
        {
            self.entries[e].repeat_of = Some(self.notes[first].id.clone());
            self.entries[e].skipped = Some("repeat");
            return Ok(());
        }
        let (data, format, width, height): (Vec<u8>, &'static str, u32, u32) = match outcome {
            Outcome::Error(_) => return Ok(()),
            Outcome::Rgb { width, height, data } => {
                let fmt = self.raster_format(n, fallback.is_some());
                match encode::encode(&data, width, height, fmt) {
                    Ok(bytes) => (bytes, fmt.extension(), width, height),
                    Err(_) => {
                        self.entries[e].error = Some("encode".into());
                        return Ok(());
                    }
                }
            }
            Outcome::Svg(text) => match svg::compact_until(&text, s.svg_precision, Some(self.deadline)) {
                Ok(small) if small.len() as u64 <= s.svg_max_bytes.min(s.max_file_bytes) => {
                    let (x0, y0, x1, y1) = n.bbox;
                    let (w, h) = ((x1 - x0).ceil() as u32, (y1 - y0).ceil() as u32);
                    (small.into_bytes(), "svg", w, h)
                }
                Ok(_) => {
                    self.fallbacks.push((e, "svg_size".into()));
                    return Ok(());
                }
                Err(_) => {
                    self.fallbacks.push((e, "svg_allowlist".into()));
                    return Ok(());
                }
            },
        };
        if data.len() as u64 > s.max_file_bytes {
            self.entries[e].error = Some("file_bytes".into());
            return Ok(());
        }
        if self.written + 1 > s.max_images {
            return Err(format!("more than {} images to write (--images-max)", s.max_images));
        }
        if self.total_bytes + data.len() as u64 > s.max_bytes {
            return Err(format!("images over {} bytes (--images-max-bytes)", s.max_bytes));
        }
        let name = format!("{}.{format}", n.id);
        self.dir.write(&name, &data)?;
        self.written += 1;
        self.total_bytes += data.len() as u64;
        if s.repeats != Repeats::Keep {
            self.seen_payload.insert(key, e);
        }
        let entry = &mut self.entries[e];
        entry.sha256 = Some(hex(&Sha256::digest(&data)));
        entry.file = Some(name);
        entry.format = Some(format);
        entry.bytes = data.len() as u64;
        entry.width = width;
        entry.height = height;
        Ok(())
    }

    fn raster_format(&self, n: &Note, svg_fallback: bool) -> Raster {
        let s = self.s;
        if n.vector {
            // a figure drawn with paths holds labels: lossless
            let png =
                if svg_fallback { s.raster_format == RasterFormat::Png } else { s.vector_format == VectorFormat::Png };
            return if png { Raster::Png } else { Raster::WebpLossless };
        }
        if s.raster_format == RasterFormat::Png {
            Raster::Png
        } else if s.raster_lossless || n.text {
            Raster::WebpLossless
        } else {
            Raster::WebpLossy(s.raster_quality)
        }
    }
}

fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(s, "{b:02x}");
    }
    s
}

/// A JSON string of fixed vocabulary or checked ids: escaped anyway.
fn js(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if (c as u32) < 0x20 || c == '\u{7f}' => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// `manifest.json`: one image per line, sorted by page, then pictures before vector figures,
/// by index; then the totals. The same input gives the same bytes.
fn manifest(run: &Run) -> String {
    let notes = run.notes;
    let mut order: Vec<usize> = (0..run.entries.len()).collect();
    order.sort_by_key(|&i| id_order(&notes[i].id));
    let opt = |v: &Option<String>| v.as_deref().map(js).unwrap_or_else(|| "null".into());
    let mut out = String::new();
    let _ = write!(
        out,
        "{{\"version\":1,\"dpi\":{},\"min_side_pt\":{:.2},\"repeats\":{},\n\"images\":[",
        run.s.dpi,
        run.s.min_side_pt(),
        js(match run.s.repeats {
            Repeats::Skip => "skip",
            Repeats::Link => "link",
            Repeats::Keep => "keep",
        })
    );
    let (mut repeats, mut errors, mut fallbacks, mut marked, mut small) = (0, 0, 0, 0, 0);
    for (k, &i) in order.iter().enumerate() {
        let n = &notes[i];
        let e = &run.entries[i];
        repeats += usize::from(e.skipped == Some("repeat"));
        errors += usize::from(e.error.is_some());
        fallbacks += usize::from(e.fallback.is_some());
        marked += usize::from(matches!(n.fate, Fate::Block | Fate::Inline));
        small += usize::from(e.skipped == Some("min_side"));
        out.push_str(if k == 0 { "\n" } else { ",\n" });
        let _ = write!(
            out,
            "{{\"id\":{},\"page\":{},\"bbox\":[{:.2},{:.2},{:.2},{:.2}],\"kind\":{},\"fate\":{},\"file\":{},\"format\":{},\"bytes\":{},\"sha256\":{},\"width\":{},\"height\":{}",
            js(&n.id),
            n.page,
            n.bbox.0,
            n.bbox.1,
            n.bbox.2,
            n.bbox.3,
            js(kind(n)),
            js(n.fate.as_str()),
            opt(&e.file),
            e.format.map(js).unwrap_or_else(|| "null".into()),
            e.bytes,
            opt(&e.sha256),
            e.width,
            e.height
        );
        if let Some(r) = e.repeat_of.as_ref().filter(|_| run.s.repeats == Repeats::Link) {
            let _ = write!(out, ",\"repeat_of\":{}", js(r));
        }
        if let Some(f) = &e.fallback {
            let _ = write!(out, ",\"fallback\":{}", js(f));
        }
        if let Some(s) = e.skipped {
            let _ = write!(out, ",\"skipped\":{}", js(s));
        }
        if let Some(err) = &e.error {
            let _ = write!(out, ",\"error\":{}", js(err));
        }
        out.push('}');
    }
    let _ = write!(
        out,
        "\n],\n\"totals\":{{\"images\":{},\"marked\":{marked},\"written\":{},\"bytes\":{},\"repeats\":{repeats},\"small\":{small},\"fallbacks\":{fallbacks},\"errors\":{errors}}}}}\n",
        notes.len(),
        run.written,
        run.total_bytes
    );
    out
}
