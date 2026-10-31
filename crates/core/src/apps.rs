//! Installed applications, footprints, leftovers and uninstall plans (M6).
//!
//! Apps are read from their `Info.plist`. Related files are entries directly inside the usual
//! Library folders, and an entry belongs to an app only when it is named after the app's bundle id
//! (exactly, or the bundle id followed by a dot), or exactly after its display name when that name
//! is long and specific. Every entry gets at most one owner: the app with the longest matching
//! bundle id. Entries named after a developer rather than one app (group containers shared by
//! several apps) are marked shared and never suggested.

use std::collections::{HashMap, HashSet};
use std::ffi::OsStr;
use std::io::Read;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use crate::cleanup::Claims;
use crate::index::{flags, NodeId, NodeKind, Tree};
use crate::models::{AppArch, AppSource, AppSummary, Confidence, RelatedFile, RelatedKind};
use crate::trash::{self, Candidate, Expected};

/// Library folders (relative to home) searched for an app's files and for leftovers.
pub const HOME_LOCATIONS: &[(&str, RelatedKind)] = &[
    ("Library/Application Support", RelatedKind::Support),
    ("Library/Caches", RelatedKind::Caches),
    ("Library/Preferences", RelatedKind::Preferences),
    ("Library/Containers", RelatedKind::Containers),
    ("Library/Group Containers", RelatedKind::GroupContainers),
    ("Library/Saved Application State", RelatedKind::SavedState),
    ("Library/Logs", RelatedKind::Logs),
    ("Library/LaunchAgents", RelatedKind::LaunchAgents),
    ("Library/HTTPStorages", RelatedKind::HttpStorage),
    ("Library/WebKit", RelatedKind::WebKit),
];
pub const SYSTEM_SUPPORT: &str = "/Library/Application Support";

/// Display names too common to identify one app.
const GENERIC_NAMES: &[&str] = &[
    "Adobe",
    "Apple",
    "Application",
    "Applications",
    "Backup",
    "Backups",
    "Cache",
    "Caches",
    "Chrome",
    "Code",
    "Data",
    "Default",
    "Documents",
    "Google",
    "Helper",
    "Library",
    "Logs",
    "Microsoft",
    "Mozilla",
    "Preferences",
    "Settings",
    "Support",
    "Updater",
    "Utilities",
];

#[derive(Debug, Clone)]
pub struct App {
    pub name: String,
    pub bundle_id: String,
    pub version: String,
    pub path: PathBuf,
    pub executable: Option<PathBuf>,
    pub icon: Option<PathBuf>,
    pub source: AppSource,
    pub arch: AppArch,
    /// Bundle ids of helpers, login items and extensions inside the app.
    pub embedded: Vec<String>,
}

/// Folders apps are installed in.
pub fn app_dirs(home: &Path) -> Vec<PathBuf> {
    vec![
        PathBuf::from("/Applications"),
        home.join("Applications"),
        PathBuf::from("/System/Applications"),
    ]
}

fn plist_dict(path: &Path) -> Option<plist::Dictionary> {
    plist::Value::from_file(path).ok()?.into_dictionary()
}

fn string(d: &plist::Dictionary, key: &str) -> Option<String> {
    d.get(key)?.as_string().map(str::to_string)
}

/// Read one `.app` bundle. `None` when it has no readable Info.plist or bundle id.
pub fn read_app(path: &Path) -> Option<App> {
    let contents = path.join("Contents");
    let d = plist_dict(&contents.join("Info.plist"))?;
    let bundle_id = string(&d, "CFBundleIdentifier")?;
    let stem = path.file_stem()?.to_string_lossy().into_owned();
    let name = string(&d, "CFBundleDisplayName")
        .or_else(|| string(&d, "CFBundleName"))
        .filter(|n| !n.is_empty())
        .unwrap_or(stem);
    let executable = string(&d, "CFBundleExecutable").map(|e| contents.join("MacOS").join(e));
    let icon = string(&d, "CFBundleIconFile").map(|i| {
        let file = if Path::new(&i).extension().is_some() {
            i
        } else {
            format!("{i}.icns")
        };
        contents.join("Resources").join(file)
    });
    let source = if path.starts_with("/System") {
        AppSource::System
    } else if contents.join("_MASReceipt/receipt").exists() {
        AppSource::AppStore
    } else {
        AppSource::Other
    };
    let arch = executable.as_deref().map_or(AppArch::Unknown, macho_arch);
    Some(App {
        name,
        bundle_id,
        version: string(&d, "CFBundleShortVersionString").unwrap_or_default(),
        path: path.to_path_buf(),
        executable,
        icon,
        source,
        arch,
        embedded: embedded_ids(&contents),
    })
}

/// Bundle ids of bundles inside an app (one level deep in the usual places).
fn embedded_ids(contents: &Path) -> Vec<String> {
    let mut ids = Vec::new();
    for sub in [
        "Library/LoginItems",
        "Library/LaunchServices",
        "Helpers",
        "XPCServices",
        "PlugIns",
        "Frameworks",
    ] {
        let Ok(rd) = std::fs::read_dir(contents.join(sub)) else {
            continue;
        };
        for e in rd.flatten() {
            let p = e.path();
            let bundle = p
                .extension()
                .is_some_and(|x| x == "app" || x == "xpc" || x == "appex");
            if !bundle || e.file_type().is_ok_and(|t| t.is_symlink()) {
                continue;
            }
            if let Some(id) = plist_dict(&p.join("Contents/Info.plist"))
                .and_then(|d| string(&d, "CFBundleIdentifier"))
            {
                ids.push(id);
            }
        }
    }
    ids
}

/// Every `.app` in `dirs`, plus apps one folder down (like /Applications/Utilities).
/// Symlinks are skipped, never followed.
pub fn discover(dirs: &[PathBuf]) -> Vec<App> {
    let mut seen = HashSet::new();
    let mut apps = Vec::new();
    let mut visit = |p: PathBuf, apps: &mut Vec<App>| {
        if seen.insert(p.clone()) {
            if let Some(a) = read_app(&p) {
                apps.push(a);
            }
        }
    };
    for dir in dirs {
        let Ok(rd) = std::fs::read_dir(dir) else {
            continue;
        };
        for e in rd.flatten() {
            let p = e.path();
            let Ok(t) = e.file_type() else { continue };
            if t.is_symlink() || !t.is_dir() {
                continue;
            }
            if p.extension().is_some_and(|x| x == "app") {
                visit(p, &mut apps);
            } else if !e.file_name().as_bytes().starts_with(b".") {
                if let Ok(inner) = std::fs::read_dir(&p) {
                    for f in inner.flatten() {
                        let q = f.path();
                        let link = f.file_type().map_or(true, |t| t.is_symlink());
                        if !link && q.extension().is_some_and(|x| x == "app") {
                            visit(q, &mut apps);
                        }
                    }
                }
            }
        }
    }
    apps.sort_by_key(|a| a.name.to_lowercase());
    apps
}

/// Lower-cased bundle ids of installed apps, their helpers, and running apps.
pub fn installed_ids(apps: &[App], running: &[crate::cleanup::RunningApp]) -> HashSet<String> {
    apps.iter()
        .flat_map(|a| std::iter::once(&a.bundle_id).chain(a.embedded.iter()))
        .chain(running.iter().map(|r| &r.bundle_id))
        .map(|s| s.to_lowercase())
        .collect()
}

/// Architecture from the executable's Mach-O header.
pub fn macho_arch(exe: &Path) -> AppArch {
    const ARM64: u32 = 0x0100_000c;
    const X86_64: u32 = 0x0100_0007;
    let mut head = [0u8; 4096];
    let Ok(n) = std::fs::File::open(exe).and_then(|mut f| f.read(&mut head)) else {
        return AppArch::Unknown;
    };
    let head = &head[..n];
    let u32_at = |off: usize, be: bool| -> Option<u32> {
        let b: [u8; 4] = head.get(off..off + 4)?.try_into().ok()?;
        Some(if be {
            u32::from_be_bytes(b)
        } else {
            u32::from_le_bytes(b)
        })
    };
    let classify = |cpus: &[u32]| {
        let arm = cpus.contains(&ARM64);
        let intel = cpus.contains(&X86_64);
        match (arm, intel) {
            (true, true) => AppArch::Universal,
            (true, false) => AppArch::AppleSilicon,
            (false, true) => AppArch::Intel,
            _ => AppArch::Unknown,
        }
    };
    match u32_at(0, true) {
        // Universal binary: big-endian header, then (cputype, ...) entries of 20 or 32 bytes.
        Some(magic @ (0xcafe_babe | 0xcafe_babf)) => {
            let Some(count) = u32_at(4, true) else {
                return AppArch::Unknown;
            };
            // Java class files share the magic; real fat headers list only a few slices.
            if count == 0 || count > 8 {
                return AppArch::Unknown;
            }
            let stride = if magic == 0xcafe_babf { 32 } else { 20 };
            let cpus: Vec<u32> = (0..count as usize)
                .filter_map(|i| u32_at(8 + i * stride, true))
                .collect();
            classify(&cpus)
        }
        // Thin 64-bit Mach-O, little-endian on disk.
        Some(0xcffa_edfe) => u32_at(4, false).map_or(AppArch::Unknown, |c| classify(&[c])),
        _ => AppArch::Unknown,
    }
}

/// Why an app may not be removed, or `None`.
pub fn locked_reason(app: &App) -> Option<String> {
    if app.source == AppSource::System || app.path.starts_with("/System") {
        return Some("Part of macOS. It cannot be removed.".into());
    }
    let id = app.bundle_id.to_lowercase();
    if id.starts_with("com.apple.") && app.source != AppSource::AppStore {
        return Some("Part of macOS. It cannot be removed.".into());
    }
    let own = trash::own_app_bundle();
    if id == "com.mymacstorage.app" || own.as_deref() == Some(app.path.as_path()) {
        return Some("MyMacStorage cannot remove itself.".into());
    }
    None
}

// ---------------------------------------------------------------------------------------------
// Ownership of Library entries
// ---------------------------------------------------------------------------------------------

/// The bundle id an entry is named after: suffixes like `.plist` and team-id or `group.`
/// prefixes removed.
pub fn entry_id(name: &str, kind: RelatedKind) -> String {
    let mut s = name;
    for suffix in [".plist", ".savedState", ".binarycookies"] {
        if let Some(t) = s.strip_suffix(suffix) {
            s = t;
            break;
        }
    }
    if kind == RelatedKind::GroupContainers {
        if let Some((team, rest)) = s.split_once('.') {
            let team_like = team.len() == 10
                && team
                    .chars()
                    .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit());
            if team_like {
                s = rest;
            }
        }
    }
    // App groups are `group.<id>`; some Apple ones (Podcasts) use `groups.<id>`.
    s.strip_prefix("group.")
        .or_else(|| s.strip_prefix("groups."))
        .unwrap_or(s)
        .to_string()
}

/// `com.vendor.app`-like: at least three dot-separated parts of letters, digits, `-` and `_`.
pub fn looks_like_bundle_id(s: &str) -> bool {
    let parts: Vec<&str> = s.split('.').collect();
    parts.len() >= 3
        && parts.iter().all(|p| {
            !p.is_empty()
                && p.chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        })
}

fn vendor(id: &str) -> String {
    id.split('.').take(2).collect::<Vec<_>>().join(".")
}

/// Who an entry belongs to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Owner {
    One(usize, Confidence),
    Shared(Vec<usize>),
}

/// Lookup tables over all installed apps, built once.
pub struct Owners<'a> {
    apps: &'a [App],
    /// Bundle id (lower case) → app; helpers point at their parent app.
    by_id: HashMap<String, usize>,
    /// Every proper prefix (2+ parts) of every bundle id → apps having it.
    by_prefix: HashMap<String, Vec<usize>>,
    /// Display name → app, for names only one app has.
    by_name: HashMap<String, usize>,
}

impl<'a> Owners<'a> {
    pub fn new(apps: &'a [App]) -> Self {
        let mut by_id = HashMap::new();
        let mut by_prefix: HashMap<String, Vec<usize>> = HashMap::new();
        let mut names: HashMap<String, Vec<usize>> = HashMap::new();
        for (i, a) in apps.iter().enumerate() {
            let main = a.bundle_id.to_lowercase();
            by_id.insert(main.clone(), i);
            for e in &a.embedded {
                by_id.entry(e.to_lowercase()).or_insert(i);
            }
            let parts: Vec<&str> = main.split('.').collect();
            for n in 2..parts.len() {
                let v = by_prefix.entry(parts[..n].join(".")).or_default();
                if !v.contains(&i) {
                    v.push(i);
                }
            }
            names.entry(a.name.clone()).or_default().push(i);
        }
        let by_name = names
            .into_iter()
            .filter(|(n, v)| {
                v.len() == 1 && n.chars().count() >= 4 && !GENERIC_NAMES.contains(&n.as_str())
            })
            .map(|(n, v)| (n, v[0]))
            .collect();
        Self {
            apps,
            by_id,
            by_prefix,
            by_name,
        }
    }

    pub fn owner(&self, raw_name: &str, kind: RelatedKind) -> Option<Owner> {
        let id = entry_id(raw_name, kind).to_lowercase();
        // Exact bundle id, then the longest bundle id the name starts with (plus a dot).
        if let Some(&i) = self.by_id.get(&id) {
            let main = self.apps.get(i)?.bundle_id.to_lowercase();
            let conf = if main == id {
                Confidence::Exact
            } else {
                Confidence::Prefix
            };
            return Some(Owner::One(i, conf));
        }
        let parts: Vec<&str> = id.split('.').collect();
        for n in (3..parts.len()).rev() {
            if let Some(&i) = self.by_id.get(&parts[..n].join(".")) {
                return Some(Owner::One(i, Confidence::Prefix));
            }
        }
        // Named after a developer: shared by the apps under that prefix. Apple's own shared
        // containers belong to macOS, not to any one app, so they are never listed.
        if parts.len() >= 2 && !id.starts_with("com.apple.") {
            let shared = self.by_prefix.get(&id).or_else(|| {
                (kind == RelatedKind::GroupContainers)
                    .then(|| self.by_prefix.get(&vendor(&id)))
                    .flatten()
            });
            if let Some(v) = shared {
                return Some(Owner::Shared(v.clone()));
            }
        }
        // Display name, only where apps keep data under their name.
        if matches!(
            kind,
            RelatedKind::Support | RelatedKind::Caches | RelatedKind::Logs
        ) {
            if let Some(&i) = self.by_name.get(raw_name) {
                return Some(Owner::One(i, Confidence::Name));
            }
        }
        None
    }
}

/// A Library entry related to an app.
#[derive(Debug, Clone)]
pub struct Related {
    pub node: NodeId,
    pub path: PathBuf,
    pub kind: RelatedKind,
    pub confidence: Confidence,
    pub bytes: u64,
}

impl Related {
    /// Exact or prefix matches in the user's Library are suggested; the rest are for review.
    /// Containers hold a sandboxed app's own documents, so they are never pre-selected.
    pub fn suggested(&self) -> bool {
        matches!(self.confidence, Confidence::Exact | Confidence::Prefix)
            && !matches!(
                self.kind,
                RelatedKind::SystemSupport | RelatedKind::Containers | RelatedKind::GroupContainers
            )
    }

    pub fn to_model(&self) -> RelatedFile {
        RelatedFile {
            node_id: self.node,
            name: self
                .path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            path: self.path.to_string_lossy().into_owned(),
            kind: self.kind,
            confidence: self.confidence,
            bytes: self.bytes,
            suggested: self.suggested(),
        }
    }
}

fn locations(home: &Path) -> Vec<(PathBuf, RelatedKind)> {
    HOME_LOCATIONS
        .iter()
        .map(|(rel, k)| (home.join(rel), *k))
        .chain(std::iter::once((
            PathBuf::from(SYSTEM_SUPPORT),
            RelatedKind::SystemSupport,
        )))
        .collect()
}

/// Folders an app scan must cover: the app folders and the Library locations.
pub fn scan_roots(home: &Path) -> Vec<PathBuf> {
    app_dirs(home)
        .into_iter()
        .chain(locations(home).into_iter().map(|(p, _)| p))
        .filter(|p| p.is_dir())
        .collect()
}

fn child_name(tree: &Tree, id: NodeId) -> String {
    OsStr::from_bytes(tree.name(id))
        .to_string_lossy()
        .into_owned()
}

fn usable(n: &crate::index::Node) -> bool {
    n.kind != NodeKind::Symlink && !n.has(flags::DELETED)
}

/// Related files of every app, by app index. Computed in one pass over the locations.
pub fn related_all(tree: &Tree, home: &Path, apps: &[App]) -> HashMap<usize, Vec<Related>> {
    let owners = Owners::new(apps);
    let mut out: HashMap<usize, Vec<Related>> = HashMap::new();
    for (loc, kind) in locations(home) {
        let Some(root) = tree.find_path(&loc) else {
            continue;
        };
        for c in tree.children(root) {
            let Some(n) = tree.node(c) else { continue };
            if !usable(n) {
                continue;
            }
            let name = child_name(tree, c);
            let rel = |confidence| Related {
                node: c,
                path: loc.join(&name),
                kind,
                confidence,
                bytes: n.total_alloc,
            };
            match owners.owner(&name, kind) {
                Some(Owner::One(i, conf)) => out.entry(i).or_default().push(rel(conf)),
                Some(Owner::Shared(v)) => {
                    for i in v {
                        out.entry(i).or_default().push(rel(Confidence::Shared));
                    }
                }
                None => {}
            }
        }
    }
    for v in out.values_mut() {
        v.sort_by_key(|r| std::cmp::Reverse(r.bytes));
    }
    out
}

/// One Library entry that looks left behind by a removed app.
#[derive(Debug, Clone)]
pub struct Leftover {
    pub node: NodeId,
    pub path: PathBuf,
    /// The Library folder it sits in; it must stay inside.
    pub root: PathBuf,
    pub bundle_id: String,
    pub kind: RelatedKind,
}

impl Leftover {
    /// A sandboxed app's container is where it keeps all of its user's data (notes, journals,
    /// libraries), so it is only ever removed one at a time, with confirmation.
    pub fn holds_user_data(&self) -> bool {
        matches!(
            self.kind,
            RelatedKind::Containers | RelatedKind::GroupContainers
        )
    }
}

/// True if a launch agent plist still points at a program that exists, or cannot be read (then
/// nothing is known, so it is kept). Agents a user wrote for their own scripts look like an
/// app's, but their program is still there.
fn launch_agent_alive(plist_path: &Path) -> bool {
    let Some(d) = plist_dict(plist_path) else {
        return true;
    };
    let program = string(&d, "Program").or_else(|| {
        d.get("ProgramArguments")?
            .as_array()?
            .first()?
            .as_string()
            .map(str::to_string)
    });
    match program {
        // A bare command name is found through PATH; it cannot be ruled out.
        Some(p) if p.starts_with('/') => std::fs::symlink_metadata(&p).is_ok(),
        _ => true,
    }
}

/// Library entries named after an app that is not installed. Apple's own entries, anything a
/// cleanup rule already claimed, anything sharing a developer prefix with an installed app, and
/// launch agents whose program still exists are left out.
pub fn leftover_nodes(
    tree: &Tree,
    home: &Path,
    installed: &HashSet<String>,
    claims: &Claims,
) -> Vec<Leftover> {
    let vendors: HashSet<String> = installed.iter().map(|i| vendor(i)).collect();
    let mut out = Vec::new();
    for (rel, kind) in HOME_LOCATIONS {
        let loc = home.join(rel);
        let Some(root) = tree.find_path(&loc) else {
            continue;
        };
        for c in tree.children(root) {
            let Some(n) = tree.node(c) else { continue };
            if !usable(n)
                || n.has(flags::UNREADABLE)
                || n.total_alloc == 0
                || claims.is_claimed_or_inside(tree, c)
                || claims.contains_claimed(c)
            {
                continue;
            }
            let name = child_name(tree, c);
            let id = entry_id(&name, *kind).to_lowercase();
            if !looks_like_bundle_id(&id)
                || id.starts_with("com.apple.")
                || id.starts_with("systemgroup.")
                || id.starts_with("homebrew.")
                || vendors.contains(&vendor(&id))
                || installed.iter().any(|i| {
                    *i == id || id.starts_with(&format!("{i}.")) || i.starts_with(&format!("{id}."))
                })
            {
                continue;
            }
            let path = loc.join(&name);
            if *kind == RelatedKind::LaunchAgents && launch_agent_alive(&path) {
                continue;
            }
            out.push(Leftover {
                node: c,
                path,
                root: loc.clone(),
                bundle_id: id,
                kind: *kind,
            });
        }
    }
    out
}

/// Summary row for the list.
pub fn summarize(
    idx: usize,
    app: &App,
    tree: &Tree,
    related: &[Related],
    last_used: Option<i64>,
    running: bool,
) -> AppSummary {
    let app_bytes = tree
        .find_path(&app.path)
        .and_then(|id| tree.node(id))
        .map_or(0, |n| n.total_alloc);
    let own = related
        .iter()
        .filter(|r| r.confidence != Confidence::Shared);
    let locked = locked_reason(app);
    AppSummary {
        id: idx as u32,
        name: app.name.clone(),
        bundle_id: app.bundle_id.clone(),
        version: app.version.clone(),
        path: app.path.to_string_lossy().into_owned(),
        source: app.source,
        arch: app.arch,
        app_bytes,
        data_bytes: own.clone().map(|r| r.bytes).sum(),
        cache_bytes: own
            .filter(|r| r.kind == RelatedKind::Caches)
            .map(|r| r.bytes)
            .sum(),
        last_used,
        removable: locked.is_none(),
        locked_reason: locked,
        running,
    }
}

/// Installer receipts for an app (names only; they are never touched).
pub fn receipts(bundle_id: &str) -> Vec<String> {
    let id = bundle_id.to_lowercase();
    let Ok(rd) = std::fs::read_dir("/private/var/db/receipts") else {
        return Vec::new();
    };
    let mut v: Vec<String> = rd
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.to_lowercase().starts_with(&id) && n.ends_with(".plist"))
        .collect();
    v.sort();
    v
}

fn candidate(tree: &Tree, id: NodeId, path: PathBuf) -> Option<(Candidate, u64)> {
    let n = tree.node(id)?;
    Some((
        Candidate {
            path,
            expected: Expected {
                dev: n.dev,
                inode: n.inode,
                size: n.size_logical,
                mtime: n.mtime,
                is_dir: n.is_dir(),
            },
        },
        n.total_alloc,
    ))
}

/// What to move for an uninstall: the app itself plus the chosen related files, which must all
/// be among the app's related files. Refuses apps that may not be removed.
pub fn plan_uninstall(
    tree: &Tree,
    app: &App,
    related: &[Related],
    chosen: &[NodeId],
) -> Result<Vec<(Candidate, u64)>, String> {
    if let Some(why) = locked_reason(app) {
        return Err(why);
    }
    let app_node = tree
        .find_path(&app.path)
        .ok_or("The app was not found in the last applications scan. Scan again.")?;
    let mut items = vec![candidate(tree, app_node, app.path.clone())
        .ok_or("The app was not found in the last applications scan.")?];
    let mut seen = HashSet::new();
    for id in chosen {
        if !seen.insert(*id) {
            continue;
        }
        let r = related
            .iter()
            .find(|r| r.node == *id)
            .ok_or("A selected file does not belong to this app.")?;
        if r.confidence == Confidence::Shared {
            return Err(format!(
                "{} is shared with other apps from the same developer, so it is kept.",
                r.path.display()
            ));
        }
        if let Some(c) = candidate(tree, r.node, r.path.clone()) {
            items.push(c);
        }
    }
    Ok(items)
}

/// Carry out a plan from [`plan_uninstall`]: the app first, then its files. When the app itself
/// cannot move (it was updated since the scan, or is protected), its files stay too: removing
/// the data of an app that is still installed would lose it for nothing.
pub fn uninstall(
    plan: &[(Candidate, u64)],
    ctx: &trash::Context,
    backend: &dyn trash::TrashBackend,
) -> trash::MoveOutcome {
    let Some((app, files)) = plan.split_first() else {
        return trash::MoveOutcome::default();
    };
    let mut out = trash::move_items(std::slice::from_ref(app), ctx, backend);
    if out.moved.is_empty() {
        out.skipped.extend(files.iter().map(|(c, size)| {
            (
                c.path.clone(),
                *size,
                "Kept because the app itself was not removed.".to_string(),
            )
        }));
        return out;
    }
    let rest = trash::move_items(files, ctx, backend);
    out.moved.extend(rest.moved);
    out.skipped.extend(rest.skipped);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::{NewNode, ROOT};

    fn app(name: &str, id: &str) -> App {
        App {
            name: name.into(),
            bundle_id: id.into(),
            version: "1.0".into(),
            path: PathBuf::from(format!("/Applications/{name}.app")),
            executable: None,
            icon: None,
            source: AppSource::Other,
            arch: AppArch::AppleSilicon,
            embedded: Vec::new(),
        }
    }

    fn add(t: &mut Tree, parent: NodeId, name: &str, dir: bool, size: u64) -> NodeId {
        t.add(
            parent,
            NewNode {
                name: name.as_bytes(),
                kind: if dir { NodeKind::Dir } else { NodeKind::File },
                flags: 0,
                dev: 1,
                inode: 0,
                mtime: 0,
                atime: 0,
                size_logical: size,
                size_alloc: size,
            },
        )
        .unwrap()
    }

    /// Ten apps with deliberately similar ids and names.
    fn ten() -> Vec<App> {
        let mut slack = app("Slack", "com.tinyspeck.slackmacgap");
        slack.embedded = vec!["com.tinyspeck.slackmacgap.helper".into()];
        vec![
            app("Foo", "com.foo.bar"),
            app("Foo Pro", "com.foo.bar.pro"),
            app("Barista", "com.foo.barista"),
            app("Google Chrome", "com.google.Chrome"),
            app("Chrome Canary", "com.google.Chrome.canary"),
            app("Firefox", "org.mozilla.firefox"),
            app("Microsoft Word", "com.microsoft.Word"),
            app("Microsoft Excel", "com.microsoft.Excel"),
            slack,
            app("Code", "com.microsoft.VSCode"),
        ]
    }

    fn library(entries: &[(&str, &[&str])]) -> Tree {
        let mut t = Tree::new();
        let h = add(&mut t, ROOT, "/Users/me", true, 0);
        let lib = add(&mut t, h, "Library", true, 0);
        for (loc, names) in entries {
            let rel = loc.strip_prefix("Library/").unwrap_or(loc);
            let d = add(&mut t, lib, rel, true, 0);
            for n in *names {
                add(&mut t, d, n, false, 100);
            }
        }
        t.aggregate();
        t
    }

    fn sample_tree() -> Tree {
        library(&[
            (
                "Library/Application Support",
                &[
                    "com.foo.bar",
                    "com.foo.bar.pro",
                    "com.foo.barista",
                    "Slack",
                    "Code",
                    "Google",
                    "Firefox",
                    "com.google.Chrome.canary",
                ],
            ),
            (
                "Library/Preferences",
                &[
                    "com.foo.bar.plist",
                    "com.foo.bar.pro.plist",
                    "com.google.Chrome.plist",
                    "com.google.Chrome.canary.plist",
                    "com.microsoft.Word.plist",
                    "com.microsoft.Excel.plist",
                    "com.tinyspeck.slackmacgap.helper.plist",
                    "com.gone.app.plist",
                ],
            ),
            (
                "Library/Group Containers",
                &[
                    "UBF8T346G9.com.microsoft",
                    "UBF8T346G9.Office",
                    "group.com.foo",
                    "group.com.apple.people",
                ],
            ),
            (
                "Library/Caches",
                &["com.microsoft.VSCode.ShipIt", "org.mozilla.firefox"],
            ),
        ])
    }

    #[test]
    fn no_file_is_matched_to_the_wrong_app() {
        let apps = ten();
        let t = sample_tree();
        let rel = related_all(&t, Path::new("/Users/me"), &apps);
        let names = |i: usize| -> Vec<String> {
            rel.get(&i)
                .map(|v| {
                    v.iter()
                        .filter(|r| r.confidence != Confidence::Shared)
                        .map(|r| r.path.file_name().unwrap().to_string_lossy().into_owned())
                        .collect()
                })
                .unwrap_or_default()
        };
        let sorted = |mut v: Vec<String>| {
            v.sort();
            v
        };
        assert_eq!(sorted(names(0)), ["com.foo.bar", "com.foo.bar.plist"]);
        assert_eq!(
            sorted(names(1)),
            ["com.foo.bar.pro", "com.foo.bar.pro.plist"]
        );
        assert_eq!(names(2), ["com.foo.barista"]);
        assert_eq!(names(3), ["com.google.Chrome.plist"]);
        assert_eq!(
            sorted(names(4)),
            ["com.google.Chrome.canary", "com.google.Chrome.canary.plist"]
        );
        assert_eq!(sorted(names(5)), ["Firefox", "org.mozilla.firefox"]);
        assert_eq!(names(6), ["com.microsoft.Word.plist"]);
        assert_eq!(names(7), ["com.microsoft.Excel.plist"]);
        assert_eq!(
            sorted(names(8)),
            ["Slack", "com.tinyspeck.slackmacgap.helper.plist"]
        );
        // "Code" is too generic to match on its name.
        assert_eq!(names(9), ["com.microsoft.VSCode.ShipIt"]);

        // Every entry has at most one non-shared owner.
        let mut owners: HashMap<NodeId, usize> = HashMap::new();
        for v in rel.values() {
            for r in v.iter().filter(|r| r.confidence != Confidence::Shared) {
                *owners.entry(r.node).or_default() += 1;
            }
        }
        assert!(owners.values().all(|n| *n == 1));
    }

    #[test]
    fn developer_containers_are_shared_and_not_suggested() {
        let apps = ten();
        let t = sample_tree();
        let rel = related_all(&t, Path::new("/Users/me"), &apps);
        for i in [6, 7, 9] {
            let ms = rel[&i]
                .iter()
                .find(|r| r.path.ends_with("UBF8T346G9.com.microsoft"))
                .unwrap();
            assert_eq!(ms.confidence, Confidence::Shared);
            assert!(!ms.suggested());
        }
        let foo_group = rel[&0]
            .iter()
            .find(|r| r.path.ends_with("group.com.foo"))
            .unwrap();
        assert_eq!(foo_group.confidence, Confidence::Shared);
        assert!(!rel
            .values()
            .flatten()
            .any(|r| r.path.ends_with("UBF8T346G9.Office")
                || r.path.ends_with("group.com.apple.people")));
    }

    #[test]
    fn leftovers_skip_installed_vendors_and_apple() {
        let apps = ten();
        let installed = installed_ids(&apps, &[]);
        let t = library(&[
            (
                "Library/Application Support",
                &[
                    "com.gone.editor",
                    "com.foo.oldtool",
                    "com.apple.TV",
                    "Random Folder",
                ],
            ),
            (
                "Library/Preferences",
                &[
                    "com.gone.editor.plist",
                    "com.google.Keystone.plist",
                    "group.com.apple.mail.plist",
                ],
            ),
            ("Library/LaunchAgents", &["homebrew.mxcl.postgresql.plist"]),
        ]);
        let found = leftover_nodes(&t, Path::new("/Users/me"), &installed, &Claims::default());
        let mut ids: Vec<&str> = found.iter().map(|f| f.bundle_id.as_str()).collect();
        ids.sort();
        assert_eq!(ids, ["com.gone.editor", "com.gone.editor"]);
        for f in &found {
            assert!(crate::cleanup::within_root(&f.path, &f.root));
            assert!(!f.holds_user_data());
        }
    }

    #[test]
    fn containers_of_removed_apps_hold_user_data() {
        let t = library(&[("Library/Containers", &["com.gone.journal"])]);
        let found = leftover_nodes(
            &t,
            Path::new("/Users/me"),
            &HashSet::new(),
            &Claims::default(),
        );
        assert_eq!(found.len(), 1);
        assert!(found[0].holds_user_data());
    }

    #[test]
    fn launch_agents_with_a_live_program_are_kept() {
        let tmp = tempfile::tempdir().unwrap();
        let write = |name: &str, program: &str| {
            let p = tmp.path().join(name);
            let mut d = plist::Dictionary::new();
            d.insert("Label".into(), name.into());
            d.insert(
                "ProgramArguments".into(),
                plist::Value::Array(vec![program.into(), "--run".into()]),
            );
            plist::Value::Dictionary(d).to_file_xml(&p).unwrap();
            p
        };
        assert!(launch_agent_alive(&write("a.plist", "/bin/sh")));
        assert!(!launch_agent_alive(&write(
            "b.plist",
            "/nonexistent/gone-helper"
        )));
        assert!(
            launch_agent_alive(&write("c.plist", "node")),
            "PATH lookups are unknown"
        );
        assert!(launch_agent_alive(&tmp.path().join("missing.plist")));
    }

    #[test]
    fn apple_containers_are_not_shared_with_apple_apps() {
        let apps = [app("Keynote", "com.apple.iWork.Keynote")];
        let o = Owners::new(&apps);
        assert_eq!(
            o.owner("group.com.apple.people", RelatedKind::GroupContainers),
            None
        );
        assert_eq!(
            o.owner("com.apple.iWork.Keynote", RelatedKind::Containers),
            Some(Owner::One(0, Confidence::Exact))
        );
    }

    #[test]
    fn entry_ids() {
        assert_eq!(
            entry_id("com.x.y.plist", RelatedKind::Preferences),
            "com.x.y"
        );
        assert_eq!(
            entry_id("com.x.y.savedState", RelatedKind::SavedState),
            "com.x.y"
        );
        assert_eq!(
            entry_id("UBF8T346G9.com.x.y", RelatedKind::GroupContainers),
            "com.x.y"
        );
        assert_eq!(
            entry_id("group.com.x.y", RelatedKind::GroupContainers),
            "com.x.y"
        );
        // Apple Podcasts: must resolve to com.apple.*, so it is never a leftover.
        assert_eq!(
            entry_id(
                "243LU875E5.groups.com.apple.podcasts",
                RelatedKind::GroupContainers
            ),
            "com.apple.podcasts"
        );
        assert_eq!(
            entry_id("UBF8T346G9.com.x", RelatedKind::Support),
            "UBF8T346G9.com.x"
        );
        assert!(looks_like_bundle_id("com.foo.bar"));
        assert!(!looks_like_bundle_id("com.foo"));
        assert!(!looks_like_bundle_id("Random Folder"));
    }

    #[test]
    fn system_and_own_apps_are_locked() {
        let mut a = app("Safari", "com.apple.Safari");
        assert!(locked_reason(&a).is_some());
        a.source = AppSource::AppStore;
        a.bundle_id = "com.apple.iWork.Pages".into();
        assert!(
            locked_reason(&a).is_none(),
            "App Store apps from Apple can go"
        );
        let mut s = app("Notes", "com.apple.Notes");
        s.source = AppSource::System;
        s.path = "/System/Applications/Notes.app".into();
        assert!(locked_reason(&s).is_some());
        assert!(locked_reason(&app("MyMacStorage", "com.mymacstorage.app")).is_some());
        assert!(locked_reason(&app("Foo", "com.foo.bar")).is_none());
    }

    #[test]
    fn uninstall_plan_only_takes_the_apps_files() {
        let apps = ten();
        let mut t = Tree::new();
        let appdir = add(&mut t, ROOT, "/Applications", true, 0);
        let foo = add(&mut t, appdir, "Foo.app", true, 0);
        add(&mut t, foo, "bin", false, 5000);
        let h = add(&mut t, ROOT, "/Users/me", true, 0);
        let lib = add(&mut t, h, "Library", true, 0);
        let sup = add(&mut t, lib, "Application Support", true, 0);
        add(&mut t, sup, "com.foo.bar", false, 10);
        let other = add(&mut t, sup, "com.foo.barista", false, 10);
        t.aggregate();
        let rel = related_all(&t, Path::new("/Users/me"), &apps);
        let foo_rel = &rel[&0];
        let ids: Vec<NodeId> = foo_rel.iter().map(|r| r.node).collect();
        let plan = plan_uninstall(&t, &apps[0], foo_rel, &ids).unwrap();
        assert_eq!(plan.len(), 2);
        assert_eq!(plan[0].0.path, PathBuf::from("/Applications/Foo.app"));
        assert!(plan_uninstall(&t, &apps[0], foo_rel, &[other]).is_err());

        // A container shared with the developer's other apps is refused even if asked for.
        let mut shared = foo_rel[0].clone();
        shared.confidence = Confidence::Shared;
        let err = plan_uninstall(&t, &apps[0], &[shared.clone()], &[shared.node]).unwrap_err();
        assert!(err.contains("shared"));
    }

    /// An app updated after the scan is not moved, and then neither is its data.
    #[test]
    fn data_stays_when_the_app_itself_cannot_move() {
        use crate::scanner::{scan, std_reader::StdReader};
        let tmp = tempfile::tempdir().unwrap();
        let base = tmp.path().canonicalize().unwrap();
        let apps_dir = base.join("Applications");
        let app = apps_dir.join("Foo.app");
        std::fs::create_dir_all(app.join("Contents")).unwrap();
        let data = base.join("Library/Containers/com.foo.bar");
        std::fs::create_dir_all(&data).unwrap();
        std::fs::write(data.join("notes.db"), "precious").unwrap();
        let roots = vec![apps_dir.clone(), base.join("Library/Containers")];
        let out = scan(
            &StdReader,
            &roots,
            &Default::default(),
            &std::sync::atomic::AtomicBool::new(false),
            &mut |_| {},
        )
        .unwrap();
        let cand = |p: &Path| {
            candidate(&out.tree, out.tree.find_path(p).unwrap(), p.to_path_buf()).unwrap()
        };
        let plan = vec![cand(&app), cand(&data)];
        // An update replaces the bundle after the scan.
        std::fs::remove_dir_all(&app).unwrap();
        std::fs::create_dir_all(app.join("Contents")).unwrap();
        let trash_bin = trash::FakeTrash::new(&base).unwrap();
        let out = uninstall(&plan, &trash::Context::new(roots), &trash_bin);
        assert!(out.moved.is_empty(), "{:?}", out.moved);
        assert_eq!(out.skipped.len(), 2);
        assert!(out.skipped[1].2.contains("app itself"));
        assert_eq!(
            std::fs::read_to_string(data.join("notes.db")).unwrap(),
            "precious"
        );
    }

    /// A real app bundle and its Library files: uninstall moves exactly those, and restoring
    /// the batch brings every one of them back.
    #[test]
    fn uninstall_then_undo_restores_everything() {
        use crate::scanner::{scan, std_reader::StdReader};
        let tmp = tempfile::tempdir().unwrap();
        let base = tmp.path().canonicalize().unwrap();
        let home = base.join("home");
        let apps_dir = base.join("Applications");
        let bundle = apps_dir.join("Foo.app/Contents");
        std::fs::create_dir_all(bundle.join("MacOS")).unwrap();
        std::fs::write(
            bundle.join("Info.plist"),
            r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><dict>
<key>CFBundleIdentifier</key><string>com.foo.bar</string>
<key>CFBundleName</key><string>Foo</string>
<key>CFBundleExecutable</key><string>Foo</string>
</dict></plist>"#,
        )
        .unwrap();
        std::fs::write(
            bundle.join("MacOS/Foo"),
            [0xcf, 0xfa, 0xed, 0xfe, 0x0c, 0, 0, 1],
        )
        .unwrap();
        let support = home.join("Library/Application Support/com.foo.bar");
        std::fs::create_dir_all(&support).unwrap();
        std::fs::write(support.join("db"), "data").unwrap();
        let prefs = home.join("Library/Preferences");
        std::fs::create_dir_all(&prefs).unwrap();
        std::fs::write(prefs.join("com.foo.bar.plist"), "prefs").unwrap();
        std::fs::write(prefs.join("com.other.app.plist"), "other").unwrap();

        let apps = discover(std::slice::from_ref(&apps_dir));
        assert_eq!(apps.len(), 1);
        assert_eq!(apps[0].arch, AppArch::AppleSilicon);
        let roots = vec![apps_dir.clone(), home.join("Library")];
        let out = scan(
            &StdReader,
            &roots,
            &Default::default(),
            &std::sync::atomic::AtomicBool::new(false),
            &mut |_| {},
        )
        .unwrap();
        let rel = related_all(&out.tree, &home, &apps);
        let foo = &rel[&0];
        assert_eq!(foo.len(), 2);
        let ids: Vec<NodeId> = foo.iter().map(|r| r.node).collect();
        let plan = plan_uninstall(&out.tree, &apps[0], foo, &ids).unwrap();
        let trash_bin = trash::FakeTrash::new(&base).unwrap();
        let moved = uninstall(&plan, &trash::Context::new(roots), &trash_bin);
        assert_eq!(moved.moved.len(), 3, "{:?}", moved.skipped);
        assert!(!apps_dir.join("Foo.app").exists());
        assert!(
            prefs.join("com.other.app.plist").exists(),
            "other apps untouched"
        );
        for m in &moved.moved {
            assert!(matches!(
                trash::restore(&m.trashed, &m.original, false),
                trash::Restore::Restored(_)
            ));
        }
        assert!(bundle.join("MacOS/Foo").exists());
        assert_eq!(std::fs::read_to_string(support.join("db")).unwrap(), "data");
        assert_eq!(
            std::fs::read_to_string(prefs.join("com.foo.bar.plist")).unwrap(),
            "prefs"
        );
    }

    #[test]
    fn macho_headers() {
        let tmp = tempfile::tempdir().unwrap();
        let write = |name: &str, bytes: &[u8]| {
            let p = tmp.path().join(name);
            std::fs::write(&p, bytes).unwrap();
            p
        };
        let thin_arm = write("arm", &[0xcf, 0xfa, 0xed, 0xfe, 0x0c, 0, 0, 0x01]);
        assert_eq!(macho_arch(&thin_arm), AppArch::AppleSilicon);
        let thin_x86 = write("x86", &[0xcf, 0xfa, 0xed, 0xfe, 0x07, 0, 0, 0x01]);
        assert_eq!(macho_arch(&thin_x86), AppArch::Intel);
        let mut fat = vec![0xca, 0xfe, 0xba, 0xbe, 0, 0, 0, 2];
        for cpu in [[0x01, 0, 0, 0x07], [0x01, 0, 0, 0x0c]] {
            fat.extend_from_slice(&cpu);
            fat.extend_from_slice(&[0; 16]);
        }
        assert_eq!(macho_arch(&write("fat", &fat)), AppArch::Universal);
        let java = write("Class", &[0xca, 0xfe, 0xba, 0xbe, 0, 0, 0, 52]);
        assert_eq!(macho_arch(&java), AppArch::Unknown);
        assert_eq!(macho_arch(Path::new("/nope")), AppArch::Unknown);
    }

    #[test]
    fn reads_a_real_system_app() {
        let apps = discover(&[PathBuf::from("/System/Applications")]);
        let calc = apps.iter().find(|a| a.bundle_id == "com.apple.calculator");
        if let Some(c) = calc {
            assert_eq!(c.source, AppSource::System);
            assert!(locked_reason(c).is_some());
            assert_ne!(c.arch, AppArch::Unknown);
        }
    }
}
