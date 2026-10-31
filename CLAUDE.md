# MyMacStorage — project rules

## What this is
A macOS 14+ disk analyzer / cleaner / duplicate finder. Full spec: prd/PRD.md (source of truth).
Build prompts per milestone: prompt/CLAUDE_CODE_PROMPT.md.
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
- prd/PRD.md, prompt/CLAUDE_CODE_PROMPT.md, CLAUDE.md

## Conventions
- Rust: edition 2021, `thiserror` for errors, `tracing` for logs, `cargo clippy -- -D warnings`, `cargo fmt`.
- TS: strict mode, no `any`, Zustand for state, `@tanstack/react-virtual` for long lists.
- Shared types are generated from Rust (`specta`/`tauri-specta` or `ts-rs`) into `src/types/bindings.ts`. Never hand-write duplicates.
- Every feature: unit tests in Rust + a short note in docs/ if behavior is non-obvious.

## Definition of done (per task)
`cargo test`, `cargo clippy -D warnings`, `npm run lint`, `npm run typecheck`, `npm run build` all pass, and acceptance criteria from the PRD for that feature are demonstrated.

## Workflow
Work in small commits. Before large changes, state a plan. When a PRD requirement is ambiguous, pick the safest option and record it in docs/decisions.md.
