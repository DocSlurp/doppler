//! The output folder of `--images-dir`: created new, written only through a held directory
//! descriptor, never through a symbolic link.
//!
//! The parent is opened one path component at a time with `O_NOFOLLOW | O_DIRECTORY`, so no
//! component may be a symbolic link; the folder is made with `mkdirat` (it must not exist) and
//! opened with `O_NOFOLLOW`. Files are written to a temporary name created with
//! `O_CREAT | O_EXCL | O_NOFOLLOW` and renamed into place with `RENAME_NOREPLACE`. Names are
//! checked against the id pattern: no document string reaches a path.

use std::ffi::OsStr;
use std::io::Write;
use std::os::fd::OwnedFd;
use std::path::{Component, Path};

use rustix::fs::{AtFlags, Mode, OFlags, RenameFlags};
use rustix::io::Errno;

/// The folder being written: its descriptor and its parent's, and the names created in it.
pub struct OutDir {
    parent: OwnedFd,
    name: std::ffi::OsString,
    dir: OwnedFd,
    created: Vec<String>,
}

fn err(what: &str, e: Errno) -> String {
    format!("{what}: {}", std::io::Error::from(e))
}

impl OutDir {
    /// Creates `path` (mode 0700). Fails when it exists, when its parent does not, or when any
    /// component of the path is a symbolic link.
    pub fn create(path: &Path) -> Result<OutDir, String> {
        let name = match path.components().next_back() {
            Some(Component::Normal(n)) => n.to_os_string(),
            _ => return Err("--images-dir must end in a folder name".into()),
        };
        let parent_path = path.parent().unwrap_or(Path::new(""));
        let dir_flags = OFlags::PATH | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC;
        let start: &OsStr = if parent_path.is_absolute() { OsStr::new("/") } else { OsStr::new(".") };
        let mut fd = rustix::fs::open(start, dir_flags, Mode::empty()).map_err(|e| err("opening the parent", e))?;
        for c in parent_path.components() {
            let step: &OsStr = match c {
                Component::RootDir | Component::CurDir => continue,
                Component::Prefix(_) => return Err("unsupported path".into()),
                Component::ParentDir => OsStr::new(".."),
                Component::Normal(n) => n,
            };
            fd = rustix::fs::openat(&fd, step, dir_flags, Mode::empty()).map_err(|e| {
                if e == Errno::NOTDIR || e == Errno::LOOP {
                    format!("{} is a symbolic link or not a folder", Path::new(step).display())
                } else {
                    err("opening the parent", e)
                }
            })?;
        }
        // O_PATH descriptors cannot be used for mkdirat on every kernel: reopen the parent
        let parent = rustix::fs::openat(
            &fd,
            ".",
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .map_err(|e| err("opening the parent", e))?;
        rustix::fs::mkdirat(&parent, &name, Mode::from_raw_mode(0o700)).map_err(|e| {
            if e == Errno::EXIST {
                format!("{} already exists", path.display())
            } else {
                err(&format!("creating {}", path.display()), e)
            }
        })?;
        let dir = rustix::fs::openat(
            &parent,
            &name,
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        );
        let dir = match dir {
            Ok(d) => d,
            Err(e) => {
                let _ = rustix::fs::unlinkat(&parent, &name, AtFlags::REMOVEDIR);
                return Err(err(&format!("opening {}", path.display()), e));
            }
        };
        Ok(OutDir { parent, name, dir, created: Vec::new() })
    }

    /// Writes `data` as `name` (checked with [`valid_name`]), which must not exist.
    pub fn write(&mut self, name: &str, data: &[u8]) -> Result<(), String> {
        if !valid_name(name) {
            return Err("invalid file name".into());
        }
        let tmp = format!(".{name}.part");
        let fd = rustix::fs::openat(
            &self.dir,
            tmp.as_str(),
            OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::from_raw_mode(0o600),
        )
        .map_err(|e| err(&format!("creating {name}"), e))?;
        self.created.push(tmp.clone());
        let mut f = std::fs::File::from(fd);
        f.write_all(data).map_err(|e| format!("writing {name}: {e}"))?;
        drop(f);
        rustix::fs::renameat_with(&self.dir, tmp.as_str(), &self.dir, name, RenameFlags::NOREPLACE)
            .map_err(|e| err(&format!("renaming {name}"), e))?;
        self.created.pop();
        self.created.push(name.to_string());
        Ok(())
    }

    /// Flushes the folder's entries to disk.
    pub fn sync(&self) -> Result<(), String> {
        rustix::fs::fsync(&self.dir).map_err(|e| err("syncing the folder", e))
    }

    /// Removes the files written and the folder (which fails, leaving it, when something else
    /// put a file in it).
    pub fn remove(self) -> Result<(), String> {
        for n in &self.created {
            let _ = rustix::fs::unlinkat(&self.dir, n.as_str(), AtFlags::empty());
        }
        drop(self.dir);
        rustix::fs::unlinkat(&self.parent, &self.name, AtFlags::REMOVEDIR).map_err(|e| err("removing the folder", e))
    }
}

/// `manifest.json`, or `p<page>.<n>` / `p<page>.g<n>` with a `.webp`, `.png` or `.svg`
/// extension (page 1..=9 digits, n 1..=6, no leading zeros).
pub fn valid_name(name: &str) -> bool {
    if name == "manifest.json" {
        return true;
    }
    let Some((stem, ext)) = name.rsplit_once('.') else { return false };
    matches!(ext, "webp" | "png" | "svg") && valid_id(stem)
}

/// An image id as pdftotext and the converter make them: `p<page>.<n>` or `p<page>.g<n>`.
pub fn valid_id(id: &str) -> bool {
    let Some(rest) = id.strip_prefix('p') else { return false };
    let Some((page, n)) = rest.split_once('.') else { return false };
    let n = n.strip_prefix('g').unwrap_or(n);
    let digits = |s: &str, max: usize| {
        !s.is_empty() && s.len() <= max && s.bytes().all(|b| b.is_ascii_digit()) && !s.starts_with('0')
    };
    digits(page, 9) && digits(n, 6)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_come_only_from_ids() {
        for ok in ["p1.1.webp", "p3.g12.svg", "p123456789.999999.png", "manifest.json"] {
            assert!(valid_name(ok), "{ok}");
        }
        for bad in [
            "p1.1",
            "p0.1.webp",
            "p1.01.webp",
            "p1.1.exe",
            "../p1.1.webp",
            "p1.1.webp/x",
            "p1.g.svg",
            "p1234567890.1.png",
            "p1.1234567.png",
            "q1.1.png",
            "p1.1.webp\n",
            ".p1.1.webp",
        ] {
            assert!(!valid_name(bad), "{bad:?}");
        }
    }
}
