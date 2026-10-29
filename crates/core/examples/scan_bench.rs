//! Real-disk scan benchmark: `cargo run --release -p duobolt-core --example scan_bench -- <dir> [std|bulk]`

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::time::Instant;

use duobolt_core::models::ScanOptions;
use duobolt_core::scanner::bulk::BulkReader;
use duobolt_core::scanner::scan;
use duobolt_core::scanner::std_reader::StdReader;

fn main() {
    let mut args = std::env::args().skip(1);
    let Some(dir) = args.next() else {
        eprintln!("usage: scan_bench <dir> [std|bulk]");
        std::process::exit(2);
    };
    let which = args.next().unwrap_or_else(|| "bulk".into());
    let cancel = AtomicBool::new(false);
    let opts = ScanOptions::default();
    let roots = [PathBuf::from(dir)];
    let start = Instant::now();
    let res = if which == "std" {
        scan(&StdReader, &roots, &opts, &cancel, &mut |_| {})
    } else {
        scan(&BulkReader::new(), &roots, &opts, &cancel, &mut |_| {})
    };
    match res {
        Ok(out) => {
            let secs = start.elapsed().as_secs_f64();
            let s = &out.summary;
            println!(
                "{which}: {} files, {} dirs, {:.2} GiB alloc, {} skipped, {:.2}s, {:.0} entries/s, {} KiB (du -sk style)",
                s.files,
                s.dirs,
                s.total_alloc as f64 / (1u64 << 30) as f64,
                s.skipped,
                secs,
                (s.files + s.dirs) as f64 / secs,
                s.total_alloc / 1024,
            );
        }
        Err(e) => {
            eprintln!("scan failed: {e}");
            std::process::exit(1);
        }
    }
}
