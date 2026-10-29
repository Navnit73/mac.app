//! Overview: the storage bar and category totals (M2).
//!
//! The categories here come from a small built-in set of well-known locations. They are a preview
//! of the rule engine that arrives with Cleanup (M6), which will replace them so Overview and
//! Cleanup share one source of truth.

use std::path::Path;

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

struct Found {
    id: NodeId,
    bytes: u64,
    count: u32,
}

fn lookup(tree: &Tree, home: &Path, rel: &str) -> Option<Found> {
    let id = tree.find_path(&home.join(rel))?;
    let n = tree.node(id)?;
    Some(Found {
        id,
        bytes: n.total_alloc,
        count: n.file_count,
    })
}

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

#[allow(clippy::too_many_arguments)]
pub fn summarize(
    tree: &Tree,
    scan: &ScanSummary,
    roots: &[String],
    volume: VolumeInfo,
    home: &Path,
    now: i64,
) -> OverviewSummary {
    let scanned = tree.node(ROOT).map_or(0, |n| n.total_alloc);
    let mut claimed: Vec<NodeId> = Vec::new();

    // Developer files.
    let dev: Vec<Found> = DEVELOPER_PATHS
        .iter()
        .filter_map(|p| lookup(tree, home, p))
        .collect();
    let dev_bytes: u64 = dev.iter().map(|f| f.bytes).sum();
    let dev_items: u32 = dev.iter().map(|f| f.count).sum();
    claimed.extend(dev.iter().map(|f| f.id));

    // Caches, excluding the developer caches that live inside ~/Library/Caches.
    let caches = lookup(tree, home, "Library/Caches");
    let (mut cache_bytes, mut cache_items) = caches.as_ref().map_or((0, 0), |f| (f.bytes, f.count));
    if let Some(c) = &caches {
        claimed.push(c.id);
        for f in dev.iter().filter(|f| tree.is_within(f.id, c.id)) {
            cache_bytes = cache_bytes.saturating_sub(f.bytes);
            cache_items = cache_items.saturating_sub(f.count);
        }
    }

    let logs = lookup(tree, home, "Library/Logs");
    claimed.extend(logs.as_ref().map(|f| f.id));

    // Old downloads: direct children of ~/Downloads untouched for 30+ days.
    let downloads = lookup(tree, home, "Downloads");
    let (mut dl_bytes, mut dl_items) = (0u64, 0u32);
    if let Some(d) = &downloads {
        claimed.push(d.id);
        for c in tree.children(d.id) {
            if let Some(n) = tree.node(c) {
                if now.saturating_sub(n.mtime) > 30 * DAY {
                    dl_bytes += n.total_alloc;
                    dl_items += n.file_count.max(1);
                }
            }
        }
    }

    // Large, old files that no other category already covers.
    let (mut large_bytes, mut large_items) = (0u64, 0u32);
    for id in tree.ids() {
        let Some(n) = tree.node(id) else { continue };
        if n.kind != NodeKind::File
            || n.has(flags::DELETED | flags::DATALESS | flags::HARDLINK_DUP)
            || n.total_alloc < LARGE_OLD_MIN
            || now.saturating_sub(n.mtime) < 365 * DAY
            || tree.has_flagged_ancestor(id, flags::PACKAGE)
            || claimed.iter().any(|c| tree.is_within(id, *c))
        {
            continue;
        }
        large_bytes += n.total_alloc;
        large_items += 1;
    }

    let cat = |id, name: &str, blurb: &str, bytes, items, risk, analyzed| CategorySummary {
        id,
        name: name.to_string(),
        blurb: blurb.to_string(),
        bytes,
        items,
        risk,
        analyzed,
    };
    let categories = vec![
        cat(
            CategoryId::Developer,
            "Developer files",
            "Build output and package caches. Your tools rebuild them.",
            dev_bytes,
            dev_items,
            Risk::Safe,
            true,
        ),
        cat(
            CategoryId::OldDownloads,
            "Old downloads",
            "Items in Downloads you have not touched for 30 days.",
            dl_bytes,
            dl_items,
            Risk::Review,
            true,
        ),
        cat(
            CategoryId::Caches,
            "Caches",
            "App caches. They grow back as you use your apps.",
            cache_bytes,
            cache_items,
            Risk::Safe,
            true,
        ),
        cat(
            CategoryId::Logs,
            "Logs",
            "Log files and crash reports.",
            logs.as_ref().map_or(0, |f| f.bytes),
            logs.as_ref().map_or(0, |f| f.count),
            Risk::Safe,
            true,
        ),
        cat(
            CategoryId::LargeOld,
            "Large old files",
            "Files over 500 MB not modified for a year.",
            large_bytes,
            large_items,
            Risk::Review,
            true,
        ),
        cat(
            CategoryId::Duplicates,
            "Duplicates",
            "Exact copies of the same file. Arrives with the Duplicates room.",
            0,
            0,
            Risk::Review,
            false,
        ),
        cat(
            CategoryId::Leftovers,
            "Leftovers",
            "Files from apps you already removed. Arrives with Applications.",
            0,
            0,
            Risk::Review,
            false,
        ),
    ];

    let clearable: u64 = categories
        .iter()
        .filter(|c| c.analyzed)
        .map(|c| c.bytes)
        .sum();
    let used = volume.total.saturating_sub(volume.available_important);

    let trash_bytes = tree.find_path(&home.join(".Trash")).and_then(|id| {
        let n = tree.node(id)?;
        (!n.has(flags::UNREADABLE)).then_some(n.total_alloc)
    });

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
        let o = summarize(&t, &scan, &["/Users/me".into()], vol(), &home, 400 * DAY);
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
        let o = summarize(
            &t,
            &ScanSummary::default(),
            &["/Users/me".into()],
            vol(),
            &home,
            0,
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
        let o = summarize(
            &t,
            &ScanSummary::default(),
            &["/Users/me".into()],
            vol(),
            &home,
            0,
        );
        assert_eq!(o.trash_bytes, None);
    }
}
