# Paired Local Builds Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Prevent a local application rebuild from silently updating only one of IGit's two executable entry points.

**Architecture:** Keep the two existing Cargo binary targets and the serial command lock. Before running a `build` that selects only `igit` or `ig`, the project wrapper adds the other binary to the same Cargo invocation and reports that adjustment. Preserve focused library builds and other Cargo commands.

**Tech Stack:** Python 3 standard library, unittest with fake Cargo, existing Rust/Cargo binary targets.

---

### Task 1: Reproduce partial binary selection

**Files:**
- Modify: `scripts/test_cargo_serial.py`

1. Add fake-Cargo cases for both executable names, both `--bin NAME` and `--bin=NAME`, and glob selectors. Assert that selecting one application binary builds both in one invocation.
2. Cover selectors for both binaries, `--bins`, `--all-targets`, default builds, focused library/example builds, and the `--` boundary.
3. Keep `run`, `check`, `test`, `clippy`, and `install` argument forwarding intact. Check the paired-build notice and failure/lock behavior.
4. Run `python3 -B -m unittest discover -s scripts -p 'test_cargo_serial.py'`. Expect the new paired-build assertions to fail before the implementation.

### Task 2: Pair application builds

**Files:**
- Modify: `scripts/cargo_serial.py`

1. Add a helper that examines Cargo target selectors before `--` only for `build`.
2. Resolve binary selectors against the two known application names with `fnmatch.fnmatchcase`. If exactly one is selected, insert `--bin` and the missing name before `--`.
3. Leave broad builds, explicit selection of both binaries, focused nonbinary builds, and other commands intact.
4. Run the normalized arguments through the existing single lock and Cargo process. Print a concise notice when a companion binary is added.
5. Rerun the wrapper tests; expect all tests to pass.

### Task 3: Record the rebuild contract

**Files:**
- Modify: `AGENTS.md`
- Modify: `README.md`

1. Use `python3 scripts/cargo_serial.py build --locked --bins` as the canonical development rebuild.
2. Document automatic companion selection, the focused-build exception, and the fact that `run --bin`, tests, and Clippy do not refresh both standalone executables.
3. Require both commands to be built before reporting a general rebuild complete. Distinguish local build artifacts from installed commands.

### Task 4: Verify the original failure path

1. Run the original single-target request through the updated wrapper: `python3 scripts/cargo_serial.py build --locked --bin igit --message-format=json`.
2. Verify Cargo reports executable artifacts for both `igit` and `ig`, and run each executable with `--version`.
3. Run `git diff --check` and inspect the focused patch. Preserve existing feature edits, the old `target/`, installed commands, and Git history.

## Execution result

- Added four regression tests with multiple argument cases. Before implementation, the suite failed on all seven single-binary selection cases and the missing paired-build notice.
- All nine wrapper tests now pass, including the existing serialization, interrupt, failure propagation, and terminal-session checks.
- Replayed `python3 scripts/cargo_serial.py build --locked --bin igit --message-format=json`. The wrapper added `--bin ig`; Cargo reported executable artifacts for both commands, and both `--version` checks passed.
- Updated the canonical README rebuild and agent completion requirements. Existing CI already runs this test module, so no workflow or distribution change was needed.
- `git diff --check` passed. Existing feature changes, installed commands, the old `target/`, and Git history were left intact.
