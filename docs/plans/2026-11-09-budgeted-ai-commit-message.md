# Budgeted AI Commit Message Implementation Plan

**Goal:** Generate commit messages for large selections using configurable request budgets, complete chunk summaries, and responsive background preparation while retaining existing workspace changes.

**Architecture:** Capture the selected Roots, native paths, configuration, and original draft before dispatching one worker. Build one immutable selected tree per repository, with staged versions taking priority. Budget the complete serialized request, send small selections directly, and summarize every chunk before reducing summaries into a final request. All subprocesses share cancellation and an overall deadline; completion revalidates sources and the panel selection before persisting a candidate.

**Tech Stack:** Rust, Ratatui, Crossterm, existing serde_json/tempfile, system Git/curl, standard threads/mpsc, disposable repositories, local mock HTTP servers.

---

### Task 1: Configure limits

**Files:** `src/config.rs`, `config/default.toml`

1. Add `ai.input_budget_bytes` (160000; 8192–2097152), `ai.max_requests` (32; 1–128), and `ai.total_timeout_seconds` (300; 1–3600).
2. Keep `ai.timeout_seconds` as the individual request timeout.
3. Test old configurations, merged defaults, explicit limits, boundary values, invalid types, and unknown fields.
4. Run `cargo test --locked config::tests --lib`.

### Task 2: Prepare bounded immutable selections

**Files:** `src/commit/backend.rs`, `src/commit/backend/ai.rs`, `src/commit/backend/ai/tests.rs`, `src/commit/process.rs`, `src/commit.rs`

1. Add deadline/cancellation-aware subprocess execution with bounded stdout/stderr and file-backed IO.
2. Move AI-specific patch capture into its backend module; build each private index once and retain the captured trees.
3. Emit per-file path, status, numstat, and one-line-context patches with explicit Git options.
4. Preserve mixed staging, unrelated index entries, literal paths, binary markers, symlink/mode changes, and unborn branches.
5. Bound total patch preparation at 32 MiB without silent truncation; revalidate only sources relevant to the selected versions.
6. Test large patches, inherited diff settings, stale sources, and index/worktree/HEAD preservation.

### Task 3: Budget and summarize complete inputs

**Files:** `src/commit/ai/input.rs`, `src/commit/ai/input/tests.rs`, `src/commit/ai/transport.rs`, `src/commit/ai.rs`

1. Measure the actual serialized chat-completions request, including JSON escaping, prompt, metadata, model, and the summary output limit field.
2. Send complete small inputs once; split large inputs at file/hunk/line boundaries, with UTF-8-safe fragments for oversized single lines.
3. Include repository/path/status/statistics on every fragment and preserve all selected file identities.
4. Summarize all chunks sequentially, reduce oversized summaries without dropping coverage, then use the custom prompt for final generation.
5. Enforce request count, per-request timeout, overall deadline, bounded responses, and sanitized failures. Never adopt partial results.
6. Test exact serialized budgets, escaped Unicode, oversized lines/hunks, reductions, and limit failures.

### Task 4: Run preparation and generation in the background

**Files:** `src/commit/ai.rs`, `src/commit/ai/tests.rs`, `src/commit/view.rs`

1. Freeze only cheap panel state on the UI thread, then start the worker immediately.
2. Report preparing, summarizing, reducing, generating, and checking stages through the existing pending channel.
3. Cancel stale selection/scope jobs and reject changed source versions before adopting output.
4. Retain active-input deferral, duplicate-click protection, newer-draft preservation, and rollback on persistence failure.
5. Test delayed filters, local HTTP multi-request workflows, intermediate failures, stale Roots/checkboxes/content, editing, and small-terminal rendering.

### Task 5: Document and verify

**Files:** `README.md`, `docs/configuration.md`, `docs/design.md`, `docs/prd.md`, `docs/architecture.md`

1. Document defaults, byte-budget semantics, multi-request costs, limits, progress, source validation, and restart precedence.
2. Run focused AI/backend/config checks, then `cargo test --locked`.
3. Run `cargo fmt --all --check`, `cargo clippy --locked --all-targets -- -D warnings`, and `git diff --check`.
4. Preserve preexisting edits. Do not stage, commit, push, publish, or call a real model.

### Verification

- macOS: `cargo test --locked --all-targets -- --test-threads=6` finished with 802 passed, one ignored, and one Push test that timed out waiting for a local push. The Push test passed when rerun on its own. All 242 Commit tests that ran and all 12 configuration tests passed. The Commit tests include 48 AI planning and integration cases.
- Both binary test targets passed when run separately. Configuration validation through `igit --check-config`, formatting, Clippy with warnings denied, and `git diff --check` passed.
- `cargo check --locked --all-targets --target x86_64-pc-windows-msvc` passed with existing unused-import warnings outside this change. Runtime checks ran on macOS.
- AI integration tests used temporary repositories and local HTTP mocks.
