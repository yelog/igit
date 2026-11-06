# Commit File Watching Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Replace Commit's one-second full scans with debounced native file notifications and low-frequency reconciliation.

**Architecture:** A `notify` watcher maps working-tree and Git metadata paths to repository IDs and accumulates bounded per-repository dirty state. The existing single background scanner reads only dirty repositories after a 300ms quiet period (at most two seconds during sustained writes). A 30-second consistency scan covers missed events or unavailable native watching. Existing UI state and reviewed plans remain separate from refresh snapshots.

**Tech Stack:** Rust, notify 8, standard threads/mpsc, system Git, Ratatui.

---

### Task 1: Native notifications and scheduling

- Add `notify` to `Cargo.toml` / `Cargo.lock`.
- Add `src/commit/refresh/watch.rs` for watcher lifetime, native path routing, debouncing, and bounded dirty-root accumulation.
- Watch canonical worktrees and their Git/common directories, including submodules and linked worktrees. Disable following directory symlinks. Ignore access events and irrelevant Git object/log/lock traffic.
- Initialize watching before the first status read, outside the terminal event loop. Retain pending events across tab changes, dialogs, and active scans; stop watching when the panel is dropped.
- Use unit tests for event ownership, rename paths, shared metadata, bounded debounce, and watcher failures.

### Task 2: Refresh only affected repositories

- Update `src/commit/refresh.rs` so snapshots carry repository IDs and merge only scanned repositories' status/errors/branches.
- Refresh the current Diff only if its repository was scanned. Preserve untouched repositories, selections, drafts, folds, and review snapshots.
- Keep one scan in flight; events arriving during a scan remain pending for the next scan. Manual refresh / ignored-file toggles invalidate old results.
- Make returning to Commit or regaining terminal focus drain queued changes without forcing an all-repository scan. Keep F5 as the explicit full refresh.
- Extend `src/commit/refresh/tests.rs` and `src/shell/workspace_tests/refresh.rs` to cover native events, idle polling, dirty-root isolation, staging, worktrees, failures, and stale results.

### Task 3: Verification and documentation

- Run focused refresh tests, then `cargo test --locked`, `cargo fmt --all --check`, and `cargo clippy --locked --all-targets -- -D warnings`.
- Use disposable repositories to check actual terminal refresh and idle Git command counts; verify HEAD, index, drafts, and unselected staged content stay intact.
- Update README, interaction design, architecture, PRD, and AGENTS handoff with debounce/fallback behavior and dependency rationale.
- Leave all work uncommitted.

### Verification results

- All 12 refresh tests passed, including native events, linked worktrees, idle focus changes, edits during a scan, and missed-event fallback.
- `cargo test --locked --quiet -- --test-threads=4`: 357 unit tests and 6 integration tests passed.
- Formatting and Clippy passed for the refresh changes. A subsequent all-targets Clippy run encountered concurrently added Branch code with a missing test module and an unused method; those unrelated edits were preserved.
- Built the terminal binaries and used a disposable three-repository practice workspace with Git Trace2: six idle seconds produced zero Git commands; a single edit started its affected-repository scan after about 438ms; ten rapid writes produced one status scan and one branch query for that repository only. The new file appeared without manual refresh; HEAD, real index, and tracked working files stayed unchanged.
