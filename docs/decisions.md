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

## M2 (Overview, Space, Inspector)
- Overview categories (Developer, Old downloads, Caches, Logs, Large old files) come from a small built-in list of well-known locations in `core::overview`. They preview the Cleanup rule engine (M6), which will replace them so both screens share one source. Duplicates and Leftovers show as "Coming soon" until their rooms exist.
- Overview "free" is the volume's available capacity for important usage (includes purgeable space, as System Settings does). "Used" is total minus that. Anything the scan did not account for is shown as "System & other" (whole-disk scan) or "Outside the scan" (home-only scan).
- Space and Inspector commands take a scan id and a node id, never a path, so the UI cannot ask Rust to act on a path the scan did not produce. Reveal, Open and Quick Look run fixed system tools (`open`, `qlmanage`) with the path as a separate argument, never through a shell.
- Large-files and smart-group lists skip hard-link duplicates, iCloud placeholders and anything inside a package (one row per package), so each byte is listed once.
- Chart data is capped at 300 nodes. The largest openable folder is expanded first; the rest of each folder folds into an "Other" bucket. Packages are drawn as one block.
- Folder colors in charts follow the file kind that dominates them (at least 35% of bytes), otherwise neutral. Age coloring uses one blue ramp because age is ordered.
- Full Disk Access is detected by trying to list protected folders. The banner, footer pill and scan report all explain the cause of "Protected by macOS" skips and link to System Settings. When running in development, the permission belongs to the app that launches DuoBolt (Terminal or the editor).
- Exclusions added from the Inspector are stored in `~/Library/Application Support/DuoBolt/settings.json` and merged into every scan. A settings screen to manage them arrives in M9.
- `core::trash::protected_reason` exists now (shared with the Inspector's lock indicator); M4 builds the move/undo machinery around it.
- A dev-only mock of the Tauri bridge (`src/lib/mockBridge.ts`) lets the UI run in a browser with sample data for screenshots. It is loaded only when `import.meta.env.DEV` and no Tauri runtime is present, and is absent from production bundles.
- `target/` can reach several GB; dev builds use line-table debug info only, and fixtures default to `$TMPDIR/duobolt-fixtures` so `cargo clean` is never blocked by the locked-directory fixture.

## M3 (Find)
- The search index is one byte buffer of lower-cased, NFC-normalized names (NUL-separated) plus a start offset per node. Substring, prefix and exact use a single `memmem` pass; glob and regex test each name. Queries are normalized the same way, so NFC and NFD spellings match each other.
- Glob and regex match the file name only, not the full path. Glob is case-insensitive.
- The index is built on the first search after a scan and dropped when a subtree rescan changes the tree. The last query's full id list is cached so paging does not re-run it.
- At most 10,000 matches are listed (best first for the chosen sort); the true total is always reported and the UI asks the user to refine.
- An empty query with filters lists everything that passes the filters ("all videos over 1 GB").
- "Skip hidden" also hides files inside hidden folders (for example everything in `.git`).
- Date filtering uses the modified time. Created and last-opened dates are not in the scan index (they need extra system calls per file), so they are shown in the Inspector but not filterable yet.
- Results that no longer exist are detected when an action runs on them (Reveal, Open, Quick Look check the path first) and are marked "Missing" in the list.

## M4 (Safe Trash and Activity)
- The UI never sends paths for removal: `preview_trash` and `trash_items` take node ids, and Rust uses the path and the device/inode/size/mtime recorded by the scan. Selecting a folder and something inside it moves only the folder.
- Each item is checked again right before it moves (`core::trash::preflight`): absolute and normalized (also checked on the raw text, because Rust drops `.` components when parsing), parent canonicalized and compared (rejects any symlink on the way), protected list, inside a scan root and not the root itself, not DuoBolt's own bundle, not a mount point, and unchanged since the scan. Folders compare identity and modification time only, because directory sizes differ between readers. One item failing never stops the others.
- Protected list additions: `/private` and its first level, any `.Trash`/`.Trashes` path, mount points, and `/Users/<name>` itself.
- Moves use `NSFileManager trashItemAtURL:resultingItemURL:`, which never follows a symlink. A test verified moving three temp files into the real Trash and back works without Full Disk Access.
- Empty Trash empties the whole Trash through Finder (user's choice), after a second confirmation carrying a one-time token that expires in 60 seconds. It needs the Automation permission for Finder; packaging (M10) must add `NSAppleEventsUsageDescription` and the Apple Events entitlement.
- Undo never overwrites: if the original path is taken, the item stays in the Trash as "name taken" until the user picks "Restore with a new name" (`name (restored).ext`).
- "Recovered" bytes count items in the Trash or emptied; restored items are excluded. Weeks start Monday 00:00 UTC.
- If the Trash cannot be read (no Full Disk Access), reconciliation leaves items as "In Trash" rather than guessing they were emptied.
- The CSV export is written to Downloads by Rust, never overwriting an existing file, then revealed in Finder.
- Destructive dialogs open with focus on Cancel.
