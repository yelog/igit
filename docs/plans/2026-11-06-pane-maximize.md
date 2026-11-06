# Pane Maximize Implementation Plan

> **For Claude:** Follow the steps below in the existing workspace; preserve unrelated pending changes. The user separately authorized committing this feature after implementation.

**Goal:** Maximize the focused IGit pane with `Ctrl-w f` and synchronize Kitty's stack layout using an optional smart binding, configured locally as `Cmd+Ctrl+F`.

**Architecture:** The shell owns a session-only maximize flag. Each panel renders its focused pane across the available work area while retaining its split sizes and logical pane order. The existing bounded Kitty worker discovers the source window's tab and layout groups, toggles stack/previous layout, and verifies the result before the shell changes local state.

**Tech Stack:** Rust, Ratatui, Crossterm, TOML, serde_json, Kitty remote control.

### 1. Local pane maximize

- Modify `src/ui/panes.rs`, `src/shell/panes.rs`, `src/shell/workspace.rs`, and `src/shell.rs` for maximize state, routing, resize suppression, and status.
- Adapt `src/commit/view.rs`, `src/log/view.rs`, `src/push/view.rs`, and `src/merge.rs` to render only the focused pane and clear hidden mouse targets.
- Register `toggle-pane-maximized = ["Ctrl-w f"]` in `src/config/keys.rs` and `config/default.toml`; keep the prefix picker and help executable.
- Add shell tests for each pane, Tab traversal, size restoration, narrow screens, overlays, and hidden mouse targets.

### 2. Kitty synchronization

- Extend `src/shell/kitty.rs` with a bounded asynchronous maximize operation and explicit target state.
- Resolve the tab containing `KITTY_WINDOW_ID`, count layout groups rather than overlays, use `goto-layout stack` or `last-used-layout`, and verify with `ls`.
- Register unbound `smart-toggle-pane-maximized`; consume press/repeat/release safely in editors and reviews, execute only a normal-mode press.
- Test tab targeting, single-group local fallback, stack restoration, malformed responses, failure, timeout, and cancellation without contacting users' terminal sessions.

### 3. Documentation and personal configuration

- Update `README.md`, `docs/configuration.md`, `docs/design.md`, `docs/prd.md`, and `docs/architecture.md` with behavior and restore controls.
- Back up `~/.config/igit/settings.toml` and `~/.config/kitty/kitty.conf` before adding the smart binding and the `IS_IGIT`-scoped `Cmd+Ctrl+F` forwarding rule.
- Validate with `igit --check-config` and reload Kitty configuration. Preserve existing LazyDB and Neovim shortcuts.

### 4. Verification

- Run `cargo test --locked`, `cargo fmt --all --check`, and `cargo clippy --locked --all-targets -- -D warnings`.
- Build both binaries and perform a real-terminal smoke check in a temporary practice workspace, including enter/restore and local configuration.
- Where available, test synchronized stack/restore in an isolated Kitty instance; leave existing user tabs and repository data untouched.

### Observed completion

- Implemented local maximize and Kitty synchronization, including configurable/executable help and repeat/input guards.
- Full tests passed: 287 unit tests and 5 integration tests. The 16 maximize-related tests were rerun after switching the configuration example to direct Cmd+Ctrl+F.
- Formatting, whitespace checks, release builds of both binaries, and library/binary Clippy passed in the combined working tree. The isolated commit snapshot also passed all 251 unit tests, 5 integration tests, formatting, and all-target Clippy; unrelated pending changes remain outside the commit.
- Backed up both personal configuration files, configured direct Cmd+Ctrl+F passthrough, validated both binaries' settings, and reloaded Kitty.
- Real Kitty checks passed for synchronized maximize/restore, held-key repeats, input/review guards, and single-window fallback. Practice Git state remained unchanged; the isolated test instance was closed.
