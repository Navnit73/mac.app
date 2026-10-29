//! Large files, folder drill-down and pre-aggregated chart data (M2).

use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::path::Path;

use crate::errors::AppError;
use crate::index::{flags, Node, NodeId, NodeKind, Tree, ROOT};
use crate::models::{
    AgeBucket, ChartMode, ChartNode, ChildrenPage, Crumb, EntryKind, FileKind, NodeRow, SmartGroup,
    SmartGroupSummary, SortKey, TopFilesResult, TopFilters,
};

pub const PAGE_SIZE: usize = 200;
pub const MAX_TOP_FILES: usize = 1000;
/// Upper bound on nodes sent to a chart.
pub const MAX_CHART_NODES: usize = 300;
const MAX_CHART_DEPTH: u8 = 5;
const DAY: i64 = 86_400;

/// Classify by extension. Directories are `Folder`, except packages which follow their extension.
pub fn file_kind(name: &[u8], is_dir: bool, is_package: bool) -> FileKind {
    if is_dir && !is_package {
        return FileKind::Folder;
    }
    let lower = String::from_utf8_lossy(name).to_lowercase();
    let Some(ext) = Path::new(&lower).extension().and_then(|e| e.to_str()) else {
        return if is_package {
            FileKind::Apps
        } else {
            FileKind::Other
        };
    };
    match ext {
        "jpg" | "jpeg" | "png" | "gif" | "heic" | "heif" | "tiff" | "tif" | "bmp" | "webp"
        | "raw" | "cr2" | "nef" | "arw" | "dng" | "svg" | "psd" | "ico" | "photoslibrary" => {
            FileKind::Images
        }
        "mp4" | "mov" | "m4v" | "avi" | "mkv" | "webm" | "wmv" | "flv" | "mpg" | "mpeg" | "mts"
        | "m2ts" => FileKind::Video,
        "mp3" | "m4a" | "aac" | "wav" | "flac" | "aiff" | "aif" | "ogg" | "opus" | "wma" => {
            FileKind::Audio
        }
        "zip" | "tar" | "gz" | "tgz" | "bz2" | "xz" | "7z" | "rar" | "zst" => FileKind::Archives,
        "dmg" | "iso" | "img" | "vmdk" | "vdi" | "qcow2" | "vhd" | "vhdx" | "pvm"
        | "sparseimage" | "sparsebundle" | "utm" => FileKind::DiskImages,
        "app" | "pkg" | "framework" | "bundle" | "plugin" | "kext" => FileKind::Apps,
        "pdf" | "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" | "pages" | "numbers" | "key"
        | "txt" | "rtf" | "md" | "csv" | "epub" | "odt" => FileKind::Documents,
        "rs" | "js" | "ts" | "tsx" | "jsx" | "py" | "rb" | "go" | "java" | "c" | "h" | "cpp"
        | "hpp" | "swift" | "kt" | "json" | "toml" | "yaml" | "yml" | "html" | "css" | "sh"
        | "php" | "cs" | "o" | "a" | "class" | "jar" => FileKind::Code,
        _ if is_package => FileKind::Apps,
        _ => FileKind::Other,
    }
}

pub fn age_bucket(mtime: i64, now: i64) -> AgeBucket {
    let age = now.saturating_sub(mtime);
    if age < 30 * DAY {
        AgeBucket::Recent
    } else if age < 365 * DAY {
        AgeBucket::ThisYear
    } else {
        AgeBucket::Old
    }
}

fn entry_kind(k: NodeKind) -> EntryKind {
    match k {
        NodeKind::File => EntryKind::File,
        NodeKind::Dir => EntryKind::Dir,
        NodeKind::Symlink => EntryKind::Symlink,
    }
}

/// Display name: the last path component, or the whole path for a scan root like `/`.
pub fn display_name(tree: &Tree, id: NodeId) -> String {
    let raw = String::from_utf8_lossy(tree.name(id)).into_owned();
    match tree.node(id) {
        Some(n) if n.parent == ROOT => match Path::new(&raw).file_name() {
            Some(f) => f.to_string_lossy().into_owned(),
            None if raw == "/" => "Macintosh HD".to_string(),
            None => raw,
        },
        _ => raw,
    }
}

fn row(tree: &Tree, id: NodeId, n: &Node, parent_total: u64) -> NodeRow {
    let is_dir = n.is_dir();
    let is_package = n.has(flags::PACKAGE);
    NodeRow {
        id,
        name: display_name(tree, id),
        kind: entry_kind(n.kind),
        file_kind: file_kind(tree.name(id), is_dir, is_package),
        is_package,
        is_hidden: n.has(flags::HIDDEN),
        is_dataless: n.has(flags::DATALESS),
        is_unreadable: n.has(flags::UNREADABLE),
        size: n.total_alloc,
        size_logical: n.total_logical,
        file_count: n.file_count,
        mtime: n.mtime,
        parent_fraction: if parent_total == 0 {
            0.0
        } else {
            (n.total_alloc as f64 / parent_total as f64).min(1.0) as f32
        },
        has_children: is_dir && n.first_child != crate::index::NONE,
    }
}

/// One page of a folder's children. `cursor` is the offset of the first row.
pub fn get_children(
    tree: &Tree,
    node: NodeId,
    sort: SortKey,
    descending: bool,
    cursor: u32,
) -> Result<ChildrenPage, AppError> {
    let parent = tree
        .node(node)
        .ok_or_else(|| AppError::NotFound(format!("node {node}")))?;
    let parent_total = parent.total_alloc;
    let mut ids: Vec<NodeId> = tree.children(node).collect();
    ids.sort_by(|a, b| {
        let (na, nb) = match (tree.node(*a), tree.node(*b)) {
            (Some(x), Some(y)) => (x, y),
            _ => return std::cmp::Ordering::Equal,
        };
        let ord = match sort {
            SortKey::Size => na.total_alloc.cmp(&nb.total_alloc),
            SortKey::Count => na.file_count.cmp(&nb.file_count),
            SortKey::Modified => na.mtime.cmp(&nb.mtime),
            SortKey::Name => tree
                .name(*a)
                .to_ascii_lowercase()
                .cmp(&tree.name(*b).to_ascii_lowercase()),
        };
        // Stable, deterministic tie-break.
        let ord = ord.then_with(|| a.cmp(b));
        if descending {
            ord.reverse()
        } else {
            ord
        }
    });
    let total = ids.len();
    let start = (cursor as usize).min(total);
    let end = (start + PAGE_SIZE).min(total);
    let items = ids[start..end]
        .iter()
        .filter_map(|id| tree.node(*id).map(|n| row(tree, *id, n, parent_total)))
        .collect();
    Ok(ChildrenPage {
        items,
        total: total as u32,
        next_cursor: (end < total).then_some(end as u32),
        parent_size: parent_total,
    })
}

fn in_group(
    tree: &Tree,
    id: NodeId,
    n: &Node,
    g: SmartGroup,
    now: i64,
    downloads: Option<NodeId>,
) -> bool {
    let old = now.saturating_sub(n.mtime) > 30 * DAY;
    let name = tree.name(id);
    let lower = String::from_utf8_lossy(name).to_lowercase();
    let ext = Path::new(&lower)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");
    match g {
        SmartGroup::OldInstallers => matches!(ext, "dmg" | "pkg") && old,
        SmartGroup::Archives => file_kind(name, false, false) == FileKind::Archives,
        SmartGroup::Videos => file_kind(name, false, false) == FileKind::Video,
        SmartGroup::DiskImages => file_kind(name, false, false) == FileKind::DiskImages,
        SmartGroup::OldDownloads => old && downloads.is_some_and(|d| tree.is_within(id, d)),
    }
}

/// Largest files (and packages), filtered. Hard-link duplicates, dataless files and the insides of
/// packages are never listed, so every byte appears once.
pub fn get_top_files(
    tree: &Tree,
    f: &TopFilters,
    limit: usize,
    now: i64,
    home: Option<&Path>,
) -> TopFilesResult {
    let limit = limit.clamp(1, MAX_TOP_FILES);
    let downloads = home.and_then(|h| tree.find_path(&h.join("Downloads")));
    let scope = f.scope.filter(|s| tree.node(*s).is_some());
    let mut heap: BinaryHeap<Reverse<(u64, NodeId)>> = BinaryHeap::new();
    let (mut count, mut bytes) = (0u32, 0u64);

    for id in tree.ids() {
        let Some(n) = tree.node(id) else { continue };
        if n.has(flags::DELETED | flags::DATALESS | flags::HARDLINK_DUP) {
            continue;
        }
        let listed = n.kind == NodeKind::File || (n.is_dir() && n.has(flags::PACKAGE));
        if !listed || n.total_alloc == 0 || n.total_alloc < f.min_size {
            continue;
        }
        if let Some(k) = f.kind {
            if file_kind(tree.name(id), n.is_dir(), n.has(flags::PACKAGE)) != k {
                continue;
            }
        }
        if let Some(days) = f.older_than_days {
            if now.saturating_sub(n.mtime) < i64::from(days) * DAY {
                continue;
            }
        }
        if let Some(g) = f.group {
            if !in_group(tree, id, n, g, now, downloads) {
                continue;
            }
        }
        if tree.has_flagged_ancestor(id, flags::PACKAGE) {
            continue;
        }
        if let Some(s) = scope {
            if !tree.is_within(id, s) {
                continue;
            }
        }
        count += 1;
        bytes += n.total_alloc;
        heap.push(Reverse((n.total_alloc, id)));
        if heap.len() > limit {
            heap.pop();
        }
    }
    let mut picked: Vec<(u64, NodeId)> = heap.into_iter().map(|Reverse(x)| x).collect();
    picked.sort_by(|a, b| b.cmp(a));
    let items = picked
        .into_iter()
        .filter_map(|(_, id)| tree.node(id).map(|n| row(tree, id, n, 0)))
        .collect();
    TopFilesResult {
        items,
        matched_count: count,
        matched_bytes: bytes,
    }
}

pub fn smart_groups(tree: &Tree, now: i64, home: Option<&Path>) -> Vec<SmartGroupSummary> {
    use SmartGroup::*;
    let groups = [OldInstallers, Archives, Videos, DiskImages, OldDownloads];
    let downloads = home.and_then(|h| tree.find_path(&h.join("Downloads")));
    let mut out: Vec<SmartGroupSummary> = groups
        .iter()
        .map(|g| SmartGroupSummary {
            group: *g,
            count: 0,
            bytes: 0,
        })
        .collect();
    for id in tree.ids() {
        let Some(n) = tree.node(id) else { continue };
        if n.kind != NodeKind::File
            || n.has(flags::DELETED | flags::DATALESS | flags::HARDLINK_DUP)
            || n.total_alloc == 0
            || tree.has_flagged_ancestor(id, flags::PACKAGE)
        {
            continue;
        }
        for (i, g) in groups.iter().enumerate() {
            if in_group(tree, id, n, *g, now, downloads) {
                out[i].count += 1;
                out[i].bytes += n.total_alloc;
            }
        }
    }
    out
}

pub fn get_breadcrumb(tree: &Tree, node: NodeId) -> Result<Vec<Crumb>, AppError> {
    tree.node(node)
        .ok_or_else(|| AppError::NotFound(format!("node {node}")))?;
    let mut out = Vec::new();
    let mut cur = node;
    while cur != ROOT {
        out.push(Crumb {
            id: cur,
            name: display_name(tree, cur),
        });
        match tree.node(cur) {
            Some(n) => cur = n.parent,
            None => break,
        }
    }
    out.reverse();
    Ok(out)
}

struct Slot {
    src: Option<NodeId>,
    name: String,
    value: u64,
    is_dir: bool,
    kind: FileKind,
    age: AgeBucket,
    other_count: u32,
    depth: u8,
    children: Vec<usize>,
}

/// Chart tree below `node`, capped at [`MAX_CHART_NODES`]. The largest expandable folder is opened
/// first, so the budget goes where the bytes are; the remainder of each folder is folded into one
/// "Other" bucket.
pub fn get_chart_data(
    tree: &Tree,
    node: NodeId,
    depth: u8,
    mode: ChartMode,
    now: i64,
) -> Result<ChartNode, AppError> {
    let root = tree
        .node(node)
        .ok_or_else(|| AppError::NotFound(format!("node {node}")))?;
    let depth_limit = depth.clamp(1, MAX_CHART_DEPTH);
    let min_fraction = match mode {
        ChartMode::Sunburst => 0.004,
        ChartMode::Treemap => 0.002,
    };
    let root_value = root.total_alloc.max(1);
    let is_package = |n: &Node| n.has(flags::PACKAGE);

    let mut slots = vec![Slot {
        src: Some(node),
        name: display_name(tree, node),
        value: root.total_alloc,
        is_dir: root.is_dir(),
        kind: file_kind(tree.name(node), root.is_dir(), is_package(root)),
        age: age_bucket(root.mtime, now),
        other_count: 0,
        depth: 0,
        children: Vec::new(),
    }];
    let mut heap: BinaryHeap<(u64, usize)> = BinaryHeap::new();
    if root.is_dir() {
        heap.push((root.total_alloc, 0));
    }

    while let Some((_, idx)) = heap.pop() {
        let (src, d) = (slots[idx].src, slots[idx].depth);
        let Some(src) = src else { continue };
        if d >= depth_limit {
            continue;
        }
        let mut kids: Vec<(NodeId, &Node)> = tree
            .children(src)
            .filter_map(|c| tree.node(c).map(|n| (c, n)))
            .filter(|(_, n)| n.total_alloc > 0)
            .collect();
        kids.sort_by(|a, b| b.1.total_alloc.cmp(&a.1.total_alloc).then(a.0.cmp(&b.0)));
        let threshold = (root_value as f64 * min_fraction) as u64;
        let keep = kids
            .iter()
            .take_while(|(_, n)| n.total_alloc >= threshold)
            .count()
            .min(60);
        let rest = &kids[keep..];
        let other_sum: u64 = rest.iter().map(|(_, n)| n.total_alloc).sum();
        let needed = keep + usize::from(!rest.is_empty() && other_sum > 0);
        if needed == 0 || slots.len() + needed > MAX_CHART_NODES {
            continue;
        }
        for (cid, n) in &kids[..keep] {
            let child = slots.len();
            slots.push(Slot {
                src: Some(*cid),
                name: display_name(tree, *cid),
                value: n.total_alloc,
                is_dir: n.is_dir(),
                kind: file_kind(tree.name(*cid), n.is_dir(), is_package(n)),
                age: age_bucket(n.mtime, now),
                other_count: 0,
                depth: d + 1,
                children: Vec::new(),
            });
            slots[idx].children.push(child);
            // Packages are drawn as one block.
            if n.is_dir() && !is_package(n) && n.first_child != crate::index::NONE {
                heap.push((n.total_alloc, child));
            }
        }
        if !rest.is_empty() && other_sum > 0 {
            let child = slots.len();
            slots.push(Slot {
                src: None,
                name: "Other".to_string(),
                value: other_sum,
                is_dir: false,
                kind: FileKind::Other,
                age: AgeBucket::ThisYear,
                other_count: rest.len() as u32,
                depth: d + 1,
                children: Vec::new(),
            });
            slots[idx].children.push(child);
        }
    }

    fn build(slots: &[Slot], i: usize) -> ChartNode {
        let s = &slots[i];
        ChartNode {
            id: s.src,
            name: s.name.clone(),
            value: s.value,
            is_dir: s.is_dir,
            file_kind: s.kind,
            age: s.age,
            other_count: s.other_count,
            children: s.children.iter().map(|c| build(slots, *c)).collect(),
        }
    }
    Ok(build(&slots, 0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::NewNode;

    fn add(t: &mut Tree, parent: NodeId, name: &str, kind: NodeKind, size: u64, f: u8) -> NodeId {
        t.add(
            parent,
            NewNode {
                name: name.as_bytes(),
                kind,
                flags: f,
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

    fn sample() -> (Tree, NodeId) {
        let mut t = Tree::new();
        let r = add(&mut t, ROOT, "/r", NodeKind::Dir, 0, 0);
        let docs = add(&mut t, r, "Docs", NodeKind::Dir, 0, 0);
        add(&mut t, docs, "a.pdf", NodeKind::File, 4000, 0);
        add(&mut t, docs, "b.mp4", NodeKind::File, 9000, 0);
        add(&mut t, r, "big.dmg", NodeKind::File, 20_000, 0);
        let app = add(&mut t, r, "Tool.app", NodeKind::Dir, 0, flags::PACKAGE);
        add(&mut t, app, "bin", NodeKind::File, 7000, 0);
        t.aggregate();
        (t, r)
    }

    #[test]
    fn kinds_by_extension() {
        assert_eq!(file_kind(b"x.JPG", false, false), FileKind::Images);
        assert_eq!(file_kind(b"x.dmg", false, false), FileKind::DiskImages);
        assert_eq!(file_kind(b"Tool.app", true, true), FileKind::Apps);
        assert_eq!(file_kind(b"Docs", true, false), FileKind::Folder);
        assert_eq!(file_kind(b"noext", false, false), FileKind::Other);
    }

    #[test]
    fn children_sorted_and_paged() {
        let (t, r) = sample();
        let p = get_children(&t, r, SortKey::Size, true, 0).unwrap();
        let names: Vec<_> = p.items.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(names, ["big.dmg", "Docs", "Tool.app"]);
        assert_eq!(p.total, 3);
        assert!(p.next_cursor.is_none());
        let sum: u64 = p.items.iter().map(|i| i.size).sum();
        assert_eq!(sum, p.parent_size, "folder total equals sum of children");
        let p = get_children(&t, r, SortKey::Name, false, 0).unwrap();
        assert_eq!(p.items[0].name, "big.dmg");
        assert!(get_children(&t, 9999, SortKey::Size, true, 0).is_err());
    }

    #[test]
    fn top_files_skip_package_insides_and_filter() {
        let (t, _) = sample();
        let all = get_top_files(&t, &TopFilters::default(), 10, 1_000_000, None);
        let names: Vec<_> = all.items.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(names, ["big.dmg", "b.mp4", "Tool.app", "a.pdf"]);
        assert_eq!(all.matched_bytes, 20_000 + 9000 + 7000 + 4000);

        let f = TopFilters {
            min_size: 8000,
            ..TopFilters::default()
        };
        assert_eq!(get_top_files(&t, &f, 10, 0, None).items.len(), 2);
        let f = TopFilters {
            kind: Some(FileKind::Video),
            ..TopFilters::default()
        };
        assert_eq!(get_top_files(&t, &f, 10, 0, None).items[0].name, "b.mp4");
        let f = TopFilters {
            group: Some(SmartGroup::OldInstallers),
            ..TopFilters::default()
        };
        let r = get_top_files(&t, &f, 10, 100 * DAY, None);
        assert_eq!(r.items.len(), 1);
        assert_eq!(
            get_top_files(&t, &TopFilters::default(), 2, 0, None)
                .items
                .len(),
            2
        );
    }

    #[test]
    fn breadcrumb_runs_from_root() {
        let (t, r) = sample();
        let docs = t.children(r).find(|c| t.name(*c) == b"Docs").unwrap();
        let crumbs = get_breadcrumb(&t, docs).unwrap();
        let names: Vec<_> = crumbs.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["r", "Docs"]);
    }

    #[test]
    fn chart_is_capped_and_conserves_bytes() {
        let mut t = Tree::new();
        let r = add(&mut t, ROOT, "/r", NodeKind::Dir, 0, 0);
        for d in 0..40 {
            let dir = add(&mut t, r, &format!("d{d}"), NodeKind::Dir, 0, 0);
            for f in 0..40 {
                add(
                    &mut t,
                    dir,
                    &format!("f{f}"),
                    NodeKind::File,
                    1000 + (d * f) as u64,
                    0,
                );
            }
        }
        t.aggregate();
        let c = get_chart_data(&t, r, 5, ChartMode::Sunburst, 0).unwrap();
        fn count(n: &ChartNode) -> usize {
            1 + n.children.iter().map(count).sum::<usize>()
        }
        assert!(count(&c) <= MAX_CHART_NODES);
        fn check(n: &ChartNode) {
            let sum: u64 = n.children.iter().map(|c| c.value).sum();
            assert!(sum <= n.value, "children never exceed parent");
            n.children.iter().for_each(check);
        }
        check(&c);
        assert_eq!(c.value, t.node(r).unwrap().total_alloc);
    }

    #[test]
    fn chart_folds_tiny_entries_into_other() {
        let mut t = Tree::new();
        let r = add(&mut t, ROOT, "/r", NodeKind::Dir, 0, 0);
        add(&mut t, r, "huge", NodeKind::File, 1_000_000, 0);
        for i in 0..20 {
            add(&mut t, r, &format!("tiny{i}"), NodeKind::File, 10, 0);
        }
        t.aggregate();
        let c = get_chart_data(&t, r, 3, ChartMode::Treemap, 0).unwrap();
        assert_eq!(c.children.len(), 2);
        let other = c.children.iter().find(|n| n.id.is_none()).unwrap();
        assert_eq!(other.other_count, 20);
        assert_eq!(other.value, 200);
    }

    #[test]
    fn smart_group_totals() {
        let (t, _) = sample();
        let g = smart_groups(&t, 100 * DAY, None);
        let installers = g
            .iter()
            .find(|x| x.group == SmartGroup::OldInstallers)
            .unwrap();
        assert_eq!((installers.count, installers.bytes), (1, 20_000));
        let videos = g.iter().find(|x| x.group == SmartGroup::Videos).unwrap();
        assert_eq!(videos.bytes, 9000);
    }
}
