# Show File at Revision Implementation Plan

**Goal:** Open the selected File History commit's complete file with an IDEA-style `filename (short hash)` title, using a read-only Neovim buffer when embedded.

**Architecture:** File History emits a typed revision request using the selected commit's historical path, independently of asynchronous Diff previews. Shell validates and reads immutable Git objects, then sends session-scoped metadata to the Neovim companion or opens a temporary snapshot with Git's configured editor. The companion reads the immutable blob asynchronously and preserves its running terminal session.

**Tech Stack:** Rust, Ratatui, system Git, existing tempfile/serde dependencies, Lua, Neovim terminal OSC requests.

### 1. Select the historical object

- Modify `src/log/file_history.rs`, `src/log.rs`, `src/log/keymap.rs`, `src/config/keys.rs`, `config/default.toml`, and `src/ui.rs`.
- Add `e` / `log.show-file-at-revision`, available in File History's list and inline preview, always targeting the selected commit.
- Resolve the commit's recorded historical path, preserve scope, and reject empty/stale histories.
- Add regression coverage in `src/log/file_history/tests.rs` for rename, deletion, selection changes before preview completion, and immutable Git state.

### 2. Prepare the editor handoff

- Extend `src/core/revision.rs` to distinguish absent files from empty blobs.
- Modify `src/shell/editor.rs`, `src/shell.rs`, and their tests.
- Validate a full immutable object ID and repository-relative path; reuse bounded UTF-8 blob reads.
- Emit OSC 51 `igit-show-revision` with JSON `{version:1,session,repo,path,revision}` (absolute repository, relative historical path, full commit ID).
- Standalone: retain a private temporary snapshot for the external editor's lifetime, pass its literal path as an argument, and remove it afterward.

### 3. Implement the Neovim consumer

- Work in `/Users/yelog/workspace/vi/igit.nvim`; keep changes separate from native files.
- Extend the existing terminal bridge, preserving session validation and delayed-focus safeguards.
- Open/reuse a listed, read-only scratch buffer identified by repository, full revision, and path; end its name with `filename (first eight hash characters)`.
- Detect syntax from the original path, preserve line endings, hide floats, and retain split/tab terminal jobs.
- Test with real headless Neovim and temporary Git fixtures; document usage.

### 4. Verify and document

- Run `python3 scripts/cargo_serial.py test --locked --lib log::file_history::tests`.
- Run focused editor, shell dispatch, configuration, and revision regressions.
- Run `nvim --headless -u NONE -l tests/run.lua` in the companion repository.
- Run `python3 scripts/cargo_serial.py fmt --all --check`.
- Update README, interaction design, PRD, architecture, and configuration documentation. Preserve pre-existing changes; do not commit or publish.

## Completed Verification

- Rust: 7 File History tests, 10 editor/dispatch tests, 12 configuration tests,
  and 3 immutable comparison tests passed.
- Neovim: 43 headless tests passed, including LF/CRLF/mixed endings, missing final
  newline, empty files, literal paths, stale requests, and preserved unsaved buffers.
- A real native IGit terminal embedded in headless Neovim passed `e`, rapid `je`,
  revision buffer reuse, syntax/title checks, retained terminal job, and unchanged
  HEAD/index/working-file/unsaved-buffer assertions.
- Serial Cargo format check, Clippy with warnings denied, and both binary builds
  passed. Both repository diffs passed whitespace checks.
- Changes remain uncommitted in the native and companion repositories.
