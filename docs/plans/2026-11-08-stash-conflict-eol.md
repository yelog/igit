# Stash Conflict EOL Compatibility Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Open ordinary LF text conflicts from stash apply/pop when the path declares `eol=lf`, preserving the existing reviewed write boundary.

**Architecture:** Keep `Document::open` as the shared gate for Enter, mouse opening, side previews, and draft preparation. Admit standard EOL attributes to the existing per-stage checkout-byte comparison; retain custom-conversion rejection and Apply revalidation. Base post-Apply guidance on the actual repository operation, without inferring a stash identity from conflict markers.

**Tech Stack:** Rust, Ratatui/Crossterm, system Git, temporary test repositories.

---

### Task 1: Reproduce the failure through the real event path

**Files:** `src/merge/engine.rs`, `src/merge/engine/tests/conversion.rs`.

1. Create temporary repositories with a committed LF attribute rule, Unicode/space paths, and conflicting stash apply/pop changes.
2. Press Enter through `MergePanel::event`; assert the editor opens with pending choices and no Git writes.
3. Cover closing/reopening, reviewed application, exact worktree/index result, unchanged HEAD and stash, and an unrelated file with both staged and unstaged content.
4. Run `cargo test --locked merge::engine::tests::conversion` and record the pre-fix failure.

### Task 2: Admit byte-preserving EOL rules and correct completion guidance

**Files:** `src/merge/engine.rs`, `src/merge.rs`, `src/merge/engine/tests/conversion.rs`.

1. Add `(key == "eol" && matches!(v, "lf" | "crlf"))` to the attribute allowlist. Keep the `cat-file --filters` equality check for all three source stages.
2. Keep all custom merge/filter/encoding/ident restrictions. Test actual CRLF transformation, no-op EOL rules, and attribute/config changes after opening.
3. When `Document::op` is absent, report `Review in Commit.` after Apply; retain the separate Continue guidance for real merge/rebase/sequencer operations.
4. Re-run focused tests and `cargo test --locked merge::`.

### Task 3: Document and verify the supported boundary

**Files:** `README.md`, `docs/design.md`, `docs/prd.md`, `docs/architecture.md`.

1. Document byte-preserving EOL support, conversion limits, and the stash resolution flow without Continue or automatic stash removal.
2. Run `cargo fmt --all --check`, `cargo test --locked`, and `cargo clippy --locked --all-targets -- -D warnings`.
3. Review the final diff for scope and consistency. Do not commit or push; implementation in this workspace is already requested.

### Verification outcome

- Before the fix, the Enter regression failed with the exact reported
  `Path uses eol=lf; use its configured Git merge/filter workflow` error.
- After the fix, all four conversion regression tests and all 32 Merge tests pass.
  The stash test exercises Enter, close/reopen, resolution, review, and Apply
  through real panel events against temporary Git repositories.
- The full suite passes: 518 unit tests and 8 integration tests.
- `cargo fmt --all --check`, Clippy with warnings denied, and `git diff --check`
  pass. No existing user repository was used to reproduce the conflict.
