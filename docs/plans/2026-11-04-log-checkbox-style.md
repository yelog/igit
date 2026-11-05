# Log Checkbox Style Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Give the Log Show, Highlight, and Columns menu checkboxes the same blue checked and gray unchecked styles as Commit.

**Architecture:** Reuse `checkbox` from `src/ui/idea.rs` and draw checkboxes separately from text in `src/log/display.rs` menu rows. Preserve existing row backgrounds and disabled text styles, with equal-width blank space for the Columns entry. Truncate text to the remaining terminal cell width.

**Tech Stack:** Rust, Ratatui, Crossterm, existing menu interaction tests, local temporary Git workspaces, and pseudoterminals.

---

### Task 1: Fix menu row style composition

**Files:** Modify `src/log/display.rs:345`; reuse `src/ui/idea.rs:31`.

1. Before making changes, confirm that `menu_row` embeds the glyph in the entire row string, causing the checkbox to inherit the text color.
2. Import the shared `checkbox`. Use an equal-width blank `Span` for the Columns entry and the shared checkbox `Span` for the remaining rows.
3. Subtract the checkbox width and two spacing cells from the available width, then truncate text separately. Keep the existing row background and hit-target logic.

Core rendering approach:

```rust
let mark = if matches!(hit, MenuHit::Item(Item::Columns)) {
    Span::raw(" ".repeat(cells(self.icons.check(false)).into()))
} else {
    checkbox(self.icons, checked)
};
let width = usize::from(r.width).saturating_sub(mark.width() + 2);
let line = Line::from(vec![
    Span::raw(" "),
    mark,
    Span::raw(format!(" {}", clipped(label, width))),
]);
f.render_widget(Paragraph::new(line).style(style), r);
```

### Task 2: Validate actual rendering and interaction

1. Run `cargo test --locked view_options`, covering three icon modes, the main menu and Columns, short windows, hover, mouse, keyboard, and column reordering.
2. Run `cargo fmt --all --check` and `cargo build --locked --bin igit`.
3. Open the main menu and Columns in a temporary demo repository through a pseudoterminal. Inspect actual ANSI colors for bold blue checked boxes, gray unchecked boxes, text colors, selected row backgrounds, and disabled items. Compare repository HEAD, index, and worktree state before and after.

### Task 3: Update the interaction baseline

**Files:** Modify the Log View Options description in `docs/design.md`.

Document that menu checkboxes use Commit's colors and that row highlighting is independent of checkbox color. Preserve existing workspace changes; do not commit or push.

### Completion and validation

- `cargo test --locked view_options`: 10 tests passed; `cargo fmt --all --check` and `cargo build --locked --bin igit` passed.
- Pseudoterminal validation covered Nerd Font, Unicode, and ASCII modes, each at 80×24 and 120×36. Across six runs, 60 status rows were checked: checked/unchecked states, navigation highlighting, disabled text, the main menu, and Columns all matched expectations.
- HEAD, staged entries, and worktree state in both temporary demo repositories remained unchanged.
