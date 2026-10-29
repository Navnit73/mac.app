//! `getattrlistbulk` reader: one syscall returns the attributes of many directory entries,
//! avoiding a separate `lstat` per file. Falls back to [`StdReader`] when the filesystem
//! does not support bulk listing.
//!
//! Differences from `StdReader`: directories report no own size (APFS directories occupy no
//! blocks anyway) and symlinks never contribute allocated bytes.

use std::ffi::CString;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

use super::reader::{DirListing, DirReader, RawEntry, ReadError};
use super::std_reader::StdReader;
use crate::index::NodeKind;

const ATTR_CMN_ERROR: u32 = 0x2000_0000;
const VREG: u32 = 1;
const VDIR: u32 = 2;
const VLNK: u32 = 5;
const BUF_SIZE: usize = 256 * 1024;

#[derive(Debug, Default, Clone, Copy)]
pub struct BulkReader {
    fallback: StdReader,
}

impl BulkReader {
    pub fn new() -> Self {
        Self::default()
    }
}

/// Cursor over one entry's bytes. Every read is bounds-checked and alignment-agnostic.
struct Cursor<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let s = self.buf.get(self.pos..self.pos.checked_add(n)?)?;
        self.pos += n;
        Some(s)
    }
    fn u32(&mut self) -> Option<u32> {
        self.take(4)
            .map(|b| u32::from_ne_bytes([b[0], b[1], b[2], b[3]]))
    }
    fn i32(&mut self) -> Option<i32> {
        self.u32().map(|v| v as i32)
    }
    fn i64(&mut self) -> Option<i64> {
        self.take(8).map(|b| {
            let mut a = [0u8; 8];
            a.copy_from_slice(b);
            i64::from_ne_bytes(a)
        })
    }
    fn u64(&mut self) -> Option<u64> {
        self.i64().map(|v| v as u64)
    }
}

/// Parse one packed entry (`entry` starts at its length field). Returns `Ok(None)` for an
/// entry that carried an error code (reported via `listing.errors`).
fn parse_entry(entry: &[u8], listing: &mut DirListing) -> Option<()> {
    let mut c = Cursor { buf: entry, pos: 4 };
    // attribute_set_t: commonattr, volattr, dirattr, fileattr, forkattr
    let common = c.u32()?;
    let _vol = c.u32()?;
    let _dir = c.u32()?;
    let file = c.u32()?;
    let _fork = c.u32()?;

    let mut name_ref_pos = None;
    let mut name_ref = (0i32, 0u32);
    if common & ATTR_CMN_ERROR != 0 {
        let code = c.u32()?;
        // The name is usually packed even on error; try to recover it for the report.
        let name = if common & libc::ATTR_CMN_NAME != 0 {
            let pos = c.pos;
            let off = c.i32()?;
            let len = c.u32()?;
            read_name(entry, pos, off, len)
        } else {
            Vec::new()
        };
        let err = match code as i32 {
            libc::EACCES | libc::EPERM => ReadError::PermissionDenied,
            libc::ENOENT => ReadError::Vanished,
            other => ReadError::Io(format!("errno {other}")),
        };
        listing.errors.push((name, err));
        return Some(());
    }
    if common & libc::ATTR_CMN_NAME != 0 {
        name_ref_pos = Some(c.pos);
        name_ref = (c.i32()?, c.u32()?);
    }
    let dev = if common & libc::ATTR_CMN_DEVID != 0 {
        c.u32()?
    } else {
        0
    };
    let objtype = if common & libc::ATTR_CMN_OBJTYPE != 0 {
        c.u32()?
    } else {
        VREG
    };
    let ts = |bit: u32, c: &mut Cursor| -> Option<i64> {
        if common & bit != 0 {
            let sec = c.i64()?;
            let _nsec = c.i64()?;
            Some(sec)
        } else {
            Some(0)
        }
    };
    let mtime = ts(libc::ATTR_CMN_MODTIME, &mut c)?;
    let atime = ts(libc::ATTR_CMN_ACCTIME, &mut c)?;
    let st_flags = if common & libc::ATTR_CMN_FLAGS != 0 {
        c.u32()?
    } else {
        0
    };
    let inode = if common & libc::ATTR_CMN_FILEID != 0 {
        c.u64()?
    } else {
        0
    };
    let nlink = if file & libc::ATTR_FILE_LINKCOUNT != 0 {
        c.u32()?
    } else {
        1
    };
    let alloc = if file & libc::ATTR_FILE_ALLOCSIZE != 0 {
        c.i64()?
    } else {
        0
    };
    let logical = if file & libc::ATTR_FILE_DATALENGTH != 0 {
        c.i64()?
    } else {
        0
    };

    let name = read_name(entry, name_ref_pos?, name_ref.0, name_ref.1);
    let kind = match objtype {
        VDIR => NodeKind::Dir,
        VLNK => NodeKind::Symlink,
        _ => NodeKind::File,
    };
    let (size_alloc, size_logical) = if kind == NodeKind::File {
        (alloc.max(0) as u64, logical.max(0) as u64)
    } else {
        (0, 0)
    };
    listing.entries.push(RawEntry {
        name,
        kind,
        size_logical,
        size_alloc,
        mtime,
        atime,
        inode,
        dev,
        nlink,
        st_flags,
    });
    Some(())
}

/// Read an `attrreference_t` string that lives inside the entry buffer.
fn read_name(entry: &[u8], ref_pos: usize, off: i32, len: u32) -> Vec<u8> {
    let start = (ref_pos as i64) + off as i64;
    let Ok(start) = usize::try_from(start) else {
        return Vec::new();
    };
    let Some(end) = start.checked_add(len as usize) else {
        return Vec::new();
    };
    match entry.get(start..end) {
        // `len` includes the terminating NUL.
        Some(b) => b.split(|x| *x == 0).next().unwrap_or(&[]).to_vec(),
        None => Vec::new(),
    }
}

fn errno_to_read_error(e: i32) -> ReadError {
    match e {
        libc::EACCES | libc::EPERM => ReadError::PermissionDenied,
        libc::ENOENT | libc::ENOTDIR => ReadError::Vanished,
        other => ReadError::Io(format!("errno {other}")),
    }
}

impl DirReader for BulkReader {
    fn read_dir(&self, path: &Path) -> Result<DirListing, ReadError> {
        let c_path = CString::new(path.as_os_str().as_bytes())
            .map_err(|_| ReadError::Io("path contains NUL".into()))?;
        // SAFETY: `c_path` is a valid NUL-terminated string for the duration of the call.
        let fd = unsafe {
            libc::open(
                c_path.as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC | libc::O_NOFOLLOW,
            )
        };
        if fd < 0 {
            let e = std::io::Error::last_os_error().raw_os_error().unwrap_or(0);
            return Err(errno_to_read_error(e));
        }
        let result = self.list_fd(fd, path);
        // SAFETY: `fd` was opened above and is closed exactly once.
        unsafe { libc::close(fd) };
        result
    }

    fn stat(&self, path: &Path) -> Result<RawEntry, ReadError> {
        self.fallback.stat(path)
    }
}

impl BulkReader {
    fn list_fd(&self, fd: i32, path: &Path) -> Result<DirListing, ReadError> {
        let mut attrs = libc::attrlist {
            bitmapcount: libc::ATTR_BIT_MAP_COUNT,
            reserved: 0,
            commonattr: libc::ATTR_CMN_RETURNED_ATTRS
                | libc::ATTR_CMN_NAME
                | libc::ATTR_CMN_DEVID
                | libc::ATTR_CMN_OBJTYPE
                | libc::ATTR_CMN_MODTIME
                | libc::ATTR_CMN_ACCTIME
                | libc::ATTR_CMN_FLAGS
                | libc::ATTR_CMN_FILEID
                | ATTR_CMN_ERROR,
            volattr: 0,
            dirattr: 0,
            fileattr: libc::ATTR_FILE_LINKCOUNT
                | libc::ATTR_FILE_ALLOCSIZE
                | libc::ATTR_FILE_DATALENGTH,
            forkattr: 0,
        };
        let mut buf = vec![0u8; BUF_SIZE];
        let mut out = DirListing::default();
        loop {
            // SAFETY: `attrs` and `buf` are valid for the call; `buf.len()` is its true size.
            let n = unsafe {
                libc::getattrlistbulk(
                    fd,
                    &mut attrs as *mut libc::attrlist as *mut libc::c_void,
                    buf.as_mut_ptr() as *mut libc::c_void,
                    buf.len(),
                    0,
                )
            };
            if n < 0 {
                let e = std::io::Error::last_os_error().raw_os_error().unwrap_or(0);
                if e == libc::ENOTSUP || e == libc::EINVAL {
                    return self.fallback.read_dir(path);
                }
                return Err(errno_to_read_error(e));
            }
            if n == 0 {
                return Ok(out);
            }
            let mut pos = 0usize;
            for _ in 0..n {
                let Some(len_bytes) = buf.get(pos..pos + 4) else {
                    return Err(ReadError::Io("truncated bulk buffer".into()));
                };
                let len =
                    u32::from_ne_bytes([len_bytes[0], len_bytes[1], len_bytes[2], len_bytes[3]])
                        as usize;
                let Some(entry) = buf.get(pos..pos + len).filter(|_| len >= 24) else {
                    return Err(ReadError::Io("malformed bulk entry".into()));
                };
                if parse_entry(entry, &mut out).is_none() {
                    return Err(ReadError::Io("could not parse bulk entry".into()));
                }
                pos += len;
            }
        }
    }
}
