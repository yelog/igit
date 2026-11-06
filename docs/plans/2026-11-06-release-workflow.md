# Release Workflow Implementation Plan

> Follow the tasks below in order; no release, commit, tag, or push is part of this implementation request.

**Goal:** Adapt LazyDB's repository release skill and GitHub Actions workflow to IGit, with validated beta/stable versions and downloadable archives containing both commands.

**Architecture:** Repository scripts own version selection, Cargo/Changelog consistency, packaging, checksums, and artifact smoke tests. A reusable CI workflow tests and builds five native targets; Release validates an existing annotated tag and publishes only the artifacts from that exact commit. Keep publication separate from preparation and never overwrite published assets.

**Tech Stack:** Rust 1.94.0, Python 3.11+ standard library, Git, GitHub Actions, GitHub CLI.

---

### Task 1: Version and release metadata

- Add `scripts/release/next-version.py`, adapting LazyDB's tested decimal rollover and Beta policy.
- Add `scripts/release/metadata.py` for commit collection, scoped version edits, Changelog insertion/extraction, and tagged-source validation.
- Add fixture-based Python tests covering first release, Beta promotion, duplicates, lockfile preservation, malformed notes, and tag/HEAD/branch mismatches.
- Replace the hardcoded header version in `src/shell.rs` with `env!("CARGO_PKG_VERSION")`; test both binaries through `tests/version_cli.rs`.

### Task 2: Distribution and workflows

- Add `scripts/release/distribution.py` and tests for archives containing both binaries, documentation/config assets, executable permissions, exact asset inventory, and checksums.
- Smoke-test the extracted binaries' version, help, config validation, and repository discovery in an isolated temporary workspace.
- Add pinned Rust configuration, `.github/workflows/ci.yml`, and `.github/workflows/release.yml`.
- Build macOS Intel/ARM64, Linux x86_64/ARM64 musl, and Windows x86_64 MSVC. Reuse the CI-produced archives for Release; publish only after all checks pass.

### Task 3: Skill and documentation

- Add `.agents/skills/release/SKILL.md`, its UI metadata, and Changelog guidance. Support prepare-only, publish, and verified resume without implying that skill edits authorize publication.
- Add `CHANGELOG.md` with an Unreleased section and `docs/releasing.md` with invocation, target matrix, artifact contents, checks, and recovery boundaries.
- Update only release-related sections of `README.md`, `AGENTS.md`, and `docs/prd.md`; preserve existing worktree changes.

### Task 4: Verification

- Run `python3.11 -m unittest discover -s scripts/release -p 'test_*.py'` and the skill validator.
- Run `cargo fmt --all --check`, `cargo test --locked`, `cargo clippy --locked --all-targets -- -D warnings`, and `cargo build --release --locked --bins` sequentially.
- Package and smoke-test both native macOS binaries, inspect archive entries/checksums, and lint the GitHub Actions YAML.
- Report remote/Linux/Windows checks as pending until GitHub Actions actually runs. Do not claim a release was published.

## Verification outcome

- 310 Rust tests passed on macOS ARM64 with Rust 1.94.0; formatting, Clippy with warnings denied, and the release build passed.
- 24 release-tool tests passed, including version policy, metadata consistency, archive inventory/permissions/checksums, draft lookup, partial-upload resume, and refusal to replace published assets.
- Actionlint 1.7.12 accepted both workflows. The skill validator, Python lint, and relative documentation-link checks passed.
- A native release archive was extracted and both commands passed version/help/configuration/repository-discovery smoke checks in temporary directories.
- No commit, tag, push, workflow dispatch, or GitHub Release was created. Remote runners and interactive behavior on other platforms remain unverified locally.
