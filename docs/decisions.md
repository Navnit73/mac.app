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
- The UI has no sample-data mode. Opened outside the DuoBolt window (in a plain browser), it shows a note asking to start the app instead of showing made-up numbers.
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

## M5 (Duplicates)
- Duplicates are found among the files of the last scan (sizes, inodes and mtimes come from the index), not by walking the disk again. One name per inode is checked, so hard links never count as copies.
- Stages: same size → BLAKE3 of the first and last 64 KB plus the size → full streamed BLAKE3 with a 1 MB buffer. Size and modification time are read before and after hashing; a file that changed is dropped and listed as left out.
- Never searched: `/System`, `/Library`, `/private`, package insides (apps and bundles), `node_modules`, `.git`. Hidden folders and `~/Library` are skipped unless "Include app data" is on; Photos and Music libraries only with their own opt-in. Dataless (iCloud) files are skipped because reading them would download them.
- Hashing uses at most 6 threads, or 2 when any candidate is on another volume (external drives are often slow or rotational).
- The selection lives in Rust. The UI asks Rust to change one checkbox or apply a rule, and Rust refuses a change that would select the last unselected copy of a group. Removal (`remove_duplicates`) moves Rust's selection, not ids sent by the UI.
- After each scan Rust applies "Keep the oldest" (usually the original). "Preferred folder" is offered per copy ("Prefer this folder"): in every group, the copy inside that folder is kept; groups with no copy there are left as they are.
- Reclaimable bytes use APFS's private size, so a copy that is already a clone of another counts as freeing almost nothing. The totals update after every change.
- Before anything moves, each selected copy is compared byte for byte with the kept copy (streaming, stops at the first difference). If the kept copy changed or is gone, nothing in that group moves.
- Replacing with APFS clones is behind its own confirmation and is off by default. The selected copy is cloned from the kept one to a temporary name, the original copy goes to the Trash (so Undo works), and the clone takes its path.
- Groups are paged (50 at a time, "Show more") rather than virtualized; each page is a short list of cards with a few rows each. Ticking a box updates the card in place instead of re-sorting, so the card does not jump away from the pointer.
- Image thumbnails use Tauri's asset protocol with an empty scope. When a page of groups is sent to the UI, Rust allows only the first file of each image group (its copies are identical), so the UI can read nothing else.
- Undo after a clone replacement finds the clone already at each path, so (as with any Undo) nothing is overwritten: the originals stay in the Trash as "name taken". The clones hold identical bytes, so no data is lost; "Restore with a new name" brings the originals back beside them.

## M6 (Cleanup and Applications)
- Cleanup rules live in `crates/core/rules/cleanup.toml`, embedded at build time (no network). Unknown fields, `.`/`..`, `//`, relative patterns and roots whose children would be protected are rejected when the rules load.
- Rules match on the scan index, not a new disk walk. Each pattern's root is everything before its first wildcard; matches are always real descendants of that root, because the scanner never follows symlinks. Symlinks are never listed, and a root that resolves elsewhere on disk yields nothing. Right before a move, each item is checked again against its root on disk (`verify_confined`), then by `trash::preflight`.
- Rules run in file order and each node is claimed once. A match that would contain an already claimed node is dropped rather than double counted (for example `~/Library/Caches/Google` is skipped because the Chrome cache inside it is a browser-cache item). `.venv` is matched before `__pycache__` so a virtual environment is one item, not thousands.
- Project rules (`target/` with a sibling `Cargo.toml`, `node_modules` with a sibling `package.json` untouched for 60 days, `.venv`/`__pycache__`) skip `~/Library` and hidden folders in the home folder, so tools' own dependencies (editor extensions and the like) are never offered. They are `review`, not preselected, because rebuilding takes time.
- "Untouched for N days" for a folder means the newest modification time of anything inside it.
- Only `safe` items of apps that are not open are selected by default. `careful` items are never batch-selected and must be confirmed one at a time. Report-only rules (simulator devices, Docker data) are shown with advice and never removed. The selection lives in Rust.
- Running apps come from NSWorkspace. A cache belongs to an app when the rule names its bundle id or the cache folder is named after it. With "Quit them first", apps are asked to quit normally (`terminate`, never forced) and DuoBolt waits up to 10 s; items of apps still open are left alone and reported.
- Overview's category totals come from the same cached analysis as Cleanup (keyed by scan and tree version), so they are equal. "Large old files" stays an Overview-only category and excludes anything a cleanup item covers. Report-only items are not counted as clearable.
- Leftovers: entries directly inside the Library folders named like a bundle id (three or more parts) whose app is not installed. Apple's own entries, Homebrew service agents, and anything sharing a developer prefix with an installed app (for example `com.google.Keystone` while Chrome is installed) are left out. Leftovers are `review` and never preselected.
- Applications are read from `/Applications` (one folder deep), `~/Applications` and `/System/Applications` (shown, never removable). Apps elsewhere found by a scan are not listed yet.
- The Applications room runs its own quick scan of the app folders and the Library locations (about 4 s), so sizes and the before-move checks work even after a home-only scan. Uninstall moves only the app and related files from that scan, checked against those roots.
- A Library entry belongs to an app only when it is named exactly after its bundle id, after the bundle id plus a dot (helpers, extensions; for example a preferences file of "Chrome for Testing" counts as Chrome's), or exactly after its display name when the name is at least 4 characters and not generic. Each entry has one owner (the longest matching bundle id). Containers named after a developer are marked Shared and never suggested; Apple's shared containers are never listed. Name matches and `/Library/Application Support` items are not suggested.
- Apple apps are locked unless they came from the App Store (Pages, Keynote, Numbers can be removed). DuoBolt never removes itself.
- Architecture comes from the Mach-O header of the app's executable; signer from `codesign -dv` (fixed path, path as an argument); last-opened dates from one `mdls` call for all apps. App icons are converted once with `sips` into `~/Library/Application Support/DuoBolt/icons` and only those files are added to the asset protocol scope.
- Undo after an uninstall restores the app and its files like any batch; the Applications room reads the apps again after any change.

## Scope change (2026-10-29)
- Monitor (M7) and Compress (M8) are removed from scope at the product owner's request. Their placeholder modules, sidebar entries, icons and the unused `Compress` activity room were deleted, and the PRD and build prompts mark them as removed. With every remaining room built, the "arrives in M…" placeholder screen and sidebar milestone badges were removed too.
- The app opens in light mode regardless of the macOS appearance (product owner's choice, 2026-10-29). The dark palette is kept behind `data-theme="dark"` for a future Settings option, and the Tauri window is set to the light theme so the title bar matches.

## Safety audit (2026-10-29)
Every removal path was reviewed (Space, Duplicates, Cleanup, Applications, Undo, Empty Trash). Where a rule was ambiguous the safer option was taken.
- Protected list widened. Never movable: every top-level folder of the startup volume (`/Applications`, `/Library`, `/opt` ... themselves), each `/Library/<folder>` itself, keychains (`/Library/Keychains`, `~/Library/Keychains`), a home folder's standard folders themselves (Desktop, Documents, Downloads, Library ...), each `~/Library/<folder>` itself (Caches, Mail, Containers ...), `~/.ssh` and `~/.gnupg` with their contents (private keys cannot be downloaded again), and iCloud Drive / cloud provider folders down to their top level (`~/Library/Mobile Documents/<container>`, `~/Library/CloudStorage/<provider>`), because removing a synced folder removes it on every device. Contents of all of these stay movable unless listed.
- Undo and clone replacement rename with `renamex_np(RENAME_EXCL)`, so a file that appears at the target between the check and the rename is never replaced. Volumes without exclusive rename (exFAT, network shares) fall back to check-then-rename.
- Overlapping scan roots are merged (a root inside another is dropped). Before this, `~` plus `~/Documents` listed each document twice, and the duplicate finder offered the only copy as a duplicate of itself. As defense in depth, duplicate candidates are unique per (device, inode) for every file, and verification refuses a "copy" that is the same file on disk as the kept one.
- "Old installers" only looks in Downloads and on the Desktop. A `.dmg` elsewhere may be an encrypted vault or a backup.
- Leftovers: apps Spotlight knows anywhere (other folders, external drives) count as installed. Leftover Containers and Group Containers are `careful` (a sandboxed app keeps all its user's data there): never batch-selected, confirmed one at a time. A leftover launch agent whose program (absolute path) still exists, or whose plist cannot be read, is not a leftover; this keeps agents users write for their own scripts.
- Cleanup execution moves only items the user saw in the Review sheet: the sheet's node ids go back to Rust and anything else is unselected first, since the analysis can be rebuilt in between.
- Uninstall: the app moves first and its files only if it did. If the bundle changed since the scan (an update) or is otherwise skipped, its data stays. Containers marked Shared are refused even when asked for, and the UI no longer lets them be ticked. App ids stay stable after an uninstall (removed apps are hidden, not deleted from the list), and the UI sends the bundle id it showed; a mismatch refuses the uninstall instead of removing a different app.
- `trash_items` ignores node ids the index already marked deleted.
