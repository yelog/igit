# Git Log View Options Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Implement the display items, Columns submenu, and commit highlighting from IDEA's eye menu at the top right of the native Log area.

**Architecture:** Centralize display preferences and menu state in `src/log/display.rs`, reusing Log's drawing, hit testing, and modal input dispatch. Read the author email and both dates with history; query signatures and cherry equivalence only when enabled. Share display options across Log tabs in the same session. Toggling an option does not change filters, commit order, or selection, write Git state, or add dependencies.

**Tech Stack:** Rust, Ratatui TestBackend, Crossterm, system Git.

---

### Task 1: Menu access and keyboard/mouse interaction

**Files:** `src/log/display.rs`, `src/log.rs`, `src/log/view.rs`, `src/log/filters.rs`, `src/log/view_tests.rs`, `src/shell.rs`.

1. Add TestBackend cases for the eye button, menu groups, and Columns submenu at 48/80/120/180 columns. Run `cargo test --locked log::view_tests::view_options` and confirm that the new features are not yet present.
2. Reserve an eye button at the right end of the graph toolbar; in single-region mode, put it at the right end of region navigation. `o` opens it, `j/k` select, `l/Right` enters a submenu, `h/Left` returns, `Space/Enter` toggle, and `Esc` returns or closes. Mouse clicks, hover, and keyboard input share state.
3. In narrow windows, place the submenu to the left of or on top of the main menu. Scroll the current menu in short windows. The menu surface intercepts clicks on commits underneath; clicking outside closes it and continues handling that click. The Shell header can still directly switch panels or Roots.

### Task 2: Real display data and highlighting

**Files:** `src/log/model.rs`, `src/log/model/tests.rs`, `src/log/model/presentation.rs`, `src/log/view.rs`, `src/log/model/graph.rs`, `src/log/model/graph/render.rs`, `src/log/view_tests.rs`.

1. Read `%ae`, `%ad`, and `%cd`; preserve `%ct` sorting semantics. Match My Commits against each repository's identity. When the signature column is enabled, read `%G?` and distinguish valid, unsigned, missing key, expired, and invalid signatures.
2. Implement Root Names, Compact References View, Tag Names, and References on the Left. Keep tag icons when tag names are hidden. Independently control Author, Hash, Date, and GPG Signature while preserving room for the commit title.
3. With Long Edges disabled, shorten branch lines beyond the threshold with arrows that can jump to the other end. Keep shared lanes complete when other branches join midway. Enabling the option restores complete lanes; filtered gaps must not create artificial edges.
4. Independently toggle bold My Commits, dimmed Merge Commits, and blue-background Current Branch. After another branch is selected, use `git cherry HEAD <branch>` to distinguish commits already applied equivalently and dim them. Show an explicit unavailable hint when no comparable branch exists.
5. Use temporary real repositories to verify both dates, per-repository identities, signatures, cherry-equivalent commits, and isolation across repositories sharing a SHA. Verify that display toggles preserve the current selection and Git state.

### Task 3: Documentation and regression checks

**Files:** `README.md`, `docs/design.md`, `docs/prd.md`, `docs/architecture.md`.

1. Document menu items, keyboard/mouse actions, defaults, session sharing, narrow-screen behavior, and the on-demand boundaries for signature/cherry queries.
2. Run `cargo test --locked log::` and fix related regressions.
3. Run `cargo test --locked`, `cargo fmt --all --check`, and `cargo clippy --locked --all-targets -- -D warnings`. Check actual terminal output with temporary demo history; do not commit or push.
