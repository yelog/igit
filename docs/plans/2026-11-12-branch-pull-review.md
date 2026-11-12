# Branch Pull Review Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Make lowercase `p` review and pull the focused local branch in Branch, using the existing Branch Push review design.

**Architecture:** Route Branch's `p` to the existing reviewed, background, fast-forward-only branch sync operation. Share the Merge/Push review renderer with Pull, showing the configured upstream as From and the focused local branch as To. Preserve `U` as an alias and keep workspace Pull available through Actions and its existing shortcuts in other panels.

**Tech Stack:** Rust, Ratatui, Crossterm, TestBackend, temporary Git repositories and local bare remotes. No new dependencies.

---

## Task 1: Reproduce the routing and scope problem

**Files:**
- Create `src/shell/branch_pull_tests.rs`.
- Register it in `src/shell.rs`.

1. Build disposable repositories with differently named upstreams, incoming commits, another repository, and mixed staged/unstaged changes.
2. Press `p` on current and non-current local branches. Require a Branch-owned confirmation and verify only the reviewed branch changes after confirmation.
3. Cover cancellation, Enter and mouse confirmation, retained `U`, remapped/disabled bindings, help, repository/group/remote focus, and the Actions route to workspace Pull.
4. Check From/To direction, default Confirm focus, fixed controls, small terminals, all icon modes, and wrapped Unicode names.
5. Run `python3 scripts/cargo_serial.py test --locked --lib branch_pull_reviews_only_the_focused_branch`; expect failure because `p` opens the workspace update review.

## Task 2: Reuse Branch sync and review presentation

**Files:**
- Modify `src/branch.rs`, `src/branch/keymap.rs`, and `src/branch/operations/view.rs`.
- Modify `src/shell.rs`, `src/config/keys.rs`, `config/default.toml`, and `src/ui/help.rs`.
- Update the existing footer assertion in `src/shell/branch_tests.rs`.

1. Give branch Pull the default bindings `p` and `U`, excluding Branch from the global Pull binding context.
2. Route `p` to Branch before opening any workspace update dialog. Preserve editor, comparison, loading, and busy-operation behavior.
3. Give Pull its own review token and use the shared Branch renderer and background executor.
4. Show the upstream in purple under From and the local branch in green under To, emphasize Fast-forward only, and identify a retained current checkout.
5. Match Push's Confirm focus, keyboard navigation, y/n controls, cancellation, scrolling, and mouse targets.
6. Keep help, footer, and the global Actions label consistent with the shortcut scope.

## Task 3: Document and verify

**Files:**
- Modify `README.md`, `docs/design.md`, `docs/prd.md`, `docs/configuration.md`, and `docs/architecture.md`.

1. Document contextual `p`, the `U` compatibility alias, the Pull review design, and workspace Pull through Actions.
2. Run focused Pull regressions, existing Shell Branch regressions, focused branch sync checks, configuration/help checks, and workspace update checks through `scripts/cargo_serial.py`.
3. Run `python3 scripts/cargo_serial.py fmt --all --check`, appropriate Clippy checks, and `git diff --check`.
4. Preserve existing workspace edits and `target/`. Do not commit, push, or use everyday repositories for write tests.

## Verification record (2026-11-12)

- All 6 new `branch_pull` tests passed, including current/non-current branches, selected repositories, exact index preservation, remapping/disabling, executable help, controls, Unicode wrapping, and all three icon modes at four terminal sizes.
- All 23 existing Shell Branch tests passed in the related Branch run. Two initial failures in the new tests were fixed by rendering the fixture before using layout-dependent pane navigation; the complete new Pull group then passed.
- All 13 configuration parser tests, 25 workspace update/strategy tests, and the existing focused sync integration test passed.
- Configuration routing checks passed 25 of 26 tests; help-related checks passed 60 of 61. Both groups exposed the same untouched `configuration_hunk_view_remaps_and_help_review_keeps_git_unchanged_on_cancel` failure at `src/shell/configuration_tests.rs:32`: Ctrl-Y remained in `Commit / Changes` instead of entering `Commit / Stage Hunks`.
- Log prefix checks passed 5 of 6 tests. The untouched `log_space_filters_take_priority_over_global_pull_and_keep_editor_keys` assertion at `src/shell/key_sequence_tests.rs:221` expected `Pane Search` but received the existing `Log / Branch Search` context.
- `python3 scripts/cargo_serial.py fmt --all --check`, `python3 scripts/cargo_serial.py clippy --locked --lib -- -D warnings`, `python3 scripts/cargo_serial.py build --locked --bin igit`, and `git diff --check` passed.
- All-target Clippy remains blocked by the previously recorded needless borrow in `src/branch/tests/smart_checkout.rs:352`; that unrelated test was preserved.
- The built binary was exercised in actual 120×36 and 44×12 PTYs against a disposable repository and local bare remote. Lowercase p opened `Pull local branch`, and canceling preserved refs, exact index bytes, and mixed staged/unstaged changes.
- The first focused command waited behind an already running serialized full suite and ran after implementation, so no pre-fix failing run was observed. All commands used the shared wrapper and `target-serial/`; `target/` and pre-existing workspace edits were preserved.
