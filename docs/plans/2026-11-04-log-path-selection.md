# Log Paths Selection Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add line-by-line input and an expandable repository checkbox tree to Paths, allowing real commits to be filtered by multiple files or directories at once.

**Architecture:** Store relative paths and optional repository IDs in the Log tab's Filter; pass only applicable paths to Git for each repository. Keep immediate-apply Roots behavior in the menu. The two selectors maintain separate drafts and reload history only on confirmation. The tree reads only a directory's direct children when it is expanded.

**Tech Stack:** Rust, Ratatui, Crossterm, system Git; reuse the current test backend without new dependencies.

---

1. **Validate entry points and scope first.** In `src/log/view_tests/path_selection.rs`, create duplicate filenames, names containing brackets, and directories in temporary repositories. Use rendered hit targets and keyboard operations to verify menu order, multiline paste, the union of multiple paths, repository boundaries for tree selection, cancellation, and tab restoration. First reproduce the missing entry points with `cargo test --locked paths_menu_offers`.
2. **Store and query paths.** Parse literal relative paths in `src/log/model/path_filter.rs`, and build `git --literal-pathspecs log … -- path1 path2` per repository in `src/log/model.rs`. Add no paths for an entire repository. Skip a repository's history when selected paths belong only to other repositories. Preserve matching unqualified paths in every checked repository.
3. **Connect the selectors.** Implement the menu, multiline editor with a cursor, and directory tree in `src/log/paths.rs` and `src/log/paths/`. The tree has separate arrow and checkbox hit targets, partial selection, and keyboard expansion/collapse. Route drawing, keys, mouse, and paste through `src/log.rs`, `src/log/filters.rs`, and `src/log/view.rs`; provide corresponding help in `src/log/keymap.rs`.
4. **Acceptance.** Update README, interaction design, architecture, and PRD. Run `cargo test --locked paths_` first, then full tests, formatting, and Clippy. Confirm that small screens can still scroll to the last repository and that Apply and Cancel remain clickable.

### Validation record (2026-11-04)

- Full tests passed (182 unit tests and 3 integration tests). `cargo fmt --all --check`, `cargo clippy --locked --all-targets -- -D warnings`, and `cargo build --locked --bin igit` passed.
- Ran the Log demo workspace in a 140×32 pseudoterminal: pasted two lines and applied with `Ctrl-S`, then selected `atlas-web/src` in the tree. References, index entries, and worktree state remained unchanged in both repositories.
