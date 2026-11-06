# Branch Navigation and Presentation Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Make Branch navigation consistent with Log and make repository, branch type, and upstream relationships easier to scan.

**Architecture:** Keep branch snapshots and reviewed Git operations intact. Store explicit fold overrides in the tree, use the rendered list height for navigation, and integrate `gg` with the existing shell sequence picker. Render upstreams in a shared cell-aligned column, reusing Log's reference colors without adding dependencies or persistence.

**Tech Stack:** Rust, Ratatui, Crossterm, existing Unicode cell metrics, temporary Git fixtures.

---

### Task 1: Folding and navigation

**Files:** `src/branch.rs`, `src/branch/tree.rs`, new `src/branch/navigation.rs`, `src/branch/keymap.rs`, `src/config/keys.rs`, `src/shell.rs`, `src/ui/key_sequence.rs`, Branch and Shell tests.

1. Cover default remote collapse, retained explicit folds through refresh/Roots changes, `gg` interruption, viewport H/M/L, full/half paging, resize, and batched input.
2. Default only remote top-level groups to closed; retain user overrides by stable row ID.
3. Add `gg` and its hint/cancellation lifecycle. Keep Home/End and PgUp/PgDn aliases. Register configurable commands and help.
4. Run `cargo test --locked branch` and relevant configuration/sequence tests.

### Task 2: Visual hierarchy and alignment

**Files:** `src/branch/view.rs`, `src/branch/tree.rs`, `src/ui/idea.rs`, `src/log/view.rs`, Branch presentation tests.

1. Add repository header surfaces, branch counts, shared local/remote branch glyphs, and Log's green/purple reference colors.
2. Align upstreams independently of name width, current markers, folder depth, and Unicode cells. Give the current branch and unavailable upstream/worktree states distinct emphasis.
3. Keep full focused names in details; clip labels safely and prioritize branch names in narrow terminals. Disable action buttons that cannot apply to the focused row.
4. Verify rendered cells, mouse hit areas, icon modes, empty states, and 44/80/120-column layouts with TestBackend.

### Task 3: Documentation and regression checks

**Files:** `README.md`, `docs/design.md`, `docs/prd.md`, `docs/architecture.md`.

1. Document defaults, all navigation bindings, presentation, and sequence ownership.
2. Run `cargo fmt --all --check`, `cargo test --locked`, and `cargo clippy --locked --all-targets -- -D warnings`.
3. Inspect a rendered representative multi-repository fixture and exercise a temporary workspace in a PTY. Do not commit or push.
