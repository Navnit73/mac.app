# MyMacStorage for Mac — Product Requirements Document

| | |
|---|---|
| **Product** | MyMacStorage (working name) — native-feeling macOS disk analyzer, cleaner and duplicate finder |
| **Platform** | macOS 14 (Sonoma) and later, Apple Silicon + Intel |
| **Stack** | Rust core · Tauri 2 shell · React + Vite + TypeScript + Tailwind UI |
| **Business model** | Free download, full access, no licence (changed 2026-10-31) |
| **Doc status** | v1.0 — ready for implementation |

---

## 1. Vision and principles

**Vision:** Show a Mac owner exactly what is filling their disk, then let them reclaim it safely in a few clicks.

**Principles (these override any feature decision):**

1. **Nothing is permanently deleted.** Every cleanup moves items to the Trash. Worst case, the user drags a file back.
2. **Review before action.** The user sees every item, its path and its size before anything moves.
3. **Local only.** Scanning, hashing, matching and analysis happen on the Mac. No file name, path or content is ever uploaded. No account, no analytics, no sync. Only the license key is validated, and it validates offline.
4. **Honest numbers.** Sizes must account for hard links, APFS clones, sparse files, and files that changed mid-scan. Never show "you will save X GB" unless it is real.
5. **Rust does all filesystem work.** The React UI only renders data and sends commands.
6. **Fast and cancellable.** A scan must never freeze the UI and must always be cancellable.

## 2. Goals and non-goals

### Goals (v1.0)
- One full-disk scan that powers every screen (a single in-memory/indexed tree, scanned once).
- Seven feature areas ("rooms"): Overview, Find, Space, Cleanup, Duplicates, Applications, Activity. (Monitor and Compress were removed from scope on 2026-10-29.)
- Safe cleanup with Trash-based undo.
- Signed, notarized `.dmg` sold from the product's own website.

### Non-goals (v1.0)
- No automatic deletion of macOS system files, no "registry-style optimizers", no memory "boosters".
- No cloud sync, accounts, or telemetry.
- No visually-similar photo detection (planned v1.1; exact matches only in v1.0).
- No Mac App Store build (the sandbox blocks whole-disk scanning). Distribute directly.
- No Windows build in v1.0 — but keep `core` crate platform-agnostic so it is possible later.

## 3. Target users

| Persona | Pain | Wins with |
|---|---|---|
| **Everyday Mac owner** | "Storage almost full", "System Data is huge" | Overview, Cleanup, one-click safe clear |
| **Developer** | Xcode, node_modules, Docker, simulators, build caches | Cleanup (developer files), Space, Applications |
| **Creator** (photo/video) | Huge video files, duplicate exports | Space, Duplicates |
| **Privacy-minded user** | Distrusts "cleaner" apps | Local-only guarantee, Trash-only, offline license |

## 4. Architecture overview

```
┌────────────────────────────────────────────────────────────┐
│ React + Vite + TypeScript + Tailwind + Zustand              │
│ Rooms UI · Charts · Virtualized lists · Review dialogs      │
└──────────────▲───────────────────────────┬─────────────────┘
               │ events (progress, deltas)  │ typed commands (invoke)
┌──────────────┴───────────────────────────▼─────────────────┐
│ Tauri 2 shell (src-tauri)                                   │
│ commands/ · events · state · permissions · menu · updater   │
└──────────────▲───────────────────────────┬─────────────────┘
               │                           │
┌──────────────┴───────────────────────────▼─────────────────┐
│ mymacstorage-core (Rust library, no Tauri dependency)            │
│ scanner · index · find · space · cleanup · duplicates ·     │
│ apps · activity · trash · license                           │
└──────────────▲───────────────────────────┬─────────────────┘
               │                           │
        macOS filesystem · IOKit · NSFileManager (Trash) · SQLite
```

**Key decisions**
- **UI framework: React 18 + Vite + TypeScript (SPA).** Tauri loads a static bundle from a WebView; there is no server at runtime, so a meta-framework such as Next.js adds routing, SSR and build complexity with no benefit. Vite is Tauri's first-class, officially supported frontend path, with instant HMR. Routing uses `react-router` (hash or memory router). All data comes from Tauri `invoke` and events.
- **Why not alternatives:** SwiftUI would be the most native look but would discard the Rust-centred plan and a shared engine for a future Windows build; egui/Slint lack mature virtualized tables and chart tooling. React + Vite gives the best speed of UI development with a reusable Rust core.
- **Cargo workspace:** `crates/core` (pure logic, unit-testable, no UI) and `src-tauri` (thin command layer).
- **Persistence:** SQLite (`rusqlite`) in `~/Library/Application Support/MyMacStorage/` for settings, activity log, trash manifests, scan snapshots. Scan index in memory with optional on-disk cache.
- **IPC:** Commands return small payloads. Large lists are paginated or streamed via events (never ship 5 million rows in one `invoke`). Use a generated TypeScript types file (`ts-rs` or `specta`) so Rust and TS types never drift.

## 5. Shared platform requirements

### 5.1 The scan engine (foundation for every room)
- **Input:** one or more roots (default: whole startup volume at `/` plus the user's home; external and network volumes opt-in).
- **Traversal:** parallel, bounded workers (`jwalk` or `ignore`, or direct `getattrlistbulk` for speed on APFS). Do not follow symlinks. Do not cross into other volumes unless chosen. Skip `/System/Volumes/Data` double-counting by using firmlink-aware traversal (scan `/` once; treat `/System/Volumes/Data` as the same data).
- **Per-entry data:** path (interned), name, parent id, kind (file/dir/symlink/package), logical size, **allocated size** (`st_blocks * 512`), inode + device (hard-link detection), mtime, atime/last-used (`kMDItemLastUsedDate` where cheap), flags (hidden, package, compressed, cloud-placeholder/dataless).
- **Rules:**
  - Count a hard-linked inode once.
  - Treat `.app`, `.photoslibrary`, `.sparsebundle` etc. as packages (one row; expandable).
  - Skip dataless iCloud placeholders in size totals (flag them).
  - Report every skipped path with a reason (permission, vanished, error) in a **Scan Report**.
  - Handle files changing mid-scan without crashing.
- **Output:** an in-memory tree with pre-aggregated directory totals, plus a stream of progress events (files scanned, bytes, current path, elapsed).
- **Targets:** 1M files indexed in under 60 s on an SSD Apple Silicon Mac (warm cache); UI stays at 60 fps during scan; peak memory < 1.5 GB for 5M files.
- **Cancellation:** cooperative via an atomic flag. Cancel stops within 500 ms and keeps partial results.
- **Re-scan:** support rescanning a subtree (after a cleanup) without a whole-disk rescan.

### 5.2 Permissions (Full Disk Access)
- On first launch, explain *why* Full Disk Access is needed, then deep-link to **System Settings → Privacy & Security → Full Disk Access**.
- Detect FDA status by attempting to read a protected location (for example `~/Library/Safari`), not by guessing.
- App works without FDA, with a persistent banner listing the protected areas that are excluded and how to enable them.
- Folder picker (native dialog) for scanning specific folders.

### 5.3 Safe delete subsystem (Trash)
- All removals go through one module: `trash::move_items(items) -> TrashBatch`.
- Use `NSFileManager trashItemAtURL:resultingItemURL:` (via `objc2-foundation`) so the *resulting Trash path is captured* and Undo is possible.
- **Before each item moves:** re-stat the path (exists? same inode/size/mtime as when listed?). For duplicates, re-verify content hash against a kept copy. If anything changed, skip the item and report it.
- **Protected-path denylist** (hard-coded, not overridable): `/System`, `/usr`, `/bin`, `/sbin`, `/private/var/db`, `/Library/Apple`, the app's own bundle, the Trash itself, any path not on a user-visible volume, and the last remaining copy in a duplicate group.
- **Batch manifest** (SQLite): batch id, timestamp, room, item list (original path, trashed path, size). Powers **Undo** (move each back, recreating parent dirs if needed; report conflicts) and the Activity room.
- **Bytes freed ≠ bytes moved.** Moving to the Trash does not free space until it is emptied. UI must say "Moved to Trash — empty Trash to reclaim 70.5 GB", with an **Empty Trash** button (confirmation required; this is the only permanent-deletion action in the app).
- Failures are per-item and never abort the batch.

### 5.4 UI shell
- Sidebar with nine rooms, a top toolbar (scan button, volume picker, global search), and a status footer (scan state, FDA status).
- macOS-native feel: system font stack (`-apple-system`), light and dark mode via `prefers-color-scheme`, neutral palette, hairline dividers, subtle accent. No gradients or glassmorphism.
- Large lists use virtualization (`@tanstack/react-virtual`). Charts: custom SVG/canvas (sunburst, treemap, bars) for performance.
- Keyboard: ⌘F focus Find, ⌘R rescan, Space = Quick Look the selected item, ⌘⌫ move selection to Trash (with review), ⌘Z undo last batch.
- Accessibility: full keyboard navigation, VoiceOver labels, color is never the only signal.

### 5.5 Settings
Exclusions (paths/globs), hidden files on/off, include external/network volumes, scan on launch, minimum duplicate size (default 1 MB), keep-rule defaults for duplicates, license status, privacy statement, reset all data.

---

## 6. Feature specifications

Each feature uses the same template: **Purpose · User stories · Functional requirements · Rust API · UI · Edge cases · Acceptance criteria.**

---

### 6.1 Overview — "the whole drive on one screen"

**Purpose.** The home screen. After one scan it explains the System Settings–style storage bar, with each slice named and clearable.

**User stories**
- As a user I see how much of my disk is used and what categories fill it.
- As a user I can click any colored slice and jump to the room that clears it.
- As a user I see a total of "what you could clear" vs "what you keep".

**Functional requirements**
1. Storage bar for the startup volume: *used / free / total* using the volume's available capacity including purgeable space (`volumeAvailableCapacityForImportantUsage`) as the headline "free", and raw free as a tooltip.
2. Segments: **Kept** (grey) and clearable categories in color: Developer files, Old downloads, Caches, Duplicates, Logs, Leftovers (from uninstalled apps), Large old files. Each segment shows size and item count.
3. "Clearable total" with a primary **Review & Clear** button that opens the Cleanup review sheet pre-filled with the *safe* defaults (never including anything flagged "review carefully").
4. Cards: Files scanned, scan time, last scan date, largest folder, Trash size (with Empty Trash).
5. Scan control states: *Never scanned → Scanning (progress, current folder, cancel) → Done → Stale (changes since scan)*.
6. After a cleanup: animated bar update plus "Done. 70.5 GB moved to Trash" with **Undo**.

**Rust API**
- `start_scan(roots, options) -> ScanId`, `cancel_scan(id)`, event `scan://progress`, `scan://done`.
- `get_overview(scan_id) -> OverviewSummary { volume, categories[], trash_bytes, skipped_count }`.
- Categorisation is rule-driven (see 6.4) so Overview and Cleanup share one source of truth.

**Edge cases.** Purgeable space and local Time Machine snapshots make "used" differ from sum of files; show an "Other / System" segment for the unexplained remainder with an explanation tooltip. Multiple volumes: volume picker.

**Acceptance criteria**
- Sum of category bytes ≤ used bytes; the remainder appears as "System & other".
- Clicking each segment navigates to the correct filtered view.
- Overview renders < 100 ms after scan completes.

---

### 6.2 Find — "any file, found as you type"

**Purpose.** Instant search across every scanned file and folder, hidden ones included.

**User stories**
- As a user I type part of a name and see results instantly.
- As a user I filter by size, kind, modified date, and location.

**Functional requirements**
1. Search over the in-memory index; results update on each keystroke (debounce ≤ 50 ms). Target < 100 ms for 5M entries.
2. Match modes: substring (default), prefix, exact, glob, regex (toggle). Case-insensitive default. Use a precomputed lowercase name table; optionally a trigram or suffix index for speed.
3. Filters: kind (documents, images, video, audio, archives, apps, code, disk images, other), size range, modified/created/last-opened range, hidden only, in folder (scoped).
4. Sort: relevance, name, size, modified date.
5. Result row: icon, name, path (middle-truncated), size, modified. Selecting a row opens the **Inspector** (see 6.10).
6. Row actions: Reveal in Finder, Quick Look, Copy Path, Move to Trash (through review).
7. Multi-select with ⇧/⌘ click; bulk Trash with review.

**Rust API.** `search(scan_id, query, filters, sort, page) -> { total, items[] }` (paginated, cursor-based).

**Edge cases.** Unicode normalisation (NFC vs NFD names on macOS: normalise both query and index), very large result sets (virtualize, cap preview at 10k with "refine" hint), files deleted after the scan (show "missing" state and refresh the row).

**Acceptance criteria**
- Searching "report" on a 1M-file index returns the first page in < 100 ms.
- Hidden files and dot-folders are searchable when "Include hidden" is on.
- Moving a result to the Trash updates results without a rescan.

---

### 6.3 Space — "the heavy things come to the top"

**Purpose.** Visual and list-based explanation of what is using space: biggest files, biggest folders, and a visual map.

**User stories**
- As a user I see the largest files on my Mac, sorted.
- As a user I drill into a folder and see what inside it is heavy.
- As a user I toggle between a sunburst, treemap, and list.

**Functional requirements**
1. **Large files list:** top N (default 500) by allocated size, with filters (min size: 100 MB / 500 MB / 1 GB / custom, kind, age, location).
2. **Folder tree/list:** expandable hierarchy with size bars showing % of parent; sort by size, name, file count, modified.
3. **Sunburst chart:** rings by depth (cap at 5 visible rings), hover tooltip, click to zoom into folder, breadcrumb to zoom out, color by kind or by age.
4. **Treemap chart:** squarified layout, same interaction.
5. **Age insight:** "Not opened in 1+ year" badge using last-used date.
6. Breadcrumb and back/forward navigation. Selection is shared with the Inspector.
7. Actions: Reveal in Finder, Quick Look, Move to Trash (via review), Add to exclusions.
8. Smart groups: *Old installers (.dmg, .pkg)*, *Archives (.zip, .tar, .7z)*, *Videos*, *Disk images / VM files*, *Downloads older than 30 days*.

**Rust API**
- `get_children(node_id, sort, page)`, `get_top_files(filters, limit)`, `get_chart_data(node_id, depth, mode)` (pre-aggregated, capped nodes with an "other" bucket for tiny slices), `get_breadcrumb(node_id)`.

**Edge cases.** Packages (treat as a single block with "Show contents"), symlink loops (not followed), sparse and compressed files (show allocated vs logical size; default to allocated), APFS clones (document that clones share blocks; show a "shared data" hint).

**Acceptance criteria**
- Drilling into any folder updates the chart in < 150 ms.
- Folder totals equal the sum of children (verified by test fixtures).
- Chart never renders more than ~300 visible nodes (aggregate the remainder).

---

### 6.4 Cleanup — "caches, build files and old installers, grouped and totalled"

**Purpose.** Rule-based discovery of safely-reclaimable space, grouped and totalled, with review before action.

**User stories**
- As a user I see categories of junk with sizes and can expand each to see items.
- As a developer I can clear Xcode DerivedData, simulators, `node_modules` and similar, knowing they regenerate.
- As a user I can tick items and clear them in one action.

**Functional requirements**
1. **Rule engine:** declarative rules in a versioned JSON/TOML file bundled with the app. Each rule: `id`, `category`, `name`, `description`, `paths/globs`, `risk` (`safe` | `review` | `careful`), `regenerates` (bool), `requires_app_closed` (optional bundle id), `min_age_days` (optional).
2. **Categories and example rules**
   - **Developer files** (`safe`/`review`): `~/Library/Developer/Xcode/DerivedData`, `Xcode/Archives` (review), `CoreSimulator/Devices` unavailable runtimes (use `xcrun simctl delete unavailable` guidance), `~/Library/Caches/CocoaPods`, `~/.gradle/caches`, `~/.npm/_cacache`, `~/Library/Caches/Homebrew`, `~/.cargo/registry` cache, `target/` dirs in Cargo projects (detected by sibling `Cargo.toml`), `node_modules` in projects not modified for 60+ days (detected by sibling `package.json`), Docker data (report only; link to `docker system prune`).
   - **Old downloads** (`review`): files in `~/Downloads` older than N days (default 30), and old `.dmg`/`.pkg` installers anywhere.
   - **Caches** (`safe`): `~/Library/Caches/*` per app (skip caches of running apps), browser caches (Safari/Chrome/Firefox/Edge/Arc) with "will log nothing out" copy.
   - **Logs** (`safe`): `~/Library/Logs`, crash reports in `~/Library/Logs/DiagnosticReports`.
   - **Mail & messaging** (`review`): Mail downloads cache.
   - **Leftovers** (`review`): remnants of uninstalled apps (provided by Applications room, 6.6).
   - **Trash** (`careful`): shown as info with Empty Trash.
3. Each category shows: total size, item count, risk badge, short "what is this / will it come back" explanation.
4. Items default to selected only if risk is `safe`. `review` items are unselected by default. `careful` are never batch-cleared and require per-item confirmation.
5. **Review sheet:** list of everything to move (path, size, risk), totals, and a "Move N items (X GB) to Trash" button. A checkbox "Quit running apps first" appears when relevant; never force-quit apps silently.
6. **Running-app guard:** if a cache belongs to a running app (via `NSWorkspace` running apps), flag it and skip by default.
7. **Result:** success banner with total moved, per-item failures, **Undo**, and **Empty Trash**.
8. Rules update by shipping a new rules file in app updates only (no network fetch for rules in v1).

**Rust API.** `list_cleanup_categories(scan_id)`, `list_cleanup_items(category_id, page)`, `build_cleanup_plan(selection) -> Plan`, `execute_plan(plan) -> TrashBatch`.

**Edge cases.** Path no longer exists, permission denied (needs FDA), symlinked cache directories (never traverse out of the declared root), paths that match multiple rules (count bytes once).

**Acceptance criteria**
- No rule can ever match outside its declared root (unit-tested with traversal and symlink fixtures).
- Totals in Overview equal totals in Cleanup.
- Undo restores 100% of a batch in a test that moves and restores 1,000 files.

---

### 6.5 Duplicates — "files that exist twice, matched byte for byte"

**Purpose.** Find exact duplicate files and let users keep one copy of each safely.

**User stories**
- As a user I see groups of identical files with total reclaimable space.
- As a user I choose which copy to keep per group, or apply a rule to all.
- As a creator I can filter to photos or videos only.

**Functional requirements**
1. **Algorithm (staged):**
   1. Group by exact byte size (ignore 0-byte files and sizes below the minimum threshold, default 1 MB).
   2. Exclude hard links of the same inode (these are not real duplicates; do not count savings).
   3. **Partial hash**: BLAKE3 of the first 64 KB + last 64 KB + size. Drop singletons.
   4. **Full hash**: streamed BLAKE3 with a fixed buffer (1 MB). Parallel with bounded concurrency (`rayon` with a capped pool); lower concurrency for external/spinning drives.
   5. **Byte-for-byte verification** of the files in a group *before any deletion* (done lazily when the user confirms cleanup).
2. **Excluded by default:** `/System`, `/Library`, app bundles' internal resources, `node_modules`, `.git` objects, Photos/Music library internals (shown as a separate opt-in because editing them breaks the apps), cloud placeholders.
3. **Results UI:** groups sorted by reclaimable bytes. Each group: thumbnail/icon, file name(s), size, count, per-file path, created/modified dates. Expand to see all copies.
4. **Keep rules** (applied globally or per group): keep newest, keep oldest, keep the one in the shortest path, keep the one in a chosen preferred folder, manual pick. The app **always preserves at least one copy per group** and blocks selecting all copies.
5. Filters: kind (photos, videos, audio, documents, archives, other), min size, folder scope.
6. **Savings calculation:** `sum over groups of (copies_selected_for_removal × size)`, excluding clones/hard links, recalculated live as the selection changes.
7. Review sheet, then move to Trash with pre-action revalidation (re-hash a kept copy and each removed copy; if mismatch, skip and report).
8. Progress UI: stage name ("Grouping by size", "Quick-hashing", "Verifying"), files and bytes processed, ETA, cancel.
9. Optional (setting, off by default): replace duplicates with APFS clones (`clonefile`) instead of removing. Space is reclaimed while paths still work. Mark as "advanced".

**Rust API.** `start_duplicate_scan(scan_id, options)`, event `dupes://progress`, `get_duplicate_groups(page, filters, sort)`, `apply_keep_rule(rule, scope)`, `compute_savings(selection)`, `verify_and_trash(selection)`.

**Edge cases.** Files modified during hashing (compare mtime/size before and after hashing, discard on mismatch), unreadable files (report and skip), same file reachable via multiple paths (dedupe by inode), case-insensitive volumes, very large files (stream only), very many small files (batch hashing jobs).

**Acceptance criteria**
- A fixture with known duplicates (including same name/different content and different name/same content) is classified with 0 false positives and 0 false negatives.
- Selecting every copy in a group is impossible in UI and rejected in Rust.
- 100 GB of mixed files hashes without memory growth beyond a fixed buffer budget.
- Savings shown equals actual space change after Trash is emptied (verified in an integration test on APFS).

---

### 6.6 Applications — "every app with everything it left behind"

**Purpose.** List installed apps with true total footprint, last-used date, and related files, and uninstall cleanly (to Trash).

**User stories**
- As a user I see every app sorted by total size, with when I last opened it.
- As a user I uninstall an app and its support files in one reviewed action.
- As a user I find leftovers of apps I already deleted.

**Functional requirements**
1. **Discovery:** `/Applications`, `~/Applications`, `/System/Applications` (read-only, shown but not removable), plus apps found by the scan. Read `Info.plist` (name, bundle id, version, icon).
2. **Footprint:** app bundle size + related files, found by matching bundle id and app name in these locations: `~/Library/Application Support`, `~/Library/Caches`, `~/Library/Preferences`, `~/Library/Containers`, `~/Library/Group Containers`, `~/Library/Saved Application State`, `~/Library/Logs`, `~/Library/LaunchAgents`, `~/Library/HTTPStorages`, `~/Library/WebKit`, `/Library/Application Support` (review), and receipts in `/private/var/db/receipts` (information only).
3. **Matching safety:** match on exact bundle id or exact bundle-id-prefixed name; never match on short or generic names. Show match confidence and let the user untick items.
4. **App details panel (inspector):** version, bundle id, architecture (Intel/ARM/Universal), signed by, last opened, size breakdown (app / data / caches), related file list.
5. **Uninstall flow:** select app → review list of app + related files → Move to Trash. If the app is running, ask the user to quit it first (offer to quit via `NSRunningApplication.terminate()` politely; never force kill). Apple system apps and the app itself are never removable.
6. **Leftovers detector:** scan the Library locations above for bundle ids that have no installed app; present as the "Leftovers" category (shared with Cleanup), `review` risk.
7. Sorting/filtering: size, last used, name, kind (Intel only helps Rosetta users), source (App Store vs other).
8. Hint: "Unused for 12+ months" badge.

**Rust API.** `list_apps()`, `get_app_footprint(bundle_id)`, `find_leftovers()`, `plan_uninstall(bundle_id, selected_paths)`.

**Edge cases.** Apps with helper tools and login items, shared containers used by multiple apps from one vendor (flag as "shared", unselected by default), apps installed on external volumes, Mac App Store apps (SIP/permissions may block trashing; surface the error cleanly).

**Acceptance criteria**
- For 10 sample apps, the related-file list contains no file belonging to another app.
- Uninstalling then Undo restores the app and every related file.

---

### 6.7 / 6.8 Monitor and Compress — removed

Removed from scope by the product owner on 2026-10-29. MyMacStorage focuses on storage: no live system monitor and no media compression. Section numbers 6.9 onward are kept so existing references stay valid.

---

### 6.9 Activity — "the space you got back, week by week"

**Purpose.** A history of cleanups and recovered space, and the place to undo past batches.

**Functional requirements**
1. **Log:** every batch (timestamp, room, item count, bytes moved, status: *in Trash / restored / emptied*), stored in local SQLite.
2. **Chart:** space recovered per week (bar chart), cumulative total, filter by room.
3. **Detail view:** list of items in a batch with original path, size, and state.
4. **Undo:** restore an entire batch or selected items while they are still in the Trash. Show conflicts (destination now exists) and offer "restore with a (restored) suffix".
5. **Trash reconciliation:** on launch, check whether manifest items still exist in the Trash; mark "emptied" if gone.
6. **Stats header:** total reclaimed to date, biggest single cleanup, streak/week summary (simple, no gamification pressure).
7. **Export** the log to CSV. **Clear history** (does not touch files).
8. Data never leaves the Mac.

**Rust API.** `list_batches(range)`, `get_batch(id)`, `undo_batch(id, item_ids?)`, `get_weekly_stats(range)`.

**Acceptance criteria**
- Undo works across app restarts as long as items remain in the Trash.
- Weekly totals match the sum of batch bytes.

---

### 6.10 Inspector (cross-cutting)

A right-side panel shown for any selected file, folder or app.
- Shows: name, full path, kind, logical vs allocated size, created/modified/last-opened, owner/permissions, hard-link count, tags, iCloud state, "why is this here?" (matching cleanup rule or owning app when known), quarantine/download source (`kMDItemWhereFroms`) when available.
- Quick Look preview (via a Swift helper or `qlmanage -p` sidecar).
- Actions: Reveal in Finder, Open, Copy path, Add to exclusions, Move to Trash.
- Read-only for protected paths with an explanatory lock indicator.

### 6.11 Licensing and onboarding

> **2026-10-31:** Licensing is removed. The app is free with full access: no keys, no trial, no paywall. The licensing bullets below are kept for history only (see docs/decisions.md). Onboarding still applies.

- **Trial:** full scanning and analysis free; cleanup actions limited (for example, total 1 GB moved, or 7 days). Exact limits are configurable constants.
- **License key:** Ed25519-signed token (`ed25519-dalek`), verified offline. Store in macOS Keychain. Activation = paste key, validated locally; no account. Optional online check only to confirm refunds/revocation (non-blocking, fails open).
- **Payment:** Merchant of record (Lemon Squeezy, Paddle, or Polar) on the product website (a separate static site, built with Astro); webhook issues the signed key by email.
- **Onboarding (3 steps):** welcome and privacy promise → grant Full Disk Access → first scan.
- "Check a key" and "Lost your key" pages live on the website, not in the app.

---

## 7. Non-functional requirements

| Area | Requirement |
|---|---|
| Performance | See 5.1. UI never blocks; all heavy work off the main thread. |
| Memory | Interned paths, compact node structs (`u32` ids). Budget in 5.1. |
| Reliability | No panics across the IPC boundary; every command returns `Result`. Crash-safe SQLite (WAL). |
| Safety | Section 5.3 invariants covered by unit and integration tests. |
| Privacy | No network calls except the license check and the mymacstorage.com app API: heartbeat with update offers, and anonymous feature events (counts and sizes only; "Share Anonymous Usage Data" turns events off). Never paths, names or contents. Added 2026-10-31 at the product owner's request; see docs/app-api.md. |
| Security | Tauri capability allow-list minimal; CSP strict; no remote content; shell/sidecar allow-list exact binaries only. |
| Compatibility | macOS 14+; Apple Silicon and Intel (universal binary). |
| Accessibility | Keyboard, VoiceOver, reduced-motion respected, contrast AA. |
| Localization | English v1; strings externalised for later. |
| Distribution | Developer ID signed, hardened runtime, notarized, stapled `.dmg`; Tauri updater with signed update manifests. |

## 8. Data model (summary)

- `ScanNode { id: u32, parent: u32, name_id: u32, kind, size_logical: u64, size_alloc: u64, mtime: i64, atime: i64, inode: u64, dev: u32, flags }`
- `CleanupRule`, `CleanupItem { rule_id, path, size, risk }`
- `DuplicateGroup { id, size, hash, files[] }`
- `TrashBatch { id, ts, room, items[ { original, trashed, size, status } ] }`
- `Settings`, `License`.

## 9. Testing strategy

- **Rust unit tests:** size aggregation, hard-link counting, rule matching confinement, keep-rule logic, license verification.
- **Fixture generator:** script that builds a synthetic tree (nested dirs, hard links, symlink loops, sparse files, unicode names, duplicates with same name/different content, huge file) and asserts expected results.
- **Integration tests (macOS CI runner):** Trash move + undo, APFS clone behavior, permission-denied handling.
- **Property tests (`proptest`):** a duplicate group never has all members selected for removal.
- **UI tests:** Playwright against the static build with a mocked Tauri bridge; screenshot tests for light/dark.
- **Manual QA matrix:** clean macOS VM, no FDA, with FDA, external APFS/exFAT drive, network share, low-disk scenario, sleep/wake during scan.

## 10. Milestones

| # | Milestone | Deliverable |
|---|---|---|
| M0 | Scaffold | Tauri 2 + React/Vite, workspace, CI, shared types |
| M1 | Scan engine | Full traversal, index, progress, cancel, scan report |
| M2 | Overview + Space + Inspector | Storage bar, large files, folder tree, sunburst/treemap |
| M3 | Find | Instant search with filters |
| M4 | Safe Trash + Activity | Trash module, manifests, Undo, Activity room |
| M5 | Duplicates | Staged detection, keep rules, review |
| M6 | Cleanup + Applications | Rule engine, leftovers, uninstall |
| M7 | ~~Monitor~~ | Removed from scope (2026-10-29) |
| M8 | ~~Compress~~ | Removed from scope (2026-10-29) |
| M9 | Onboarding + website | Onboarding, landing site (licensing removed 2026-10-31) |
| M10 | Hardening + release | Signing, notarization, QA matrix, `.dmg`, updater |

Realistic effort: M0–M5 ≈ 6–8 weeks solo for a solid core; full scope M0–M10 ≈ 14–20 weeks. Ship a **v0.9 with Overview, Space, Find, Duplicates, Cleanup** first if time is tight.

## 11. Risks and mitigations

| Risk | Mitigation |
|---|---|
| Deleting something needed | Trash-only, denylist, revalidation, review, undo, risk labels |
| Misleading savings numbers | Hard-link/clone awareness, "empty Trash to reclaim" wording, integration test |
| FDA friction | Clear onboarding, graceful degraded mode, skipped-path report |
| Scan too slow/heavy | Bounded parallelism, native bulk APIs, benchmark gates in CI |
| macOS version differences | Test matrix on 14/15/latest; feature-detect, don't version-sniff |
| Notarization/signing issues | Set up the pipeline in M0, not at the end |
| "Cleaner app" distrust | Open privacy statement, no network, plain-language explanations |

## 12. Success metrics (local, self-reported — no telemetry)

- First scan completes without errors on ≥ 95% of beta machines.
- Zero reports of unrecoverable data loss.
- ≥ 4.5/5 average beta satisfaction; refund rate < 5%.

## 13. Open questions

1. Trial limit: time-based or bytes-based?
2. Include Windows later from the same `core` crate? (Decide after v1.0.)
3. ~~Ship Compress in v1.0 or v1.1?~~ Compress removed from scope.
