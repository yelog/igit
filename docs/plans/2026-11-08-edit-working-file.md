# Edit Working Files Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Press `e` or `E` on a Commit file to edit its working copy, using the configured editor standalone or the existing Neovim instance when embedded.

**Architecture:** Commit returns a typed file action for the highlighted row, independent of checkboxes. Shell owns editor handoff and terminal suspension. Standalone editing follows `git var GIT_EDITOR`; the Neovim plugin advertises a per-session OSC request bridge and handles file/window changes inside its parent editor.

**Tech Stack:** Existing Rust, Crossterm/Ratatui, system Git, Lua and Neovim TermRequest. No additional dependencies.

---

### Task 1: File action and native editor

- Modify `src/ui.rs`, `src/commit.rs`, `src/commit/keymap.rs`, `src/config/keys.rs`, and `config/default.toml`.
- Only a highlighted file in Commit Changes may request editing. Containers, other panes, message input and reviews retain their own keys. Resolve the selected repository and literal path without changing checkboxes or the Git index.
- Add `src/shell/editor.rs` for target validation, Git's configured editor, argument-safe file forwarding, exit errors, and the embedded request.
- Modify `src/shell.rs` to queue handoffs outside drawing, release terminal modes around an external editor, restore them on success/failure, and refresh after returning.
- Verify remapping/help, mixed staged/unstaged files, duplicate names across repositories, missing files, literal shell metacharacters, and failed editors with disposable fixtures.

### Task 2: Parent Neovim handoff

- Modify `/Users/yelog/workspace/vi/igit.nvim/lua/igit/init.lua` and add a focused editor integration module if useful.
- Set a session identifier in the terminal environment; accept only matching edit requests from a managed session.
- Schedule editor API calls outside TermRequest. Reuse existing buffers without discarding unsaved text. Floating windows hide only after a successful switch; splits stay visible and focus the source editing window; tab sessions remain available.
- Preserve the terminal job, Git selection, drafts and source context. Report unavailable targets and buffer-switch errors without hiding the active window.
- Extend headless integration tests with real terminal requests, float/split/tab behavior, modified buffers, failed switches, and malformed or stale requests.

### Task 3: Documentation and verification

- Update `README.md`, `docs/design.md`, `docs/architecture.md`, `docs/prd.md`, `docs/configuration.md`, and the plugin README/help.
- Run focused Rust and Neovim tests, the full Rust suite, formatting and Clippy, then build both binaries.
- Smoke-test an external editor under a PTY and `e` through the real Neovim plugin using temporary repositories. Verify the index and HEAD remain unchanged by opening/editing a working file.
- Use the local development binaries in the user's existing Neovim and Kitty configuration; do not publish a release or replace the managed installation.

### Research and decisions

- [Git editor selection](https://git-scm.com/docs/git-var.html): `GIT_EDITOR`, `core.editor`, `VISUAL`, `EDITOR`, then Git's compiled default. This preserves editor commands with flags and shell syntax.
- [Neovim terminal requests](https://neovim.io/doc/user/terminal/): a managed terminal can request a parent-editor action without another Neovim process or a private RPC server.
- Reusing established editors supplies existing undo, language support, encodings and unsaved-buffer behavior. A second general-purpose editor inside IGit would duplicate those responsibilities.

### Observed verification

- `cargo check --locked --bins` completed successfully. The focused Rust test binary compiled, but execution stalled before its test harness started.
- The Neovim suite passed 31 tests, including actual terminal-request delivery, float/split/tab handoff, retained terminal jobs, unsaved buffers, failed switches and stale requests. An additional Nvim 0.10/source-window recovery test was added; its rerun was blocked before Neovim startup.
- Git whitespace checks passed. Subsequent formatting, Clippy and native PTY attempts were blocked by macOS process startup. The native PTY produced zero bytes; a process sample showed only `_dyld_start`, before application/test code. These attempts are not recorded as passes.
- The existing local debug binaries contain the edit bridge. Current Kitty and Neovim launch configuration points to that local development build, preserving the managed installation. Restart Neovim to load the new plugin bridge.
