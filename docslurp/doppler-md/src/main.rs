//! `doppler-md [--html] [--stream | --window N] [--body-pages N] [--body-size PT]
//! [--body-color C] [--params JSON] [--report] [FILE]`: doppler XHTML (file or stdin) to
//! Markdown pages joined with form feeds on stdout. `--window N` writes each page once the next
//! N are complete (see `pdftomarkdown`). Repeated content (running headers and footers,
//! repeated watermarks) is found only within the window: `--window 0` removes none of it.
//!
//! Exit status 2 (with a message on stderr) when the input has no `<page>`, which means it is
//! not doppler XHTML. `--report` prints a one-line JSON report of skipped words and rules,
//! unclosed elements and pages read as plain lines to stderr. `--html` writes tables as HTML
//! `<table>` blocks instead of GFM pipe tables.

#![forbid(unsafe_code)]

use std::process::ExitCode;

use doppler_md::{Options, Params, TableFormat};

mod cli;

const USAGE: &str = "usage: doppler-md [--html] [--math] [--markers] [--stream | --window N] [--body-pages N]
                  [--body-size PT] [--body-color C] [--heading-levels size|rank] [--params JSON] [--report] [FILE|-]
";

fn main() -> ExitCode {
    let mut params = Params::default();
    let mut opts = Options::default();
    let mut input: Option<String> = None;
    let mut want_report = false;
    let mut win = cli::WindowArgs::default();
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match win.take(&a, &mut || args.next()) {
            Some(Ok(())) => continue,
            Some(Err(e)) => {
                eprint!("doppler-md: {e}\n{USAGE}");
                return ExitCode::from(2);
            }
            None => {}
        }
        match a.as_str() {
            "-h" | "--help" => {
                print!("{USAGE}\n{}", cli::WINDOW_HELP);
                return ExitCode::SUCCESS;
            }
            "--report" => want_report = true,
            "--html" => opts.tables = TableFormat::Html,
            "--math" => opts.math = true,
            "--markers" => opts.markers = true,
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
    let mut run = cli::Run::new(&params, &opts, win.window());
    let read = match input.as_deref() {
        None | Some("-") => run.read_from(&mut std::io::stdin().lock()),
        Some(path) => std::fs::File::open(path).and_then(|mut f| run.read_from(&mut f)),
    };
    if let Err(e) = read {
        eprintln!("doppler-md: {e}");
        return ExitCode::FAILURE;
    }
    let no_page = "no <page> in the input (not doppler -bbox-layout XHTML?)";
    run.finish("doppler-md", want_report, no_page)
}
