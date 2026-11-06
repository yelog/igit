# Merge Automatic Resolution Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Make automatic resolution available for the reference file's three non-conflicting changes, with a readable toolbar action and two real conflicts left for manual decisions.

**Architecture:** Reuse the existing safe-block and conservative disjoint-character rules through one candidate function. Share it between toolbar availability, the editor action, and prepared session drafts. Keep the batch in one undo snapshot and retain the existing Apply and Continue reviews.

**Tech Stack:** Rust, Ratatui, Crossterm, similar, system Git, existing icon modes.

---

### Task 1: Reproduce automatic resolution being unavailable

- Add temporary-repository regressions under `src/merge/engine.rs` for identical edits, left-only and right-only changes, and two overlapping conflicts, matching the screenshots.
- Check that mouse activation and `m` produce the same result, one undo restores all decisions, and no-op activation adds no history.
- Preserve partial decisions, ignored blocks, manual edits, working file bytes, index entries, and HEAD.
- Run `cargo test --locked merge::` and confirm the new regression fails before the fix.

### Task 2: Unify eligibility and application

- Update `src/merge/engine.rs`, `src/merge.rs`, and `src/merge/list.rs` to use one automatic-result rule for pending safe edits and untouched disjoint single-line edits.
- Retain the narrower Left / All / Right actions and existing configurable key names. Advance to a remaining unresolved block after the batch and report the result.
- Keep overlapping changes and insertion-order ambiguities manual.

### Task 3: Make the toolbar action readable

- Update `src/merge/view.rs` and `src/ui/icons.rs` with a wand / lightning icon, an explicit Resolve Automatically label, candidate count, and configured shortcut.
- Reserve space for automatic resolution and Undo before optional toolbar detail; shorten labels at narrow widths without shrinking the action to an unidentified icon.
- Exercise all icon modes, enabled/disabled/editing states, and 44-, 80-, 120-, and 184-column layouts using the test backend.

### Task 4: Verify and document

- Update `src/merge/keymap.rs`, `README.md`, `docs/design.md`, `docs/prd.md`, and `docs/architecture.md` to distinguish automatic resolution from applying only non-conflicting changes.
- Run merge regressions, formatting, Clippy, and relevant icon/help tests. Use a temporary practice repository for a PTY smoke check of activation, undo, save/reopen, and unchanged Git state before Apply.
- Do not commit or write to the user's practice repository.

### Verification completed

- The three new regressions failed on the original implementation and pass after the fix. All 17 merge tests pass.
- `cargo test --locked`: 297 library tests and 5 integration tests passed. `cargo fmt --all --check`, `git diff --check`, and `cargo clippy --locked --all-targets -- -D warnings` passed.
- Built the `igit` binary and exercised it in PTYs at 44×12, 80×24, 120×30, and 184×52. Mouse activation and `m` apply three safe changes and leave two conflicts; a no-op does not consume Undo; one Undo restores the batch; saved drafts reopen with the same decisions.
- Temporary-repository snapshots confirm that previewing and saving drafts preserve working bytes, index entries, and HEAD. Inspected rendered before/after previews. No writes were made to the user's practice repository.
