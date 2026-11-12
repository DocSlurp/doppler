//! Image and graphic extraction for `pdftomarkdown --images-dir DIR`.
//!
//! After the conversion, the report's images say what became of every picture and vector
//! figure. The marked ones (`block`, `inline`) that are large enough and do not repeat an
//! earlier one are rendered by `pdfregions` (docslurp/extraction/PROTOCOL.md): pictures as
//! RGB crops at the extraction dpi, vector figures as SVG. Crops are encoded as WebP (lossy,
//! or lossless when they hold text) or PNG; SVG is checked against an allowlist and compacted.
//! A refused SVG figure becomes a lossless PNG wrapped in an SVG, so its `.svg` link stays
//! valid. Files go where the Markdown links point ([`layout`]: `page-0012/image-003.webp`);
//! `manifest.json`, written last, lists every image and every file.
//!
//! Full-page scans (`page-scan`) are taken out without rendering where the scan image's
//! dictionary allows: a JPEG is copied byte for byte (`.jpg`), a bilevel image decoded once to
//! a 1-bit PNG, any other single image decoded once at native size and scaled to
//! `--scan-max-dpi` at most; tiles, layers, turned images and `--scan-pages render` are
//! rendered at `--scan-dpi`. A copy that fails its checks is `not-extracted` (reason `copy`),
//! never rendered into another format. Scans have their own budget per batch (`--scan-max`,
//! `--scan-max-bytes`), so a scanned book never starves the figures, nor the reverse.
//!
//! Extraction runs in batches of `--images-batch-pages` pages, one renderer process each, with
//! that batch's own budgets (`--images-max`, `--images-max-bytes`, `--images-timeout`). A
//! batch that hits a budget, times out or whose renderer fails keeps the files it wrote; its
//! other images are `not-extracted` with the reason, the batch records the page it was cut at,
//! and the next batch goes on. Two failed batches in a row stop the rest (`aborted`), as do the
//! document ceilings (`--images-doc-max-bytes`, `--images-deadline`). Only a write failure or
//! a renderer that cannot be started fails the extraction: the caller removes the folder.

pub mod encode;
pub mod layout;
pub mod outdir;
pub mod plan;
pub mod regions;
pub mod settings;
pub mod svg;

use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;
use std::time::{Duration, Instant};

use sha2::{Digest, Sha256};

use crate::Report;
use crate::images::{Fate, Note, ScanSource};
use encode::Raster;
use layout::{RepeatKey, kind, repeat_key};
use outdir::OutDir;
use regions::{Halt, Job, Mode, Outcome, Renderer, RunError};
use settings::{RasterFormat, Repeats, Settings, VectorFormat};

/// Extra time past `--images-timeout` before the renderer is killed (it stops itself at the
/// timeout, exit status 3).
const GRACE: Duration = Duration::from_secs(2);
/// Raw SVG larger than this is not compacted: the figure is rendered as a raster.
pub const MAX_SVG_INPUT: usize = 8 << 20;
/// Failed batches in a row that stop the rest of the extraction.
const MAX_FAILED_IN_A_ROW: usize = 2;

/// Why a batch stopped before its end, as the manifest says it (the batch's `status` and the
/// `reason` of the images it did not get to).
const STOPS: [&str; 6] = ["budget", "doc_budget", "timeout", "deadline", "renderer", "aborted"];

/// What became of one image.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Status {
    /// no link in the Markdown (its fate)
    Unlinked,
    /// waiting for its batch
    Pending,
    Extracted,
    NotExtracted(&'static str),
}

/// One image of the manifest.
#[derive(Clone, Debug)]
struct Entry {
    status: Status,
    /// the renderer's error code or a record problem, with `NotExtracted("error")`
    error: Option<String>,
    format: Option<&'static str>,
    source: Option<&'static str>,
    bytes: u64,
    sha256: Option<String>,
    width: u32,
    height: u32,
    repeat_of: Option<String>,
    fallback: Option<String>,
}

impl Default for Entry {
    fn default() -> Self {
        Entry {
            status: Status::Pending,
            error: None,
            format: None,
            source: None,
            bytes: 0,
            sha256: None,
            width: 0,
            height: 0,
            repeat_of: None,
            fallback: None,
        }
    }
}

impl Entry {
    /// Not extracted because its batch stopped (not for a reason of its own).
    fn stopped(&self) -> bool {
        matches!(self.status, Status::NotExtracted(r) if STOPS.contains(&r))
    }
}

/// One batch of the manifest: its pages, how it ended and where it was cut.
#[derive(Clone, Debug)]
struct Batch {
    pages: (u32, u32),
    status: &'static str,
    cut_page: Option<u32>,
}

/// Where extraction spent its time, for the report (`images_ms`, `images_cpu_ms`). The
/// encoders run on this thread, so their wall time is their CPU time; `wait` is time blocked
/// on the renderer's records; `render` is the renderer processes' CPU (user + system, from
/// `/proc/self/stat`'s waited-for children). `copy` and `decode` are for copied page scans.
#[derive(Clone, Debug, Default)]
pub struct Timing {
    pub wait: Duration,
    pub webp: Duration,
    pub png: Duration,
    pub svg: Duration,
    pub copy: Duration,
    pub decode: Duration,
    /// children's user and system CPU in milliseconds, when `/proc` says
    pub children_ms: Option<(u64, u64)>,
}

impl Timing {
    /// `"images_ms":{...},"images_cpu_ms":{...}`.
    pub fn to_json(&self) -> String {
        let ms = |d: Duration| format!("{:.1}", d.as_secs_f64() * 1000.0);
        let (user, sys) = self.children_ms.unwrap_or((0, 0));
        format!(
            "\"images_ms\":{{\"render\":{},\"copy\":{},\"decode\":{},\"webp\":{},\"png\":{},\"svg\":{},\"wait\":{}}},\"images_cpu_ms\":{{\"children_user\":{user},\"children_sys\":{sys},\"known\":{}}}",
            user + sys,
            ms(self.copy),
            ms(self.decode),
            ms(self.webp),
            ms(self.png),
            ms(self.svg),
            ms(self.wait),
            self.children_ms.is_some()
        )
    }
}

/// User and system CPU (ms) of this process's waited-for children, from `/proc/self/stat`.
fn children_cpu_ms() -> Option<(u64, u64)> {
    let stat = std::fs::read_to_string("/proc/self/stat").ok()?;
    // the fields after the command name, which is in parentheses and may hold anything
    let rest = &stat[stat.rfind(')')? + 1..];
    let f: Vec<&str> = rest.split_whitespace().collect();
    // f[0] is field 3 (state); cutime and cstime are fields 16 and 17
    let (cu, cs): (u64, u64) = (f.get(13)?.parse().ok()?, f.get(14)?.parse().ok()?);
    let hz = rustix::param::clock_ticks_per_second().max(1);
    Some((cu.saturating_mul(1000) / hz, cs.saturating_mul(1000) / hz))
}

/// The outcome of an extraction that wrote its manifest.
#[derive(Clone, Copy, Debug)]
pub struct Summary {
    pub written: usize,
    /// every batch finished (manifest `complete`)
    pub complete: bool,
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
    /// this batch's files and bytes (pictures and figures)
    batch_written: usize,
    batch_bytes: u64,
    /// page scans: this batch's files and bytes, a scan over that budget in this batch, and
    /// the document's
    scan_batch_written: usize,
    scan_batch_bytes: u64,
    scan_over: bool,
    scans_written: usize,
    scan_bytes: u64,
    /// content repeats: (key, sha256 of the rendered payload) -> entry
    seen_payload: HashMap<(RepeatKey, [u8; 32]), usize>,
    /// repeat keys held by more than one job: only their payloads can repeat one another, so
    /// only they are hashed (a raw crop is megabytes; hashing every one was ~4% of the pass)
    shared_keys: HashSet<RepeatKey>,
    /// this batch's figures whose SVG failed: rendered again as rasters
    fallbacks: Vec<(usize, String)>,
    /// this batch's deadline: past it, records are not encoded or compacted any more
    deadline: Instant,
    /// the document's deadline (`--images-deadline`), when given
    doc_deadline: Option<Instant>,
    /// set when `--images-doc-max-bytes` was reached
    doc_full: bool,
    timing: Timing,
}

/// Extracts the images of `report` into `dir` and writes `manifest.json` last: sets each note's
/// link (from `s`, as the Markdown has them) and `file`, and the report's `images_written`,
/// `images_complete`, `images_confined` and `images_timing`. `source_sha256` names the PDF in
/// the manifest. `Err` is a hard failure (a write failure, or a renderer that cannot be
/// started): the caller removes the folder.
pub fn extract(
    report: &mut Report,
    s: &Settings,
    source_sha256: &str,
    renderer: &Renderer,
    dir: &mut OutDir,
) -> Result<Summary, String> {
    let start = Instant::now();
    let cpu_before = children_cpu_ms();
    // the links the Markdown has (an images plan's notes get them here)
    layout::Linker::new(s.link_style()).assign(&mut report.images);
    let notes = &report.images;
    let mut entries: Vec<Entry> = vec![Entry::default(); notes.len()];
    let min_side = s.min_side_pt();
    let mut jobs: Vec<(Job, usize)> = Vec::new();
    let index: HashMap<&str, usize> = notes.iter().enumerate().map(|(i, n)| (n.id.as_str(), i)).collect();
    let mut promoted: HashSet<&str> = HashSet::new();
    for (i, n) in notes.iter().enumerate() {
        let e = &mut entries[i];
        let scan = n.fate == Fate::PageScan;
        if n.scan.is_some() && !scan {
            // part of a scan placed as a picture (--scan-pages skip): never extracted
            e.status = if n.fate.marked() { Status::NotExtracted("scan-pages") } else { Status::Unlinked };
            continue;
        }
        if !n.fate.marked() {
            e.status = Status::Unlinked;
            continue;
        }
        if n.link.is_none() {
            e.status = Status::NotExtracted("id");
            continue;
        }
        if let Some((a, b)) = s.pages
            && !(a..=b).contains(&n.page)
        {
            e.status = Status::NotExtracted("pages");
            continue;
        }
        let (x0, y0, x1, y1) = n.bbox;
        let finite = [x0, y0, x1, y1].iter().all(|v| v.is_finite());
        if !finite || x1 <= x0 || y1 <= y0 {
            e.status = Status::NotExtracted("bbox");
            continue;
        }
        if !scan && (x1 - x0).min(y1 - y0) < min_side {
            e.status = Status::NotExtracted("min_side");
            continue;
        }
        if let Some(first) = &n.repeat_of {
            // the first placement's file is this one's link; when that placement lies outside
            // --images-pages, the first repeat inside them writes it
            let target = index.get(first.as_str()).copied();
            let outside = target.is_some_and(|t| entries[t].status == Status::NotExtracted("pages"));
            if !(outside && promoted.insert(first.as_str())) {
                let e = &mut entries[i];
                e.status = Status::NotExtracted("repeat");
                e.repeat_of = Some(first.clone());
                continue;
            }
            entries[i].repeat_of = Some(first.clone());
        }
        let job = match (scan, n.scan, n.obj) {
            (true, Some(ScanSource::Jpeg | ScanSource::Bilevel | ScanSource::Decode), Some(obj)) => {
                Job { id: n.id.clone(), page: n.page, bbox: n.bbox, mode: Mode::Copy, dpi: 0, obj: Some(obj) }
            }
            (true, _, _) => {
                Job { id: n.id.clone(), page: n.page, bbox: n.bbox, mode: Mode::Raster, dpi: s.scan_dpi, obj: None }
            }
            _ => {
                let mode = if n.vector && s.vector_format == VectorFormat::Svg { Mode::Svg } else { Mode::Raster };
                Job { id: n.id.clone(), page: n.page, bbox: n.bbox, mode, dpi: s.dpi, obj: None }
            }
        };
        jobs.push((job, i));
    }
    // the renderer takes the jobs by page; the report lists them by page already
    jobs.sort_by_key(|(j, _)| j.page);
    // a fallback raster re-renders a job's own note, so the first pass's keys cover every record
    let mut key_count: HashMap<RepeatKey, usize> = HashMap::new();
    // (page scans are not compared by payload: each is its page's own, and decoded ones are
    // megabytes; repeats of one scan object are linked at conversion)
    for (_, i) in jobs.iter().filter(|(_, i)| notes[*i].fate != Fate::PageScan) {
        *key_count.entry(repeat_key(&notes[*i])).or_default() += 1;
    }
    let shared_keys = key_count.into_iter().filter(|(_, c)| *c > 1).map(|(k, _)| k).collect();
    let mut run = Run {
        s,
        notes,
        entries,
        dir,
        written: 0,
        total_bytes: 0,
        batch_written: 0,
        batch_bytes: 0,
        scan_batch_written: 0,
        scan_batch_bytes: 0,
        scan_over: false,
        scans_written: 0,
        scan_bytes: 0,
        seen_payload: HashMap::new(),
        shared_keys,
        fallbacks: Vec::new(),
        deadline: start,
        doc_deadline: s.deadline.map(|d| start + d),
        doc_full: false,
        timing: Timing::default(),
    };
    let batch_pages = s.batch_pages.max(1);
    let mut batches: Vec<Batch> = Vec::new();
    let mut failed_in_a_row = 0;
    let mut confined: Option<bool> = None;
    let mut k = 0;
    while k < jobs.len() {
        let index = jobs[k].0.page.saturating_sub(1) / batch_pages;
        let end = k + jobs[k..].iter().take_while(|(j, _)| j.page.saturating_sub(1) / batch_pages == index).count();
        let first_page = index * batch_pages + 1;
        let pages = (first_page, first_page.saturating_add(batch_pages - 1));
        let batch = &jobs[k..end];
        k = end;
        let skip = if failed_in_a_row >= MAX_FAILED_IN_A_ROW {
            Some("aborted")
        } else if run.doc_full {
            Some("doc_budget")
        } else if run.doc_deadline.is_some_and(|d| Instant::now() >= d) {
            Some("deadline")
        } else {
            None
        };
        if let Some(why) = skip {
            for (_, i) in batch {
                run.entries[*i].status = Status::NotExtracted(why);
            }
            batches.push(Batch { pages, status: why, cut_page: Some(batch[0].0.page) });
            continue;
        }
        let (status, c) = run.batch(batch, renderer)?;
        confined = match (confined, c) {
            (Some(a), Some(b)) => Some(a && b),
            (a, b) => a.or(b),
        };
        let cut_page = batch.iter().filter(|(_, i)| run.entries[*i].stopped()).map(|(j, _)| j.page).min();
        if matches!(status, "timeout" | "renderer") {
            failed_in_a_row += 1;
        } else {
            failed_in_a_row = 0;
        }
        batches.push(Batch { pages, status, cut_page: if status == "ok" { None } else { cut_page } });
    }
    let complete = batches.iter().all(|b| b.status == "ok");
    if let (Some(b), Some(a)) = (cpu_before, children_cpu_ms()) {
        run.timing.children_ms = Some((a.0.saturating_sub(b.0), a.1.saturating_sub(b.1)));
    }
    // the files first, then the manifest that lists them
    run.dir.sync()?;
    let manifest = manifest(&run, source_sha256, &batches, complete);
    run.dir.write("manifest.json", manifest.as_bytes())?;
    run.dir.sync_top()?;
    let written = run.written;
    let files: Vec<Option<String>> = run
        .entries
        .iter()
        .zip(notes)
        .map(|(e, n)| if e.status == Status::Extracted { n.link.clone() } else { None })
        .collect();
    let timing = run.timing;
    for (n, f) in report.images.iter_mut().zip(files) {
        n.file = f;
    }
    report.images_written = Some(written);
    report.images_confined = confined;
    report.images_complete = Some(complete);
    report.images_timing = Some(timing);
    Ok(Summary { written, complete })
}

impl Run<'_> {
    /// Renders and writes one batch (its jobs sorted by page), then its SVG fallbacks. Returns
    /// how the batch ended (`ok` or one of [`STOPS`]) and whether the renderer said it was
    /// confined. `Err` only for hard failures.
    fn batch(&mut self, jobs: &[(Job, usize)], renderer: &Renderer) -> Result<(&'static str, Option<bool>), String> {
        let s = self.s;
        self.batch_written = 0;
        self.batch_bytes = 0;
        self.scan_batch_written = 0;
        self.scan_batch_bytes = 0;
        self.scan_over = false;
        self.fallbacks.clear();
        self.deadline = Instant::now() + s.timeout + GRACE;
        if let Some(d) = self.doc_deadline {
            self.deadline = self.deadline.min(d + GRACE);
        }
        // the protocol's limit: the rest of a huge batch is over its budget
        let (sent, over) = jobs.split_at(jobs.len().min(regions::MAX_JOBS));
        let (mut status, mut confined) = self.render(sent, renderer, false)?;
        for (_, i) in over {
            self.entries[*i].status = Status::NotExtracted("budget");
        }
        if status == "ok" && (!over.is_empty() || self.scan_over) {
            status = "budget";
        }
        // figures whose SVG was refused: lossless rasters, in a second pass
        if !self.fallbacks.is_empty() {
            let fb: Vec<(Job, usize)> = self
                .fallbacks
                .iter()
                .map(|(e, _)| {
                    let n = &self.notes[*e];
                    (
                        Job { id: n.id.clone(), page: n.page, bbox: n.bbox, mode: Mode::Raster, dpi: s.dpi, obj: None },
                        *e,
                    )
                })
                .collect();
            if status == "ok" {
                let (again, c) = self.render(&fb, renderer, true)?;
                status = again;
                confined = match (confined, c) {
                    (Some(a), Some(b)) => Some(a && b),
                    (a, b) => a.or(b),
                };
            } else {
                for (_, i) in &fb {
                    if self.entries[*i].status == Status::Pending {
                        self.entries[*i].status = Status::NotExtracted(status);
                    }
                }
            }
        }
        Ok((status, confined))
    }

    /// One renderer run over `jobs` (`fallbacks`: the raster pass of refused SVG figures); every
    /// job it leaves pending (and, in the first pass, not waiting for a raster fallback) gets
    /// the reason it stopped. `Err` only for hard failures.
    fn render(
        &mut self,
        jobs: &[(Job, usize)],
        renderer: &Renderer,
        fallbacks: bool,
    ) -> Result<(&'static str, Option<bool>), String> {
        let (job_list, owners): (Vec<Job>, Vec<usize>) = jobs.iter().cloned().unzip();
        let left = self.deadline.saturating_duration_since(Instant::now()).saturating_sub(GRACE);
        let timeout_arg = left.as_secs().clamp(1, self.s.timeout.as_secs().max(1));
        let r = renderer.run(&job_list, self.deadline, timeout_arg, &mut |k, outcome| self.record(owners[k], outcome));
        self.timing.wait += r.wait;
        let status = match r.result {
            Ok(()) => "ok",
            Err(RunError::Spawn(e)) | Err(RunError::Halted(Halt::Fatal(e))) => return Err(e),
            Err(RunError::Halted(Halt::Stop(why))) => why,
            Err(RunError::Timeout(_)) if self.doc_deadline.is_some_and(|d| Instant::now() >= d) => "deadline",
            Err(RunError::Timeout(_)) => "timeout",
            Err(RunError::Failed(_)) => "renderer",
        };
        for i in owners {
            if self.entries[i].status == Status::Pending && (fallbacks || !self.fallbacks.iter().any(|(f, _)| *f == i))
            {
                // a run that ended well answered every job
                self.entries[i].status = Status::NotExtracted(if status == "ok" { "renderer" } else { status });
            }
        }
        Ok((status, r.confined))
    }

    /// One record of the renderer for entry `e`. `Halt::Stop` ends the batch (a budget or a
    /// deadline: `e` stays pending and gets that reason); `Halt::Fatal` is a write failure.
    fn record(&mut self, e: usize, outcome: Outcome) -> Result<(), Halt> {
        let n = &self.notes[e];
        let s = self.s;
        let fallback = self.fallbacks.iter().find(|(f, _)| *f == e).map(|(_, why)| why.clone());
        if fallback.is_some() {
            self.entries[e].fallback = fallback.clone();
        }
        let now = Instant::now();
        if self.doc_deadline.is_some_and(|d| now >= d) {
            return Err(Halt::Stop("deadline"));
        }
        if now >= self.deadline {
            return Err(Halt::Stop("timeout"));
        }
        if n.fate == Fate::PageScan {
            return self.record_scan(e, outcome);
        }
        // a marked figure whose rendering is the same as an earlier one's is a repeat
        let payload: &[u8] = match &outcome {
            // the renderer refused an SVG as too large: a raster instead
            Outcome::Error(code) if code == "svgsize" && fallback.is_none() && n.vector => {
                self.fallbacks.push((e, "svg_size".into()));
                return Ok(());
            }
            Outcome::Error(code) => {
                self.entries[e].status = Status::NotExtracted("error");
                self.entries[e].error = Some(code.clone());
                return Ok(());
            }
            Outcome::Svg(text) if text.len() > MAX_SVG_INPUT => {
                self.fallbacks.push((e, "svg_input".into()));
                return Ok(());
            }
            Outcome::Rgb { data, .. } => data,
            Outcome::Svg(text) => text.as_bytes(),
            // copy records answer copy jobs only, which pictures and figures never are
            Outcome::Jpeg { .. } | Outcome::Gray1 { .. } | Outcome::Gray8 { .. } => {
                self.entries[e].status = Status::NotExtracted("error");
                self.entries[e].error = Some("mismatch".into());
                return Ok(());
            }
        };
        // no other job has this key: no payload can repeat it (nor be compared against it)
        let rk = repeat_key(n);
        let key = (s.repeats != Repeats::Keep && self.shared_keys.contains(&rk))
            .then(|| (rk, <[u8; 32]>::from(Sha256::digest(payload))));
        if let Some(key) = &key
            && let Some(&first) = self.seen_payload.get(key)
        {
            self.entries[e].repeat_of = Some(self.notes[first].id.clone());
            self.entries[e].status = Status::NotExtracted("repeat");
            return Ok(());
        }
        let (data, format, width, height): (Vec<u8>, &'static str, u32, u32) = match outcome {
            Outcome::Error(_) | Outcome::Jpeg { .. } | Outcome::Gray1 { .. } | Outcome::Gray8 { .. } => return Ok(()),
            Outcome::Rgb { width, height, data } => {
                // a refused SVG figure: a lossless PNG inside an SVG, so its .svg link holds
                if n.vector && fallback.is_some() && s.vector_format == VectorFormat::Svg {
                    match self.wrapped_png(n, &data, width, height) {
                        Some(svg) => (svg.into_bytes(), "svg", width, height),
                        None => {
                            self.entries[e].status = Status::NotExtracted("encode");
                            return Ok(());
                        }
                    }
                } else {
                    let fmt = self.raster_format(n);
                    let t = Instant::now();
                    let encoded = encode::encode_with(&data, width, height, fmt, s.effort());
                    *(if fmt == Raster::Png { &mut self.timing.png } else { &mut self.timing.webp }) += t.elapsed();
                    match encoded {
                        Ok(bytes) => (bytes, fmt.extension(), width, height),
                        Err(_) => {
                            self.entries[e].status = Status::NotExtracted("encode");
                            return Ok(());
                        }
                    }
                }
            }
            Outcome::Svg(text) => {
                let t = Instant::now();
                let small = svg::compact_until(&text, s.svg_precision, Some(self.deadline));
                self.timing.svg += t.elapsed();
                match small {
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
                }
            }
        };
        if data.len() as u64 > s.max_file_bytes {
            self.entries[e].status = Status::NotExtracted("file_bytes");
            return Ok(());
        }
        if self.batch_written + 1 > s.max_images || self.batch_bytes + data.len() as u64 > s.max_bytes {
            return Err(Halt::Stop("budget"));
        }
        if self.total_bytes + data.len() as u64 > s.doc_max_bytes {
            self.doc_full = true;
            return Err(Halt::Stop("doc_budget"));
        }
        let Some(path) = n.link.clone() else {
            self.entries[e].status = Status::NotExtracted("id");
            return Ok(());
        };
        // the file is what its link's extension says
        if !path.ends_with(&format!(".{format}")) {
            self.entries[e].status = Status::NotExtracted("encode");
            return Ok(());
        }
        self.dir.write(&path, &data).map_err(Halt::Fatal)?;
        self.written += 1;
        self.batch_written += 1;
        self.total_bytes += data.len() as u64;
        self.batch_bytes += data.len() as u64;
        if let Some(key) = key {
            self.seen_payload.insert(key, e);
        }
        let entry = &mut self.entries[e];
        entry.status = Status::Extracted;
        entry.sha256 = Some(hex(&Sha256::digest(&data)));
        entry.format = Some(format);
        entry.source = Some("render");
        entry.bytes = data.len() as u64;
        entry.width = width;
        entry.height = height;
        Ok(())
    }

    /// One record for page scan `e`: checked against how its file is made (a JPEG copy, 1-bit
    /// rows, decoded samples, or a render), encoded, and written against the scan budget.
    fn record_scan(&mut self, e: usize, outcome: Outcome) -> Result<(), Halt> {
        let n = &self.notes[e];
        let s = self.s;
        let source = n.scan.unwrap_or(ScanSource::Render);
        let wrong = |run: &mut Self, why: &str| {
            run.entries[e].status = Status::NotExtracted("copy");
            run.entries[e].error = Some(why.to_string());
            Ok(())
        };
        let t = Instant::now();
        let (data, format, width, height, how): (Vec<u8>, &'static str, u32, u32, &'static str) =
            match (outcome, source) {
                (Outcome::Error(code), _) => {
                    self.entries[e].status =
                        Status::NotExtracted(if source == ScanSource::Render { "error" } else { "copy" });
                    self.entries[e].error = Some(code);
                    return Ok(());
                }
                (Outcome::Jpeg { width, height, data }, ScanSource::Jpeg) => {
                    if (width, height) != n.px || !encode::jpeg_ok(&data, width, height) {
                        return wrong(self, "jpeg");
                    }
                    self.timing.copy += t.elapsed();
                    (data, "jpg", width, height, "copy")
                }
                (Outcome::Gray1 { width, height, data }, ScanSource::Bilevel) => {
                    if (width, height) != n.px {
                        return wrong(self, "size");
                    }
                    let png = encode::png_gray1(&data, width, height);
                    self.timing.png += t.elapsed();
                    match png {
                        Ok(png) => (png, "png", width, height, "decode"),
                        Err(_) => {
                            self.entries[e].status = Status::NotExtracted("encode");
                            return Ok(());
                        }
                    }
                }
                (Outcome::Gray8 { width, height, data }, ScanSource::Decode)
                | (Outcome::Rgb { width, height, data }, ScanSource::Decode) => {
                    if (width, height) != n.px {
                        return wrong(self, "size");
                    }
                    let rgb =
                        if data.len() == width as usize * height as usize { encode::gray_to_rgb(&data) } else { data };
                    // at most --scan-max-dpi across the scan's box
                    let (x0, y0, x1, y1) = n.bbox;
                    let fit = |px: u32, pt: f64| -> u32 {
                        let most = (pt / 72.0 * f64::from(s.scan_max_dpi)).round();
                        if most.is_finite() && most >= 1.0 && most < f64::from(px) { most as u32 } else { px }
                    };
                    let (w, h) = (fit(width, x1 - x0), fit(height, y1 - y0));
                    let rgb = encode::downscale(rgb, width, height, 3, w, h);
                    self.timing.decode += t.elapsed();
                    match self.encode_raster(n, &rgb, w, h) {
                        Some((bytes, ext)) => (bytes, ext, w, h, "decode"),
                        None => {
                            self.entries[e].status = Status::NotExtracted("encode");
                            return Ok(());
                        }
                    }
                }
                (Outcome::Rgb { width, height, data }, ScanSource::Render) => {
                    match self.encode_raster(n, &data, width, height) {
                        Some((bytes, ext)) => (bytes, ext, width, height, "render"),
                        None => {
                            self.entries[e].status = Status::NotExtracted("encode");
                            return Ok(());
                        }
                    }
                }
                _ => return wrong(self, "kind"),
            };
        let len = data.len() as u64;
        if self.scan_batch_written + 1 > s.scan_max || self.scan_batch_bytes + len > s.scan_max_bytes {
            // the scan pool of this batch is spent: this scan is not written, the rest of the
            // batch (figures, and scans that still fit) goes on
            self.entries[e].status = Status::NotExtracted("budget");
            self.scan_over = true;
            return Ok(());
        }
        if self.total_bytes + len > s.doc_max_bytes {
            self.doc_full = true;
            return Err(Halt::Stop("doc_budget"));
        }
        let Some(path) = n.link.clone() else {
            self.entries[e].status = Status::NotExtracted("id");
            return Ok(());
        };
        if !path.ends_with(&format!(".{format}")) {
            return wrong(self, "kind");
        }
        self.dir.write(&path, &data).map_err(Halt::Fatal)?;
        self.written += 1;
        self.scans_written += 1;
        self.scan_batch_written += 1;
        self.total_bytes += len;
        self.scan_bytes += len;
        self.scan_batch_bytes += len;
        let entry = &mut self.entries[e];
        entry.status = Status::Extracted;
        entry.sha256 = Some(hex(&Sha256::digest(&data)));
        entry.format = Some(format);
        entry.source = Some(how);
        entry.bytes = len;
        entry.width = width;
        entry.height = height;
        Ok(())
    }

    /// RGB pixels in the raster format a picture `n` gets, timed; with the file extension.
    fn encode_raster(&mut self, n: &Note, rgb: &[u8], width: u32, height: u32) -> Option<(Vec<u8>, &'static str)> {
        let fmt = self.raster_format(n);
        let t = Instant::now();
        let encoded = encode::encode_with(rgb, width, height, fmt, self.s.effort());
        *(if fmt == Raster::Png { &mut self.timing.png } else { &mut self.timing.webp }) += t.elapsed();
        encoded.ok().map(|b| (b, fmt.extension()))
    }

    /// The SVG fallback of figure `n`: its crop as a lossless PNG in a minimal SVG of the
    /// figure's box, checked and rewritten by the same allowlist as any SVG.
    fn wrapped_png(&mut self, n: &Note, rgb: &[u8], width: u32, height: u32) -> Option<String> {
        let t = Instant::now();
        let png = encode::encode_with(rgb, width, height, Raster::Png, self.s.effort());
        self.timing.png += t.elapsed();
        let png = png.ok()?;
        let (x0, y0, x1, y1) = n.bbox;
        let (w, h) = (x1 - x0, y1 - y0);
        let raw = format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w:.3}pt\" height=\"{h:.3}pt\" viewBox=\"0 0 {w:.3} {h:.3}\"><image width=\"{w:.3}\" height=\"{h:.3}\" href=\"data:image/png;base64,{}\"/></svg>",
            base64(&png)
        );
        let t = Instant::now();
        let svg = svg::compact_until(&raw, self.s.svg_precision, Some(self.deadline)).ok();
        self.timing.svg += t.elapsed();
        svg
    }

    fn raster_format(&self, n: &Note) -> Raster {
        let s = self.s;
        if n.vector {
            // a figure drawn with paths holds labels: lossless
            return if s.vector_format == VectorFormat::Png { Raster::Png } else { Raster::WebpLossless };
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

/// Standard base64 with padding.
fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for c in data.chunks(3) {
        let b = [c[0], *c.get(1).unwrap_or(&0), *c.get(2).unwrap_or(&0)];
        let v = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for (k, shift) in [18, 12, 6, 0].into_iter().enumerate() {
            if k <= c.len() {
                out.push(T[((v >> shift) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
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

/// `manifest.json` (version 2): the run's settings and batches, then one image per line,
/// sorted by page, then pictures before vector figures, by index; then the totals. The same
/// input gives the same bytes.
fn manifest(run: &Run, source_sha256: &str, batches: &[Batch], complete: bool) -> String {
    let notes = run.notes;
    let s = run.s;
    let mut order: Vec<usize> = (0..run.entries.len()).collect();
    order.sort_by_key(|&i| id_order(&notes[i].id));
    let opt = |v: &Option<String>| v.as_deref().map(js).unwrap_or_else(|| "null".into());
    let mut out = String::new();
    let _ = write!(
        out,
        "{{\"version\":2,\"source_sha256\":{},\"link_prefix\":{},\"dpi\":{},\"min_side_pt\":{:.2},\"repeats\":{},\"scan_pages\":{},\"scan_dpi\":{},\"scan_max_dpi\":{},\"batch_pages\":{},\"complete\":{complete},\n\"batches\":[",
        js(source_sha256),
        js(&s.link_prefix),
        s.dpi,
        s.min_side_pt(),
        js(match s.repeats {
            Repeats::Skip => "skip",
            Repeats::Link => "link",
            Repeats::Keep => "keep",
        }),
        js(s.scan_pages.as_str()),
        s.scan_dpi,
        s.scan_max_dpi,
        s.batch_pages
    );
    for (k, b) in batches.iter().enumerate() {
        out.push_str(if k == 0 { "\n" } else { ",\n" });
        let _ = write!(out, "{{\"pages\":[{},{}],\"status\":{}", b.pages.0, b.pages.1, js(b.status));
        if let Some(c) = b.cut_page {
            let _ = write!(out, ",\"cut_page\":{c}");
        }
        out.push('}');
    }
    out.push_str("\n],\n\"images\":[");
    let (mut repeats, mut fallbacks, mut linked, mut extracted, mut not_extracted, mut unlinked) = (0, 0, 0, 0, 0, 0);
    for (k, &i) in order.iter().enumerate() {
        let n = &notes[i];
        let e = &run.entries[i];
        let (status, reason) = match e.status {
            Status::Unlinked => ("unlinked", None),
            Status::Extracted => ("extracted", None),
            Status::NotExtracted(r) => ("not-extracted", Some(r)),
            // a job no batch settled (every batch settles its jobs)
            Status::Pending => ("not-extracted", Some("renderer")),
        };
        let path = if e.status == Status::Unlinked { None } else { n.link.clone() };
        repeats += usize::from(reason == Some("repeat"));
        fallbacks += usize::from(e.fallback.is_some());
        linked += usize::from(path.is_some());
        extracted += usize::from(status == "extracted");
        not_extracted += usize::from(status == "not-extracted");
        unlinked += usize::from(status == "unlinked");
        let link = path.as_ref().map(|p| format!("{}{p}", s.link_prefix));
        let format = path.as_deref().and_then(|p| p.rsplit_once('.')).map(|(_, x)| x);
        out.push_str(if k == 0 { "\n" } else { ",\n" });
        let _ = write!(
            out,
            "{{\"id\":{},\"page\":{},\"bbox\":[{:.2},{:.2},{:.2},{:.2}],\"kind\":{},\"fate\":{},\"obj\":{},\"path\":{},\"link\":{},\"status\":{}",
            js(&n.id),
            n.page,
            n.bbox.0,
            n.bbox.1,
            n.bbox.2,
            n.bbox.3,
            js(kind(n)),
            js(n.fate.as_str()),
            n.obj.map(|(a, b)| format!("[{a},{b}]")).unwrap_or_else(|| "null".into()),
            opt(&path),
            opt(&link),
            js(status)
        );
        if let Some(r) = reason {
            let _ = write!(out, ",\"reason\":{}", js(r));
        }
        if let Some(err) = &e.error {
            let _ = write!(out, ",\"error\":{}", js(err));
        }
        if let Some(r) = &e.repeat_of {
            let _ = write!(out, ",\"repeat_of\":{}", js(r));
        }
        let _ = write!(
            out,
            ",\"format\":{},\"source\":{},\"bytes\":{},\"sha256\":{},\"width\":{},\"height\":{}",
            e.format.or(format).map(js).unwrap_or_else(|| "null".into()),
            e.source.map(js).unwrap_or_else(|| "null".into()),
            e.bytes,
            opt(&e.sha256),
            e.width,
            e.height
        );
        if let Some(f) = &e.fallback {
            let _ = write!(out, ",\"fallback\":{}", js(f));
        }
        out.push('}');
    }
    let cut = batches.iter().filter(|b| b.status != "ok").count();
    let _ = write!(
        out,
        "\n],\n\"totals\":{{\"images\":{},\"linked\":{linked},\"extracted\":{extracted},\"not_extracted\":{not_extracted},\"unlinked\":{unlinked},\"bytes\":{},\"repeats\":{repeats},\"fallbacks\":{fallbacks},\"scans\":{},\"scan_bytes\":{},\"batches\":{},\"batches_cut\":{cut}}}}}\n",
        notes.len(),
        run.total_bytes,
        run.scans_written,
        run.scan_bytes,
        batches.len()
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_is_standard() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
        assert_eq!(base64(&[0xff, 0xfe]), "//4=");
    }

    #[test]
    fn children_cpu_reads_proc() {
        if std::path::Path::new("/proc/self/stat").exists() {
            assert!(children_cpu_ms().is_some());
        }
    }
}
