//! Shared tail of the `doppler-md` and `pdftomarkdown` binaries: read doppler XHTML as it
//! arrives, convert it page by page, and write the pages and the report. Included by both with
//! `mod cli;` (not part of the library API).

use std::io::{BufWriter, Read, StdoutLock, Write};
use std::process::ExitCode;

use doppler_md::model::{self, Parser};
use doppler_md::{BodySample, Converter, Options, Params, Report, Window};

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
    /// stop reading once the body estimate is fixed (`--estimate-body`)
    stop_at_body: bool,
}

impl<'p> Run<'p> {
    /// `win`: the furniture window, body estimate and context (see [`Converter`]).
    pub fn new(p: &'p Params, o: &Options, win: Window) -> Self {
        let flush_pages = win.furniture.is_some();
        Run {
            parser: Parser::with_glyphs(o.math),
            conv: Converter::with(p, *o, win),
            partial: Vec::new(),
            text: String::new(),
            out: BufWriter::new(std::io::stdout().lock()),
            written: 0,
            flush_pages,
            write_error: None,
            stop_at_body: false,
        }
    }

    /// Stop reading once the body estimate is fixed (the rest of the input is not needed).
    #[allow(dead_code)] // pdftomarkdown only
    pub fn stop_at_body(&mut self) {
        self.stop_at_body = true;
    }

    /// Whether reading stopped early because the body estimate was fixed.
    #[allow(dead_code)] // pdftomarkdown only
    pub fn body_known(&self) -> bool {
        self.conv.body().is_some()
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
            if self.stop_at_body && self.conv.body().is_some() {
                return Ok(());
            }
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

use doppler_md::{BODY_SIZE_MAX, BODY_SIZE_MIN, STREAM_WINDOW, body_color_ok, body_size_ok};

/// The largest `--window` (pages either side): the converter holds the window + 1 pages.
pub const MAX_WINDOW: usize = 64;
/// The largest `--body-pages` (pages with text; at most twice as many are held).
pub const MAX_BODY_PAGES: usize = 256;
/// The largest `--context` (pages either side of a range; more than the window buys nothing).
#[allow(dead_code)] // pdftomarkdown only
pub const MAX_CONTEXT: usize = 64;

/// The window and body options shared by both binaries.
#[derive(Default)]
pub struct WindowArgs {
    stream: bool,
    window: Option<usize>,
    body_pages: Option<usize>,
    body_size: Option<f64>,
    body_color: Option<String>,
}

/// Help text for [`WindowArgs`].
pub const WINDOW_HELP: &str = concat!(
    "  --stream       write pages as they are converted: --window 16, body from 32 pages with text\n",
    "  --window N     furniture window, 0 to 64: running headers/footers, repeated watermark\n",
    "                 words and pictures are judged over the N pages either side of each page\n",
    "                 (not the whole document); each page is written once the next N are\n",
    "                 complete and the body sample is taken. 16 differs least from the whole\n",
    "                 document; 4 removes more chapter running heads; 2 removes some content;\n",
    "                 0 removes no repeated content at all\n",
    "  --body-pages N estimate the body font size and colour from the pages up to the N-th with\n",
    "                 text, 1 to 256 (at most 2N pages, and fewer when they hold over 262144\n",
    "                 words and rules); none is written before then (default with a window: 32;\n",
    "                 without one, the whole document)\n",
    "  --body-size PT use this body font size, 0.5 to 1000, not an estimate (the report's\n",
    "                 body_size, exactly as printed)\n",
    "  --body-color C use this body colour, not an estimate (the report's body_color, exactly)\n",
);

impl WindowArgs {
    /// Takes `flag` (and its value from `next`) when it is one of these options.
    pub fn take(&mut self, flag: &str, next: &mut dyn FnMut() -> Option<String>) -> Option<Result<(), String>> {
        let count = |v: Option<String>, what: &str, min: usize, max: usize| -> Result<usize, String> {
            let v = v.ok_or(format!("{what} needs a number of pages"))?;
            match v.parse::<usize>() {
                Ok(n) if (min..=max).contains(&n) => Ok(n),
                _ => Err(format!("{what}: not a number of pages from {min} to {max}: {v:?}")),
            }
        };
        let r = match flag {
            "--stream" => {
                self.stream = true;
                Ok(())
            }
            "--window" => count(next(), "--window", 0, MAX_WINDOW).map(|n| self.window = Some(n)),
            "--body-pages" => count(next(), "--body-pages", 1, MAX_BODY_PAGES).map(|n| self.body_pages = Some(n)),
            // the range every estimate is in (lib::body_size clamps to it)
            "--body-size" => match next().map(|v| (v.parse::<f64>(), v)) {
                Some((Ok(pt), _)) if body_size_ok(pt) => {
                    self.body_size = Some(pt);
                    Ok(())
                }
                Some((_, v)) => {
                    Err(format!("--body-size: not a font size in points ({BODY_SIZE_MIN} to {BODY_SIZE_MAX}): {v:?}"))
                }
                None => Err("--body-size needs a font size in points".into()),
            },
            "--body-color" => match next() {
                Some(c) if body_color_ok(&c) => {
                    self.body_color = Some(c);
                    Ok(())
                }
                Some(_) => Err("--body-color: at most 64 characters, no control characters".into()),
                None => Err("--body-color needs a colour (as the report's body_color)".into()),
            },
            _ => return None,
        };
        Some(r)
    }

    /// The furniture window these options ask for (`None`: the whole document).
    #[allow(dead_code)] // pdftomarkdown only
    pub fn furniture(&self) -> Option<usize> {
        self.window.or(self.stream.then_some(STREAM_WINDOW))
    }

    /// The converter window these options ask for.
    pub fn window(&self) -> Window {
        Window {
            furniture: self.furniture(),
            // None: the library's default (DEFAULT_BODY_PAGES with a window, else the whole
            // document)
            body: self.body_pages.map(BodySample::TextPages),
            body_size: self.body_size,
            body_color: self.body_color.clone(),
            ..Window::default()
        }
    }
}
