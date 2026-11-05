# Kitty Smart Resize Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Share Cmd+Ctrl+Shift+h/j/k/l pane resizing across IGit, Kitty, LazyDB, and Neovim.

**Architecture:** Four opt-in native commands move a visible divider by three cells in the requested direction. Prefer the nearest right/bottom divider, otherwise the left/top divider; fall back to Kitty when that divider is unavailable or at its limit. Reuse the bounded Kitty worker and install an IGit-specific helper that explicitly resizes the originating window without changing focus.

**Tech Stack:** Existing Rust/Ratatui/Crossterm, standard-library subprocess worker, Kitty's Python kitten API. No new production dependency.

### Task 1: Directional divider behavior

- Add geometry tests to `src/ui/panes.rs` for both sides, all directions, nested dividers, clamping, repeats, and absent/hidden dividers.
- Run `cargo test --locked smart_resize` and observe the missing implementation.
- Add a directional resize method that distinguishes absent dividers from clamped dividers, preserving existing Ctrl-W size controls.

### Task 2: Native commands and Kitty handoff

- Extend `src/config/keys.rs`, `src/config.rs`, and `config/default.toml` with unbound `smart-resize-pane-left/down/up/right` commands and single-modified-chord validation.
- Extend native event handling, executable help, terminal opt-in registration, and `src/shell/panes.rs`. Accept Press/Repeat, consume Release, and shield editors/reviews. Retain focus, selections, and Git state.
- Extend `src/shell/kitty.rs` to run `kitten @ kitten --match id:<source> igit_resize.py <direction> 3` through the existing cancellable worker.
- Add `contrib/kitty/igit_resize.py` and standard-library unit tests, including targeting an inactive window/tab, axis neighbors, stack layout, invalid inputs, and missing windows.
- Add shell tests for internal movement, Kitty fallback, tool isolation, editor/review protection, configured help, and resize-only configuration.

### Task 3: Local setup and verification

- Back up `~/.config/igit/settings.toml` and `~/.config/kitty/kitty.conf`; append four resize bindings and four `IS_IGIT` pass-through mappings. Install the helper in Kitty's configuration directory, preserving Neovim and LazyDB configuration.
- Update README, configuration guide, architecture, interaction design, and PRD with direction semantics, step size, helper installation, and verification.
- Run the targeted tests, helper tests, full Rust suite, formatting, Clippy, and build both debug binaries. Validate user settings and reload Kitty mappings.
- Verify isolated Kitty/Neovim/IGit sessions on a disposable Git fixture, including internal widths/heights, remote fallback, stable focus, and marker cleanup. Compare HEAD/index/diff/status, close test tabs, and restore the user's original tab.
- Do not commit or push.

### Completion

- Implemented and configured all four resize commands, native help, repeat
  handling, and guarded Kitty fallback. Fixed inner boundaries remain distinct
  from missing boundaries without changing the existing mouse/Ctrl-W controls.
- Installed the helper using Kitty 0.47.4's `window.tabref()` and explicit
  `resize_window_by` source ID. Backed up both user configuration files and
  reloaded Kitty; existing Neovim and LazyDB files were retained.
- Passed 227 Rust unit tests, 5 integration tests, 3 Python helper tests,
  formatting, Clippy, and both debug builds. Live horizontal/vertical
  Kitty/Neovim checks and unchanged Git state are recorded in the PRD.
