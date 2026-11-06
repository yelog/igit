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

fn text(t: &Terminal<TestBackend>) -> String {
    let b = t.backend().buffer();
    (0..b.area.height)
        .map(|y| {
            (0..b.area.width)
                .map(|x| b[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn key(view: &mut RevisionDiff, code: K) -> RevisionEvent {
    view.event(&Event::Key(KeyEvent::new(code, M::NONE)))
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
            .collect::<Vec<_>>();
        assert_eq!(actual, text.lines().collect::<Vec<_>>());
    }
    assert!(map
        .sections
        .iter()
        .any(|s| s.lines.iter().any(Vec::is_empty)));
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
