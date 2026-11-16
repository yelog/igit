# Remote Branch Creation Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Prefill a remote branch's local name and establish its upstream when creating a local branch, while retaining the current checkout.

**Architecture:** Reuse `Snapshot::local_name` for Create and remote checkout. Review remote creation with an explicit upstream, revalidate the existing repository snapshot, and execute one `git branch --track` command against the full remote reference. Keep local-source creation on the reviewed OID with `--no-track`.

**Tech Stack:** Rust, Ratatui, system Git, disposable repository tests.

---

### Task 1: Reproduce remote creation behavior

**Files:**
- Modify: `src/branch/tests.rs`
- Create: `src/branch/tests/creation.rs`

1. Exercise `a` on `origin/develop`; expect `develop` in the editor and an explicit tracking relationship in the review.
2. Check nested Unicode names, non-origin remotes, and a remote name containing `/`.
3. Verify an editable local name still tracks the selected remote, independent of `branch.autoSetupMerge`.
4. Check existing local names, unconfigured remotes, stale sources, changed Roots, and canceled input without overwrites.
5. Preserve exact index bytes, mixed staged/unstaged content, the current checkout, and another repository.
6. Confirm local-source creation still starts empty and creates no upstream.

Run the first focused regression:

```sh
python3 scripts/cargo_serial.py test --locked --lib branch::tests::creation::remote_create_prefills_name_and_tracks_without_switching
```

Expected before implementation: the default-name assertion fails.

### Task 2: Implement source-aware creation

**Files:**
- Modify: `src/branch.rs`
- Modify: `src/branch/operations.rs`

1. Resolve the default local name for remote Create and Checkout through the same snapshot helper.
2. Keep Checkout's existing matching-local-branch reuse; Create continues through the name editor and Cancel-first review.
3. Require a configured remote for both tracking operations and remote-source creation.
4. Describe `Track: <remote>/<branch>` in remote creation reviews.
5. Execute remote Create with `git branch --track -- <name> <full-remote-ref>`; retain `git branch --no-track -- <name> <oid>` for local sources.
6. Reuse existing validation, snapshot checks, write scope, and refresh behavior.

Run `python3 scripts/cargo_serial.py test --locked --lib branch::tests::creation`; expect all regressions to pass.

### Task 3: Document and verify

**Files:**
- Modify: `README.md`
- Modify: `docs/design.md`
- Modify: `docs/architecture.md`
- Modify: `docs/prd.md`

1. Document default names, explicit tracking, editable names, existing-name rejection, and retained checkout.
2. Run `python3 scripts/cargo_serial.py test --locked --lib branch::`.
3. Run `python3 scripts/cargo_serial.py test --locked --lib shell::branch`.
4. Run `python3 scripts/cargo_serial.py fmt --all --check`, `python3 scripts/cargo_serial.py clippy --locked --all-targets -- -D warnings`, and `git diff --check`.
5. Build both commands with `python3 scripts/cargo_serial.py build --locked --bins`.

Implementation stays in this session. Do not commit, publish, or change branches in the user's repositories. All write-behavior tests use temporary repositories without network access.

## Verification

- The first focused regression failed before implementation: the default name was empty instead of `develop`.
- All seven creation regressions passed, including edited names, Git tracking defaults, nested Unicode names, non-origin remotes, duplicate names, stale reviews, and exact index preservation.
- Branch regression tests: 72 passed.
- Shell Branch integration tests: 31 passed.
- `python3 scripts/cargo_serial.py fmt --all --check` passed.
- `python3 scripts/cargo_serial.py clippy --locked --all-targets -- -D warnings` passed.
- `python3 scripts/cargo_serial.py build --locked --bins` built both debug commands successfully.
- `git diff --check` passed.
