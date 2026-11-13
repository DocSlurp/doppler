//! `pdftomarkdown [--html] [--strict] [--report] [--params JSON] [-f N] [-l N] [--opw PW]
//! [--upw PW] [--] FILE.pdf`: PDF to Markdown in one step. Runs the DocSlurp fork's
//! `pdftotext -q -bbox-layout -wordmeta -rules -compact-layout FILE -`, converts its XHTML as
//! it arrives and writes the pages joined with form feeds on stdout. By default every page
//! waits for the whole document; `--stream` (or `--window N`) writes each page once the next N
//! are complete and the body sample (`--body-pages`, 16 pages with text) is taken, holding the
//! larger of the two. `--context N` (default: the window) reads N pages around a `-f`/`-l`
//! range for repetition without writing them; with `--body-size`/`--body-color` the range's
//! pages are the same bytes as in a whole run with the same window. `--strict` drops
//! `-compact-layout` (full-precision coordinates): the same bytes as piping
//! `pdftotext -bbox-layout -wordmeta -rules` into `doppler-md`.
//!
//! `--images-dir DIR` also writes the marked pictures and figures (rendered by `pdfregions`)
//! to DIR; exit status 5 when the Markdown was written but image extraction failed.
//!
//! pdftotext is `$PDFTOTEXT` if set, else a `pdftotext` next to this executable, else
//! `pdftotext` on `PATH`. Exit status 2 for bad arguments or output without a `<page>` (a
//! pdftotext that is not the fork), 1 when pdftotext cannot be run or exits non-zero.

#![forbid(unsafe_code)]

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};

use doppler_md::extract::outdir::{NewFile, OutDir};
use doppler_md::extract::plan;
use doppler_md::extract::regions::{self, Renderer};
use doppler_md::extract::settings::Settings;
use doppler_md::{Options, Params, TableFormat};

#[path = "../cli.rs"]
mod cli;

const USAGE: &str = "\
usage: pdftomarkdown [--html] [--math] [--markers] [--strict] [--stream | --window N] [--body-pages N]
                     [--body-size PT] [--body-color C] [--report] [--params JSON] [-f N] [-l N] [--context N]
                     [--estimate-body] [--opw PW] [--upw PW] [--images-dir DIR [image options]] [--] FILE.pdf

  --html         write tables as HTML <table> blocks (prose stays Markdown)
  --math         read typeset math (pdftotext -glyphs) and write it as LaTeX: $...$ inline,
                 $$...$$ display; doubtful formulas stay text (report: mathUncertainPages)
  --markers      mark where pictures sit in the text for restitching: ![image](doppler-image:pN.M)
                 between paragraphs or inside a line, and charts and diagrams drawn with paths:
                 ![image](doppler-image:pN.gK) (pdftotext -images); report: images
%WINDOW%  --page-info    report: page_info, per page its crop box size as pdfinfo prints it, its word and
                 invisible-word counts, and its text layer (none, invisible, visible or mixed)
  --strict       full-precision layout coordinates (pdftotext without -compact-layout):
                 the same output as `pdftotext -bbox-layout -wordmeta -rules | doppler-md`
  --report       print a one-line JSON report to stderr (it always has the body_size and
                 body_color used, and with -f/-l a range object: see --context)
  --params JSON  override layout tuning parameters
  -f N, -l N     first and last page to convert
  --context N    with -f/-l: also read the N pages before -f and after -l (within the document),
                 0 to 64 (default: the --window, 0 without one), for repetition and the body
                 estimate; they are not written or reported (with --markers the pages before
                 are laid out, charged to the document's work budget). With N at least the
                 window and --body-size/--body-color from the whole document, the pages are
                 the same bytes as in a whole-document run with the same window; the report's
                 range.identity lists what can still differ
  --estimate-body  print the body estimate (JSON: body_size, body_color, ...) on stdout
                 and stop once it is fixed (reads only the sample pages, or every page read
                 when the sample is longer); no Markdown. Give the values back exactly
  --opw, --upw   owner and user password (also -opw, -upw); as with pdftotext they are
                 visible in the process list


image extraction (implies --markers; files named from the ids: DIR/p3.1.webp, DIR/p3.g1.svg,
and DIR/manifest.json listing every image):
  --images-dir DIR            write the marked pictures and figures to DIR, which must not exist
  --images-min-side N[px|pt]  no file under this smallest side (24px: pixels at the dpi)
  --images-repeats MODE       skip (no file for a repeat of an earlier figure), link (manifest
                              points to the first file) or keep (default skip)
  --raster-format webp|png    pictures (webp: lossy, lossless where the crop holds text)
  --raster-quality N          lossy WebP quality (90)
  --raster-lossless           lossless WebP for every picture
  --raster-method N           lossy WebP encoder effort, 0 (fastest) to 6 (4)
  --lossless-method N         lossless WebP encoder effort, 0 to 6 (4); with
  --lossless-quality N        lossless effort 0 to 100 (50): lower is faster, a little larger,
                              and decodes to the same pixels
  --vector-format svg|webp|png  vector figures (svg; a refused or too large SVG becomes
                              lossless WebP)
  --svg-precision 1|2         decimals of a point in SVG coordinates (1)
  --svg-max-bytes N           largest SVG kept (512KiB)
  --images-dpi N              raster resolution, 36 to 600 (150)
  --images-max N              most files (500); --images-max-bytes N (256MiB); both fail
                              the extraction; --images-max-file-bytes N (8MiB): that file only
  --images-timeout S          renderer time for the document (120)
  --images-confine MODE       pdfregions -confine: require, auto or off (auto)
  --images-pages A-B          extract only figures on pages A to B (all); the Markdown still
                              covers every converted page
  --images-plan-out FILE      with --markers (or --images-dir): also write the images plan (what
                              extraction needs from this conversion, exactly) to FILE, which must
                              not exist; its folder is reached without following symbolic links.
                              The Markdown and report are unchanged; exit status 5 when the plan
                              could not be written
  --images-plan FILE          with --images-dir: extract from a plan written by an earlier run,
                              without running pdftotext or converting again (no Markdown, no
                              --report); the same files and manifest as extracting after a
                              conversion. The plan is bound to that run: give the same FILE.pdf
                              (same bytes), -f/-l, --params, --strict, --stream/--window, --math
                              and --html, and the same pdftomarkdown version, or it is refused
                              (exit status 2)
  Exit status 5: Markdown written, image extraction failed (DIR removed).

pdftotext is $PDFTOTEXT, else pdftotext next to this executable, else pdftotext on PATH.
It must be the DocSlurp fork (-bbox-layout -wordmeta -rules -compact-layout). pdfregions
(image extraction) is found the same way ($PDFREGIONS).
";

fn main() -> ExitCode {
    let mut params = Params::default();
    let mut opts = Options::default();
    let mut want_report = false;
    let mut strict = false;
    let mut win = cli::WindowArgs::default();
    let mut context: Option<usize> = None;
    let mut estimate = false;
    let mut page_sizes = false;
    // pdftotext options, in order
    let mut pass: Vec<OsString> = Vec::new();
    let mut passwords: Vec<OsString> = Vec::new();
    let mut images = Settings::default();
    let (mut first, mut last): (Option<i32>, Option<i32>) = (None, None);
    let mut input: Option<OsString> = None;
    let (mut plan_out, mut plan_in): (Option<OsString>, Option<OsString>) = (None, None);
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
        let mut next = || args.next().and_then(|v| v.into_string().ok());
        match win.take(s, &mut next) {
            Some(Ok(())) => continue,
            Some(Err(e)) => return usage_error(&e),
            None => {}
        }
        match s {
            "-h" | "--help" => {
                print!("{}", USAGE.replace("%WINDOW%", cli::WINDOW_HELP));
                return ExitCode::SUCCESS;
            }
            "--" => options_done = true,
            "--report" => want_report = true,
            "--html" => opts.tables = TableFormat::Html,
            "--math" => opts.math = true,
            "--markers" => opts.markers = true,
            "--page-info" => page_sizes = true,
            "--strict" => strict = true,
            "--estimate-body" => estimate = true,
            "--context" => match args.next().and_then(|n| n.into_string().ok()).and_then(|n| n.parse::<usize>().ok()) {
                Some(n) if n <= cli::MAX_CONTEXT => context = Some(n),
                _ => return usage_error(&format!("--context needs a number of pages from 0 to {}", cli::MAX_CONTEXT)),
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
                Some(n) if n >= 1 => *(if s == "-f" { &mut first } else { &mut last }) = Some(n),
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
    if images.dir.is_some() {
        if input == "-" {
            return usage_error("--images-dir needs a FILE, not standard input");
        }
        // pictures and figures are found through the markers
        opts.markers = true;
    }
    if estimate && (images.dir.is_some() || plan_in.is_some() || plan_out.is_some()) {
        return usage_error("--estimate-body takes no image options");
    }
    // the page range pdftotext reads: the range and its context, within the document (pdftotext
    // clamps -l to the last page)
    let mut window = win.window();
    // by default a range reads the furniture window either side, so its pages are judged as in
    // a whole run
    let context = context.unwrap_or(win.furniture().unwrap_or(0)).min(cli::MAX_CONTEXT) as i32;
    if let Some(f) = first {
        let read = f.saturating_sub(context).max(1);
        window.skip = (f - read) as usize;
        window.first_page = read as u32;
        pass.push("-f".into());
        pass.push(read.to_string().into());
    }
    if let Some(l) = last {
        window.emit = Some((l - first.unwrap_or(1) + 1) as usize);
        window.after = context as usize;
        pass.push("-l".into());
        pass.push(l.saturating_add(context).to_string().into());
    }
    if plan_in.is_some() && (images.dir.is_none() || want_report || plan_out.is_some()) {
        return usage_error("--images-plan needs --images-dir, and takes no --report or --images-plan-out");
    }
    if plan_out.is_some() && !opts.markers {
        // without markers the report has no images: the plan would be empty
        return usage_error("--images-plan-out needs --markers (or --images-dir)");
    }
    if (plan_out.is_some() || plan_in.is_some()) && input == "-" {
        return usage_error("an images plan needs a FILE, not standard input");
    }
    // what an images plan is bound to, besides the PDF's bytes and the page range
    let settings = plan::settings_digest(&format!(
        "params {params:?}\nstrict {strict}\nwindow {window:?}\ncontext {context}\nmath {}\ntables {:?}\nmarkers {}\n",
        opts.math, opts.tables, opts.markers
    ));
    let pages = (first.map(|n| n as u32), last.map(|n| n as u32));
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
    // the folder is created before anything runs: it must be new
    let mut out_dir = match images.dir.as_deref().map(OutDir::create) {
        Some(Ok(d)) => Some(d),
        Some(Err(e)) => {
            eprintln!("pdftomarkdown: --images-dir: {e}");
            return ExitCode::from(2);
        }
        None => None,
    };
    let code = match &plan_in {
        Some(plan) => extract_from_plan(plan, &file, pages, &settings, &images, &passwords, &mut out_dir),
        None => convert(
            &params,
            &opts,
            window,
            estimate,
            strict,
            page_sizes,
            &pass,
            &file,
            want_report,
            &images,
            &passwords,
            &mut out_dir,
            plan_file.map(|f| (f, pages, settings)),
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
    window: doppler_md::Window,
    estimate: bool,
    strict: bool,
    page_sizes: bool,
    pass: &[OsString],
    file: &OsString,
    want_report: bool,
    images: &Settings,
    passwords: &[OsString],
    out_dir: &mut Option<OutDir>,
    plan_out: Option<(NewFile, (Option<u32>, Option<u32>), String)>,
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
    let child = Command::new(&exe)
        .args(["-q", "-bbox-layout", "-wordmeta", "-rules"])
        .args(if strict { None } else { Some("-compact-layout") })
        .args(if opts.math { Some("-glyphs") } else { None })
        .args(if opts.markers { Some("-images") } else { None })
        .args(if page_sizes { Some("-crop-sizes") } else { None })
        .args(pass)
        .arg(file)
        .arg("-")
        .stdin(Stdio::inherit())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn();
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
    let window = if estimate { doppler_md::Window { emit: Some(0), ..window } } else { window };
    let mut run = cli::Run::new(params, opts, window);
    if estimate {
        run.stop_at_body();
    }
    let read = match child.stdout.take() {
        Some(mut out) => run.read_from(&mut out),
        None => Ok(()),
    };
    if estimate && read.is_ok() && run.body_known() {
        // the sample is complete: the rest of the document is not needed
        let _ = child.kill();
        let _ = child.wait();
        let (report, _) = run.finish_report();
        println!("{}", report.body_to_json());
        return ExitCode::SUCCESS;
    }
    // the pipe is closed here: after a write error on our stdout pdftotext stops on EPIPE
    // instead of blocking, and its status no longer matters
    if run.write_failed() {
        let _ = child.kill();
        let _ = child.wait();
        return run.finish("pdftomarkdown", want_report, &no_page);
    }
    let status = match child.wait() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("pdftomarkdown: cannot wait for {}: {e}", exe.display());
            return ExitCode::FAILURE;
        }
    };
    if let Err(e) = read {
        eprintln!("pdftomarkdown: reading the output of {}: {e}", exe.display());
        return ExitCode::FAILURE;
    }
    // a failed pdftotext leaves stdout empty, except for the pages a --window run already wrote
    if !status.success() {
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
            Some(n) => eprintln!("pdftomarkdown: {} exited with status {n}", exe.display()),
            None => eprintln!("pdftomarkdown: {} {}", exe.display(), status),
        }
        return ExitCode::FAILURE;
    }
    let (mut report, written) = run.finish_report();
    if estimate {
        // every page read is context here (none is written): the estimate stands when any was
        // read. The sample ended with the input (a short document or range, or no window)
        if report.pages_read == 0 {
            eprintln!("pdftomarkdown: {no_page}");
            return ExitCode::from(2);
        }
        println!("{}", report.body_to_json());
        return ExitCode::SUCCESS;
    }
    let mut failed = false;
    if let Some((f, b)) = plan_out
        && written.is_ok()
        && report.page_found()
        && let Err(e) = f.write(plan::write(&b, &report.images).as_bytes())
    {
        // the Markdown and report stand; the plan asked for is missing: exit status 5
        eprintln!("pdftomarkdown: --images-plan-out: {e}");
        failed = true;
    }
    if let Some(dir) = out_dir.as_mut()
        && written.is_ok()
        && report.page_found()
    {
        let renderer = renderer(file, images, passwords);
        match doppler_md::extract::extract(&mut report, images, &renderer, dir) {
            Ok(_) => *out_dir = None,
            Err(e) => {
                eprintln!("pdftomarkdown: image extraction failed: {e}");
                for n in report.images.iter_mut() {
                    n.file = None;
                }
                report.images_written = Some(0);
                report.images_error = Some(e);
                failed = true;
            }
        }
    }
    let code = cli::conclude("pdftomarkdown", &report, written, want_report, &no_page);
    if failed && code == ExitCode::SUCCESS {
        return ExitCode::from(5);
    }
    code
}

fn renderer(file: &OsString, images: &Settings, passwords: &[OsString]) -> Renderer {
    Renderer {
        exe: regions::locate(),
        passwords: passwords.to_vec(),
        file: file.clone(),
        confine: images.confine,
        max_svg_bytes: doppler_md::extract::MAX_SVG_INPUT as u64,
        // raw RGB is several times the encoded files: 8 times the file cap, 64 MiB..2 GiB
        max_output_bytes: images.max_bytes.saturating_mul(8).clamp(64 << 20, 2 << 30),
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
/// after a conversion: 0, or 5 when extraction failed (DIR removed); 2 for an unreadable plan
/// or one bound to another PDF, page range, settings or version.
fn extract_from_plan(
    plan_path: &OsString,
    file: &OsString,
    pages: (Option<u32>, Option<u32>),
    settings: &str,
    images: &Settings,
    passwords: &[OsString],
    out_dir: &mut Option<OutDir>,
) -> ExitCode {
    use std::io::Read as _;
    let mut text = String::new();
    let read = std::fs::File::open(plan_path)
        .and_then(|f| f.take(plan::MAX_PLAN_BYTES + 1).read_to_string(&mut text))
        .map_err(|e| e.to_string())
        .and_then(|n| if n as u64 > plan::MAX_PLAN_BYTES { Err("too large".into()) } else { Ok(()) })
        .and_then(|()| plan::read(&text))
        .and_then(|(bound, notes)| {
            let this = binding(file, pages, settings.to_string())?;
            match this.mismatch(&bound) {
                Some(m) => Err(format!("refused: the plan was {m}")),
                None => Ok(notes),
            }
        });
    let notes = match read {
        Ok(n) => n,
        Err(e) => {
            eprintln!("pdftomarkdown: --images-plan: {e}");
            return ExitCode::from(2);
        }
    };
    let mut report = doppler_md::Report { images: notes, ..Default::default() };
    let Some(dir) = out_dir.as_mut() else { return ExitCode::from(2) };
    match doppler_md::extract::extract(&mut report, images, &renderer(file, images, passwords), dir) {
        Ok(_) => {
            *out_dir = None;
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("pdftomarkdown: image extraction failed: {e}");
            ExitCode::from(5)
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
    eprint!("pdftomarkdown: {msg}\n{}", USAGE.replace("%WINDOW%", cli::WINDOW_HELP));
    ExitCode::from(2)
}
