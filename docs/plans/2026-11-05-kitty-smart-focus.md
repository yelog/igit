# Kitty Smart Focus Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Use the user's existing Cmd+Ctrl+h/j/k/l shortcuts to navigate IGit panes and cross into neighboring Kitty windows, interoperating with Neovim and LazyDB.

**Architecture:** Four opt-in smart focus commands reuse current pane geometry. At an application edge, a bounded background `kitten @ action --match id:<source>` request selects the Kitty neighbor. Terminal setup negotiates modified keys and sets/clears `IS_IGIT`; modal editors and reviews consume these chords without moving or editing.

**Tech Stack:** Existing Rust/Crossterm/Ratatui, standard process/thread/channel APIs, installed Kitty remote control.

### Task 1: Native commands and local navigation

- Update `src/config/keys.rs`, `src/config.rs`, and `config/default.toml` with typed smart-focus commands, initially unbound. Require single modified chords and retain conflict validation.
- Update `src/shell/configuration.rs`, `src/shell/panes.rs`, and `src/ui/help.rs` so keyboard and help use the same native action, preserving original Ctrl-W behavior.
- Test internal neighbors, edge fallback, dialogs, editor text preservation, selection state, repeat/release handling, and help execution.

### Task 2: Kitty bridge and terminal lifecycle

- Add `src/shell/kitty.rs`: source-window targeting, one pending request, bounded subprocess lifetime, and nonfatal error reporting. Unit tests use local fake commands, never a user's Kitty session.
- Update `src/shell.rs`: negotiate keyboard enhancement; register `IS_IGIT` only for configured smart focus; restore flags and clear the marker on exit; poll background results.
- Document requirements and boundary behavior in the configuration guide, README, design, architecture, and PRD.

### Task 3: User configuration and integration verification

- Create `~/.config/igit/settings.toml` with the same four chords as `~/lazydb/settings.toml`.
- Back up `~/.config/kitty/kitty.conf`, then add four `IS_IGIT` conditional pass-through mappings. Preserve existing Neovim, LazyDB, resize, and maximize settings.
- Build both current debug binaries used by the user's existing launches. Validate configuration and reload Kitty after the implementation is ready.
- Verify a separate temporary Kitty test workspace, with a temporary Git fixture and Neovim splits. Confirm internal movement, both boundary directions, marker cleanup, and unchanged Git state. Restore the user's prior tab after testing.
- Run `cargo test --locked`, `cargo fmt --all --check`, and `cargo clippy --locked --all-targets -- -D warnings`. No commits or remote Git writes.

### Completion

- Implemented native commands, source-targeted Kitty requests, terminal cleanup,
  help integration, and workspace-rescan preservation without new dependencies.
- User settings written; Kitty configuration backed up and reloaded. Existing
  Neovim/LazyDB settings retained. Both debug entry points rebuilt.
- Passed 218 unit tests and 5 integration tests, formatting, Clippy, and live
  Kitty/Neovim checks. See the PRD verification record. No Git commit or push.
