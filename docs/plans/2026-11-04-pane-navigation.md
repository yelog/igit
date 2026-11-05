# Workspace Pane Focus and Sizing Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Mark the region that actually receives input with a border, and use Tab, Ctrl-W direction keys, and border dragging within the same split workspace.

**Architecture:** Panels register accessible panes and actual dividers during rendering. Shell searches for targets only inside the current workspace or operation window. Sizes, drag capture, and pending key sequences remain session-only. Layout changes do not change repository selection or execute Git.

**Tech Stack:** Rust, Ratatui, Crossterm, TestBackend, local demo repositories.

---

1. Exercise complete keyboard and mouse paths in `src/shell/workspace_tests.rs`: cycle across Commit / Log, move by direction, stay put at boundaries, resize with keyboard and mouse, reset sizes, and isolate dialogs and editing. Use `cargo test --locked pane_navigation` to validate the old behavior.
2. Store per-frame panes, dividers, and valid dimensions in `src/ui/panes.rs`. Hit testing and keyboard resizing share limits. Cover shrinking windows, resizing back after reaching a limit, and drag capture in layout tests.
3. Register actual regions in Commit, Log, Push, and Merge views. Update border styles, dividers, blank-area hit testing, and compact layouts. Preserve each panel's selection, shortcuts, and tabs.
4. Connect focus across splits, dragging, and tool-dialog priority in `src/shell/workspace.rs` and `src/shell.rs`. Support executable Ctrl-W continuations in `src/ui/key_sequence.rs` and `src/ui/help.rs`.
5. Update README, design, PRD, and architecture documentation. Run `cargo fmt --all --check`, `cargo test --locked`, and `cargo clippy --locked --all-targets -- -D warnings`, plus a terminal smoke test in temporary demo repositories.
