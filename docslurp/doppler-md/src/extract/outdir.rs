//! The output folder of `--images-dir`: created new, written only through a held directory
//! descriptor, never through a symbolic link.
//!
//! The parent is opened one path component at a time with `O_NOFOLLOW | O_DIRECTORY`, so no
//! component may be a symbolic link; the folder is made with `mkdirat` (it must not exist) and
//! opened with `O_NOFOLLOW`. Files are written to a temporary name created with
//! `O_CREAT | O_EXCL | O_NOFOLLOW` and renamed into place with `RENAME_NOREPLACE`. Images go to
//! one subfolder per page (`page-0012/image-003.webp`), made with `mkdirat` (mode 0700, it must
//! not exist) under the folder's descriptor and opened with `O_NOFOLLOW`; files are 0600.
//! Whatever the umask, nothing is readable by others: copy or `chmod` the files to share them.
//! Names are checked against the layout's pattern ([`super::layout::valid_rel`]): no document
//! string reaches a path.

use std::ffi::OsStr;
use std::io::Write;
use std::os::fd::OwnedFd;
use std::path::{Component, Path};

use rustix::fs::{AtFlags, Mode, OFlags, RenameFlags};
use rustix::io::Errno;

/// The folder being written: its descriptor and its parent's, the page subfolders made in it
/// (with the one in use held open), and the files created (subfolder, name).
pub struct OutDir {
    parent: OwnedFd,
    name: std::ffi::OsString,
    dir: OwnedFd,
    created: Vec<(Option<String>, String)>,
    subdirs: std::collections::HashSet<String>,
    /// the subfolder last written to, and its descriptor
    current: Option<(String, OwnedFd)>,
}

fn err(what: &str, e: Errno) -> String {
    format!("{what}: {}", std::io::Error::from(e))
}

impl OutDir {
    /// Creates `path` (mode 0700). Fails when it exists, when its parent does not, or when any
    /// component of the path is a symbolic link.
    pub fn create(path: &Path) -> Result<OutDir, String> {
        let (parent, name) = open_parent(path, "--images-dir must end in a folder name")?;
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
        Ok(OutDir { parent, name, dir, created: Vec::new(), subdirs: std::collections::HashSet::new(), current: None })
    }

    /// The descriptor of page subfolder `sub`: made (0700, must not exist) the first time,
    /// reopened without following links after another subfolder was used.
    fn subdir(&mut self, sub: &str) -> Result<&OwnedFd, String> {
        if self.current.as_ref().is_none_or(|(n, _)| n != sub) {
            self.current = None;
            if !self.subdirs.contains(sub) {
                rustix::fs::mkdirat(&self.dir, sub, Mode::from_raw_mode(0o700))
                    .map_err(|e| err(&format!("creating {sub}"), e))?;
                self.subdirs.insert(sub.to_string());
            }
            let fd = rustix::fs::openat(
                &self.dir,
                sub,
                OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                Mode::empty(),
            )
            .map_err(|e| err(&format!("opening {sub}"), e))?;
            self.current = Some((sub.to_string(), fd));
        }
        Ok(&self.current.as_ref().expect("set above").1)
    }

    /// Writes `data` as `name` (checked with [`valid_name`]: `manifest.json`, or a page
    /// subfolder and a file in it), which must not exist.
    pub fn write(&mut self, name: &str, data: &[u8]) -> Result<(), String> {
        if !valid_name(name) {
            return Err("invalid file name".into());
        }
        let (sub, file) = match name.split_once('/') {
            Some((d, f)) => (Some(d.to_string()), f),
            None => (None, name),
        };
        if let Some(d) = &sub {
            self.subdir(d)?;
        }
        let at: &OwnedFd = match (&sub, &self.current) {
            (Some(_), Some((_, fd))) => fd,
            _ => &self.dir,
        };
        let tmp = format!(".{file}.part");
        let fd = rustix::fs::openat(
            at,
            tmp.as_str(),
            OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::from_raw_mode(0o600),
        )
        .map_err(|e| err(&format!("creating {name}"), e))?;
        self.created.push((sub.clone(), tmp.clone()));
        let mut f = std::fs::File::from(fd);
        f.write_all(data).map_err(|e| format!("writing {name}: {e}"))?;
        drop(f);
        rustix::fs::renameat_with(at, tmp.as_str(), at, file, RenameFlags::NOREPLACE)
            .map_err(|e| err(&format!("renaming {name}"), e))?;
        self.created.pop();
        self.created.push((sub, file.to_string()));
        Ok(())
    }

    /// Flushes the entries of the page subfolders and of the folder to disk.
    pub fn sync(&mut self) -> Result<(), String> {
        self.current = None;
        let mut subs: Vec<&String> = self.subdirs.iter().collect();
        subs.sort();
        for d in subs {
            let fd = rustix::fs::openat(
                &self.dir,
                d.as_str(),
                OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                Mode::empty(),
            )
            .map_err(|e| err("opening a page folder", e))?;
            rustix::fs::fsync(&fd).map_err(|e| err("syncing a page folder", e))?;
        }
        self.sync_top()
    }

    /// Flushes the folder's own entries (not its page subfolders') to disk.
    pub fn sync_top(&self) -> Result<(), String> {
        rustix::fs::fsync(&self.dir).map_err(|e| err("syncing the folder", e))
    }

    /// Removes the files written, the page subfolders made and the folder (a folder that
    /// something else put a file in stays, and this fails).
    pub fn remove(mut self) -> Result<(), String> {
        self.current = None;
        for (sub, n) in &self.created {
            match sub {
                None => {
                    let _ = rustix::fs::unlinkat(&self.dir, n.as_str(), AtFlags::empty());
                }
                Some(d) => {
                    let opened = rustix::fs::openat(
                        &self.dir,
                        d.as_str(),
                        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                        Mode::empty(),
                    );
                    if let Ok(fd) = opened {
                        let _ = rustix::fs::unlinkat(&fd, n.as_str(), AtFlags::empty());
                    }
                }
            }
        }
        for d in &self.subdirs {
            let _ = rustix::fs::unlinkat(&self.dir, d.as_str(), AtFlags::REMOVEDIR);
        }
        drop(self.dir);
        rustix::fs::unlinkat(&self.parent, &self.name, AtFlags::REMOVEDIR).map_err(|e| err("removing the folder", e))
    }
}

/// The parent of `path`, opened one component at a time with `O_NOFOLLOW` (no component may be
/// a symbolic link), and the last component, which must be a name (else `Err(not_a_name)`).
fn open_parent(path: &Path, not_a_name: &str) -> Result<(OwnedFd, std::ffi::OsString), String> {
    let name = match path.components().next_back() {
        Some(Component::Normal(n)) => n.to_os_string(),
        _ => return Err(not_a_name.into()),
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
    Ok((parent, name))
}

/// A new file (`--images-plan-out`), written like the files of [`OutDir`]: through its parent's
/// descriptor, held from [`NewFile::reserve`] on and reached without following a symbolic link,
/// to a temporary name (`O_EXCL | O_NOFOLLOW`) renamed into place with `RENAME_NOREPLACE`, so the
/// file appears whole or not at all and never replaces anything.
pub struct NewFile {
    parent: OwnedFd,
    name: std::ffi::OsString,
    shown: String,
}

impl NewFile {
    /// Opens the parent of `path`; fails when a component is a symbolic link or `path` exists.
    pub fn reserve(path: &Path) -> Result<NewFile, String> {
        let (parent, name) = open_parent(path, "must end in a file name")?;
        let shown = path.display().to_string();
        match rustix::fs::statat(&parent, &name, AtFlags::SYMLINK_NOFOLLOW) {
            Ok(_) => return Err(format!("{shown} already exists")),
            Err(Errno::NOENT) => {}
            Err(e) => return Err(err(&format!("checking {shown}"), e)),
        }
        Ok(NewFile { parent, name, shown })
    }

    /// The parent's descriptor, the file's name in it and its path as shown in messages, for a
    /// file written as it goes (`pdftomarkdown --page-facts`, see `src/facts_file.rs`).
    pub fn into_parts(self) -> (OwnedFd, std::ffi::OsString, String) {
        (self.parent, self.name, self.shown)
    }

    /// Writes `data` as the file; nothing is left behind when this fails.
    pub fn write(self, data: &[u8]) -> Result<(), String> {
        let mut tmp = std::ffi::OsString::from(".");
        tmp.push(&self.name);
        tmp.push(format!(".{}.part", std::process::id()));
        let fd = rustix::fs::openat(
            &self.parent,
            &tmp,
            OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::from_raw_mode(0o600),
        )
        .map_err(|e| err(&format!("creating {}", self.shown), e))?;
        let mut f = std::fs::File::from(fd);
        let written = f.write_all(data).map_err(|e| format!("writing {}: {e}", self.shown)).and_then(|()| {
            rustix::fs::renameat_with(&self.parent, &tmp, &self.parent, &self.name, RenameFlags::NOREPLACE)
                .map_err(|e| err(&format!("renaming to {}", self.shown), e))
        });
        drop(f);
        if written.is_err() {
            let _ = rustix::fs::unlinkat(&self.parent, &tmp, AtFlags::empty());
        }
        written
    }
}

/// `manifest.json`, or an image path of the layout (`page-0012/image-003.webp`; see
/// [`super::layout::valid_rel`]).
pub fn valid_name(name: &str) -> bool {
    name == "manifest.json" || super::layout::valid_rel(name)
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
        for ok in ["page-0001/image-001.webp", "page-0003/figure-012.svg", "manifest.json"] {
            assert!(valid_name(ok), "{ok}");
        }
        for bad in [
            "p1.1.webp",
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
            "page-0001",
            "page-0001/",
            "/page-0001/image-001.webp",
            "page-0001//image-001.webp",
            "manifest.json/x",
        ] {
            assert!(!valid_name(bad), "{bad:?}");
        }
    }

    #[test]
    fn page_folders_are_private_and_removed() {
        use std::os::unix::fs::PermissionsExt;
        let base = std::env::temp_dir().join(format!("doppler-outdir-sub-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        let dir = base.join("out");
        let mut d = OutDir::create(&dir).unwrap();
        d.write("page-0001/image-001.webp", b"a").unwrap();
        d.write("page-0002/figure-001.svg", b"b").unwrap();
        // back to a page folder made earlier
        d.write("page-0001/image-002.webp", b"c").unwrap();
        assert!(d.write("page-0001/image-001.webp", b"again").is_err(), "never replaces");
        assert!(d.write("../x/image-001.webp", b"x").is_err());
        d.write("manifest.json", b"{}").unwrap();
        d.sync().unwrap();
        let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&dir), 0o700);
        assert_eq!(mode(&dir.join("page-0001")), 0o700);
        assert_eq!(mode(&dir.join("page-0001/image-002.webp")), 0o600);
        assert_eq!(std::fs::read(dir.join("page-0001/image-002.webp")).unwrap(), b"c");
        // a page folder that something else made first is refused
        std::fs::create_dir(dir.join("page-0003")).unwrap();
        assert!(d.write("page-0003/image-001.webp", b"x").is_err());
        std::fs::remove_dir(dir.join("page-0003")).unwrap();
        // a symbolic link planted as a page folder is not followed
        std::os::unix::fs::symlink(&base, dir.join("page-0004")).unwrap();
        assert!(d.write("page-0004/image-001.webp", b"x").is_err());
        std::fs::remove_file(dir.join("page-0004")).unwrap();
        d.remove().unwrap();
        assert!(!dir.exists());
        assert_eq!(std::fs::read_dir(&base).unwrap().count(), 0);
        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn new_file_is_confined_and_never_replaces() {
        let base = std::env::temp_dir().join(format!("doppler-outdir-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join("real")).unwrap();
        std::fs::create_dir_all(base.join("outside")).unwrap();
        std::os::unix::fs::symlink(base.join("outside"), base.join("link")).unwrap();
        // a symbolic link in the parent path, or as the file itself
        assert!(NewFile::reserve(&base.join("link/p.plan")).is_err());
        assert!(NewFile::reserve(&base.join("link/../real/p.plan")).is_err());
        std::os::unix::fs::symlink(base.join("outside/x"), base.join("real/leaf")).unwrap();
        assert!(NewFile::reserve(&base.join("real/leaf")).is_err());
        assert!(NewFile::reserve(&base.join("absent/p.plan")).is_err());
        assert!(NewFile::reserve(&base.join("real/..")).is_err());
        let f = NewFile::reserve(&base.join("real/p.plan")).unwrap();
        // made by something else meanwhile: kept, and the write fails without leaving a part file
        let g = NewFile::reserve(&base.join("real/q.plan")).unwrap();
        std::fs::write(base.join("real/q.plan"), b"other").unwrap();
        assert!(g.write(b"plan").is_err());
        assert_eq!(std::fs::read(base.join("real/q.plan")).unwrap(), b"other");
        f.write(b"plan").unwrap();
        assert_eq!(std::fs::read(base.join("real/p.plan")).unwrap(), b"plan");
        let mut names: Vec<String> = std::fs::read_dir(base.join("real"))
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        names.sort();
        assert_eq!(names, ["leaf", "p.plan", "q.plan"]);
        assert_eq!(std::fs::read_dir(base.join("outside")).unwrap().count(), 0);
        std::fs::remove_dir_all(&base).unwrap();
    }
}
