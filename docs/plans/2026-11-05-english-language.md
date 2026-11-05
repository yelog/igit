# English Language Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Make all repository-owned text English, using concise, idiomatic labels that remain usable in narrow terminal panels.

**Architecture:** Translate existing strings in place without adding a localization layer or changing Git operations. Keep shortcuts and native paths intact, preserve existing user changes, and retain Unicode coverage with fullwidth Latin fixtures where terminal-cell and UTF-8 boundaries matter.

**Tech Stack:** Rust, Ratatui, Crossterm, Markdown, HTML, shell; no new dependencies.

---

### Task 1: Inventory and preserve existing work

- Inspect `AGENTS.md`, `README.md`, `docs/design.md`, `docs/prd.md`, and `docs/architecture.md` and scan repository text for Han characters.
- Preserve the existing changes in `src/ui/icons.rs` and `docs/plans/2026-11-05-file-type-icons.md`; translate their text without reverting their implementation.
- Inspect reference images for embedded language that text search cannot detect.

### Task 2: Translate contextual help and key sequences

- Translate `src/{log,commit,push,merge}/keymap.rs`, `src/ui/help.rs`, `src/ui/key_sequence.rs`, and `src/ui/panes.rs`.
- Use short action-first descriptions and consistent terms: repository, scope, selection, review, stage, commit, push, and resolve.
- Keep registered keys and commands unchanged. Check help filtering, wrapping, hit testing, and key-sequence overlays at narrow widths.

### Task 3: Translate runtime messages and preserve Unicode tests

- Translate remaining messages and comments in `src/`, including shell dialogs, Commit review and AI prompts, Log filters, and clipboard errors.
- Update affected test expectations and example content. Replace language-specific fixtures with English or fullwidth Latin while preserving the tested byte and cell boundaries.
- Check `src/shell/{help_tests,key_sequence_tests,workspace_tests}.rs`, panel view tests, and `src/ui/selection.rs` against the translated strings.

### Task 4: Translate maintained documentation

- Translate `AGENTS.md`, `README.md`, and `docs/{design,prd,architecture}.md` completely, preserving links, commands, behavioral guarantees, and historical acceptance dates.
- Document English as the application language while preserving support for Unicode repository content.

### Task 5: Translate historical references and plans

- Translate `docs/plans/*.md` and `docs/reference/{IGit,IGit-Design}.html`; set HTML document language to English.
- Preserve historical scope and implementation claims; inspect reference layouts after longer labels are translated.
- Refresh reference screenshots containing application-owned Chinese text from their actual source where possible.

### Task 6: Verify the integrated result

- Run focused help, key-sequence, selection, and panel tests while integrating.
- Run `cargo test --locked`, `cargo fmt --all --check`, and `cargo clippy --locked --all-targets -- -D warnings`.
- Review the final diff, verify local documentation links, and repeat a repository-wide Han-character scan excluding generated build output and Git internals.
- Exercise English help and review dialogs at 120x36, 80x24, and 44x12 through existing render tests and local demo smoke checks. Report any remaining unverified artifact explicitly.
- Do not commit or push.

## Validation Results — 2026-11-05

- Translated all repository-owned Chinese text. Final scans of 104 tracked and untracked text files found no Han characters or encoded Han text; relative Markdown links resolve.
- Kept the existing file-icon implementation and its documentation. Unicode tests retain fullwidth Latin, combining-character, emoji, and native-path coverage.
- Help uses a narrower shortcut column in compact windows and complete footer hints. Commit editor hints adapt to available width; review titles use the correct repository plural.
- `cargo test --locked`: 196 unit tests and 3 integration tests passed. After final wording polish, all 17 shell help tests and both Commit flow tests passed again.
- `cargo fmt --all --check` and `cargo clippy --locked --all-targets -- -D warnings` passed.
- Real PTY walkthroughs passed at 120×36, 80×24, and 44×12: workspace, help/search, commit message editor, commit review/cancel, Push, Merge, and quit. All three demo repositories retained their HEAD, index entries, staged/unstaged diffs, and status.
- Browser checks passed for all four prototype panels at 660, 800, and 1200 px and compact help/AI dialogs. The illustrated design document has no horizontal overflow at 660/1200 px; all images load. Embedded JavaScript passed `node --check`.
- OCR found no Han text in 12 reference PNGs or 4 embedded screenshots. Image bytes were preserved.
