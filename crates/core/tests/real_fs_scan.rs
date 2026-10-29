//! Scans a real temporary tree and checks it against an independent reference walk.

use std::collections::HashSet;
use std::fs::{self, File};
use std::io::Write;
use std::os::macos::fs::MetadataExt;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::Path;
use std::sync::atomic::AtomicBool;

use duobolt_core::index::{flags, NodeKind, ROOT};
use duobolt_core::models::{ScanOptions, SkipReason};
use duobolt_core::scanner::scan;
use duobolt_core::scanner::std_reader::StdReader;

/// Reference: plain recursive lstat walk, hard links once, no symlink following.
fn reference(dir: &Path, seen: &mut HashSet<(u64, u64)>, tot: &mut (u64, u64, u64)) {
    let Ok(rd) = fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let Ok(m) = fs::symlink_metadata(e.path()) else {
            continue;
        };
        let first = m.st_nlink() <= 1 || seen.insert((m.st_dev(), m.st_ino()));
        if first {
            tot.0 += m.st_blocks() * 512;
            tot.1 += m.st_size();
        }
        if m.is_dir() {
            reference(&e.path(), seen, tot);
        } else {
            tot.2 += 1;
        }
    }
}

fn build(root: &Path) {
    let w = |p: &str, data: &[u8]| {
        let path = root.join(p);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        File::create(path).unwrap().write_all(data).unwrap();
    };
    w("a/one.bin", &[1u8; 5000]);
    w("a/b/two.bin", &[2u8; 9000]);
    w("dupes/x.dat", &[3u8; 100_000]);
    w("dupes/y.dat", &[3u8; 100_000]);
    w("links/orig.bin", &[4u8; 20_000]);
    fs::hard_link(root.join("links/orig.bin"), root.join("links/hard.bin")).unwrap();
    w("unicode/caf\u{e9}-nfc.txt", b"nfc");
    w("unicode/cafe\u{301}-nfd.txt", b"nfd");
    w(".hidden/h.txt", b"h");
    fs::create_dir_all(root.join("loop/a")).unwrap();
    symlink("..", root.join("loop/a/up")).unwrap();
    File::create(root.join("sparse.bin"))
        .unwrap()
        .set_len(256 * 1024 * 1024)
        .unwrap();
    w("locked/secret.txt", b"x");
}

#[test]
fn totals_match_reference_and_edge_cases_hold() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().canonicalize().unwrap();
    build(&root);
    let locked = root.join("locked");
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();

    let cancel = AtomicBool::new(false);
    let out = scan(
        &StdReader,
        std::slice::from_ref(&root),
        &ScanOptions::default(),
        &cancel,
        &mut |_| {},
    )
    .unwrap();

    // Restore permissions before any assertion can panic, so the temp dir cleans up.
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();

    // Compare against the reference walk (run after restoring perms so it sees the same set,
    // minus the secret file which the scan could not read).
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();
    let mut tot = (0, 0, 0);
    reference(&root, &mut HashSet::new(), &mut tot);
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();

    let root_meta = fs::symlink_metadata(&root).unwrap();
    let expect_alloc = tot.0 + root_meta.st_blocks() * 512;
    let expect_logical = tot.1 + root_meta.st_size();
    assert_eq!(out.summary.total_alloc, expect_alloc, "allocated bytes");
    assert_eq!(out.summary.total_logical, expect_logical, "logical bytes");
    assert_eq!(out.summary.files, tot.2, "file count");

    // Locked dir reported (skipped when running as root, where chmod 000 is readable).
    if unsafe { libc::geteuid() } != 0 {
        assert_eq!(out.report.count(SkipReason::PermissionDenied), 1);
    }

    // Symlink recorded, not followed.
    let up = (1..out.tree.len() as u32)
        .find(|id| out.tree.name(*id) == b"up")
        .unwrap();
    assert_eq!(out.tree.node(up).unwrap().kind, NodeKind::Symlink);
    assert_eq!(out.tree.children(up).count(), 0);

    // Exactly one of the two hard links contributes bytes.
    let dups = (1..out.tree.len() as u32)
        .filter(|id| out.tree.node(*id).unwrap().has(flags::HARDLINK_DUP))
        .count();
    assert_eq!(dups, 1);

    // Unicode forms kept as two entries; hidden dir present.
    let names: Vec<Vec<u8>> = (1..out.tree.len() as u32)
        .map(|id| out.tree.name(id).to_vec())
        .collect();
    assert!(names.contains(&"caf\u{e9}-nfc.txt".as_bytes().to_vec()));
    assert!(names.contains(&"cafe\u{301}-nfd.txt".as_bytes().to_vec()));
    assert!(names.contains(&b".hidden".to_vec()));

    // Sparse file: allocated far below logical.
    let sparse = (1..out.tree.len() as u32)
        .find(|id| out.tree.name(*id) == b"sparse.bin")
        .unwrap();
    let n = out.tree.node(sparse).unwrap();
    assert_eq!(n.size_logical, 256 * 1024 * 1024);
    assert!(n.size_alloc < 1024 * 1024);

    assert_eq!(
        out.tree.node(ROOT).unwrap().total_alloc,
        out.summary.total_alloc
    );
}
