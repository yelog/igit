# Pane Search Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Find and highlight text in the ten requested workbench panes with `/`, `n`, and `N`.

**Architecture:** A shared search controller edits a literal query, keeps its current occurrence, and paints matches without altering source bytes. Each pane supplies its loaded rows in display order and reveals the matched row through its existing navigation; search neither filters the tree nor runs Git to discover more data.

**Tech Stack:** Rust, Crossterm, Ratatui, system Git fixtures, and TestBackend.

---

1. In `src/ui/search.rs`, cover Unicode and literal matching, repeated hits in one row, wraparound, cancellation, and narrow rendering. Add the shared prompt to the existing footer in `src/shell/workspace.rs`. Give the editor first refusal on typed characters through each panel's `editing()` state.
2. In `src/commit/`, feed the Changes tree's expanded row order and the selected file's loaded patch to the shared search. Reveal only the relevant ancestors; use the existing text layout to scroll to patch hits, including long lines. Keep Rollback and hunk navigation independent.
3. In `src/log/refs.rs` and `src/log/inspector.rs`, find ref and file labels within their existing scope. Reuse the inspector search through `src/log/snapshot.rs` for Stash. Keep Log's commit filter and `S` ref filter, and leave the standalone revision comparison unchanged.
4. In `src/push/`, `src/branch/`, and `src/stash/`, wire the outgoing commits, changed files, repository/branch rows, and saved stash list. Preserve checkbox state, operation reviews, and existing preview loading. Folded rows participate; reveal only the ancestors of the selected result.
5. Add context help and TOML bindings. Give Commit's New Changelist `Ctrl-N` so `n` is available in Changes and Preview. Document the supported panes, Escape behavior, and scope in the README, design, PRD, and architecture.
6. Use temporary Git fixtures and terminal buffers to verify both focus routing and navigation in the actual panes. Run `cargo test --locked`, `cargo fmt --all --check`, and `cargo clippy --locked --all-targets -- -D warnings`.

### Validation results

- The ten pane search tests cover all ten panes, literal and Unicode matches,
  folding, duplicate filenames, repeated matches, long patches, help, and key
  remapping. Git fixtures verify that browsing retains the real index,
  working files, and refs.
- `cargo test --locked --quiet -- --test-threads=6` passed 675 tests; one
  manual performance test remained ignored. Format and strict Clippy checks
  passed.
- In a native terminal at 44×12 and 80×25, `/index.vue` found two files in
  Changes, `n` and `N` moved between them, and the Diff preview found patch
  text. The disposable repository's HEAD, index, and worktree were unchanged.
