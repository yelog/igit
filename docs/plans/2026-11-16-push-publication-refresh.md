# Push Publication Refresh Implementation Plan

> **For Codex:** Execute the plan in this session; preserve existing changes and do not commit.

**Goal:** Remove published commits from Push's outgoing list after a successful push while retaining remaining outgoing work and per-repository results.

**Architecture:** Outgoing reads prefer an endpoint-specific observation over local tracking refs. Successful push execution must return the exact published OID, including any Merge/Rebase result, and update that observation before requesting the next background snapshot. Publication must not impersonate a manual Fetch or replace tracking refs belonging to a different fetch endpoint.

**Tech Stack:** Rust, system Git, existing Push background workers and generation-based loaders, disposable local bare remotes.

---

### Task 1: Reproduce stale observations

**Files:** `src/push/fetch_tests.rs`.

1. Cache the old push-target tip, review and push through `PushPanel`, then finish its background refresh.
2. Assert that the remote has the pushed OID, outgoing counts/logs/file previews are empty, and the repository result remains successful.
3. Run `python3 scripts/cargo_serial.py test --locked --lib push::fetch_tests::successful_push_replaces_a_cached_target_tip -- --exact` and confirm the old count remains.

### Task 2: Carry publication identity to the UI

**Files:** `src/push/update.rs`, `src/push.rs`, `src/push/update/tests.rs`.

1. Return the execution outcome with the exact final published OID rather than rereading mutable HEAD after completion.
2. Send a publication observation from the worker using the captured repository, remote, destination branch, and normalized endpoint.
3. Apply it through the existing observation cache and loader invalidation, keeping publication separate from Fetch status.
4. Keep unsuccessful jobs and unreviewed repositories unchanged.

### Task 3: Exercise publication boundaries

**Files:** `src/push/fetch_tests.rs`, `src/push/update/tests.rs`.

1. Cover a different push endpoint, relative endpoints, and previously missing target branches.
2. Cover both Merge and Rebase so the cache uses the integrated OID.
3. Cover up-to-date completion, upstream-configuration warnings, partial failure/retry, and a new local commit created before UI completion is processed.
4. Check remote refs, index and working content in disposable fixtures.

### Task 4: Document and verify

**Files:** `docs/architecture.md`, `docs/design.md`, `docs/prd.md`, this plan.

1. Document publication observations and the post-push list behavior.
2. Run focused Push tests, Shell Push tests, formatting, strict Clippy, and `git diff --check` through the required serial Cargo wrapper.
3. Build both application binaries with `python3 scripts/cargo_serial.py build --locked --bins` and report their absolute paths.
4. Record actual results and any unrelated failures without changing unrelated files or `target/`.

## Verification record (2026-11-16)

- The initial reproduction failed after the remote had advanced: the installed entry still carried the cached pre-push OID. The same test passed after publication observations were added.
- `python3 scripts/cargo_serial.py test --locked --lib push::fetch_tests:: -- --quiet`: 20 passed.
- `python3 scripts/cargo_serial.py test --locked --lib push:: -- --quiet`: 135 passed, including publication, integration, partial failure/retry, upstream warnings, submodule ordering, loading, and rendering.
- `python3 scripts/cargo_serial.py test --locked --lib shell::push_tests:: -- --quiet`: 9 passed. The end-to-end regression retains the success notification and checked repository, removes the ahead counter, and renders zero changed files. Its initial empty-state expectation was corrected to the existing `No changed files` wording.
- `python3 scripts/cargo_serial.py fmt --all --check`, `python3 scripts/cargo_serial.py clippy --locked --all-targets -- -D warnings`, and `git diff --check` passed.
- `python3 scripts/cargo_serial.py build --locked --bins` passed. Both `target-serial/debug/igit --version` and `target-serial/debug/ig --version` executed successfully.
- Validated 294 local links across README, architecture, interaction design, PRD, and this plan.
- Publication regressions used disposable repositories and local bare remotes, including mixed staged/unstaged content and a local commit created after publication but before UI completion. Concurrent Branch changes were preserved.
