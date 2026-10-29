//! Parallel filesystem traversal that builds the in-memory tree (M1).
//!
//! Workers list directories concurrently and send batches to a single collector (the calling
//! thread) which owns the [`Tree`], assigns ids and queues child directories. Symlinks are never
//! followed, other volumes are not entered unless asked, and the firmlinked `/System/Volumes`
//! tree is skipped when scanning `/` so data is counted once.

pub mod bulk;
pub mod mock;
pub mod reader;
pub mod report;
pub mod std_reader;

use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use crossbeam_channel::unbounded;
use globset::{Glob, GlobSet, GlobSetBuilder};

use crate::errors::AppError;
use crate::index::{flags, NewNode, NodeId, NodeKind, Tree, NONE, ROOT};
use crate::models::{ScanOptions, ScanProgress, ScanSummary, SkipReason};
use reader::{st_flags, DirListing, DirReader, RawEntry, ReadError};
use report::ScanReport;

const PROGRESS_INTERVAL: Duration = Duration::from_millis(100);
const MAX_DEFAULT_WORKERS: usize = 8;

/// Directory extensions treated as packages (one row in the UI).
const PACKAGE_EXTS: &[&str] = &[
    "app",
    "photoslibrary",
    "sparsebundle",
    "sparseimage",
    "bundle",
    "framework",
    "plugin",
    "kext",
    "xcarchive",
    "imovielibrary",
    "musiclibrary",
    "tvlibrary",
    "fcpbundle",
];

/// Resolved, immutable settings for one scan; reused by subtree rescans.
#[derive(Debug)]
pub struct WalkPolicy {
    allowed_devs: Vec<u32>,
    skip_prefixes: Vec<PathBuf>,
    include_hidden: bool,
    cross_volumes: bool,
    exclude: Option<GlobSet>,
    workers: usize,
}

impl WalkPolicy {
    pub fn new(
        reader: &dyn DirReader,
        roots: &[PathBuf],
        opts: &ScanOptions,
    ) -> Result<Self, AppError> {
        let mut allowed_devs = Vec::new();
        let mut skip_prefixes = Vec::new();
        for root in roots {
            if let Ok(e) = reader.stat(root) {
                allowed_devs.push(e.dev);
            }
            if root == Path::new("/") {
                // Firmlinks make /Users etc. live on the Data volume; allow it, and skip the
                // direct /System/Volumes/* mounts so nothing is counted twice.
                if let Ok(e) = reader.stat(Path::new("/System/Volumes/Data")) {
                    allowed_devs.push(e.dev);
                }
                skip_prefixes.push(PathBuf::from("/System/Volumes"));
            }
        }
        allowed_devs.sort_unstable();
        allowed_devs.dedup();

        let exclude = if opts.exclude_globs.is_empty() {
            None
        } else {
            let mut b = GlobSetBuilder::new();
            for g in &opts.exclude_globs {
                let glob = Glob::new(g)
                    .map_err(|e| AppError::InvalidInput(format!("bad glob '{g}': {e}")))?;
                b.add(glob);
            }
            Some(
                b.build()
                    .map_err(|e| AppError::InvalidInput(e.to_string()))?,
            )
        };
        let workers = opts
            .workers
            .map(|w| w as usize)
            .unwrap_or_else(|| {
                std::thread::available_parallelism()
                    .map(|n| n.get())
                    .unwrap_or(4)
                    .min(MAX_DEFAULT_WORKERS)
            })
            .clamp(1, 64);
        Ok(Self {
            allowed_devs,
            skip_prefixes,
            include_hidden: opts.include_hidden,
            cross_volumes: opts.cross_volumes,
            exclude,
            workers,
        })
    }
}

/// Everything a finished (or cancelled) scan produces.
#[derive(Debug)]
pub struct ScanOutput {
    pub tree: Tree,
    pub report: ScanReport,
    pub summary: ScanSummary,
    pub policy: WalkPolicy,
}

#[derive(Default)]
struct Counters {
    files: u64,
    dirs: u64,
    bytes: u64,
}

struct Job {
    id: NodeId,
    path: PathBuf,
}

struct Batch {
    id: NodeId,
    path: PathBuf,
    listing: Result<DirListing, ReadError>,
}

fn lossy(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

fn reason_of(e: &ReadError) -> SkipReason {
    match e {
        ReadError::PermissionDenied => SkipReason::PermissionDenied,
        ReadError::Vanished => SkipReason::Vanished,
        ReadError::Io(_) => SkipReason::IoError,
    }
}

fn has_package_ext(name: &[u8]) -> bool {
    let name = String::from_utf8_lossy(name).to_lowercase();
    Path::new(&name)
        .extension()
        .and_then(OsStr::to_str)
        .is_some_and(|ext| PACKAGE_EXTS.contains(&ext))
}

/// Scan `roots`. Cancelling keeps whatever was collected so far.
pub fn scan(
    reader: &dyn DirReader,
    roots: &[PathBuf],
    opts: &ScanOptions,
    cancel: &AtomicBool,
    sink: &mut dyn FnMut(ScanProgress),
) -> Result<ScanOutput, AppError> {
    if roots.is_empty() {
        return Err(AppError::InvalidInput("no scan roots".into()));
    }
    let policy = WalkPolicy::new(reader, roots, opts)?;
    let start = Instant::now();
    let mut tree = Tree::new();
    let mut report = ScanReport::default();
    let mut counters = Counters::default();
    let mut jobs = Vec::new();

    for root in roots {
        match reader.stat(root) {
            Ok(mut e) => {
                e.name = root.as_os_str().as_bytes().to_vec();
                insert(
                    &mut tree,
                    &mut report,
                    &policy,
                    &mut counters,
                    &mut jobs,
                    ROOT,
                    Path::new(""),
                    e,
                    cancel,
                );
            }
            Err(err) => report.push(lossy(root), reason_of(&err)),
        }
    }

    walk(
        reader,
        &mut tree,
        &mut report,
        &policy,
        jobs,
        cancel,
        sink,
        &mut counters,
        start,
    );
    tree.aggregate();
    let summary = summarize(&tree, &report, cancel, start);
    Ok(ScanOutput {
        tree,
        report,
        summary,
        policy,
    })
}

/// Re-walk one directory in place. Other node ids stay valid.
pub fn rescan_subtree(
    reader: &dyn DirReader,
    out: &mut ScanOutput,
    node: NodeId,
    cancel: &AtomicBool,
    sink: &mut dyn FnMut(ScanProgress),
) -> Result<(), AppError> {
    let n = out
        .tree
        .node(node)
        .ok_or_else(|| AppError::NotFound(format!("node {node}")))?;
    if node == ROOT || !n.is_dir() {
        return Err(AppError::InvalidInput("not a rescannable directory".into()));
    }
    let start = Instant::now();
    let path = out.tree.path(node);
    out.report.remove_under(&lossy(&path));
    out.tree.clear_children(node);
    if let Some(n) = out.tree.node_mut(node) {
        n.flags &= !flags::UNREADABLE;
    }

    match reader.stat(&path) {
        Err(ReadError::Vanished) => {
            if let Some(n) = out.tree.node_mut(node) {
                n.flags |= flags::DELETED;
            }
        }
        Err(e) => {
            if let Some(n) = out.tree.node_mut(node) {
                n.flags |= flags::UNREADABLE;
            }
            out.report.push(lossy(&path), reason_of(&e));
        }
        Ok(_) => {
            let mut counters = Counters::default();
            walk(
                reader,
                &mut out.tree,
                &mut out.report,
                &out.policy,
                vec![Job { id: node, path }],
                cancel,
                sink,
                &mut counters,
                start,
            );
        }
    }
    out.tree.aggregate();
    out.summary = summarize(&out.tree, &out.report, cancel, start);
    Ok(())
}

fn summarize(tree: &Tree, report: &ScanReport, cancel: &AtomicBool, start: Instant) -> ScanSummary {
    let (files, dirs) = tree.live_counts();
    let root = tree.node(ROOT);
    ScanSummary {
        files,
        dirs,
        total_alloc: root.map_or(0, |n| n.total_alloc),
        total_logical: root.map_or(0, |n| n.total_logical),
        skipped: report.len() as u64,
        cancelled: cancel.load(Ordering::Relaxed),
        elapsed_ms: start.elapsed().as_millis() as u64,
    }
}

#[allow(clippy::too_many_arguments)]
fn walk(
    reader: &dyn DirReader,
    tree: &mut Tree,
    report: &mut ScanReport,
    policy: &WalkPolicy,
    initial: Vec<Job>,
    cancel: &AtomicBool,
    sink: &mut dyn FnMut(ScanProgress),
    counters: &mut Counters,
    start: Instant,
) {
    if initial.is_empty() {
        return;
    }
    let (job_tx, job_rx) = unbounded::<Job>();
    let (res_tx, res_rx) = unbounded::<Batch>();
    let mut pending = 0usize;
    for j in initial {
        pending += 1;
        let _ = job_tx.send(j);
    }

    std::thread::scope(|s| {
        for _ in 0..policy.workers {
            let rx = job_rx.clone();
            let tx = res_tx.clone();
            s.spawn(move || {
                while let Ok(job) = rx.recv() {
                    let listing = if cancel.load(Ordering::Relaxed) {
                        Ok(DirListing::default())
                    } else {
                        reader.read_dir(&job.path)
                    };
                    let batch = Batch {
                        id: job.id,
                        path: job.path,
                        listing,
                    };
                    if tx.send(batch).is_err() {
                        break;
                    }
                }
            });
        }
        drop(res_tx);
        drop(job_rx);

        let mut last_emit = Instant::now();
        let mut child_jobs = Vec::new();
        while pending > 0 {
            // Errors only if every worker died; stop instead of hanging.
            let Ok(batch) = res_rx.recv() else { break };
            pending -= 1;
            let current = batch.path.clone();
            handle_batch(
                tree,
                report,
                policy,
                counters,
                batch,
                &mut child_jobs,
                cancel,
            );
            for j in child_jobs.drain(..) {
                pending += 1;
                let _ = job_tx.send(j);
            }
            if last_emit.elapsed() >= PROGRESS_INTERVAL {
                last_emit = Instant::now();
                let elapsed = start.elapsed();
                let secs = elapsed.as_secs_f64().max(0.001);
                sink(ScanProgress {
                    files: counters.files,
                    dirs: counters.dirs,
                    bytes: counters.bytes,
                    current_path: lossy(&current),
                    elapsed_ms: elapsed.as_millis() as u64,
                    files_per_sec: (counters.files as f64 / secs) as u64,
                });
            }
        }
        drop(job_tx);
    });
}

fn handle_batch(
    tree: &mut Tree,
    report: &mut ScanReport,
    policy: &WalkPolicy,
    counters: &mut Counters,
    batch: Batch,
    jobs: &mut Vec<Job>,
    cancel: &AtomicBool,
) {
    let Batch { id, path, listing } = batch;
    match listing {
        Err(e) => {
            if let Some(n) = tree.node_mut(id) {
                n.flags |= flags::UNREADABLE;
            }
            report.push(lossy(&path), reason_of(&e));
        }
        Ok(l) => {
            for (name, e) in &l.errors {
                let p = if name.is_empty() {
                    path.clone()
                } else {
                    path.join(OsStr::from_bytes(name))
                };
                report.push(lossy(&p), reason_of(e));
            }
            for e in l.entries {
                insert(tree, report, policy, counters, jobs, id, &path, e, cancel);
            }
        }
    }
}

/// Add one entry under `parent`, applying hidden/exclude/volume rules.
#[allow(clippy::too_many_arguments)]
fn insert(
    tree: &mut Tree,
    report: &mut ScanReport,
    policy: &WalkPolicy,
    counters: &mut Counters,
    jobs: &mut Vec<Job>,
    parent: NodeId,
    parent_path: &Path,
    e: RawEntry,
    cancel: &AtomicBool,
) {
    let hidden = e.name.first() == Some(&b'.') || e.st_flags & st_flags::UF_HIDDEN != 0;
    if hidden && !policy.include_hidden {
        return;
    }
    let dataless = e.st_flags & st_flags::SF_DATALESS != 0;
    let is_dir = e.kind == NodeKind::Dir;
    let need_path =
        is_dir || dataless || policy.exclude.is_some() || !policy.skip_prefixes.is_empty();
    let child_path = need_path.then(|| parent_path.join(OsStr::from_bytes(&e.name)));

    if let (Some(set), Some(p)) = (&policy.exclude, &child_path) {
        if set.is_match(p) {
            report.push(lossy(p), SkipReason::Excluded);
            return;
        }
    }

    let mut f = 0u8;
    if hidden {
        f |= flags::HIDDEN;
    }
    if e.st_flags & st_flags::UF_COMPRESSED != 0 {
        f |= flags::COMPRESSED;
    }
    if dataless {
        f |= flags::DATALESS;
    }
    if is_dir && has_package_ext(&e.name) {
        f |= flags::PACKAGE;
    }
    if e.kind == NodeKind::File && e.nlink > 1 {
        f |= flags::MULTILINK;
    }

    let Some(id) = tree.add(
        parent,
        NewNode {
            name: &e.name,
            kind: e.kind,
            flags: f,
            dev: e.dev,
            inode: e.inode,
            mtime: e.mtime,
            atime: e.atime,
            size_logical: e.size_logical,
            size_alloc: e.size_alloc,
        },
    ) else {
        if let Some(p) = &child_path {
            report.push(lossy(p), SkipReason::IoError);
        }
        return;
    };

    if is_dir {
        counters.dirs += 1;
    } else {
        counters.files += 1;
    }
    if dataless {
        if let Some(p) = &child_path {
            report.push(lossy(p), SkipReason::Dataless);
        }
    } else {
        counters.bytes += e.size_alloc;
    }

    if is_dir {
        let Some(p) = child_path else { return };
        if cancel.load(Ordering::Relaxed) {
            return;
        }
        if policy.skip_prefixes.iter().any(|pre| p.starts_with(pre))
            || (!policy.cross_volumes && !policy.allowed_devs.contains(&e.dev))
        {
            report.push(lossy(&p), SkipReason::Excluded);
            return;
        }
        jobs.push(Job { id, path: p });
    }
    debug_assert!(id != NONE);
}

#[cfg(test)]
mod tests {
    use super::mock::MockFs;
    use super::*;
    use crate::index::Node;
    use std::sync::Arc;

    fn run(fs: &MockFs, roots: &[&str], opts: &ScanOptions) -> ScanOutput {
        let roots: Vec<PathBuf> = roots.iter().map(PathBuf::from).collect();
        let cancel = AtomicBool::new(false);
        scan(fs, &roots, opts, &cancel, &mut |_| {}).unwrap()
    }

    fn find(out: &ScanOutput, path: &str) -> Option<(NodeId, Node)> {
        let want = PathBuf::from(path);
        (1..out.tree.len() as NodeId)
            .find(|id| {
                out.tree.path(*id) == want && !out.tree.node(*id).unwrap().has(flags::DELETED)
            })
            .map(|id| (id, out.tree.node(id).unwrap().clone()))
    }

    #[test]
    fn aggregates_mock_tree() {
        let mut fs = MockFs::new();
        fs.add_dir("/r", 1)
            .add_dir("/r/d", 1)
            .add_file("/r/d/a", 4096, 1)
            .add_file("/r/b", 100, 1);
        let out = run(&fs, &["/r"], &ScanOptions::default());
        assert_eq!(out.summary.files, 2);
        assert_eq!(out.summary.dirs, 2);
        assert_eq!(out.summary.total_alloc, 4096 + 4096);
        let (_, d) = find(&out, "/r/d").unwrap();
        assert_eq!(d.total_alloc, 4096);
    }

    #[test]
    fn hard_links_counted_once() {
        let mut fs = MockFs::new();
        fs.add_dir("/r", 1)
            .add_hardlink("/r/a", 8192, 1, 777)
            .add_hardlink("/r/b", 8192, 1, 777);
        let out = run(&fs, &["/r"], &ScanOptions::default());
        assert_eq!(out.summary.files, 2);
        assert_eq!(out.summary.total_alloc, 8192);
    }

    #[test]
    fn symlinks_are_recorded_not_followed() {
        let mut fs = MockFs::new();
        fs.add_dir("/r", 1).add_symlink("/r/loop");
        let out = run(&fs, &["/r"], &ScanOptions::default());
        let (id, n) = find(&out, "/r/loop").unwrap();
        assert_eq!(n.kind, NodeKind::Symlink);
        assert_eq!(out.tree.children(id).count(), 0);
    }

    #[test]
    fn unreadable_dir_is_reported_and_flagged() {
        let mut fs = MockFs::new();
        fs.add_dir("/r", 1)
            .add_dir("/r/locked", 1)
            .fail_dir("/r/locked", ReadError::PermissionDenied)
            .add_dir("/r/gone", 1)
            .fail_dir("/r/gone", ReadError::Vanished);
        let out = run(&fs, &["/r"], &ScanOptions::default());
        assert_eq!(out.report.count(SkipReason::PermissionDenied), 1);
        assert_eq!(out.report.count(SkipReason::Vanished), 1);
        let (_, n) = find(&out, "/r/locked").unwrap();
        assert!(n.has(flags::UNREADABLE));
    }

    #[test]
    fn other_volumes_not_entered_by_default() {
        let mut fs = MockFs::new();
        fs.add_dir("/r", 1)
            .add_dir("/r/mnt", 9)
            .add_file("/r/mnt/big", 1 << 20, 9);
        let out = run(&fs, &["/r"], &ScanOptions::default());
        assert!(find(&out, "/r/mnt/big").is_none());
        assert_eq!(out.report.count(SkipReason::Excluded), 1);

        let opts = ScanOptions {
            cross_volumes: true,
            ..ScanOptions::default()
        };
        let out = run(&fs, &["/r"], &opts);
        assert!(find(&out, "/r/mnt/big").is_some());
    }

    #[test]
    fn firmlinked_data_volume_is_not_double_counted() {
        let mut fs = MockFs::new();
        fs.add_dir("/", 1)
            .add_dir("/Users", 2) // firmlink: lives on the Data volume
            .add_file("/Users/a.bin", 4096, 2)
            .add_dir("/System", 1)
            .add_dir("/System/Volumes", 1)
            .add_dir("/System/Volumes/Data", 2)
            .add_dir("/System/Volumes/Data/Users", 2)
            .add_file("/System/Volumes/Data/Users/a.bin", 4096, 2);
        let out = run(&fs, &["/"], &ScanOptions::default());
        assert!(
            find(&out, "/Users/a.bin").is_some(),
            "firmlink target must be scanned"
        );
        assert!(find(&out, "/System/Volumes/Data/Users/a.bin").is_none());
        assert_eq!(out.summary.files, 1);
        assert_eq!(out.summary.total_alloc, 4096);
    }

    #[test]
    fn hidden_toggle_and_globs() {
        let mut fs = MockFs::new();
        fs.add_dir("/r", 1)
            .add_file("/r/.secret", 10, 1)
            .add_file("/r/visible", 10, 1)
            .add_dir("/r/node_modules", 1)
            .add_file("/r/node_modules/x", 10, 1);
        let out = run(&fs, &["/r"], &ScanOptions::default());
        assert!(find(&out, "/r/.secret").is_some());

        let opts = ScanOptions {
            include_hidden: false,
            exclude_globs: vec!["**/node_modules".into()],
            ..ScanOptions::default()
        };
        let out = run(&fs, &["/r"], &opts);
        assert!(find(&out, "/r/.secret").is_none());
        assert!(find(&out, "/r/visible").is_some());
        assert!(find(&out, "/r/node_modules").is_none());
        assert_eq!(out.report.count(SkipReason::Excluded), 1);
    }

    #[test]
    fn bad_glob_is_invalid_input() {
        let fs = MockFs::new();
        let opts = ScanOptions {
            exclude_globs: vec!["[".into()],
            ..ScanOptions::default()
        };
        let cancel = AtomicBool::new(false);
        let r = scan(&fs, &[PathBuf::from("/r")], &opts, &cancel, &mut |_| {});
        assert!(matches!(r, Err(AppError::InvalidInput(_))));
    }

    #[test]
    fn unicode_nfc_and_nfd_names_stay_distinct() {
        let mut fs = MockFs::new();
        fs.add_dir("/r", 1)
            .add_file("/r/caf\u{e9}", 1, 1)
            .add_file("/r/cafe\u{301}", 1, 1);
        let out = run(&fs, &["/r"], &ScanOptions::default());
        assert_eq!(out.summary.files, 2);
        assert!(find(&out, "/r/caf\u{e9}").is_some());
        assert!(find(&out, "/r/cafe\u{301}").is_some());
    }

    #[test]
    fn rescan_replaces_subtree_and_keeps_other_ids() {
        let mut before = MockFs::new();
        before
            .add_dir("/r", 1)
            .add_dir("/r/d", 1)
            .add_file("/r/d/old", 4096, 1)
            .add_file("/r/keep", 4096, 1);
        let mut out = run(&before, &["/r"], &ScanOptions::default());
        let (d, _) = find(&out, "/r/d").unwrap();
        let (keep, _) = find(&out, "/r/keep").unwrap();

        let mut after = MockFs::new();
        after
            .add_dir("/r", 1)
            .add_dir("/r/d", 1)
            .add_file("/r/d/new1", 4096, 1)
            .add_file("/r/d/new2", 8192, 1)
            .add_file("/r/keep", 4096, 1);
        let cancel = AtomicBool::new(false);
        rescan_subtree(&after, &mut out, d, &cancel, &mut |_| {}).unwrap();

        assert!(find(&out, "/r/d/old").is_none());
        assert!(find(&out, "/r/d/new2").is_some());
        assert_eq!(out.tree.path(keep), PathBuf::from("/r/keep"));
        assert_eq!(out.summary.files, 3);
        assert_eq!(out.tree.node(d).unwrap().total_alloc, 4096 + 8192);
    }

    #[test]
    fn rescan_of_vanished_dir_removes_it() {
        let mut before = MockFs::new();
        before
            .add_dir("/r", 1)
            .add_dir("/r/d", 1)
            .add_file("/r/d/a", 4096, 1);
        let mut out = run(&before, &["/r"], &ScanOptions::default());
        let (d, _) = find(&out, "/r/d").unwrap();
        let mut after = MockFs::new();
        after.add_dir("/r", 1);
        let cancel = AtomicBool::new(false);
        rescan_subtree(&after, &mut out, d, &cancel, &mut |_| {}).unwrap();
        assert!(find(&out, "/r/d").is_none());
        assert_eq!(out.summary.files, 0);
    }

    #[test]
    fn cancellation_returns_quickly_with_partial_tree() {
        let mut fs = MockFs::synthetic("/big", 20_000, 10);
        fs.delay = Duration::from_millis(5);
        let fs = Arc::new(fs);
        let cancel = Arc::new(AtomicBool::new(false));
        let c2 = cancel.clone();
        let canceller = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(100));
            c2.store(true, Ordering::Relaxed);
            Instant::now()
        });
        let out = scan(
            &*fs,
            &[PathBuf::from("/big")],
            &ScanOptions::default(),
            &cancel,
            &mut |_| {},
        )
        .unwrap();
        let done = Instant::now();
        let cancelled_at = canceller.join().unwrap();
        assert!(out.summary.cancelled);
        assert!(done.duration_since(cancelled_at) < Duration::from_millis(500));
        assert!(out.summary.files > 0, "partial results kept");
        assert!(out.summary.files < 20_000, "scan must have stopped early");
    }

    #[test]
    fn progress_is_throttled_and_monotonic() {
        let mut fs = MockFs::synthetic("/p", 2_000, 5);
        fs.delay = Duration::from_millis(20);
        let cancel = AtomicBool::new(false);
        let mut seen: Vec<ScanProgress> = Vec::new();
        let start = Instant::now();
        scan(
            &fs,
            &[PathBuf::from("/p")],
            &ScanOptions::default(),
            &cancel,
            &mut |p| seen.push(p),
        )
        .unwrap();
        let max = start.elapsed().as_millis() as usize / 100 + 1;
        assert!(!seen.is_empty());
        assert!(
            seen.len() <= max,
            "{} events in {} ms",
            seen.len(),
            max * 100
        );
        assert!(seen.windows(2).all(|w| w[0].files <= w[1].files));
    }
}
