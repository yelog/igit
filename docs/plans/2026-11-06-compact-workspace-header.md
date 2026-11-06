# Compact Workspace Header Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Combine the workspace header and context strip into one row and prefix the existing footer hints with IGit.

**Architecture:** Shell reserves the right side of one header row for workspace tabs and passes the remaining left side to the existing Log tab strip or Push title. Remove the old workspace name and Console / Roots / Actions / Help toolbar, including its mouse targets. Keep existing shortcut dispatch and contextual footer hints.

**Tech Stack:** Rust, Ratatui, Crossterm, existing TestBackend fixtures.

---

### Task 1: Simplify the shared workspace layout

- Modify `src/shell/workspace.rs`: return the unused rectangle from `draw_header`, remove `draw_toolbar`, and use vertical constraints of one header row, remaining content, and one footer row. Replace the mode badge with `IGit` while keeping notification and hint selection unchanged.
- Modify `src/shell.rs`: remove the obsolete toolbar hit collection and initialization. Keep full-screen conflict document rendering unchanged.
- Remove the now-unused toolbar-only glyphs from `src/ui/icons.rs`.
- Update existing regressions in `src/shell/workspace_tests.rs`, `src/shell/help_tests.rs`, `src/shell/maximize_tests.rs`, and `src/shell.rs` for the shared header, remaining help shortcut, modal/editor guards, and static footer branding. Check Log tab creation by mouse, workspace switching, and pane geometry at 120×36, 80×25, and 44×12.
- Run `cargo test --locked shell::` and resolve any layout-dependent regressions.

### Task 2: Document and verify

- Update `README.md`, `docs/design.md`, `docs/prd.md`, and `docs/architecture.md` to describe the single header, keyboard access to removed toolbar actions, and IGit footer.
- Run `cargo test --locked`, `cargo fmt --all --check`, and `cargo clippy --locked --all-targets -- -D warnings`.
- Inspect only this task's changes against the existing working-tree content; preserve unrelated edits. Do not commit or push.
