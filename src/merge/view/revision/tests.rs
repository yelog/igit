use super::*;
use crossterm::event::{KeyEvent, MouseEvent};
use ratatui::{backend::TestBackend, Terminal};

fn fixture() -> RevisionDiff {
    let context = (0..24)
        .map(|i| format!("    // unchanged {i}\n"))
        .collect::<String>();
    let left =
        format!("function sample() {{\n    const 名 = 'left';\n{context}    return 1;\n}}\n");
    let result =
        format!("function sample() {{\n    const 名 = 'left';\n{context}    return 2;\n}}\n");
    let right =
        format!("function sample() {{\n    const 名 = 'right';\n{context}    return 2;\n}}\n");
    RevisionDiff::new(
        "example.js".into(),
        ["11111111".into(), "22222222".into(), "33333333".into()],
        [left, result, right],
        ["LF".into(), "LF".into(), "LF".into()],
    )
}

fn draw(view: &mut RevisionDiff, w: u16, h: u16, icons: IconMode) -> Terminal<TestBackend> {
    let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
    t.draw(|f| view.draw(f, f.area(), icons, 0, 4)).unwrap();
    t
}

fn draw_inline(view: &mut RevisionDiff, w: u16, h: u16, icons: IconMode) -> Terminal<TestBackend> {
    let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
    t.draw(|f| view.draw_inline(f, f.area(), icons)).unwrap();
    t
}

fn text(t: &Terminal<TestBackend>) -> String {
    let b = t.backend().buffer();
    (0..b.area.height)
        .map(|y| {
            let mut line = String::new();
            let mut x = 0;
            while x < b.area.width {
                let symbol = b[(x, y)].symbol();
                line.push_str(symbol);
                x += unicode_width::UnicodeWidthStr::width(symbol).max(1) as u16;
            }
            line
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn key(view: &mut RevisionDiff, code: K) -> RevisionEvent {
    view.event(&Event::Key(KeyEvent::new(code, M::NONE)))
}

fn pane_has_focus(
    terminal: &Terminal<TestBackend>,
    view: &RevisionDiff,
    map: &RowMap<'_>,
    pane: usize,
    index: usize,
) -> bool {
    let section = &map.sections[index];
    let y = view.code_areas[pane].y + (section.first - view.scroll) as u16;
    let width = terminal.backend().buffer().area.width;
    let (start, end) = if pane == 0 {
        (0, width / 2)
    } else {
        (width / 2, width)
    };
    let buffer = terminal.backend().buffer();
    (start..end).any(|x| buffer[(x, y)].bg == crate::ui::SEL)
}

#[test]
fn local_source_line_is_centered_through_alignment_and_folded_context() {
    let before = (1..=180)
        .map(|line| format!("source line {line}\n"))
        .collect::<String>();
    let after = before
        .replacen("source line 3\n", "", 1)
        .replacen("source line 9\n", "inserted one\ninserted two\n", 1)
        .replace("source line 160\n", "changed near end\n");
    for (width, height) in [(160, 36), (80, 24), (44, 12)] {
        let mut view = RevisionDiff::local(
            "source.txt".into(),
            "12345678".into(),
            before.clone(),
            after.clone(),
            String::new(),
            String::new(),
            true,
        );
        view.reveal_local_line(85);
        let screen = text(&draw(&mut view, width, height, IconMode::Unicode));
        assert!(screen.contains("source line 85"), "{screen}");
        assert_eq!(view.side, 1);
        let map = RowMap::revision(&view.chunks, &view.view, view.single_pane);
        let row = map
            .sections
            .iter()
            .find_map(|section| {
                let offset = 84usize.checked_sub(section.numbers[1])?;
                (offset < section.lines[1].len()).then(|| {
                    section.first
                        + (0..section.height)
                            .find(|&row| section.line_at(row) == Some(offset))
                            .unwrap()
                })
            })
            .unwrap();
        assert_eq!(row - view.scroll, view.page / 2, "{width}x{height}");
        let scroll = view.scroll;
        key(&mut view, K::Char('j'));
        draw(&mut view, width, height, IconMode::Unicode);
        assert_eq!(view.scroll, scroll + 1);
    }
}

#[test]
fn local_source_line_clamps_at_boundaries_and_accepts_empty_files() {
    for text in ["", "single line\n", "first\nlast"] {
        for line in [0, 1, usize::MAX] {
            let mut view = RevisionDiff::local(
                "source.txt".into(),
                "12345678".into(),
                text.into(),
                text.into(),
                String::new(),
                String::new(),
                true,
            );
            view.reveal_local_line(line);
            draw(&mut view, 120, 24, IconMode::Ascii);
            assert!(view.reveal_line.is_none());
            assert_eq!(view.side, 1);
            assert_eq!(view.scroll, 0);
        }
    }
}

#[test]
fn local_source_line_centers_a_changed_row_even_when_surrounding_context_is_folded() {
    let before = (1..=200)
        .map(|line| format!("source line {line}\n"))
        .collect::<String>();
    let after = before.replace("source line 100\n", "changed line 100\n");
    for (width, height) in [(160, 36), (80, 24), (44, 12)] {
        let mut view = RevisionDiff::local(
            "source.txt".into(),
            "12345678".into(),
            before.clone(),
            after.clone(),
            String::new(),
            String::new(),
            true,
        );
        view.reveal_local_line(100);
        let terminal = draw(&mut view, width, height, IconMode::Unicode);
        let row = view.code_areas[1].y + (view.page / 2) as u16;
        let center = (0..width)
            .map(|column| terminal.backend().buffer()[(column, row)].symbol())
            .collect::<String>();
        assert!(
            center.contains("changed line 100"),
            "{width}x{height}: {center}"
        );
    }
}

#[test]
fn historical_alignment_highlights_only_changed_sides_and_has_no_write_controls() {
    let mut view = fixture();
    let t = draw(&mut view, 180, 32, IconMode::Unicode);
    let screen = text(&t);
    for expected in [
        "11111111",
        "22222222",
        "33333333",
        "Parent 1",
        "Result",
        "Parent 2",
        "2 changes, No conflicts",
        "1/4 files",
        "unchanged lines",
    ] {
        assert!(screen.contains(expected), "missing {expected}: {screen}");
    }
    assert!(!screen.contains("Apply") && !screen.contains("Accept") && !screen.contains("Ignore"));
    let map = RowMap::new(&view.chunks, &view.view, None);
    assert!(map.total < map.numbers[1]);
    assert_eq!(map.numbers, [28, 28, 28]);
    for pane in 0..3 {
        let source = view
            .chunks
            .iter()
            .map(|c| match pane {
                0 => c.left.as_str(),
                1 => &c.result,
                _ => &c.right,
            })
            .collect::<String>();
        assert_eq!(source, view.texts[pane]);
    }
    let row = map.sections[view.changes[0]].first + 4;
    let buffer = t.backend().buffer();
    assert_eq!(
        buffer[(1, row as u16)].bg,
        EDITOR_BG,
        "unchanged left side stays neutral"
    );
    assert_eq!(buffer[(65, row as u16)].bg, Color::Rgb(31, 40, 50));
    assert!(buffer
        .content
        .iter()
        .any(|c| c.bg == Color::Rgb(43, 66, 93)));
    assert!(buffer
        .content
        .iter()
        .any(|c| c.fg == Color::Rgb(207, 142, 109)));
    assert!(
        !buffer
            .content
            .iter()
            .any(|c| c.bg == Color::Rgb(35, 28, 28)),
        "historical differences are not unresolved conflicts"
    );
}

#[test]
fn revision_navigation_folds_scrollbars_and_narrow_panes_are_read_only() {
    let mut view = fixture();
    let original = view.texts.clone();
    draw(&mut view, 120, 20, IconMode::NerdFont);
    key(&mut view, K::Char('n'));
    assert_eq!(view.selected, *view.changes.last().unwrap());
    key(&mut view, K::Char('p'));
    assert_eq!(view.selected, view.changes[0]);
    let fold = view
        .hits
        .iter()
        .find(|(_, t)| matches!(t, Target::Fold(_)))
        .unwrap()
        .0;
    view.event(&Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: fold.x + 1,
        row: fold.y,
        modifiers: M::NONE,
    }));
    assert_eq!(view.view.expanded.len(), 1);
    key(&mut view, K::Char('f'));
    let wide = draw(&mut view, 180, 22, IconMode::Ascii);
    assert_eq!(view.total, 28);
    assert!(!view.view.tracks.is_empty());
    assert!(text(&wide).contains("[!]"));
    key(&mut view, K::Char('G'));
    assert_eq!(view.scroll, view.total - view.page);
    key(&mut view, K::Tab);
    let small = draw(&mut view, 44, 12, IconMode::Unicode);
    assert!(text(&small).contains("Parent 2"));
    assert!(!text(&small).contains("Parent 1"));
    key(&mut view, K::Char('l'));
    draw(&mut view, 44, 12, IconMode::Unicode);
    key(&mut view, K::Char('a')); // No apply or resolution path.
    key(&mut view, K::Char('i'));
    assert_eq!(view.texts, original);
    assert!(matches!(key(&mut view, K::Char('y')), RevisionEvent::Copy(s) if s == original[2]));
    assert!(matches!(
        key(&mut view, K::Char(']')),
        RevisionEvent::File(1)
    ));
    assert!(matches!(key(&mut view, K::Esc), RevisionEvent::Close));
    for (w, h) in [(80, 24), (24, 7), (10, 4), (1, 1)] {
        draw(&mut view, w, h, IconMode::Ascii);
    }
}

#[test]
fn revision_goto_requires_gg_and_cancels_its_prefix_on_other_keys() {
    let mut view = fixture();
    key(&mut view, K::Char('f'));
    draw(&mut view, 120, 20, IconMode::NerdFont);
    assert!(view.total > view.page + 2);
    let bottom = view.total.saturating_sub(view.page);
    view.scroll = bottom;

    key(&mut view, K::Char('g'));
    assert_eq!(view.scroll, bottom);
    key(&mut view, K::Char('g'));
    assert_eq!(view.scroll, 0);

    key(&mut view, K::Char('G'));
    assert_eq!(view.scroll, bottom);
    key(&mut view, K::Home);
    assert_eq!(view.scroll, 0);

    key(&mut view, K::Char('j'));
    assert_eq!(view.scroll, 1);
    key(&mut view, K::Char('g'));
    assert_eq!(view.scroll, 1);
    key(&mut view, K::Char('j'));
    assert_eq!(view.scroll, 2);
    key(&mut view, K::Char('g'));
    assert_eq!(view.scroll, 2);
    key(&mut view, K::Char('g'));
    assert_eq!(view.scroll, 0);
}

#[test]
fn differing_lengths_deletions_and_unicode_keep_source_line_numbers() {
    let texts = [
        "a\nremoved\n界\nend\n",
        "a\n界\ninserted\nend\n",
        "a\n界\nend\n",
    ]
    .map(str::to_owned);
    let view = RevisionDiff::new(
        "file.ts".into(),
        ["1".into(), "2".into(), "3".into()],
        texts.clone(),
        Default::default(),
    );
    let map = RowMap::new(&view.chunks, &view.view, None);
    assert_eq!(map.numbers, [4, 4, 3]);
    for (pane, text) in texts.iter().enumerate() {
        let actual = map
            .sections
            .iter()
            .flat_map(|s| s.lines[pane].iter().copied())
            .map(rows::line_body)
            .collect::<Vec<_>>();
        assert_eq!(actual, text.lines().collect::<Vec<_>>());
    }
    assert!(map
        .sections
        .iter()
        .any(|s| s.lines.iter().any(Vec::is_empty)));
    let anchors = [0, 1, 2].map(|pane| {
        map.sections.iter().find_map(|section| {
            section.lines[pane]
                .iter()
                .position(|line| *line == "界\n")
                .map(|line| section.first + line)
        })
    });
    assert!(anchors[0].is_some());
    assert_eq!(anchors, [anchors[0]; 3]);
}

#[test]
fn two_way_layout_has_two_revisions_and_never_focuses_the_hidden_alignment_side() {
    let mut view = RevisionDiff::two_way(
        "sample.js".into(),
        ["11111111".into(), "22222222".into()],
        ["const answer = 1;\n".into(), "const answer = 2;\n".into()],
        ["LF".into(), "LF".into()],
    );
    for (w, h) in [(180, 40), (120, 36), (80, 24), (48, 12)] {
        let t = draw(&mut view, w, h, IconMode::Unicode);
        let screen = text(&t);
        assert!(
            screen.contains("Before") && screen.contains("After"),
            "{screen}"
        );
        assert!(!screen.contains("Parent 2") && !screen.contains("No conflicts"));
        assert_eq!(view.code_areas.iter().filter(|r| !r.is_empty()).count(), 2);
        let cols = view.code_areas;
        assert!(cols[0].right() < cols[1].x);
        for _ in 0..4 {
            key(&mut view, K::Tab);
            assert!(view.side < 2);
        }
        let highlights = t
            .backend()
            .buffer()
            .content
            .iter()
            .filter(|c| c.bg == Color::Rgb(43, 66, 93))
            .count();
        assert!(highlights > 0);
    }
    let small = draw(&mut view, 44, 12, IconMode::Ascii);
    assert!(text(&small).contains("After") && !text(&small).contains("Before"));
    key(&mut view, K::Tab);
    let small = draw(&mut view, 44, 12, IconMode::Ascii);
    assert!(text(&small).contains("Before") && !text(&small).contains("After"));
}

#[test]
fn local_merge_treats_adjacent_alignment_chunks_as_one_change_group() {
    let revision = "alpha\nbeta\ngamma\ndelta\n";
    let local = "alpha\nBETA\ninserted\ngamma\ndelta\n";
    let mut view = RevisionDiff::local(
        "sample.ts".into(),
        "11111111".into(),
        revision.into(),
        local.into(),
        "LF".into(),
        "LF".into(),
        true,
    );
    assert_eq!(
        view.change_count(),
        1,
        "{:?}",
        view.chunks
            .iter()
            .map(|chunk| (chunk.kind, chunk.left.as_str(), chunk.result.as_str()))
            .collect::<Vec<_>>()
    );
    let start = view.changes[0];
    assert_ne!(view.chunks[start + 1].kind, Kind::Context);
    assert_eq!(
        view.merged_local_text(Some(start)).as_deref(),
        Some(revision)
    );
    assert_eq!(
        view.merged_local_text(Some(start + 1)).as_deref(),
        Some(revision)
    );

    draw(&mut view, 120, 24, IconMode::Unicode);
    let applies: Vec<_> = view
        .hits
        .iter()
        .filter_map(|(rect, target)| {
            (matches!(target, Target::Apply(_)) && rect.y >= view.code_areas[0].y).then_some(*rect)
        })
        .collect();
    assert_eq!(applies.len(), 1);
    let gutter = applies[0];
    assert!(matches!(
        view.event(&Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: gutter.x,
            row: gutter.y,
            modifiers: M::NONE,
        })),
        RevisionEvent::Apply(index) if index == start
    ));
    view.selected = start + 1;
    assert!(matches!(
        key(&mut view, K::Char('L')),
        RevisionEvent::Apply(index) if index == start
    ));
}

#[test]
fn local_merge_marks_the_current_change_group_in_both_panes() {
    let revision = (1..=32)
        .map(|line| format!("source line {line}\n"))
        .collect::<String>();
    let local = revision
        .replace("source line 5\n", "changed line 5\ninserted after 5\n")
        .replace("source line 24\n", "changed line 24\n");
    let mut view = RevisionDiff::local(
        "sample.txt".into(),
        "11111111".into(),
        revision,
        local,
        "LF".into(),
        "LF".into(),
        true,
    );
    assert_eq!(view.changes.len(), 2);
    let terminal = draw(&mut view, 120, 32, IconMode::Unicode);
    let screen = text(&terminal);
    assert!(screen.contains("Change 1/2"), "{screen}");

    let first = view.changes[0];
    let second = view.changes[1];
    let group: Vec<_> = (first..view.change_end(first))
        .filter(|&index| view.chunks[index].kind != Kind::Context)
        .collect();
    assert!(group.len() > 1);
    {
        let map = RowMap::revision(&view.chunks, &view.view, view.single_pane);
        for pane in 0..2 {
            for &index in &group {
                assert!(
                    pane_has_focus(&terminal, &view, &map, pane, index),
                    "pane {pane}, chunk {index}: {screen}"
                );
            }
            assert!(
                !pane_has_focus(&terminal, &view, &map, pane, second),
                "pane {pane}: {screen}"
            );
        }
    }

    assert!(matches!(key(&mut view, K::Char('n')), RevisionEvent::None));
    assert_eq!(view.selected, second);
    let terminal = draw(&mut view, 120, 32, IconMode::Unicode);
    let screen = text(&terminal);
    assert!(screen.contains("Change 2/2"), "{screen}");
    let map = RowMap::revision(&view.chunks, &view.view, view.single_pane);
    for pane in 0..2 {
        assert!(
            !pane_has_focus(&terminal, &view, &map, pane, first),
            "pane {pane}: {screen}"
        );
        assert!(
            pane_has_focus(&terminal, &view, &map, pane, second),
            "pane {pane}: {screen}"
        );
    }
}

#[test]
fn local_merge_inline_gutter_arrows_align_with_each_hunk_first_row() {
    let revision = "alpha\none old\ncontext one\ncontext two\ncontext three\ntwo old\ntail\n";
    let local = "alpha\none new\ncontext one\ncontext two\ncontext three\ntwo new\ntail\n";
    let mut view = RevisionDiff::local(
        "sample.ts".into(),
        "11111111".into(),
        revision.into(),
        local.into(),
        "LF".into(),
        "LF".into(),
        true,
    );
    let terminal = draw_inline(&mut view, 120, 24, IconMode::Unicode);
    let screen = text(&terminal);
    let map = RowMap::revision(&view.chunks, &view.view, None);
    let code = view.code_areas[0];
    let arrows: Vec<_> = view
        .hits
        .iter()
        .filter_map(|(rect, target)| {
            (rect.y >= code.y && matches!(target, Target::Apply(_))).then_some((*rect, *target))
        })
        .collect();
    assert_eq!(arrows.len(), 2, "{screen}");
    for (rect, target) in arrows {
        let Target::Apply(index) = target else {
            unreachable!();
        };
        let section = &map.sections[index];
        let expected_y = code.y + (section.first - view.scroll) as u16;
        assert_eq!(rect.y, expected_y, "{screen}");
        let first_line = rows::line_body(section.lines[0][0]);
        assert!(
            screen
                .lines()
                .nth(rect.y as usize)
                .unwrap()
                .contains(first_line),
            "arrow for change {index} is not on `{first_line}`:\n{screen}"
        );
    }
}

#[test]
fn local_merge_preserves_equivalent_context_indentation_for_one_change() {
    let revision = "top\nold\n\ttabbed\nend\n";
    let mut view = RevisionDiff::local(
        "sample.ts".into(),
        "11111111".into(),
        revision.into(),
        "top\nnew\n    tabbed\nend\n".into(),
        "LF".into(),
        "LF".into(),
        true,
    );
    assert_eq!(view.change_count(), 1);
    let change = view.selected_change().unwrap();
    assert_eq!(
        view.merged_local_text(Some(change)).as_deref(),
        Some("top\nold\n    tabbed\nend\n")
    );
    assert_eq!(view.merged_local_text(None).as_deref(), Some(revision));
    assert!(matches!(
        key(&mut view, K::Char('L')),
        RevisionEvent::Apply(index) if index == change
    ));
}

#[test]
fn local_merge_disables_apply_all_when_the_revision_deleted_the_file() {
    let mut view = RevisionDiff::local(
        "sample.ts".into(),
        "11111111".into(),
        String::new(),
        "local\n".into(),
        "Absent".into(),
        "LF".into(),
        false,
    );
    draw(&mut view, 120, 24, IconMode::Unicode);
    assert!(!view
        .hits
        .iter()
        .any(|(_, target)| matches!(target, Target::ApplyAll)));
    assert!(matches!(key(&mut view, K::Char('A')), RevisionEvent::None));
    assert_eq!(
        view.merged_local_text(Some(view.changes[0])).as_deref(),
        Some("")
    );
}

#[test]
fn reformatted_mapper_lines_stay_aligned_around_insertions_and_removed_comment_lines() {
    let before = concat!(
        "interface Mapper {\r\n",
        "    List<Row> all();\r\n",
        "    /**\r\n     * Paged query\r\n     *\r\n     * @param query filter\r\n     */\r\n",
        "    Page<Row> list(Page<?> page, Query query);\r\n\r\n",
        "    /**\r\n     * Full query\r\n     *\r\n     * @param query filter\r\n     */\r\n",
        "    List<Row> list(Query query);\r\n\r\n",
        "    Result write(Query old);\r\n",
        "}\r\n",
    );
    let after = concat!(
        "interface Mapper {\r\n",
        "\tList<Row> all();\r\n\r\n",
        "\t/**\r\n\t * Paged query\r\n\t * @param query filter\r\n\t */\r\n",
        "\tPage<Row> list(Page<?> page, Query query);\r\n\r\n",
        "\t/**\r\n\t * Full query\r\n\t * @param query filter\r\n\t */\r\n",
        "\tList<Row> list(Query query);\r\n\r\n",
        "\tList<Row> candidates();\r\n",
        "\tResult write(Query updated);\r\n",
        "}\r\n",
    );
    let mut view = RevisionDiff::two_way(
        "Mapper.java".into(),
        ["Index".into(), "Working tree".into()],
        [before.into(), after.into()],
        Default::default(),
    );
    let terminal = draw_inline(&mut view, 180, 32, IconMode::Unicode);
    let buffer = terminal.backend().buffer();
    let [left, right, _] = view.code_areas;
    let row = |area: Rect, needle: &str| {
        (area.y..area.bottom())
            .find(|&y| {
                (area.x..area.right())
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
                    .contains(needle)
            })
            .unwrap_or_else(|| panic!("missing {needle}"))
    };
    for anchor in [
        "List<Row> all",
        "Paged query",
        "Full query",
        "List<Row> list",
    ] {
        let y = row(left, anchor);
        assert_eq!(row(right, anchor), y, "alignment for {anchor}");
        assert_eq!(buffer[(left.x, y)].bg, EDITOR_BG);
        assert_eq!(buffer[(right.x, y)].bg, EDITOR_BG);
    }
    let blank = row(right, "List<Row> all") + 1;
    assert_eq!(buffer[(left.x + 2, blank)].bg, EDITOR_BG);
    assert_eq!(buffer[(right.x, blank)].bg, Color::Rgb(38, 54, 45));
    let added = row(right, "candidates");
    assert_eq!(buffer[(right.x, added)].bg, Color::Rgb(38, 54, 45));
    assert_eq!(buffer[(left.x, added)].bg, EDITOR_BG);
    let changed = row(left, "Query old");
    assert_eq!(row(right, "Query updated"), changed);
    assert_eq!(buffer[(right.x, changed)].bg, Color::Rgb(31, 40, 50));
    assert_eq!(view.changes.len(), 4);
    let rail = buffer.area.right() - 1;
    for color in [
        Color::Rgb(83, 139, 99),
        Color::Rgb(85, 132, 165),
        Color::Rgb(116, 117, 131),
    ] {
        assert!((right.y..right.bottom()).any(|y| buffer[(rail, y)].fg == color));
    }
    view.event(&Event::Key(KeyEvent::new(K::Char('a'), M::CONTROL)));
    assert_eq!(view.selected_text(), Some(after));
}

#[test]
fn single_revision_view_compacts_padding_for_a_large_addition() {
    let before = "first\nlast\n";
    let added = (0..32)
        .map(|i| format!("added entry {i}\n"))
        .collect::<String>();
    let after = format!("first\n{added}last\n");
    let mut view = RevisionDiff::two_way(
        "example.txt".into(),
        Default::default(),
        [before.into(), after],
        Default::default(),
    );
    let split = draw_inline(&mut view, 120, 12, IconMode::Ascii);
    let track = view
        .view
        .tracks
        .iter()
        .find(|track| matches!(track.axis, Scroll::Vertical) && track.area.x == 119)
        .unwrap();
    let thumb = &split.backend().buffer()[(track.area.x, track.area.y + track.thumb as u16)];
    assert_eq!(thumb.symbol(), "|");
    assert_eq!(thumb.bg, Color::Rgb(38, 54, 45));
    key(&mut view, K::Tab);
    for width in [120, 44] {
        key(&mut view, K::Char('z'));
        let terminal = draw_inline(&mut view, width, 12, IconMode::Ascii);
        assert_eq!(view.total, 3);
        assert!(text(&terminal).contains("last"));
    }
    assert!(matches!(key(&mut view, K::Char('y')), RevisionEvent::Copy(s) if s == before));
}

#[test]
fn xml_preview_marks_equivalent_indent_but_shows_changes_to_text_and_line_endings() {
    let before = "    <select id=\"搜索\">\r\n    SELECT 'a b'\r\n    </select>\r\n";
    let after = concat!(
        "\t<select id=\"搜索\">\r\n",
        "    SELECT 'a  b'\r\n",
        "    <if test=\"active\">AND active = 1</if><!-- filter -->\r\n",
        "  </select>\n",
    );
    let mut view = RevisionDiff::two_way(
        "Mapper.xml".into(),
        ["Index".into(), "Working tree".into()],
        [before.into(), after.into()],
        Default::default(),
    );
    let terminal = draw_inline(&mut view, 180, 12, IconMode::Unicode);
    let buffer = terminal.backend().buffer();
    let code = view.code_areas[1];
    assert_eq!(buffer[(code.x, code.y)].bg, EDITOR_BG);
    assert_eq!(buffer[(code.x - 1, code.y)].symbol(), "·");
    assert_eq!(
        buffer[(code.right() - 1, code.y + 1)].bg,
        Color::Rgb(31, 40, 50)
    );
    assert_eq!(
        buffer[(code.right() - 1, code.y + 2)].bg,
        Color::Rgb(38, 54, 45)
    );
    assert_eq!(
        buffer[(code.right() - 1, code.y + 3)].bg,
        Color::Rgb(31, 40, 50)
    );
    let added = after.lines().nth(2).unwrap();
    for (needle, color) in [
        ("<if", Color::Rgb(207, 142, 109)),
        ("test=", Color::Rgb(199, 125, 187)),
        ("\"active\"", Color::Rgb(106, 171, 115)),
        ("<!--", Color::Rgb(104, 125, 101)),
    ] {
        let start = rows::cell_width(&added[..added.find(needle).unwrap()]);
        assert_eq!(buffer[(code.x + start as u16, code.y + 2)].fg, color);
    }
    assert!(matches!(key(&mut view, K::Char('y')), RevisionEvent::Copy(s) if s == after));
}

#[test]
fn mixed_change_colors_survive_revision_layouts_and_selection_keeps_original_bytes() {
    let before = "\tconst 名 = old; tail\r\n";
    let after = "\tconst 名 = new; extra tail\r\nconst added = 1;\r\n";
    for three_way in [false, true] {
        let mut view = if three_way {
            RevisionDiff::new(
                "example.js".into(),
                ["parent 1".into(), "result".into(), "parent 2".into()],
                [before.into(), after.into(), before.into()],
                Default::default(),
            )
        } else {
            RevisionDiff::two_way(
                "example.js".into(),
                ["before".into(), "after".into()],
                [before.into(), after.into()],
                Default::default(),
            )
        };
        for (width, height) in [(180, 32), (120, 24), (80, 16), (44, 12)] {
            for icons in [IconMode::NerdFont, IconMode::Unicode, IconMode::Ascii] {
                let terminal = draw_inline(&mut view, width, height, icons);
                let code = view.code_areas[1];
                assert!(!code.is_empty());
                let buffer = terminal.backend().buffer();
                assert_eq!(buffer[(code.x, code.y)].bg, Color::Rgb(31, 40, 50));
                assert_eq!(buffer[(code.x, code.y + 1)].bg, Color::Rgb(38, 54, 45));
                for (needle, background) in [
                    ("new", Color::Rgb(43, 66, 93)),
                    ("extra", Color::Rgb(57, 92, 66)),
                ] {
                    let line = after.lines().next().unwrap();
                    let start = rows::cell_width(&line[..line.find(needle).unwrap()]);
                    assert_eq!(buffer[(code.x + start as u16, code.y)].bg, background);
                }
                view.event(&Event::Key(KeyEvent::new(K::Char('a'), M::CONTROL)));
                assert_eq!(view.selected_text(), Some(after));
                let selected = draw_inline(&mut view, width, height, icons);
                assert_eq!(
                    selected.backend().buffer()[(code.x, code.y + 1)].bg,
                    crate::ui::idea::SELECT
                );
                view.clear_selection();
                assert!(
                    matches!(key(&mut view, K::Char('y')), RevisionEvent::Copy(s) if s == after)
                );
            }
        }
        key(&mut view, K::Char('z'));
        let terminal = draw_inline(&mut view, 120, 24, IconMode::Unicode);
        assert_eq!(
            view.code_areas
                .iter()
                .filter(|area| !area.is_empty())
                .count(),
            1
        );
        let code = view.code_areas[1];
        assert_eq!(
            terminal.backend().buffer()[(code.x, code.y + 1)].bg,
            Color::Rgb(38, 54, 45)
        );
    }
}

fn drag_text(view: &mut RevisionDiff, from: (u16, u16), to: (u16, u16)) -> String {
    let mut result = RevisionEvent::None;
    for (kind, (column, row)) in [
        (MouseEventKind::Down(MouseButton::Left), from),
        (MouseEventKind::Drag(MouseButton::Left), to),
        (MouseEventKind::Up(MouseButton::Left), to),
    ] {
        result = view.event(&Event::Mouse(MouseEvent {
            kind,
            column,
            row,
            modifiers: M::NONE,
        }));
    }
    match result {
        RevisionEvent::Copy(s) => s,
        _ => panic!("expected copied source text"),
    }
}

#[test]
fn split_selection_preserves_tabs_unicode_crlf_and_hidden_context() {
    let context = (0..24)
        .map(|i| format!("// context {i}\r\n"))
        .collect::<String>();
    let before = format!("\tconst 名 = 'old';\r\n{context}\treturn 1;\r\n");
    let after = format!("\tconst 名 = 'new';\r\n{context}\treturn 2;\r\n");
    let mut view = RevisionDiff::two_way(
        "sample.js".into(),
        ["old".into(), "new".into()],
        [before, after.clone()],
        Default::default(),
    );
    draw(&mut view, 140, 32, IconMode::Unicode);
    let code = view.code_areas[1];
    let width = rows::cell_width("\tconst 名 = 'new';") as u16;
    assert_eq!(
        drag_text(&mut view, (code.x, code.y), (code.x + width, code.y)),
        "\tconst 名 = 'new';"
    );
    assert!(view.selected_text().is_none());
    let end = code.y + view.total as u16 - 1;
    assert_eq!(
        drag_text(
            &mut view,
            (code.x, code.y),
            (code.x + rows::cell_width("\treturn 2;") as u16, end)
        ),
        after.trim_end_matches(['\r', '\n'])
    );
    view.event(&Event::Key(KeyEvent::new(K::Char('a'), M::CONTROL)));
    assert_eq!(view.selected_text(), Some(after.as_str()));
    let small = draw(&mut view, 44, 12, IconMode::Unicode);
    assert!(text(&small).contains("After"));
    assert_eq!(view.selected_text(), Some(after.as_str()));
    assert!(
        matches!(view.event(&Event::Key(KeyEvent::new(K::Char('c'),M::CONTROL))), RevisionEvent::Copy(s) if s == after)
    );
    assert!(matches!(key(&mut view, K::Esc), RevisionEvent::None));
    assert!(view.selected_text().is_none());
    assert!(matches!(key(&mut view, K::Esc), RevisionEvent::Close));
}

#[test]
fn selection_excludes_alignment_padding() {
    let mut view = RevisionDiff::two_way(
        "sample.js".into(),
        ["old".into(), "new".into()],
        ["keep\nend\n".into(), "keep\ninserted\nmore\nend\n".into()],
        Default::default(),
    );
    draw(&mut view, 120, 24, IconMode::Ascii);
    let code = view.code_areas[0];
    assert_eq!(
        drag_text(&mut view, (code.x, code.y), (code.x + 3, code.y + 3)),
        "keep\nend"
    );
}

#[test]
fn inline_merge_has_one_toolbar_and_revision_heading_without_modal_controls() {
    let mut view = fixture();
    let terminal = draw_inline(&mut view, 180, 24, IconMode::Unicode);
    let screen = text(&terminal);
    let lines = screen.lines().collect::<Vec<_>>();
    for label in ["↑", "↓", "Unfold", "Zoom", "2 changes, No conflicts"] {
        assert!(lines[0].contains(label), "missing {label}: {screen}");
    }
    for label in [
        "11111111",
        "22222222",
        "33333333",
        "Parent 1",
        "Result",
        "Parent 2",
        "example.js",
    ] {
        assert!(lines[1].contains(label), "missing {label}: {screen}");
    }
    assert!(lines[2].contains("function sample()"), "{screen}");
    for label in ["Repository Diff", "Close", "files", "Tab Pane", "Apply"] {
        assert!(!screen.contains(label), "unexpected {label}: {screen}");
    }
    assert!(view
        .hits
        .iter()
        .all(|(_, target)| !matches!(target, Target::Close | Target::File(_))));
    assert!(view
        .code_areas
        .iter()
        .all(|area| area.y == 2 && area.height == 21));
    assert_eq!(view.page, 21);
    assert!(view.hits.iter().any(|(_, t)| matches!(t, Target::Fold(_))));
    assert!(terminal
        .backend()
        .buffer()
        .content
        .iter()
        .any(|c| c.fg == Color::Rgb(207, 142, 109)));

    view.name = "src/a/long/directory/path/example名.js".into();
    let narrower = text(&draw_inline(&mut view, 120, 24, IconMode::Unicode));
    let heading = narrower.lines().nth(1).unwrap();
    assert!(heading.contains("example名.js"), "{heading}");
    assert!(!heading.contains("src/a/long"));
}

#[test]
fn inline_two_way_and_merge_split_at_the_same_widths_as_the_overlay() {
    let mut before_after = RevisionDiff::two_way(
        "example.js".into(),
        ["1111111111111111".into(), "2222222222222222".into()],
        ["const answer = 1;\n".into(), "const answer = 2;\n".into()],
        Default::default(),
    );
    let mut merge = fixture();
    for (view, threshold, count) in [(&mut before_after, 48, 2), (&mut merge, 72, 3)] {
        for width in [threshold, 100, 180] {
            let terminal = draw_inline(view, width, 12, IconMode::Ascii);
            assert_eq!(
                view.code_areas.iter().filter(|r| !r.is_empty()).count(),
                count
            );
            assert!(text(&terminal).contains("11111111"));
            assert!(text(&terminal).contains("22222222"));
        }
        draw_inline(view, threshold - 1, 12, IconMode::Ascii);
        assert_eq!(view.code_areas.iter().filter(|r| !r.is_empty()).count(), 1);
        assert!(!view.code_areas[1].is_empty());
        key(view, K::Tab);
        draw_inline(view, threshold - 1, 12, IconMode::Ascii);
        assert!(!view.code_areas[2 % count].is_empty());
        key(view, K::BackTab);
        draw_inline(view, 180, 12, IconMode::Ascii);
        let zoom = view
            .hits
            .iter()
            .find(|(_, target)| matches!(target, Target::Zoom))
            .unwrap()
            .0;
        view.event(&Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: zoom.x,
            row: zoom.y,
            modifiers: M::NONE,
        }));
        let zoomed = draw_inline(view, 180, 12, IconMode::Ascii);
        assert!(text(&zoomed).lines().next().unwrap().contains("Split"));
        assert_eq!(view.code_areas.iter().filter(|r| !r.is_empty()).count(), 1);
        assert!(!view.code_areas[1].is_empty());
    }
    let screen = text(&draw_inline(&mut before_after, 180, 12, IconMode::Ascii));
    assert!(screen.contains("After") && screen.contains("1 difference"));
    assert!(!screen.contains("Parent") && !screen.contains("No conflicts"));
    assert!(!screen.contains("2222222222222222"));
}

#[test]
fn inline_scrolling_and_selection_share_the_visible_code_geometry() {
    let mut view = fixture();
    draw_inline(&mut view, 180, 12, IconMode::Unicode);
    let code = view.code_areas[1];
    assert_eq!(
        drag_text(
            &mut view,
            (code.x, code.y),
            (code.x + "function sample() {".width() as u16, code.y)
        ),
        "function sample() {"
    );
    key(&mut view, K::Char('f'));
    draw_inline(&mut view, 180, 12, IconMode::Unicode);
    assert_eq!(view.total, 28);
    assert_eq!(view.page, 9);
    key(&mut view, K::Char('j'));
    let scrolled = draw_inline(&mut view, 180, 12, IconMode::Unicode);
    assert_eq!(view.scroll, 1);
    assert_eq!(
        view.view
            .tracks
            .iter()
            .filter(|track| matches!(track.axis, Scroll::Vertical))
            .count(),
        3
    );
    assert!(view
        .view
        .tracks
        .iter()
        .filter(|track| matches!(track.axis, Scroll::Vertical))
        .all(|track| track.position == 1 && track.area.y == 2 && track.area.height == 9));
    let top = text(&scrolled).lines().nth(2).unwrap().to_owned();
    assert_eq!(top.matches("const 名").count(), 3, "{top}");
    key(&mut view, K::Char('G'));
    assert_eq!(view.scroll, view.total - view.page);
    assert!(matches!(key(&mut view, K::Esc), RevisionEvent::Close));
}

#[test]
fn inline_rendering_stays_inside_its_area_even_when_tiny() {
    let mut view = fixture();
    for (w, h) in [
        (180, 24),
        (72, 12),
        (48, 8),
        (16, 4),
        (10, 3),
        (1, 1),
        (0, 0),
    ] {
        let area = rect(5, 3, w, h);
        let mut terminal = Terminal::new(TestBackend::new(190, 30)).unwrap();
        terminal
            .draw(|f| {
                f.render_widget(Block::default().bg(Color::Magenta), f.area());
                view.draw_inline(f, area, IconMode::Ascii);
            })
            .unwrap();
        let buffer = terminal.backend().buffer();
        for y in 0..buffer.area.height {
            for x in 0..buffer.area.width {
                if !inside(area, x, y) {
                    assert_eq!(buffer[(x, y)].bg, Color::Magenta, "{w}x{h} at {x},{y}");
                }
            }
        }
        assert!(view.hits.iter().all(|(hit, _)| {
            hit.x >= area.x
                && hit.y >= area.y
                && hit.right() <= area.right()
                && hit.bottom() <= area.bottom()
        }));
        assert!(!text(&terminal).contains("Close"));
    }
}

fn search_key(view: &mut RevisionDiff, code: K) -> bool {
    view.search_event(&Event::Key(KeyEvent::new(code, M::NONE)))
}

fn find(view: &mut RevisionDiff, query: &str) {
    assert!(search_key(view, K::Char('/')));
    assert!(view.search_prompt().editing());
    assert!(view.search_event(&Event::Paste(query.into())));
    assert!(search_key(view, K::Enter));
    assert!(!view.search_prompt().editing());
}

fn highlighted(cell: &ratatui::buffer::Cell, current: bool) -> bool {
    if std::env::var_os("NO_COLOR").is_some_and(|value| !value.is_empty()) {
        cell.modifier.contains(if current {
            ratatui::style::Modifier::REVERSED
        } else {
            ratatui::style::Modifier::UNDERLINED
        })
    } else {
        let style = if current {
            crate::ui::search::CURRENT
        } else {
            crate::ui::search::MATCH
        };
        Some(cell.bg) == style.bg && Some(cell.fg) == style.fg
    }
}

#[test]
fn revision_pane_search_wraps_repeated_unicode_matches_and_pans_after_resize() {
    let line = format!("\tconst 名 = '{} É[.] É[.]';", "界".repeat(80));
    let after = format!("after\r\n{line}\r\ntail É[.]\r\n");
    let mut view = RevisionDiff::two_way(
        "sample.js".into(),
        ["old".into(), "new".into()],
        ["before\r\nno match\r\n".into(), after.clone()],
        Default::default(),
    );
    draw_inline(&mut view, 180, 12, IconMode::Unicode);
    find(&mut view, "é[.]");
    let first = view.search_hit.clone().unwrap();
    assert_eq!(first.row, (1, 1));
    assert_eq!(&line[first.bytes.clone()], "É[.]");
    assert_eq!(first.bytes.start, line.find("É[.]").unwrap());
    let start = rows::cell_width(&line[..first.bytes.start]);
    let end = rows::cell_width(&line[..first.bytes.end]);
    for (width, height) in [(180, 12), (44, 8), (16, 4), (120, 24)] {
        let terminal = draw_inline(&mut view, width, height, IconMode::Unicode);
        let code = view.code_areas[view.side()];
        let left = view.view.hscroll[view.side()];
        assert!(left > 0 && left <= start, "{width}: {left} > {start}");
        assert!(
            end <= left + usize::from(code.width),
            "{width}: {left}..{end}"
        );
        assert!(text(&terminal).contains("É[.]"));
        let x = code.x + (start - left) as u16;
        let map = RowMap::new(&view.chunks, &view.view, None);
        let source_row = map
            .sections
            .iter()
            .find_map(|section| {
                (0..section.height)
                    .find(|&row| {
                        section.line_at(row).is_some_and(|line| {
                            section.numbers[1] + line == first.row.1
                                && line < section.lines[1].len()
                        })
                    })
                    .map(|row| section.first + row)
            })
            .unwrap();
        assert!(highlighted(
            &terminal.backend().buffer()[(x, code.y + (source_row - view.scroll) as u16)],
            true
        ));
    }

    let first_left = view.view.hscroll[1];
    assert!(search_key(&mut view, K::Char('n')));
    let second = view.search_hit.clone().unwrap();
    assert_eq!(second.row, first.row);
    assert_eq!(second.bytes.start, line.rfind("É[.]").unwrap());
    draw_inline(&mut view, 120, 24, IconMode::Unicode);
    assert_eq!(
        view.view.hscroll[1], first_left,
        "nearby repeated matches stay together in the viewport"
    );
    assert!(search_key(&mut view, K::Char('n')));
    assert_eq!(view.search_hit.as_ref().unwrap().row, (1, 2));
    assert!(search_key(&mut view, K::Char('n')));
    assert!(view.search_hit.as_ref() == Some(&first));
    assert!(search_key(&mut view, K::Char('N')));
    assert_eq!(view.search_hit.as_ref().unwrap().row, (1, 2));
    assert!(search_key(&mut view, K::Char('N')));
    assert!(view.search_hit.as_ref() == Some(&second));
    draw(&mut view, 120, 12, IconMode::Ascii);
    assert!(!search_key(&mut view, K::Char('j')));
    assert!(!search_key(&mut view, K::Char('y')));
    assert!(matches!(key(&mut view, K::Char('y')), RevisionEvent::Copy(s) if s == after));
}

#[test]
fn revision_pane_search_reveals_only_the_matching_context_with_alignment_padding() {
    let first_context = (0..48)
        .map(|i| {
            if i == 31 {
                "\t// hidden needle 名\r\n".into()
            } else {
                format!("// first context {i}\r\n")
            }
        })
        .collect::<String>();
    let second_context = (0..48)
        .map(|i| format!("// second context {i}\r\n"))
        .collect::<String>();
    let before = format!("old start\r\n{first_context}old middle\r\n{second_context}old end\r\n");
    let after = format!(
        "new start\r\ninserted\r\n{first_context}new middle\r\n{second_context}new end\r\n"
    );
    let mut view = RevisionDiff::two_way(
        "sample.js".into(),
        ["old".into(), "new".into()],
        [before, after.clone()],
        Default::default(),
    );
    draw_inline(&mut view, 100, 12, IconMode::Unicode);
    let source_line = after
        .lines()
        .position(|line| line.contains("needle"))
        .unwrap();
    let map = RowMap::new(&view.chunks, &view.view, None);
    let context = map
        .sections
        .iter()
        .enumerate()
        .filter(|(_, s)| (0..s.height).any(|row| s.line_at(row).is_none()))
        .map(|(i, _)| i)
        .collect::<Vec<_>>();
    assert_eq!(context.len(), 2);
    let matching = map
        .sections
        .iter()
        .position(|s| (s.numbers[1]..s.numbers[1] + s.lines[1].len()).contains(&source_line))
        .unwrap();
    assert!(view.view.expanded.is_empty());

    find(&mut view, "needle");
    let terminal = draw_inline(&mut view, 80, 10, IconMode::Ascii);
    assert!(text(&terminal).contains("hidden needle 名"));
    assert_eq!(view.view.expanded, BTreeSet::from([matching]));
    assert_eq!(view.selected, matching);
    let map = RowMap::new(&view.chunks, &view.view, None);
    let section = &map.sections[matching];
    let row = (0..section.height)
        .find(|&row| section.line_at(row) == Some(source_line - section.numbers[1]))
        .unwrap()
        + section.first;
    assert!((view.scroll..view.scroll + view.page).contains(&row));
    assert_eq!(view.search_hit.as_ref().unwrap().row, (1, source_line));
    assert!(context
        .iter()
        .filter(|&&i| i != matching)
        .all(|i| !view.view.expanded.contains(i)));
    view.event(&Event::Key(KeyEvent::new(K::Char('a'), M::CONTROL)));
    assert_eq!(view.selected_text(), Some(after.as_str()));
    assert!(matches!(
        view.event(&Event::Key(KeyEvent::new(K::Char('c'), M::CONTROL))),
        RevisionEvent::Copy(s) if s == after
    ));
}

#[test]
fn revision_pane_search_switches_scope_with_tab_backtab_and_mouse() {
    let mut view = RevisionDiff::two_way(
        "sample.js".into(),
        ["old".into(), "new".into()],
        ["名 before 名\r\n".into(), "名 after 名 名\r\n".into()],
        Default::default(),
    );
    draw_inline(&mut view, 120, 12, IconMode::Unicode);
    find(&mut view, "名");
    assert_eq!(view.search_hit.as_ref().unwrap().row.0, 1);
    key(&mut view, K::BackTab); // The embedding maps '[' to this operation.
    assert_eq!(view.side(), 0);
    assert!(view.search_hit.is_none());
    assert!(view.search_prompt().visible());
    let terminal = draw_inline(&mut view, 120, 12, IconMode::Unicode);
    let before = view.code_areas[0];
    let after = view.code_areas[1];
    assert!(highlighted(
        &terminal.backend().buffer()[(before.x, before.y)],
        false
    ));
    assert!(!highlighted(
        &terminal.backend().buffer()[(after.x, after.y)],
        false
    ));
    assert!(search_key(&mut view, K::Char('n')));
    assert_eq!(view.search_hit.as_ref().unwrap().row, (0, 0));
    assert_eq!(view.search_hit.as_ref().unwrap().bytes, 0.."名".len());
    assert!(search_key(&mut view, K::Char('n')));
    assert_eq!(
        view.search_hit.as_ref().unwrap().bytes.start,
        view.texts[0].rfind('名').unwrap()
    );
    key(&mut view, K::Tab); // The embedding maps ']' to this operation.
    assert_eq!(view.side(), 1);
    assert!(view.search_hit.is_none());
    draw_inline(&mut view, 44, 12, IconMode::Unicode);
    assert!(search_key(&mut view, K::Char('N')));
    assert_eq!(view.search_hit.as_ref().unwrap().row, (1, 0));
    assert_eq!(
        view.search_hit.as_ref().unwrap().bytes.start,
        view.texts[1].rfind('名').unwrap()
    );
    draw_inline(&mut view, 120, 12, IconMode::Unicode);
    let heading = view
        .hits
        .iter()
        .find(|(_, target)| matches!(target, Target::Pane(0)))
        .unwrap()
        .0;
    let event = Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: heading.x,
        row: heading.y,
        modifiers: M::NONE,
    });
    assert!(!view.search_event(&event));
    view.event(&event);
    assert_eq!(view.side(), 0);
    assert!(view.search_hit.is_none());
    assert!(matches!(
        key(&mut view, K::Char('[')),
        RevisionEvent::File(-1)
    ));
}

#[test]
fn revision_pane_search_overlays_keep_syntax_selection_and_source_copying() {
    let after = "\tconst 名 = 'É[.] É[.]';\r\n// original bytes\r\n";
    let mut view = RevisionDiff::two_way(
        "sample.js".into(),
        ["old".into(), "new".into()],
        [
            "\tconst 名 = 'old';\r\n// original bytes\r\n".into(),
            after.into(),
        ],
        Default::default(),
    );
    let baseline = draw_inline(&mut view, 180, 12, IconMode::Unicode);
    assert!(search_key(&mut view, K::Char('/')));
    view.search_event(&Event::Paste("é[.]".into()));
    let draft = draw_inline(&mut view, 180, 12, IconMode::Unicode);
    assert_eq!(text(&draft), text(&baseline));
    let code = view.code_areas[1];
    let line = after.lines().next().unwrap();
    let matches = line
        .match_indices("É[.]")
        .map(|(byte, matched)| {
            rows::cell_width(&line[..byte])..rows::cell_width(&line[..byte + matched.len()])
        })
        .collect::<Vec<_>>();
    for x in 0..code.width {
        let cell = &draft.backend().buffer()[(code.x + x, code.y)];
        if matches.iter().any(|range| range.contains(&usize::from(x))) {
            assert!(highlighted(cell, false));
        } else {
            assert_eq!(cell, &baseline.backend().buffer()[(code.x + x, code.y)]);
        }
    }
    assert!(search_key(&mut view, K::Enter));
    let matched = draw_inline(&mut view, 180, 12, IconMode::Unicode);
    assert!(highlighted(
        &matched.backend().buffer()[(code.x + matches[0].start as u16, code.y)],
        true
    ));
    assert!(highlighted(
        &matched.backend().buffer()[(code.x + matches[1].start as u16, code.y)],
        false
    ));
    view.event(&Event::Key(KeyEvent::new(K::Char('a'), M::CONTROL)));
    assert!(search_key(&mut view, K::Char('n')));
    let selected = draw_inline(&mut view, 180, 12, IconMode::Unicode);
    assert_eq!(view.selected_text(), Some(after));
    assert_eq!(
        selected.backend().buffer()[(code.x + matches[1].start as u16, code.y)].bg,
        crate::ui::idea::SELECT
    );
    view.clear_selection();
    assert!(view.selected_text().is_none());
    assert!(view.search_prompt().visible());
    draw_inline(&mut view, 180, 12, IconMode::Unicode);
    assert_eq!(
        drag_text(
            &mut view,
            (code.x, code.y),
            (code.x + rows::cell_width(line) as u16, code.y)
        ),
        line
    );
    assert!(matches!(key(&mut view, K::Char('y')), RevisionEvent::Copy(s) if s == after));
}

#[test]
fn revision_pane_search_pans_complete_combining_graphemes_with_four_cell_tabs() {
    let line = format!("{}\tconst 名 = 'e\u{301} e\u{301}';", "界".repeat(41));
    let mut view = RevisionDiff::two_way(
        "sample.js".into(),
        ["old".into(), "new".into()],
        ["old\n".into(), format!("{line}\r\n")],
        Default::default(),
    );
    draw_inline(&mut view, 120, 12, IconMode::Unicode);
    find(&mut view, "\u{301}");
    for (width, height) in [(44, 12), (16, 4), (80, 24), (180, 32)] {
        let terminal = draw_inline(&mut view, width, height, IconMode::Unicode);
        let code = view.code_areas[1];
        let left = view.view.hscroll[1];
        let start = rows::cell_width(&line[..line.find("e\u{301}").unwrap()]);
        assert!(start >= left && start < left + usize::from(code.width));
        let cell = &terminal.backend().buffer()[(code.x + (start - left) as u16, code.y)];
        assert_eq!(cell.symbol(), "e\u{301}");
        assert!(highlighted(cell, true));
    }
    assert!(search_key(&mut view, K::Char('n')));
    assert_eq!(
        view.search_hit.as_ref().unwrap().bytes.start,
        line.rfind('\u{301}').unwrap()
    );
    draw_inline(&mut view, 10, 3, IconMode::Ascii);
    draw_inline(&mut view, 44, 12, IconMode::Ascii);
    assert!(matches!(
        key(&mut view, K::Char('y')),
        RevisionEvent::Copy(s) if s == format!("{line}\r\n")
    ));
}

#[test]
fn revision_embedding_helpers_track_selection_and_scroll_drags() {
    let mut view = fixture();
    key(&mut view, K::Char('f'));
    draw_inline(&mut view, 120, 12, IconMode::Unicode);
    assert!(!view.mouse_active());
    let code = view.code_areas[1];
    let mouse = |kind, column, row| {
        Event::Mouse(MouseEvent {
            kind,
            column,
            row,
            modifiers: M::NONE,
        })
    };
    view.event(&mouse(
        MouseEventKind::Down(MouseButton::Left),
        code.x,
        code.y,
    ));
    assert!(view.mouse_active());
    view.event(&mouse(
        MouseEventKind::Drag(MouseButton::Left),
        code.x + 5,
        code.y,
    ));
    assert_eq!(view.selected_text(), Some("funct"));
    assert!(view.mouse_active());
    view.clear_selection();
    assert!(!view.mouse_active());
    assert!(view.selected_text().is_none());

    let track = *view
        .view
        .tracks
        .iter()
        .find(|track| matches!(track.axis, Scroll::Vertical))
        .unwrap();
    for cancel in [false, true] {
        view.event(&mouse(
            MouseEventKind::Down(MouseButton::Left),
            track.area.x,
            track.area.y,
        ));
        assert!(
            view.mouse_active(),
            "scrollbar capture must stay in the viewer"
        );
        view.event(&mouse(
            MouseEventKind::Drag(MouseButton::Left),
            track.area.x,
            track.area.bottom() + 2,
        ));
        assert!(view.scroll > 0);
        assert!(view.mouse_active());
        if cancel {
            view.clear_selection();
        } else {
            view.event(&mouse(
                MouseEventKind::Up(MouseButton::Left),
                track.area.x,
                track.area.bottom() + 2,
            ));
        }
        assert!(!view.mouse_active());
    }
    view.event(&mouse(
        MouseEventKind::Down(MouseButton::Left),
        track.area.x,
        track.area.y,
    ));
    assert!(view.mouse_active());
    view.event(&Event::FocusLost);
    assert!(!view.mouse_active());
}

#[test]
fn revision_search_events_are_opt_in_and_escape_cancels_then_clears_search() {
    let mut view = fixture();
    draw_inline(&mut view, 180, 24, IconMode::Ascii);
    assert!(!search_key(&mut view, K::Char('n')));
    assert!(!search_key(&mut view, K::Esc));
    key(&mut view, K::Char('/'));
    assert!(!view.search_prompt().visible());
    key(&mut view, K::Char('n'));
    assert_eq!(view.selected, *view.changes.last().unwrap());
    find(&mut view, "unchanged");
    let hit = view.search_hit.clone();
    assert!(search_key(&mut view, K::Char('/')));
    assert!(view.search_event(&Event::Paste("not present".into())));
    assert!(search_key(&mut view, K::Esc));
    assert!(!view.search_prompt().editing());
    assert!(view.search_prompt().visible());
    assert!(view.search_hit == hit);
    assert!(search_key(&mut view, K::Esc));
    assert!(!view.search_prompt().visible());
    assert!(view.search_hit.is_none());
    assert!(!search_key(&mut view, K::Esc));
    assert!(matches!(key(&mut view, K::Esc), RevisionEvent::Close));
    let released = Event::Key(KeyEvent::new_with_kind(
        K::Char('/'),
        M::NONE,
        crossterm::event::KeyEventKind::Release,
    ));
    assert!(!view.search_event(&released));
}

#[test]
fn revision_replacement_inherits_session_search_without_old_source_or_view_state() {
    let mut previous = fixture();
    find(&mut previous, "unchanged 12");
    draw_inline(&mut previous, 180, 12, IconMode::Unicode);
    find(&mut previous, "unchanged");
    assert!(search_key(&mut previous, K::Char('n')));
    key(&mut previous, K::Char('z'));
    previous.view.hscroll[1] = 4;
    previous.event(&Event::Key(KeyEvent::new(K::Char('a'), M::CONTROL)));
    assert!(previous.search_hit.is_some());
    assert!(!previous.view.expanded.is_empty());
    assert!(previous.selected_text().is_some());

    let before = "\tunchanged Before 名\r\n";
    let after = "\tunchanged After 名\r\nunchanged again\r\n";
    let mut replacement = RevisionDiff::two_way(
        "replacement.js".into(),
        ["before".into(), "after".into()],
        [before.into(), after.into()],
        Default::default(),
    );
    let mut patch: Search<usize> = Search::default();
    patch.inherit_query(previous.search_prompt());
    replacement.inherit_query(patch.prompt());
    assert!(replacement.search_prompt().visible());
    assert!(replacement.search_hit.is_none());
    assert!(!replacement.reveal_search);
    assert!(!replacement.zoom);
    assert!(replacement.view.expanded.is_empty());
    assert_eq!(replacement.view.hscroll, [0; 3]);
    assert_eq!(replacement.scroll, 0);
    assert!(replacement.selected_text().is_none());
    assert!(!replacement.mouse_active());

    // Focus and copy can change twice before the replacement's first frame.
    for (side, source) in [(1, after), (0, before)] {
        assert_eq!(replacement.side(), side);
        replacement.event(&Event::Key(KeyEvent::new(K::Char('a'), M::CONTROL)));
        assert_eq!(replacement.selected_text(), Some(source));
        assert!(matches!(
            replacement.event(&Event::Key(KeyEvent::new(K::Char('c'), M::CONTROL))),
            RevisionEvent::Copy(text) if text == source
        ));
        key(&mut replacement, K::BackTab);
        assert!(replacement.selected_text().is_none());
    }

    assert!(search_key(&mut replacement, K::Char('n')));
    assert_eq!(replacement.search_hit.as_ref().unwrap().row, (1, 0));
    let terminal = draw_inline(&mut replacement, 100, 8, IconMode::Unicode);
    let code = replacement.code_areas[1];
    assert!(highlighted(
        &terminal.backend().buffer()[(code.x + 4, code.y)],
        true
    ));
    assert_eq!(replacement.texts[1], after);
    assert!(matches!(
        key(&mut replacement, K::Char('y')),
        RevisionEvent::Copy(text) if text == after
    ));

    assert!(search_key(&mut previous, K::Char('/')));
    previous.search_event(&Event::Paste("After".into()));
    replacement.inherit_search(&previous);
    assert!(replacement.search_prompt().editing());
    assert!(search_key(&mut replacement, K::Enter));
    assert_eq!(replacement.search_hit.as_ref().unwrap().row, (1, 0));
    let hit = replacement.search_hit.as_ref().unwrap();
    assert_eq!(&after[hit.bytes.clone()], "After");
}
