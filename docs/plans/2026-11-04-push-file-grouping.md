# Push File Grouping Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Provide IDEA's Group By menu in the Push right pane, with independent Directory, Repository, and Module toggles, all enabled by default.

**Architecture:** Store session options in the Push panel, project the current hierarchy from the existing file tree's stable node IDs, then apply collapse state. Files still reference original changes returned by Git; the menu does not participate in push planning. Follow Commit's eye-button, keyboard, and mouse conventions.

**Tech Stack:** Rust, Ratatui, Crossterm, system Git; no new dependencies.

---

1. **Grouping and navigation** (`src/push/files.rs`): use duplicate filenames to cover all eight combinations, parent navigation, and positioning after collapse. Project in repository, content root, compacted directory order. Without directories, flatten by filename and path. Run `cargo test --locked push::files::`.
2. **Menu and details** (`src/push.rs`, `src/push/display.rs`, `src/push/view.rs`, `src/push/keymap.rs`, `src/shell.rs`): open the menu with the toolbar eye button or `o`; choose items with arrows or mouse, toggle immediately with Space / Enter, and close with Esc. In flat mode, show the parent directory beside each filename. Preserve the patch and scroll position for the same file; retain grouping when switching repositories/commits.
3. **Acceptance** (`src/push/view_tests.rs`, `src/push.rs`, `src/shell/help_tests.rs`): verify all three icon modes, narrow-screen hit testing, and that menu input does not accidentally check repositories. Check duplicate names, rename patches, and Git content in real temporary repositories. Update `README.md`, `docs/design.md`, `docs/prd.md`, and the module index. Run `cargo test --locked`, `cargo fmt --all --check`, and `cargo clippy --locked --all-targets -- -D warnings`.
