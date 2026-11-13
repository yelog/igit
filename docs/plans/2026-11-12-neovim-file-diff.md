# Neovim Current File Diff Implementation Plan

**Goal:** Open the current Neovim file against HEAD in the existing aligned diff renderer, center its originating line, and restore individual hunks without staging or losing unsaved edits.

**Architecture:** Add a focused native `igit diff --line N -- <file>` entry point backed by the existing local comparison and revision renderer. The Neovim companion supplies a private UTF-8 snapshot through `IGIT_DIFF_BUFFER`, opens the native viewer in a float, and imports reviewed restores into the original buffer when the viewer closes or hides, checking buffer identity and changedtick first.

**Tech Stack:** Existing Rust/Ratatui/Crossterm, system Git, Lua/Neovim, and temporary Git fixtures. No new dependencies.

### 1. Native entry point and guarded comparison

- Modify `src/shell/cli.rs`, `src/shell.rs`, `src/log/file_history/compare.rs`, and add `src/log/file_diff.rs`.
- Parse `diff`, require a file, and validate a positive one-based `--line`.
- Read HEAD directly rather than loading file history. Reuse local comparison validation, alignment, navigation, and hunk replacement.
- Standalone restores use the existing checked working-file write. Embedded restores update only the private buffer snapshot.
- Keep the real index, HEAD, unrelated staged content, permissions, and unaffected hunks unchanged. Review Apply All separately.

### 2. Center the source line

- Modify `src/merge/view/revision.rs` and its existing tests.
- Locate the requested Local line in aligned sections, expand its context fold, and center the resulting row after viewport dimensions are known.
- Cover unchanged context, unequal insertions/deletions, narrow views, empty files, and file boundaries.

### 3. Neovim companion

- Modify files only in `/Users/yelog/workspace/vi/igit.nvim`.
- Expose `:Igit diff`, `:IgitDiff`, and `require("igit").file_diff()`.
- Capture current buffer bytes and cursor before opening a floating viewer; preserve Unicode and literal paths through argv.
- Import restored bytes as an undoable buffer change without saving or staging. Reject stale imports and retain recoverable restored content.
- Reopening captures fresh buffer contents and the current cursor. Clean up snapshots and preserve existing terminal sessions.

### 4. Documentation and validation

- Update native README, PRD, interaction design, and architecture, plus companion README/help.
- Run one focused Rust test first through `python3 scripts/cargo_serial.py`; then relevant native tests, Neovim headless tests, formatting, and Clippy.
- Build both native binaries with `python3 scripts/cargo_serial.py build --locked --bins`.
- Exercise the real binary through headless Neovim on a disposable repository, including mixed staged/unstaged content, an unrelated staged file, unsaved buffer edits, hunk restore, undo, and centered source-line display.

## Implementation and verification

- Implemented the native focused viewer, Neovim snapshot/import lifecycle, safe recovery buffers, source-line centering, and contextual Help. BOM and final-newline restores participate in buffer undo/redo. Hidden viewers finish before snapshot import or removal.
- Native focused checks passed: `test --locked --lib file_diff` (11), `test --locked --lib merge::view::revision::tests::` (29), and the configuration, version, icon, discovery, state, and update CLI integration targets (10).
- Neovim checks passed: `tests/run.lua` (43), `tests/editor.lua` (5), `tests/hunks.lua` (16), `tests/blame.lua` (13), `tests/browse.lua` (17), and `tests/file_diff.lua` (17).
- The real-binary `tests/native_file_diff.lua` companion smoke test passed all eight checks: centered changed lines, scoped restores, undo, unchanged-line navigation, final-newline undo/redo, BOM undo, new-file-to-empty restore, and empty-buffer comparison. It verifies working bytes, HEAD, the real index, and unrelated staging. Run it with `IGIT_NATIVE_BINARY=/absolute/path/to/igit nvim --headless -u NONE -l tests/native_file_diff.lua`.
- Formatting and relative documentation links passed. Both debug binaries built successfully at `target-serial/debug/igit` and `target-serial/debug/ig`. All native Cargo commands used the serial wrapper.
- Full native regression reported 1,184 passed, seven failed, and one ignored. Each failure reproduced individually: Commit hunk/preview selection expectations, two tab-number expectations (`Remote` versus `Conflicts`), Log branch-search Help context, and Commit branch-label text. These existing interaction assertions were left unchanged.
- Standard all-target Clippy is blocked by `nonminimal_bool` in unchanged `src/commit/view.rs:109`. Allowing that lint exposed `needless_borrows_for_generic_args` in unchanged `src/branch/tests/smart_checkout.rs:355`. Production library/binary Clippy passed with only the known `nonminimal_bool` lint allowed; the standard check is not reported as passing.
