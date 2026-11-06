//! `doppler-md [--html] [--params JSON] [--report] [FILE]`: doppler XHTML (file or stdin) to
//! Markdown pages joined with form feeds on stdout.
//!
//! Exit status 2 (with a message on stderr) when the input has no `<page>`, which means it is
//! not doppler XHTML. `--report` prints a one-line JSON report of skipped words and rules,
//! unclosed elements and pages read as plain lines to stderr. `--html` writes tables as HTML
//! `<table>` blocks instead of GFM pipe tables.

#![forbid(unsafe_code)]

use std::io::Read;
use std::process::ExitCode;

use doppler_md::{Options, Params, TableFormat};

mod cli;

const USAGE: &str = "usage: doppler-md [--html] [--math] [--params JSON] [--report] [FILE|-]\n";

fn main() -> ExitCode {
    let mut params = Params::default();
    let mut opts = Options::default();
    let mut input: Option<String> = None;
    let mut want_report = false;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "-h" | "--help" => {
                print!("{USAGE}");
                return ExitCode::SUCCESS;
            }
            "--report" => want_report = true,
            "--html" => opts.tables = TableFormat::Html,
            "--math" => opts.math = true,
            "--params" => {
                let Some(json) = args.next() else {
                    eprint!("--params needs a JSON object\n{USAGE}");
                    return ExitCode::from(2);
                };
                if let Err(e) = params.apply_json(&json) {
                    eprintln!("doppler-md: {e}");
                    return ExitCode::from(2);
                }
            }
            _ if input.is_none() => input = Some(a),
            _ => {
                eprint!("{USAGE}");
                return ExitCode::from(2);
            }
        }
    }
    let mut bytes = Vec::new();
    let read = match input.as_deref() {
        None | Some("-") => std::io::stdin().lock().read_to_end(&mut bytes),
        Some(path) => std::fs::File::open(path).and_then(|mut f| f.read_to_end(&mut bytes)),
    };
    if let Err(e) = read {
        eprintln!("doppler-md: {e}");
        return ExitCode::FAILURE;
    }
    let no_page = "no <page> in the input (not doppler -bbox-layout XHTML?)";
    cli::convert("doppler-md", &bytes, &params, &opts, want_report, no_page)
}
