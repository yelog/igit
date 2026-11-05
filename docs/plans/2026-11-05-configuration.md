# User Configuration Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add optional TOML application settings and contextual command keybindings, following LazyDB's embedded-defaults and partial-overrides model.

**Architecture:** Embed `config/default.toml`, recursively merge user tables, replace arrays, and validate before terminal initialization. Resolve configured keys to the existing reviewed action paths in Shell; preserve text entry, confirmation controls, and unmodified shortcuts. Keep application preferences separate from workspace drafts and Git configuration.

**Tech Stack:** Rust, Serde, TOML, Crossterm, Ratatui, existing temporary Git fixtures.

---

### Task 1: Settings and command catalogue

Create `src/config.rs`, `src/config/keys.rs`, and `config/default.toml`; update `Cargo.toml`, `Cargo.lock`, and `src/lib.rs`.

1. Define version 1 settings for terminal mouse capture, UI icons, sequence hint delay, and grouped normal-mode commands.
2. Add a strict key parser (named keys, modifiers, space-separated sequences), partial merge, platform/environment path resolution, and missing/invalid file handling.
3. Validate unknown names, invalid values, duplicate bindings, and ambiguous prefixes in overlapping contexts. Keep Escape, F1, and text-selection controls available.
4. Test defaults, overrides, unbinding, parser normalization, context-specific conflicts, and file/path behavior with `cargo test --locked config::`.

### Task 2: Startup and contextual input

Modify `src/shell.rs`; create `src/shell/configuration.rs`; update `src/shell/panes.rs` and `src/shell/workspace.rs`.

1. Add `--config`, `--check-config`, `--print-default-config`, and `--mouse`; preserve `--icons` precedence and non-UTF-8 file paths.
2. Load settings before discovering repositories or entering raw mode. Preserve settings when opening another workspace.
3. Route changed shortcuts to existing built-in command sequences, suppress replaced bindings, and preserve input/modal handling and Git review boundaries.
4. Support custom multi-key prefixes, cancellation, and focus/resize/paste invalidation. Preserve existing window counts for unmodified window shortcuts.

### Task 3: Help and sequence hints

Modify `src/ui/help.rs` and `src/ui/key_sequence.rs`.

1. Display and search effective key labels while retaining the trusted original command identity for help execution.
2. Show custom sequence continuations using the existing picker layout and configured delay; use the same command path for typing, Enter, and mouse selection.
3. Ensure global status hints do not advertise replaced keys.

### Task 4: Regression checks and documentation

Create `src/shell/configuration_tests.rs` and `docs/configuration.md`; update `README.md`, `docs/design.md`, `docs/architecture.md`, and `docs/prd.md`.

1. Test remapping, swaps, disabling, shared prefixes, input isolation, help execution, rescan persistence, and confirmation boundaries in temporary repositories.
2. Verify HEAD, real index, and working tree remain unchanged after browsing and canceling a remapped review with mixed staged/unstaged content.
3. Run `cargo test --locked`, `cargo fmt --all --check`, and `cargo clippy --locked --all-targets -- -D warnings`.
4. Document paths, precedence, exact settings, reserved/modal keys, examples, and startup-only loading. No commits or external repository writes are part of this task.

## Outcome

Implemented all four tasks in the current workspace. The configurable surface
uses the existing action/review handlers; editors and confirmations remain
context-owned. Temporary-repository tests cover replaced keys, swaps, disabling,
shared prefixes, selection-copy precedence, window counts, context changes,
help execution, rescan persistence, and unchanged Git state after review cancel.
The native CLI and three pseudo-terminal sizes were also exercised; see the
configuration acceptance record in `docs/prd.md`.
