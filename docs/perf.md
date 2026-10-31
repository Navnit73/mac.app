# Performance notes

Machine: Apple Silicon Mac, SSD, macOS 27 (warm cache). Numbers are from this dev machine,
single runs unless stated; re-run on release hardware before quoting them.

## Scan engine (M1)

Target (PRD 5.1): 1M files indexed in under 60 s.

### Real disk: `examples/scan_bench`

```
cargo run --release -p mymacstorage-core --example scan_bench -- <dir> [std|bulk]
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

### Engine only: `cargo bench -p mymacstorage-core --bench scan`

1M synthetic entries over an in-memory filesystem (tree building, aggregation, worker and
collector pipeline, no disk I/O): about 32 ms per scan. The engine is not the bottleneck;
real scans are bound by filesystem calls.

## Memory

`Node` is about 100 bytes plus the name bytes, so 5M files is roughly 0.5 GB, inside the
1.5 GB budget. Not yet measured on a real 5M-file tree.

## M2 views on a real tree

`scan_bench <home> bulk --views` on a real home folder (1.95M files, 225k folders, 129 GB):

| Step | Time |
|---|---|
| Scan | 14.6 s |
| Overview summary | 23 ms |
| Folder listing (sorted, first page) | under 1 ms |
| Large files (100 MB+, top 500) | 3.5 ms |
| Smart groups (5 groups, whole tree) | 98 ms (was 1.0 s before removing per-file string allocation) |
| Sunburst chart data (5 rings, 300 nodes max) | 0.3 ms |

## M3 Find

Target (PRD 6.2): first page for "report" on a 1M-file index in under 100 ms.

Criterion, 1M synthetic names (`cargo bench -p mymacstorage-core --bench scan -- search_1m`):

| Step | Time |
|---|---|
| Build index | 22 ms |
| Substring, first page | 3.6 ms |
| Prefix, first page | 3.5 ms |
| Glob, first page | 14 ms |

Real home folder (2.18M names, `scan_bench <home> bulk --views`):

| Query | Matches | Time |
|---|---|---|
| Index build | | 103 ms |
| `report` (contains) | 1,868 | 5.5 ms |
| `read` (starts with) | 22,362 | 7.9 ms |
| `*.pdf` (glob) | 75 | 93 ms |
| `^img_\d+\.jpe?g$` (regex) | 11 | 53 ms |
| `e` (contains, worst case) | 1.6M | 222 ms |

The one-letter worst case is bound by ranking 1.6M hits; the UI debounces input, so it only runs
once the user pauses. Glob and regex test every name (no index acceleration).
