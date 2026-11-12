# Log User Filter Design Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Make Log's User dropdown readable and discoverable, with clear search, grouped choices, independent applied/hover states, and usable narrow-terminal controls.

**Architecture:** Keep the existing author filter, loaded-history suggestions, per-tab scope, and Git execution paths. Give the author menu its own Ratatui renderer and a stable scroll offset; reuse shared colors, icons, and dropdown event dispatch.

**Tech Stack:** Rust, Ratatui 0.29, Crossterm, Unicode cell metrics, existing disposable Git fixtures and TestBackend; no new dependencies.

---

### Task 1: Cover the author menu's missing states

**Files:** `src/log/view_tests.rs`, `src/log/view_tests/author_menu.rs`.

1. Cover empty search guidance, Quick filters / Authors grouping, the currently applied checkmark, and keyboard focus.
2. Reopen an applied author with an empty search draft; verify that Escape retains the condition.
3. Exercise long Unicode names, no matches, custom author input, scrolling, hover stability, and mouse targets at 44×12, 80×24, 120×36, and 180×36 in all icon modes.
4. Run `python3 scripts/cargo_serial.py test --locked --lib author_menu` and observe a targeted failure before implementation.

### Task 2: Implement a compact author picker

**Files:** `src/log/authors.rs`, `src/log.rs`, `src/log/filters.rs`, `src/log/view.rs`, `src/log/keymap.rs`, `src/log/branches.rs`.

1. Render a neutral rounded surface with inset search, quiet section headings, aligned secondary labels, a distinct applied marker, and a blue focused row.
2. Show a scrollbar for overflow and explicit no-match guidance, with responsive Select / Apply text and Close actions.
3. Keep the scroll viewport stable on hover; support arrows, paging, Home/End, Tab, wheel, clickable search clearing, and scrollbar seeking.
4. Preserve typed-author filtering, cancellation, other filters, and per-tab state; author suggestions remain based on loaded history.

### Task 3: Verify and document the delivered interaction

**Files:** `README.md`, `docs/design.md`, `docs/prd.md`, `docs/reference/IGit-Log-User.png`.

1. Run focused author-menu tests, then the Log regression tests and formatting/Clippy checks using `scripts/cargo_serial.py`.
2. Inspect real Ratatui cell renders at normal and narrow sizes; deliver a visual reference generated from those cells.
3. Document search/apply semantics and keyboard/mouse controls, and check that disposable repository HEAD, index, and working files remain unchanged.
4. Record actual verification results. Do not commit, push, or modify user repositories.

## Design Decisions

- Use the existing dark palette with a slightly raised neutral popup, quiet
  rounded borders, inset search, and a single blue accent.
- Separate shortcuts from authors; align secondary descriptions and reserve a
  dedicated checkmark column so the applied condition survives navigation.
- Keep text input immediately available. Typing clears the list highlight, so
  Enter can still apply a custom author condition without a matching suggestion.
- Short windows remove the shortcut heading and footer divider, keeping actual
  authors visible rather than spending every available row on decoration.
- A stable viewport prevents hover from moving a row before a following click.

## Observed Verification — 2026-11-11

- The initial two targeted tests failed on the prefilled search and missing
  no-match guidance before implementation.
- `python3 scripts/cargo_serial.py test --locked --lib author_menu`: 6 passed;
  the temporary screenshot export test was subsequently removed.
- `python3 scripts/cargo_serial.py test --locked --lib log::`: 215 passed,
  including all six new author-menu tests.
- `python3 scripts/cargo_serial.py test --locked --lib shell::help_tests`:
  18 passed.
- `python3 scripts/cargo_serial.py fmt --all --check` and `git diff --check`:
  passed.
- Strict Clippy found an existing `clippy::obfuscated_if_else` warning in
  `src/shell.rs:1447`. After fixing the new menu's separate `int_plus_one`
  warning, `python3 scripts/cargo_serial.py clippy --locked --all-targets -- -D warnings -A clippy::obfuscated_if_else`
  passed. No warning suppression was added to source.
- Inspected the actual Ratatui TestBackend cells for browsing, search, no matches,
  and a compact 44×12 terminal; saved the browsing reference as
  `docs/reference/IGit-Log-User.png`.
- Tests cover three icon modes, Unicode and long names, keyboard/mouse targets,
  hover stability, scrolling, cancellation, and unchanged HEAD, real index bytes,
  working file bytes, and Git status in a disposable repository.
- All 256 relative documentation links checked in README, design, PRD, and
  architecture resolve. No interactive PTY walkthrough or full repository test
  suite was run.
