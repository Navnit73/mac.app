//! Inspector details for one node (M2). Reads fresh metadata so the panel shows the present state.

use std::ffi::CStr;
use std::os::macos::fs::MetadataExt;
use std::path::Path;

use crate::errors::AppError;
use crate::index::{flags, NodeId, Tree};
use crate::models::{ICloudState, NodeDetails};
use crate::space::{display_name, file_kind};
use crate::{overview, platform, trash};

/// `rwxr-xr-x` style string for the permission bits.
pub fn permission_string(mode: u32) -> String {
    let bit = |m: u32, c: char| if mode & m != 0 { c } else { '-' };
    [
        bit(0o400, 'r'),
        bit(0o200, 'w'),
        bit(0o100, 'x'),
        bit(0o040, 'r'),
        bit(0o020, 'w'),
        bit(0o010, 'x'),
        bit(0o004, 'r'),
        bit(0o002, 'w'),
        bit(0o001, 'x'),
    ]
    .iter()
    .collect()
}

fn owner_name(uid: u32) -> Option<String> {
    // SAFETY: getpwuid returns a pointer to static storage or null; the name is copied at once.
    unsafe {
        let pw = libc::getpwuid(uid);
        if pw.is_null() {
            return None;
        }
        Some(CStr::from_ptr((*pw).pw_name).to_string_lossy().into_owned())
    }
}

pub fn node_details(tree: &Tree, id: NodeId, home: &Path) -> Result<NodeDetails, AppError> {
    let n = tree
        .node(id)
        .ok_or_else(|| AppError::NotFound(format!("node {id}")))?;
    let path = tree.path(id);
    let is_package = n.has(flags::PACKAGE);

    let meta = std::fs::symlink_metadata(&path).ok();
    let (created, owner, permissions, hard_links) = match &meta {
        Some(m) => (
            Some(m.st_birthtime()),
            owner_name(m.st_uid()),
            Some(permission_string(m.st_mode())),
            m.st_nlink() as u32,
        ),
        None => (None, None, None, 1),
    };

    let (tags, where_froms, last_used) = if meta.is_some() {
        (
            platform::xattr_plist_strings(&path, "com.apple.metadata:_kMDItemUserTags")
                .into_iter()
                // Tags are stored as "Name\n<color index>".
                .map(|t| t.split('\n').next().unwrap_or("").to_string())
                .filter(|t| !t.is_empty())
                .collect(),
            platform::xattr_plist_strings(&path, "com.apple.metadata:kMDItemWhereFroms"),
            platform::last_used(&path),
        )
    } else {
        (Vec::new(), Vec::new(), None)
    };

    Ok(NodeDetails {
        id,
        name: display_name(tree, id),
        path: path.to_string_lossy().into_owned(),
        kind: match n.kind {
            crate::index::NodeKind::File => crate::models::EntryKind::File,
            crate::index::NodeKind::Dir => crate::models::EntryKind::Dir,
            crate::index::NodeKind::Symlink => crate::models::EntryKind::Symlink,
        },
        file_kind: file_kind(tree.name(id), n.is_dir(), is_package),
        is_package,
        is_hidden: n.has(flags::HIDDEN),
        exists: meta.is_some(),
        size_logical: n.size_logical,
        size_alloc: n.size_alloc,
        total_alloc: n.total_alloc,
        file_count: n.file_count,
        created,
        modified: n.mtime,
        last_used,
        owner,
        permissions,
        hard_links,
        tags,
        where_froms,
        icloud: if n.has(flags::DATALESS) {
            ICloudState::CloudOnly
        } else {
            ICloudState::Local
        },
        protected_reason: trash::protected_reason(&path).map(str::to_string),
        why: overview::describe(&path, home),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::{NewNode, NodeKind, ROOT};
    use std::io::Write;

    #[test]
    fn permission_strings() {
        assert_eq!(permission_string(0o755), "rwxr-xr-x");
        assert_eq!(permission_string(0o600), "rw-------");
        assert_eq!(permission_string(0), "---------");
    }

    #[test]
    fn details_for_real_file_and_vanished_file() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().canonicalize().unwrap();
        let file = dir.join("a.txt");
        std::fs::File::create(&file)
            .unwrap()
            .write_all(b"hello")
            .unwrap();

        let mut t = Tree::new();
        let r = t
            .add(
                ROOT,
                NewNode {
                    name: dir.as_os_str().as_encoded_bytes(),
                    kind: NodeKind::Dir,
                    flags: 0,
                    dev: 1,
                    inode: 0,
                    mtime: 0,
                    atime: 0,
                    size_logical: 0,
                    size_alloc: 0,
                },
            )
            .unwrap();
        let mk = |t: &mut Tree, name: &str| {
            t.add(
                r,
                NewNode {
                    name: name.as_bytes(),
                    kind: NodeKind::File,
                    flags: 0,
                    dev: 1,
                    inode: 0,
                    mtime: 5,
                    atime: 0,
                    size_logical: 5,
                    size_alloc: 4096,
                },
            )
            .unwrap()
        };
        let a = mk(&mut t, "a.txt");
        let gone = mk(&mut t, "gone.txt");
        t.aggregate();

        let d = node_details(&t, a, Path::new("/nonexistent")).unwrap();
        assert!(d.exists);
        assert_eq!(d.name, "a.txt");
        assert!(d.permissions.is_some() && d.created.is_some());
        assert_eq!(d.hard_links, 1);
        assert!(d.protected_reason.is_none());

        let d = node_details(&t, gone, Path::new("/nonexistent")).unwrap();
        assert!(!d.exists);
        assert!(node_details(&t, 9999, Path::new("/")).is_err());
    }
}
