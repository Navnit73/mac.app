//! Thin macOS helpers: volume capacity, Full Disk Access probe, extended attributes and the
//! handful of allow-listed Finder / Quick Look / System Settings actions.
//!
//! Everything here is read-only or opens a system UI; nothing removes files.

use std::ffi::CString;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use std::process::{Command, Stdio};

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2_foundation::{
    NSArray, NSNumber, NSString, NSURLVolumeAvailableCapacityForImportantUsageKey,
    NSURLVolumeAvailableCapacityKey, NSURLVolumeLocalizedNameKey, NSURLVolumeTotalCapacityKey,
    NSURL,
};

use crate::errors::AppError;
use crate::models::{FdaStatus, VolumeInfo};

/// Capacity of the volume holding `path`.
///
/// `available_important` includes space macOS can purge on demand, matching the "free" figure in
/// System Settings; `free_raw` is what `df` reports.
pub fn volume_info(path: &Path) -> Result<VolumeInfo, AppError> {
    let p = NSString::from_str(&path.to_string_lossy());
    let url = NSURL::fileURLWithPath(&p);
    // SAFETY: these are valid, immutable Foundation constants.
    let (k_total, k_raw, k_important, k_name) = unsafe {
        (
            NSURLVolumeTotalCapacityKey,
            NSURLVolumeAvailableCapacityKey,
            NSURLVolumeAvailableCapacityForImportantUsageKey,
            NSURLVolumeLocalizedNameKey,
        )
    };
    let keys = NSArray::from_slice(&[k_total, k_raw, k_important, k_name]);
    let values = url
        .resourceValuesForKeys_error(&keys)
        .map_err(|e| AppError::Io(e.localizedDescription().to_string()))?;

    let number = |key| -> Option<u64> {
        let obj: Retained<AnyObject> = values.objectForKey(key)?;
        let n = obj.downcast::<NSNumber>().ok()?;
        u64::try_from(n.longLongValue()).ok()
    };
    let total = number(k_total).unwrap_or(0);
    let free_raw = number(k_raw).unwrap_or(0);
    let available_important = number(k_important).unwrap_or(free_raw).max(free_raw);
    let name = values
        .objectForKey(k_name)
        .and_then(|o| o.downcast::<NSString>().ok())
        .map(|s| s.to_string())
        .unwrap_or_else(|| "Macintosh HD".to_string());
    Ok(VolumeInfo {
        name,
        total,
        free_raw,
        available_important,
        purgeable: available_important.saturating_sub(free_raw),
    })
}

/// Detect Full Disk Access by trying to list folders that macOS protects with it.
///
/// Returns `Unknown` when none of the probe folders exist (nothing to learn from).
pub fn full_disk_access(home: &Path) -> FdaStatus {
    let probes = [
        home.join("Library/Safari"),
        home.join("Library/Mail"),
        home.join("Library/Messages"),
        home.join("Library/Suggestions"),
    ];
    let mut tested = false;
    for p in &probes {
        match std::fs::read_dir(p) {
            Ok(_) => return FdaStatus::Granted,
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => tested = true,
            Err(_) => {}
        }
    }
    if tested {
        FdaStatus::Denied
    } else {
        FdaStatus::Unknown
    }
}

/// Read an extended attribute holding a binary plist array of strings (tags, download URLs).
pub fn xattr_plist_strings(path: &Path, name: &str) -> Vec<String> {
    let (Ok(c_path), Ok(c_name)) = (
        CString::new(path.as_os_str().as_bytes()),
        CString::new(name),
    ) else {
        return Vec::new();
    };
    // SAFETY: valid NUL-terminated strings; a null buffer asks only for the size.
    let size = unsafe {
        libc::getxattr(
            c_path.as_ptr(),
            c_name.as_ptr(),
            std::ptr::null_mut(),
            0,
            0,
            libc::XATTR_NOFOLLOW,
        )
    };
    if size <= 0 || size > 1 << 20 {
        return Vec::new();
    }
    let mut buf = vec![0u8; size as usize];
    // SAFETY: `buf` is `size` bytes long.
    let got = unsafe {
        libc::getxattr(
            c_path.as_ptr(),
            c_name.as_ptr(),
            buf.as_mut_ptr() as *mut libc::c_void,
            buf.len(),
            0,
            libc::XATTR_NOFOLLOW,
        )
    };
    if got <= 0 {
        return Vec::new();
    }
    buf.truncate(got as usize);
    match plist::Value::from_reader(std::io::Cursor::new(buf)) {
        Ok(plist::Value::Array(items)) => {
            items.into_iter().filter_map(|v| v.into_string()).collect()
        }
        _ => Vec::new(),
    }
}

/// Bytes that would be freed if this file were deleted: APFS's private size, which leaves out
/// blocks shared with clones. `None` when the volume does not report it.
pub fn private_size(path: &Path) -> Option<u64> {
    let c_path = CString::new(path.as_os_str().as_bytes()).ok()?;
    let mut attrs = libc::attrlist {
        bitmapcount: libc::ATTR_BIT_MAP_COUNT,
        reserved: 0,
        commonattr: libc::ATTR_CMN_RETURNED_ATTRS,
        volattr: 0,
        dirattr: 0,
        fileattr: 0,
        forkattr: libc::ATTR_CMNEXT_PRIVATESIZE,
    };
    let mut buf = [0u8; 64];
    // SAFETY: `attrs` and `buf` are valid for the call and `buf.len()` is its real size.
    let rc = unsafe {
        libc::getattrlist(
            c_path.as_ptr(),
            &mut attrs as *mut libc::attrlist as *mut libc::c_void,
            buf.as_mut_ptr() as *mut libc::c_void,
            buf.len(),
            libc::FSOPT_ATTR_CMN_EXTENDED | libc::FSOPT_NOFOLLOW,
        )
    };
    if rc != 0 {
        return None;
    }
    // Layout: u32 length, attribute_set_t (5 x u32), then the off_t private size.
    let returned_fork = u32::from_ne_bytes(buf.get(20..24)?.try_into().ok()?);
    if returned_fork & libc::ATTR_CMNEXT_PRIVATESIZE == 0 {
        return None;
    }
    let v = i64::from_ne_bytes(buf.get(24..32)?.try_into().ok()?);
    u64::try_from(v).ok()
}

/// APFS copy-on-write clone of `src` at `dst` (which must not exist). Never follows symlinks.
pub fn clone_file(src: &Path, dst: &Path) -> Result<(), AppError> {
    let (Ok(s), Ok(d)) = (
        CString::new(src.as_os_str().as_bytes()),
        CString::new(dst.as_os_str().as_bytes()),
    ) else {
        return Err(AppError::InvalidInput("path contains NUL".into()));
    };
    const CLONE_NOFOLLOW: u32 = 0x0001;
    // SAFETY: both are valid NUL-terminated strings for the duration of the call.
    let rc = unsafe { libc::clonefile(s.as_ptr(), d.as_ptr(), CLONE_NOFOLLOW) };
    if rc == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error().into())
    }
}

/// Spotlight's "last opened" time as Unix seconds, if known.
pub fn last_used(path: &Path) -> Option<i64> {
    let out = Command::new("/usr/bin/mdls")
        .args(["-name", "kMDItemLastUsedDate", "-raw"])
        .arg(path)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    parse_mdls_date(String::from_utf8_lossy(&out.stdout).trim())
}

/// Parse `2026-10-01 10:11:12 +0000` into Unix seconds.
pub fn parse_mdls_date(s: &str) -> Option<i64> {
    let mut parts = s.split_whitespace();
    let date = parts.next()?;
    let time = parts.next()?;
    let mut d = date.split('-').map(|x| x.parse::<i64>().ok());
    let (y, m, day) = (d.next()??, d.next()??, d.next()??);
    let mut t = time.split(':').map(|x| x.parse::<i64>().ok());
    let (hh, mm, ss) = (t.next()??, t.next()??, t.next()??);
    // Days since epoch (Howard Hinnant's civil-from-days inverse).
    let y2 = if m <= 2 { y - 1 } else { y };
    let era = y2.div_euclid(400);
    let yoe = y2 - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    let mut secs = days * 86_400 + hh * 3600 + mm * 60 + ss;
    if let Some(off) = parts.next() {
        let sign = if off.starts_with('-') { -1 } else { 1 };
        let digits = off.trim_start_matches(['+', '-']);
        if digits.len() == 4 {
            let oh: i64 = digits[..2].parse().ok()?;
            let om: i64 = digits[2..].parse().ok()?;
            secs -= sign * (oh * 3600 + om * 60);
        }
    }
    Some(secs)
}

fn spawn_detached(cmd: &str, args: &[&std::ffi::OsStr]) -> Result<(), AppError> {
    Command::new(cmd)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|mut child| {
            // Reap the child in the background so it never lingers as a zombie.
            std::thread::spawn(move || {
                let _ = child.wait();
            });
        })
        .map_err(AppError::from)
}

/// Reveal in Finder (`open -R`). `path` must be absolute.
pub fn reveal_in_finder(path: &Path) -> Result<(), AppError> {
    require_absolute(path)?;
    spawn_detached("/usr/bin/open", &["-R".as_ref(), path.as_os_str()])
}

/// Open with the default application (`open`).
pub fn open_path(path: &Path) -> Result<(), AppError> {
    require_absolute(path)?;
    spawn_detached("/usr/bin/open", &["--".as_ref(), path.as_os_str()])
}

/// Quick Look preview (`qlmanage -p`).
pub fn quick_look(path: &Path) -> Result<(), AppError> {
    require_absolute(path)?;
    spawn_detached("/usr/bin/qlmanage", &["-p".as_ref(), path.as_os_str()])
}

/// Open System Settings at Privacy & Security > Full Disk Access.
pub fn open_full_disk_access_settings() -> Result<(), AppError> {
    spawn_detached(
        "/usr/bin/open",
        &["x-apple.systempreferences:com.apple.preference.security?Privacy_AllFiles".as_ref()],
    )
}

/// The path must be absolute and still exist (it may have changed since the scan).
fn require_absolute(path: &Path) -> Result<(), AppError> {
    if !path.is_absolute() {
        return Err(AppError::InvalidInput("path must be absolute".into()));
    }
    if std::fs::symlink_metadata(path).is_err() {
        return Err(AppError::NotFound(
            "This item no longer exists. It changed after the scan.".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_mdls_dates() {
        assert_eq!(parse_mdls_date("1970-01-01 00:00:00 +0000"), Some(0));
        assert_eq!(
            parse_mdls_date("2000-03-01 00:00:00 +0000"),
            Some(951_868_800)
        );
        assert_eq!(
            parse_mdls_date("2000-03-01 01:00:00 +0100"),
            Some(951_868_800)
        );
        assert_eq!(parse_mdls_date("(null)"), None);
    }

    #[test]
    fn relative_paths_are_rejected() {
        assert!(open_path(Path::new("relative/file")).is_err());
        assert!(reveal_in_finder(Path::new("x")).is_err());
        assert!(quick_look(Path::new("x")).is_err());
    }

    #[test]
    fn private_size_and_clones() {
        use std::io::Write;
        let tmp = tempfile::tempdir().unwrap();
        let a = tmp.path().join("a.bin");
        std::fs::File::create(&a)
            .unwrap()
            .write_all(&vec![7u8; 1 << 20])
            .unwrap();
        let before = private_size(&a).unwrap();
        assert!(before >= 1 << 20, "a lone file owns all its blocks");
        let b = tmp.path().join("b.bin");
        clone_file(&a, &b).unwrap();
        assert_eq!(std::fs::read(&a).unwrap(), std::fs::read(&b).unwrap());
        // After cloning, the blocks are shared, so removing one copy frees (almost) nothing.
        assert!(private_size(&b).unwrap() < before / 4);
        assert!(clone_file(&a, &b).is_err(), "never overwrites");
    }

    #[test]
    fn missing_paths_are_reported() {
        let e = open_path(Path::new("/definitely/not/here.txt")).unwrap_err();
        assert!(matches!(e, AppError::NotFound(_)));
    }

    #[test]
    fn volume_info_for_root_is_sane() {
        let v = volume_info(Path::new("/")).unwrap();
        assert!(v.total > 0);
        assert!(v.available_important >= v.free_raw);
        assert!(v.free_raw <= v.total);
    }

    #[test]
    fn fda_probe_does_not_panic() {
        let _ = full_disk_access(Path::new("/nonexistent-home"));
        assert_eq!(
            full_disk_access(Path::new("/nonexistent-home")),
            FdaStatus::Unknown
        );
    }
}
