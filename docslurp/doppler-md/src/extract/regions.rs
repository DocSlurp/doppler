//! The `pdfregions` client (docslurp/extraction/PROTOCOL.md): one process per pass, jobs on
//! its stdin, records read from its stdout and checked against the jobs.

use std::ffi::OsString;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// At most this many jobs per run (the protocol's limit).
pub const MAX_JOBS: usize = 5000;
/// A record header longer than this is a framing error.
const MAX_HEADER: usize = 256;
/// A payload larger than this ends the run: the stream cannot be trusted.
const MAX_PAYLOAD: u64 = 1 << 30;
/// An SVG payload larger than this is not kept (its figure becomes a raster).
pub const MAX_SVG_INPUT: u64 = 64 << 20;
/// Bytes of the renderer's diagnostics forwarded to stderr.
const MAX_DIAGNOSTICS: usize = 4096;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Raster,
    Svg,
}

/// One region to render.
#[derive(Clone, Debug)]
pub struct Job {
    pub id: String,
    pub page: u32,
    pub bbox: (f64, f64, f64, f64),
    pub mode: Mode,
    pub dpi: u32,
}

impl Job {
    /// The pixel size the renderer should return for a raster job: the box at `dpi`, rounded,
    /// at least 1.
    pub fn expected_px(&self) -> (u32, u32) {
        let s = f64::from(self.dpi) / 72.0;
        let px = |v: f64| (v * s).round().clamp(1.0, f64::from(u32::MAX)) as u32;
        (px(self.bbox.2 - self.bbox.0), px(self.bbox.3 - self.bbox.1))
    }

    fn line(&self) -> String {
        let (x0, y0, x1, y1) = self.bbox;
        let mode = match self.mode {
            Mode::Raster => "raster",
            Mode::Svg => "svg",
        };
        format!("{}\t{}\t{x0:.3}\t{y0:.3}\t{x1:.3}\t{y1:.3}\t{mode}\t{}\n", self.id, self.page, self.dpi)
    }
}

/// What came back for a job.
#[derive(Debug)]
pub enum Outcome {
    Rgb {
        width: u32,
        height: u32,
        data: Vec<u8>,
    },
    Svg(String),
    /// the renderer's `err:<code>`, or `mismatch` / `svg_size` / `utf8` when the record does not
    /// fit the job
    Error(String),
}

/// How to start the renderer.
pub struct Renderer {
    pub exe: PathBuf,
    /// `-opw PW`, `-upw PW`
    pub passwords: Vec<OsString>,
    pub file: OsString,
}

/// `$PDFREGIONS`, else `pdfregions` beside this executable, else `pdfregions` on `PATH`.
pub fn locate() -> PathBuf {
    if let Some(p) = std::env::var_os("PDFREGIONS").filter(|p| !p.is_empty()) {
        return PathBuf::from(p);
    }
    let name = format!("pdfregions{}", std::env::consts::EXE_SUFFIX);
    if let Some(beside) = std::env::current_exe().ok().and_then(|e| Some(e.parent()?.join(&name)))
        && beside.is_file()
    {
        return beside;
    }
    PathBuf::from(name)
}

enum Msg {
    Record(usize, Outcome),
    /// the stream ended or broke before every job was answered
    Fatal(String),
    Done,
}

impl Renderer {
    /// Renders `jobs` (sorted by page), calling `each` with every record in job order. Fails
    /// (killing the renderer) when it cannot run, breaks the framing, ends early, exits
    /// non-zero, passes `deadline`, or when `each` fails.
    pub fn run(
        &self,
        jobs: &[Job],
        deadline: Instant,
        timeout_arg: u64,
        each: &mut dyn FnMut(usize, Outcome) -> Result<(), String>,
    ) -> Result<(), String> {
        if jobs.is_empty() {
            return Ok(());
        }
        if jobs.len() > MAX_JOBS {
            return Err(format!("more than {MAX_JOBS} regions to render"));
        }
        if jobs.windows(2).any(|w| w[1].page < w[0].page) {
            return Err("regions are not sorted by page".into());
        }
        let mut child = Command::new(&self.exe)
            .args(&self.passwords)
            .arg("-timeout")
            .arg(timeout_arg.to_string())
            .arg(&self.file)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("cannot run {}: {e}", self.exe.display()))?;
        let result = self.drive(&mut child, jobs, deadline, each);
        if result.is_err() {
            let _ = child.kill();
            let _ = child.wait();
        }
        result
    }

    fn drive(
        &self,
        child: &mut Child,
        jobs: &[Job],
        deadline: Instant,
        each: &mut dyn FnMut(usize, Outcome) -> Result<(), String>,
    ) -> Result<(), String> {
        let name = self.exe.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let (Some(mut stdin), Some(stdout), Some(stderr)) =
            (child.stdin.take(), child.stdout.take(), child.stderr.take())
        else {
            return Err("renderer pipes".into());
        };
        let lines: String = jobs.iter().map(Job::line).collect();
        // writes may block until the renderer reads: a thread of their own; a renderer that
        // exits early closes the pipe and the write fails, which is fine
        std::thread::spawn(move || {
            let _ = stdin.write_all(lines.as_bytes());
        });
        let diag_name = name.clone();
        let diagnostics = std::thread::spawn(move || forward_diagnostics(stderr, &diag_name));
        let (tx, rx) = mpsc::sync_channel::<Msg>(2);
        let expect: Vec<Job> = jobs.to_vec();
        std::thread::spawn(move || read_records(stdout, &expect, &tx));
        let mut answered = 0;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            match rx.recv_timeout(left) {
                Ok(Msg::Record(k, outcome)) => {
                    answered += 1;
                    each(k, outcome)?;
                }
                Ok(Msg::Fatal(e)) => return Err(e),
                Ok(Msg::Done) => break,
                Err(mpsc::RecvTimeoutError::Timeout) => return Err(format!("{name} timed out")),
                Err(mpsc::RecvTimeoutError::Disconnected) => return Err(format!("{name} output ended")),
            }
        }
        debug_assert_eq!(answered, jobs.len());
        // every job answered: the exit status must still be 0
        loop {
            match child.try_wait() {
                Ok(Some(status)) => {
                    let _ = diagnostics.join();
                    return match status.code() {
                        Some(0) => Ok(()),
                        Some(n) => Err(format!("{name} exited with status {n}")),
                        None => Err(format!("{name} was killed by a signal")),
                    };
                }
                Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(2)),
                Ok(None) => return Err(format!("{name} timed out")),
                Err(e) => return Err(format!("waiting for {name}: {e}")),
            }
        }
    }
}

fn forward_diagnostics(stderr: impl Read, name: &str) {
    let mut r = BufReader::new(stderr);
    let mut sent = 0usize;
    let mut line = Vec::new();
    loop {
        line.clear();
        match r.by_ref().take(1024).read_until(b'\n', &mut line) {
            Ok(0) | Err(_) => return,
            Ok(_) => {}
        }
        if sent < MAX_DIAGNOSTICS {
            // the renderer writes fixed ASCII strings; anything else is not shown
            let text: String =
                line.iter().map(|&b| if b.is_ascii_graphic() || b == b' ' { b as char } else { '?' }).collect();
            let text = text.trim_end_matches('?');
            sent += text.len();
            eprintln!("pdftomarkdown: {name}: {text}");
        }
    }
}

/// Reads one record per job, in order, and sends each checked against its job.
fn read_records(stdout: impl Read, jobs: &[Job], tx: &mpsc::SyncSender<Msg>) {
    let mut r = BufReader::with_capacity(1 << 16, stdout);
    for (k, job) in jobs.iter().enumerate() {
        let msg = match read_record(&mut r, job) {
            Ok(outcome) => Msg::Record(k, outcome),
            Err(e) => Msg::Fatal(e),
        };
        let fatal = matches!(msg, Msg::Fatal(_));
        if tx.send(msg).is_err() || fatal {
            return;
        }
    }
    // nothing may follow the last record
    let mut extra = [0u8; 1];
    let msg = match r.read(&mut extra) {
        Ok(0) => Msg::Done,
        Ok(_) => Msg::Fatal("renderer wrote more records than jobs".into()),
        Err(e) => Msg::Fatal(format!("reading the renderer: {e}")),
    };
    let _ = tx.send(msg);
}

fn read_record(r: &mut impl BufRead, job: &Job) -> Result<Outcome, String> {
    let mut header = Vec::with_capacity(64);
    r.by_ref()
        .take(MAX_HEADER as u64)
        .read_until(b'\n', &mut header)
        .map_err(|e| format!("reading the renderer: {e}"))?;
    if header.is_empty() {
        return Err("renderer output ended before every region was answered".into());
    }
    if header.last() != Some(&b'\n') {
        return Err("renderer record header malformed".into());
    }
    header.pop();
    let header = std::str::from_utf8(&header).map_err(|_| "renderer record header malformed".to_string())?;
    let f: Vec<&str> = header.split('\t').collect();
    if f.len() != 6 {
        return Err("renderer record header malformed".into());
    }
    let num = |s: &str| -> Result<u64, String> {
        if s.is_empty() || s.len() > 12 || !s.bytes().all(|b| b.is_ascii_digit()) {
            return Err("renderer record header malformed".into());
        }
        s.parse::<u64>().map_err(|_| "renderer record header malformed".to_string())
    };
    let (width, height, bytes) = (num(f[3])?, num(f[4])?, num(f[5])?);
    if bytes > MAX_PAYLOAD {
        return Err("renderer record too large".into());
    }
    let (id, status, kind) = (f[0], f[1], f[2]);
    // checks that leave the stream in step: the payload is read (or skipped) either way
    let problem: Option<String> = if id != job.id {
        Some("mismatch".into())
    } else if let Some(code) = status.strip_prefix("err:") {
        if kind != "-" || width != 0 || height != 0 || bytes != 0 {
            Some("mismatch".into())
        } else {
            Some(match code {
                "pixels" | "render" | "empty" | "page" | "svgsize" => code.to_string(),
                _ => "renderer".to_string(),
            })
        }
    } else if status != "ok" {
        Some("mismatch".into())
    } else {
        match (kind, job.mode) {
            ("rgb8", Mode::Raster) => {
                let (ew, eh) = job.expected_px();
                let near = |a: u64, b: u32| a.abs_diff(u64::from(b)) <= 1;
                let size_ok = width > 0 && height > 0 && near(width, ew) && near(height, eh);
                (!size_ok || width.checked_mul(height).and_then(|n| n.checked_mul(3)) != Some(bytes))
                    .then(|| "mismatch".to_string())
            }
            ("svg", Mode::Svg) => (bytes > MAX_SVG_INPUT).then(|| "svgsize".to_string()),
            _ => Some("mismatch".into()),
        }
    };
    if let Some(p) = problem {
        let skipped = std::io::copy(&mut r.by_ref().take(bytes), &mut std::io::sink())
            .map_err(|e| format!("reading the renderer: {e}"))?;
        if skipped != bytes {
            return Err("renderer output ended inside a record".into());
        }
        return Ok(Outcome::Error(p));
    }
    let mut data = Vec::new();
    data.try_reserve_exact(bytes as usize).map_err(|_| "out of memory for a record".to_string())?;
    r.by_ref().take(bytes).read_to_end(&mut data).map_err(|e| format!("reading the renderer: {e}"))?;
    if data.len() as u64 != bytes {
        return Err("renderer output ended inside a record".into());
    }
    Ok(match job.mode {
        Mode::Raster => Outcome::Rgb { width: width as u32, height: height as u32, data },
        Mode::Svg => match String::from_utf8(data) {
            Ok(s) => Outcome::Svg(s),
            Err(_) => Outcome::Error("utf8".into()),
        },
    })
}

/// Whether `exe` names a file that exists (for a clear message before anything runs).
pub fn exists(exe: &Path) -> bool {
    if exe.components().count() > 1 {
        return exe.is_file();
    }
    std::env::var_os("PATH").is_some_and(|p| std::env::split_paths(&p).any(|d| d.join(exe).is_file()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn job(mode: Mode) -> Job {
        Job { id: "p1.2".into(), page: 1, bbox: (10.0, 20.0, 82.0, 56.0), mode, dpi: 150 }
    }

    fn parse(bytes: &[u8], job: &Job) -> Result<Outcome, String> {
        read_record(&mut std::io::Cursor::new(bytes), job)
    }

    #[test]
    fn expected_size_is_the_box_at_the_dpi() {
        assert_eq!(job(Mode::Raster).expected_px(), (150, 75));
        assert_eq!(job(Mode::Raster).line(), "p1.2\t1\t10.000\t20.000\t82.000\t56.000\traster\t150\n");
    }

    #[test]
    fn records_are_checked_against_the_job() {
        let j = job(Mode::Raster);
        let mut ok = b"p1.2\tok\trgb8\t151\t74\t33522\n".to_vec();
        ok.extend(vec![255u8; 151 * 74 * 3]);
        assert!(matches!(parse(&ok, &j), Ok(Outcome::Rgb { width: 151, height: 74, .. })));
        // two pixels off
        let mut far = b"p1.2\tok\trgb8\t152\t75\t34200\n".to_vec();
        far.extend(vec![0u8; 34200]);
        assert!(matches!(parse(&far, &j), Ok(Outcome::Error(e)) if e == "mismatch"));
        // wrong id
        let mut other = b"p1.3\tok\trgb8\t150\t75\t33750\n".to_vec();
        other.extend(vec![0u8; 33750]);
        assert!(matches!(parse(&other, &j), Ok(Outcome::Error(e)) if e == "mismatch"));
        // byte count does not match the size
        let mut short = b"p1.2\tok\trgb8\t150\t75\t10\n".to_vec();
        short.extend(vec![0u8; 10]);
        assert!(matches!(parse(&short, &j), Ok(Outcome::Error(e)) if e == "mismatch"));
        // wrong kind for the mode
        assert!(matches!(parse(b"p1.2\tok\tsvg\t0\t0\t3\n<s>", &j), Ok(Outcome::Error(e)) if e == "mismatch"));
        assert!(matches!(parse(b"p1.2\terr:pixels\t-\t0\t0\t0\n", &j), Ok(Outcome::Error(e)) if e == "pixels"));
        assert!(matches!(parse(b"p1.2\terr:boom\t-\t0\t0\t0\n", &j), Ok(Outcome::Error(e)) if e == "renderer"));
        let s = job(Mode::Svg);
        assert!(matches!(parse(b"p1.2\tok\tsvg\t0\t0\t6\n<svg/>", &s), Ok(Outcome::Svg(x)) if x == "<svg/>"));
    }

    #[test]
    fn broken_framing_is_fatal() {
        let j = job(Mode::Raster);
        assert!(parse(b"", &j).is_err());
        assert!(parse(b"p1.2\tok\trgb8\t150\t75", &j).is_err());
        assert!(parse(b"p1.2 ok rgb8 150 75 33750\n", &j).is_err());
        assert!(parse(b"p1.2\tok\trgb8\t150\t75\t33750\nshort", &j).is_err());
        assert!(parse(b"p1.2\tok\trgb8\t150\t75\t-1\n", &j).is_err());
        assert!(parse(b"p1.2\tok\trgb8\t150\t75\t99999999999\n", &j).is_err());
        assert!(parse(&[b'x'; 300], &j).is_err());
    }
}
