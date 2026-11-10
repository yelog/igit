# Update Before Push Implementation Plan

**Goal:** Show Branch-style target divergence in Push, persist a shared Merge/Rebase preference, and update reviewed repositories before publishing them.

**Architecture:** Share the update strategy and integration commands between Pull and Push. Keep Push jobs scoped to reviewed repositories, source commits, targets, configuration, and strategy. Fetch the explicit push target, integrate only when necessary, publish the resulting immutable commit, and retain native conflict state without an automatic delayed push.

**Tech Stack:** Rust, Ratatui, Crossterm, system Git, existing JSON state storage and local bare-repository tests.

---

### 1. Shared strategy and selector

- Add an application preference for Merge/Rebase, defaulting to Merge, saved atomically outside repositories.
- Provide one reusable keyboard/mouse selector. Push and Pull expose it through `s`; Pull retains its per-operation `m`/`r` choices.
- Register the Push command in the remappable command catalogue and document persistence.
- Verify saving, loading, cancellation, read/write errors, and Pull's temporary override.

### 2. Target divergence and Push review

- Read ahead/behind against the selected target and reuse Branch's tracking spans, retaining counts when names clip.
- Keep branch browsing, checking, targets, scope, and preview behavior unchanged.
- Review the strategy plus the complete update-and-push authorization and invalidate that review when strategy changes.
- Cover zero/unavailable counts, custom targets, Unicode, and narrow terminal layouts.

### 3. Shared integration and update-and-push execution

- Factor explicit Merge/Rebase integration out of the Pull worker.
- Fetch the reviewed target from the actual push endpoint with a bounded exact refspec.
- Revalidate the source and configuration around Fetch; distinguish operation-owned HEAD movement from external changes.
- Require clean tracked content when integration is needed, disable autostash and recursive submodule writes, and preserve merge topology during Rebase.
- Retain ancestry, exact-tip lease, hook, tag, upstream-publication, and submodule-publication protections.
- Report Fetch, updating, pushing, conflicts, updated-but-push-failed, and successful outcomes separately.
- Preserve partial batch success and stop dependent parents after child failure.

### 4. Regression and documentation

- Start with one focused test through `python3 scripts/cargo_serial.py test --locked --lib <filter>`.
- Verify Merge/Rebase divergence, conflicts without publication, dirty index/worktree preservation, stale sources, differing targets/endpoints, remote races, new targets, scoped batches, and retries against temporary local remotes.
- Run affected Push, Pull, configuration and Shell checks, then the shared-code regression suite, formatting, and Clippy through the serial wrapper.
- Update README, interaction design, PRD, architecture, and configuration documentation without replacing unrelated edits.
- Do not commit, push, change branches, or touch the retained `target/` directory.

### Verification recorded

- The shared preference and selector checks passed, including atomic persistence,
  cancellation, temporary Pull overrides, failed saves, and shortcut remapping.
- Native PTY runs with local bare remotes passed for Merge and Rebase: one
  confirmation integrated two incoming commits and published the verified result.
- Final safety review added focused cases for exact target refs, chained URL
  rewrites, ignored-path replacement refs, reviewed source identity, and commits
  introduced by integration hooks.
- Final focused checks passed: `core::update::tests` 4/4,
  `push::update::tests` 18/18, `shell::update_strategy_tests` 4/4,
  `shell::update::` 11/11, and Log operation tests 25/25.
- The final full regression passed 944 unit tests and 10 integration tests; one
  existing unit test was ignored. `fmt --all --check`, Clippy with warnings
  denied, all 260 local documentation links, and `git diff --check` passed.
