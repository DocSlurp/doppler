//! `doppler-md [--params JSON] [--report] [FILE]`: doppler XHTML (file or stdin) to Markdown
//! pages joined with form feeds on stdout.
//!
//! Exit status 2 (with a message on stderr) when the input has no `<page>`, which means it is
//! not doppler XHTML. `--report` prints a one-line JSON report of skipped words and rules,
//! unclosed elements and pages read as plain lines to stderr.

use std::io::{Read, Write};
use std::process::ExitCode;

use doppler_md::{Params, to_markdown_report};

const USAGE: &str = "usage: doppler-md [--params JSON] [--report] [FILE|-]\n";

fn main() -> ExitCode {
    let mut params = Params::default();
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
    // Python read the XHTML with errors='replace'
    // (validate first: the strict check is several times faster than the lossy decoder, which
    // only runs when the input really has invalid bytes)
    let xhtml = match std::str::from_utf8(&bytes) {
        Ok(s) => std::borrow::Cow::Borrowed(s),
        Err(_) => String::from_utf8_lossy(&bytes),
    };
    let (pages, report) = to_markdown_report(&xhtml, &params);
    if want_report {
        eprintln!("{}", report.to_json());
    }
    if !report.page_found() {
        eprintln!("doppler-md: no <page> in the input (not doppler -bbox-layout XHTML?)");
        return ExitCode::from(2);
    }
    let mut out = std::io::stdout().lock();
    if let Err(e) = out.write_all(pages.join("\x0c").as_bytes()).and_then(|_| out.flush()) {
        eprintln!("doppler-md: {e}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}
