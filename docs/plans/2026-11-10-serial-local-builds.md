# Serial Local Builds Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Keep the existing target untouched, use target-serial, and serialize local Cargo compilation and tests while investigating macOS freezes.

**Architecture:** Project Cargo configuration sets the new target directory, one build job and one libtest thread. A standard-library Python entry point holds a per-user OS lock across each entire Cargo command so separate checkouts and agent commands queue. CI explicitly retains its existing target directory for packaging.

**Tech Stack:** Cargo TOML, Python 3 (fcntl on macOS/Linux), unittest.

### Task 1: Defaults and command lock
- Create `.cargo/config.toml`: build.target-dir=target-serial, build.jobs=1, forced RUST_TEST_THREADS=1.
- Create `scripts/cargo_serial.py`: acquire an advisory lock under the user's cache directory; run Cargo from this checkout with forced target/jobs/test-thread environment; reject conflicting concurrency/directory overrides; forward interrupt/termination signals and wait before releasing the lock.
- Add fake-Cargo tests for simultaneous invocations, argument/environment forwarding, failure propagation and interruption. Tests must not run real project builds.
- Ignore target-serial; preserve target. No commits or cache deletion.

### Task 2: Document the entry point
- Update AGENTS.md and relevant README build/test/run examples to use the wrapper and target-serial paths, preserving existing working edits.
- Update local release verification instructions and add CARGO_TARGET_DIR=target to CI so archive paths remain compatible. No release or remote actions.

### Task 3: Bounded verification
- Run wrapper unit tests and syntax/config validation first.
- Select one small pure unit test and run it once through the wrapper, using the fresh target and one build/test thread; periodically observe syspolicyd and stop this verification if sustained abnormal load returns.
- Report observed result and remaining uncertainty; do not run an unsolicited full test suite or claim the system freeze has been fixed.

## Execution result

Implemented project defaults, the POSIX per-user command lock, updated local documentation, and an explicit CI target-directory override. Added five fake-Cargo tests covering command exclusion, forced settings/argument forwarding, failure, interruption, terminal-session preservation, and rejection of unsafe overrides; all passed. `cargo metadata --offline --locked --no-deps` confirmed `/Users/yelog/workspace/tui/igit/target-serial`. `git diff --check` passed.

Attempted one real cold build/test through the wrapper:

```sh
python3 scripts/cargo_serial.py test --locked --offline --lib config::tests::defaults_and_partial_overrides -- --exact
```

Dependencies compiled in the new target. Sampled syspolicyd CPU was 0.0%; no system freeze was observed during this attempt. Compilation of existing, concurrently developed application sources failed with seven type errors, including `src/log/view.rs:934` indexing repositories with a `Rect`, `src/log/view.rs:937` passing that `Rect` where a repository index is expected, and `src/log/view_tests/commit_actions.rs` comparing strings with unit values. These files were not modified by this change; the actual Rust test did not run. No claim is made that the system freeze has been fixed.

The original target directory remains present with the same inode and top-level modification timestamp. No cargo clean, deletion, commit, push, source rollback or full Rust suite was performed.
