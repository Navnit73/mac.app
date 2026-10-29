//! Compact in-memory tree of scanned entries.
//!
//! Nodes live in one `Vec<Node>` addressed by `u32` ids. Names are stored in a single byte
//! arena. Children are linked through `first_child`/`next_sibling` so no per-directory
//! allocation is needed. A parent always has a smaller id than its children, which lets the
//! aggregation pass run as two linear sweeps.

use std::collections::HashSet;
use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::path::PathBuf;

pub type NodeId = u32;

/// Id of the virtual root; real scan roots are its children.
pub const ROOT: NodeId = 0;
/// Sentinel for "no node".
pub const NONE: NodeId = u32::MAX;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeKind {
    File,
    Dir,
    Symlink,
}

/// Bit flags stored in `Node::flags`.
pub mod flags {
    pub const HIDDEN: u8 = 1;
    pub const PACKAGE: u8 = 1 << 1;
    /// iCloud placeholder: excluded from size totals.
    pub const DATALESS: u8 = 1 << 2;
    pub const COMPRESSED: u8 = 1 << 3;
    /// File whose inode has more than one name.
    pub const MULTILINK: u8 = 1 << 4;
    /// A later name of an already counted inode; contributes no bytes.
    pub const HARDLINK_DUP: u8 = 1 << 5;
    /// Tombstone left by a subtree rescan.
    pub const DELETED: u8 = 1 << 6;
    /// Directory that could not be read.
    pub const UNREADABLE: u8 = 1 << 7;
}

#[derive(Debug, Clone)]
pub struct Node {
    pub parent: NodeId,
    pub first_child: NodeId,
    pub next_sibling: NodeId,
    name_off: u32,
    name_len: u16,
    pub kind: NodeKind,
    pub flags: u8,
    pub dev: u32,
    pub inode: u64,
    pub mtime: i64,
    pub atime: i64,
    /// Own logical size (as reported by the filesystem).
    pub size_logical: u64,
    /// Own allocated size (`st_blocks * 512`).
    pub size_alloc: u64,
    /// Aggregated allocated size of this node and everything below it.
    pub total_alloc: u64,
    pub total_logical: u64,
    /// Number of non-directory entries below (and including) this node.
    pub file_count: u32,
}

impl Node {
    pub fn has(&self, flag: u8) -> bool {
        self.flags & flag != 0
    }

    pub fn is_dir(&self) -> bool {
        self.kind == NodeKind::Dir
    }

    /// Bytes this node itself adds to totals after hard-link and dataless rules.
    fn effective(&self) -> (u64, u64) {
        if self.has(flags::DATALESS) || self.has(flags::HARDLINK_DUP) {
            (0, 0)
        } else {
            (self.size_alloc, self.size_logical)
        }
    }
}

/// Attributes of an entry being added.
pub struct NewNode<'a> {
    pub name: &'a [u8],
    pub kind: NodeKind,
    pub flags: u8,
    pub dev: u32,
    pub inode: u64,
    pub mtime: i64,
    pub atime: i64,
    pub size_logical: u64,
    pub size_alloc: u64,
}

#[derive(Debug)]
pub struct Tree {
    nodes: Vec<Node>,
    names: Vec<u8>,
}

impl Default for Tree {
    fn default() -> Self {
        Self::new()
    }
}

impl Tree {
    /// A tree holding only the virtual root.
    pub fn new() -> Self {
        let mut t = Tree {
            nodes: Vec::new(),
            names: Vec::new(),
        };
        t.nodes.push(Node {
            parent: NONE,
            first_child: NONE,
            next_sibling: NONE,
            name_off: 0,
            name_len: 0,
            kind: NodeKind::Dir,
            flags: 0,
            dev: 0,
            inode: 0,
            mtime: 0,
            atime: 0,
            size_logical: 0,
            size_alloc: 0,
            total_alloc: 0,
            total_logical: 0,
            file_count: 0,
        });
        t
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.len() <= 1
    }

    pub fn node(&self, id: NodeId) -> Option<&Node> {
        self.nodes.get(id as usize)
    }

    pub fn node_mut(&mut self, id: NodeId) -> Option<&mut Node> {
        self.nodes.get_mut(id as usize)
    }

    /// Append a child under `parent`. Returns `None` if `parent` is invalid or ids are exhausted.
    pub fn add(&mut self, parent: NodeId, n: NewNode<'_>) -> Option<NodeId> {
        let id = NodeId::try_from(self.nodes.len())
            .ok()
            .filter(|i| *i != NONE)?;
        let name_off = u32::try_from(self.names.len()).ok()?;
        let name_len = u16::try_from(n.name.len()).ok()?;
        let head = self.nodes.get(parent as usize)?.first_child;
        self.names.extend_from_slice(n.name);
        self.nodes.push(Node {
            parent,
            first_child: NONE,
            next_sibling: head,
            name_off,
            name_len,
            kind: n.kind,
            flags: n.flags,
            dev: n.dev,
            inode: n.inode,
            mtime: n.mtime,
            atime: n.atime,
            size_logical: n.size_logical,
            size_alloc: n.size_alloc,
            total_alloc: 0,
            total_logical: 0,
            file_count: 0,
        });
        self.nodes[parent as usize].first_child = id;
        Some(id)
    }

    /// Raw name bytes, for building the search index.
    pub fn name_for_index(&self, id: NodeId) -> &[u8] {
        self.name(id)
    }

    pub fn name(&self, id: NodeId) -> &[u8] {
        match self.nodes.get(id as usize) {
            Some(n) => {
                let start = n.name_off as usize;
                self.names
                    .get(start..start + n.name_len as usize)
                    .unwrap_or(&[])
            }
            None => &[],
        }
    }

    /// Full filesystem path of a node, rebuilt by walking up to the virtual root.
    pub fn path(&self, id: NodeId) -> PathBuf {
        let mut parts = Vec::new();
        let mut cur = id;
        while cur != ROOT && cur != NONE {
            parts.push(cur);
            cur = self.nodes.get(cur as usize).map_or(NONE, |n| n.parent);
        }
        let mut p = PathBuf::new();
        for id in parts.into_iter().rev() {
            p.push(OsStr::from_bytes(self.name(id)));
        }
        p
    }

    /// Find a node by absolute path (matching scan roots first, then walking down).
    pub fn find_path(&self, target: &std::path::Path) -> Option<NodeId> {
        for root in self.children(ROOT) {
            let root_path = PathBuf::from(OsStr::from_bytes(self.name(root)));
            let Ok(rest) = target.strip_prefix(&root_path) else {
                continue;
            };
            let mut cur = root;
            for comp in rest.components() {
                let want = comp.as_os_str().as_bytes();
                cur = self.children(cur).find(|c| self.name(*c) == want)?;
            }
            return Some(cur);
        }
        None
    }

    /// True if `id` is `ancestor` or lies below it.
    pub fn is_within(&self, id: NodeId, ancestor: NodeId) -> bool {
        let mut cur = id;
        while cur != NONE {
            if cur == ancestor {
                return true;
            }
            cur = self.nodes.get(cur as usize).map_or(NONE, |n| n.parent);
        }
        false
    }

    /// True if any ancestor (excluding `id` itself) has `flag` set.
    pub fn has_flagged_ancestor(&self, id: NodeId, flag: u8) -> bool {
        let mut cur = self.nodes.get(id as usize).map_or(NONE, |n| n.parent);
        while cur != NONE && cur != ROOT {
            let Some(n) = self.nodes.get(cur as usize) else {
                return false;
            };
            if n.has(flag) {
                return true;
            }
            cur = n.parent;
        }
        false
    }

    /// All ids except the virtual root, in creation order.
    pub fn ids(&self) -> impl Iterator<Item = NodeId> {
        1..self.nodes.len() as NodeId
    }

    /// Live (non-deleted) children of a node.
    pub fn children(&self, id: NodeId) -> impl Iterator<Item = NodeId> + '_ {
        let mut cur = self.nodes.get(id as usize).map_or(NONE, |n| n.first_child);
        std::iter::from_fn(move || {
            while cur != NONE {
                let this = cur;
                cur = self.nodes[this as usize].next_sibling;
                if !self.nodes[this as usize].has(flags::DELETED) {
                    return Some(this);
                }
            }
            None
        })
    }

    /// Every descendant of `id` (excluding `id`), in no particular order.
    pub fn descendants(&self, id: NodeId) -> Vec<NodeId> {
        let mut out = Vec::new();
        let mut stack: Vec<NodeId> = self.children(id).collect();
        while let Some(n) = stack.pop() {
            out.push(n);
            stack.extend(self.children(n));
        }
        out
    }

    /// Detach and tombstone everything below `id`; ids of other nodes stay valid.
    pub fn clear_children(&mut self, id: NodeId) {
        for d in self.descendants(id) {
            self.nodes[d as usize].flags |= flags::DELETED;
        }
        if let Some(n) = self.nodes.get_mut(id as usize) {
            n.first_child = NONE;
        }
    }

    /// (files, directories) that are still live, excluding the virtual root.
    pub fn live_counts(&self) -> (u64, u64) {
        let mut files = 0;
        let mut dirs = 0;
        for n in self.nodes.iter().skip(1).filter(|n| !n.has(flags::DELETED)) {
            if n.kind == NodeKind::Dir {
                dirs += 1;
            } else {
                files += 1;
            }
        }
        (files, dirs)
    }

    /// Recompute hard-link ownership and bottom-up totals for the whole tree.
    pub fn aggregate(&mut self) {
        let mut seen: HashSet<(u32, u64)> = HashSet::new();
        for n in self.nodes.iter_mut() {
            n.total_alloc = 0;
            n.total_logical = 0;
            n.file_count = 0;
            if n.has(flags::DELETED) {
                continue;
            }
            if n.has(flags::MULTILINK) {
                if seen.insert((n.dev, n.inode)) {
                    n.flags &= !flags::HARDLINK_DUP;
                } else {
                    n.flags |= flags::HARDLINK_DUP;
                }
            }
            let (a, l) = n.effective();
            n.total_alloc = a;
            n.total_logical = l;
            if n.kind != NodeKind::Dir {
                n.file_count = 1;
            }
        }
        for i in (1..self.nodes.len()).rev() {
            let (parent, a, l, c, deleted) = {
                let n = &self.nodes[i];
                (
                    n.parent,
                    n.total_alloc,
                    n.total_logical,
                    n.file_count,
                    n.has(flags::DELETED),
                )
            };
            if deleted || parent == NONE {
                continue;
            }
            let p = &mut self.nodes[parent as usize];
            p.total_alloc += a;
            p.total_logical += l;
            p.file_count += c;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nn(name: &[u8], kind: NodeKind, alloc: u64) -> NewNode<'_> {
        NewNode {
            name,
            kind,
            flags: 0,
            dev: 1,
            inode: 0,
            mtime: 0,
            atime: 0,
            size_logical: alloc,
            size_alloc: alloc,
        }
    }

    #[test]
    fn totals_equal_sum_of_children() {
        let mut t = Tree::new();
        let r = t.add(ROOT, nn(b"/r", NodeKind::Dir, 0)).unwrap();
        let d = t.add(r, nn(b"d", NodeKind::Dir, 0)).unwrap();
        t.add(d, nn(b"a", NodeKind::File, 100)).unwrap();
        t.add(d, nn(b"b", NodeKind::File, 50)).unwrap();
        t.add(r, nn(b"c", NodeKind::File, 7)).unwrap();
        t.aggregate();
        assert_eq!(t.node(d).unwrap().total_alloc, 150);
        assert_eq!(t.node(r).unwrap().total_alloc, 157);
        assert_eq!(t.node(r).unwrap().file_count, 3);
        assert_eq!(t.node(ROOT).unwrap().total_alloc, 157);
    }

    #[test]
    fn hard_links_counted_once() {
        let mut t = Tree::new();
        let r = t.add(ROOT, nn(b"/r", NodeKind::Dir, 0)).unwrap();
        for name in [&b"x"[..], b"y", b"z"] {
            let mut n = nn(name, NodeKind::File, 4096);
            n.flags = flags::MULTILINK;
            n.inode = 42;
            t.add(r, n).unwrap();
        }
        t.aggregate();
        assert_eq!(t.node(r).unwrap().total_alloc, 4096);
        // Aggregating twice is stable.
        t.aggregate();
        assert_eq!(t.node(r).unwrap().total_alloc, 4096);
    }

    #[test]
    fn dataless_excluded_from_totals() {
        let mut t = Tree::new();
        let r = t.add(ROOT, nn(b"/r", NodeKind::Dir, 0)).unwrap();
        let mut n = nn(b"cloud", NodeKind::File, 9999);
        n.flags = flags::DATALESS;
        t.add(r, n).unwrap();
        t.aggregate();
        assert_eq!(t.node(r).unwrap().total_alloc, 0);
        assert_eq!(t.node(r).unwrap().total_logical, 0);
    }

    #[test]
    fn paths_rebuild_and_clear_children() {
        let mut t = Tree::new();
        let r = t.add(ROOT, nn(b"/data", NodeKind::Dir, 0)).unwrap();
        let d = t.add(r, nn(b"d", NodeKind::Dir, 0)).unwrap();
        let f = t.add(d, nn(b"f.txt", NodeKind::File, 10)).unwrap();
        assert_eq!(t.path(f), PathBuf::from("/data/d/f.txt"));
        t.clear_children(d);
        t.aggregate();
        assert_eq!(t.children(d).count(), 0);
        assert_eq!(t.node(r).unwrap().total_alloc, 0);
    }
}
