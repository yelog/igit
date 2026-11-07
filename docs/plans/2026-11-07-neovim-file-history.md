# Neovim Integration and File History Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Embed IGit in Neovim with configurable windows, direct panel commands, and a file-history tab with an adjacent revision Diff.

**Architecture:** The Lua plugin launches the existing IGit executable in a Neovim terminal using an argument array. The native CLI selects a startup panel and resolves file-history targets; Log retains its normal tab and adds a file-scoped history view that reuses the existing graph and two/three-way read-only revision renderer.

**Tech Stack:** Lua / Neovim terminal APIs, Rust / Ratatui / Crossterm, system Git, existing test backends and temporary Git fixtures.

---

### Task 1: Neovim plugin

**Files:** Create `lua/igit/init.lua`, `plugin/igit.lua`, `doc/igit.txt`, `README.md`, and headless tests in `/Users/yelog/workspace/vi/igit.nvim`.

1. Implement `setup`, `open`, and `close`, plus `:Igit [commit|log|push|branch|stash] [file]` and a command-line-only lowercase abbreviation.
2. Use a list of process arguments, resolve `%` from the originating file buffer, and retain paths with spaces and Unicode.
3. Support float, left, right, bottom, and tab modes, custom executable/arguments, sizing, terminal startup, cleanup, resize, and return focus.
4. Verify all layouts and argument forwarding using headless Neovim and a temporary fake executable; document installation and real-binary usage.

### Task 2: Native CLI and startup

**Files:** `src/shell.rs`, `src/shell/startup.rs`, a new CLI parsing module and focused tests as needed.

1. Preserve existing directory/options/update usage while parsing direct panel commands and `igit log <file>`.
2. Resolve file paths from the invocation directory, including deleted tracked files, and identify their owning worktree without changing Git state.
3. Configure the file-history view before the background Log load; preserve responsive startup and existing global tab navigation.
4. Test every panel, literal option-like paths, malformed arguments, workspace paths, and startup selection.

### Task 3: File-history Log view

**Files:** `src/log.rs`, `src/log/model.rs`, `src/log/view.rs`, `src/log/preview.rs`, and focused new file-history modules/tests.

1. Add per-tab file identity while retaining the normal Log tab and its filters/selection.
2. Query only the owning repository with a literal path, retaining relevant merge commits and historical rename paths.
3. Render a closeable `History: <filename>` tab, compact branch toolbar, left commit graph/list, and right inline revision Diff; use two columns for ordinary commits and three for merges.
4. Preserve mouse/keyboard navigation, pane resizing, focus, synchronized Diff scrolling, asynchronous preview generation checks, and narrow-terminal behavior.
5. Test merge/ordinary/root/deleted/renamed files, spaces/Unicode, repository scope, tab lifecycle, and rendered layout without modifying Git state.

### Task 4: Integration and verification

**Files:** `README.md`, `docs/design.md`, `docs/architecture.md`, `docs/prd.md` and plugin documentation.

1. Document the CLI and Neovim setup plus terminal-cell rendering limitations relative to the supplied IDE screenshot.
2. Run targeted tests, then `cargo test --locked`, formatting and Clippy, plus plugin headless tests.
3. Build and run the real executable in Neovim against temporary practice repositories; inspect rendered layouts and cleanup.
4. Preserve all pre-existing uncommitted work; do not commit or push.
