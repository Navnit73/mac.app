//! Deterministic fixture tree generator.
//!
//! Usage: `duobolt-fixtures <output-dir> [--big]`
//! `--big` additionally creates a 2 GB sparse file (no real disk use).
//! The output dir is deleted and recreated, so it must end in `fixtures-out`
//! or be empty/nonexistent.

use std::fs::{self, File};
use std::io::{self, Write};
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::{Path, PathBuf};

/// Deterministic pseudo-random bytes (xorshift), so content is reproducible.
fn bytes(seed: u64, len: usize) -> Vec<u8> {
    let mut s = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
    (0..len)
        .map(|_| {
            s ^= s << 13;
            s ^= s >> 7;
            s ^= s << 17;
            (s & 0xff) as u8
        })
        .collect()
}

fn write(path: &Path, data: &[u8]) -> io::Result<()> {
    if let Some(p) = path.parent() {
        fs::create_dir_all(p)?;
    }
    File::create(path)?.write_all(data)
}

fn generate(root: &Path, big: bool) -> io::Result<()> {
    // Nested dirs with unique files.
    for d in 0..3u64 {
        for f in 0..4u64 {
            let p = root.join(format!("tree/dir{d}/sub{f}/file{f}.bin"));
            write(&p, &bytes(d * 10 + f, 1024 * (f as usize + 1)))?;
        }
    }

    // Same content, different names (true duplicates), 3 copies.
    let dup = bytes(1001, 256 * 1024);
    write(&root.join("dupes/same-content/a.dat"), &dup)?;
    write(&root.join("dupes/same-content/b-renamed.dat"), &dup)?;
    write(&root.join("dupes/other-place/c.dat"), &dup)?;

    // Same name, different content (NOT duplicates), same size.
    write(
        &root.join("dupes/same-name/one/report.txt"),
        &bytes(2001, 4096),
    )?;
    write(
        &root.join("dupes/same-name/two/report.txt"),
        &bytes(2002, 4096),
    )?;

    // Hard links: two names, one inode.
    let hl = root.join("links/original.bin");
    write(&hl, &bytes(3001, 8192))?;
    fs::hard_link(&hl, root.join("links/hardlink.bin"))?;

    // Symlink loop and a symlink to a file.
    fs::create_dir_all(root.join("loop/a"))?;
    symlink("..", root.join("loop/a/up"))?;
    symlink(root.join("links/original.bin"), root.join("loop/to-file"))?;

    // Sparse file: 64 MB logical, ~0 allocated.
    let sparse = File::create(root.join("sparse/sparse-64mb.bin").tap_parent()?)?;
    sparse.set_len(64 * 1024 * 1024)?;

    // Unicode names, NFC (U+00E9) and NFD (e + U+0301).
    write(&root.join("unicode/caf\u{e9}-nfc.txt"), b"nfc")?;
    write(&root.join("unicode/cafe\u{301}-nfd.txt"), b"nfd")?;
    write(&root.join("unicode/日本語.txt"), b"cjk")?;

    // Unreadable directory.
    let locked = root.join("locked");
    write(&locked.join("secret.txt"), b"hidden")?;
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o000))?;

    if big {
        let f = File::create(root.join("sparse/sparse-2gb.bin"))?;
        f.set_len(2 * 1024 * 1024 * 1024)?;
    }
    Ok(())
}

trait TapParent {
    fn tap_parent(self) -> io::Result<PathBuf>;
}

impl TapParent for PathBuf {
    /// Create the parent directory and return the path itself.
    fn tap_parent(self) -> io::Result<PathBuf> {
        if let Some(p) = self.parent() {
            fs::create_dir_all(p)?;
        }
        Ok(self)
    }
}

/// Make sure everything is removable again, then delete.
fn reset(root: &Path) -> io::Result<()> {
    let locked = root.join("locked");
    if locked.exists() {
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o755))?;
    }
    if root.exists() {
        fs::remove_dir_all(root)?;
    }
    fs::create_dir_all(root)
}

fn main() {
    let mut args = std::env::args().skip(1);
    let Some(out) = args.next() else {
        eprintln!("usage: duobolt-fixtures <output-dir> [--big]");
        std::process::exit(2);
    };
    let big = args.any(|a| a == "--big");
    let root = PathBuf::from(out);

    // Safety: only wipe a directory that is empty, absent, or a previous fixture dir.
    let is_fixture_dir = root.join(".duobolt-fixtures").exists();
    let is_empty = fs::read_dir(&root)
        .map(|mut d| d.next().is_none())
        .unwrap_or(true);
    if !is_fixture_dir && !is_empty {
        eprintln!(
            "refusing to overwrite non-empty, non-fixture directory {}",
            root.display()
        );
        std::process::exit(2);
    }

    let result = reset(&root)
        .and_then(|_| write(&root.join(".duobolt-fixtures"), b"marker"))
        .and_then(|_| generate(&root, big));
    match result {
        Ok(()) => println!("fixtures written to {}", root.display()),
        Err(e) => {
            eprintln!("failed: {e}");
            std::process::exit(1);
        }
    }
}
