# Commit Repository Grouping Implementation Plan

**Goal:** Add a default-enabled Repository checkbox to Commit's Group By menu so each changelist shows repository names before repository-relative directories.

**Architecture:** Reuse the existing repository rows and file identities. Repository and the existing Module option enable one shared repository level, avoiding duplicate nodes when both are checked. Keep grouping separate from checkboxes, Roots scope, and Git write plans; use the effective grouping consistently for directory previews, copied paths, and rollback targets.

**Tech Stack:** Rust, Ratatui, Crossterm, system Git, existing temporary-repository test fixtures.

## 1. Specify repository grouping behavior

- Modify `src/commit/tree.rs` tests to cover the default hierarchy, independent repositories with duplicate filenames, and repository checkbox scope.
- Run `python3 scripts/cargo_serial.py test --locked --lib commit::tree::tests::repository_grouping_is_enabled_by_default`.
- Before implementation, expect the test to fail because the default tree has no repository rows.

## 2. Extend display state and tree projection

- Modify `src/commit/display.rs` to add Repository between Directory and Module, enabled by default, and adapt menu layout and scrolling for the extra row.
- Modify `src/commit/tree.rs` to regroup with all three toggles while retaining active file identity and unrelated folds.
- Use `repositories || modules` as the effective repository grouping and retain one repository row per root in each changelist.
- Update `src/commit.rs`, `src/commit/view.rs`, `src/commit/discard.rs`, and `src/commit/rollback.rs` to use the effective grouping for path interpretation. Keep Rollback's existing independent two-option menu.

## 3. Verify interaction and write boundaries

- Extend `src/commit/tree.rs` coverage to all eight grouping combinations, folded navigation, Roots filtering, and checkbox isolation.
- Update `src/commit/view_tests.rs` for menu defaults, mouse and keyboard toggles, small terminal layouts, and repository-relative previews.
- Exercise directory copy targets and rollback/add targeting with both repository-grouped and workspace-grouped paths.
- Retain the real-Git regression that commits only a selected staged version while preserving subsequent working edits and an unchecked staged sibling.
- Run focused tests first, then `python3 scripts/cargo_serial.py test --locked --lib commit::`.

## 4. Update documentation and complete checks

- Update the Commit defaults, menu semantics, and acceptance criteria in `README.md`, `docs/design.md`, `docs/prd.md`, and `docs/architecture.md`, preserving other in-progress edits.
- Run `python3 scripts/cargo_serial.py fmt --all --check` and `python3 scripts/cargo_serial.py clippy --locked --all-targets -- -D warnings`.
- Review the scoped diff and report actual checks and remaining limitations. Do not commit or publish.

## Verification results (2026-11-11)

- The initial default-hierarchy test failed as expected because no repository rows were present.
- Tree regression: 8 passed, covering all eight grouping combinations, duplicate paths, Roots scope, folds, and native directory copy targets.
- `python3 scripts/cargo_serial.py test --locked --lib commit::`: 255 passed, 1 existing manual performance test ignored. This includes mouse/keyboard menus, small terminals, repository-relative previews, directory rollback, and selected staged content with an unchecked staged sibling.
- `python3 scripts/cargo_serial.py test --locked --lib file_tree_copy_routes_suffixes_before_global_actions_and_preserves_log_yank`: 1 passed.
- `python3 scripts/cargo_serial.py build --locked --bins`: passed; local binaries are in `target-serial/debug/`.
- Scoped `git diff --check` passed; 258 relative documentation links resolved.
- After correcting this feature's formatting, `python3 scripts/cargo_serial.py fmt --all --check` reported only other ongoing Branch, hosting, and browser changes.
- Strict all-target Clippy was blocked by `clippy::obfuscated_if_else` in the separately modified `src/shell.rs:1482`. Allowing that existing diagnostic exposed an unrelated compile error in the concurrently added `src/log/view_tests/browser.rs:120`, which accesses `.keys` on a tuple.
- `python3 scripts/cargo_serial.py clippy --locked --lib -- -D warnings -A clippy::obfuscated_if_else`: passed. The allowance is command-local; the unrelated files were left to their current work.
