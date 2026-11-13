//! `pdftomarkdown [--html] [--strict] [--report] [--params JSON] [-f N] [-l N] [--opw PW]
//! [--upw PW] [--] FILE.pdf`: PDF to Markdown in one step. Runs the DocSlurp fork's
//! `pdftotext -q -bbox-layout -wordmeta -rules -compact-layout FILE -`, converts its XHTML as
//! it arrives and writes the pages joined with form feeds on stdout. By default every page
//! waits for the whole document; `--stream` (or `--window N`) writes each page once the next N
//! are complete, holding at most N + 2 pages (the window and the page being parsed). `--strict` drops
//! `-compact-layout` (full-precision coordinates): the same bytes as piping
//! `pdftotext -bbox-layout -wordmeta -rules` into `doppler-md`.
//!
//! `--images-dir DIR` also writes the marked pictures and figures (rendered by `pdfregions`)
//! to DIR where the Markdown's links point; stdout is closed once the Markdown is complete,
//! before extraction. Exit status 5 when the Markdown was written and the images only in part
//! (DIR and its manifest kept), 6 when extraction failed (DIR removed).
//!
//! pdftotext is `$PDFTOTEXT` if set, else a `pdftotext` next to this executable, else
//! `pdftotext` on `PATH`. Exit status 2 for bad arguments or output without a `<page>` (a
//! pdftotext that is not the fork), 1 when pdftotext cannot be run or exits non-zero
//! (including its time limit, always passed: its own status 6, which is not ours).

#![forbid(unsafe_code)]

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};

use doppler_md::extract::outdir::{NewFile, OutDir};
use doppler_md::extract::regions::{self, Renderer};
use doppler_md::extract::settings::Settings;
use doppler_md::extract::{manifest, plan, resume};
use doppler_md::{Options, Params, TableFormat};

#[path = "../cli.rs"]
mod cli;
#[path = "../facts_file.rs"]
mod facts_file;
#[path = "../scan_thresholds.rs"]
mod scan_thresholds;

use facts_file::FactsFile;
use scan_thresholds::ScanThresholds;

/// `--stream`'s lookahead, in pages.
const STREAM_WINDOW: usize = 16;

const USAGE: &str = "\
usage: pdftomarkdown [--html] [--math] [--markers] [--strict] [--stream | --window N] [--report] [--params JSON]
                     [-f N] [-l N] [--opw PW] [--upw PW] [--images-dir DIR [image options]] [--] FILE.pdf

  --html         write tables as HTML <table> blocks (prose stays Markdown)
  --math         read typeset math (pdftotext -glyphs) and write it as LaTeX: $...$ inline,
                 $$...$$ display; doubtful formulas stay text (report: mathUncertainPages)
  --markers      mark where pictures sit in the text: ![image](images/page-0012/image-003.webp)
                 between paragraphs or inside a line, and charts and diagrams drawn with paths:
                 ![image](images/page-0012/figure-001.svg) (pdftotext -images); report: images.
                 The link is the image's path under --images-dir, the same with or without it
  --page-markers start each page with <!-- doppler:page N --> (its PDF page number); report:
                 page_numbers. Pages are also joined by form feeds: chunk k is page first+k
  --page-info    report: page_info, per page its crop box size as pdfinfo prints it, its word and
                 invisible-word counts, and its text layer (none, invisible, visible or mixed)
  --strict       full-precision layout coordinates (pdftotext without -compact-layout):
                 the same output as `pdftotext -bbox-layout -wordmeta -rules | doppler-md`
  --stream       write pages as they are converted (--window 16)
  --window N     body size and running headers/footers from N pages either side of each
                 page, not the whole document; each page is written once the next N are complete.
                 A document of at most N + 1 pages converts exactly as without it.
                 Repeated content (running headers/footers, repeated watermarks) is found only
                 within the window: do not use --window 0, which removes none of it
  --report       print a one-line JSON report to stderr
  --params JSON  override layout tuning parameters
  -f N, -l N     first and last page to convert
  --opw, --upw   owner and user password (also -opw, -upw); as with pdftotext they are
                 visible in the process list

pdftotext time limit (always on: 60 s plus 1 s per page of the range, at most 3600 s; past it
the conversion fails with exit status 1 and no Markdown, and stderr says pdftotext ran past
its time limit; with --window or --stream the pages already written stay written):
  --text-timeout S            seconds for the run, before the per-page seconds (60; 0: none)
  --text-timeout-per-page P   seconds added per page of the range (1; 0: none)
  --text-timeout-max M        cap on the total (3600; 0: no cap); --text-timeout 0
                              --text-timeout-per-page 0 runs pdftotext without a limit
                              (each 0 to 86400)


image links (with --markers or --images-dir; they shape the Markdown):
  --images-link-prefix P      put before each image path in the links (images/); empty, or up to
                              128 bytes of A-Z a-z 0-9 . _ ~ / - ending in /, relative, no ..
  --raster-format webp|png    pictures (webp: lossy, lossless where the crop holds text)
  --vector-format svg|webp|png  vector figures (svg; a refused or too large SVG becomes a
                              lossless PNG inside an SVG, so the link stays valid)
  --images-repeats MODE       skip or link (a repeat of an earlier picture links to its file and
                              gets none; the manifest names it: repeat_of), or keep (its own
                              file) (default skip)
  --scan-pages MODE           a full-page scan (images cover --scan-min-coverage of the page,
                              at most --scan-max-words visible words): image (default) starts the page
                              with ![page N scan](images/page-0012/image-001.jpg), then its OCR
                              text; the file is the scan's own JPEG (.jpg), a bilevel scan as
                              1-bit .png, else decoded or rendered (--raster-format). render:
                              always rendered (--raster-format). skip: placed as any picture,
                              never extracted

image extraction (implies --markers; files where the links point: DIR/page-0012/image-003.webp,
DIR/page-0012/figure-001.svg, and DIR/manifest.jsonl, written last, listing every image and
file (JSON Lines, doppler-images/3: a header line, one line per image, one per batch, and an
end line with the line counts and the SHA-256 of the lines before it; a file without a valid
end line is incomplete); DIR and its page folders are created 0700 and files 0600 whatever the umask; stdout is
closed once the Markdown is complete, before extraction):
  --images-dir DIR            write the marked pictures and figures to DIR, which must not exist
  --images-min-side N[px|pt]  no file under this smallest side (24px: pixels at the dpi)
  --raster-quality N          lossy WebP quality (90)
  --raster-lossless           lossless WebP for every picture
  --report-timing             with --report: also images_ms and images_cpu_ms (not deterministic)
  --raster-method N           lossy WebP encoder effort, 0 (fastest) to 6 (4)
  --lossless-method N         lossless WebP encoder effort, 0 to 6 (4); with
  --lossless-quality N        lossless effort 0 to 100 (50): lower is faster, a little larger,
                              and decodes to the same pixels
  --svg-precision 1|2         decimals of a point in SVG coordinates (1)
  --svg-max-bytes N           largest SVG kept (512KiB)
  --images-dpi N              raster resolution, 36 to 600 (150)
  --images-batch-pages N      pages per extraction batch (64): one pdfregions each, with the
                              budgets below; a batch over budget keeps its files, the rest of
                              it is not-extracted (manifest: reason, cut_page) and the next goes on;
                              two failed batches in a row, or three in all (timeout, deadline,
                              renderer, the scans' budget) stop the rest (aborted); a stop by
                              --images-max or --images-max-bytes alone is not a failure, and its
                              images are resumable
  --images-max N              most files per batch (500); --images-max-bytes N (256MiB) per
                              batch; --images-max-file-bytes N (8MiB): that file only
  --images-timeout S          renderer time per batch (120)
  --images-doc-max-bytes N    most bytes for the document (4GiB); then the rest is not extracted
  --images-deadline S         time for the whole extraction (120 + 0.5 per converted page, at most
                              3600); then the rest is not extracted (deadline)
  --images-confine MODE       pdfregions -confine: require, auto or off (auto)
  --images-pages A-B          extract only figures on pages A to B (all); the Markdown still
                              covers every converted page
  --scan-dpi N                rendered scans (tiles, layers, turned, covered, --scan-pages
                              render): 36 to 300 (200)
  --scan-max-dpi N            a decoded scan finer than this is scaled down to it (300)
  --scan-max N                most scan files per batch (1000); --scan-max-bytes N (1GiB) per
                              batch. Scans have their own budget, apart from --images-max and
                              --images-max-bytes; a scan over it is not-extracted (budget) and the
                              batch goes on
  --images-plan-out FILE      with --markers (or --images-dir): also write the images plan (what
                              extraction needs from this conversion, exactly) to FILE, which must
                              not exist; its folder is reached without following symbolic links.
                              The Markdown and report are unchanged; exit status 6 when the plan
                              could not be written
  --images-plan FILE          with --images-dir: extract from a plan written by an earlier run,
                              without running pdftotext or converting again (no Markdown, no
                              --report); the same files and manifest as extracting after a
                              conversion. The plan is bound to that run: give the same FILE.pdf
                              (same bytes), -f/-l, --params, --strict, --stream/--window, --math,
                              --html, image link options and the --scan-... thresholds, and the same
                              pdftomarkdown version,
                              or it is refused (exit status 2)
  --images-resume DIR         instead of --images-dir: finish an earlier run's DIR, extracting
                              only its resumable images (stopped by a budget, time limit or the
                              abort rule) to the paths the Markdown links, never replacing a
                              file; the manifest is replaced (resumes + 1). Give the same FILE.pdf
                              and the same conversion and image options (or --images-plan PLAN);
                              the budgets, batch size, time limits and --images-confine may
                              change. A manifest.jsonl that is not doppler-images/3 (a version 2
                              manifest.json too), is truncated or altered (end line, counts,
                              SHA-256), names another PDF or other settings, or is missing is
                              refused (exit status 2, nothing touched); a hard failure keeps
                              the old manifest and files and removes only the new ones (6); 0
                              complete, 5 still partial
  --images-resume-pages A-B   with --images-resume: retry only the resumable images on pages A
                              to B (N for one); the others stay resumable for a later resume.
                              Not bound into the manifest's settings; exit 5 while any remain
  Exit status 5: Markdown written, images in part (DIR and manifest.jsonl kept; its end line:
  complete false; why per image and per batch). 6: Markdown written, extraction failed (DIR
  removed), or the images plan could not be written.

page facts (doppler-page-facts/1 JSON lines: a doc line, one line per page, a run line;
computed by pdftotext on the same pass, written unchanged, never on stdout; see
docslurp/page-facts/README.md):
  --page-facts FILE           write them to FILE, which must not exist (mode 0600; its folder
                              is reached without following symbolic links). FILE appears when
                              pdftotext has finished, before image extraction; a FILE
                              without a run line is incomplete (pdftotext failed)
  --page-facts-fd N           write them to descriptor N (3 or more), open for writing and
                              inherited from the caller (the image renderer does not keep it)
  A facts write that fails (the reader closed it, say) keeps the Markdown: exit status 6.
  pdftotext's time limit (--text-timeout) applies with or without them.
  --page-facts-level L        basic (default) or full
  --page-facts-include LIST   families added to basic: fonts,oddtext,images-detail,vectors,
                              forms,annots,ocg,structure
  --page-facts-timing         add factsMs, totalMs and peakRssKiB (not deterministic)

scan test (pdftotext's: changes which pages are scans in the Markdown, the images and the facts;
a changed threshold is recorded on the facts doc line, scanThresholds):
  --scan-min-coverage F       share of the page one image, or the images together, must cover,
                              in (0,1] (0.85)
  --scan-max-words N          a page with more visible words is not a scan (20)
  --scan-layer-overlap F      share of the page a second image on the scan must cover to make it
                              layered, in (0,1] (0.5)

pdftotext is $PDFTOTEXT, else pdftotext next to this executable, else pdftotext on PATH.
It must be the DocSlurp fork (-bbox-layout -wordmeta -rules -compact-layout). pdfregions
(image extraction) is found the same way ($PDFREGIONS).
";

/// pdftotext's time limit, always passed: `base` seconds plus `per_page` a page of the range,
/// at most `max` (0: none of that part; pdftotext exits with status 6 past it). Plain
/// pdftotext still has per-draw costs no other bound covers (a large direct XObject
/// dictionary is parsed again on every Do), so a run is never left unbounded by default.
struct TextTimeout {
    base: u32,
    per_page: u32,
    max: u32,
}

impl Default for TextTimeout {
    fn default() -> Self {
        TextTimeout { base: 60, per_page: 1, max: 3600 }
    }
}

impl TextTimeout {
    /// pdftotext's own range for each value.
    const MAX_SECONDS: u32 = 86400;

    fn apply(&mut self, option: &str, value: Option<&str>) -> Result<(), String> {
        let n = value
            .and_then(|v| v.parse::<u32>().ok())
            .filter(|n| *n <= Self::MAX_SECONDS)
            .ok_or_else(|| format!("{option} needs whole seconds from 0 to {}", Self::MAX_SECONDS))?;
        match option {
            "--text-timeout" => self.base = n,
            "--text-timeout-per-page" => self.per_page = n,
            _ => self.max = n,
        }
        Ok(())
    }

    fn pdftotext_args(&self) -> [OsString; 6] {
        [
            "-timeout".into(),
            self.base.to_string().into(),
            "-timeout-per-page".into(),
            self.per_page.to_string().into(),
            "-timeout-max".into(),
            self.max.to_string().into(),
        ]
    }
}

fn main() -> ExitCode {
    let mut params = Params::default();
    let mut opts = Options::default();
    let mut want_report = false;
    let mut strict = false;
    let mut window: Option<usize> = None;
    let mut page_sizes = false;
    // pdftotext options, in order
    let mut pass: Vec<OsString> = Vec::new();
    let mut passwords: Vec<OsString> = Vec::new();
    let mut images = Settings::default();
    let (mut first, mut last): (Option<i32>, Option<i32>) = (None, None);
    let mut input: Option<OsString> = None;
    let (mut plan_out, mut plan_in): (Option<OsString>, Option<OsString>) = (None, None);
    let (mut facts_path, mut facts_fd): (Option<OsString>, Option<u32>) = (None, None);
    let mut facts_opts: Vec<OsString> = Vec::new();
    let mut scan = ScanThresholds::default();
    let mut text_timeout = TextTimeout::default();
    let mut text_timeout_set = false;
    let mut args = std::env::args_os().skip(1);
    let mut options_done = false;
    while let Some(a) = args.next() {
        let s = if options_done { None } else { a.to_str() };
        let Some(s) = s else {
            if input.is_some() {
                return usage_error("too many arguments");
            }
            input = Some(a);
            continue;
        };
        match s {
            "-h" | "--help" => {
                print!("{USAGE}");
                return ExitCode::SUCCESS;
            }
            "--" => options_done = true,
            "--report" => want_report = true,
            "--html" => opts.tables = TableFormat::Html,
            "--math" => opts.math = true,
            "--markers" => opts.markers = true,
            "--page-info" => page_sizes = true,
            "--page-markers" => opts.page_markers = true,
            "--strict" => strict = true,
            "--stream" => window = Some(STREAM_WINDOW),
            "--window" => match cli::parse_window(args.next().as_deref().and_then(|w| w.to_str())) {
                Ok(w) => window = Some(w),
                Err(e) => return usage_error(&e),
            },
            "--params" => {
                let Some(json) = args.next().and_then(|j| j.into_string().ok()) else {
                    return usage_error("--params needs a JSON object");
                };
                if let Err(e) = params.apply_json(&json) {
                    eprintln!("pdftomarkdown: {e}");
                    return ExitCode::from(2);
                }
            }
            // poppler reads page numbers into an int with atoi: keep them in 1..=i32::MAX
            "-f" | "-l" => match args.next().and_then(|n| n.into_string().ok()).and_then(|n| n.parse::<i32>().ok()) {
                Some(n) if n >= 1 => {
                    *(if s == "-f" { &mut first } else { &mut last }) = Some(n);
                    pass.push(a);
                    pass.push(n.to_string().into());
                }
                _ => return usage_error(&format!("{s} needs a page number from 1 to {}", i32::MAX)),
            },
            "--opw" | "--upw" | "-opw" | "-upw" => {
                let Some(pw) = args.next() else {
                    return usage_error(&format!("{s} needs a password"));
                };
                pass.push(format!("-{}", s.trim_start_matches('-')).into());
                pass.push(pw.clone());
                passwords.push(format!("-{}", s.trim_start_matches('-')).into());
                passwords.push(pw);
            }
            "--page-facts" => {
                let Some(path) = args.next() else {
                    return usage_error("--page-facts needs a FILE");
                };
                facts_path = Some(path);
            }
            "--page-facts-fd" => {
                match args.next().and_then(|n| n.into_string().ok()).and_then(|n| n.parse::<u32>().ok()) {
                    Some(n) if (3..=i32::MAX as u32).contains(&n) => facts_fd = Some(n),
                    _ => return usage_error("--page-facts-fd needs a descriptor number of 3 or more"),
                }
            }
            "--page-facts-level" => match args.next().as_deref().and_then(|l| l.to_str()) {
                Some(l @ ("basic" | "full")) => facts_opts.extend(["-page-facts-level".into(), l.into()]),
                _ => return usage_error("--page-facts-level needs basic or full"),
            },
            "--page-facts-include" => {
                const FAMILIES: [&str; 8] =
                    ["fonts", "oddtext", "images-detail", "vectors", "forms", "annots", "ocg", "structure"];
                match args.next().and_then(|l| l.into_string().ok()) {
                    Some(l) if l.len() < 128 && l.split(',').all(|f| f.is_empty() || FAMILIES.contains(&f)) => {
                        facts_opts.extend(["-page-facts-include".into(), l.into()])
                    }
                    _ => return usage_error(&format!("--page-facts-include needs a list of {}", FAMILIES.join(","))),
                }
            }
            "--page-facts-timing" => facts_opts.push("-page-facts-timing".into()),
            "--scan-min-coverage" | "--scan-max-words" | "--scan-layer-overlap" => {
                let value = args.next().and_then(|v| v.into_string().ok());
                if let Err(e) = scan.apply(s, value.as_deref()) {
                    return usage_error(&e);
                }
            }
            "--text-timeout" | "--text-timeout-per-page" | "--text-timeout-max" => {
                let value = args.next().and_then(|v| v.into_string().ok());
                if let Err(e) = text_timeout.apply(s, value.as_deref()) {
                    return usage_error(&e);
                }
                text_timeout_set = true;
            }
            "--images-plan-out" | "--images-plan" => {
                let Some(path) = args.next() else {
                    return usage_error(&format!("{s} needs a FILE"));
                };
                *(if s == "--images-plan" { &mut plan_in } else { &mut plan_out }) = Some(path);
            }
            _ if Settings::takes(s).is_some() => {
                let value = if Settings::takes(s) == Some(true) { args.next() } else { None };
                let value = match value.map(|v| v.into_string()) {
                    Some(Ok(v)) => Some(v),
                    Some(Err(_)) => return usage_error(&format!("{s}: not valid UTF-8")),
                    None => None,
                };
                if let Err(e) = images.apply(s, value.as_deref()) {
                    return usage_error(&e);
                }
            }
            _ if s.starts_with('-') && s != "-" => return usage_error(&format!("unknown option {s}")),
            _ if input.is_none() => input = Some(a),
            _ => return usage_error("too many arguments"),
        }
    }
    if let (Some(f), Some(l)) = (first, last)
        && f > l
    {
        return usage_error(&format!("-f {f} is after -l {l}"));
    }
    let Some(input) = input else {
        return usage_error("no input FILE");
    };
    if let Err(e) = images.check() {
        return usage_error(&e);
    }
    opts.first_page = first.map(|n| n as u32);
    if images.dir.is_some() {
        if input == "-" {
            return usage_error("--images-dir needs a FILE, not standard input");
        }
        // pictures and figures are found through the markers
        opts.markers = true;
    }
    if images.link_flags() && !opts.markers {
        return usage_error(
            "--images-link-prefix, --images-repeats, --raster-format, --vector-format and --scan-pages need --markers \
             or --images-dir",
        );
    }
    if plan_in.is_some() && (images.dir.is_none() || want_report || plan_out.is_some()) {
        return usage_error("--images-plan needs --images-dir, and takes no --report or --images-plan-out");
    }
    if plan_out.is_some() && !opts.markers {
        // without markers the report has no images: the plan would be empty
        return usage_error("--images-plan-out needs --markers (or --images-dir)");
    }
    if facts_path.is_some() && facts_fd.is_some() {
        return usage_error("--page-facts and --page-facts-fd are exclusive");
    }
    let want_facts = facts_path.is_some() || facts_fd.is_some();
    if !facts_opts.is_empty() && !want_facts {
        return usage_error(
            "--page-facts-level, --page-facts-include and --page-facts-timing need --page-facts or --page-facts-fd",
        );
    }
    if want_facts && plan_in.is_some() {
        return usage_error("--images-plan runs no pdftotext: it takes no --page-facts");
    }
    if text_timeout_set && plan_in.is_some() {
        return usage_error("--images-plan runs no pdftotext: it takes no --text-timeout options");
    }
    if (plan_out.is_some() || plan_in.is_some()) && input == "-" {
        return usage_error("an images plan needs a FILE, not standard input");
    }
    // what an images plan is bound to, besides the PDF's bytes and the page range
    let settings = plan::settings_digest(&format!(
        "params {params:?}\nstrict {strict}\nwindow {window:?}\nmath {}\ntables {:?}\nmarkers {}\n{}{}",
        opts.math,
        opts.tables,
        opts.markers,
        images.link_style().canonical(),
        scan.canonical()
    ));
    let pages = (first.map(|n| n as u32), last.map(|n| n as u32));
    // what a manifest is bound to (a resume must repeat it): the conversion, the page range
    // and everything that shapes the files
    let files_settings = plan::settings_digest(&format!("{settings}\npages {pages:?}\n{}", images.output_canonical()));
    // name a file starting with '-' relative to the current directory, so pdftotext cannot read
    // it as an option ("-" is stdin to pdftotext)
    let file = if input.as_encoded_bytes().first() == Some(&b'-') && input != "-" {
        let mut f = OsString::from("./");
        f.push(&input);
        f
    } else {
        input
    };

    // the plan's file is checked before anything runs: it must be new, in a folder reached
    // without symbolic links
    let plan_file = match plan_out.as_deref().map(|p| NewFile::reserve(Path::new(p))) {
        Some(Ok(f)) => Some(f),
        Some(Err(e)) => {
            eprintln!("pdftomarkdown: --images-plan-out: {e}");
            return ExitCode::from(2);
        }
        None => None,
    };
    // so is the facts file's
    let facts_file = match facts_path.as_deref().map(|p| NewFile::reserve(Path::new(p)).and_then(FactsFile::create)) {
        Some(Ok(f)) => Some(f),
        Some(Err(e)) => {
            eprintln!("pdftomarkdown: --page-facts: {e}");
            return ExitCode::from(2);
        }
        None => None,
    };
    let facts_target = facts_file.as_ref().map(|f| f.raw_fd() as u32).or(facts_fd);
    if let Some(fd) = facts_target {
        let mut args: Vec<OsString> = vec!["-page-facts-fd".into(), fd.to_string().into()];
        args.append(&mut facts_opts);
        facts_opts = args;
    }
    facts_opts.extend(scan.pdftotext_args());
    // pdftotext always runs under a wall-clock bound scaled from the page count
    facts_opts.extend(text_timeout.pdftotext_args());
    // the folder is created before anything runs: it must be new (a resumed one must exist,
    // with a manifest bound to this PDF and these settings)
    let flag = if images.resume { "--images-resume" } else { "--images-dir" };
    let opened =
        images.dir.as_deref().map(|d| if images.resume { OutDir::open_existing(d) } else { OutDir::create(d) });
    let mut out_dir = match opened {
        Some(Ok(d)) => Some(d),
        Some(Err(e)) => {
            eprintln!("pdftomarkdown: {flag}: {e}");
            return ExitCode::from(2);
        }
        None => None,
    };
    let previous = match out_dir.as_ref().filter(|_| images.resume) {
        Some(d) => {
            // read line by line (doppler-images/3); a version 2 manifest.json is refused
            let read = d.open_read(manifest::NAME).and_then(|m| {
                let Some((m, len)) = m else {
                    return Err(if d.open_read(manifest::V2_NAME)?.is_some() {
                        format!(
                            "the folder has a version 2 {} (this version reads only {}, {}): extract again",
                            manifest::V2_NAME,
                            manifest::NAME,
                            manifest::SCHEMA
                        )
                    } else {
                        format!("the folder has no {}", manifest::NAME)
                    });
                };
                if len > resume::MAX_MANIFEST_BYTES {
                    return Err(format!("{} is larger than {} bytes", manifest::NAME, resume::MAX_MANIFEST_BYTES));
                }
                let r = std::io::BufReader::new(std::io::Read::take(m, resume::MAX_MANIFEST_BYTES));
                resume::read(r, &source_sha256(&file)?, &files_settings)
            });
            match read {
                Ok(p) => Some(p),
                Err(e) => {
                    eprintln!("pdftomarkdown: --images-resume: refused: {e}");
                    return ExitCode::from(2);
                }
            }
        }
        None => None,
    };
    let bound = (files_settings.as_str(), previous.as_ref());
    let code = match &plan_in {
        Some(plan) => extract_from_plan(plan, &file, pages, &settings, &images, &passwords, &mut out_dir, bound),
        None => convert(
            &params,
            &opts,
            window,
            strict,
            page_sizes,
            &pass,
            &file,
            want_report,
            &images,
            &passwords,
            &mut out_dir,
            plan_file.map(|f| (f, pages, settings)),
            &facts_opts,
            facts_file,
            bound,
        ),
    };
    // a folder left over is a failed or skipped extraction
    if let Some(d) = out_dir {
        let _ = d.remove();
    }
    code
}

/// Runs pdftotext, converts, and extracts the images when asked (`out_dir` is taken once
/// extraction succeeds).
#[allow(clippy::too_many_arguments)]
fn convert(
    params: &Params,
    opts: &Options,
    window: Option<usize>,
    strict: bool,
    page_sizes: bool,
    pass: &[OsString],
    file: &OsString,
    want_report: bool,
    images: &Settings,
    passwords: &[OsString],
    out_dir: &mut Option<OutDir>,
    plan_out: Option<(NewFile, (Option<u32>, Option<u32>), String)>,
    facts_opts: &[OsString],
    mut facts_file: Option<FactsFile>,
    (files_settings, previous): (&str, Option<&resume::Previous>),
) -> ExitCode {
    // the plan names the bytes converted: hash them first
    let plan_out = match plan_out.map(|(f, pages, settings)| binding(file, pages, settings).map(|b| (f, b))) {
        Some(Ok(p)) => Some(p),
        Some(Err(e)) => {
            eprintln!("pdftomarkdown: --images-plan-out: {e}");
            return ExitCode::FAILURE;
        }
        None => None,
    };
    let exe = pdftotext();
    // the facts descriptor is inherited by pdftotext only
    if let Some(f) = &facts_file
        && let Err(e) = f.set_inheritable(true)
    {
        eprintln!("pdftomarkdown: --page-facts: {e}");
        return ExitCode::FAILURE;
    }
    let child = Command::new(&exe)
        .args(["-q", "-bbox-layout", "-wordmeta", "-rules"])
        .args(if strict { None } else { Some("-compact-layout") })
        .args(if opts.math { Some("-glyphs") } else { None })
        .args(if opts.markers { Some("-images") } else { None })
        .args(if page_sizes { Some("-crop-sizes") } else { None })
        .args(pass)
        .args(facts_opts)
        .arg(file)
        .arg("-")
        .stdin(Stdio::inherit())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn();
    if let Some(f) = &facts_file
        && let Err(e) = f.set_inheritable(false)
    {
        eprintln!("pdftomarkdown: --page-facts: {e}");
        if let Ok(mut c) = child {
            let _ = c.kill();
            let _ = c.wait();
        }
        return ExitCode::FAILURE;
    }
    let mut child = match child {
        Ok(c) => c,
        Err(e) => {
            eprintln!("pdftomarkdown: cannot run {}: {e}", exe.display());
            return ExitCode::FAILURE;
        }
    };
    let no_page = format!(
        "no <page> in the output of {} (probably not the DocSlurp pdftotext, which needs \
         -bbox-layout -wordmeta -rules)",
        exe.display()
    );
    let mut run = cli::Run::new(params, opts, window);
    run.set_link_style(images.link_style());
    let read = match child.stdout.take() {
        Some(mut out) => run.read_from(&mut out),
        None => Ok(()),
    };
    // the pipe is closed here: after a write error on our stdout pdftotext stops on EPIPE
    // instead of blocking, and its status no longer matters
    if run.write_failed() {
        let _ = child.kill();
        let _ = child.wait();
        commit_facts(facts_file.take());
        return run.finish("pdftomarkdown", want_report, &no_page);
    }
    let status = match child.wait() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("pdftomarkdown: cannot wait for {}: {e}", exe.display());
            return ExitCode::FAILURE;
        }
    };
    // the facts file appears as soon as pdftotext is done, before image extraction, whatever
    // the conversion's outcome (its lines say what was read; one without a run line is
    // incomplete); one pdftotext never wrote to is removed
    let mut facts_failed = !commit_facts(facts_file.take());
    // pdftotext's exit status 4: the XHTML is complete but the facts could not be written (a
    // reader that closed its end, say). The Markdown stands: exit status 6 at the end.
    let facts_requested = !facts_opts.is_empty() && facts_opts[0] == "-page-facts-fd";
    let status_ok = status.success() || (facts_requested && status.code() == Some(4));
    if !status.success() && status_ok {
        eprintln!("pdftomarkdown: {} could not write the page facts", exe.display());
        facts_failed = true;
    }
    if let Err(e) = read {
        eprintln!("pdftomarkdown: reading the output of {}: {e}", exe.display());
        return ExitCode::FAILURE;
    }
    // a failed pdftotext leaves stdout empty, except for the pages a --window run already wrote
    if !status_ok {
        match status.code() {
            // 99 is poppler's usage error: a page range past the end of the document, or a
            // pdftotext without the fork's options
            Some(99) => eprintln!(
                "pdftomarkdown: {} exited with status 99 (bad arguments: a page range past the last \
                 page, or not the DocSlurp pdftotext, which needs -bbox-layout -wordmeta -rules \
                 -compact-layout)",
                exe.display()
            ),
            // -q hides poppler's message; name its documented exit codes instead
            Some(1) => eprintln!(
                "pdftomarkdown: {} could not open or decrypt {} (status 1)",
                exe.display(),
                Path::new(file).display()
            ),
            Some(3) => eprintln!("pdftomarkdown: {} was refused by the PDF's permissions (status 3)", exe.display()),
            // pdftotext's 6 is its -timeout (always passed): the conversion fails (exit status 1,
            // like any failed pdftotext), never pdftomarkdown's own 6, which says the Markdown
            // was written. A fixed message: callers may match it.
            Some(6) => eprintln!(
                "pdftomarkdown: pdftotext ran past its time limit (status 6; see --text-timeout); no Markdown"
            ),
            Some(n) => eprintln!("pdftomarkdown: {} exited with status {n}", exe.display()),
            None => eprintln!("pdftomarkdown: {} {}", exe.display(), status),
        }
        return ExitCode::FAILURE;
    }
    let (mut report, written) = run.finish_report();
    // a page facts file that could not be written: the Markdown stands, exit status 6
    let mut code = if facts_failed { Some(6) } else { None };
    if let Some((f, b)) = plan_out
        && written.is_ok()
        && report.page_found()
        && let Err(e) = f.write(plan::write(&b, report.parse.pages, &report.images).as_bytes())
    {
        // the Markdown and report stand; the plan asked for is missing: exit status 6
        eprintln!("pdftomarkdown: --images-plan-out: {e}");
        code = Some(6);
    }
    if let Some(dir) = out_dir.as_mut()
        && written.is_ok()
        && report.page_found()
    {
        // the Markdown is complete and its links final: EOF on stdout says so, before the
        // images are extracted (the report and exit status still come after)
        close_stdout();
        let r = source_sha256(file).and_then(|sha| {
            doppler_md::extract::extract(
                &mut report,
                images,
                &sha,
                files_settings,
                previous,
                &renderer(file, images, passwords),
                dir,
            )
        });
        match r {
            Ok(summary) => {
                // the folder stays, complete or not: its manifest says what it holds
                *out_dir = None;
                if !summary.complete && code.is_none() {
                    code = Some(5);
                }
            }
            Err(e) => {
                eprintln!("pdftomarkdown: image extraction failed: {e}");
                for n in report.images.iter_mut() {
                    n.file = None;
                }
                report.images_written = Some(0);
                report.images_error = Some(e);
                code = Some(6);
            }
        }
    }
    let conclusion = cli::conclude("pdftomarkdown", &report, written, want_report, &no_page);
    match code {
        Some(c) if conclusion == ExitCode::SUCCESS => ExitCode::from(c),
        _ => conclusion,
    }
}

/// Points stdout at /dev/null (the pages are flushed): a reader sees EOF while the process
/// goes on. Nothing is written to stdout afterwards; children never inherit it.
fn close_stdout() {
    use std::io::Write as _;
    let _ = std::io::stdout().flush();
    match std::fs::OpenOptions::new().write(true).open("/dev/null") {
        Ok(null) => {
            if let Err(e) = rustix::stdio::dup2_stdout(&null) {
                eprintln!("pdftomarkdown: cannot close stdout: {e}");
            }
        }
        Err(e) => eprintln!("pdftomarkdown: cannot close stdout: {e}"),
    }
}

/// SHA-256 (hex) of the PDF, for the manifest.
fn source_sha256(file: &OsString) -> Result<String, String> {
    std::fs::File::open(file)
        .and_then(|mut f| plan::source_digest(&mut f))
        .map(|(h, _)| h)
        .map_err(|e| format!("reading {}: {e}", Path::new(file).display()))
}

/// Renames the facts file into place if pdftotext wrote to it (an empty one is removed), without
/// an unterminated last line (one with no whole line is removed); false when that failed.
fn commit_facts(facts_file: Option<FactsFile>) -> bool {
    match facts_file {
        Some(f) if f.written() => match f.commit() {
            Ok(false) => true,
            Ok(true) => {
                eprintln!(
                    "pdftomarkdown: --page-facts: dropped an unterminated last line (pdftotext stopped mid-line)"
                );
                true
            }
            Err(e) => {
                eprintln!("pdftomarkdown: --page-facts: {e}");
                false
            }
        },
        _ => true,
    }
}

fn renderer(file: &OsString, images: &Settings, passwords: &[OsString]) -> Renderer {
    Renderer {
        exe: regions::locate(),
        passwords: passwords.to_vec(),
        file: file.clone(),
        confine: images.confine,
        max_svg_bytes: doppler_md::extract::MAX_SVG_INPUT as u64,
        // raw RGB is several times the encoded files: 8 times the pictures' byte budget plus 4
        // times the scans' (decoded scans are raw samples), at least 64 MiB. It counts streamed
        // bytes; the renderer's memory stays one record
        max_output_bytes: images
            .max_bytes
            .saturating_mul(8)
            .saturating_add(images.scan_max_bytes.saturating_mul(4))
            .clamp(64 << 20, 64 << 30),
        // a copied JPEG is one file: at most 64 MiB, and within the scan budget
        max_copy_bytes: images.scan_max_bytes.min(64 << 20),
    }
}

/// What a plan written or read by this run is bound to: `file`'s SHA-256 and size, the page
/// range, the conversion settings' digest and this version.
fn binding(
    file: &OsString,
    (first, last): (Option<u32>, Option<u32>),
    settings: String,
) -> Result<plan::Binding, String> {
    let (source_sha256, source_len) = std::fs::File::open(file)
        .and_then(|mut f| plan::source_digest(&mut f))
        .map_err(|e| format!("reading {}: {e}", Path::new(file).display()))?;
    Ok(plan::Binding { source_sha256, source_len, first, last, settings, version: env!("CARGO_PKG_VERSION").into() })
}

/// `--images-plan`: extraction from a plan, without pdftotext or a conversion. Exit status as
/// after a conversion: 0, 5 when the images were extracted in part (DIR kept), 6 when
/// extraction failed (DIR removed); 2 for an unreadable plan or one bound to another PDF, page
/// range, settings or version.
#[allow(clippy::too_many_arguments)]
fn extract_from_plan(
    plan_path: &OsString,
    file: &OsString,
    pages: (Option<u32>, Option<u32>),
    settings: &str,
    images: &Settings,
    passwords: &[OsString],
    out_dir: &mut Option<OutDir>,
    (files_settings, previous): (&str, Option<&resume::Previous>),
) -> ExitCode {
    use std::io::Read as _;
    let mut text = String::new();
    let read = std::fs::File::open(plan_path)
        .and_then(|f| f.take(plan::MAX_PLAN_BYTES + 1).read_to_string(&mut text))
        .map_err(|e| e.to_string())
        .and_then(|n| if n as u64 > plan::MAX_PLAN_BYTES { Err("too large".into()) } else { Ok(()) })
        .and_then(|()| plan::read(&text))
        .and_then(|(bound, converted, notes)| {
            let this = binding(file, pages, settings.to_string())?;
            match this.mismatch(&bound) {
                Some(m) => Err(format!("refused: the plan was {m}")),
                None => Ok((converted, notes)),
            }
        });
    let (converted, notes) = match read {
        Ok(n) => n,
        Err(e) => {
            eprintln!("pdftomarkdown: --images-plan: {e}");
            return ExitCode::from(2);
        }
    };
    // the converted page count scales the default --images-deadline, as after the conversion
    let parse = doppler_md::ParseReport { pages: converted, ..Default::default() };
    let mut report = doppler_md::Report { images: notes, parse, ..Default::default() };
    let Some(dir) = out_dir.as_mut() else { return ExitCode::from(2) };
    let r = source_sha256(file).and_then(|sha| {
        doppler_md::extract::extract(
            &mut report,
            images,
            &sha,
            files_settings,
            previous,
            &renderer(file, images, passwords),
            dir,
        )
    });
    match r {
        Ok(summary) => {
            *out_dir = None;
            if summary.complete { ExitCode::SUCCESS } else { ExitCode::from(5) }
        }
        Err(e) => {
            eprintln!("pdftomarkdown: image extraction failed: {e}");
            ExitCode::from(6)
        }
    }
}

/// `$PDFTOTEXT`, else `pdftotext` beside this executable, else `pdftotext` (looked up on `PATH`).
fn pdftotext() -> PathBuf {
    if let Some(p) = std::env::var_os("PDFTOTEXT").filter(|p| !p.is_empty()) {
        return PathBuf::from(p);
    }
    let name = format!("pdftotext{}", std::env::consts::EXE_SUFFIX);
    if let Some(beside) = std::env::current_exe().ok().and_then(|e| Some(e.parent()?.join(&name)))
        && beside.is_file()
    {
        return beside;
    }
    PathBuf::from(name)
}

fn usage_error(msg: &str) -> ExitCode {
    eprint!("pdftomarkdown: {msg}\n{USAGE}");
    ExitCode::from(2)
}
