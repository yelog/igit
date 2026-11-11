# Branch Comparison Implementation Plan

**Goal:** Open an IDEA-style two-way commit comparison from a non-current local or remote branch.

**Architecture:** Branch owns a temporary comparison workspace and retains its browsing state when it closes. Two embedded Log panels share the existing graph, filters, inspector, navigation and revision Diff implementation. A background reader resolves the two exact refs together; both histories use the same immutable commit pair and stay restricted to the selected repository.

**Tech Stack:** Rust, Ratatui, Crossterm, system Git, existing bounded background readers.

## 1. Constrain Log history to a comparison range

- Modify `src/log/model.rs`, `src/log/loading.rs`, `src/log/filters.rs`, `src/log/view.rs`, and `src/log.rs`.
- Add a repository-bound immutable include/exclude range. Load `current..target` above and `target..current` below, retaining merge topology.
- Keep text, author, date, paths, columns, pagination, and read-only Diff. Reset must preserve the range. Hide branch selection and unrelated commit-write/tab actions.
- Render an amber direction banner and accurate empty, filtered-empty, loading, and error states.

## 2. Compose the comparison workspace

- Create `src/log/compare.rs`; extend `src/ui/panes.rs`.
- Resolve both full refs off the terminal thread and reject unavailable refs or repositories outside Roots.
- Tile two independent Log inspectors vertically with aligned columns and adjustable dividers. Route focus, mouse, search, copy, overlays, and keyboard navigation through the active half.
- Keep refreshes generation-safe and preserve both selections and filters. Esc/close returns to Branch.

## 3. Expose Branch entry points

- Modify Branch events, rendering, help, loading, and search adapters; update Shell polling and configuration.
- Add `c` and a right-click `Compare with <current>` item only for a non-current leaf with a committed local current branch.
- Support local and remote targets, dynamic current names, multiple repositories, remapped keys, and detached/unborn guidance.

## 4. Verify behavior and document delivery

- Add disposable Git fixtures for diverged, ancestor, identical, merge, remote, Unicode, and missing-ref cases.
- Exercise actual input and rendered cells at 44×12, 80×25, 120×36, and 184×48, including independent selection, filters, Diff, resizing, return, and Roots changes.
- Compare HEAD, refs, index bytes, worktree content, and reflogs before and after browsing.
- Update `README.md`, `docs/design.md`, `docs/architecture.md`, and `docs/prd.md`.
- Run focused tests first with `python3 scripts/cargo_serial.py test --locked --lib compare`, then relevant integration checks, formatting, Clippy, and the full suite for shared routing changes.

## Verification — 2026-11-11

- Final comparison regression run: 25 passed, including compact hit areas, bounded reader cancellation, stale previews, remote/Unicode/merge history, paging, immutable Git state, and configured Shell routing.
- Log pane regression run: 3 passed.
- All six CLI integration targets: 10 passed.
- `fmt --all --check`, Clippy with `-D warnings`, binary build, binary test targets, doc tests, and `git diff --check`: passed.
- Full-suite attempt: 994 passed, 1 ignored, and 1 existing help-palette test failed because this execution environment sets `NO_COLOR`. The unchanged help implementation intentionally returns reset colors in that environment. Running that exact test with `env -u NO_COLOR` passed. A second full-suite run with the variable cleared was not performed; the final comparison and pane regressions cover the subsequent comparison-only and shared compact-divider refinements.
- Final local documentation link check: 256 links resolved across README, PRD, interaction design, architecture, configuration, and this plan.
- An actual PTY walkthrough used a disposable `client1` repository matching the reference's ancestor scenario: open Branch, compare `test` with `main`, inspect the empty upper list and five lower commits, open Before/After Diff, return, and quit. HEAD, refs, the real index, and working content matched the pre-walkthrough snapshot.
- Terminal captures are outside the repository in the chat's visualization directory as `branch-compare-ancestor.png` and `branch-compare-diff.png`. No production dependency or persistent comparison state was added.
