use super::*;
use ratatui::{backend::TestBackend, Terminal};

const BLUE_LINE: Color = Color::Rgb(31, 40, 50);
const BLUE_WORD: Color = Color::Rgb(43, 66, 93);
const GREEN_LINE: Color = Color::Rgb(38, 54, 45);
const GREEN_GUTTER: Color = Color::Rgb(46, 73, 56);
const GREEN_WORD: Color = Color::Rgb(57, 92, 66);
const RED_LINE: Color = Color::Rgb(35, 28, 28);
const RED_GUTTER: Color = Color::Rgb(53, 36, 32);

fn draw(
    chunks: &[Chunk],
    pane: usize,
    read_only: bool,
    view: &EditorView,
) -> (Terminal<TestBackend>, Columns) {
    draw_edit(chunks, pane, read_only, view, None)
}

fn draw_edit(
    chunks: &[Chunk],
    pane: usize,
    read_only: bool,
    view: &EditorView,
    edit: Option<(usize, &str, usize)>,
) -> (Terminal<TestBackend>, Columns) {
    let aligned;
    let chunks = if read_only {
        let texts = [
            chunks
                .iter()
                .map(|chunk| chunk.left.as_str())
                .collect::<String>(),
            chunks.iter().map(|chunk| chunk.result.as_str()).collect(),
            chunks.iter().map(|chunk| chunk.right.as_str()).collect(),
        ];
        aligned =
            crate::merge::view::revision::alignment::chunks(texts.each_ref().map(String::as_str));
        aligned.as_slice()
    } else {
        chunks
    };
    let map = RowMap::new(chunks, view, edit);
    let mut terminal = Terminal::new(TestBackend::new(100, 18)).unwrap();
    let area = terminal.get_frame().area();
    let columns = if read_only {
        Columns::read_only(pane, area, 2, true)
    } else {
        Columns::new(pane, area, 2, true)
    };
    terminal
        .draw(|frame| {
            fill(frame, area, EDITOR_BG);
            Rows {
                chunks,
                read_only,
                file: "example.js",
                view,
                map: &map,
                selected: 0,
                side: pane,
                scroll: 0,
                caret: edit.map(|(_, _, row)| (row, 0)),
                active: false,
                ascii: false,
            }
            .draw(frame, pane, area, columns, &mut Vec::new());
        })
        .unwrap();
    (terminal, columns)
}

fn assert_row(
    terminal: &Terminal<TestBackend>,
    columns: Columns,
    row: u16,
    line: Color,
    gutter: Color,
) {
    let buffer = terminal.backend().buffer();
    let y = columns.code.y + row;
    assert_eq!(
        buffer[(columns.code.right() - 1, y)].bg,
        line,
        "background extends to the end of the code row"
    );
    for x in columns.numbers.x..columns.numbers.right() {
        assert_eq!(buffer[(x, y)].bg, gutter, "line-number cell at {x},{y}");
    }
}

fn assert_fragment(
    terminal: &Terminal<TestBackend>,
    columns: Columns,
    row: u16,
    text: &str,
    fragment: &str,
    offset: usize,
    background: Color,
) {
    let start = text.find(fragment).unwrap();
    let first = cell_width(&text[..start]).saturating_sub(offset);
    let last = cell_width(&text[..start + fragment.len()]).saturating_sub(offset);
    let mut x = first;
    while x < last {
        let cell = &terminal.backend().buffer()[(columns.code.x + x as u16, columns.code.y + row)];
        assert_eq!(cell.bg, background, "{fragment:?} at cell {x}");
        x += cell.symbol().width().max(1);
    }
}

#[test]
fn added_revision_lines_have_green_numbers_and_full_width_backgrounds() {
    let chunks = diff3("const added = 1;\n\n", "", "const added = 1;\n\n");
    let (terminal, columns) = draw(&chunks, 1, true, &EditorView::default());
    for row in 0..2 {
        assert_row(&terminal, columns, row, GREEN_LINE, GREEN_GUTTER);
    }
    assert_fragment(
        &terminal,
        columns,
        0,
        "const added = 1;",
        "const added = 1;",
        0,
        GREEN_LINE,
    );
}

#[test]
fn replaced_revision_lines_use_blue_gutters_and_emphasize_only_the_replacement() {
    let before = "const value = 1;";
    let after = "const value = 2;";
    let chunks = diff3(
        &format!("{after}\n"),
        &format!("{before}\n"),
        &format!("{after}\n"),
    );
    for (pane, text, replacement) in [(0, before, "1"), (1, after, "2")] {
        let (terminal, columns) = draw(&chunks, pane, true, &EditorView::default());
        assert_row(&terminal, columns, 0, BLUE_LINE, BLUE_WORD);
        assert_fragment(&terminal, columns, 0, text, replacement, 0, BLUE_WORD);
        assert_fragment(&terminal, columns, 0, text, "const value", 0, BLUE_LINE);
    }
}

#[test]
fn a_modified_revision_line_can_have_both_blue_and_green_fragments() {
    let before = "const value = old; tail";
    let after = "const value = new; extra tail";
    let chunks = diff3(
        &format!("{after}\n"),
        &format!("{before}\n"),
        &format!("{after}\n"),
    );
    let (terminal, columns) = draw(&chunks, 1, true, &EditorView::default());
    assert_row(&terminal, columns, 0, BLUE_LINE, BLUE_WORD);
    assert_fragment(&terminal, columns, 0, after, "new", 0, BLUE_WORD);
    assert_fragment(&terminal, columns, 0, after, "extra ", 0, GREEN_WORD);
    assert_fragment(&terminal, columns, 0, after, "tail", 0, BLUE_LINE);
}

#[test]
fn inserted_lines_before_a_replacement_are_not_paired_with_the_old_line() {
    let before = "const value = old;\n";
    let after = "// added note\nconst value = new;\n";
    let chunks = diff3(after, before, after);
    let (terminal, columns) = draw(&chunks, 1, true, &EditorView::default());
    assert_row(&terminal, columns, 0, GREEN_LINE, GREEN_GUTTER);
    assert_row(&terminal, columns, 1, BLUE_LINE, BLUE_WORD);
    assert_fragment(
        &terminal,
        columns,
        1,
        "const value = new;",
        "new",
        0,
        BLUE_WORD,
    );
    let (terminal, columns) = draw(&chunks, 0, true, &EditorView::default());
    assert_row(&terminal, columns, 0, EDITOR_BG, EDITOR_BG);
    assert_fragment(
        &terminal,
        columns,
        1,
        "const value = old;",
        "old",
        0,
        BLUE_WORD,
    );
}

#[test]
fn a_replacement_between_an_inserted_and_a_deleted_line_keeps_each_line_kind() {
    let before = "const value = old;\nremoved_call();\n";
    let after = "// added note\nconst value = new;\n";
    let chunks = diff3(after, before, after);
    let view = EditorView::default();
    let (terminal, columns) = draw(&chunks, 1, true, &view);
    assert_row(&terminal, columns, 0, GREEN_LINE, GREEN_GUTTER);
    assert_row(&terminal, columns, 1, BLUE_LINE, BLUE_WORD);
    let (terminal, columns) = draw(&chunks, 0, true, &view);
    assert_row(&terminal, columns, 0, EDITOR_BG, EDITOR_BG);
    assert_row(&terminal, columns, 1, BLUE_LINE, BLUE_WORD);
    assert_row(
        &terminal,
        columns,
        2,
        Color::Rgb(42, 43, 48),
        Color::Rgb(62, 63, 72),
    );
}

#[test]
fn long_replacements_and_large_blocks_still_recognize_a_leading_inserted_line() {
    for (count, length) in [(1, 3000), (64, 40)] {
        let before = (0..count)
            .map(|row| format!("const value_{row} = '{}old';\n", "x".repeat(length)))
            .collect::<String>();
        let after = format!("// added note\n{}", before.replace("old", "new"));
        let chunks = diff3(&after, &before, &after);
        let view = EditorView::default();
        let (terminal, columns) = draw(&chunks, 1, true, &view);
        assert_row(&terminal, columns, 0, GREEN_LINE, GREEN_GUTTER);
        assert_row(&terminal, columns, 1, BLUE_LINE, BLUE_WORD);
    }
}

#[test]
fn historical_result_compares_additions_to_both_parents_without_conflict_colors() {
    let chunks = diff3(
        "const value = new;\nconst extra = 1;\n",
        "const value = old;\n",
        "const value = other;\n",
    );
    assert!(chunks.iter().any(|chunk| chunk.kind == Kind::Conflict));
    let (terminal, columns) = draw(&chunks, 1, true, &EditorView::default());
    assert_row(&terminal, columns, 0, BLUE_LINE, BLUE_WORD);
    assert_row(&terminal, columns, 1, GREEN_LINE, GREEN_GUTTER);
    assert!(!terminal
        .backend()
        .buffer()
        .content
        .iter()
        .any(|cell| cell.bg == RED_LINE));
}

#[test]
fn merge_sources_and_completed_results_share_the_addition_and_modification_rules() {
    let base = "const value = old; tail\n";
    let left = "const value = new; extra tail\nconst added = 1;\n";
    let mut chunks = diff3(base, left, base);
    let (terminal, columns) = draw(&chunks, 0, false, &EditorView::default());
    assert_row(&terminal, columns, 0, BLUE_LINE, BLUE_WORD);
    assert_row(&terminal, columns, 1, GREEN_LINE, GREEN_GUTTER);
    assert_fragment(
        &terminal,
        columns,
        0,
        left.lines().next().unwrap(),
        "extra ",
        0,
        GREEN_WORD,
    );
    for chunk in &mut chunks {
        chunk.use_side(FileSide::Current);
    }
    let (terminal, columns) = draw(&chunks, 1, false, &EditorView::default());
    assert_row(&terminal, columns, 0, BLUE_LINE, BLUE_WORD);
    assert_row(&terminal, columns, 1, GREEN_LINE, GREEN_GUTTER);
    let (terminal, columns) = draw(&chunks, 0, false, &EditorView::default());
    assert_row(&terminal, columns, 0, EDITOR_BG, EDITOR_BG);
    assert_row(&terminal, columns, 1, EDITOR_BG, EDITOR_BG);
}

#[test]
fn unresolved_merge_conflicts_keep_their_red_backgrounds() {
    let chunks = diff3(
        "const value = old;\n",
        "const value = new; added();\n",
        "const value = other;\n",
    );
    assert_eq!(chunks[0].kind, Kind::Conflict);
    for pane in 0..3 {
        let (terminal, columns) = draw(&chunks, pane, false, &EditorView::default());
        assert_row(&terminal, columns, 0, RED_LINE, RED_GUTTER);
        assert!(!terminal
            .backend()
            .buffer()
            .content
            .iter()
            .any(|cell| matches!(cell.bg, GREEN_LINE | GREEN_WORD)));
    }
}

#[test]
fn disabling_inner_diff_keeps_added_rows_green_and_modified_rows_light_blue() {
    let after = "const value = new; extra tail\nconst added = 1;\n";
    let chunks = diff3(after, "const value = old; tail\n", after);
    let view = EditorView {
        inner_diff: false,
        ..Default::default()
    };
    let (terminal, columns) = draw(&chunks, 1, true, &view);
    assert_row(&terminal, columns, 0, BLUE_LINE, BLUE_WORD);
    assert_row(&terminal, columns, 1, GREEN_LINE, GREEN_GUTTER);
    assert_fragment(
        &terminal,
        columns,
        0,
        after.lines().next().unwrap(),
        "new; extra tail",
        0,
        BLUE_LINE,
    );
}

#[test]
fn a_pure_inline_addition_has_a_blue_gutter_and_green_inserted_content() {
    let before = " * Agent type: text2sql";
    let after = " * Agent type: text2sql / mcp / component";
    let chunks = diff3(
        &format!("{after}\n"),
        &format!("{before}\n"),
        &format!("{after}\n"),
    );
    let (terminal, columns) = draw(&chunks, 1, true, &EditorView::default());
    assert_row(&terminal, columns, 0, BLUE_LINE, BLUE_WORD);
    assert_fragment(&terminal, columns, 0, after, before, 0, BLUE_LINE);
    assert_fragment(
        &terminal,
        columns,
        0,
        after,
        " / mcp / component",
        0,
        GREEN_WORD,
    );
}

#[test]
fn a_replaced_identifier_does_not_turn_into_scattered_green_insertions() {
    let before = "// text2sql must use a source";
    let after = "// Text-to-SQL must use a source";
    let chunks = diff3(
        &format!("{after}\n"),
        &format!("{before}\n"),
        &format!("{after}\n"),
    );
    for (pane, text, identifier) in [(0, before, "text2sql"), (1, after, "Text-to-SQL")] {
        let (terminal, columns) = draw(&chunks, pane, true, &EditorView::default());
        assert_row(&terminal, columns, 0, BLUE_LINE, BLUE_WORD);
        assert_fragment(&terminal, columns, 0, text, identifier, 0, BLUE_WORD);
        assert!(!terminal
            .backend()
            .buffer()
            .content
            .iter()
            .any(|cell| cell.bg == GREEN_WORD));
    }
}

#[test]
fn a_comment_can_replace_an_identifier_and_insert_unspaced_chinese_text() {
    let before = "// text2sql 类型必须有数据源，MCP 类型不需要\n";
    let after = "// Text-to-SQL 类型必须有数据源，MCP 和组件类型不需要。\n";
    let chunks = diff3(after, before, after);
    let (terminal, columns) = draw(&chunks, 1, true, &EditorView::default());
    assert_fragment(&terminal, columns, 0, after, "Text-to-SQL", 0, BLUE_WORD);
    assert_fragment(&terminal, columns, 0, after, "和组件", 0, GREEN_WORD);
    assert_fragment(&terminal, columns, 0, after, "类型不需要", 0, BLUE_LINE);
}

#[test]
fn replacements_against_either_parent_take_precedence_over_inline_additions() {
    for (left, right) in [("a c\n", "a x c\n"), ("a x c\n", "a c\n")] {
        let chunks = diff3("a b c\n", left, right);
        let (terminal, columns) = draw(&chunks, 1, true, &EditorView::default());
        assert_row(&terminal, columns, 0, BLUE_LINE, BLUE_WORD);
        assert_fragment(&terminal, columns, 0, "a b c", "b", 0, BLUE_WORD);
    }
}

#[test]
fn identical_parent_and_result_lines_stay_neutral_while_the_other_parent_changes() {
    let chunks = diff3(
        "const value = new;\n",
        "const value = old;\n",
        "const value = new;\n",
    );
    let (terminal, columns) = draw(&chunks, 2, true, &EditorView::default());
    assert_row(&terminal, columns, 0, EDITOR_BG, EDITOR_BG);
}

#[test]
fn line_ending_only_changes_keep_a_visible_blue_line_without_character_changes() {
    for (before, after) in [("same\r\n", "same\n"), ("same\n", "same")] {
        let chunks = diff3(after, before, after);
        for pane in 0..2 {
            let (terminal, columns) = draw(&chunks, pane, true, &EditorView::default());
            assert_row(&terminal, columns, 0, BLUE_LINE, BLUE_WORD);
            assert_fragment(&terminal, columns, 0, "same", "same", 0, BLUE_LINE);
        }
    }
}

#[test]
fn deleted_revision_rows_retain_the_existing_deletion_palette() {
    let chunks = diff3("", "removed\n", "");
    let (terminal, columns) = draw(&chunks, 0, true, &EditorView::default());
    let line = Color::Rgb(42, 43, 48);
    assert_row(&terminal, columns, 0, line, Color::Rgb(62, 63, 72));
    assert_fragment(&terminal, columns, 0, "removed", "removed", 0, line);
}

#[test]
fn tabs_wide_characters_and_combining_marks_keep_fragment_cell_colors_when_scrolled() {
    let before = "\tconst 名 = '旧'; tail";
    let after = "\tconst 名 = '新'; 🙂 e\u{301} tail";
    let chunks = diff3(
        &format!("{after}\n"),
        &format!("{before}\n"),
        &format!("{after}\n"),
    );
    let view = EditorView {
        hscroll: [0, 3, 0],
        ..Default::default()
    };
    let (terminal, columns) = draw(&chunks, 1, true, &view);
    assert_row(&terminal, columns, 0, BLUE_LINE, BLUE_WORD);
    assert_fragment(&terminal, columns, 0, after, "新", 3, BLUE_WORD);
    assert_fragment(&terminal, columns, 0, after, "🙂 e\u{301}", 3, GREEN_WORD);
    assert_fragment(&terminal, columns, 0, after, "tail", 3, BLUE_LINE);
}

#[test]
fn partial_decisions_keep_only_pending_sources_red_and_completed_results_use_normal_colors() {
    let mut chunks = diff3("base\n", "left\n", "right\n");
    chunks[0].decide(FileSide::Current, true);
    let (terminal, columns) = draw(&chunks, 0, false, &EditorView::default());
    assert_row(&terminal, columns, 0, EDITOR_BG, EDITOR_BG);
    for pane in [1, 2] {
        let (terminal, columns) = draw(&chunks, pane, false, &EditorView::default());
        assert_row(&terminal, columns, 0, RED_LINE, RED_GUTTER);
    }
    assert_eq!(chunks[0].result, "left\n");
    assert!(chunks[0].pending(FileSide::Incoming));
    chunks[0].decide(FileSide::Incoming, false);
    let (terminal, columns) = draw(&chunks, 1, false, &EditorView::default());
    assert_row(&terminal, columns, 0, BLUE_LINE, BLUE_WORD);
    assert_eq!(chunks[0].result, "left\n");
    assert!(chunks[0].done());
}

#[test]
fn active_result_edits_use_the_edit_buffer_and_allow_a_caret_after_the_final_newline() {
    let base = "const value = old;\n";
    let chunks = diff3(base, base, base);
    let edit = "const value = new;\nconst added = 1;\n";
    let (terminal, columns) = draw_edit(
        &chunks,
        1,
        false,
        &EditorView::default(),
        Some((0, edit, 2)),
    );
    assert_row(&terminal, columns, 0, BLUE_LINE, BLUE_WORD);
    assert_fragment(
        &terminal,
        columns,
        0,
        edit.lines().next().unwrap(),
        "new",
        0,
        BLUE_LINE,
    );
    assert_row(&terminal, columns, 1, GREEN_LINE, GREEN_GUTTER);
    assert_eq!(chunks[0].result, base);
    assert!(chunks[0].done());
}

#[test]
fn very_long_modified_lines_keep_the_line_tint_without_expensive_fragment_diffing() {
    let before = format!("{} = old;\n", "x".repeat(3000));
    let after = format!("{} = new;\n", "x".repeat(3000));
    let chunks = diff3(&after, &before, &after);
    let (terminal, columns) = draw(&chunks, 1, true, &EditorView::default());
    assert_row(&terminal, columns, 0, BLUE_LINE, BLUE_WORD);
    assert_fragment(&terminal, columns, 0, &after, &"x".repeat(30), 0, BLUE_LINE);
}

#[test]
fn cached_pairings_follow_result_edits_undo_and_document_reset() {
    let base = "const value = old;\n";
    let left = "const value = new;\nconst added = 1;\n";
    let mut chunks = diff3(base, left, base);
    let mut view = EditorView::default();
    draw(&chunks, 0, false, &view);
    chunks[0].use_side(FileSide::Current);
    let (terminal, columns) = draw(&chunks, 1, false, &view);
    assert_row(&terminal, columns, 1, GREEN_LINE, GREEN_GUTTER);
    chunks[0].resolve(base.into());
    let (terminal, columns) = draw(&chunks, 1, false, &view);
    assert_row(&terminal, columns, 0, EDITOR_BG, EDITOR_BG);
    chunks[0].resolve("// added note\nconst value = updated;\n".into());
    let (terminal, columns) = draw(&chunks, 1, false, &view);
    assert_row(&terminal, columns, 0, GREEN_LINE, GREEN_GUTTER);
    assert_row(&terminal, columns, 1, BLUE_LINE, BLUE_WORD);
    view.reset_document();
    let (terminal, columns) = draw(&chunks, 1, false, &view);
    assert_row(&terminal, columns, 0, GREEN_LINE, GREEN_GUTTER);
    assert_row(&terminal, columns, 1, BLUE_LINE, BLUE_WORD);
}

#[test]
fn an_unanalyzed_long_parent_does_not_leave_unconfirmed_green_fragments_from_the_other() {
    let long = format!("a {} c\n", "x".repeat(3000));
    for (left, right) in [("a c\n", long.as_str()), (long.as_str(), "a c\n")] {
        let chunks = diff3("a b c\n", left, right);
        let (terminal, columns) = draw(&chunks, 1, true, &EditorView::default());
        assert_row(&terminal, columns, 0, BLUE_LINE, BLUE_WORD);
        assert_fragment(&terminal, columns, 0, "a b c", "b", 0, BLUE_LINE);
    }
}
