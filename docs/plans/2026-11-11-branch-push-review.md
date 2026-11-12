# Branch Push Review Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Make the pictured Branch push confirmation easy to scan and open with Confirm focused.

**Architecture:** Extend the existing immutable Branch merge review renderer to cover Push, using the reviewed upstream for the destination. Keep the shared confirmation event handler, existing push letter shortcuts, Git validation, and background execution.

**Tech Stack:** Rust, Ratatui, Crossterm, shared TextLayout, TestBackend, temporary Git repositories and local bare remotes. No new dependencies.

---

## Task 1: Cover the requested interaction

**Files:**
- Create `src/shell/branch_push_tests.rs`.
- Register tests in `src/shell.rs`.

1. Build a disposable fixture with an outgoing, non-current local branch, a differently named upstream, an unrelated remote branch, and mixed staged/unstaged files.
2. Assert the review starts on Confirm and Enter publishes only the reviewed branch without changing HEAD, index bytes, or local content.
3. Cover button focus, cancellation, help, existing y/n shortcuts, mouse activation, all icon modes, four terminal sizes, and long Unicode branch names.
4. Run `python3 scripts/cargo_serial.py test --locked --lib branch_push_review_defaults`; expect the default-focus assertion to fail before implementation.

## Task 2: Render a compact push review

**Files:**
- Modify `src/branch.rs`, `src/branch/sync.rs`, `src/branch/operations/view.rs`, and `src/shell.rs`.

1. Give Branch Push its own internal review token and default focus, retaining existing letter-key activation for this review.
2. Reuse the merge dialog frame, wrapping, scrolling, dimmed backdrop, and fixed footer. Preserve the merge review's current contents and behavior.
3. Lead with green From and purple To branches. Use the immutable reviewed remote/ref, including differently named upstreams.
4. Emphasize Fast-forward only, keep repository/path/short commit IDs secondary, and identify the current checkout when pushing another branch.
5. Route the review through the existing confirmation and background push path without changing Git execution.

## Task 3: Document and verify

**Files:**
- Modify `README.md`, `docs/design.md`, and `docs/prd.md`.

1. Document default Confirm focus, content hierarchy, fixed controls, scrolling, and retained y/n activation for Branch Push.
2. Run focused `branch_push_review` and `branch_merge` regressions, followed by related Shell Branch and focused sync checks through `scripts/cargo_serial.py`.
3. Run `python3 scripts/cargo_serial.py fmt --all --check`, appropriate Clippy checks, and `git diff --check`.
4. Preserve pre-existing workspace edits. Execute this plan in the authorized session without committing, publishing, or using everyday repositories for write tests.

## Verification record (2026-11-11)

- The initial `branch_push_review_defaults` regression failed because the original review focused Cancel.
- `python3 scripts/cargo_serial.py test --locked --lib shell::branch`: 23 passed, including all four new push reviews and existing merge, checkout, navigation, loading, and scope regressions.
- `python3 scripts/cargo_serial.py test --locked --lib focused_sync_uses_configured_upstream_without_switching_or_touching_dirty_files`: 1 passed.
- `python3 scripts/cargo_serial.py fmt --all --check`, `python3 scripts/cargo_serial.py clippy --locked --lib -- -D warnings`, and `python3 scripts/cargo_serial.py build --locked --bin igit` passed.
- All-target Clippy remains blocked by the pre-existing needless borrow at `src/branch/tests/smart_checkout.rs:352`; that unrelated code was preserved.
- The built binary was opened in real 120×36 and 44×12 PTYs against a disposable repository and local bare remote. Both rendered previews were inspected. Canceling preserved HEAD, exact index bytes, working changes, and remote refs.
- Tests cover Enter, y, and mouse confirmation; keyboard, mouse, and title-close cancellation; differently named upstreams; a non-current source branch; retained y/n help; all icon modes; long Unicode names; fixed controls; and scroll restoration. No everyday repository was pushed or committed.
