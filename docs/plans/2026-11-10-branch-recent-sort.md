# Branch Recent Usage Sorting Implementation Plan

**Goal:** Add session-only Name and Recently Used sorting to Branch, with the current branch first and worktree-local HEAD reflog as the usage source.

**Architecture:** Keep usage metadata outside operation snapshots. Read reflog in the existing background display loader, and project flat or grouped rows from cached data. Name remains the default and preserves existing default-branch priority; Recently Used removes that extra local priority, orders unknown branches by name, and leaves remote ordering unchanged.

**Tech Stack:** Rust, system Git, Ratatui, existing serialized Cargo wrapper.

## 1. Read usage metadata

- Add `src/branch/recent.rs` and register it in `src/branch.rs`.
- Parse event timestamps and checkout destinations, retaining the newest occurrence per existing local ref and reflog order for same-second ties.
- Follow rename records backwards to existing branch names. Ignore unrelated or malformed entries.
- Extend `src/branch/model.rs` display metadata only. Missing or unreadable reflogs produce an empty usage map.
- Verify same-OID switching, rename, detached HEAD, missing records, and linked worktree isolation in disposable repositories.

## 2. Sort cached trees

- Extend `src/branch/tree.rs` with Name and Recently Used modes.
- Use the best descendant ordering key for folders; preserve current-branch path priority, selection, and folds.
- Carry the mode through refresh, unfolded search projections, and concurrent display-option changes.
- Verify flat and nested ordering, unknown-name fallback, remote ordering, and refresh selection.

## 3. Expose display options

- Extend `src/branch/display.rs` with Sort By choices in View Options.
- Support keyboard navigation, activation, mouse selection, and clipped menus in short terminals.
- Update contextual help and the Branch footer in `src/branch/keymap.rs`, `src/shell/configuration.rs`, and related tests.
- Verify menu interaction does not write Git state or change the selected branch.

## 4. Document and validate

- Update `README.md`, `docs/design.md`, `docs/prd.md`, and `docs/architecture.md`.
- Run the focused new tests first, then Branch and Shell Branch tests.
- Run `python3 scripts/cargo_serial.py fmt --all --check` and `python3 scripts/cargo_serial.py clippy --locked --all-targets -- -D warnings`.
- Preserve existing unrelated work; do not commit or add persistent settings.

## Verification

- Completed all four steps.
- Branch tests: 38 passed, including seven new parser/sorting tests.
- Shell Branch tests: 14 passed.
- Clippy passed for all targets with warnings denied.
- Checked 228 relative documentation links successfully.
- Formatting checks report no differences in this task's files. The repository-wide check still reports differences in other concurrently modified files; those were left unchanged.
