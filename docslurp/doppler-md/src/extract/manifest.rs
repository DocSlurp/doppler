//! `manifest.jsonl` (schema `doppler-images/3`): what an `--images-dir` folder holds, as JSON
//! Lines, so a consumer reads it one line at a time with fixed memory.
//!
//! One JSON object per line, ASCII, each line ending in `\n` and at most [`MAX_LINE`] bytes with
//! it, keys in a fixed order, the same input giving the same bytes:
//!
//! 1. `{"type":"header","schema":"doppler-images/3",...}`: the PDF, the settings digest and the
//!    run's settings, and `resumes`;
//! 2. one `{"type":"image",...}` per image, sorted by page, pictures before figures, by index;
//! 3. one `{"type":"batch",...}` per batch, by page, each with the `run` that wrote it: this
//!    run's, and on a resume the earlier lines that still describe a page no newer line covers;
//! 4. `{"type":"end","complete":...,"resumable":...,"totals":{...},"images":N,"batches":M,
//!    "sha256":HEX}`, where `images` and `batches` count the lines above and `sha256` is the
//!    SHA-256 of every byte before the end line.
//!
//! A reader ([`Reader`]) refuses a file whose first line is not this schema's header, whose
//! lines come out of that order, whose end line is missing or followed by anything, whose
//! counts disagree or whose hash fails: a truncated or altered manifest is never taken for a
//! whole one.

use std::io::{BufRead, Read, Write};

use sha2::{Digest, Sha256};

/// The manifest's file name in DIR.
pub const NAME: &str = "manifest.jsonl";
/// The name of the version 2 manifest (one JSON object), which is refused.
pub const V2_NAME: &str = "manifest.json";
/// The header's `schema`.
pub const SCHEMA: &str = "doppler-images/3";
/// The longest line, with its `\n`.
pub const MAX_LINE: usize = 64 << 10;

/// A line's `type`, in the order the lines come.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    Header,
    Image,
    Batch,
    End,
}

impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Header => "header",
            Kind::Image => "image",
            Kind::Batch => "batch",
            Kind::End => "end",
        }
    }

    fn parse(s: &str) -> Option<Kind> {
        [Kind::Header, Kind::Image, Kind::Batch, Kind::End].into_iter().find(|k| k.as_str() == s)
    }
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(s, "{b:02x}");
    }
    s
}

/// Writes a manifest line by line to `W`, hashing as it goes, and checks each line: its kind
/// in order, ASCII, no line break inside, at most [`MAX_LINE`] bytes.
pub struct Writer<W: Write> {
    out: W,
    hash: Sha256,
    last: Option<Kind>,
    images: u64,
    batches: u64,
}

impl<W: Write> Writer<W> {
    pub fn new(out: W) -> Self {
        Writer { out, hash: Sha256::new(), last: None, images: 0, batches: 0 }
    }

    fn put(&mut self, kind: Kind, fields: &str, hashed: bool) -> Result<(), String> {
        let ordered = match self.last {
            None => kind == Kind::Header,
            Some(Kind::End) => false,
            Some(last) => kind != Kind::Header && kind >= last,
        };
        if !ordered {
            return Err(format!("manifest: a {} line out of order", kind.as_str()));
        }
        let mut line = format!("{{\"type\":\"{}\"", kind.as_str());
        if !fields.is_empty() {
            line.push(',');
            line.push_str(fields);
        }
        line.push_str("}\n");
        if !line.is_ascii() || line[..line.len() - 1].contains('\n') {
            return Err(format!("manifest: a {} line is not one ASCII line", kind.as_str()));
        }
        if line.len() > MAX_LINE {
            return Err(format!("manifest: a {} line is longer than {MAX_LINE} bytes", kind.as_str()));
        }
        if hashed {
            self.hash.update(line.as_bytes());
        }
        self.out.write_all(line.as_bytes()).map_err(|e| format!("manifest: {e}"))?;
        self.last = Some(kind);
        match kind {
            Kind::Image => self.images += 1,
            Kind::Batch => self.batches += 1,
            _ => {}
        }
        Ok(())
    }

    /// The header line: `fields` are its members after `"type"` (from `"schema"` on).
    pub fn header(&mut self, fields: &str) -> Result<(), String> {
        self.put(Kind::Header, fields, true)
    }

    /// One image line (members after `"type"`).
    pub fn image(&mut self, fields: &str) -> Result<(), String> {
        self.put(Kind::Image, fields, true)
    }

    /// One batch line (members after `"type"`).
    pub fn batch(&mut self, fields: &str) -> Result<(), String> {
        self.put(Kind::Batch, fields, true)
    }

    /// The end line: `fields` (`"complete"`, `"resumable"`, `"totals"`), then the line counts
    /// and the hash of every line before it. Returns the output.
    pub fn finish(mut self, fields: &str) -> Result<W, String> {
        let digest = hex(&self.hash.clone().finalize());
        let all = format!("{fields},\"images\":{},\"batches\":{},\"sha256\":\"{digest}\"", self.images, self.batches);
        self.put(Kind::End, &all, false)?;
        self.out.flush().map_err(|e| format!("manifest: {e}"))?;
        Ok(self.out)
    }
}

/// One line read: its kind and its object.
#[derive(Clone, Debug)]
pub struct Line {
    pub kind: Kind,
    pub value: serde_json::Value,
}

/// Reads a manifest one line at a time from `R`, keeping one line (at most [`MAX_LINE`] bytes)
/// and a hash state: [`Reader::next`] gives each line once it is checked, and `None` only
/// after a valid end line followed by nothing. Every failure is an `Err`, and the reader is
/// done after one.
pub struct Reader<R: BufRead> {
    r: R,
    buf: Vec<u8>,
    hash: Sha256,
    last: Option<Kind>,
    images: u64,
    batches: u64,
    line_no: u64,
    done: bool,
}

impl<R: BufRead> Reader<R> {
    pub fn new(r: R) -> Self {
        Reader {
            r,
            buf: Vec::with_capacity(4096),
            hash: Sha256::new(),
            last: None,
            images: 0,
            batches: 0,
            line_no: 0,
            done: false,
        }
    }

    /// The capacity of the line buffer: at most [`MAX_LINE`] + 1 whatever the file's length.
    pub fn buffer_capacity(&self) -> usize {
        self.buf.capacity()
    }

    fn fail<T>(&mut self, what: impl std::fmt::Display) -> Result<T, String> {
        self.done = true;
        Err(format!("{NAME} line {}: {what}", self.line_no))
    }

    /// The next line, `Ok(None)` after the end line (and nothing after it).
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Result<Option<Line>, String> {
        if self.done {
            return Ok(None);
        }
        self.buf.clear();
        let limit = MAX_LINE as u64 + 1;
        let n = match (&mut self.r).take(limit).read_until(b'\n', &mut self.buf) {
            Ok(n) => n,
            Err(e) => return self.fail(e),
        };
        self.line_no += 1;
        if self.last == Some(Kind::End) {
            if n == 0 {
                self.done = true;
                return Ok(None);
            }
            return self.fail("data after the end line");
        }
        if n == 0 {
            return if self.last.is_none() { self.fail("empty file") } else { self.fail("truncated: no end line") };
        }
        if self.buf.last() != Some(&b'\n') {
            return if n > MAX_LINE {
                self.fail(format!("longer than {MAX_LINE} bytes"))
            } else {
                self.fail("truncated: the last line has no line break")
            };
        }
        if !self.buf.is_ascii() {
            return self.fail("not ASCII");
        }
        let value: serde_json::Value = match serde_json::from_slice(&self.buf[..n - 1]) {
            Ok(v @ serde_json::Value::Object(_)) => v,
            _ => return self.fail("not a JSON object"),
        };
        let Some(kind) = value["type"].as_str().and_then(Kind::parse) else {
            return self.fail("no known type");
        };
        match self.last {
            None if kind != Kind::Header => return self.fail("not a manifest header"),
            None if value["schema"].as_str() != Some(SCHEMA) => {
                let found = value["schema"].as_str().unwrap_or("none").to_string();
                return self.fail(format!("schema {found:?}, not {SCHEMA}"));
            }
            Some(last) if kind == Kind::Header || kind < last => {
                return self.fail(format!("a {} line out of order", kind.as_str()));
            }
            _ => {}
        }
        self.last = Some(kind);
        match kind {
            Kind::Image => self.images += 1,
            Kind::Batch => self.batches += 1,
            Kind::Header => {}
            Kind::End => {
                let digest = hex(&self.hash.clone().finalize());
                if value["images"].as_u64() != Some(self.images)
                    || value["totals"]["images"].as_u64() != Some(self.images)
                {
                    return self.fail(format!("the end line's image count is not the {} image lines", self.images));
                }
                if value["batches"].as_u64() != Some(self.batches)
                    || value["totals"]["batches"].as_u64() != Some(self.batches)
                {
                    return self.fail(format!("the end line's batch count is not the {} batch lines", self.batches));
                }
                if value["sha256"].as_str() != Some(digest.as_str()) {
                    return self.fail("sha256 does not match the lines before the end line");
                }
                return Ok(Some(Line { kind, value }));
            }
        }
        self.hash.update(&self.buf);
        Ok(Some(Line { kind, value }))
    }

    /// Reads to the end, keeping only the header and end lines.
    pub fn verify(mut self) -> Result<(serde_json::Value, serde_json::Value), String> {
        let mut header = None;
        let mut end = None;
        while let Some(l) = self.next()? {
            match l.kind {
                Kind::Header => header = Some(l.value),
                Kind::End => end = Some(l.value),
                _ => {}
            }
        }
        match (header, end) {
            (Some(h), Some(e)) => Ok((h, e)),
            _ => Err(format!("{NAME}: incomplete")),
        }
    }
}

/// A JSON string, ASCII: `"`, `\`, control characters and everything past ASCII escaped.
pub fn js(s: &str) -> String {
    use std::fmt::Write as _;
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if (c as u32) < 0x20 || (c as u32) >= 0x7f => {
                let mut units = [0u16; 2];
                for u in c.encode_utf16(&mut units) {
                    let _ = write!(out, "\\u{u:04x}");
                }
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(images: usize) -> Vec<u8> {
        let mut w = Writer::new(Vec::new());
        w.header("\"schema\":\"doppler-images/3\",\"source_sha256\":\"aa\",\"resumes\":0").unwrap();
        for k in 0..images {
            w.image(&format!("\"id\":\"p1.{}\",\"status\":\"extracted\"", k + 1)).unwrap();
        }
        w.batch("\"pages\":[1,64],\"status\":\"ok\",\"cut_page\":null").unwrap();
        w.finish(&format!("\"complete\":true,\"resumable\":false,\"totals\":{{\"images\":{images},\"batches\":1}}"))
            .unwrap()
    }

    fn check(bytes: &[u8]) -> Result<(), String> {
        Reader::new(bytes).verify().map(|_| ())
    }

    #[test]
    fn writes_and_reads_back() {
        let b = sample(3);
        let text = std::str::from_utf8(&b).unwrap();
        assert!(text.starts_with("{\"type\":\"header\",\"schema\":\"doppler-images/3\""), "{text}");
        assert_eq!(text.lines().count(), 6);
        let end = text.lines().last().unwrap();
        let body = &text[..text.len() - end.len() - 1];
        let sum = hex(&Sha256::digest(body.as_bytes()));
        assert!(end.ends_with(&format!(",\"images\":3,\"batches\":1,\"sha256\":\"{sum}\"}}")), "{end}");
        check(&b).unwrap();
        let mut r = Reader::new(&b[..]);
        let kinds: Vec<Kind> = std::iter::from_fn(|| r.next().unwrap()).map(|l| l.kind).collect();
        assert_eq!(kinds, [Kind::Header, Kind::Image, Kind::Image, Kind::Image, Kind::Batch, Kind::End]);
    }

    #[test]
    fn a_truncated_file_is_refused() {
        let b = sample(3);
        let text = std::str::from_utf8(&b).unwrap();
        let without_end = &text[..text.trim_end_matches('\n').rfind('\n').unwrap() + 1];
        let e = check(without_end.as_bytes()).unwrap_err();
        assert!(e.contains("no end line"), "{e}");
        // cut inside a line, and the end line without its line break
        assert!(check(&b[..b.len() / 2]).is_err());
        assert!(check(&b[..b.len() - 1]).unwrap_err().contains("no line break"));
        assert!(check(b"").unwrap_err().contains("empty"));
    }

    #[test]
    fn a_header_only_file_is_refused() {
        let b = sample(0);
        let header = &b[..b.iter().position(|&c| c == b'\n').unwrap() + 1];
        assert!(check(header).unwrap_err().contains("no end line"));
    }

    #[test]
    fn a_tampered_line_fails_the_hash() {
        let b = sample(3);
        let t = String::from_utf8(b).unwrap().replacen("\"p1.2\"", "\"p1.9\"", 1);
        assert!(check(t.as_bytes()).unwrap_err().contains("sha256"));
        // a line dropped: the counts disagree before the hash is looked at
        let t2: Vec<&str> = String::from_utf8(sample(3)).unwrap().leak().lines().collect();
        let dropped = [t2[0], t2[1], t2[3], t2[4], t2[5]].join("\n") + "\n";
        assert!(check(dropped.as_bytes()).unwrap_err().contains("image count"));
        // anything after the end line
        let mut extra = sample(1);
        extra.extend_from_slice(b"{\"type\":\"image\"}\n");
        assert!(check(&extra).unwrap_err().contains("after the end"));
    }

    #[test]
    fn other_schemas_order_and_bytes_are_refused() {
        let v2 = b"{\"version\":2,\"source_sha256\":\"aa\",\n\"batches\":[\n],\n\"images\":[\n]}\n";
        assert!(check(v2).is_err());
        let other = String::from_utf8(sample(1)).unwrap().replace("doppler-images/3", "doppler-images/4");
        assert!(check(other.as_bytes()).unwrap_err().contains("schema"));
        // a batch before an image
        let t: Vec<&str> = String::from_utf8(sample(1)).unwrap().leak().lines().collect();
        let swapped = [t[0], t[2], t[1], t[3]].join("\n") + "\n";
        assert!(check(swapped.as_bytes()).unwrap_err().contains("out of order"));
        let non_ascii = String::from_utf8(sample(1)).unwrap().replace("p1.1", "p1.\u{e9}");
        assert!(check(non_ascii.as_bytes()).unwrap_err().contains("ASCII"));
        // a line past the cap, written or read
        let mut w = Writer::new(Vec::new());
        w.header("\"schema\":\"doppler-images/3\"").unwrap();
        assert!(w.image(&format!("\"x\":\"{}\"", "a".repeat(MAX_LINE))).is_err());
        let mut long = b"{\"type\":\"header\",\"schema\":\"doppler-images/3\",\"x\":\"".to_vec();
        long.extend(std::iter::repeat_n(b'a', MAX_LINE));
        long.extend_from_slice(b"\"}\n");
        let mut r = Reader::new(&long[..]);
        assert!(r.next().unwrap_err().contains("longer than"));
        assert!(r.buffer_capacity() <= 2 * (MAX_LINE + 1));
        // the writer keeps the order too
        let mut w = Writer::new(Vec::new());
        assert!(w.image("").is_err());
        w.header("\"schema\":\"doppler-images/3\"").unwrap();
        w.batch("").unwrap();
        assert!(w.image("").is_err());
    }

    /// A manifest generated as it is read (never held whole) of 100,000 images: the reader
    /// takes it with one line's buffer.
    #[test]
    fn the_reader_streams_with_bounded_memory() {
        const N: u64 = 100_000;
        struct Gen {
            k: u64,
            pending: Vec<u8>,
            pos: usize,
            hash: Sha256,
            produced: u64,
        }
        impl Gen {
            fn refill(&mut self) {
                self.pending.clear();
                self.pos = 0;
                let line = match self.k {
                    0 => "{\"type\":\"header\",\"schema\":\"doppler-images/3\",\"resumes\":0}\n".to_string(),
                    k if k <= N => format!(
                        "{{\"type\":\"image\",\"id\":\"p{k}.1\",\"page\":{k},\"status\":\"extracted\",\"path\":\"page-{k:04}/image-001.webp\",\"sha256\":\"{}\"}}\n",
                        "0".repeat(64)
                    ),
                    k if k == N + 1 => {
                        "{\"type\":\"batch\",\"pages\":[1,64],\"status\":\"ok\",\"cut_page\":null}\n".into()
                    }
                    k if k == N + 2 => format!(
                        "{{\"type\":\"end\",\"complete\":true,\"resumable\":false,\"totals\":{{\"images\":{N},\"batches\":1}},\"images\":{N},\"batches\":1,\"sha256\":\"{}\"}}\n",
                        hex(&self.hash.clone().finalize())
                    ),
                    _ => return,
                };
                if self.k <= N + 1 {
                    self.hash.update(line.as_bytes());
                }
                self.k += 1;
                self.pending.extend_from_slice(line.as_bytes());
            }
        }
        impl Read for Gen {
            fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
                if self.pos == self.pending.len() {
                    self.refill();
                }
                let n = out.len().min(self.pending.len() - self.pos);
                out[..n].copy_from_slice(&self.pending[self.pos..self.pos + n]);
                self.pos += n;
                self.produced += n as u64;
                Ok(n)
            }
        }
        let g = Gen { k: 0, pending: Vec::new(), pos: 0, hash: Sha256::new(), produced: 0 };
        let mut r = Reader::new(std::io::BufReader::with_capacity(8192, g));
        let mut images = 0u64;
        let mut max_cap = 0;
        while let Some(l) = r.next().unwrap() {
            images += u64::from(l.kind == Kind::Image);
            max_cap = max_cap.max(r.buffer_capacity());
        }
        assert_eq!(images, N);
        // one line's buffer (at most the cap), however long the file
        assert!(max_cap <= 4096, "{max_cap}");
        assert!(r.r.get_ref().produced > 10_000_000, "the file is large: {}", r.r.get_ref().produced);
    }

    #[test]
    fn strings_are_ascii() {
        assert_eq!(js("a\"\\\n\u{e9}\u{1f600}"), "\"a\\\"\\\\\\u000a\\u00e9\\ud83d\\ude00\"");
        let v: serde_json::Value = serde_json::from_str(&js("\u{e9}\u{1f600}")).unwrap();
        assert_eq!(v, "\u{e9}\u{1f600}");
    }
}
