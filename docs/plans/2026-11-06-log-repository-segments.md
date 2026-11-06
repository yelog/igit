# Log Repository Segments Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Make repository transitions in Log clearly visible while preserving the Paths color mapping, row density, selection, and scrolling.

**Architecture:** Treat consecutive visible commits with the same repository ID as one segment. Render a centered heavy vertical stroke with shortened end caps, an aligned short stroke for isolated commits, and a neutral gutter. Determine neighbors from the entire filtered history so viewport clipping does not create false segment boundaries.

**Tech Stack:** Rust, Ratatui, standard Unicode box drawing, existing TestBackend fixtures.

---

## Design decision

The previous quarter block is flush against the pane border, narrow, and continuous across repository transitions. Its color alone carries the boundary. A centered heavy stroke and explicit gap add position and shape cues without changing repository colors or adding rows.

Use `╻` at a segment start, `┃` inside, `╹` at its end, and `❙` for an isolated commit. The isolated marker is also a vertical stroke, with space above and below; avoid mixing dots and lines when repositories alternate frequently. Standard terminal strokes have square half-line caps; these are an intentional portable approximation of IDEA's separated rounded bars, not pixel-exact rounded rectangles. Reset bold and reverse styles in the marker gutter so commit highlighting cannot alter it. Root Names remains adjacent.

References: [kitty's standard half-line drawing](https://github.com/kovidgoyal/kitty/blob/v0.36.2/kitty/fonts/box_drawing.py#L48-L50) and [graphics placement lifecycle](https://sw.kovidgoyal.net/kitty/graphics-protocol.html). Pixel images would require a new terminal-dependent rendering and cleanup path for this small control; standard glyphs fit the existing architecture.

### Task 1: Render repository segments

**Files:** Modify `src/log/view.rs`.

1. Add a local helper deriving the marker from adjacent entries in `visible`, comparing repository IDs rather than palette colors.
2. Replace the quarter block with the segment marker. Keep the existing width budget and use the panel background for its gutter.
3. Retain row hit areas, Root Names, and all configurable columns.

### Task 2: Verify boundaries and presentation

**Files:** Modify `src/log/view_tests.rs`; add `src/log/view_tests/repository_segments.rs`.

1. Cover mixed adjacent repositories, singleton segments, and distinct repositories sharing a palette color.
2. Verify scrolling and resizing inside a segment preserve continuation strokes; filtering uses displayed adjacency.
3. Verify the neutral gutter and unchanged content selection, plus the Paths color mapping across icon modes.
4. Run `cargo test --locked log::` and formatting checks on changed Rust files.

### Task 3: Document and inspect

**Files:** Modify `README.md` and `docs/design.md`.

1. Describe separated segments, isolated short strokes, stable colors, and terminal cap limitations.
2. Inspect a temporary multi-repository render containing both long and single-row segments.

Preserve unrelated working-tree edits. No commit or remote operation is part of this task.
