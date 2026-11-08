# Git Workflow Completeness Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Deliver the next development iteration from the code-based TUI review: correct platform/path/preview handling, complete first-push tracking, keep commits responsive, support reviewed hunk staging, and provide cherry-pick and native operation recovery controls.

**Architecture:** Preserve Roots, native path identity, explicit write reviews, private-index commit construction, and independent repository outcomes. Extend the existing panel controllers and background result channels; no production dependency or replacement Git engine is needed. Keep in-progress Pull/Fetch upstream-counter changes intact.

**Tech Stack:** Rust, Ratatui, Crossterm, system Git, temporary repositories and local bare remotes.

---

## Delivery boundaries

This iteration implements the concrete next-round recommendation. Full worktree/remote/tag management, hosting integration, themes, workspace dashboards, blame, and advanced interactive rebase remain separate follow-up projects. No commit, push to a hosting service, or release is authorized by this plan.

## Task 1: Platform state-directory resolution

**Files:** `src/core.rs`, a focused state helper/test module as needed, `tests/discovery.rs`.

1. Add deterministic tests for explicit overrides, empty variables, Unix HOME, and Windows LOCALAPPDATA/APPDATA/USERPROFILE without HOME using injected environment lookup rather than process-wide mutations.
2. Resolve a private per-user state directory on Windows, preserving explicit overrides and existing Unix paths.
3. Run the focused resolver/discovery tests and a CLI check in an isolated environment without HOME.

## Task 2: Conflict identity and repository-operation controls

**Files:** `src/merge.rs`, `src/merge/engine.rs`, `src/merge/list.rs`, `src/merge/keymap.rs`, related views/tests.

1. Reproduce an ordinary conflict plus a non-UTF-8 path in an index; keep both visible using native path identities, with unsupported paths individually marked for external resolution.
2. Reject opening unsupported text without lossy path round trips; keep the repository's native operation visible.
3. Add separately reviewed Abort and Skip where Git supports them. Revalidate operation identity and affected state, preserve normal Continue semantics, and report native partial/conflict results.
4. Verify cancellation, stale reviews, multi-repository scope, and unchanged unrelated working/index contents in disposable repositories.

## Task 3: Honest untracked-file previews

**Files:** `src/commit/backend.rs`, focused backend tests.

1. Read one byte beyond the 96 KiB preview limit to distinguish complete and truncated input.
2. Add a visible truncation notice without cutting a UTF-8 character or pretending the preview is the commit boundary.
3. Verify exact-limit, over-limit, Unicode, binary, and unchanged working/index/HEAD behavior.

## Task 4: First push and upstream configuration

**Files:** `src/push.rs`, `src/push/view.rs`, `src/push/keymap.rs`, related loading/tests.

1. Capture the source branch's actual upstream configuration and expose an explicit per-repository tracking choice for the reviewed push target.
2. Include tracking intent in review and execution revalidation; set upstream only after the selected push succeeds and retain existing upstreams by default.
3. Cover new-branch first push, configured upstreams, changed targets/config, cancellation, and partial failures with local bare remotes.
4. Make the controls reachable by keyboard and mouse at supported terminal sizes.

## Task 5: Reviewed hunk staging and unstaging

**Files:** `src/commit.rs`, `src/commit/backend.rs`, a focused staging backend/controller, `src/commit/keymap.rs`, relevant views/tests, configuration command catalogue.

1. Add a Staged/Unstaged comparison mode for a highlighted file independently of commit checkboxes.
2. Parse actual Git patch hunks and let users navigate and review one hunk for Stage/Unstage.
3. Bind the review to HEAD/branch, exact index entries, working bytes/conversion state, and the selected patch. Rebuild/apply through a private index, publish with the real index lock, and preserve all working files and unrelated index entries.
4. Cover mixed staging, unselected staged siblings, stale reviews, literal paths, and binary/unsupported patch refusal with real Git fixtures.

## Task 6: Responsive commit execution

**Files:** `src/commit.rs`, a focused execution worker, `src/shell.rs`, related tests.

1. Move confirmed commit batches to a worker and report per-repository progress and completion through a channel.
2. Keep browsing, resizing, and help responsive; prevent overlapping writes or quitting while publication is in an unsafe phase.
3. Allow stopping repositories that have not started, retain successful SHAs, and preserve child/parent ordering and shared draft retry behavior.
4. Test with a controlled slow hook, partial success, and existing mixed-index fixtures.

## Task 7: Cherry-pick and simple history editing

**Files:** `src/log/operations.rs`, `src/log/actions.rs`, related tests.

1. Add a reviewed Cherry-pick action targeting only the selected commit's repository and immutable OID.
2. Reuse clean-worktree, pending-operation, ignored-path collision, and stale-review checks. Preserve native conflicts for Conflicts and its separate Continue/Abort controls.
3. Cover cancellation, normal success, conflicts, stale HEAD/Roots, and same OIDs in different repositories. Implement additional simple history actions only if they fit these existing review boundaries without weakening safety.

## Task 8: Documentation and combined verification

**Files:** `README.md`, `docs/prd.md`, `docs/design.md`, `docs/architecture.md`, preserving in-progress edits.

1. Document delivered behavior, shortcuts, unsupported cases, and remaining roadmap items in English.
2. Run each subsystem's focused tests while implementing, then `cargo test --locked`, `cargo fmt --all --check`, and `cargo clippy --locked --all-targets -- -D warnings`.
3. Inspect narrow/wide keyboard/mouse rendering and Git state preservation with temporary repositories. Do not claim Windows/Linux interactive acceptance from macOS-only checks.

## Task 9: Background Log reloads

**Files:** `src/log.rs`, `src/log/loading.rs`, `src/log/filters.rs`, relevant model/view tests, `src/shell.rs`, `src/shell/startup.rs`.

1. Extend the existing initial-history worker pattern to runtime refreshes, filter changes, and Log tab switching.
2. Retain cached rows while current results load, reject superseded Roots/filter generations, and block actions against stale history.
3. Poll current results from Shell without waiting on Git in the terminal event loop. Preserve direct synchronous panel callers for existing noninteractive consumers and tests.
4. Verify stale completions, rapid tab changes, current selection, and unchanged Git state. Document any remaining full-history-search or graph-cache limits separately.

## Acceptance

- A supported native Windows environment does not need a Unix HOME variable.
- Unsupported conflict filenames never hide other conflicts or falsely report an empty repository.
- A bounded preview always announces truncation.
- A first push can establish the reviewed upstream and make later Pull available.
- A user can stage/unstage a hunk and commit exactly that index version without altering unrelated staging or later working edits.
- A slow commit hook does not block the terminal event loop.
- Cherry-pick and supported native abort/skip flows stay within the selected repository and require their own reviews.
- Existing repository-scope, child-publication, hook, and stale-review regressions remain green.

## Delivered implementation

- Platform state-directory resolution uses nonempty overrides and native Windows fallbacks, with injected-platform and no-HOME CLI tests.
- Conflict paths retain native identity; unsupported filenames remain individually visible. Abort and supported Skip have independent, revalidated reviews.
- Untracked previews detect content beyond 96 KiB, keep UTF-8 boundaries, and announce truncation.
- Push target editing offers explicit upstream setup after a successful push. Local configuration failures retain the successful remote result and exclude it from Retry.
- Commit offers reviewed Stage/Unstage for complete tracked text hunks through a private index, preserving working files and unrelated staging.
- Production Commit batches and Log reloads run in background workers. Commit can stop unstarted repositories; cached Log browsing rejects stale write actions.
- Log offers reviewed Cherry-pick of a single non-merge commit. Native conflicts remain available for separate recovery reviews.
- Shell integrates concurrent Add to Git work with the shared write guard. Existing Pull/Fetch counter and Add to Git changes remain in the shared working tree.

## Local acceptance evidence

On macOS, the newly built binary passed a PTY smoke check in disposable repositories: hunk review cancellation preserved HEAD/index/working bytes, Stage changed only the chosen first hunk, and Unstage restored the original index entries without changing working files or a second staged file. Log entry and refresh completed. With a pre-commit hook held open, browsing remained available, quitting was blocked, and Ctrl-X let the current repository finish while skipping the next. The remaining selection and shared draft were retained, and both sessions exited cleanly.

`cargo fmt --all --check`, `cargo clippy --locked --all-targets -- -D warnings`, `cargo build --locked --bins`, configuration validation, and diff whitespace checks passed. The updated documentation's local links resolve.

The full `cargo test --locked -- --test-threads=8` run passed 646 unit tests and 10 CLI integration tests, with no failures and one ignored manual performance test. Subsequent shared-tree updates to Add to Git help and failure reporting passed `cargo test --locked --lib add_to_git` (15 passed, one ignored); both binaries were rebuilt after those updates.

Individual-line staging, advanced interactive rebase, full-history search, persistent merge drafts, themes, dashboards, and comprehensive worktree/remote/tag management remain deferred. Log still loads at most 3,200 commits per repository. Native Windows/Linux interactive acceptance and large-repository performance benchmarking remain unverified. No release, commit, or push to a hosting service was performed.
