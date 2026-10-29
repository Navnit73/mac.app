//! The bulk reader must agree with the std reader on everything a scan uses.

use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::Write;
use std::os::unix::fs::symlink;
use std::path::Path;

use duobolt_core::scanner::bulk::BulkReader;
use duobolt_core::scanner::reader::{DirReader, RawEntry};
use duobolt_core::scanner::std_reader::StdReader;

fn by_name(r: &dyn DirReader, dir: &Path) -> BTreeMap<Vec<u8>, RawEntry> {
    let l = r.read_dir(dir).unwrap();
    assert!(l.errors.is_empty(), "{:?}", l.errors);
    l.entries.into_iter().map(|e| (e.name.clone(), e)).collect()
}

#[test]
fn bulk_matches_std_on_files_dirs_symlinks_and_links() {
    let tmp = tempfile::tempdir().unwrap();
    let d = tmp.path().canonicalize().unwrap();
    File::create(d.join("small.txt"))
        .unwrap()
        .write_all(b"hello")
        .unwrap();
    File::create(d.join("big.bin"))
        .unwrap()
        .write_all(&vec![7u8; 300_000])
        .unwrap();
    File::create(d.join(".hidden"))
        .unwrap()
        .write_all(b"h")
        .unwrap();
    File::create(d.join("sparse.bin"))
        .unwrap()
        .set_len(64 * 1024 * 1024)
        .unwrap();
    fs::create_dir(d.join("sub")).unwrap();
    fs::hard_link(d.join("small.txt"), d.join("small-link.txt")).unwrap();
    symlink("small.txt", d.join("sym")).unwrap();
    File::create(d.join("caf\u{e9}-nfc.txt")).unwrap();
    File::create(d.join("日本語.txt")).unwrap();
    for i in 0..600 {
        File::create(d.join(format!("many-{i}"))).unwrap();
    }

    let a = by_name(&StdReader, &d);
    let b = by_name(&BulkReader::new(), &d);
    assert_eq!(a.len(), b.len());
    for (name, x) in &a {
        let y = b.get(name).unwrap_or_else(|| panic!("missing {name:?}"));
        assert_eq!(x.kind, y.kind, "kind {name:?}");
        assert_eq!(x.inode, y.inode, "inode {name:?}");
        assert_eq!(x.dev, y.dev, "dev {name:?}");
        assert_eq!(x.mtime, y.mtime, "mtime {name:?}");
        assert_eq!(x.st_flags, y.st_flags, "flags {name:?}");
        if x.kind == duobolt_core::index::NodeKind::File {
            assert_eq!(x.nlink, y.nlink, "nlink {name:?}");
            assert_eq!(x.size_logical, y.size_logical, "logical {name:?}");
            assert_eq!(x.size_alloc, y.size_alloc, "alloc {name:?}");
        }
    }
}

#[test]
fn bulk_reports_open_errors() {
    use duobolt_core::scanner::reader::ReadError;
    let r = BulkReader::new();
    assert_eq!(
        r.read_dir(Path::new("/definitely/not/here")).unwrap_err(),
        ReadError::Vanished
    );
}
