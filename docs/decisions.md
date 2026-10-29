# Decisions

- PRD and prompt files live in `prd/` and `prompt/` rather than the repo root; CLAUDE.md references those paths.
- Shared types use `ts-rs` (not `tauri-specta`): specta is still at release-candidate versions and its crates.io line is moving toward Tauri 3. Tauri is pinned to 2.x. Bindings are written to `src/types/bindings/` by `cargo test` (`npm run gen:types`); CI fails if they are stale. Command wrappers in `src/lib/ipc.ts` are hand-written but only reference generated types.
- Local build workaround: the installed Command Line Tools linker (ld-1267) cannot parse the default MacOSX27.0 SDK (`unknown architecture arm64e.x1-macos`). Build with `SDKROOT=/Library/Developer/CommandLineTools/SDKs/MacOSX26.sdk`. Not committed to the repo because it is machine-specific; CI on `macos-latest` is unaffected.
- `src-tauri/app-icon.png` is a placeholder generated icon; replace before release.
- `zustand` and `react-router-dom` are installed; the first store arrives with M1 (scan state).

## M1 (scan engine)
- Tree root id 0 is a virtual directory; real scan roots are its children. This allows several roots in one scan, and `Tree::path` just joins names (an absolute root name replaces the empty prefix).
- Packages are `NodeKind::Dir` plus the `PACKAGE` flag instead of a separate kind, so traversal code needs no special case. Package contents are still scanned so sizes are exact.
- Hard links: the first name seen in id order owns the bytes (`HARDLINK_DUP` on later names). Totals are exact; which name owns them depends on thread scheduling.
- Directory nodes aggregate their own size plus children, like `du`. `BulkReader` reports 0 own size for directories (APFS directories occupy no blocks); `StdReader` reports `st_size`/`st_blocks`. This only affects logical sizes of directories.
- Firmlinks: scanning `/` allows both the root device and the Data volume device and skips `/System/Volumes/*`, reported as `Excluded`. Mount points on other devices are reported `Excluded` unless `cross_volumes` is set.
- Hidden entries skipped by the "include hidden" toggle are not listed in the Scan Report (it would be huge); globs and volume rules are.
- Subtree rescan tombstones old descendants (`DELETED`) and appends new nodes, so ids held by the UI stay valid. Memory of tombstoned nodes is not reclaimed until the next full scan.
- Only one scan runs at a time, and only the latest finished scan is kept in memory.
- A cancelled directory listing appears as an empty directory in the partial tree.
