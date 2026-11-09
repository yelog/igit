# Commit Split Preview Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Selecting a file in Commit shows the same read-only Before/After comparison as file history, while containers retain their scope overview.

**Architecture:** Read the exact commit candidate from HEAD and the real index for staged files, or from the index and working copy for unstaged files. Feed bounded UTF-8 sources into the existing `RevisionDiff` inline renderer. Keep an explained Git-patch fallback for unsupported content and preserve browsing identity, checked paths, drafts, and Git state.

**Tech Stack:** Rust, Ratatui, Crossterm, system Git, existing revision rendering and pane-search helpers.

---

1. Add bounded read-only source loading in `src/commit/backend/preview.rs`, reuse shared revision validation from `src/core/revision.rs`, and cover staged/unstaged/add/delete/unborn/literal-path cases with temporary repositories.
2. Add Commit preview composition and input routing in `src/commit/preview.rs`, `src/commit.rs`, and `src/commit/view.rs`. Retain the existing pane focus cycle; `[` / `]` switches Before/After within Preview, and `Esc` returns to Changes. Reuse diff navigation, folding, zoom, scrolling, and source-backed copying.
3. Preserve `/`, `n/N`, folded-result reveal, and horizontal search panning in `src/merge/view/revision.rs` with regression coverage. Scope Shell's inline-diff key routing to Commit's focused file comparison.
4. Update `src/commit/refresh.rs` to load and compare the selected file's sources on its existing worker. Unchanged scans preserve comparison navigation and selection; stale results cannot replace another file or repository.
5. Update existing Commit preview/search/selection tests and add split-layout cases across icon modes and terminal sizes. Check HEAD, real index bytes, staged siblings, working bytes, checks, and shared drafts.
6. Update README, interaction design, PRD, and architecture. Run focused tests first, then `cargo test --locked`, `cargo fmt --all --check`, `cargo clippy --locked --all-targets -- -D warnings`, and `cargo build --locked --bins`. Exercise the built TUI in a disposable workspace.

No commit, push, publication, or changes to users' repositories are authorized.

## Verification

- Implemented all six steps. The shared revision viewer retains its read-only
  contract; Commit supplies the exact staged or working versions and keeps
  container Overview and explained patch fallbacks.
- `cargo test --locked` passed: 699 unit tests and 10 integration tests.
  The existing manual Add to Git performance test remained ignored.
- The source reader's 17 Git-fixture tests cover mixed staging, additions,
  deletions, unborn HEAD, intent-to-add, native paths, file/directory
  replacements, symlinks, modes, binary content, size limits, and conversions.
  They compare all repository file bytes, including index, refs, and objects.
- Revision search and copying passed in both color and `NO_COLOR` modes.
  Commit integration covers query retention across refresh/Overview/fallback,
  source copying, current checkbox/draft preservation, running-write guards,
  and file dragging through Preview.
- After the final footer/help adjustments,
  `cargo test --locked --lib shell::workspace_tests` passed all 18 tests.
- `cargo fmt --all --check`, `cargo clippy --locked --all-targets -- -D warnings`,
  `cargo build --locked --bins`, and `git diff --check` passed.
- Real PTY smoke checks used `examples/demo-commit.sh`: `igit` at 180×36 and
  120×36 showed Before/After; `ig` at 44×12 switched the visible revision.
  Focused source search and fold/change controls worked. The mixed staged
  script excluded later working edits. Each case quit without confirmation,
  restored terminal modes, and preserved every repository file byte.
- Verified relative links in the changed preview documentation. No staging,
  commit, push, release, or changes to the Neovim companion were performed.
- Concurrent AI configuration edits appeared after verification. Final scoped
  rustfmt checks for every preview-related Rust file and `git diff --check`
  passed; a final repository-wide formatting check reports a wrapping change
  in the independently edited `src/config.rs`. Those edits were preserved.
