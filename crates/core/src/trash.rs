//! The only module allowed to remove files. Everything goes to the Trash first and is recorded
//! so it can be undone; the single permanent action is emptying the Trash, done by Finder.
//!
//! Before each item moves it is checked again (`preflight`): the path must be absolute and
//! normalized, reachable without passing through a symlink, outside the protected list, inside a
//! scanned root, and unchanged since the scan (same device, inode, size and modification time).
//! Any failure skips that item with a reason; a batch is never aborted half way.

use std::ffi::{CString, OsString};
use std::os::macos::fs::MetadataExt;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};

use objc2_foundation::{NSFileManager, NSString, NSURL};

use crate::errors::AppError;

/// Largest number of items accepted in one batch.
pub const MAX_BATCH: usize = 10_000;

/// Why `path` may never be moved to the Trash, or `None` if it is allowed.
///
/// Matching is case-insensitive (macOS volumes usually are) and component-wise, so
/// `/System-old` is not caught by `/System` but `/system/Library` is.
pub fn protected_reason(path: &Path) -> Option<&'static str> {
    // `Path::components` silently drops `.`, so check the raw text too.
    let raw = path.as_os_str().as_encoded_bytes();
    let dotted = raw.windows(3).any(|w| w == b"/./")
        || raw.ends_with(b"/.")
        || raw.windows(4).any(|w| w == b"/../")
        || raw.ends_with(b"/..");
    if dotted
        || !path.is_absolute()
        || path
            .components()
            .any(|c| matches!(c, Component::ParentDir | Component::CurDir))
    {
        return Some("Only absolute, normalized paths can be changed.");
    }
    let lower: Vec<String> = path
        .components()
        .filter_map(|c| match c {
            Component::Normal(s) => Some(s.to_string_lossy().to_lowercase()),
            _ => None,
        })
        .collect();
    let parts: Vec<&str> = lower.iter().map(String::as_str).collect();
    let starts = |prefix: &[&str]| parts.starts_with(prefix);
    const SYSTEM: &str = "Part of macOS. It is protected and cannot be removed.";

    if parts.is_empty() {
        return Some("The startup volume itself cannot be moved.");
    }
    if starts(&["system"]) || starts(&["bin"]) || starts(&["sbin"]) {
        return Some(SYSTEM);
    }
    // /Applications, /Library, /opt, /Users ... themselves; what is inside them is checked below.
    if parts.len() == 1 {
        return Some("A top-level folder of the startup volume cannot be moved.");
    }
    if starts(&["library", "keychains"]) {
        return Some("Keychains hold your passwords and certificates. They cannot be moved.");
    }
    // /Library/Application Support, /Library/Preferences ... themselves.
    if starts(&["library"]) && parts.len() == 2 {
        return Some(SYSTEM);
    }
    if starts(&["users"]) && parts.len() >= 3 {
        if let Some(r) = protected_in_home(&parts[2..]) {
            return Some(r);
        }
    }
    if starts(&["usr"]) && !starts(&["usr", "local"]) {
        return Some(SYSTEM);
    }
    if starts(&["private", "var", "db"]) || starts(&["library", "apple"]) {
        return Some(SYSTEM);
    }
    // /private itself, /private/var and /private/etc hold system state.
    if starts(&["private"]) && parts.len() <= 2 {
        return Some(SYSTEM);
    }
    if parts.contains(&".trash") || parts.contains(&".trashes") {
        return Some("Already in the Trash.");
    }
    if starts(&["volumes"]) && parts.len() <= 2 {
        return Some("A mounted volume cannot be moved.");
    }
    if starts(&["users"]) && parts.len() <= 2 {
        return Some("A user's home folder cannot be moved.");
    }
    None
}

/// Checks below a home folder; `rel` is the lower-cased components after `/Users/<name>`.
fn protected_in_home(rel: &[&str]) -> Option<&'static str> {
    const STANDARD: &[&str] = &[
        "applications",
        "desktop",
        "documents",
        "downloads",
        "library",
        "movies",
        "music",
        "pictures",
        "public",
        "sites",
    ];
    match rel {
        [name] if STANDARD.contains(name) => {
            Some("A standard folder of your home folder cannot be moved. Items inside it can.")
        }
        // Private keys cannot be downloaded again.
        [".ssh", ..] | [".gnupg", ..] => Some("Holds your private keys. DuoBolt never moves it."),
        ["library", "keychains", ..] => {
            Some("Keychains hold your passwords and certificates. They cannot be moved.")
        }
        // Moving an iCloud Drive or cloud provider folder deletes it on every device.
        ["library", "mobile documents" | "cloudstorage", ..] if rel.len() <= 3 => {
            Some("A synced cloud folder. Removing it would remove it from your other devices too.")
        }
        // ~/Library/Caches, ~/Library/Mail, ~/Library/Containers ... themselves.
        ["library", _] => {
            Some("A system folder in your Library cannot be moved. Items inside it can.")
        }
        _ => None,
    }
}

/// `rename` that fails with `AlreadyExists` instead of replacing whatever is at `to`.
pub fn rename_no_replace(from: &Path, to: &Path) -> std::io::Result<()> {
    use std::os::unix::ffi::OsStrExt;
    let (Ok(f), Ok(t)) = (
        CString::new(from.as_os_str().as_bytes()),
        CString::new(to.as_os_str().as_bytes()),
    ) else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "path contains NUL",
        ));
    };
    // SAFETY: both are valid NUL-terminated strings for the duration of the call.
    let rc = unsafe { libc::renamex_np(f.as_ptr(), t.as_ptr(), libc::RENAME_EXCL) };
    if rc == 0 {
        return Ok(());
    }
    let err = std::io::Error::last_os_error();
    // Some volumes (exFAT, network shares) lack exclusive rename: check, then rename.
    if err.raw_os_error() == Some(libc::ENOTSUP) {
        if std::fs::symlink_metadata(to).is_ok() {
            return Err(std::io::ErrorKind::AlreadyExists.into());
        }
        return std::fs::rename(from, to);
    }
    Err(err)
}

/// What the scan recorded about an item; it must still match right before the move.
#[derive(Debug, Clone)]
pub struct Expected {
    pub dev: u32,
    pub inode: u64,
    pub size: u64,
    pub mtime: i64,
    pub is_dir: bool,
}

#[derive(Debug, Clone)]
pub struct Candidate {
    pub path: PathBuf,
    pub expected: Expected,
}

/// Facts about the environment that the checks need.
#[derive(Debug, Clone, Default)]
pub struct Context {
    /// Scan roots; items must lie strictly inside one of them.
    pub roots: Vec<PathBuf>,
    /// DuoBolt's own `.app` bundle, if running from one.
    pub app_bundle: Option<PathBuf>,
}

impl Context {
    pub fn new(roots: Vec<PathBuf>) -> Self {
        Self {
            roots,
            app_bundle: own_app_bundle(),
        }
    }
}

/// The `.app` folder this process runs from, if any.
pub fn own_app_bundle() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    exe.ancestors()
        .find(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("app")))
        .map(Path::to_path_buf)
}

/// Every check that does not change anything. `Ok` means the item may move now.
pub fn preflight(c: &Candidate, ctx: &Context) -> Result<(), String> {
    let path = &c.path;
    if let Some(r) = protected_reason(path) {
        return Err(r.to_string());
    }
    let name = path
        .file_name()
        .ok_or_else(|| "This path has no file name.".to_string())?;
    let parent = path
        .parent()
        .ok_or_else(|| "This path has no parent folder.".to_string())?;

    if let Some(app) = &ctx.app_bundle {
        if path.starts_with(app) || app.starts_with(path) {
            return Err("DuoBolt cannot move itself.".into());
        }
    }
    if !ctx
        .roots
        .iter()
        .any(|r| path.starts_with(r) && path.as_path() != r.as_path())
    {
        return Err("Outside the scanned folders.".into());
    }

    // Resolve the parent for real; if that changes the path, a folder on the way is a symlink
    // (or was replaced by one), and the item could be somewhere else entirely.
    let real_parent = std::fs::canonicalize(parent)
        .map_err(|_| "The folder holding this item no longer exists.".to_string())?;
    if real_parent.join(name) != *path {
        return Err("The path goes through a symbolic link, so it is not moved.".into());
    }
    if let Some(r) = protected_reason(&real_parent.join(name)) {
        return Err(r.to_string());
    }

    let meta = std::fs::symlink_metadata(path)
        .map_err(|_| "It no longer exists. It changed after the scan.".to_string())?;
    let parent_meta = std::fs::symlink_metadata(&real_parent)
        .map_err(|_| "The folder holding this item no longer exists.".to_string())?;
    if meta.is_dir() && meta.st_dev() != parent_meta.st_dev() {
        return Err("This is where another volume is mounted.".into());
    }

    let e = &c.expected;
    let same_identity = meta.st_dev() as u32 == e.dev && meta.st_ino() == e.inode;
    let same_kind = meta.is_dir() == e.is_dir && !(meta.file_type().is_symlink() && e.is_dir);
    // Directory sizes are not stable across readers, so folders compare identity and time only.
    let same_content = meta.st_mtime() == e.mtime && (e.is_dir || meta.st_size() == e.size);
    if !(same_identity && same_kind && same_content) {
        return Err("It changed after the scan. Scan again to review it.".into());
    }
    Ok(())
}

/// Moves one item to a Trash and reports where it ended up.
pub trait TrashBackend: Send + Sync {
    fn trash(&self, path: &Path) -> Result<PathBuf, String>;
}

/// The real Trash, through `NSFileManager`. Symlinks are moved themselves, never their targets.
#[derive(Debug, Default, Clone, Copy)]
pub struct MacTrash;

impl TrashBackend for MacTrash {
    fn trash(&self, path: &Path) -> Result<PathBuf, String> {
        let fm = NSFileManager::defaultManager();
        let url = NSURL::fileURLWithPath(&NSString::from_str(&path.to_string_lossy()));
        let mut resulting = None;
        fm.trashItemAtURL_resultingItemURL_error(&url, Some(&mut resulting))
            .map_err(|e| e.localizedDescription().to_string())?;
        resulting
            .and_then(|u| u.path())
            .map(|p| PathBuf::from(p.to_string()))
            .ok_or_else(|| "macOS did not report where the item went.".to_string())
    }
}

/// Test double: moves items into a folder named `.Trash` with unique names.
#[derive(Debug)]
pub struct FakeTrash {
    pub dir: PathBuf,
    counter: std::sync::atomic::AtomicU64,
}

impl FakeTrash {
    /// `parent` gets a `.Trash` child, so restore's "came from a Trash" check accepts it.
    pub fn new(parent: &Path) -> std::io::Result<Self> {
        let dir = parent.join(".Trash");
        std::fs::create_dir_all(&dir)?;
        Ok(Self {
            dir,
            counter: std::sync::atomic::AtomicU64::new(0),
        })
    }
}

impl TrashBackend for FakeTrash {
    fn trash(&self, path: &Path) -> Result<PathBuf, String> {
        let n = self
            .counter
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let mut name = OsString::from(format!("{n}-"));
        name.push(path.file_name().unwrap_or_default());
        let dest = self.dir.join(name);
        std::fs::rename(path, &dest).map_err(|e| e.to_string())?;
        Ok(dest)
    }
}

#[derive(Debug, Clone)]
pub struct Moved {
    pub original: PathBuf,
    pub trashed: PathBuf,
    pub size: u64,
}

#[derive(Debug, Default)]
pub struct MoveOutcome {
    pub moved: Vec<Moved>,
    pub skipped: Vec<(PathBuf, u64, String)>,
}

/// Check and move each candidate. `sizes` are the bytes shown to the user for each candidate.
pub fn move_items(
    candidates: &[(Candidate, u64)],
    ctx: &Context,
    backend: &dyn TrashBackend,
) -> MoveOutcome {
    let mut out = MoveOutcome::default();
    for (i, (c, size)) in candidates.iter().enumerate() {
        if i >= MAX_BATCH {
            out.skipped.push((
                c.path.clone(),
                *size,
                format!("A single batch is limited to {MAX_BATCH} items."),
            ));
            continue;
        }
        if let Err(reason) = preflight(c, ctx) {
            out.skipped.push((c.path.clone(), *size, reason));
            continue;
        }
        match backend.trash(&c.path) {
            Ok(trashed) => out.moved.push(Moved {
                original: c.path.clone(),
                trashed,
                size: *size,
            }),
            Err(reason) => out.skipped.push((c.path.clone(), *size, reason)),
        }
    }
    out
}

/// True if `path` sits inside a Trash folder (user Trash or a volume's `.Trashes`).
pub fn is_in_trash(path: &Path) -> bool {
    path.is_absolute()
        && path.components().any(|c| {
            matches!(c, Component::Normal(s)
                if s.eq_ignore_ascii_case(".Trash") || s.eq_ignore_ascii_case(".Trashes"))
        })
}

#[derive(Debug, PartialEq, Eq)]
pub enum Restore {
    Restored(PathBuf),
    /// Restored under a "(restored)" name because the original path was taken.
    Renamed(PathBuf),
    Conflict,
    Missing,
    Failed(String),
}

/// `name (restored).ext`, then `name (restored 2).ext`, ... for the first free name.
fn free_restored_name(original: &Path) -> Option<PathBuf> {
    let parent = original.parent()?;
    let stem = original.file_stem()?.to_string_lossy().into_owned();
    let ext = original
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();
    (1..1000).find_map(|n| {
        let label = if n == 1 {
            "(restored)".to_string()
        } else {
            format!("(restored {n})")
        };
        let candidate = parent.join(format!("{stem} {label}{ext}"));
        std::fs::symlink_metadata(&candidate)
            .is_err()
            .then_some(candidate)
    })
}

/// Move an item back from the Trash. Never overwrites anything.
pub fn restore(trashed: &Path, original: &Path, allow_rename: bool) -> Restore {
    if !is_in_trash(trashed) {
        return Restore::Failed("The recorded Trash location is not inside a Trash.".into());
    }
    if protected_reason(original).is_some_and(|r| r != "Already in the Trash.") {
        return Restore::Failed("The original location is protected.".into());
    }
    if std::fs::symlink_metadata(trashed).is_err() {
        return Restore::Missing;
    }
    let mut target = original.to_path_buf();
    let mut renamed = false;
    if std::fs::symlink_metadata(&target).is_ok() {
        if !allow_rename {
            return Restore::Conflict;
        }
        match free_restored_name(original) {
            Some(t) => {
                target = t;
                renamed = true;
            }
            None => return Restore::Conflict,
        }
    }
    if let Some(parent) = target.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            return Restore::Failed(e.to_string());
        }
    }
    // Something may have appeared at the target since the check above; never replace it.
    match rename_no_replace(trashed, &target) {
        Ok(()) if renamed => Restore::Renamed(target),
        Ok(()) => Restore::Restored(target),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Restore::Conflict,
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => Restore::Failed(
            "macOS blocked access to the Trash. Grant Full Disk Access, then try again.".into(),
        ),
        Err(e) => Restore::Failed(e.to_string()),
    }
}

/// Empty the whole Trash through Finder, exactly like Finder > Empty Trash. This is the only
/// permanent deletion in the app; callers must have collected an explicit confirmation first.
pub fn empty_trash_via_finder() -> Result<(), AppError> {
    let out = Command::new("/usr/bin/osascript")
        .args(["-e", "tell application \"Finder\" to empty trash"])
        .stdin(Stdio::null())
        .output()?;
    if out.status.success() {
        Ok(())
    } else {
        let msg = String::from_utf8_lossy(&out.stderr);
        if msg.contains("-1743") {
            Err(AppError::PermissionDenied(
                "DuoBolt is not allowed to control Finder. Allow it in System Settings > Privacy & Security > Automation.".into(),
            ))
        } else {
            Err(AppError::Io(format!(
                "Finder could not empty the Trash: {}",
                msg.trim()
            )))
        }
    }
}

/// Size of the user's Trash, if macOS lets us read it (needs Full Disk Access).
pub fn user_trash_bytes() -> Option<u64> {
    let home = PathBuf::from(std::env::var_os("HOME")?);
    fn size(p: &Path) -> u64 {
        let Ok(m) = std::fs::symlink_metadata(p) else {
            return 0;
        };
        let own = m.st_blocks() * 512;
        if !m.is_dir() {
            return own;
        }
        std::fs::read_dir(p).map_or(own, |rd| {
            own + rd.flatten().map(|e| size(&e.path())).sum::<u64>()
        })
    }
    let trash = home.join(".Trash");
    std::fs::read_dir(&trash).ok()?;
    Some(size(&trash))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn p(s: &str) -> Option<&'static str> {
        protected_reason(Path::new(s))
    }

    #[test]
    fn system_paths_are_protected() {
        for s in [
            "/System",
            "/System/Library/x",
            "/usr/bin/ls",
            "/bin/sh",
            "/sbin/mount",
            "/private/var/db/receipts",
            "/private/var",
            "/Library/Apple/foo",
            "/",
            "/Users/me",
        ] {
            assert!(p(s).is_some(), "{s}");
        }
    }

    #[test]
    fn case_variants_and_traversal_are_caught() {
        assert!(p("/system/library").is_some());
        assert!(p("/SYSTEM").is_some());
        assert!(p("/sYsTeM/Library").is_some());
        assert!(p("/Users/me/../../System").is_some());
        assert!(p("/Users/me/./x").is_some());
        assert!(p("relative/path").is_some());
        assert!(p("/Users/me/.Trash/old.txt").is_some());
        assert!(p("/Volumes/Ext/.Trashes/501/x").is_some());
    }

    #[test]
    fn user_files_are_allowed() {
        for s in [
            "/Users/me/Downloads/a.dmg",
            "/usr/local/share/x",
            "/Applications/Foo.app",
            "/System-old/x",
            "/Volumes/Ext/stuff",
            "/private/tmp/x",
        ] {
            assert!(p(s).is_none(), "{s}");
        }
        assert!(p("/Volumes/Ext").is_some());
    }

    #[test]
    fn top_level_and_home_essentials_are_protected() {
        for s in [
            "/Applications",
            "/Library",
            "/opt",
            "/Library/Application Support",
            "/Library/Keychains/System.keychain",
            "/Users/me/Documents",
            "/Users/me/desktop",
            "/Users/me/Library",
            "/Users/me/Library/Caches",
            "/Users/me/Library/Mail",
            "/Users/me/Library/Keychains/login.keychain-db",
            "/Users/me/Library/Mobile Documents/com~apple~CloudDocs",
            "/Users/me/Library/CloudStorage/Dropbox",
            "/Users/me/.ssh",
            "/Users/me/.ssh/id_ed25519",
            "/Users/me/.gnupg/private-keys-v1.d/x.key",
        ] {
            assert!(p(s).is_some(), "{s}");
        }
        for s in [
            "/Users/me/Documents/report.pdf",
            "/Users/me/Library/Caches/com.example.app",
            "/Users/me/Library/Mobile Documents/com~apple~CloudDocs/old.zip",
            "/Users/me/Library/CloudStorage/Dropbox/big.mov",
            "/Library/Application Support/Vendor",
            "/Users/me/.npm/_cacache",
        ] {
            assert!(p(s).is_none(), "{s}");
        }
    }

    #[test]
    fn rename_no_replace_never_overwrites() {
        let e = env();
        let a = e.root.join("a.txt");
        let b = e.root.join("b.txt");
        file(&a, b"a");
        file(&b, b"b");
        let err = rename_no_replace(&a, &b).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::AlreadyExists);
        assert_eq!(std::fs::read(&b).unwrap(), b"b");
        assert!(a.exists());
        let c = e.root.join("c.txt");
        rename_no_replace(&a, &c).unwrap();
        assert_eq!(std::fs::read(&c).unwrap(), b"a");
    }

    /// A temp tree whose canonical path is used as the scan root.
    struct Env {
        _tmp: tempfile::TempDir,
        root: PathBuf,
        ctx: Context,
        trash: FakeTrash,
    }

    fn env() -> Env {
        let tmp = tempfile::tempdir().unwrap();
        let base = tmp.path().canonicalize().unwrap();
        let root = base.join("scan");
        std::fs::create_dir_all(&root).unwrap();
        let trash = FakeTrash::new(&base).unwrap();
        Env {
            _tmp: tmp,
            root: root.clone(),
            ctx: Context {
                roots: vec![root],
                app_bundle: None,
            },
            trash,
        }
    }

    fn file(path: &Path, data: &[u8]) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::File::create(path)
            .unwrap()
            .write_all(data)
            .unwrap();
    }

    fn candidate(path: &Path) -> (Candidate, u64) {
        let m = std::fs::symlink_metadata(path).unwrap();
        (
            Candidate {
                path: path.to_path_buf(),
                expected: Expected {
                    dev: m.st_dev() as u32,
                    inode: m.st_ino(),
                    size: m.st_size(),
                    mtime: m.st_mtime(),
                    is_dir: m.is_dir(),
                },
            },
            m.st_size(),
        )
    }

    #[test]
    fn thousand_files_move_and_restore_exactly() {
        let e = env();
        let mut cands = Vec::new();
        for i in 0..1000 {
            let f = e.root.join(format!("d{}/f{i}.txt", i % 10));
            file(&f, format!("content {i}").as_bytes());
            cands.push(candidate(&f));
        }
        let out = move_items(&cands, &e.ctx, &e.trash);
        assert_eq!(out.moved.len(), 1000, "{:?}", out.skipped.first());
        assert!(out.skipped.is_empty());
        assert!(!e.root.join("d0/f0.txt").exists());

        for m in &out.moved {
            assert_eq!(
                restore(&m.trashed, &m.original, false),
                Restore::Restored(m.original.clone())
            );
        }
        for i in 0..1000 {
            let f = e.root.join(format!("d{}/f{i}.txt", i % 10));
            assert_eq!(std::fs::read_to_string(&f).unwrap(), format!("content {i}"));
        }
    }

    #[test]
    fn changed_after_listing_is_skipped() {
        let e = env();
        let f = e.root.join("a.txt");
        file(&f, b"one");
        let c = candidate(&f);
        file(&f, b"different length now");
        let out = move_items(&[c], &e.ctx, &e.trash);
        assert!(out.moved.is_empty());
        assert!(out.skipped[0].2.contains("changed"));
        assert!(f.exists());
    }

    #[test]
    fn replaced_by_symlink_after_listing_is_skipped() {
        let e = env();
        let f = e.root.join("a.txt");
        file(&f, b"one");
        let c = candidate(&f);
        std::fs::remove_file(&f).unwrap();
        std::os::unix::fs::symlink("/System", &f).unwrap();
        let out = move_items(&[c], &e.ctx, &e.trash);
        assert!(out.moved.is_empty());
        assert!(std::fs::symlink_metadata(&f)
            .unwrap()
            .file_type()
            .is_symlink());
    }

    #[test]
    fn symlinked_parent_is_rejected() {
        let e = env();
        let real = e.root.join("real");
        file(&real.join("x.txt"), b"x");
        std::os::unix::fs::symlink(&real, e.root.join("link")).unwrap();
        // The scan never follows symlinks, but a crafted candidate through one must still fail.
        let mut c = candidate(&real.join("x.txt"));
        c.0.path = e.root.join("link/x.txt");
        let out = move_items(&[c], &e.ctx, &e.trash);
        assert!(out.moved.is_empty());
        assert!(out.skipped[0].2.contains("symbolic link"));
        assert!(real.join("x.txt").exists());
    }

    #[test]
    fn outside_roots_root_itself_and_app_bundle_are_rejected() {
        let e = env();
        let outside = e.root.parent().unwrap().join("outside.txt");
        file(&outside, b"x");
        let out = move_items(&[candidate(&outside)], &e.ctx, &e.trash);
        assert!(out.skipped[0].2.contains("Outside"));

        let out = move_items(&[candidate(&e.root)], &e.ctx, &e.trash);
        assert!(out.moved.is_empty(), "the scan root itself is never moved");

        let app = e.root.join("DuoBolt.app");
        file(&app.join("Contents/MacOS/duobolt"), b"bin");
        let ctx = Context {
            roots: e.ctx.roots.clone(),
            app_bundle: Some(app.clone()),
        };
        let out = move_items(&[candidate(&app)], &ctx, &e.trash);
        assert!(out.skipped[0].2.contains("itself"));
        assert!(app.exists());
    }

    #[test]
    fn nfd_spelling_of_missing_path_fails_safely() {
        let e = env();
        let f = e.root.join("caf\u{e9}.txt");
        file(&f, b"x");
        let mut c = candidate(&f);
        c.0.path = e.root.join("cafe\u{301}.txt");
        // APFS treats both spellings as the same file; either the move succeeds on that same
        // file or it is skipped. It must never touch anything else.
        let out = move_items(&[c], &e.ctx, &e.trash);
        assert_eq!(out.moved.len() + out.skipped.len(), 1);
    }

    #[test]
    fn conflict_then_suffix_restore() {
        let e = env();
        let f = e.root.join("report.pdf");
        file(&f, b"original");
        let out = move_items(&[candidate(&f)], &e.ctx, &e.trash);
        let m = &out.moved[0];
        file(&f, b"a new file with the same name");
        assert_eq!(restore(&m.trashed, &m.original, false), Restore::Conflict);
        assert!(
            m.trashed.exists(),
            "a conflict leaves the item in the Trash"
        );
        let r = restore(&m.trashed, &m.original, true);
        let renamed = e.root.join("report (restored).pdf");
        assert_eq!(r, Restore::Renamed(renamed.clone()));
        assert_eq!(std::fs::read(&renamed).unwrap(), b"original");
        assert_eq!(std::fs::read(&f).unwrap(), b"a new file with the same name");
    }

    #[test]
    fn restore_recreates_missing_parents_and_reports_missing() {
        let e = env();
        let f = e.root.join("deep/er/x.bin");
        file(&f, b"x");
        let out = move_items(&[candidate(&f)], &e.ctx, &e.trash);
        std::fs::remove_dir_all(e.root.join("deep")).unwrap();
        let m = &out.moved[0];
        assert_eq!(
            restore(&m.trashed, &m.original, false),
            Restore::Restored(f.clone())
        );
        assert!(f.exists());

        let out = move_items(&[candidate(&f)], &e.ctx, &e.trash);
        std::fs::remove_file(&out.moved[0].trashed).unwrap();
        assert_eq!(restore(&out.moved[0].trashed, &f, false), Restore::Missing);
    }

    #[test]
    fn restore_refuses_sources_outside_a_trash_and_protected_targets() {
        let e = env();
        let f = e.root.join("x.txt");
        file(&f, b"x");
        assert!(matches!(
            restore(&f, &e.root.join("y.txt"), false),
            Restore::Failed(_)
        ));
        let out = move_items(&[candidate(&f)], &e.ctx, &e.trash);
        let r = restore(&out.moved[0].trashed, Path::new("/System/x.txt"), false);
        assert!(matches!(r, Restore::Failed(_)));
    }

    #[test]
    fn batch_cap_is_enforced() {
        let e = env();
        let f = e.root.join("x.txt");
        file(&f, b"x");
        let one = candidate(&f);
        let many: Vec<_> = std::iter::repeat_n(one, MAX_BATCH + 3).collect();
        let out = move_items(&many, &e.ctx, &e.trash);
        assert_eq!(
            out.moved.len(),
            1,
            "the first moves, repeats then fail the existence check"
        );
        assert_eq!(out.moved.len() + out.skipped.len(), MAX_BATCH + 3);
        assert_eq!(
            out.skipped
                .iter()
                .filter(|s| s.2.contains("limited"))
                .count(),
            3
        );
    }

    #[test]
    fn trash_detection() {
        assert!(is_in_trash(Path::new("/Users/me/.Trash/x")));
        assert!(is_in_trash(Path::new("/Volumes/E/.Trashes/501/x")));
        assert!(!is_in_trash(Path::new("/Users/me/Trash/x")));
        assert!(!is_in_trash(Path::new(".Trash/x")));
    }

    mod props {
        use super::*;
        use proptest::prelude::*;

        proptest! {
            #[test]
            fn protected_prefixes_never_allowed(
                prefix in prop::sample::select(vec!["/System", "/system", "/SYSTEM", "/usr/bin", "/bin", "/sbin", "/private/var/db", "/Library/Apple"]),
                tail in "[a-zA-Z0-9 ._-]{0,12}(/[a-zA-Z0-9 ._-]{1,12}){0,3}",
            ) {
                let path = format!("{prefix}/{tail}");
                prop_assert!(protected_reason(Path::new(&path)).is_some(), "{}", path);
            }

            #[test]
            fn dotdot_never_allowed(a in "[a-z]{1,8}", b in "[a-z]{1,8}") {
                let path = format!("/Users/{a}/../{b}");
                prop_assert!(protected_reason(Path::new(&path)).is_some());
            }

            #[test]
            fn every_request_is_moved_or_skipped(n in 0usize..12, change in proptest::collection::vec(any::<bool>(), 12)) {
                let e = env();
                let mut cands = Vec::new();
                for (i, changed) in change.iter().enumerate().take(n) {
                    let f = e.root.join(format!("f{i}"));
                    file(&f, b"abc");
                    let c = candidate(&f);
                    if *changed {
                        file(&f, b"changed!");
                    }
                    cands.push(c);
                }
                let out = move_items(&cands, &e.ctx, &e.trash);
                prop_assert_eq!(out.moved.len() + out.skipped.len(), n);
                let changed = change.iter().take(n).filter(|c| **c).count();
                prop_assert_eq!(out.skipped.len(), changed);
            }
        }
    }

    /// Uses the real Trash. Run by hand: `cargo test -p duobolt-core real_trash -- --ignored`.
    #[test]
    #[ignore]
    fn real_trash_move_and_undo() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().canonicalize().unwrap();
        let ctx = Context {
            roots: vec![root.clone()],
            app_bundle: None,
        };
        let mut cands = Vec::new();
        for i in 0..3 {
            let f = root.join(format!("duobolt-test-{i}.txt"));
            file(&f, b"hello");
            cands.push(candidate(&f));
        }
        let out = move_items(&cands, &ctx, &MacTrash);
        assert_eq!(out.moved.len(), 3, "{:?}", out.skipped);
        for m in &out.moved {
            assert!(is_in_trash(&m.trashed), "{}", m.trashed.display());
            assert!(matches!(
                restore(&m.trashed, &m.original, false),
                Restore::Restored(_)
            ));
        }
    }
}
