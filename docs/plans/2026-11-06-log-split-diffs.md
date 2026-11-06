# Log Split Diff Entry Points Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Every text file opened from a two-parent merge uses three columns, and ordinary commits use two columns.

**Architecture:** Choose the viewer by commit parent count instead of tree-group identity. Extend the existing read-only revision viewer with a two-pane presentation that reuses alignment, highlights, folds and scrolling. Read all versions from immutable Git trees; represent the initial commit's missing parent and absent file sides explicitly. Preserve text selection/copy through a source-backed projection for aligned/folded rows.

**Tech Stack:** Rust, Ratatui, Crossterm, system Git, existing three-way alignment and text-selection helpers.

---

1. Update regression fixtures to enter files in both parent groups, including parent-only paths, and assert three revision headings and exact contents.
2. Add ordinary/root/add/delete commit fixtures and two-pane layout/navigation tests; ensure Tab never selects an invisible third side.
3. Replace group-dependent routing, add immutable two-version reads, and make the read-only revision layout support two or three panes.
4. Preserve drag/select-all/copy behavior using original source byte ranges across aligned padding, context folds, tabs and Unicode. Keep live conflict editing unchanged.
5. Update README/design/architecture/PRD to describe commit-based viewer selection. Run focused tests, the full suite and format/lint checks.
6. Render both layouts, then build both debug binaries from the complete workspace and verify the actual parent-group Enter and ordinary-commit Enter paths through a pseudo-terminal.

No commit or push is authorized by this request.

## Validation completed

- `cargo test --locked`: 323 library tests and 6 integration tests passed.
- Formatting, Clippy with warnings denied, and `git diff --check` passed.
- Real temporary Git fixtures cover parent-group Enter, parent-only paths, ordinary/root/add/delete commits, and preservation of unrelated staged/unstaged content.
- Native rendering verified two equally sized Before/After panes and three panes opened from the second parent group. Copy tests cover Unicode, tabs, CRLF, folded context, alignment gaps and resize.
- Both debug binaries were rebuilt from the complete workspace. Pseudo-terminal checks exercise ordinary-file Enter followed by merge-parent-group Enter at 180×40 (`ig`) and 80×24 (`igit`), retaining identical index and working-file hashes.
