# Claude Code Build Prompts — MyMacStorage for Mac

## How to use

1. Create an empty folder `mymacstorage`, put `PRD.md` in it, and run `claude` inside it.
2. Paste **Prompt 0 (Master)** first. It becomes your standing context.
3. Save the "Project rules" block as `CLAUDE.md` in the repo root so every session follows it.
4. Then run **Milestone prompts M0 → M10 one at a time**. Do not start the next until the previous one's acceptance checks pass.
5. Use plan mode (`Shift+Tab` twice) for M1, M4 and M5. They carry the highest risk.

---

## Project rules (save as `CLAUDE.md`)

```md
# MyMacStorage — project rules

## What this is
A macOS 14+ disk analyzer / cleaner / duplicate finder. Full spec: PRD.md (source of truth).
Stack: Rust core crate + Tauri 2 shell + React 18 + Vite + TypeScript + Tailwind (SPA, no meta-framework).

## Non-negotiable invariants
1. Nothing is ever permanently deleted except the explicit "Empty Trash" action. All removals go through `core::trash::move_items`.
2. The UI never touches the filesystem. All file work happens in Rust. The UI only calls Tauri commands and listens to events.
3. No network calls except the optional license check and update check. No analytics. Never upload paths, names or contents.
4. Never follow symlinks during scans. Never operate outside declared roots. Respect the protected-path denylist.
5. Before any destructive action: re-stat (and re-hash for duplicates). Preserve at least one copy per duplicate group.
6. No `unwrap()`/`expect()` in non-test code. Commands return `Result<T, AppError>`. No panics across IPC.
7. The UI is a client-only Vite SPA loaded by Tauri. No Node server, no SSR, no Next.js. Routing via `react-router` (hash router).
8. Never send huge payloads over IPC. Paginate or stream with events.

## Layout
- crates/core/        pure Rust logic, no tauri dependency, fully unit-tested
- src-tauri/          thin command layer, events, state, capabilities
- src/                React + Vite UI
- fixtures/           fixture generator for tests
- PRD.md, CLAUDE.md

## Conventions
- Rust: edition 2021, `thiserror` for errors, `tracing` for logs, `cargo clippy -- -D warnings`, `cargo fmt`.
- TS: strict mode, no `any`, Zustand for state, `@tanstack/react-virtual` for long lists.
- Shared types are generated from Rust (`specta`/`tauri-specta` or `ts-rs`) into `src/types/bindings.ts`. Never hand-write duplicates.
- Every feature: unit tests in Rust + a short note in docs/ if behavior is non-obvious.

## Definition of done (per task)
`cargo test`, `cargo clippy -D warnings`, `npm run lint`, `npm run typecheck`, `npm run build` all pass, and acceptance criteria from PRD.md for that feature are demonstrated.

## Workflow
Work in small commits. Before large changes, state a plan. When a PRD requirement is ambiguous, pick the safest option and record it in docs/decisions.md.
```

---

## Prompt 0 — Master context (paste first)

```text
You are the lead engineer building MyMacStorage, a macOS 14+ desktop app (Apple Silicon + Intel) that analyzes disk usage, finds duplicates, and cleans junk safely. I am the product owner.

Read PRD.md fully before doing anything. It is the source of truth. Also read CLAUDE.md for the non-negotiable rules.

Stack (fixed):
- Rust core library (`crates/core`) containing ALL filesystem logic.
- Tauri 2 shell (`src-tauri`) exposing typed commands + events.
- React 18 + Vite + TypeScript + Tailwind, built to static files and loaded by Tauri. No server runtime.

Product principles: nothing is permanently deleted (Trash only, with undo), review before action, everything local (no uploads, no analytics), honest size numbers (hard links, APFS clones, allocated vs logical size), always cancellable and fast.

Rooms to build: Overview, Find, Space, Cleanup, Duplicates, Applications, Activity, plus a cross-cutting Inspector, Settings, onboarding with Full Disk Access guidance, and offline licensing.

We will build in milestones M0–M10 (listed in PRD.md section 10). I'll give you one milestone at a time. For each milestone:
1. Restate the goal and list the files you will create or change.
2. Ask me at most 3 blocking questions, only if truly necessary. Otherwise make the safest assumption and log it in docs/decisions.md.
3. Implement with tests.
4. Run the full definition-of-done checks and show me the results.
5. Summarize what's done, what's deferred, and how I can manually verify it.

Confirm you've read PRD.md and CLAUDE.md by summarizing the 8 invariants and the milestone list in under 150 words. Do not write code yet.
```

---

## M0 — Scaffold

```text
Milestone M0: project scaffold.

Create a Cargo workspace + Tauri 2 + React/Vite app.

Requirements:
1. Repo layout exactly as in CLAUDE.md: `crates/core`, `src-tauri`, Vite React app in `src/`, `fixtures/`, `docs/`.
2. Frontend: Vite + React 18 + TypeScript strict + Tailwind + `react-router` (hash router) + Zustand. Use `npm create tauri-app` with the React/TypeScript template as the starting point. Configure Tauri `frontendDist` to `../dist` and `devUrl` to `http://localhost:1420` (Vite dev server, fixed port, `strictPort: true`). Add scripts: `dev`, `build`, `lint`, `typecheck`, `tauri`.
3. `crates/core`: empty modules (`scanner`, `index`, `find`, `space`, `cleanup`, `duplicates`, `apps`, `activity`, `trash`, `license`, `errors`, `models`) each with a doc comment and a placeholder test. `AppError` via `thiserror`, serializable for IPC.
4. `src-tauri`: depends on `core`. Register one `ping` command and one event emitter to prove the IPC round trip. Use `tauri-specta` (or `ts-rs`) to generate `src/types/bindings.ts` at build time.
5. Tauri security: minimal capabilities (only what's needed), strict CSP, no remote content, no shell plugin yet.
6. App shell UI: sidebar with the 9 rooms (routes exist, each a placeholder), top toolbar, status footer, light/dark mode via CSS variables, system font stack, neutral macOS-like styling (no gradients, no glassmorphism). Window: min 1000×680, hidden-inset titlebar with proper drag region, native feel.
7. Tooling: `rustfmt`, `clippy -D warnings`, ESLint, Prettier, `lefthook` or `husky` pre-commit.
8. GitHub Actions CI on `macos-latest`: cargo fmt/clippy/test, npm lint/typecheck/build, and `tauri build --debug` smoke build.
9. Add `scripts/gen-fixtures` (Rust bin or shell) that generates a deterministic fixture tree: nested dirs, files with known duplicates (same content/different name AND same name/different content), hard links, a symlink loop, a sparse file, unicode (NFC and NFD) names, a 2 GB sparse file, and unreadable dir (chmod 000).

Acceptance: `npm run tauri dev` opens the window with the shell; clicking a "Ping" dev button shows a response from Rust; CI is green; generated TS bindings compile.
```

---

## M1 — Scan engine (use plan mode)

```text
Milestone M1: the scan engine in `crates/core`. This is the foundation, so correctness and speed matter. Plan first, show me the plan, then implement.

Implement PRD section 5.1 completely:
1. Parallel traversal with bounded workers. Prefer `getattrlistbulk` via `libc` on macOS for speed (fallback to `jwalk`/`std::fs` behind a trait so it's testable). Never follow symlinks. Do not cross volumes unless allowed. Handle firmlinks: scanning `/` must not double count `/System/Volumes/Data`.
2. Compact in-memory tree: `Vec<Node>` with `u32` ids, interned names (string arena), parent index, kind, logical size, allocated size (`st_blocks*512`), mtime, atime, inode, dev, flags (hidden, package, dataless/iCloud placeholder, compressed).
3. Hard links counted once (by (dev, inode)). Packages (.app, .photoslibrary, etc.) flagged. Dataless files flagged and excluded from size totals.
4. Post-pass computes aggregated directory totals (size, file count) bottom-up.
5. Progress events throttled to ~10/s: files, bytes, current path, elapsed, rate. Cooperative cancellation (atomic flag) stops within 500 ms and keeps partial results.
6. Scan Report: every skipped path with a reason enum (PermissionDenied, Vanished, IoError, Excluded, Dataless).
7. Exclusion support (globs) and a hidden-files toggle (hidden files are included by default).
8. Tauri commands: `start_scan`, `cancel_scan`, `get_scan_state`, `get_scan_report(page)`, plus events `scan://progress` and `scan://done`. Scan state lives in managed state behind an `Arc<RwLock<…>>` and a `ScanId`.
9. Subtree rescan API: `rescan_subtree(scan_id, node_id)`.
10. Frontend: a Scan control component (Start/Cancel), progress panel with live counters and current folder, and a "Scan report" dialog with skipped paths. Use the dev route for now.

Tests:
- Unit: size aggregation, hard-link once-count, symlink loop not followed, unicode names, permission-denied reporting, cancellation latency, firmlink no double count (mock).
- Integration: run on `fixtures/` and assert exact expected totals.
- Benchmarks (`criterion`): 1M synthetic entries; document results in docs/perf.md. Target ≥ 1M files in < 60 s on SSD Apple Silicon (report if not met and propose optimizations).

Acceptance: scanning my home folder shows live progress, can be cancelled, and the final totals agree (within 1%) with `du -sk` for a test folder. No UI jank.
```

---

## M2 — Overview, Space, Inspector

```text
Milestone M2: Overview room, Space room, and the Inspector (PRD 6.1, 6.3, 6.10).

Rust (`core`):
- `overview::summary(scan)`: volume capacity (use available capacity for important usage vs raw free), category sums, trash size, skipped count, "System & other" remainder.
- `space::get_children(node, sort, cursor)`, `get_top_files(filters, limit)`, `get_chart_data(node, depth, mode)` returning pre-aggregated nodes (cap ~300; group the rest as "Other"), `get_breadcrumb(node)`, `get_node_details(node)` for the Inspector (including hard-link count, tags, quarantine `kMDItemWhereFroms`, last used date, iCloud state).
- Smart groups: old installers, archives, videos, disk images, old downloads.

UI:
- Overview: the segmented storage bar (kept = grey, clearable categories = color, each labeled), category cards that deep-link to the right room, Trash card, last-scan info, "Review & Clear" button (disabled until M6; wire to a stub).
- Space: three views (List, Sunburst, Treemap) with shared selection and breadcrumb. Sunburst/treemap are custom SVG or canvas components, with hover tooltip, click-to-zoom, color by kind/age, and keyboard focus support. Virtualized lists.
- Inspector: right-side panel with the metadata, Reveal in Finder, Open, Copy Path, Add to exclusions. Quick Look via Tauri command calling `qlmanage -p` (allow-listed). Move to Trash button is disabled until M4.

Acceptance: drilling into any folder updates charts in < 150 ms; folder totals equal sum of children in tests; no chart renders more than ~300 nodes; Overview appears < 100 ms after scan completion; works in light and dark mode.
```

---

## M3 — Find

```text
Milestone M3: Find room (PRD 6.2).

Rust: build a search index at scan completion: lowercase NFC-normalized name table; implement substring (fast, e.g. memchr/`aho-corasick` or a trigram index), prefix, exact, glob (`globset`), regex (`regex`) modes. Filters (kind, size range, modified/created/last-used range, hidden only, folder scope) and sorts. Cursor-based pagination. Command: `search(scan_id, query, filters, sort, cursor)`.

UI: search field with ⌘F focus, mode toggle, filter chips, virtualized results with icon/name/middle-truncated path/size/modified, multi-select, row actions (Reveal, Quick Look, Copy Path), Inspector integration, debounce ≤ 50 ms with request cancellation (ignore stale responses).

Tests: unicode NFC/NFD, hidden files, regex errors handled gracefully, 1M-entry perf benchmark (< 100 ms to first page).
```

---

## M4 — Safe Trash + Activity (use plan mode)

```text
Milestone M4: the safe-delete subsystem and the Activity room (PRD 5.3, 6.9). This is the most safety-critical code in the app; plan first and be conservative.

Rust `core::trash`:
1. `move_items(items, ctx) -> TrashBatch` using `NSFileManager trashItemAtURL:resultingItemURL:` via `objc2-foundation`, capturing the resulting Trash URL for each item.
2. Pre-flight per item: exists, not a symlink escaping root, not in the protected denylist (/System, /usr, /bin, /sbin, /private/var/db, /Library/Apple, app bundle itself, the Trash, non-user volumes), metadata (inode/size/mtime) unchanged since listed. Failures are per-item, never abort the batch.
3. SQLite (`rusqlite`, WAL) in `~/Library/Application Support/MyMacStorage/`: tables `batches`, `batch_items` (original_path, trashed_path, size, status: in_trash | restored | emptied | failed, reason).
4. `undo_batch(id, item_ids?)`: move back from Trash, recreate parent dirs, handle conflicts (offer "(restored)" suffix), update statuses.
5. `empty_trash()`: the ONLY permanent deletion, requires explicit confirmation token from UI, logs the batch as emptied; reconcile statuses on launch.
6. After a batch, trigger `rescan_subtree` for affected parents to keep the index honest.

Tauri commands: `trash_items`, `undo_batch`, `empty_trash`, `list_batches`, `get_batch`, `get_weekly_stats`.

UI:
- A reusable `ReviewSheet` modal (list with path, size, risk badge; totals; confirm button text "Move N items (X GB) to Trash"). Every destructive entry point in the app must use it.
- Result toast/banner: "Moved X GB to Trash. Empty Trash to reclaim the space." with Undo and Empty Trash buttons.
- Wire the Inspector and Find/Space "Move to Trash" actions to the ReviewSheet.
- Activity room: weekly bar chart, batch list, batch detail, undo (whole or selected items), CSV export, clear history (keeps files untouched).

Tests (must include): denylist can't be bypassed (path traversal, symlink, `..`, case variants), move+undo 1,000 files restores all, conflicts handled, files changed after listing are skipped, empty-trash reconciliation, proptest invariants. Do a manual verification script in docs/manual-qa.md.
```

---

## M5 — Duplicates (use plan mode)

```text
Milestone M5: Duplicates room (PRD 6.5). Plan first.

Rust `core::duplicates`:
1. Pipeline: size grouping (min size default 1 MB, ignore 0 bytes) → exclude same-inode hard links → partial hash (BLAKE3 of first 64 KB + last 64 KB + size) → full streamed BLAKE3 (1 MB fixed buffer) → groups. Use `rayon` with a bounded pool; reduce concurrency for external/rotational volumes. Cooperative cancellation. Progress events per stage with files/bytes/ETA.
2. Detect file changes during hashing (compare mtime/size before and after; discard changed files and report).
3. Default exclusions: /System, /Library, app bundle internals, node_modules, .git, Photos/Music library internals (separate opt-in), dataless files.
4. Keep rules: newest, oldest, shortest path, preferred folder, manual. Invariant enforced in Rust: at least one file per group is never selected. Reject selections violating it.
5. `compute_savings(selection)` excludes clones/hard links, recalculated for any selection.
6. `verify_and_trash(selection)`: before trashing, byte-for-byte compare each removal candidate with a kept copy (streaming, early exit); mismatch → skip and report. Then call `trash::move_items`.
7. Optional advanced setting: replace duplicate with APFS clone via `clonefile` (off by default, separate confirmation).

UI: scan options (folders, min size, kinds), staged progress view, results list sorted by reclaimable bytes (virtualized, thumbnails for images via Tauri asset protocol limited to selected paths), per-group expand, per-file checkbox, global keep-rule dropdown, live "Reclaimable: X GB" total, filters by kind, ReviewSheet confirmation, result banner with Undo.

Tests: fixture with known groups (zero false positives/negatives), same-name-different-content, hard links not counted, file modified mid-hash, proptest that selecting all copies is impossible, 100 GB sparse/synthetic stream with bounded memory, integration test that reported savings equals real free-space change after emptying Trash on APFS (skip in CI if not available, run manually).
```

---

## M6 — Cleanup + Applications

```text
Milestone M6: Cleanup room and Applications room (PRD 6.4, 6.6).

Rust:
1. Rule engine: `rules/cleanup.toml` (bundled, versioned, embedded with `include_str!`) with fields id, category, name, description, paths/globs, risk (safe|review|careful), regenerates, requires_app_closed (bundle id), min_age_days. Implement the rules listed in PRD 6.4 (developer files, old downloads/installers, caches, logs, mail cache, trash info). Project-aware rules: `target/` next to Cargo.toml, `node_modules` next to package.json untouched 60+ days, Python `__pycache__`/`.venv` (review).
2. Confinement guarantee: a rule can never yield a path outside its declared root; resolve and verify canonical paths; never traverse symlinks out. Unit-test traversal and symlink attacks.
3. Running-app guard via NSWorkspace; flag and skip default-selected caches of running apps.
4. `build_cleanup_plan(selection)`, `execute_plan(plan)` → `trash::move_items`. Byte-dedupe across overlapping rules.
5. Applications: discovery from /Applications, ~/Applications, /System/Applications (read-only); Info.plist parsing (`plist` crate); footprint via bundle-id exact matching in the Library locations in PRD 6.6; shared-container detection; last used date; architecture detection (Mach-O headers); signing info via `codesign` allow-listed or Security framework.
6. Leftovers detector: Library entries whose bundle id has no installed app → category "Leftovers" (review).
7. Uninstall plan: app + selected related files; polite quit for running app; never remove system apps or MyMacStorage itself.

UI: Cleanup room with category accordion (size, count, risk badge, explanation), item lists with checkboxes, default selection rules (only `safe` preselected), ReviewSheet, result banner. Wire the Overview "Review & Clear" button to a pre-filled safe plan. Applications room: sortable table (size, last used, name, source), details panel, uninstall flow with review of related files and confidence labels.

Tests: rule confinement, no cross-app matches for 10 sample apps, uninstall + undo restores everything, totals in Overview equal Cleanup totals.
```

---

## M7, M8 — removed

Monitor (M7) and Compress (M8) were removed from scope on 2026-10-29. Skip from M6 straight to M9.

---

## M9 — Licensing, onboarding, settings, marketing site

```text
Milestone M9: licensing, onboarding, settings, and the marketing website (PRD 6.11, 5.5).

App:
1. Offline licensing: Ed25519-signed license token (`ed25519-dalek`); public key embedded; verify locally; store in macOS Keychain (`security-framework`). Include a `tools/license-gen` Rust CLI (private key never committed; read from env) to mint keys for testing. Trial mode: scanning and analysis are free; total bytes moved to Trash capped at a configurable constant (default 1 GB). Optional non-blocking online revocation check (fails open, user-disableable).
2. Onboarding: privacy promise → Full Disk Access (detect by probing a protected path, deep-link to System Settings) → first scan. Persistent banner when FDA is missing listing protected areas excluded.
3. Settings screen per PRD 5.5, including exclusions editor, minimum duplicate size, keep-rule default, hidden files, external/network volumes, menu bar toggle, update check toggle, license status, reset data.
4. App menu (About, Check for Updates, Settings ⌘,), Tauri updater with signed manifests (keys via env, documented).

Website (separate `website/` Astro static site, deployed to any static host; checkout webhook as a serverless function): landing page (hero storage bar, nine rooms, safety promises, pricing $19.99 one-time, FAQ), download page, "Lost your key" and "Check a key" pages, Privacy and Terms. Checkout integration stub for a merchant of record (Lemon Squeezy/Paddle/Polar) with a webhook route that mints and emails a license key. Write original copy and visuals; do not copy competitor wording or assets.

Acceptance: invalid/tampered keys rejected; license works offline; trial cap enforced in Rust (not just UI); FDA flow works on a clean macOS VM.
```

---

## M10 — Hardening and release

```text
Milestone M10: hardening and release.

1. Security review: Tauri capabilities minimal, CSP strict, sidecar allow-list exact, no `dangerouslyDisableCsp`, no wildcard fs scopes, input validation on every command (absolute paths only, canonicalize, denylist).
2. Run the safety test suite end-to-end plus `cargo audit`, `cargo deny`, `npm audit`.
3. Performance pass: profile scan, search, duplicates, charts on a ≥ 2M file dataset; fix regressions; document numbers in docs/perf.md.
4. Packaging: universal macOS build (aarch64 + x86_64), hardened runtime, entitlements (no sandbox; document why), Developer ID signing, notarization with `notarytool`, stapling, `.dmg` with background and Applications symlink. Provide a `release.sh` and a GitHub Actions workflow reading secrets from CI (never commit secrets).
5. Auto-update manifest generation and signing.
6. QA matrix checklist in docs/manual-qa.md (clean VM no FDA, with FDA, external APFS and exFAT, network share, low-disk, sleep/wake during scan, macOS 14/15/latest). Run what's automatable and list what I must run manually.
7. Final README with build steps, architecture diagram, and contribution notes.

Deliver a release checklist and a list of known limitations.
```

---

## Handy follow-up prompts

**When something breaks**
```text
Reproduce this first with a failing test (Rust or Playwright), then fix the root cause. Don't mask errors with try/catch or unwrap. Show the test before and after.
```

**Before merging a milestone**
```text
Act as a skeptical reviewer. Audit this milestone against PRD.md acceptance criteria and CLAUDE.md invariants. List any path where a file could be removed without going through trash::move_items, any IPC payload that could grow unbounded, and any place the UI touches the filesystem directly. Fix what you find.
```

**Safety red-team**
```text
Try to break the denylist and confinement: write tests with `..`, symlinks to /System, case-variant paths, NFC/NFD tricks, paths with trailing slashes, and race conditions (file replaced by a symlink between listing and trashing). Make all of them fail safely.
```

**Visual polish**
```text
Review every screen against a native macOS look: spacing on an 8px grid, system font, hairline dividers, correct dark mode contrast, empty/loading/error states for each room, and keyboard focus rings. Fix inconsistencies and add screenshot tests.
```
