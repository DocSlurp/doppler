//! Shared tail of the `doppler-md` and `pdftomarkdown` binaries: decode doppler XHTML bytes,
//! convert, and write the pages and the report. Included by both with `mod cli;` (not part of
//! the library API).

use std::io::Write;
use std::process::ExitCode;

use doppler_md::{Options, Params, to_markdown_report_with};

/// Converts `bytes` and writes the pages joined by form feeds to stdout (`want_report`: the JSON
/// report line to stderr). Exit status 2 with `no_page` on stderr when the input has no
/// `<page>`, 1 on a write error.
pub fn convert(prog: &str, bytes: &[u8], p: &Params, o: &Options, want_report: bool, no_page: &str) -> ExitCode {
    // Python read the XHTML with errors='replace'
    // (validate first: the strict check is several times faster than the lossy decoder, which
    // only runs when the input really has invalid bytes)
    let xhtml = match std::str::from_utf8(bytes) {
        Ok(s) => std::borrow::Cow::Borrowed(s),
        Err(_) => String::from_utf8_lossy(bytes),
    };
    let (pages, report) = to_markdown_report_with(&xhtml, p, o);
    if want_report {
        eprintln!("{}", report.to_json());
    }
    if !report.page_found() {
        eprintln!("{prog}: {no_page}");
        return ExitCode::from(2);
    }
    let mut out = std::io::stdout().lock();
    if let Err(e) = out.write_all(pages.join("\x0c").as_bytes()).and_then(|_| out.flush()) {
        eprintln!("{prog}: {e}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}
