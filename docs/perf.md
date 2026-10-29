# Performance notes

Machine: Apple Silicon Mac, SSD, macOS 27 (warm cache). Numbers are from this dev machine,
single runs unless stated; re-run on release hardware before quoting them.

## Scan engine (M1)

Target (PRD 5.1): 1M files indexed in under 60 s.

### Real disk: `examples/scan_bench`

```
cargo run --release -p duobolt-core --example scan_bench -- <dir> [std|bulk]
```

`~/Library/Caches` (316,641 files, 20,209 dirs, 12.6 GB allocated):

| Reader | Time | Entries/s |
|---|---|---|
| `StdReader` (readdir + lstat) | 1.0 - 2.4 s | 140k - 320k |
| `BulkReader` (`getattrlistbulk`) | 0.48 - 0.58 s | 580k - 700k |

`du -sk` reports 13,248,476 KiB for the same folder; `BulkReader` reports 13,248,476 KiB (exact).
At about 600k entries/s, 1M files is roughly 2 s on this machine, far below the 60 s target.
Not yet measured: a cold cache, an external drive, and a 2M+ file dataset (done in M10).

`BulkReader` is the default; `StdReader` is the fallback for filesystems without bulk support.

### Engine only: `cargo bench -p duobolt-core --bench scan`

1M synthetic entries over an in-memory filesystem (tree building, aggregation, worker and
collector pipeline, no disk I/O): about 32 ms per scan. The engine is not the bottleneck;
real scans are bound by filesystem calls.

## Memory

`Node` is about 100 bytes plus the name bytes, so 5M files is roughly 0.5 GB, inside the
1.5 GB budget. Not yet measured on a real 5M-file tree.
