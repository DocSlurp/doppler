//! Shared tail of the `doppler-md` and `pdftomarkdown` binaries: read doppler XHTML as it
//! arrives, convert it page by page, and write the pages and the report. Included by both with
//! `mod cli;` (not part of the library API).

use std::io::{BufWriter, Read, StdoutLock, Write};
use std::process::ExitCode;

use doppler_md::model::{self, Parser};
use doppler_md::{Converter, Options, Params, Report};

/// Read size. Pages are written as soon as the converter's window allows, so this bounds the
/// input buffered ahead of the parser, not the latency.
const CHUNK: usize = 64 * 1024;

/// One conversion: input pieces in through [`Run::read_from`], pages out on stdout joined by
/// form feeds. Holds the parser's unconsumed input (usually part of a tag), the converter's
/// window of pages, and at most 3 bytes of an incomplete UTF-8 sequence.
pub struct Run<'p> {
    parser: Parser,
    conv: Converter<'p>,
    /// bytes of a UTF-8 sequence split across reads
    partial: Vec<u8>,
    text: String,
    out: BufWriter<StdoutLock<'static>>,
    written: usize,
    /// flush after every page (a streaming window), not only at the end
    flush_pages: bool,
    write_error: Option<std::io::Error>,
}

impl<'p> Run<'p> {
    /// `window`: pages of lookahead (see [`Converter`]); `None` reads the whole document first.
    pub fn new(p: &'p Params, o: &Options, window: Option<usize>) -> Self {
        Run {
            parser: Parser::with_glyphs(o.math),
            conv: Converter::new(p, *o, window),
            partial: Vec::new(),
            text: String::new(),
            out: BufWriter::new(std::io::stdout().lock()),
            written: 0,
            flush_pages: window.is_some(),
            write_error: None,
        }
    }

    /// The links of marked images (see [`doppler_md::extract::layout`]); before any input.
    #[allow(dead_code)] // pdftomarkdown only
    pub fn set_link_style(&mut self, style: doppler_md::extract::layout::LinkStyle) {
        self.conv.set_link_style(style);
    }

    /// Converts everything `input` yields. Stops early, with `Ok`, once stdout fails (the error
    /// is reported by [`Run::finish`]).
    pub fn read_from(&mut self, input: &mut dyn Read) -> std::io::Result<()> {
        let mut buf = vec![0u8; CHUNK];
        while self.write_error.is_none() {
            let n = match input.read(&mut buf) {
                Ok(0) => return Ok(()),
                Ok(n) => n,
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(e),
            };
            self.decode(&buf[..n], false);
            self.feed();
        }
        Ok(())
    }

    /// Whether writing to stdout failed (the rest of the input is not needed).
    #[allow(dead_code)] // pdftomarkdown only
    pub fn write_failed(&self) -> bool {
        self.write_error.is_some()
    }

    /// Ends the input and writes the remaining pages (`want_report`: the JSON report line to
    /// stderr). Exit status 2 with `no_page` on stderr when the input had no `<page>`, 1 on a
    /// write error.
    #[allow(dead_code)] // doppler-md only
    pub fn finish(self, prog: &str, want_report: bool, no_page: &str) -> ExitCode {
        let (report, written) = self.finish_report();
        conclude(prog, &report, written, want_report, no_page)
    }

    /// Ends the input and writes the remaining pages; the report and how writing went.
    pub fn finish_report(mut self) -> (Report, std::io::Result<()>) {
        if self.write_error.is_none() {
            self.decode(&[], true);
            self.feed();
        }
        let Run { parser, mut conv, mut out, mut written, flush_pages, mut write_error, .. } = self;
        let parse = parser.finish(&mut |page| {
            conv.push(page, &mut |_, md| write_page(&mut out, &mut written, flush_pages, &mut write_error, &md))
        });
        let report = conv.finish(parse, &mut |_, md| write_page(&mut out, &mut written, false, &mut write_error, &md));
        if write_error.is_none()
            && let Err(e) = out.flush()
        {
            write_error = Some(e);
        }
        (report, write_error.map_or(Ok(()), Err))
    }

    fn decode(&mut self, bytes: &[u8], eof: bool) {
        model::decode_lossy(&mut self.partial, bytes, eof, &mut self.text);
    }

    /// Passes the decoded text to the parser, its pages to the converter, and the converter's
    /// pages to stdout.
    fn feed(&mut self) {
        let Run { parser, conv, text, out, written, flush_pages, write_error, .. } = self;
        parser.push(text, &mut |page| {
            conv.push(page, &mut |_, md| write_page(out, written, *flush_pages, write_error, &md))
        });
        text.clear();
    }
}

/// Prints the report (`want_report`) and gives the exit status: 2 with `no_page` on stderr
/// when the input had no `<page>`, 1 on a write error, else 0.
pub fn conclude(
    prog: &str,
    report: &Report,
    written: std::io::Result<()>,
    want_report: bool,
    no_page: &str,
) -> ExitCode {
    if want_report {
        eprintln!("{}", report.to_json());
    }
    if !report.page_found() {
        eprintln!("{prog}: {no_page}");
        return ExitCode::from(2);
    }
    if let Err(e) = written {
        eprintln!("{prog}: {e}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

/// Writes one page (after a form feed, unless it is the first). After a write error, drops
/// the rest.
fn write_page(
    out: &mut BufWriter<StdoutLock<'static>>,
    written: &mut usize,
    flush: bool,
    error: &mut Option<std::io::Error>,
    md: &str,
) {
    if error.is_some() {
        return;
    }
    let sep: &[u8] = if *written > 0 { b"\x0c" } else { b"" };
    *written += 1;
    let r = out.write_all(sep).and_then(|_| out.write_all(md.as_bytes()));
    let r = if flush { r.and_then(|_| out.flush()) } else { r };
    if let Err(e) = r {
        *error = Some(e);
    }
}

/// The value of `--window N`: pages of lookahead either side.
pub fn parse_window(v: Option<&str>) -> Result<usize, String> {
    let v = v.ok_or("--window needs a number of pages")?;
    v.parse::<usize>().map_err(|_| format!("--window: not a number of pages: {v:?}"))
}
