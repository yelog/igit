# Log List Vim Navigation and Copying Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Scroll the commit list only when selection crosses a visible edge, and support gg, H/M/L, full-page and half-page movement, and yy/ym/ya copying.

**Architecture:** Log stores the commit list's vertical offset and actual available row count, correcting the viewport whenever selection changes and during drawing. A new navigation module handles key sequences in the commit list. Copying passes through a Panel Action to Shell and the system clipboard. Read the full commit message only for ym, independently of the asynchronous preview.

**Tech Stack:** Rust, Ratatui TestBackend, Crossterm, system Git, and system clipboard commands; no new dependencies.

---

### Task 1: Viewport and navigation regression coverage

**Files:** `src/log.rs`, `src/log/navigation.rs`, `src/log/view.rs`, `src/log/view_tests.rs`.

1. Add combined rendering and event tests: j/k scroll only at an edge; batched input retains its order; the two-key gg sequence jumps to the first row; G jumps to the last; H/M/L move only among currently visible commits; Ctrl-F/B/D/U page by the actual row count.
2. Run `cargo test --locked log::view_tests::navigation_` and confirm the new tests expose the current behavior.
3. Preserve the vertical viewport and keep the selected item visible when selection or window size changes. Move a full page by the available row count and a half page by half that count (at least one row), clamped to the boundaries. PageUp/PageDown use the same full-page behavior.
4. Handle g/y prefixes; other operations, focus changes, and input overlays cancel pending sequences. H/L gain their new navigation meanings in the commit list; display features retain toolbar and menu access.

### Task 2: Copy the selected commit

**Files:** `src/log/navigation.rs`, `src/log/view_tests.rs`, `src/ui.rs`, `src/ui/clipboard.rs`, `src/shell.rs`.

1. Add yy/ym/ya cases to verify the full OID, message including its body, and name/email. In particular, cover moving to another repository before its details have loaded. Empty lists cannot copy; invalid sequences must not trigger filtering or accidental copying.
2. Pass the text and success notification through `Action::Copy`. Shell writes the text through stdin to macOS pbcopy, Windows clip, or Linux wl-copy/xclip/xsel. Report failures clearly without logging the copied content.
3. Verify clipboard bytes and failure handling with a temporary receiver command, without changing the test runner's real clipboard.

### Task 3: Documentation and validation

**Files:** `README.md`, `docs/design.md`, `docs/prd.md`, `docs/architecture.md`, `src/shell.rs`.

1. Update application help, key tables, viewport and copy boundaries, and reproducible acceptance criteria.
2. Run `cargo test --locked log::`, then `cargo test --locked`, `cargo fmt --all --check`, and `cargo clippy --locked --all-targets -- -D warnings`.
3. Modify only files related to this task; preserve existing workspace changes and do not commit or push.
