//! Engine throughput over an in-memory filesystem (no disk I/O): tree building, aggregation
//! and the worker/collector pipeline. Real-disk numbers come from `examples/scan_bench`.

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;

use criterion::{criterion_group, criterion_main, Criterion};
use duobolt_core::models::ScanOptions;
use duobolt_core::scanner::mock::MockFs;
use duobolt_core::scanner::scan;

fn bench_scan(c: &mut Criterion) {
    // 10 x 10 x 10 = 1000 leaf dirs of 1000 files: about 1.0M files + 1.1k dirs.
    let fs = MockFs::synthetic("/bench", 1_000_000, 10);
    let roots = [PathBuf::from("/bench")];
    let opts = ScanOptions::default();
    let cancel = AtomicBool::new(false);

    let mut g = c.benchmark_group("scan_1m_mock");
    g.sample_size(10);
    g.bench_function("scan", |b| {
        b.iter(|| {
            let out = scan(&fs, &roots, &opts, &cancel, &mut |_| {}).expect("scan");
            assert!(out.summary.files >= 1_000_000);
            out.summary.total_alloc
        })
    });
    g.finish();
}

criterion_group!(benches, bench_scan);
criterion_main!(benches);
