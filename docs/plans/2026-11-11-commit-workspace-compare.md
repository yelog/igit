# Commit to Workspace Comparison Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Compare the highlighted Log commit with its repository's actual working tree through `D` and Commit Actions.

**Architecture:** Retain the ordinary Log state while a temporary read-only workspace reuses its file inspector and the shared two-revision renderer. Read cumulative changes against the captured immutable commit in a private temporary index, including current tracked and non-ignored untracked paths; never refresh or write the real index. Generation-safe background reads bind lists and previews to the original repository and current Roots.

**Tech Stack:** Rust, Ratatui, Crossterm, system Git, existing tempfile and background-reader support; no new dependencies.

## 1. Share bounded working-tree reads

- Extract Commit preview's guarded working-source reader into `src/core/revision/working.rs`, exposed through `src/core/revision.rs`.
- Preserve size, encoding, attribute, symlink-parent, nested-repository, and line-ending handling in existing Commit previews.
- Add `src/log/workspace/backend.rs` for cumulative changed paths and bounded Commit/Working tree content, including additions, deletions, mixed staging, literal byte paths, and explained unsupported entries.

## 2. Add the temporary comparison workspace

- Create `src/log/workspace.rs` with an independent file inspector, adjacent two-pane Diff, direction banner, refresh and return controls.
- Use the existing latest-request readers for list and preview work. Cancel stale results on refresh, Roots changes, selection changes, and close.
- Adapt Log focus, search, copy, help, input, refresh, drawing, and wakeup routes without changing its retained filters, selected commit, or cherry-pick checks.
- Keep comparisons read-only, including files and unsupported-content fallbacks.

## 3. Add entry points and documentation

- Extend `src/log/actions.rs`, `src/log/keymap.rs`, `src/config/keys.rs`, and `config/default.toml` with `D` / `compare-with-local` and a captured-target Commit Actions entry.
- Preserve File History's existing `D` behavior and expose configurable labels in Shell footers.
- Update README, interaction design, architecture, requirements, and configuration guidance.

## 4. Validate behavior

- Start with one focused disposable-repository test through `python3 scripts/cargo_serial.py test --locked --lib workspace_compare`.
- Cover cumulative committed and local changes, mixed staging, unselected staged siblings, deletions/recreation, untracked/ignored paths, Unicode/literal paths, binary/large files, symlinks, multiple repositories, stale reads, empty states, and unchanged HEAD/index/refs/files.
- Exercise real key/mouse/menu/help/focus/remapping routes and 44×12, 80×25, 120×36, and 184×48 rendering.
- Run relevant Log, Commit-preview, Shell, and configuration regressions, then formatting, Clippy and diff checks. Preserve pre-existing edits; do not commit or push.

## Validation — 2026-11-11

- Focused comparison tests: 13 passed, including the Shell remapping/help/focus route, cumulative working contents, staged deletion/recreation, same-OID repositories, stale list/preview reads, mouse/keyboard resizing, narrow layouts in all icon modes, Unicode/literal/native byte paths, symlinks, size limits, and unavailable history.
- Real PTY smoke test: the ordinary Log listed 19 cumulative changes; a file with staged and unstaged edits showed its final working content beside the captured commit. The comparison footer exposed `[/] Revisions`, `F5 Refresh`, and `Esc Log`; returning restored the ordinary Log. A SHA-256 snapshot of all 52 repository and `.git` files was identical before and after both runs.
- Log regressions: 202 passed before the final boundary and footer tests. The focused comparison suite was rerun after the footer-routing fix and passed.
- The focused comparison suite also covers `J/K` scrolling the right preview from the file tree and Diff panes without moving the file selection or changing Git state.
- Full suite: 1023 passed, 2 failed, 1 ignored. The configuration remap failure passed when rerun alone against the concurrent final tree. The Help search color failure reproduces alone at 44×12 in pre-existing user-edited Help rendering and does not exercise this comparison.
- Local documentation links: 259 resolved. `cargo fmt --all --check` and `git diff --check` passed.
- Strict Clippy reached an unrelated existing `clippy::obfuscated_if_else` warning in `src/shell.rs`'s notification hint. That code was preserved. All-target Clippy passed with only that rule temporarily allowed on the command line; no lint suppression was added to the source.
