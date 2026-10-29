//! Overview: the storage bar and category totals (M2).
//!
//! Category totals come from the cleanup rule engine (`core::cleanup`), so Overview and Cleanup
//! share one source of truth.

use std::collections::HashSet;
use std::path::Path;

use crate::cleanup;
use crate::index::{flags, NodeId, NodeKind, Tree, ROOT};
use crate::models::{
    CategoryId, CategorySummary, Crumb, OverviewSummary, Risk, ScanSummary, VolumeInfo,
};
use crate::space::display_name;

const DAY: i64 = 86_400;
const LARGE_OLD_MIN: u64 = 500 * 1_000_000;

/// Locations (relative to the home folder) holding developer build output and package caches.
pub const DEVELOPER_PATHS: &[&str] = &[
    "Library/Developer/Xcode/DerivedData",
    ".npm/_cacache",
    ".gradle/caches",
    "Library/Caches/Homebrew",
    "Library/Caches/CocoaPods",
    ".cargo/registry",
];

/// Plain-language hint for a path, when it matches a well-known location.
pub fn describe(path: &Path, home: &Path) -> Option<String> {
    let rel = path.strip_prefix(home).ok()?;
    let in_dir = |p: &str| rel.starts_with(p);
    if DEVELOPER_PATHS.iter().any(|p| in_dir(p)) {
        return Some(
            "Developer cache or build output. Tools recreate it when you build again.".into(),
        );
    }
    if in_dir("Library/Caches") {
        return Some("App cache. Apps rebuild these files when they need them.".into());
    }
    if in_dir("Library/Logs") {
        return Some("Log or crash report. Safe to remove; new logs are written as needed.".into());
    }
    if in_dir("Downloads") {
        return Some("In your Downloads folder.".into());
    }
    if in_dir(".Trash") {
        return Some("In the Trash. Emptying the Trash frees this space for good.".into());
    }
    None
}

/// Fill in the Duplicates category once a duplicate scan has run, keeping totals consistent.
pub fn set_duplicates(o: &mut OverviewSummary, bytes: u64, copies: u32) {
    if let Some(c) = o
        .categories
        .iter_mut()
        .find(|c| c.id == CategoryId::Duplicates)
    {
        c.bytes = bytes;
        c.items = copies;
        c.analyzed = true;
        c.blurb = "Extra copies of identical files. One copy of each is always kept.".into();
    }
    let clearable: u64 = o
        .categories
        .iter()
        .filter(|c| c.analyzed)
        .map(|c| c.bytes)
        .sum();
    o.kept_bytes = o.scanned_bytes.saturating_sub(clearable);
}

/// The storage bar and category cards. Category totals come from the cleanup analysis, so the
/// Overview and Cleanup always agree; "Large old files" covers what no cleanup item does.
#[allow(clippy::too_many_arguments)]
pub fn summarize(
    tree: &Tree,
    scan: &ScanSummary,
    roots: &[String],
    volume: VolumeInfo,
    now: i64,
    analysis: &cleanup::Analysis,
    rules: &cleanup::Rules,
) -> OverviewSummary {
    let scanned = tree.node(ROOT).map_or(0, |n| n.total_alloc);
    let totals = analysis.totals(rules);
    let claimed: HashSet<NodeId> = analysis.items.iter().map(|i| i.node).collect();
    let inside_claimed = |id: NodeId| {
        let mut cur = id;
        loop {
            if claimed.contains(&cur) {
                return true;
            }
            match tree.node(cur) {
                Some(n) if n.parent != cur && tree.node(n.parent).is_some() => cur = n.parent,
                _ => return false,
            }
        }
    };

    // Large, old files that no cleanup item already covers.
    let (mut large_bytes, mut large_items) = (0u64, 0u32);
    for id in tree.ids() {
        let Some(n) = tree.node(id) else { continue };
        if n.kind != NodeKind::File
            || n.has(flags::DELETED | flags::DATALESS | flags::HARDLINK_DUP)
            || n.total_alloc < LARGE_OLD_MIN
            || now.saturating_sub(n.mtime) < 365 * DAY
            || tree.has_flagged_ancestor(id, flags::PACKAGE)
            || inside_claimed(id)
        {
            continue;
        }
        large_bytes += n.total_alloc;
        large_items += 1;
    }

    let mut categories: Vec<CategorySummary> = cleanup::CATEGORIES
        .iter()
        .map(|(id, name, blurb, risk)| {
            let (bytes, items) = totals.get(id).copied().unwrap_or_default();
            CategorySummary {
                id: *id,
                name: (*name).to_string(),
                blurb: (*blurb).to_string(),
                bytes,
                items,
                risk: *risk,
                analyzed: true,
            }
        })
        .collect();
    categories.push(CategorySummary {
        id: CategoryId::LargeOld,
        name: "Large old files".into(),
        blurb: "Files over 500 MB not modified for a year.".into(),
        bytes: large_bytes,
        items: large_items,
        risk: Risk::Review,
        analyzed: true,
    });
    categories.push(CategorySummary {
        id: CategoryId::Duplicates,
        name: "Duplicates".into(),
        blurb: "Exact copies of the same file. Run a duplicate scan to count them.".into(),
        bytes: 0,
        items: 0,
        risk: Risk::Review,
        analyzed: false,
    });

    let clearable: u64 = categories
        .iter()
        .filter(|c| c.analyzed)
        .map(|c| c.bytes)
        .sum();
    let used = volume.total.saturating_sub(volume.available_important);

    let trash_bytes = analysis.trash_bytes;

    let (largest_folder, largest_folder_bytes) = tree
        .children(ROOT)
        .flat_map(|r| tree.children(r))
        .filter_map(|c| tree.node(c).map(|n| (c, n)))
        .filter(|(_, n)| n.is_dir())
        .max_by_key(|(_, n)| n.total_alloc)
        .map(|(c, n)| {
            (
                Some(Crumb {
                    id: c,
                    name: display_name(tree, c),
                }),
                n.total_alloc,
            )
        })
        .unwrap_or((None, 0));

    OverviewSummary {
        volume,
        roots: roots.to_vec(),
        covers_volume: roots.iter().any(|r| r == "/"),
        scanned_bytes: scanned,
        categories,
        kept_bytes: scanned.saturating_sub(clearable),
        system_other_bytes: used.saturating_sub(scanned),
        trash_bytes,
        files: scan.files,
        dirs: scan.dirs,
        skipped: scan.skipped,
        elapsed_ms: scan.elapsed_ms,
        largest_folder,
        largest_folder_bytes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::NewNode;
    use std::path::PathBuf;

    fn add(
        t: &mut Tree,
        parent: NodeId,
        name: &str,
        kind: NodeKind,
        size: u64,
        mtime: i64,
    ) -> NodeId {
        t.add(
            parent,
            NewNode {
                name: name.as_bytes(),
                kind,
                flags: 0,
                dev: 1,
                inode: 0,
                mtime,
                atime: 0,
                size_logical: size,
                size_alloc: size,
            },
        )
        .unwrap()
    }

    fn vol() -> VolumeInfo {
        VolumeInfo {
            name: "Test".into(),
            total: 1_000_000_000_000,
            free_raw: 400_000_000_000,
            available_important: 450_000_000_000,
            purgeable: 50_000_000_000,
        }
    }

    fn analysis(t: &Tree, home: &Path, now: i64) -> (cleanup::Analysis, cleanup::Rules) {
        let r = cleanup::Rules::bundled(home).unwrap();
        let a = cleanup::analyze(t, &r, home, now, &[], &Default::default());
        (a, r)
    }

    #[test]
    fn overview_totals_equal_cleanup_totals() {
        let (t, home) = home_tree();
        let (a, r) = analysis(&t, &home, 400 * DAY);
        let o = summarize(
            &t,
            &ScanSummary::default(),
            &["/Users/me".into()],
            vol(),
            400 * DAY,
            &a,
            &r,
        );
        let s = cleanup::Session::new(a, &r, None).summary(&r);
        for c in &s.categories {
            let ov = o.categories.iter().find(|x| x.id == c.id).unwrap();
            assert_eq!((ov.bytes, ov.items), (c.bytes, c.items), "{:?}", c.id);
        }
    }

    fn home_tree() -> (Tree, PathBuf) {
        let mut t = Tree::new();
        let h = add(&mut t, ROOT, "/Users/me", NodeKind::Dir, 0, 0);
        let lib = add(&mut t, h, "Library", NodeKind::Dir, 0, 0);
        let caches = add(&mut t, lib, "Caches", NodeKind::Dir, 0, 0);
        let brew = add(&mut t, caches, "Homebrew", NodeKind::Dir, 0, 0);
        add(&mut t, brew, "bottle", NodeKind::File, 5000, 0);
        let other = add(&mut t, caches, "com.foo", NodeKind::Dir, 0, 0);
        add(&mut t, other, "blob", NodeKind::File, 3000, 0);
        let logs = add(&mut t, lib, "Logs", NodeKind::Dir, 0, 0);
        add(&mut t, logs, "x.log", NodeKind::File, 700, 0);
        let dl = add(&mut t, h, "Downloads", NodeKind::Dir, 0, 0);
        add(&mut t, dl, "old.dmg", NodeKind::File, 9000, 0);
        add(&mut t, dl, "new.zip", NodeKind::File, 2000, 390 * DAY);
        add(&mut t, h, "huge.mov", NodeKind::File, 600_000_000, 0);
        add(&mut t, h, "notes.txt", NodeKind::File, 100, 390 * DAY);
        t.aggregate();
        (t, PathBuf::from("/Users/me"))
    }

    #[test]
    fn categories_do_not_double_count() {
        let (t, home) = home_tree();
        let scan = ScanSummary::default();
        let (a, r) = analysis(&t, &home, 400 * DAY);
        let o = summarize(&t, &scan, &["/Users/me".into()], vol(), 400 * DAY, &a, &r);
        let by = |id| o.categories.iter().find(|c| c.id == id).unwrap();
        assert_eq!(by(CategoryId::Developer).bytes, 5000);
        assert_eq!(
            by(CategoryId::Caches).bytes,
            3000,
            "homebrew is developer, not cache"
        );
        assert_eq!(by(CategoryId::Logs).bytes, 700);
        assert_eq!(by(CategoryId::OldDownloads).bytes, 9000);
        assert_eq!(by(CategoryId::LargeOld).bytes, 600_000_000);
        assert!(!by(CategoryId::Duplicates).analyzed);
        let clearable: u64 = o
            .categories
            .iter()
            .filter(|c| c.analyzed)
            .map(|c| c.bytes)
            .sum();
        assert_eq!(o.kept_bytes + clearable, o.scanned_bytes);
        assert!(clearable <= o.scanned_bytes);
    }

    #[test]
    fn remainder_is_system_and_other() {
        let (t, home) = home_tree();
        let (a, r) = analysis(&t, &home, 0);
        let o = summarize(
            &t,
            &ScanSummary::default(),
            &["/Users/me".into()],
            vol(),
            0,
            &a,
            &r,
        );
        assert_eq!(
            o.system_other_bytes,
            (1_000_000_000_000 - 450_000_000_000) - o.scanned_bytes
        );
        assert!(!o.covers_volume);
        assert_eq!(o.largest_folder.unwrap().name, "Downloads");
    }

    #[test]
    fn describe_known_locations() {
        let home = Path::new("/Users/me");
        assert!(describe(Path::new("/Users/me/Library/Caches/x"), home).is_some());
        assert!(describe(Path::new("/Users/me/.npm/_cacache/a"), home)
            .unwrap()
            .contains("Developer"));
        assert!(describe(Path::new("/Users/me/Documents/a"), home).is_none());
        assert!(describe(Path::new("/elsewhere"), home).is_none());
    }

    #[test]
    fn trash_unreadable_is_none() {
        let (mut t, home) = home_tree();
        let h = t.find_path(&home).unwrap();
        let trash = add(&mut t, h, ".Trash", NodeKind::Dir, 0, 0);
        t.node_mut(trash).unwrap().flags |= flags::UNREADABLE;
        t.aggregate();
        let (a, r) = analysis(&t, &home, 0);
        let o = summarize(
            &t,
            &ScanSummary::default(),
            &["/Users/me".into()],
            vol(),
            0,
            &a,
            &r,
        );
        assert_eq!(o.trash_bytes, None);
    }
}
