# Git Workflow Improvements Implementation Plan

**Goal:** Make precise commits, local recovery, long-running reads, history cleanup, and Neovim integration reliable in both IGit product forms.

**Architecture:** Keep Git writes scoped to reviewed immutable inputs and preserve the real index, working files, and unrelated repositories. Extend the existing panels and companion plugin instead of adding another application layer. Share process isolation with the existing bounded AI runner, retain native Git recovery state for history operations, and use versioned capability and terminal-event contracts for the companion.

**Tech Stack:** Rust, Ratatui, Crossterm, system Git, existing tempfile/serde dependencies, Lua, Neovim 0.10+.

**Execution:** Implement in the current two checkouts, without commits, branches, publication, new production dependencies, or changes to the existing `target/`. Preserve all pre-existing companion edits. Run native commands only through `python3 scripts/cargo_serial.py`.

## 1. Precise staging and explicit review

### Behavior

- Provide explicit Staged and Unstaged comparisons in Commit, including a file with both states.
- Preserve the default preview of the actual commit candidate.
- Extend the tracked text hunk dialog with a keyboard/mouse-selectable line range.
- Build a valid partial patch for additions, removals, and replacements; unselected changes stay outside the index write.
- Show the exact partial patch in the existing separate review before changing the index.
- In Neovim, retain existing range staging and add an explicit staged/unstaged hunk-preview selector and filtered navigation.

### Implementation

- Native files: `src/commit/backend/staging.rs`, `src/commit/staging.rs`, `src/commit/backend/preview.rs`, `src/commit/preview.rs`, and focused Commit view/keymap tests.
- Companion files: `lua/igit/hunks.lua`, `lua/igit/init.lua`, `lua/igit/command.lua`, `tests/hunks.lua`, README and help.
- Construct the selected result from captured HEAD/index/working sources; use the existing private-index verification and exact-path publication.
- Keep line selection separate from clipboard selection and commit checkboxes.
- Reject unsupported encodings, conversions, stale source/index/branch changes and special modes with the existing explanations.

### Acceptance

- HEAD=A, index=B, working/buffer=C remains explicitly inspectable.
- One hunk containing adjacent unrelated edits can stage or unstage only the selected lines.
- Additions, deletions, replacements, missing final newline, and offsets from earlier changes produce the expected index.
- Working bytes, unrelated index entries, unchecked siblings, shared message, and Roots remain unchanged.
- Keyboard, mouse, and help work at 120x36, 80x24, and 44x12.

## 2. Recoverable local operations

### Behavior

- Provide a distinct Undo Stage action in Neovim which restores the exact previous index entry.
- Before Rollback, persist backups only for reviewed target paths and their affected index entries.
- Offer a reviewed restoration action, including after restarting IGit.
- Persist merge decisions and edited results independently of working files.
- Reload a merge draft only when repository identity, path, conflict stages, and operation still match.

### Implementation

- Native files: `src/commit/backend/discard.rs`, `src/commit/rollback.rs`, related recovery modules/tests, and `src/merge.rs`.
- Companion files: `lua/igit/hunks.lua`, command/public wrappers, tests and documentation.
- Store recovery data outside repositories in the application's state directory with bounded retained history.
- Write backup metadata atomically before destructive writes. Record actual completed paths after partial failure.
- Restore only entries whose post-operation state still matches. Detect external edits rather than overwriting them.
- Preserve working permissions and native literal path identity. Never restore an entire old index over unrelated subsequent staging.
- Keep the existing Unstage semantics; Undo Stage is a separately named operation.
- Rollback retains eight records per repository. Initial working content and
  saved staged blobs together are limited to 32 MiB; a journal is limited to
  256 MiB. ACLs, xattrs and hardlink relationships are not captured.

### Acceptance

- Stage C over existing staged B, then Undo Stage restores B while leaving the buffer at C.
- External index changes refuse Undo Stage without modifying any entry.
- Rollback and restore preserve staged/unstaged distinctions and unselected siblings.
- Deleted untracked content can be recovered; ignored and hidden repositories stay untouched.
- Restart recovers a matching merge draft; stale stages or operations keep the saved draft without applying it.

## 3. Controlled Git reads and responsive workspace rescans

### Behavior

- Bound read-only Git command execution with timeout, cancellation, and descendant-process cleanup.
- Superseding a background read cancels its active subprocess so the next request can run.
- Report repository, operation, elapsed time and a useful cancellation/timeout explanation without exposing credentials.
- Preserve the existing stop-after-current-repository behavior for commit writes.
- Open/rescan runs in the background, can be canceled, and retains the existing workspace until success.
- A same-workspace rescan retains Roots by repository path; newly discovered repositories require inclusion.

### Implementation

- Native files: shared process runner, `src/core.rs`, `src/core/loading.rs`, core revision readers, `src/shell.rs`, and focused process/loading/workspace tests.
- Reuse existing cross-platform process-group isolation instead of adding a subprocess dependency.
- Install read controls on background worker threads and propagate them to bounded per-repository workers.
- Keep writes outside automatic read cancellation; do not classify an uncertain publication as a retryable read.
- Use generation-safe installation and preserve cached pane state where repository identity survives.

### Acceptance

- A hung read times out and reaps descendants.
- Requesting newer data cancels a hung old read and eventually installs only the new result.
- Bounded output remains bounded and timeout/cancel errors omit sensitive arguments.
- Normal UI navigation and quitting remain usable during rescans.
- Rescanning the same workspace preserves hidden Roots and does not execute Git writes.

## 4. Reviewed linear history cleanup

### Behavior

- Add an interactive history plan for the current local branch's unpublished linear commits.
- Support squash, fixup, and reorder, with editable squash messages and an explicit final review.
- Show the source branch, affected commits in execution order, and the effect on commit IDs.
- Keep other branches/tags intact and refuse published, stale, merge-topology, active-operation, or dirty tracked state.
- Limit plans to 2–128 non-root commits. Publication is checked against known
  local remote-tracking refs; no implicit fetch is performed.
- On conflict, leave native Git state for the existing separately reviewed Continue/Abort workflow.

### Implementation

- Native files: `src/log/actions.rs`, `src/log/operations.rs`, new history-plan module and focused temporary-repository tests.
- Use immutable commit IDs in the plan, validate the branch/ref and remote-tracking publication state again before execution.
- Disable autostash, update-refs, automatic autosquash, fork-point, replacement refs and directory rename inference as appropriate.
- Reuse existing exact-path collision protection.
- Provide a user-visible recovery reference to the original tip and native conflict recovery.
- Avoid exposing an unrestricted arbitrary shell command or silently rewriting a second repository.

### Acceptance

- Squash combines adjacent changes with the reviewed message; fixup retains the destination message.
- Reorder produces the expected tree and order.
- Cancel and stale review leave HEAD/index/worktree/refs unchanged.
- Published or merge-containing plans explain their unsupported scope.
- Conflicts retain native rebase state and do not start a push.

## 5. Neovim return synchronization and compatibility

### Behavior

- Reconcile loaded file buffers when a normal TUI session hides or exits.
- Check unmodified buffers for disk changes. Preserve modified buffers and explain concurrent disk changes.
- Refresh affected status, signs and blame through the existing `IgitChanged` / `IgitUpdated` contract.
- Detect native capabilities before invoking editor/revision/current-file-diff integration.
- Add `:checkhealth igit` with actual executable, application version, protocol and supported features.
- Explain an authenticated but incompatible editor request instead of silently ignoring it.

### Contract

`igit --capabilities` returns one JSON object:

```json
{
  "schema_version": 1,
  "version": "0.1.3",
  "editor_protocol": 1,
  "capabilities": ["editor", "revision", "file-diff", "repository-events"]
}
```

Only implemented capabilities are advertised. The companion accepts extra future
fields and rejects an unsupported schema or required missing capability.

Repository invalidation notifications use OSC 51 with the `igit-changed` prefix
and a JSON payload containing `version`, authenticated `session`, and absolute
`repo`. They request reconciliation rather than asserting a successful write.

### Implementation

- Native files: `src/shell/cli.rs`, `src/shell.rs`, `src/shell/editor.rs`, and CLI/editor contract tests.
- Companion files: `lua/igit/init.lua`, `lua/igit/editor.lua`, new capability/health modules, lifecycle/editor tests and help.
- Cache capabilities by executable identity and do not start an incompatible integrated action.
- Associate callbacks with the originating session and repository.
- Keep private current-file-diff snapshot imports and changedtick protection unchanged.

### Acceptance

- TUI checkout/stash/restore followed by hide/exit reconciles existing clean buffers.
- Dirty buffers retain unsaved text and show a disk-change warning.
- Wrong-session terminal events are ignored.
- Missing, old, invalid-JSON and partially capable binaries produce actionable diagnostics.
- All existing float/split/tab, unsaved-diff import, recovery, and editor bridge tests remain valid.

## Integration and verification

1. Implement each area in disjoint file scopes and review the resulting diffs.
2. Run one focused native staging test first through the serial wrapper.
3. Run focused recovery, process/loading, history cleanup, capability and companion tests.
4. Run native format, complete tests, and Clippy when the focused checks pass.
5. Build both standalone binaries with `python3 scripts/cargo_serial.py build --locked --bins`.
6. Exercise the native/companion boundary against the freshly built absolute binary path.
7. Update README, interaction design, architecture and PRD with delivered behavior and exact limits.
8. Record executed checks and remaining limitations separately from the acceptance specification.

## Implementation decisions

| Area | Delivered interaction | Implementation boundary |
| --- | --- | --- |
| Precise staging | Commit preview uses `I` / `W`; `Ctrl-G` opens the hunk dialog, `v` anchors a range, `j/k` extends it, and `a` restores whole-hunk selection. Click, Shift-click and drag select lines. | A range stays inside one tracked-text hunk. The private-index result supplies the final canonical review. Partial new-file unstaging and deletion staging normalize file metadata headers; complete-file ranges retain addition/deletion semantics. |
| Recovery | Rollback `d` → `u` opens a separate restore review with Back selected. Merge Save/Close persists drafts. The companion exposes `:IgitUndoStage`. | Rollback backups survive restart, include staged blobs and record completed paths. Native paths, file modes and symlink targets are preserved; ACLs/xattrs/hardlink relationships are not. Companion Undo Stage retains at most 100 successful operations in the current Neovim process. |
| Controlled reads | New background requests cancel obsolete reads; Open/rescan shows progress and supports `Ctrl-X`. | Shared process isolation enforces 30 seconds per read, 16 MiB stdout and 64 KiB stderr. Same-workspace Roots and container IDs are remapped by repository path. A scan waits for an open conflict document to be saved/closed. Writes and the legacy streaming submodule history check retain their original execution paths. |
| History cleanup | Log `a` → Clean Up Linear History; `p/s/f` chooses pick/squash/fixup, `J/K` reorders, `m` edits a message, `F2` opens the final review. | Only the selected commit through HEAD, 2–128 non-root linear commits. Publication uses known remote-tracking refs without fetching. The visible backup ref and native stopped rebase state remain available for recovery. |
| Companion lifecycle | Ordinary hide/exit reconciles buffers; authenticated repository events invalidate status/signs/blame; `:checkhealth igit` diagnoses the executable and protocol. | Dirty buffers are never replaced. Fingerprints hash at most 1 MiB and use metadata for larger files. Integrated editor/revision/diff features are capability-gated; older binaries may still launch ordinary panels. |

## Verification record

Checks below were executed on the development host; acceptance requirements
above are not a claim of additional platform coverage.

- Native staging backend: 21 tests passed before integration.
- Native preview/refresh, configuration, shared AI process runner, Log model and
  Remote model regression pass: 70 tests passed.
- Rollback recovery, existing discard semantics, Roots retention and Commit
  remapping: 25 tests passed after fixing ignore-query handling and the macOS
  native-path fixture.
- Rescan conflict-document deferral, cancellation and container remapping:
  4 tests passed.
- Neovim 0.12.4 headless: the final complete companion run passed 208 tests
  across terminal, hunks, status, blame, browsing, editor bridge, file diff,
  capabilities and lifecycle.
- Local relative links in README, PRD, interaction design, architecture and this
  plan were checked: 290 local destinations resolve.

The complete native command `python3 scripts/cargo_serial.py test --locked`
was executed once: 1,307 tests passed, 22 failed, and one manual performance
measurement was ignored. Eighteen failures shared a lost-stdin regression in
the bounded reader; three expected synchronous workspace replacement; one
preview assertion was obscured by its own clipboard notification. These were
corrected. Explicit-input reads now use a separate controlled API, scan tests
wait for actual installation, and the preview test dismisses only its own
notifications before checking zoom and split.

The following post-fix native regression runs passed, totaling 194 passed tests
and one ignored manual performance measurement:

| Filter | Passed | Ignored |
| --- | ---: | ---: |
| `log::operations::history::` | 20 | 0 |
| `core::process::` | 16 | 0 |
| `persistent_merge_draft_` | 8 | 0 |
| `shell::rescan::` | 4 | 0 |
| `commit::backend::` | 116 | 1 |
| `commit::backend::working::` | 1 | 0 |
| Configuration retention after rescan and smart-focus retention | 2 | 0 |
| `shell::notification_tests::` | 24 | 0 |
| `shell::workspace_tests::commit_preview::` | 2 | 0 |
| Background loading after workspace replacement | 1 | 0 |

The fingerprint regression additionally covers cancellation/deadlines across
129 literal Unicode paths while retaining exact index and working bytes.
The complete native suite was not rerun after the corrections; the focused
passes above cover every previously failing test and the additional fixes.

Final checks completed successfully:

- `python3 scripts/cargo_serial.py fmt --all --check`
- `python3 scripts/cargo_serial.py clippy --locked --all-targets -- -D warnings`
- `python3 scripts/cargo_serial.py build --locked --bins`
- Both `target-serial/debug/igit --capabilities` and
  `target-serial/debug/ig --capabilities` return the same schema-1,
  editor-protocol-1 contract and all four implemented integrations.
- The real `:checkhealth igit` provider, configured with the freshly built
  absolute executable, reports that path, version 0.1.3 and all four features
  without an error.
- With `IGIT_NATIVE_BINARY` set to the freshly built absolute `igit` path,
  both `tests/native_lifecycle.lua` and `tests/native_file_diff.lua` in the
  companion checkout exit successfully. The lifecycle case exercises real
  authenticated invalidation, separate disk changes before hide and hidden
  exit, clean reload, unchanged dirty-buffer text/changedtick, and clean Git
  index/worktree state. The diff case verifies unsaved snapshots, hunk restore,
  editor undo, newline/BOM metadata, and unchanged real index/HEAD/working files.
- Whitespace checks pass in both checkouts.

Build artifacts are
`/Users/yelog/workspace/tui/igit/target-serial/debug/igit` and
`/Users/yelog/workspace/tui/igit/target-serial/debug/ig`. Installed commands were
not replaced. Windows and Neovim 0.10 compatibility remain unverified on this
host; the attempted downloaded Neovim 0.10 executable exited with status 137
even for `--version`. The legacy outgoing submodule raw-history reader retains
its streaming path outside the shared read timeout/cancellation layer.
