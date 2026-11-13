//! `pdftomarkdown --page-facts FILE`: the facts file, written by pdftotext through an inherited
//! descriptor (`-page-facts-fd N`) while the Markdown is converted.
//!
//! FILE is reserved like `--images-plan-out` ([`NewFile::reserve`]: it must not exist and its
//! folder is reached without following symbolic links). The facts are written to a temporary
//! name in that folder, created `O_CREAT | O_EXCL | O_NOFOLLOW` with mode 0600, and renamed to
//! FILE with `RENAME_NOREPLACE` once pdftotext has exited, so FILE appears whole or not at all
//! and never replaces anything. The descriptor is inheritable only while pdftotext is being
//! started; nothing else (the renderer, say) receives it. This process never changes a line:
//! the bytes are pdftotext's. It does drop an unterminated last line before the rename
//! (pdftotext stopped mid-line, at its time limit while a reader stalled): a FILE holds whole
//! lines only, and one with no whole line is not committed.

use std::ffi::OsString;
use std::os::fd::{AsFd, AsRawFd, OwnedFd};

use doppler_md::extract::outdir::NewFile;
use rustix::fs::{AtFlags, Mode, OFlags, RenameFlags};
use rustix::io::FdFlags;

pub struct FactsFile {
    parent: OwnedFd,
    tmp: OsString,
    name: OsString,
    shown: String,
    file: Option<OwnedFd>,
}

impl FactsFile {
    /// Creates the temporary file for `reserved`.
    pub fn create(reserved: NewFile) -> Result<FactsFile, String> {
        let (parent, name, shown) = reserved.into_parts();
        let mut tmp = OsString::from(".");
        tmp.push(&name);
        tmp.push(format!(".{}.part", std::process::id()));
        let file = rustix::fs::openat(
            &parent,
            &tmp,
            // read too: commit() looks for an unterminated last line through this descriptor
            OFlags::RDWR | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::from_raw_mode(0o600),
        )
        .map_err(|e| format!("creating {shown}: {}", std::io::Error::from(e)))?;
        Ok(FactsFile { parent, tmp, name, shown, file: Some(file) })
    }

    /// The descriptor number to pass as `-page-facts-fd`.
    pub fn raw_fd(&self) -> i32 {
        self.file.as_ref().map_or(-1, |f| f.as_raw_fd())
    }

    /// Lets a child started now inherit the descriptor (`true`), or no longer (`false`).
    pub fn set_inheritable(&self, on: bool) -> Result<(), String> {
        let Some(f) = &self.file else { return Err("closed".into()) };
        rustix::io::fcntl_setfd(f.as_fd(), if on { FdFlags::empty() } else { FdFlags::CLOEXEC })
            .map_err(|e| format!("{}: {}", self.shown, std::io::Error::from(e)))
    }

    /// Whether anything was written (pdftotext ran far enough to open it).
    pub fn written(&self) -> bool {
        self.file.as_ref().and_then(|f| rustix::fs::fstat(f).ok()).is_some_and(|st| st.st_size > 0)
    }

    /// Renames the written file to FILE (which must still not exist), after dropping an
    /// unterminated last line: `Ok(true)` when one was dropped. A file left with no whole line
    /// is refused (an error; it is removed).
    pub fn commit(mut self) -> Result<bool, String> {
        let file = self.file.take();
        let mut dropped = false;
        if let Some(f) = &file {
            let io = |e: rustix::io::Errno| format!("writing {}: {}", self.shown, std::io::Error::from(e));
            let (keep, size) = whole_lines(f).map_err(io)?;
            if keep < size {
                rustix::fs::ftruncate(f, keep).map_err(io)?;
                dropped = true;
            }
            if keep == 0 {
                return Err(format!("{}: pdftotext wrote no whole line (it stopped mid-line)", self.shown));
            }
            rustix::fs::fsync(f).map_err(io)?;
        }
        drop(file);
        rustix::fs::renameat_with(&self.parent, &self.tmp, &self.parent, &self.name, RenameFlags::NOREPLACE)
            .map_err(|e| format!("renaming to {}: {}", self.shown, std::io::Error::from(e)))?;
        self.tmp.clear();
        Ok(dropped)
    }
}

/// (the length up to and including the last newline, the file size), reading back from the end.
fn whole_lines(f: &OwnedFd) -> Result<(u64, u64), rustix::io::Errno> {
    let size = rustix::fs::fstat(f)?.st_size as u64;
    let mut buf = vec![0u8; 1 << 16];
    let mut end = size;
    while end > 0 {
        let start = end.saturating_sub(buf.len() as u64);
        let chunk = &mut buf[..(end - start) as usize];
        let mut got = 0;
        while got < chunk.len() {
            let n = rustix::io::pread(f, &mut chunk[got..], start + got as u64)?;
            if n == 0 {
                return Err(rustix::io::Errno::IO);
            }
            got += n;
        }
        if let Some(i) = chunk.iter().rposition(|&b| b == b'\n') {
            return Ok((start + i as u64 + 1, size));
        }
        end = start;
    }
    Ok((0, size))
}

impl Drop for FactsFile {
    /// An uncommitted file is removed.
    fn drop(&mut self) {
        self.file = None;
        if !self.tmp.is_empty() {
            let _ = rustix::fs::unlinkat(&self.parent, &self.tmp, AtFlags::empty());
        }
    }
}
