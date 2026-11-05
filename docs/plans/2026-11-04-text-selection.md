# Text Selection and Copying Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Allow original text to be selected and copied in Log details and Diff, the Commit message editor and preview, and Push Details / Diff.

**Architecture:** Add shared text layout and selection components. Store selection as byte positions in the original text, mapping terminal cells, soft wrapping, and scrolling back to that same source. Panels own only region focus and lifecycle. Copying reuses `Action::Copy` and the system clipboard; Shell gives the panel priority for Ctrl-C when a text selection exists. Editors reuse selection for mouse interaction, Shift navigation, Ctrl-A, and replacing selected text.

**Tech Stack:** Rust, Ratatui 0.29 / TestBackend, Crossterm, existing Unicode cell-width support and clipboard implementation; no new dependencies.

---

### Task 1: Shared layout and selection

**Files:** `src/ui/selection.rs`, `src/ui.rs`.

1. Cover forward/reverse selection, multiple lines, empty lines, Chinese/combining characters, horizontal and vertical scrolling, soft wrapping, edge dragging, and invalidation when source text changes.
2. Implement shared layout projection, selection highlighting, mouse hit testing, and copy actions. Do not include soft wraps in copied output; preserve original indentation and line breaks.
3. Run `cargo test --locked ui::selection::`.

### Task 2: Panel integration and editors

**Files:** `src/log.rs`, `src/log/inspector.rs`, `src/log/preview.rs`, `src/commit.rs`, `src/commit/view.rs`, `src/push.rs`, `src/push/view.rs`, `src/shell.rs`, and corresponding view tests and keymaps.

1. Integrate shared layout and selection in Log details, Log Diff, Commit Diff/summaries, and Push details. Dragging text must not activate links, tree drag-and-drop, or divider resizing.
2. Select by dragging with the left mouse button; on release, copy once and clear the selection. Single clicks, empty selections, and repeated releases do not copy. Keep the editing cursor at the selection endpoint. Keyboard selections retain Ctrl-C / Ctrl-Insert and y in read-only regions. Esc clears a keyboard selection before following the existing close behavior.
3. Commit messages support Shift+arrows/Home/End, Ctrl-A, copying, and replacement of selected text by typing/pasting/deletion. Preserve existing save and cancel behavior.
4. Clear stale hit targets and selections when switching objects, opening covering dialogs, or hiding regions. Scrolling the same text preserves selection. Shell's Ctrl-C retains its existing exit behavior when there is no selection.
5. Use TestBackend at 120×36, 80×24, and 44×12 to validate events, visible highlighting, message drafts, and read-only Git behavior. Tests do not access the system clipboard.

### Task 3: Documentation and regression checks

**Files:** `README.md`, `docs/design.md`, `docs/prd.md`, `docs/architecture.md`.

1. Explain selection, copying, cancellation, edit replacement, and terminal shortcut constraints; add acceptance cases.
2. Run `cargo test --locked`, `cargo fmt --all --check`, and `cargo clippy --locked --all-targets -- -D warnings`.
3. Preserve existing workspace changes. Do not commit, push, or perform repository history operations.

## Validation record (2026-11-04)

- `cargo test --locked`: 169 unit tests and 3 integration tests passed. New behavior was also verified with `cargo test --locked text_selection --lib` and shared selection tests.
- `cargo fmt --all --check`, `cargo clippy --locked --all-targets -- -D warnings`, `cargo build --locked --bins`, and `git diff --check` passed.
- TestBackend covered 120×36, 80×24, and 44×12. A real 120×36 PTY passed SGR mouse drag selection, Ctrl-C, Ctrl-A, Shift+Left, and copying in all five regions; highlighted ANSI output and normal exit were checked.
- Thread-local/temporary command receivers verified clipboard bytes without changing the real clipboard. HEAD, index, worktree, and operation scope in the temporary demo repositories remained unchanged.

### Interaction follow-up: Copy on release

- As additionally requested by the user, all five text regions now return a copy action and clear selection on left-button release, retaining the editing cursor endpoint. Ordinary clicks, empty selections, and repeated releases do not copy. Drag-selecting parent commits does not navigate.
- Full regression passed 172 unit tests and 3 integration tests; fmt, Clippy, build, and diff checks passed.
- A real 120×36 PTY verified that dragging does not copy; release writes the exact text to a temporary clipboard command without a copy key. Subsequent typing remains at the selection endpoint, and Git state is unchanged.

### Fix: Single click incorrectly copies after a previous copy

- Reproduced: the old selection was already collapsed and cleared, but the old implementation treated mouse down as dragging and remapped coordinates on release. The next press cleared the "Copied" notification and caused editor reflow. Identical screen coordinates then mapped to different source positions on Down/Up, creating a selection that did not exist.
- The shared component now records the press coordinates separately from actual dragging. Release may copy only after a Drag crosses a character-cell boundary. An ordinary click or movement inside one cell keeps the cursor from mouse down and does not recreate a selection.
- Added `click_after_copy` regressions for the shared component and Shell. Both failed before the fix; at 120×35, Shell copied twice instead of once. After the fix, reflow cases at 120×35, 80×25, and 44×13, viewport scrolling, release-coordinate jitter, and existing copy tests for all five regions passed.
- Full regression passed 176 unit tests and 3 integration tests; fmt, Clippy, build, and diff checks passed. A real 120×35 PTY rechecked "drag to copy → notification appears → click again and trigger reflow → continue editing". The temporary clipboard receiver file's content and modification time remained unchanged; subsequent editing, normal copying, and Git state checks passed.
