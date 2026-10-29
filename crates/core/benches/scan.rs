//! Engine throughput over an in-memory filesystem (no disk I/O): tree building, aggregation
//! and the worker/collector pipeline. Real-disk numbers come from `examples/scan_bench`.

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;

use criterion::{criterion_group, criterion_main, Criterion};
use duobolt_core::find::{search, SearchIndex};
use duobolt_core::models::{ScanOptions, SearchMode, SearchQuery};
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

fn bench_search(c: &mut Criterion) {
    let fs = MockFs::synthetic("/bench", 1_000_000, 10);
    let cancel = AtomicBool::new(false);
    let out = scan(
        &fs,
        &[PathBuf::from("/bench")],
        &ScanOptions::default(),
        &cancel,
        &mut |_| {},
    )
    .expect("scan");

    let mut g = c.benchmark_group("search_1m");
    g.sample_size(10);
    g.bench_function("build_index", |b| {
        b.iter(|| SearchIndex::build(&out.tree).len())
    });
    let idx = SearchIndex::build(&out.tree);
    for (name, text, mode) in [
        ("first_page_substring", "f12", SearchMode::Substring),
        ("first_page_prefix", "f99", SearchMode::Prefix),
        ("first_page_glob", "f1?.dat", SearchMode::Glob),
    ] {
        let q = SearchQuery {
            text: text.into(),
            mode,
            ..SearchQuery::default()
        };
        g.bench_function(name, |b| b.iter(|| search(&out.tree, &idx, &q, 0).total));
    }
    g.finish();
}

criterion_group!(benches, bench_scan, bench_search);
criterion_main!(benches);
