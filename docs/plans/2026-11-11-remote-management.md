# Remote Management Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add a Remote workspace tab that lists every remote under repositories in Roots and supports reviewed add, edit, and removal.

**Architecture:** A repository-grouped Ratatui panel owns browsing, folds, and a three-field editor for the remote name, fetch URLs, and optional push URLs. A separate Git backend snapshots the selected repository's configuration, validates edits, and rechecks scope and configuration before invoking local Git remote operations. Shell supplies workspace navigation, configurable keys, shared confirmation, refresh, and notifications.

**Tech Stack:** Rust, Ratatui, Crossterm, system Git, existing settings and test helpers; no new dependencies.

---

### Task 1: Read and write reviewed remote configuration

**Files:** Create `src/remote/model.rs`, `src/remote/operations.rs`, and backend tests below `src/remote/`.

1. Load all remotes and all configured fetch/push URLs for each repository in Roots; retain repositories with no remotes and isolate read failures.
2. Validate names, duplicate names, empty fetch URLs, Unicode, and control characters without using network requests or shell interpolation.
3. Prepare add/edit/remove reviews that name the exact repository and show before/after endpoints, with URL credentials hidden in display and diagnostics.
4. Recheck repository identity, Roots, and configuration at confirmation. Preserve unrelated configuration and use Git's native rename/removal behavior for tracking refs and upstreams. Report partial failures explicitly.
5. Test real disposable repositories, duplicate remote names across repositories, custom fetch refspecs, multiple URLs, stale configuration, rename/removal, and unchanged HEAD/index/worktree.

### Task 2: Build the Remote panel

**Files:** Create `src/remote.rs`, `src/remote/editor.rs`, `src/remote/view.rs`, and `src/remote/tests.rs`; register the panel in `src/lib.rs`.

1. Render repository groups with Name/URL columns, every fetch/push endpoint, empty/error rows, and an Add/Edit/Remove toolbar.
2. Implement keyboard, mouse, scrolling, group folds, and selection independent of Git writes.
3. Add a Unicode-aware editor for Name, Fetch URLs, and Push URLs; allow one URL per line, Tab navigation, Ctrl-S review, and Esc cancellation.
4. Reuse Shell confirmations and notifications. Preserve drafts on validation failure or canceled review.
5. Test rendering and hit areas at 120×36, 80×24, and 44×12 and verify read-only browsing and canceled edits leave repositories unchanged.

### Task 3: Integrate workspace navigation and settings

**Files:** Modify `src/shell.rs`, `src/shell/workspace.rs`, `src/shell/panes.rs`, `src/shell/cli.rs`, `src/shell/configuration.rs`, `src/config/keys.rs`, `config/default.toml`, `src/ui/help.rs`, and `src/ui/panes.rs`; add `src/shell/remote_tests.rs`.

1. Keep visible tab shortcuts continuous: use `6` for Remote and `7` for the conditional Conflicts tab; support `igit remote`.
2. Register Remote in header, Actions, help, normal-mode command settings, focus/layout routing, refresh, Roots changes, and main-tab return behavior.
3. Ensure the single-row header fits all tabs on a 44-column terminal and hit areas match displayed labels.
4. Guard writes while another Git operation runs, invalidate cached Branch/Log/Push views after changes, and refresh on reentry or F5.
5. Test direct launch, normal and remapped keys, mouse activation, help replay, review cancellation, and Roots isolation.

### Task 4: Document and verify

**Files:** Update `README.md`, `docs/design.md`, `docs/architecture.md`, `docs/prd.md`, and `docs/configuration.md`.

1. Document Remote's grouping, endpoint editor, review boundaries, shortcuts, and Git-only local configuration effects.
2. Start with `python3 scripts/cargo_serial.py test --locked --lib remote::`.
3. Run relevant Shell/configuration tests and then `python3 scripts/cargo_serial.py test --locked` because workspace integration spans panels.
4. Run `python3 scripts/cargo_serial.py fmt --all --check`, `python3 scripts/cargo_serial.py clippy --locked --all-targets -- -D warnings`, and `git diff --check`; distinguish unrelated pre-existing failures.
5. Preserve existing uncommitted work and the original `target/`; do not commit, push, or contact real remotes.

## Verification

Verified on 2026-11-12 with disposable repositories and local Git configuration only.

- `remote::`: 17 passed. `shell::remote_tests::`: 5 passed.
- The 44-column header test initially exposed that adding Remote left too little room for Log's close/next controls. Narrow headers now abbreviate earlier; the affected workspace header test passes.
- `cargo fmt --all --check` passed for the Remote change. A final rerun after concurrent edits landed in unrelated Branch files reports formatting differences there; `rustfmt --check` for every Remote-touched file still passes. `git diff --check` and `cargo clippy --locked --lib -- -D warnings` pass.
- A real PTY launch through `igit remote` listed two repositories and all configured fetch/push endpoints; add, rename/edit both endpoint lists, remove, and cancellation of each confirmation worked. Comparable HEAD, index, unstaged diff, status, and working-file hashes were unchanged. Existing remotes, a custom fetch refspec, and the explicit origin push URL were preserved.
- The final full test run reports 1105 passed, 5 failed, and 1 ignored. All Remote tests and the 44-column header test pass; the five remaining deterministic failures are in pre-existing uncommitted Commit tree-grouping and Log branch-search work, not Remote.
- `cargo clippy --locked --all-targets -- -D warnings` reaches an unrelated existing test warning in `src/branch/tests/smart_checkout.rs`; the library target passes.
