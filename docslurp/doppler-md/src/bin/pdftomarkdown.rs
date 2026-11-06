//! `pdftomarkdown [--html] [--strict] [--report] [--params JSON] [-f N] [-l N] [--opw PW]
//! [--upw PW] [--] FILE.pdf`: PDF to Markdown in one step. Runs the DocSlurp fork's
//! `pdftotext -q -bbox-layout -wordmeta -rules -compact-layout FILE -`, converts its XHTML in
//! memory and writes the pages joined with form feeds on stdout. `--strict` drops
//! `-compact-layout` (full-precision coordinates): the same bytes as piping
//! `pdftotext -bbox-layout -wordmeta -rules` into `doppler-md`.
//!
//! pdftotext is `$PDFTOTEXT` if set, else a `pdftotext` next to this executable, else
//! `pdftotext` on `PATH`. Exit status 2 for bad arguments or output without a `<page>` (a
//! pdftotext that is not the fork), 1 when pdftotext cannot be run or exits non-zero.

#![forbid(unsafe_code)]

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};

use doppler_md::{Options, Params, TableFormat};

#[path = "../cli.rs"]
mod cli;

const USAGE: &str = "\
usage: pdftomarkdown [--html] [--strict] [--report] [--params JSON] [-f N] [-l N]
                     [--opw PW] [--upw PW] [--] FILE.pdf

  --html         write tables as HTML <table> blocks (prose stays Markdown)
  --math         read typeset math (pdftotext -glyphs) and write it as LaTeX: $...$ inline,
                 $$...$$ display; doubtful formulas stay text (report: mathUncertainPages)
  --strict       full-precision layout coordinates (pdftotext without -compact-layout):
                 the same output as `pdftotext -bbox-layout -wordmeta -rules | doppler-md`
  --report       print a one-line JSON report to stderr
  --params JSON  override layout tuning parameters
  -f N, -l N     first and last page to convert
  --opw, --upw   owner and user password (also -opw, -upw); as with pdftotext they are
                 visible in the process list

pdftotext is $PDFTOTEXT, else pdftotext next to this executable, else pdftotext on PATH.
It must be the DocSlurp fork (-bbox-layout -wordmeta -rules -compact-layout).
";

fn main() -> ExitCode {
    let mut params = Params::default();
    let mut opts = Options::default();
    let mut want_report = false;
    let mut strict = false;
    // pdftotext options, in order
    let mut pass: Vec<OsString> = Vec::new();
    let (mut first, mut last): (Option<i32>, Option<i32>) = (None, None);
    let mut input: Option<OsString> = None;
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
            "--strict" => strict = true,
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
                pass.push(pw);
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
    // name a file starting with '-' relative to the current directory, so pdftotext cannot read
    // it as an option ("-" is stdin to pdftotext)
    let file = if input.as_encoded_bytes().first() == Some(&b'-') && input != "-" {
        let mut f = OsString::from("./");
        f.push(&input);
        f
    } else {
        input
    };

    let exe = pdftotext();
    let output = Command::new(&exe)
        .args(["-q", "-bbox-layout", "-wordmeta", "-rules"])
        .args(if strict { None } else { Some("-compact-layout") })
        .args(if opts.math { Some("-glyphs") } else { None })
        .args(&pass)
        .arg(&file)
        .arg("-")
        .stdin(Stdio::inherit())
        .stderr(Stdio::inherit())
        .output();
    let output = match output {
        Ok(o) => o,
        Err(e) => {
            eprintln!("pdftomarkdown: cannot run {}: {e}", exe.display());
            return ExitCode::FAILURE;
        }
    };
    if !output.status.success() {
        match output.status.code() {
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
                Path::new(&file).display()
            ),
            Some(3) => eprintln!("pdftomarkdown: {} was refused by the PDF's permissions (status 3)", exe.display()),
            Some(n) => eprintln!("pdftomarkdown: {} exited with status {n}", exe.display()),
            None => eprintln!("pdftomarkdown: {} {}", exe.display(), output.status),
        }
        return ExitCode::FAILURE;
    }
    let no_page = format!(
        "no <page> in the output of {} (probably not the DocSlurp pdftotext, which needs \
         -bbox-layout -wordmeta -rules)",
        exe.display()
    );
    cli::convert("pdftomarkdown", &output.stdout, &params, &opts, want_report, &no_page)
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
