//! Exact duplicate detection and safe removal (M5).
//!
//! Stages: group by size → quick BLAKE3 of the first and last 64 KB → full streamed BLAKE3.
//! Each file's size and modification time are read before and after hashing; a file that
//! changed while being read is dropped. Before anything moves, every copy is compared byte for
//! byte with the copy being kept, and a group never loses its last copy: [`Session`] refuses any
//! selection that covers a whole group.

use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::os::macos::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use rayon::prelude::*;

use crate::errors::AppError;
use crate::index::{flags, NodeId, NodeKind, Tree, ROOT};
use crate::models::{
    DupFile, DupGroup, DupGroupsPage, DupOptions, DupProgress, DupSavings, DupStage, DupSummary,
    FileKind, KeepRule, SkippedItem,
};
use crate::platform;
use crate::space::file_kind;
use crate::trash::{self, Candidate, Context, Expected, MoveOutcome, TrashBackend};

const EDGE: u64 = 64 * 1024;
const BUF: usize = 1 << 20;
pub const GROUPS_PAGE: usize = 50;
const MAX_REPORTED_SKIPS: usize = 500;

/// A file that may have a twin, with what the scan saw.
#[derive(Debug, Clone)]
pub struct Cand {
    pub node: NodeId,
    pub path: PathBuf,
    pub size: u64,
    pub mtime: i64,
    pub dev: u32,
    pub inode: u64,
}

impl Cand {
    fn candidate(&self) -> Candidate {
        Candidate {
            path: self.path.clone(),
            expected: Expected {
                dev: self.dev,
                inode: self.inode,
                size: self.size,
                mtime: self.mtime,
                is_dir: false,
            },
        }
    }
}

/// Folders never searched: system areas, dependency and VCS stores.
fn excluded_dir(tree: &Tree, id: NodeId, opts: &DupOptions) -> bool {
    let Some(n) = tree.node(id) else { return true };
    let name = tree.name(id);
    if name == b"node_modules" || name == b".git" {
        return true;
    }
    // App data: hidden folders and the home folder's Library. Copies there are usually caches
    // or profiles that apps expect to find, so they are left out unless asked for.
    if !opts.include_app_data {
        if name.first() == Some(&b'.') {
            return true;
        }
        if name == b"Library" {
            let path = tree.path(id);
            let parts: Vec<_> = path.components().collect();
            if parts.len() == 4 && path.starts_with("/Users") {
                return true;
            }
        }
    }
    if n.has(flags::PACKAGE) {
        let lower = String::from_utf8_lossy(name).to_lowercase();
        let media = lower.ends_with(".photoslibrary") || lower.ends_with(".musiclibrary");
        // Package insides (apps, bundles) are never touched; media libraries only on request.
        return !(media && opts.include_media_libraries);
    }
    if n.parent == ROOT || tree.node(n.parent).is_some_and(|p| p.parent == ROOT) {
        let path = tree.path(id);
        return path == Path::new("/System")
            || path == Path::new("/Library")
            || path == Path::new("/private")
            || path.starts_with("/System/");
    }
    false
}

/// Files worth checking, one name per inode, honoring options and exclusions.
pub fn candidates(tree: &Tree, opts: &DupOptions) -> Vec<Cand> {
    let start = opts
        .scope
        .filter(|s| tree.node(*s).is_some())
        .unwrap_or(ROOT);
    let min = opts.min_size.max(1);
    let mut seen_inodes: HashSet<(u32, u64)> = HashSet::new();
    let mut out = Vec::new();
    let mut stack = vec![start];
    while let Some(dir) = stack.pop() {
        for c in tree.children(dir) {
            let Some(n) = tree.node(c) else { continue };
            match n.kind {
                NodeKind::Dir => {
                    if !excluded_dir(tree, c, opts) {
                        stack.push(c);
                    }
                }
                NodeKind::File => {
                    if n.size_logical < min || n.has(flags::DATALESS | flags::DELETED) {
                        continue;
                    }
                    if !opts.kinds.is_empty()
                        && !opts.kinds.contains(&file_kind(tree.name(c), false, false))
                    {
                        continue;
                    }
                    // Hard links share their bytes: they are one file, not duplicates.
                    if n.has(flags::MULTILINK) && !seen_inodes.insert((n.dev, n.inode)) {
                        continue;
                    }
                    out.push(Cand {
                        node: c,
                        path: tree.path(c),
                        size: n.size_logical,
                        mtime: n.mtime,
                        dev: n.dev,
                        inode: n.inode,
                    });
                }
                NodeKind::Symlink => {}
            }
        }
    }
    out
}

/// (size, mtime seconds, mtime nanoseconds) as seen now.
fn signature(path: &Path) -> std::io::Result<(u64, i64, i64)> {
    let m = std::fs::symlink_metadata(path)?;
    if !m.is_file() {
        return Err(std::io::Error::other("not a regular file anymore"));
    }
    Ok((m.len(), m.st_mtime(), m.st_mtime_nsec()))
}

fn quick_hash(path: &Path, size: u64) -> std::io::Result<[u8; 32]> {
    let mut f = File::open(path)?;
    let mut h = blake3::Hasher::new();
    let mut buf = vec![0u8; EDGE as usize];
    let head = size.min(EDGE) as usize;
    f.read_exact(&mut buf[..head])?;
    h.update(&buf[..head]);
    if size > EDGE {
        let tail_start = size.saturating_sub(EDGE).max(EDGE.min(size));
        let tail = (size - tail_start) as usize;
        f.seek(SeekFrom::Start(tail_start))?;
        f.read_exact(&mut buf[..tail])?;
        h.update(&buf[..tail]);
    }
    h.update(&size.to_le_bytes());
    Ok(*h.finalize().as_bytes())
}

/// Stream the whole file through BLAKE3 with one fixed buffer. `None` if cancelled.
fn full_hash(
    path: &Path,
    buf: &mut [u8],
    cancel: &AtomicBool,
    bytes_done: &AtomicU64,
) -> std::io::Result<Option<[u8; 32]>> {
    let mut f = File::open(path)?;
    let mut h = blake3::Hasher::new();
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Ok(None);
        }
        let n = f.read(buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
        bytes_done.fetch_add(n as u64, Ordering::Relaxed);
    }
    Ok(Some(*h.finalize().as_bytes()))
}

/// True if both files have exactly the same bytes. Streams, stops at the first difference.
pub fn same_bytes(a: &Path, b: &Path) -> std::io::Result<bool> {
    let (ma, mb) = (std::fs::symlink_metadata(a)?, std::fs::symlink_metadata(b)?);
    if !ma.is_file() || !mb.is_file() || ma.len() != mb.len() {
        return Ok(false);
    }
    let (mut fa, mut fb) = (File::open(a)?, File::open(b)?);
    let (mut ba, mut bb) = (vec![0u8; BUF], vec![0u8; BUF]);
    loop {
        let na = read_full(&mut fa, &mut ba)?;
        let nb = read_full(&mut fb, &mut bb)?;
        if na != nb || ba[..na] != bb[..nb] {
            return Ok(false);
        }
        if na == 0 {
            return Ok(true);
        }
    }
}

fn read_full(f: &mut File, buf: &mut [u8]) -> std::io::Result<usize> {
    let mut filled = 0;
    while filled < buf.len() {
        let n = f.read(&mut buf[filled..])?;
        if n == 0 {
            break;
        }
        filled += n;
    }
    Ok(filled)
}

/// A finished group of identical files.
#[derive(Debug, Clone)]
pub struct Found {
    pub size: u64,
    pub hash: [u8; 32],
    pub files: Vec<Cand>,
}

#[derive(Debug, Default)]
pub struct FindResult {
    pub groups: Vec<Found>,
    pub skipped: Vec<(PathBuf, String)>,
    pub cancelled: bool,
}

/// Called after a file is read and before its final size/time check. Tests use it to change a
/// file in the middle of hashing.
pub type Hook = dyn Fn(&Path) + Sync;

struct Counters {
    files: AtomicU32,
    bytes: AtomicU64,
}

/// Number of hashing threads: fewer when files live on other (often slower) volumes.
fn threads_for(cands: &[&Cand]) -> usize {
    let cores = std::thread::available_parallelism().map_or(4, |n| n.get());
    let home_dev = std::env::var_os("HOME")
        .and_then(|h| std::fs::metadata(h).ok())
        .map(|m| m.st_dev() as u32);
    let root_dev = std::fs::metadata("/").ok().map(|m| m.st_dev() as u32);
    let external = cands
        .iter()
        .any(|c| Some(c.dev) != home_dev && Some(c.dev) != root_dev);
    if external {
        2
    } else {
        cores.clamp(1, 6)
    }
}

/// Run `work` over `items` on a bounded pool while a reporter thread emits progress.
#[allow(clippy::too_many_arguments)]
fn staged<T: Sync, R: Send>(
    stage: DupStage,
    items: &[T],
    bytes_total: u64,
    threads: usize,
    progress: &(dyn Fn(DupProgress) + Sync),
    counters: &Counters,
    work: impl Fn(&T, &mut Vec<u8>) -> R + Sync + Send,
) -> Vec<R> {
    counters.files.store(0, Ordering::Relaxed);
    counters.bytes.store(0, Ordering::Relaxed);
    let files_total = items.len() as u32;
    let done = AtomicBool::new(false);
    let start = Instant::now();
    let report = || {
        let files_done = counters.files.load(Ordering::Relaxed);
        let bytes_done = counters.bytes.load(Ordering::Relaxed);
        let elapsed = start.elapsed().as_millis() as u64;
        let eta_ms = (bytes_done > 0 && bytes_total > bytes_done)
            .then(|| elapsed * (bytes_total - bytes_done) / bytes_done);
        progress(DupProgress {
            stage,
            files_done,
            files_total,
            bytes_done,
            bytes_total,
            eta_ms,
        });
    };
    let pool = rayon::ThreadPoolBuilder::new().num_threads(threads).build();
    std::thread::scope(|s| {
        s.spawn(|| {
            while !done.load(Ordering::Relaxed) {
                report();
                std::thread::sleep(Duration::from_millis(100));
            }
        });
        let run = || {
            items
                .par_iter()
                .map_init(Vec::new, |buf, item| {
                    let r = work(item, buf);
                    counters.files.fetch_add(1, Ordering::Relaxed);
                    r
                })
                .collect::<Vec<R>>()
        };
        let out = match &pool {
            Ok(p) => p.install(run),
            Err(_) => run(),
        };
        done.store(true, Ordering::Relaxed);
        report();
        out
    })
}

/// Find groups of identical files among `cands`.
pub fn find_duplicates(
    cands: Vec<Cand>,
    cancel: &AtomicBool,
    progress: &(dyn Fn(DupProgress) + Sync),
    hook: Option<&Hook>,
) -> FindResult {
    let mut result = FindResult::default();
    let skipped = Mutex::new(Vec::<(PathBuf, String)>::new());
    let skip = |p: &Path, why: String| {
        if let Ok(mut s) = skipped.lock() {
            s.push((p.to_path_buf(), why));
        }
    };
    let counters = Counters {
        files: AtomicU32::new(0),
        bytes: AtomicU64::new(0),
    };

    // 1. Same size.
    progress(DupProgress {
        stage: DupStage::Grouping,
        files_done: 0,
        files_total: cands.len() as u32,
        bytes_done: 0,
        bytes_total: 0,
        eta_ms: None,
    });
    let mut by_size: HashMap<u64, Vec<Cand>> = HashMap::new();
    for c in cands {
        by_size.entry(c.size).or_default().push(c);
    }
    let sized: Vec<Cand> = by_size
        .into_values()
        .filter(|v| v.len() > 1)
        .flatten()
        .collect();
    if cancel.load(Ordering::Relaxed) {
        result.cancelled = true;
        return result;
    }

    // 2. Quick hash of both ends. The file must still have the size the scan saw.
    let refs: Vec<&Cand> = sized.iter().collect();
    let threads = threads_for(&refs);
    let quick_total: u64 = sized.iter().map(|c| c.size.min(2 * EDGE)).sum();
    let quick = staged(
        DupStage::QuickCheck,
        &sized,
        quick_total,
        threads,
        progress,
        &counters,
        |c, _| {
            if cancel.load(Ordering::Relaxed) {
                return None;
            }
            let h = match signature(&c.path) {
                Ok((size, _, _)) if size == c.size => quick_hash(&c.path, c.size),
                Ok(_) => {
                    skip(&c.path, "It changed after the scan.".into());
                    return None;
                }
                Err(e) => Err(e),
            };
            counters
                .bytes
                .fetch_add(c.size.min(2 * EDGE), Ordering::Relaxed);
            match h {
                Ok(h) => Some((c.size, h)),
                Err(e) => {
                    skip(&c.path, format!("Could not be read: {e}"));
                    None
                }
            }
        },
    );
    if cancel.load(Ordering::Relaxed) {
        result.cancelled = true;
        result.skipped = skipped.into_inner().unwrap_or_default();
        return result;
    }
    let mut by_quick: HashMap<(u64, [u8; 32]), Vec<&Cand>> = HashMap::new();
    for (c, k) in sized.iter().zip(quick) {
        if let Some(k) = k {
            by_quick.entry(k).or_default().push(c);
        }
    }
    let stage3: Vec<&Cand> = by_quick
        .into_values()
        .filter(|v| v.len() > 1)
        .flatten()
        .collect();

    // 3. Full hash, with a before/after check so a file changing mid-read is never trusted.
    let full_total: u64 = stage3.iter().map(|c| c.size).sum();
    let full = staged(
        DupStage::Comparing,
        &stage3,
        full_total,
        threads,
        progress,
        &counters,
        |c, buf| {
            if buf.len() != BUF {
                buf.resize(BUF, 0);
            }
            let before = match signature(&c.path) {
                Ok(s) => s,
                Err(e) => {
                    skip(&c.path, format!("Could not be read: {e}"));
                    return None;
                }
            };
            let h = match full_hash(&c.path, buf, cancel, &counters.bytes) {
                Ok(Some(h)) => h,
                Ok(None) => return None,
                Err(e) => {
                    skip(&c.path, format!("Could not be read: {e}"));
                    return None;
                }
            };
            if let Some(hook) = hook {
                hook(&c.path);
            }
            match signature(&c.path) {
                Ok(after) if after == before && after.0 == c.size => Some(h),
                _ => {
                    skip(&c.path, "It changed while being compared.".into());
                    None
                }
            }
        },
    );
    if cancel.load(Ordering::Relaxed) {
        result.cancelled = true;
    }
    let mut by_full: HashMap<(u64, [u8; 32]), Vec<Cand>> = HashMap::new();
    for (c, h) in stage3.iter().zip(full) {
        if let Some(h) = h {
            by_full.entry((c.size, h)).or_default().push((*c).clone());
        }
    }
    result.groups = by_full
        .into_iter()
        .filter(|(_, v)| v.len() > 1)
        .map(|((size, hash), mut files)| {
            files.sort_by(|a, b| a.path.cmp(&b.path));
            Found { size, hash, files }
        })
        .collect();
    result.skipped = skipped.into_inner().unwrap_or_default();
    result
}

// ---------------------------------------------------------------------------------------------
// Session: groups plus the user's selection. Invariant: no group is ever fully selected.
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct SFile {
    pub cand: Cand,
    pub private_size: u64,
    pub selected: bool,
}

#[derive(Debug, Clone)]
pub struct SGroup {
    pub id: u32,
    pub size: u64,
    pub kind: FileKind,
    pub files: Vec<SFile>,
}

impl SGroup {
    fn reclaimable(&self) -> u64 {
        self.files
            .iter()
            .filter(|f| f.selected)
            .map(|f| f.private_size)
            .sum()
    }

    /// Select everything except `keep`.
    fn keep_only(&mut self, keep: usize) {
        for (i, f) in self.files.iter_mut().enumerate() {
            f.selected = i != keep;
        }
    }
}

#[derive(Debug, Default)]
pub struct Session {
    pub groups: Vec<SGroup>,
    pub summary: DupSummary,
}

fn name_of(p: &Path) -> String {
    p.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

impl Session {
    /// Build a session from found groups. `private` reports freeable bytes per file
    /// (falls back to the logical size).
    pub fn new(found: FindResult, elapsed_ms: u64, private: &dyn Fn(&Path) -> Option<u64>) -> Self {
        let mut groups: Vec<SGroup> = found
            .groups
            .into_iter()
            .map(|g| SGroup {
                id: 0,
                size: g.size,
                kind: g.files.first().map_or(FileKind::Other, |f| {
                    file_kind(name_of(&f.path).as_bytes(), false, false)
                }),
                files: g
                    .files
                    .into_iter()
                    .map(|c| SFile {
                        private_size: private(&c.path).unwrap_or(c.size),
                        cand: c,
                        selected: false,
                    })
                    .collect(),
            })
            .collect();
        // Largest potential first; ids follow that order and stay stable afterwards.
        groups.sort_by_key(|g| std::cmp::Reverse(potential(g)));
        for (i, g) in groups.iter_mut().enumerate() {
            g.id = i as u32;
        }
        let summary = DupSummary {
            groups: groups.len() as u32,
            files: groups.iter().map(|g| g.files.len() as u32).sum(),
            potential_bytes: groups.iter().map(potential).sum(),
            skipped: found
                .skipped
                .iter()
                .take(MAX_REPORTED_SKIPS)
                .map(|(p, r)| SkippedItem {
                    path: p.to_string_lossy().into_owned(),
                    reason: r.clone(),
                })
                .collect(),
            elapsed_ms,
            cancelled: found.cancelled,
        };
        Self { groups, summary }
    }

    fn group_mut(&mut self, id: u32) -> Result<&mut SGroup, AppError> {
        self.groups
            .iter_mut()
            .find(|g| g.id == id)
            .ok_or_else(|| AppError::NotFound(format!("duplicate group {id}")))
    }

    /// Apply a keep rule to the chosen groups (all when `ids` is `None`).
    pub fn apply_rule(&mut self, rule: KeepRule, preferred: Option<&Path>, ids: Option<&[u32]>) {
        for g in self
            .groups
            .iter_mut()
            .filter(|g| ids.is_none_or(|ids| ids.contains(&g.id)))
        {
            let pick = |key: &dyn Fn(&SFile) -> (i64, usize, String)| {
                g.files
                    .iter()
                    .enumerate()
                    .min_by_key(|(_, f)| key(f))
                    .map(|(i, _)| i)
            };
            let path_s = |f: &SFile| f.cand.path.to_string_lossy().into_owned();
            let keep = match rule {
                KeepRule::Newest => pick(&|f| (-f.cand.mtime, 0, path_s(f))),
                KeepRule::Oldest => pick(&|f| (f.cand.mtime, 0, path_s(f))),
                KeepRule::ShortestPath => pick(&|f| {
                    let s = path_s(f);
                    (s.matches('/').count() as i64, s.len(), s)
                }),
                KeepRule::PreferredFolder => preferred.and_then(|dir| {
                    g.files
                        .iter()
                        .enumerate()
                        .filter(|(_, f)| f.cand.path.starts_with(dir))
                        .min_by_key(|(_, f)| path_s(f))
                        .map(|(i, _)| i)
                }),
                KeepRule::Manual => {
                    g.files.iter_mut().for_each(|f| f.selected = false);
                    None
                }
            };
            if let Some(k) = keep {
                g.keep_only(k);
            }
        }
    }

    /// Select or unselect one copy. Refuses to select the last unselected copy of a group.
    pub fn set_selected(&mut self, group: u32, node: NodeId, on: bool) -> Result<(), AppError> {
        let g = self.group_mut(group)?;
        let unselected = g.files.iter().filter(|f| !f.selected).count();
        let f = g
            .files
            .iter_mut()
            .find(|f| f.cand.node == node)
            .ok_or_else(|| AppError::NotFound("file is not in this group".into()))?;
        if on && !f.selected && unselected <= 1 {
            return Err(AppError::InvalidInput(
                "At least one copy must be kept.".into(),
            ));
        }
        f.selected = on;
        Ok(())
    }

    pub fn savings(&self) -> DupSavings {
        let mut s = DupSavings::default();
        for g in &self.groups {
            let n = g.files.iter().filter(|f| f.selected).count() as u32;
            if n > 0 {
                s.groups += 1;
                s.files += n;
                s.bytes += g.reclaimable();
            }
        }
        s
    }

    /// Groups ordered by current reclaimable bytes, then potential.
    pub fn page(&self, page: u32, kind: Option<FileKind>) -> DupGroupsPage {
        let mut gs: Vec<&SGroup> = self
            .groups
            .iter()
            .filter(|g| kind.is_none_or(|k| g.kind == k))
            .collect();
        gs.sort_by_key(|g| {
            (
                std::cmp::Reverse(g.reclaimable()),
                std::cmp::Reverse(potential(g)),
                g.id,
            )
        });
        let start = (page as usize) * GROUPS_PAGE;
        let end = (start + GROUPS_PAGE).min(gs.len());
        DupGroupsPage {
            groups: gs
                .get(start..end)
                .unwrap_or(&[])
                .iter()
                .map(|g| DupGroup {
                    id: g.id,
                    size: g.size,
                    file_kind: g.kind,
                    name: g
                        .files
                        .first()
                        .map(|f| name_of(&f.cand.path))
                        .unwrap_or_default(),
                    reclaimable: g.reclaimable(),
                    files: g
                        .files
                        .iter()
                        .map(|f| DupFile {
                            node_id: f.cand.node,
                            name: name_of(&f.cand.path),
                            path: f.cand.path.to_string_lossy().into_owned(),
                            mtime: f.cand.mtime,
                            private_size: f.private_size,
                            selected: f.selected,
                        })
                        .collect(),
                })
                .collect(),
            total: gs.len() as u32,
            next_page: (end < gs.len()).then_some(page + 1),
        }
    }

    /// Bytes that could be freed by keeping one copy per group.
    pub fn potential_bytes(&self) -> u64 {
        self.groups.iter().map(potential).sum()
    }

    /// Node ids of every selected copy (for the Review sheet).
    pub fn selected_nodes(&self) -> Vec<NodeId> {
        self.groups
            .iter()
            .flat_map(|g| g.files.iter().filter(|f| f.selected).map(|f| f.cand.node))
            .collect()
    }

    /// Forget files that were removed or replaced; drop groups with a single file left.
    pub fn forget(&mut self, nodes: &HashSet<NodeId>) {
        for g in &mut self.groups {
            g.files.retain(|f| !nodes.contains(&f.cand.node));
        }
        self.groups.retain(|g| g.files.len() > 1);
    }

    /// Check every selected copy against the copy being kept, byte for byte.
    /// Returns the copies safe to move (with sizes) and the ones left alone with a reason.
    pub fn verify(&self, ctx: &Context) -> Verified {
        let mut v = Verified::default();
        for g in &self.groups {
            let selected: Vec<&SFile> = g.files.iter().filter(|f| f.selected).collect();
            if selected.is_empty() {
                continue;
            }
            let leave_all = |v: &mut Verified, why: &str| {
                for f in &selected {
                    v.skipped
                        .push((f.cand.path.clone(), f.private_size, why.to_string()));
                }
            };
            // Defense in depth: the session never allows this, but never trust it blindly.
            let Some(kept) = g.files.iter().find(|f| !f.selected) else {
                leave_all(
                    &mut v,
                    "Every copy was selected; nothing in this group was moved.",
                );
                continue;
            };
            if trash::preflight(&kept.cand.candidate(), ctx).is_err() {
                leave_all(
                    &mut v,
                    "The copy you keep changed or is gone, so nothing in this group was moved.",
                );
                continue;
            }
            for f in selected {
                match same_bytes(&kept.cand.path, &f.cand.path) {
                    Ok(true) => v.ok.push((f.cand.clone(), f.private_size)),
                    Ok(false) => v.skipped.push((
                        f.cand.path.clone(),
                        f.private_size,
                        "Its contents no longer match the copy you keep.".into(),
                    )),
                    Err(e) => v.skipped.push((
                        f.cand.path.clone(),
                        f.private_size,
                        format!("Could not be compared: {e}"),
                    )),
                }
            }
        }
        v
    }
}

fn potential(g: &SGroup) -> u64 {
    let total: u64 = g.files.iter().map(|f| f.private_size).sum();
    let max = g.files.iter().map(|f| f.private_size).max().unwrap_or(0);
    total - max
}

#[derive(Debug, Default)]
pub struct Verified {
    pub ok: Vec<(Cand, u64)>,
    pub skipped: Vec<(PathBuf, u64, String)>,
}

/// Move verified copies to the Trash. Items left alone by verification are reported too.
pub fn trash_verified(
    v: Verified,
    ctx: &Context,
    backend: &dyn TrashBackend,
) -> (MoveOutcome, Vec<NodeId>) {
    let cands: Vec<(Candidate, u64)> = v.ok.iter().map(|(c, s)| (c.candidate(), *s)).collect();
    let mut out = trash::move_items(&cands, ctx, backend);
    let moved_paths: HashSet<&Path> = out.moved.iter().map(|m| m.original.as_path()).collect();
    let moved_nodes =
        v.ok.iter()
            .filter(|(c, _)| moved_paths.contains(c.path.as_path()))
            .map(|(c, _)| c.node)
            .collect();
    out.skipped.extend(v.skipped);
    (out, moved_nodes)
}

/// Replace each verified duplicate with an APFS clone of the kept copy. The original goes to the
/// Trash first (so Undo works); the clone then takes its name. Paths keep working.
pub fn replace_with_clones(
    session: &Session,
    v: Verified,
    ctx: &Context,
    backend: &dyn TrashBackend,
) -> (MoveOutcome, Vec<NodeId>) {
    let kept_for: HashMap<NodeId, PathBuf> = session
        .groups
        .iter()
        .filter_map(|g| {
            let kept = g.files.iter().find(|f| !f.selected)?;
            Some(
                g.files
                    .iter()
                    .filter(|f| f.selected)
                    .map(|f| (f.cand.node, kept.cand.path.clone()))
                    .collect::<Vec<_>>(),
            )
        })
        .flatten()
        .collect();
    let mut out = MoveOutcome::default();
    let mut done = Vec::new();
    out.skipped.extend(v.skipped);
    for (i, (c, size)) in v.ok.into_iter().enumerate() {
        let Some(kept) = kept_for.get(&c.node) else {
            continue;
        };
        match clone_one(kept, &c, ctx, backend, i) {
            Ok(trashed) => {
                out.moved.push(trash::Moved {
                    original: c.path.clone(),
                    trashed,
                    size,
                });
                done.push(c.node);
            }
            Err(reason) => out.skipped.push((c.path.clone(), size, reason)),
        }
    }
    (out, done)
}

fn clone_one(
    kept: &Path,
    dup: &Cand,
    ctx: &Context,
    backend: &dyn TrashBackend,
    n: usize,
) -> Result<PathBuf, String> {
    trash::preflight(&dup.candidate(), ctx)?;
    let parent = dup.path.parent().ok_or("This path has no parent folder.")?;
    let tmp = parent.join(format!(".duobolt-clone-{}-{n}", std::process::id()));
    platform::clone_file(kept, &tmp).map_err(|e| format!("Could not create a clone: {e}"))?;
    let cleanup = |e: String| {
        let _ = std::fs::remove_file(&tmp);
        e
    };
    // Keep the duplicate's permissions and timestamps on the clone.
    let meta = std::fs::metadata(&dup.path).map_err(|e| cleanup(e.to_string()))?;
    std::fs::set_permissions(&tmp, meta.permissions()).map_err(|e| cleanup(e.to_string()))?;
    let times = std::fs::FileTimes::new()
        .set_accessed(meta.accessed().map_err(|e| cleanup(e.to_string()))?)
        .set_modified(meta.modified().map_err(|e| cleanup(e.to_string()))?);
    File::options()
        .write(true)
        .open(&tmp)
        .and_then(|f| f.set_times(times))
        .map_err(|e| cleanup(e.to_string()))?;
    if !same_bytes(kept, &tmp).map_err(|e| cleanup(e.to_string()))? {
        return Err(cleanup(
            "The clone did not match; nothing was changed.".into(),
        ));
    }
    let trashed = backend.trash(&dup.path).map_err(cleanup)?;
    if let Err(e) = std::fs::rename(&tmp, &dup.path) {
        let _ = std::fs::remove_file(&tmp);
        return Err(format!(
            "The original is in the Trash but the clone could not take its place ({e}). Undo restores it."
        ));
    }
    Ok(trashed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scanner::{scan, std_reader::StdReader};
    use crate::trash::FakeTrash;
    use std::io::Write;

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
        Env {
            trash: FakeTrash::new(&base).unwrap(),
            ctx: Context {
                roots: vec![root.clone()],
                app_bundle: None,
            },
            root,
            _tmp: tmp,
        }
    }

    fn write(path: &Path, data: &[u8]) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::File::create(path)
            .unwrap()
            .write_all(data)
            .unwrap();
    }

    fn bytes(seed: u8, len: usize) -> Vec<u8> {
        (0..len)
            .map(|i| (i as u8).wrapping_mul(31).wrapping_add(seed))
            .collect()
    }

    fn run(root: &Path, opts: &DupOptions, hook: Option<&Hook>) -> (Tree, Session) {
        let cancel = AtomicBool::new(false);
        let out = scan(
            &StdReader,
            &[root.to_path_buf()],
            &Default::default(),
            &cancel,
            &mut |_| {},
        )
        .unwrap();
        let cands = candidates(&out.tree, opts);
        let found = find_duplicates(cands, &cancel, &|_| {}, hook);
        (out.tree, Session::new(found, 0, &platform::private_size))
    }

    fn opts() -> DupOptions {
        DupOptions {
            min_size: 1,
            ..DupOptions::default()
        }
    }

    fn group_names(s: &Session) -> Vec<Vec<String>> {
        let mut v: Vec<Vec<String>> = s
            .groups
            .iter()
            .map(|g| {
                let mut n: Vec<String> = g
                    .files
                    .iter()
                    .map(|f| {
                        f.cand
                            .path
                            .file_name()
                            .unwrap()
                            .to_string_lossy()
                            .into_owned()
                    })
                    .collect();
                n.sort();
                n
            })
            .collect();
        v.sort();
        v
    }

    #[test]
    fn fixture_has_no_false_positives_or_negatives() {
        let e = env();
        let r = &e.root;
        let a = bytes(1, 300_000);
        write(&r.join("x/a.dat"), &a);
        write(&r.join("y/a-copy.dat"), &a);
        write(&r.join("z/renamed.bin"), &a);
        // Same name, different content.
        write(&r.join("p/report.txt"), &bytes(2, 5000));
        write(&r.join("q/report.txt"), &bytes(3, 5000));
        // Same size and same ends; only the middle differs.
        let mut m1 = bytes(4, 400_000);
        let mut m2 = m1.clone();
        m1[200_000] = 1;
        m2[200_000] = 2;
        write(&r.join("m1.bin"), &m1);
        write(&r.join("m2.bin"), &m2);
        // A pair of small identical files.
        write(&r.join("s1"), b"hello");
        write(&r.join("s2"), b"hello");
        // Hard links are one file.
        write(&r.join("h/orig"), &bytes(5, 70_000));
        std::fs::hard_link(r.join("h/orig"), r.join("h/link")).unwrap();
        // Excluded folders.
        write(&r.join("node_modules/a.dat"), &a);
        write(&r.join(".git/objects/a.dat"), &a);
        write(&r.join(".cache/a.dat"), &a);
        // Empty files never match.
        write(&r.join("e1"), b"");
        write(&r.join("e2"), b"");

        let (_, s) = run(r, &opts(), None);
        assert_eq!(
            group_names(&s),
            vec![
                vec![
                    "a-copy.dat".to_string(),
                    "a.dat".into(),
                    "renamed.bin".into()
                ],
                vec!["s1".to_string(), "s2".into()],
            ]
        );
        assert_eq!(s.summary.groups, 2);
    }

    #[test]
    fn app_data_is_opt_in() {
        let e = env();
        let d = bytes(4, 3000);
        write(&e.root.join(".cache/a"), &d);
        write(&e.root.join(".cache/b"), &d);
        let (_, s) = run(&e.root, &opts(), None);
        assert!(s.groups.is_empty(), "hidden folders are skipped by default");
        let (_, s) = run(
            &e.root,
            &DupOptions {
                include_app_data: true,
                ..opts()
            },
            None,
        );
        assert_eq!(s.groups.len(), 1);
    }

    #[test]
    fn minimum_size_and_kind_filters() {
        let e = env();
        write(&e.root.join("a.mov"), &bytes(1, 2000));
        write(&e.root.join("b.mov"), &bytes(1, 2000));
        write(&e.root.join("a.txt"), &bytes(9, 2000));
        write(&e.root.join("b.txt"), &bytes(9, 2000));
        let (_, s) = run(
            &e.root,
            &DupOptions {
                min_size: 5000,
                ..opts()
            },
            None,
        );
        assert_eq!(s.groups.len(), 0);
        let (_, s) = run(
            &e.root,
            &DupOptions {
                kinds: vec![FileKind::Video],
                ..opts()
            },
            None,
        );
        assert_eq!(
            group_names(&s),
            vec![vec!["a.mov".to_string(), "b.mov".into()]]
        );
    }

    #[test]
    fn file_changed_mid_hash_is_dropped() {
        let e = env();
        let d = bytes(7, 200_000);
        write(&e.root.join("a"), &d);
        write(&e.root.join("b"), &d);
        write(&e.root.join("c"), &d);
        let target = e.root.join("b");
        let hook = move |p: &Path| {
            if p == target {
                std::fs::OpenOptions::new()
                    .append(true)
                    .open(p)
                    .unwrap()
                    .write_all(b"!")
                    .unwrap();
            }
        };
        let (_, s) = run(&e.root, &opts(), Some(&hook));
        assert_eq!(group_names(&s), vec![vec!["a".to_string(), "c".into()]]);
        assert!(s
            .summary
            .skipped
            .iter()
            .any(|x| x.reason.contains("changed")));
    }

    #[test]
    fn keep_rules_and_last_copy_is_protected() {
        let e = env();
        let d = bytes(3, 50_000);
        write(&e.root.join("deep/er/one.bin"), &d);
        write(&e.root.join("two.bin"), &d);
        write(&e.root.join("keepme/three.bin"), &d);
        let (_, mut s) = run(&e.root, &opts(), None);
        let gid = s.groups[0].id;

        s.apply_rule(KeepRule::ShortestPath, None, None);
        let kept: Vec<_> = s.groups[0].files.iter().filter(|f| !f.selected).collect();
        assert_eq!(kept.len(), 1);
        assert!(kept[0].cand.path.ends_with("two.bin"));

        s.apply_rule(
            KeepRule::PreferredFolder,
            Some(&e.root.join("keepme")),
            None,
        );
        let kept: Vec<_> = s.groups[0].files.iter().filter(|f| !f.selected).collect();
        assert!(kept[0].cand.path.ends_with("keepme/three.bin"));

        // Selecting the kept copy too would leave nothing: refused.
        let last = kept[0].cand.node;
        assert!(s.set_selected(gid, last, true).is_err());
        assert_eq!(s.savings().files, 2);

        s.apply_rule(KeepRule::Manual, None, None);
        assert_eq!(s.savings().files, 0);
    }

    #[test]
    fn verify_and_trash_then_forget() {
        let e = env();
        let d = bytes(8, 120_000);
        write(&e.root.join("a.bin"), &d);
        write(&e.root.join("b.bin"), &d);
        write(&e.root.join("c.bin"), &d);
        let (_, mut s) = run(&e.root, &opts(), None);
        s.apply_rule(KeepRule::ShortestPath, None, None);
        // b changes after the scan: it must be skipped, the others move.
        let mut changed = d.clone();
        changed[5] ^= 1;
        write(&e.root.join("c.bin"), &changed);
        let v = s.verify(&e.ctx);
        assert_eq!(v.ok.len(), 1);
        assert_eq!(v.skipped.len(), 1);
        let (out, moved) = trash_verified(v, &e.ctx, &e.trash);
        assert_eq!(out.moved.len(), 1);
        assert!(
            e.root.join("a.bin").exists(),
            "the kept copy is never moved"
        );
        s.forget(&moved.into_iter().collect());
        assert_eq!(s.groups[0].files.len(), 2);
    }

    #[test]
    fn missing_kept_copy_skips_the_whole_group() {
        let e = env();
        let d = bytes(6, 80_000);
        write(&e.root.join("a.bin"), &d);
        write(&e.root.join("bb.bin"), &d);
        let (_, mut s) = run(&e.root, &opts(), None);
        s.apply_rule(KeepRule::ShortestPath, None, None);
        std::fs::remove_file(e.root.join("a.bin")).unwrap();
        let v = s.verify(&e.ctx);
        assert!(v.ok.is_empty());
        assert!(v.skipped[0].2.contains("copy you keep"));
        assert!(e.root.join("bb.bin").exists());
    }

    #[test]
    fn clone_replacement_keeps_paths_and_trashes_original() {
        let e = env();
        let d = bytes(2, 2_000_000);
        write(&e.root.join("a.bin"), &d);
        write(&e.root.join("b.bin"), &d);
        let (_, mut s) = run(&e.root, &opts(), None);
        s.apply_rule(KeepRule::ShortestPath, None, None);
        let v = s.verify(&e.ctx);
        let (out, done) = replace_with_clones(&s, v, &e.ctx, &e.trash);
        assert_eq!(done.len(), 1, "{:?}", out.skipped);
        assert_eq!(std::fs::read(e.root.join("b.bin")).unwrap(), d);
        assert!(
            out.moved[0].trashed.exists(),
            "original is in the Trash for Undo"
        );
        // The clone shares blocks with the kept copy.
        assert!(platform::private_size(&e.root.join("b.bin")).unwrap() < 500_000);
        let leftovers = std::fs::read_dir(&e.root)
            .unwrap()
            .flatten()
            .filter(|x| {
                x.file_name()
                    .to_string_lossy()
                    .starts_with(".duobolt-clone")
            })
            .count();
        assert_eq!(leftovers, 0);
    }

    #[test]
    fn large_sparse_files_hash_with_fixed_buffer() {
        let e = env();
        for n in ["a", "b"] {
            let f = std::fs::File::create(e.root.join(n)).unwrap();
            f.set_len(256 * 1024 * 1024).unwrap();
        }
        let (_, s) = run(&e.root, &opts(), None);
        assert_eq!(s.groups.len(), 1);
        assert_eq!(s.groups[0].size, 256 * 1024 * 1024);
    }

    #[test]
    fn cancellation_stops_and_reports() {
        let e = env();
        write(&e.root.join("a"), &bytes(1, 10_000));
        write(&e.root.join("b"), &bytes(1, 10_000));
        let cancel = AtomicBool::new(true);
        let out = scan(
            &StdReader,
            std::slice::from_ref(&e.root),
            &Default::default(),
            &AtomicBool::new(false),
            &mut |_| {},
        )
        .unwrap();
        let r = find_duplicates(candidates(&out.tree, &opts()), &cancel, &|_| {}, None);
        assert!(r.cancelled);
        assert!(r.groups.is_empty());
    }

    #[test]
    fn same_bytes_detects_any_difference() {
        let e = env();
        let d = bytes(1, 3 * BUF + 17);
        write(&e.root.join("a"), &d);
        write(&e.root.join("b"), &d);
        assert!(same_bytes(&e.root.join("a"), &e.root.join("b")).unwrap());
        let mut d2 = d.clone();
        let last = d2.len() - 1;
        d2[last] ^= 1;
        write(&e.root.join("c"), &d2);
        assert!(!same_bytes(&e.root.join("a"), &e.root.join("c")).unwrap());
    }

    mod props {
        use super::*;
        use proptest::prelude::*;

        fn session(groups: &[usize]) -> Session {
            let mut node = 0;
            let gs = groups
                .iter()
                .enumerate()
                .map(|(gi, n)| SGroup {
                    id: gi as u32,
                    size: 100,
                    kind: FileKind::Other,
                    files: (0..*n)
                        .map(|i| {
                            node += 1;
                            SFile {
                                cand: Cand {
                                    node,
                                    path: PathBuf::from(format!(
                                        "/r/{}{gi}/{i}",
                                        "d/".repeat(i % 3)
                                    )),
                                    size: 100,
                                    mtime: (i * 7 % 5) as i64,
                                    dev: 1,
                                    inode: node as u64,
                                },
                                private_size: 10 + i as u64,
                                selected: false,
                            }
                        })
                        .collect(),
                })
                .collect();
            Session {
                groups: gs,
                summary: DupSummary::default(),
            }
        }

        #[derive(Debug, Clone)]
        enum Op {
            Rule(u8),
            Toggle(usize, usize, bool),
        }

        fn op() -> impl Strategy<Value = Op> {
            prop_oneof![
                (0u8..5).prop_map(Op::Rule),
                (0usize..6, 0usize..6, any::<bool>()).prop_map(|(g, f, on)| Op::Toggle(g, f, on)),
            ]
        }

        proptest! {
            #[test]
            fn never_every_copy_selected(
                sizes in proptest::collection::vec(2usize..6, 1..6),
                ops in proptest::collection::vec(op(), 0..60),
            ) {
                let mut s = session(&sizes);
                for o in ops {
                    match o {
                        Op::Rule(r) => {
                            let rule = [KeepRule::Newest, KeepRule::Oldest, KeepRule::ShortestPath, KeepRule::PreferredFolder, KeepRule::Manual][r as usize];
                            s.apply_rule(rule, Some(Path::new("/r/d")), None);
                        }
                        Op::Toggle(g, f, on) => {
                            if let Some(grp) = s.groups.get(g) {
                                if let Some(file) = grp.files.get(f) {
                                    let (gid, node) = (grp.id, file.cand.node);
                                    let _ = s.set_selected(gid, node, on);
                                }
                            }
                        }
                    }
                    for g in &s.groups {
                        prop_assert!(g.files.iter().any(|f| !f.selected), "group {} fully selected", g.id);
                    }
                    let expect: u64 = s.groups.iter().flat_map(|g| g.files.iter()).filter(|f| f.selected).map(|f| f.private_size).sum();
                    prop_assert_eq!(s.savings().bytes, expect);
                }
            }
        }
    }
}
