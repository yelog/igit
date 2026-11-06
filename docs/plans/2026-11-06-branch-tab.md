# Branch Tab Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add a Branch workspace tab with repository/local/remote trees, current-branch pinning, upstream labels, and reviewed branch operations.

**Architecture:** A dedicated `BranchPanel` owns branch snapshots, tree focus/folding, name input, and operation reviews. It uses the existing system-Git boundary and shell confirmation flow; successful or failed writes refresh the workspace so conflicts remain visible. Existing tab shortcuts stay intact; `5` opens Branch, and Branch owns `r` while Actions retains access to Roots.

**Tech Stack:** Rust, Ratatui, Crossterm, system Git, existing test fixtures and temporary repositories; no new dependencies.

---

### Task 1: Read refs and build repository trees

**Files:** Create `src/branch.rs`, `src/branch/model.rs`, `src/branch/tree.rs`, `src/branch/tests.rs`; modify `src/lib.rs`.

1. Add tests using temporary ordinary repositories for local/remote refs, upstreams (including gone refs), nested slash names, current-first traversal, empty/unborn and detached HEAD, and independent Roots.
2. Read full refs and OIDs with `for-each-ref`; omit symbolic remote HEAD aliases. Retain worktree ownership and actual upstream metadata.
3. Build repository → Local Branches / Remote Branches → slash folders → branch leaves. Expand by default, order the current branch's ancestors first, and preserve focus/folds by stable IDs.
4. Run `cargo test --locked branch::` and verify branch rows against real refs.

### Task 2: Review and execute branch operations

**Files:** Create `src/branch/operations.rs`; extend `src/branch/tests.rs`.

1. Cover cancellation, stale refs/HEAD/scope, invalid/duplicate names, unmerged/current/other-worktree deletion, checkout protection, and merge conflicts before wiring controls.
2. Implement `a` as creating a local branch at the focused branch's reviewed commit without switching; `d` as safe local deletion (`branch -d`); Enter as local checkout or a new tracking checkout from a remote ref; `m` as merging the reviewed source into the current local branch; `r` as local rename.
3. Name input precedes shell review. Reviews show repository path, source, target/current branch, and commit IDs. Validate scope and reread branch state before executing. Git retains its overwrite/merge safety checks; remote deletion/rename is outside this local-management feature.
4. Require a clean worktree/index for merges, reject an existing operation, and preserve conflicts for the Conflicts tab. Never force deletion, reset, autostash, or push.
5. Verify actual refs, working files, index entries, and HEAD after success and refusal.

### Task 3: Render and connect the workspace tab

**Files:** Create `src/branch/view.rs`, `src/branch/keymap.rs`; modify `src/shell.rs`, `src/shell/workspace.rs`, `src/shell/panes.rs`, `src/config.rs`, `src/config/keys.rs`, `config/default.toml`, `src/ui/help.rs`, `src/ui/panes.rs`; add `src/shell/branch_tests.rs`.

1. Render a single branch tree with current markers, upstream suffixes, repository paths, scrolling, folder hit targets, and a focused-branch details strip. Add mouse action controls sharing the keyboard review path.
2. Wire tree navigation, folding, Unicode name input/paste, contextual help, remappable commands, the Branch header tab, and `5` shortcut.
3. Refresh on tab entry, F5, branch writes, and Fetch; keep review/input guards and existing Commit/Log/Push/Conflicts shortcuts.
4. Test 120×36, 80×25, and 44×12 layouts, header hit areas, Roots via Actions, remapping, help/input isolation, and merge conflict visibility.

### Task 4: Documentation and verification

**Files:** Update `README.md`, `docs/prd.md`, `docs/design.md`, `docs/architecture.md`, `docs/configuration.md`; adjust existing shell expectations for the extra tab.

1. Document the new tab, shortcuts, current-first slash grouping, tracking labels, local-only delete/rename, and checkout/merge semantics.
2. Run `cargo fmt --all --check`, `cargo test --locked`, and `cargo clippy --locked --all-targets -- -D warnings`.
3. Inspect the resulting diff while preserving all pre-existing edits. Do not commit or publish.
