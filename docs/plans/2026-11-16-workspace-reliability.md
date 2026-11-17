# Workspace Reliability Implementation Plan

**Goal:** Make history search trustworthy, Git operations responsive and observable, and multi-repository work recoverable across sessions.

**Architecture:** Extend the existing panel workers and controlled Git execution rather than introducing another runtime. Keep Git state authoritative, preserve reviewed write boundaries, and store only versioned application metadata outside repositories.

**Tech Stack:** Rust, Ratatui, Crossterm, system Git, existing serde and tempfile dependencies.

## 1. Trustworthy history and bounded topology

- Update `src/log/loading.rs`, `src/log/filters.rs`, `src/log/model.rs`, and their tests.
- Keep immediate loaded-page filtering and explicitly start cancellable full-history search on submission.
- Search complete commit messages and resolve commit hashes without depending on the loaded window.
- Cache topology by repository identity and ref tips, bound cold topology reads, and retain a safe local-layout fallback.
- Verify old/body-only matches, empty results, stale requests, ref changes, and unchanged Git state.

## 2. Responsive and observable writes

- Update `src/core/process.rs`, `src/core.rs`, `src/commit/execution.rs`, and Shell operation dispatch.
- Move Commit preparation and remaining confirmed panel writes off the terminal thread.
- Reuse owned subprocess groups and bounded output, report task age and phase, and retain operation-specific stopping semantics.
- Keep writes mutually exclusive and prevent stale panel results from replacing another workspace.
- Verify slow hooks, output budgets, cancellation of preparation, and mixed-index preservation.

## 3. Consistent scope and durable activity

- Add shared scope descriptions to reviews and live operation feedback.
- Persist bounded, versioned activity records with repository identities, before/after HEAD observations, status, and recovery guidance.
- Preserve unfinished records as unknown after restart; never replay an uncertain write.
- Provide an Activity / Recovery entry beside existing recovery and notifications controls.
- Verify partial outcomes, malformed state, interrupted records, and redacted diagnostics.

## 4. Persistent workspace context

- Save named Roots worksets and the last panel under the existing workspace state directory.
- Restore by canonical path, exclude newly discovered repositories from saved worksets, and report missing repositories.
- Add workset save/load/delete controls to Roots; preserve explicit CLI panel selection.
- Extend Roots with lightweight cached status information without implicit network access or writes.
- Verify restart, new/missing roots, empty worksets, and invalid state.

## 5. Regression and documentation

- Run focused tests first through `python3 scripts/cargo_serial.py`.
- Add repeatable disposable-fixture performance/PTY validation and portable process-tree coverage where supported.
- Update PRD, interaction design, architecture, and README to describe delivered behavior and limits.
- Run format, Clippy, affected tests, and build both binaries.
- Record actual verification results here. Do not commit or publish.

## Verification

- `python3 scripts/cargo_serial.py fmt --all --check` passed.
- `python3 scripts/cargo_serial.py clippy --locked --all-targets -- -D warnings` passed.
- The final `python3 scripts/cargo_serial.py test --locked` run passed 1,451 library tests with 3 ignored and no failures. All CLI integration tests and documentation tests also passed.
- `python3 scripts/cargo_serial.py build --locked --bins` rebuilt both `igit` and `ig` successfully.
- The disposable 20,000-commit probe completed in 269.988 ms cold and 177.253 ms warm for initial history/topology loading; complete commit-message body search completed in 392.498 ms.
- `scripts/smoke_workspace_reliability.py` passed against both `target-serial/debug/igit` and `target-serial/debug/ig`. It covered exact and complete body search, named worksets, empty scope, Activity on wide and minimum-width terminals, session restore, panel restore, and unchanged HEAD/worktree state. The final parallel run observed 0.146 s and 0.188 s for `igit`, and 0.145 s and 0.186 s for `ig`; these are smoke timings rather than performance guarantees.
- `git diff --check` and Python bytecode compilation of the smoke harness passed.

Full-history search has a 30-second per-repository budget and reports an incomplete result instead of claiming exhaustive matches when that budget expires. Log topology reads are capped at 8,192 commits. Activity records each confirmed step/repository and is not a transaction log, universal undo, or remote rollback. An interrupted write can have partially succeeded and must be inspected before retrying.

No commit, push, release, or publication was performed. The existing `target/` directory was not cleaned or modified.
