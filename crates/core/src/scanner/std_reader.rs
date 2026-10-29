//! Portable reader built on `std::fs` (readdir + lstat). Baseline and fallback.

use std::fs;
use std::os::macos::fs::MetadataExt;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

use super::reader::{DirListing, DirReader, RawEntry, ReadError};
use crate::index::NodeKind;

#[derive(Debug, Default, Clone, Copy)]
pub struct StdReader;

pub(crate) fn entry_from_metadata(name: Vec<u8>, m: &fs::Metadata) -> RawEntry {
    let ft = m.file_type();
    let kind = if ft.is_symlink() {
        NodeKind::Symlink
    } else if ft.is_dir() {
        NodeKind::Dir
    } else {
        NodeKind::File
    };
    RawEntry {
        name,
        kind,
        size_logical: m.st_size(),
        size_alloc: m.st_blocks().saturating_mul(512),
        mtime: m.st_mtime(),
        atime: m.st_atime(),
        inode: m.st_ino(),
        dev: m.st_dev() as u32,
        nlink: m.st_nlink() as u32,
        st_flags: m.st_flags(),
    }
}

impl DirReader for StdReader {
    fn read_dir(&self, path: &Path) -> Result<DirListing, ReadError> {
        let rd = fs::read_dir(path).map_err(|e| ReadError::from(&e))?;
        let mut out = DirListing::default();
        for item in rd {
            let entry = match item {
                Ok(e) => e,
                Err(e) => {
                    out.errors.push((Vec::new(), ReadError::from(&e)));
                    continue;
                }
            };
            let name = entry.file_name().as_bytes().to_vec();
            // `symlink_metadata` is lstat: symlinks are never followed.
            match fs::symlink_metadata(entry.path()) {
                Ok(m) => out.entries.push(entry_from_metadata(name, &m)),
                Err(e) => out.errors.push((name, ReadError::from(&e))),
            }
        }
        Ok(out)
    }

    fn stat(&self, path: &Path) -> Result<RawEntry, ReadError> {
        let m = fs::symlink_metadata(path).map_err(|e| ReadError::from(&e))?;
        Ok(entry_from_metadata(
            path.as_os_str().as_bytes().to_vec(),
            &m,
        ))
    }
}
