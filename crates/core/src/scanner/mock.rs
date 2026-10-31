//! In-memory fake filesystem for tests and benchmarks.

use std::collections::HashMap;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use super::reader::{DirListing, DirReader, RawEntry, ReadError};
use crate::index::NodeKind;

#[derive(Default)]
pub struct MockFs {
    dirs: HashMap<PathBuf, Vec<RawEntry>>,
    errors: HashMap<PathBuf, ReadError>,
    roots: HashMap<PathBuf, RawEntry>,
    /// Artificial latency per `read_dir`, for cancellation tests.
    pub delay: Duration,
    next_inode: Mutex<u64>,
}

fn entry(name: &[u8], kind: NodeKind, size: u64, dev: u32, inode: u64) -> RawEntry {
    RawEntry {
        name: name.to_vec(),
        kind,
        size_logical: size,
        size_alloc: size.div_ceil(4096) * 4096,
        mtime: 0,
        atime: 0,
        inode,
        dev,
        nlink: 1,
        st_flags: 0,
    }
}

impl MockFs {
    pub fn new() -> Self {
        Self::default()
    }

    fn inode(&self) -> u64 {
        let mut g = self.next_inode.lock().unwrap_or_else(|e| e.into_inner());
        *g += 1;
        *g
    }

    /// Add a directory (parents must be added first, except for roots).
    pub fn add_dir(&mut self, path: &str, dev: u32) -> &mut Self {
        let p = PathBuf::from(path);
        let e = entry(file_name(&p), NodeKind::Dir, 0, dev, self.inode());
        self.dirs.entry(p.clone()).or_default();
        self.attach(&p, e);
        self
    }

    /// A directory whose contents live only in iCloud (`SF_DATALESS`). Its children are listed
    /// but must never be read by the scanner.
    pub fn add_dataless_dir(&mut self, path: &str) -> &mut Self {
        let p = PathBuf::from(path);
        let mut e = entry(file_name(&p), NodeKind::Dir, 0, 1, self.inode());
        e.st_flags = super::reader::st_flags::SF_DATALESS;
        self.dirs.entry(p.clone()).or_default();
        self.attach(&p, e);
        self
    }

    pub fn add_file(&mut self, path: &str, size: u64, dev: u32) -> &mut Self {
        let p = PathBuf::from(path);
        let e = entry(file_name(&p), NodeKind::File, size, dev, self.inode());
        self.attach(&p, e);
        self
    }

    /// Add another name for an existing inode.
    pub fn add_hardlink(&mut self, path: &str, size: u64, dev: u32, inode: u64) -> &mut Self {
        let p = PathBuf::from(path);
        let mut e = entry(file_name(&p), NodeKind::File, size, dev, inode);
        e.nlink = 2;
        self.attach(&p, e);
        self
    }

    pub fn add_symlink(&mut self, path: &str) -> &mut Self {
        let p = PathBuf::from(path);
        let e = entry(file_name(&p), NodeKind::Symlink, 10, 1, self.inode());
        self.attach(&p, e);
        self
    }

    pub fn fail_dir(&mut self, path: &str, err: ReadError) -> &mut Self {
        self.errors.insert(PathBuf::from(path), err);
        self
    }

    fn attach(&mut self, p: &Path, e: RawEntry) {
        match p.parent().filter(|pp| self.dirs.contains_key(*pp)) {
            Some(parent) => self.dirs.entry(parent.to_path_buf()).or_default().push(e),
            None => {
                self.roots.insert(p.to_path_buf(), e);
            }
        }
    }

    /// Wide synthetic tree of roughly `files` files: `fanout` subdirs per level, 3 levels.
    pub fn synthetic(root: &str, files: usize, fanout: usize) -> Self {
        let mut fs = MockFs::new();
        fs.add_dir(root, 1);
        let dirs = fanout * fanout * fanout;
        let per_dir = (files / dirs.max(1)).max(1);
        for a in 0..fanout {
            let pa = format!("{root}/a{a}");
            fs.add_dir(&pa, 1);
            for b in 0..fanout {
                let pb = format!("{pa}/b{b}");
                fs.add_dir(&pb, 1);
                for c in 0..fanout {
                    let pc = format!("{pb}/c{c}");
                    fs.add_dir(&pc, 1);
                    for f in 0..per_dir {
                        fs.add_file(&format!("{pc}/f{f}.dat"), 1000 + f as u64, 1);
                    }
                }
            }
        }
        fs
    }
}

fn file_name(p: &Path) -> &[u8] {
    match p.file_name() {
        Some(n) => n.as_bytes(),
        None => p.as_os_str().as_bytes(),
    }
}

impl DirReader for MockFs {
    fn read_dir(&self, path: &Path) -> Result<DirListing, ReadError> {
        if !self.delay.is_zero() {
            std::thread::sleep(self.delay);
        }
        if let Some(e) = self.errors.get(path) {
            return Err(e.clone());
        }
        match self.dirs.get(path) {
            Some(v) => Ok(DirListing {
                entries: v.clone(),
                errors: Vec::new(),
            }),
            None => Err(ReadError::Vanished),
        }
    }

    fn stat(&self, path: &Path) -> Result<RawEntry, ReadError> {
        if let Some(e) = self.roots.get(path) {
            let mut e = e.clone();
            e.name = path.as_os_str().as_bytes().to_vec();
            return Ok(e);
        }
        let parent = path.parent().ok_or(ReadError::Vanished)?;
        let name = file_name(path);
        self.dirs
            .get(parent)
            .and_then(|v| v.iter().find(|e| e.name == name))
            .map(|e| {
                let mut e = e.clone();
                e.name = path.as_os_str().as_bytes().to_vec();
                e
            })
            .ok_or(ReadError::Vanished)
    }
}
