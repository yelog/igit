# Git Log Date Filter Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Change the Log Date menu to `Select...`, `Last 24 hours`, and `Last 7 days`, with the first item editing a custom start/end date range.

**Architecture:** Add fixed date options and a two-field range editor to the existing filter menu. Reuse cancellation, clearing, and tab restoration behavior. Represent the date condition with a separate enum and produce relative time or local date boundaries through the existing Git history query, without new dependencies.

**Tech Stack:** Rust, Ratatui TestBackend, Crossterm, system Git.

---

### Task 1: Fixed menu and range input

**Files:** `src/log.rs`, `src/log/filters.rs`, `src/log/view.rs`, `src/log/view_tests.rs`.

1. Add rendering and input tests requiring the Date menu to show exactly three options in screenshot order. Run `cargo test --locked log::view_tests::date_menu` and confirm that the old implementation fails.
2. Select the first menu item by default; support arrow keys / `j/k` and the mouse. The first item opens From / To editing in `YYYY-MM-DD` format: `Tab` switches fields, `Ctrl-U` clears, `Enter` applies, and `Esc` cancels.
3. Validate both dates and their order before applying. Preserve the draft and applied filter on errors. Clicking outside the menu cancels the draft while allowing that same click to take effect.
4. Check options, input, and hit targets at 48, 80, 120, and 180 columns. Verify independent date clearing and tab restoration.

### Task 2: Real history queries

**Files:** `src/log/model.rs`, `src/log/model/tests.rs`.

1. Use `Option<DateFilter>` for no condition, the last 24 hours, the last 7 days, and a custom range. Produce readable toolbar and tab labels.
2. Relative ranges use `--since-as-filter=24 hours ago` / `--since-as-filter=7 days ago` and `--until=now`. Custom ranges run from local `00:00:00` on the start date through `23:59:59` on the end date.
3. Use temporary repositories and real Git to verify recent ranges, exclusion of future timestamps, ancestors with out-of-order timestamps, and full-day inclusion at both custom range boundaries.

### Task 3: Documentation and regression checks

**Files:** `README.md`, `docs/design.md`, `docs/prd.md`.

1. Document menu options, date input format, keyboard/mouse actions, and date filter acceptance criteria.
2. Run `cargo test --locked log::`; expect the date model, UI, and existing log cases to pass.
3. Run `cargo test --locked`, `cargo fmt --all --check`, and `cargo clippy --locked --all-targets -- -D warnings`, recording actual results. Do not commit or push.

### Validation record (2026-11-04)

- `cargo test --locked date_`: all 6 date tests passed, covering real Git time boundaries and keyboard/mouse input at 48 / 80 / 120 / 180 columns.
- `cargo test --locked`: 83 unit tests and 3 integration tests passed.
- `cargo build --locked --bins` passed. Real pseudoterminal checks at 120×36 and 80×24 in a temporary Log demo workspace covered applying and reediting a range, both preset periods, and exit. HEAD, index entries, and worktree state remained unchanged in both repositories.
- The final global format and Clippy checks had not passed: concurrently added view options still had formatting differences, and `menu_row` in `src/log/display.rs` triggered `too_many_arguments`. The date implementation preserved those concurrent changes.
