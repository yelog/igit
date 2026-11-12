# Push Toolbar and Shortcuts Implementation Plan

**Goal:** Put the update strategy at the right of the outgoing toolbar, expose action shortcuts inside labels, and restore complete focused pane borders.

**Architecture:** Keep Push's existing pane geometry, hit areas, scoped reviews, and Git operations. Render the strategy separately from left-aligned toolbar actions so it stays right-aligned, with shorter labels in narrow panes. Replace Push's strategy selector with an immediate, atomically saved Merge/Rebase toggle; Pull keeps its existing selector.

**Tech Stack:** Rust, Ratatui, Crossterm, TestBackend, disposable Git repositories and local bare remotes. No new dependencies.

## Task 1: Reproduce layout and shortcut defects

**Files:** `src/push/view_tests.rs`, `src/push/view_tests/controls.rs`, `src/push/view_tests/strategy.rs`, `src/shell/update_strategy_tests.rs`.

1. Add a focused rendering regression for complete bottom borders, focus colors, right-aligned strategy, and highlighted mnemonic cells.
2. Cover Nerd Font, Unicode, and ASCII at 38×10, 44×12, 80×24, 120×36, and 180×36, including Details and maximization.
3. Exercise F/H option toggles without changing checked repositories; verify disabled and remapped controls do not advertise stale shortcuts.
4. Run `python3 scripts/cargo_serial.py test --locked --lib push_panes_keep_complete_focus_borders` and record the initial failure.

## Task 2: Implement toolbar and footer

**Files:** `src/push/view.rs`, `src/push.rs`, `src/push/keymap.rs`.

1. Restore the shared pane renderer's complete borders for Outgoing commits and Changed files.
2. Remove the toolbar Target button; preserve per-row target editing and `t`.
3. Anchor `Update strategy: Merge/Rebase` to the toolbar's right edge, shortening it when space requires.
4. Accent and underline the matching s/f/P/F/H letters with Commit's existing gold style, preserving mouse hit areas and suppressing unavailable default mnemonics.
5. Remove the old strategy footer row and redundant separator; retain responsive option/action rows and hidden-selection notices.

## Task 3: Route shortcuts and save direct toggles

**Files:** `src/ui.rs`, `src/ui/help.rs`, `src/shell.rs`, `src/shell/configuration.rs`, `src/config/keys.rs`, `config/default.toml`, `src/shell/configuration_tests.rs`, `src/shell/push_tests.rs`.

1. Use f for Push target Fetch, F for Follow tags, and H for Hooks.
2. Restrict global Fetch to other panels and route Push's f through its existing fetch path, including configured keys and Git write guards.
3. Replace the obsolete Push-only strategy popup with one direct-toggle action. Adopt the new strategy only after its existing atomic save succeeds.
4. Preserve Pull's selector, shared preferences, review invalidation, and busy-operation restrictions.
5. Test default and remapped shortcuts, no-popup toggles, failed saves, and unchanged repository state.

## Task 4: Document and verify

**Files:** `README.md`, `docs/design.md`, `docs/prd.md`, `docs/configuration.md`, `docs/architecture.md`.

1. Update toolbar placement, full pane borders, direct strategy toggling, and default shortcuts.
2. Run focused Push rendering, strategy, Shell Push, configuration, and help regressions through `scripts/cargo_serial.py`.
3. Run formatting, Clippy, and `git diff --check`; report unrelated existing failures separately.
4. If available, inspect the built terminal app in a disposable practice workspace.
5. Preserve all existing workspace changes; do not commit or alter `target/`.

## Verification record (2026-11-12)

- The initial focused border regression failed with `│` where the bottom-left `└` should appear, confirming that the bottom border was omitted rather than covered.
- `python3 scripts/cargo_serial.py test --locked --lib push::view_tests:: -- --quiet`: 26 passed. Includes all three icon modes, five terminal sizes, complete focused borders, Details, maximization, compact toolbars, hidden selections, disabled/remapped mnemonics, and keyboard/mouse options.
- `python3 scripts/cargo_serial.py test --locked --lib shell::update_strategy_tests:: -- --quiet`: 6 passed. Covers direct keyboard/mouse toggles, remapping, ignored repeats/modifiers, busy guards, failed saves, shared persistence, and Pull's retained selector.
- `python3 scripts/cargo_serial.py test --locked --lib shell::push_tests:: -- --quiet`: 9 passed. Covers f versus F/H, configured sequences, target Fetch versus workspace Fetch, preserved selection, reviewed publication, cancellation, and unchanged repository contents.
- `python3 scripts/cargo_serial.py test --locked --lib shell::help_tests:: -- --quiet`: 18 passed.
- `python3 scripts/cargo_serial.py test --locked --lib config::tests:: -- --quiet`: 13 passed.
- `python3 scripts/cargo_serial.py test --locked --lib core::strategy:: -- --quiet`: 5 passed.
- `python3 scripts/cargo_serial.py test --locked --lib ui::strategy:: -- --quiet`: 2 passed.
- `python3 scripts/cargo_serial.py test --locked --lib running_commit_blocks_push_fetch_roots_workspace_replacement_and_quit -- --quiet`: 1 passed.
- These focused groups total 80 passing tests.
- `python3 scripts/cargo_serial.py build --locked --bin igit`, `python3 scripts/cargo_serial.py fmt --all --check`, and `git diff --check` passed. Validated 277 local documentation links across the six affected documents.
- Ran the built binary in a real 160×42 PTY against `examples/demo-push.sh`, with startup update checks disabled and preferences isolated inside the temporary fixture. Checked both strategy directions without a selector, F/H options, pane focus, Details, maximize/restore, Push review, and cancellation. All 11 repository HEADs, exact index bytes, staged/working diffs, statuses, and refs matched the saved snapshot; all 11 local bare remotes retained their refs.

### Existing failures outside this change

- The expanded `shell::configuration_tests::` run passed 24 of 27 tests. `configuration_hunk_view_remaps_and_help_review_keeps_git_unchanged_on_cancel` retains Changes instead of opening Stage Hunks. `configuration_smart_focus_stays_in_the_tab_and_survives_workspace_rescan` and `configuration_smart_resize_keeps_tabs_isolated_and_help_executes_the_action` expect Conflicts after `6`, which currently opens Remote. Their Commit/Remote behavior and expectations were not changed by this task.
- Strict all-target Clippy stops at the existing `clippy::nonminimal_bool` in `src/commit/view.rs:109`. Allowing that lint exposes the existing `clippy::needless_borrows_for_generic_args` in `src/branch/tests/smart_checkout.rs:355`. `python3 scripts/cargo_serial.py clippy --locked --all-targets -- -D warnings -A clippy::nonminimal_bool -A clippy::needless_borrows_for_generic_args` passed; those unrelated files were left intact.
