# Common Development File Icons Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Consistently show common development-language icons in Commit, Log, and Push file lists and existing file preview titles. Java must no longer fall back to a generic file, and React must be distinguishable from ordinary JS / TS.

**Architecture:** Extend the shared filename/extension mapping in `src/ui/icons.rs`, continuing to use existing named `nerd-font-symbols` constants and type colors. Filename rules take precedence over extensions. Preserve unknown-type, non-UTF-8 path, and Unicode / ASCII fallbacks. All panels continue using the same entry point, without additional file reads or dependencies.

**Tech Stack:** Rust, Ratatui, nerd-font-symbols, unicode-width; no new dependencies.

---

1. **Mapping and fallbacks** (`src/ui/icons.rs`): add Java / JVM, React JSX / TSX, C / C++ / C#, common scripting and mobile development languages, XML, and build files. Supply ordinary-character fallbacks for every icon; JSX / TSX retain language abbreviations and type colors. Preserve path content, Git status colors, and cell-measured layout logic.
2. **Review existing conventions** (existing tests in `src/ui/icons.rs`): update the TSX expectation to React. Run `cargo test --locked ui::icons::` to verify glyphs, fallbacks, case handling, filename precedence, and non-UTF-8 paths. Reuse existing cross-panel layout, color, and mouse tests; do not duplicate a test table entry for every static mapping.
3. **Documentation and validation** (`README.md`, `docs/design.md`, `docs/prd.md`): explain coverage and matching precedence. Run `cargo test --locked`, `cargo fmt --all --check`, and `cargo clippy --locked --all-targets -- -D warnings`. In temporary demo repositories, inspect Java, Python, React, and TypeScript display in Commit / Log / Push. Confirm that HEAD, index, and worktree remain unchanged after browsing. Do not commit or push in the development repository.

## Validation record (2026-11-05)

- `cargo test --locked`, `cargo fmt --all --check`, and `cargo clippy --locked --all-targets -- -D warnings` passed. The pre-commit check of the shared working tree included 196 unit tests and 3 integration tests.
- A temporary `languages` repository and local bare remote contained `App.java`, `Component.jsx`, `Mapper.xml`, `script.js`, `Typed.tsx`, `types.ts`, and `Worker.py`, with outgoing commits and working-tree changes. `App.java` had both staged and unstaged changes.
- Pseudoterminal checks passed for Nerd Font, Unicode, and ASCII at 180×48, 120×36, and 80×24: all seven file markers appeared in Commit, Log, and Push, and the Java Diff preview header used the same Java marker. Narrow layouts were checked by scrolling through each list.
- HEAD, index entries, staged and unstaged patches, and porcelain status were identical before and after browsing. No push or commit was performed through the UI.
