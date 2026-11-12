# Log Branch Search Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Focus Log's existing Branch or tag input with `/` in the branch tree and remove its duplicate footer search.

**Architecture:** Route `/` and the existing `S` alias through `begin(Field::Refs)`. Remove the branch tree's pane-search state, event handling, and highlights while preserving its live filtering, scoped references, fold restoration, and per-tab query. Restrict Log's pane-search commands to its Diff file tree.

**Tech Stack:** Rust, Crossterm, Ratatui, TestBackend, disposable Git fixtures.

---

### Task 1: Cover the branch search route

**Files:** `src/log/refs.rs`, `src/log/view_tests/panes.rs`, `src/shell/log_tests.rs`.

1. Replace the old branch pane-search test with coverage for `/` focusing the existing input, live filtering folded branches and tags within Roots, and leaving history unchanged.
2. Verify keyboard and mouse input, rendered focus, cancellation, and reapplied queries at 120×36, 80×24, and 44×12.
3. Cover Shell help, text-input guards, custom `search-refs` bindings, and preserved HEAD, index, and working files.
4. Run `python3 scripts/cargo_serial.py test --locked --lib slash_filters_log_refs` and confirm the old implementation fails.

### Task 2: Unify search and remove obsolete state

**Files:** `src/log.rs`, `src/log/refs.rs`, `src/log/filters.rs`, `src/log/loading.rs`, `src/log/file_history.rs`, `src/log/keymap.rs`, `src/config/keys.rs`, `config/default.toml`.

1. Reuse the existing branch input for `/` and retain `S`.
2. Delete branch pane-search state, snapshots, invalidation, match jumps, and highlights; preserve narrow branch labels.
3. Restrict `find`, `next-match`, and `previous-match` to Log's Diff file tree and make `/` a default `search-refs` binding.
4. Show the branch search action in contextual help without a duplicate Pane Search section.

### Task 3: Document and verify

**Files:** `README.md`, `docs/design.md`, `docs/prd.md`, `docs/architecture.md`, `docs/configuration.md`.

1. Document branch input behavior and distinguish it from Diff file-tree pane search.
2. Run focused Log, Shell Log, help, and configuration tests through `scripts/cargo_serial.py`.
3. Run formatting, strict Clippy, and `git diff --check`; review only task-related changes without committing.

## Verification — 2026-11-11

- The focused `slash_filters_log_refs` regression failed against the old footer-search route and passed after unifying the branch input.
- `python3 scripts/cargo_serial.py test --locked --lib log::`: 211 passed.
- `python3 scripts/cargo_serial.py test --locked --lib shell::log_tests`: 6 passed.
- `python3 scripts/cargo_serial.py test --locked --lib shell::help_tests`: 18 passed.
- `python3 scripts/cargo_serial.py test --locked --lib config`: 66 passed, including configuration parsing and Shell shortcut remapping.
- TestBackend covered 120×36, 80×24, and 44×12 with ASCII, Unicode, and Nerd Font icons. Coverage includes live Unicode filtering, scoped folded branches and tags, no matches, cancellation, retained queries, literal slashes during editing, mouse entry, Help, independent remapping, and unchanged history selection.
- Shell fixtures preserved HEAD, real index bytes, staged and unstaged content, and working files.
- `python3 scripts/cargo_serial.py fmt --all --check`: the branch-search changes have no remaining formatting differences. The global check still reports concurrent User filter work in `src/log/authors.rs`, the author section of `src/log/filters.rs`, and `src/log/view_tests/author_menu.rs`; those unrelated changes were preserved.
- `python3 scripts/cargo_serial.py clippy --locked --all-targets -- -D warnings`: blocked by `clippy::int_plus_one` in the concurrent `src/log/authors.rs` work and the existing `clippy::obfuscated_if_else` in `src/shell.rs`.
- `python3 scripts/cargo_serial.py clippy --locked --all-targets -- -D warnings -A clippy::int_plus_one -A clippy::obfuscated_if_else`: passed, checking all targets while allowing only those two unrelated diagnostics.
- `git diff --check`: passed.
- The full repository test suite and an interactive native-terminal walkthrough were not run.
