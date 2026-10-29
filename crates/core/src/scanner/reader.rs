//! Filesystem access behind a trait so the walker can be tested without a disk.

use std::path::Path;

use crate::index::NodeKind;

/// `st_flags` bits (see `<sys/stat.h>`).
pub mod st_flags {
    pub const UF_COMPRESSED: u32 = 0x20;
    pub const UF_HIDDEN: u32 = 0x8000;
    pub const SF_DATALESS: u32 = 0x4000_0000;
}

/// One directory entry with the attributes the scanner needs. Never follows symlinks.
#[derive(Debug, Clone)]
pub struct RawEntry {
    pub name: Vec<u8>,
    pub kind: NodeKind,
    pub size_logical: u64,
    /// `st_blocks * 512`.
    pub size_alloc: u64,
    pub mtime: i64,
    pub atime: i64,
    pub inode: u64,
    pub dev: u32,
    pub nlink: u32,
    pub st_flags: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReadError {
    PermissionDenied,
    Vanished,
    Io(String),
}

impl From<&std::io::Error> for ReadError {
    fn from(e: &std::io::Error) -> Self {
        match e.kind() {
            std::io::ErrorKind::PermissionDenied => ReadError::PermissionDenied,
            std::io::ErrorKind::NotFound => ReadError::Vanished,
            _ => ReadError::Io(e.to_string()),
        }
    }
}

/// Result of listing a directory: entries plus per-entry failures.
#[derive(Debug, Default)]
pub struct DirListing {
    pub entries: Vec<RawEntry>,
    pub errors: Vec<(Vec<u8>, ReadError)>,
}

pub trait DirReader: Send + Sync {
    /// List a directory. Must not follow symlinks.
    fn read_dir(&self, path: &Path) -> Result<DirListing, ReadError>;
    /// `lstat` a single path (used for scan roots).
    fn stat(&self, path: &Path) -> Result<RawEntry, ReadError>;
}
