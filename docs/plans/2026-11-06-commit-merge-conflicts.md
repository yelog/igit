# Commit Merge Conflicts Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Reproduce the reference's Merge Conflicts group, file styling, and Resolve link in the Commit changes list.

**Architecture:** Project scoped unmerged paths from Commit's existing Git status into one flat child group under Changes. Keep native file identities and saved changelist membership. Resolve navigates to the existing Conflicts tab; the commit, AI, and rollback eligibility rules remain in place.

**Tech Stack:** Rust, Ratatui, Crossterm, system Git.

---

### Task 1: Group and render unmerged paths

- In `src/commit/tree.rs`, count scoped conflicts in Changes and render them once, below a foldable `Merge Conflicts` row. Sort files like the flat Changes list. Keep rollback's existing tree projection.
- In `src/commit/rows.rs` and `src/commit/view.rs`, render the red filenames, type icons, gray parent paths, blue read-only marks, and an underlined Resolve link. Use the same cell range for rendering and hit testing. Keep XY visible when requested or when color is disabled.

### Task 2: Connect Resolve and verify selection

- In `src/commit.rs` and `src/commit/keymap.rs`, route Resolve clicks and Enter on the group through the existing tab action. Keep folding on the disclosure and left/right keys. Prevent changelist moves of conflicts while they appear in the dedicated group.
- Cover scoped grouping, a mixed group with an old checkbox value, and a real merge opened via Resolve in the Commit and Shell tests. Check that navigation does not change the working tree, index, or HEAD and that the group disappears after staging a resolution.

### Task 3: Document and verify

- Update `README.md`, `docs/design.md`, and `docs/prd.md` with the grouping, keyboard access, and eligibility rules.
- Run `cargo test --locked commit::`, the shell Resolve test, and the full suite. Check formatting and Clippy. Render a real conflict with the JavaScript and Markdown files from the reference.
