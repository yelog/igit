use super::*;
use unicode_width::UnicodeWidthStr;

fn press(p: &mut CommitPanel, c: &Context, code: K, modifiers: Mod) -> Action {
    p.event(&Event::Key(KeyEvent::new(code, modifiers)), c)
}
fn copy(action: Action) -> String {
    match action {
        Action::Copy { text, .. } => text,
        _ => panic!("expected copy"),
    }
}
fn drag(p: &mut CommitPanel, c: &Context, x: u16, y: u16, end: u16) -> Action {
    let mut action = Action::None;
    for (kind, column) in [
        (M::Down(MouseButton::Left), x),
        (M::Drag(MouseButton::Left), end),
        (M::Up(MouseButton::Left), end),
    ] {
        action = p.event(
            &Event::Mouse(MouseEvent {
                kind,
                column,
                row: y,
                modifiers: Mod::NONE,
            }),
            c,
        );
    }
    action
}

#[test]
fn text_selection_message_mouse_keyboard_replace_save_and_cancel_preserve_index_and_checks() {
    let (_d, c) = fixture();
    let before = git(&c.repos[0].path, &["ls-files", "--stage"]).unwrap();
    for (w, h) in [(120, 36), (80, 24), (44, 12)] {
        let mut p = CommitPanel::default();
        p.refresh(&c).unwrap();
        let checks = p.checks.clone();
        let original = "Title ＵＩ e\u{301} 👩‍💻\nSecond line\n";
        p.saved.drafts.insert(rid(&c, 0), original.into());
        p.edit_message(&c);
        p.caret = 0;
        let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
        draw(&mut p, &mut t, &c);
        let area = p.edit_view.0;
        let first = original.lines().next().unwrap();
        assert_eq!(
            copy(drag(
                &mut p,
                &c,
                area.x,
                area.y,
                area.x + first.width() as u16
            )),
            first
        );
        draw(&mut p, &mut t, &c);
        assert_ne!(t.backend().buffer()[(area.x, area.y)].bg, SEL);
        assert!(p.selected_text().is_none());
        assert_eq!(p.caret, first.len());
        assert_eq!(p.buf, original);
        press(&mut p, &c, K::Char('a'), Mod::CONTROL);
        assert_eq!(copy(press(&mut p, &c, K::Insert, Mod::CONTROL)), original);
        p.event(&Event::Paste("Replace\nBody 👩‍💻".into()), &c);
        // Selection and copying also work when several events precede the next draw.
        press(&mut p, &c, K::Left, Mod::SHIFT);
        assert_eq!(copy(press(&mut p, &c, K::Char('c'), Mod::CONTROL)), "👩‍💻");
        press(&mut p, &c, K::Delete, Mod::NONE);
        assert_eq!(p.buf, "Replace\nBody ");
        press(&mut p, &c, K::Home, Mod::SHIFT);
        assert_eq!(copy(press(&mut p, &c, K::Char('c'), Mod::CONTROL)), "Body ");
        press(&mut p, &c, K::Char('g'), Mod::NONE);
        press(&mut p, &c, K::Char('y'), Mod::NONE);
        assert_eq!(p.buf, "Replace\ngy");
        press(&mut p, &c, K::F(2), Mod::NONE);
        assert!(p.edit.is_none());
        assert_eq!(p.saved.drafts[&rid(&c, 0)], "Replace\ngy");
        p.edit_message(&c);
        p.event(&Event::Paste("discard me".into()), &c);
        press(&mut p, &c, K::Char('a'), Mod::CONTROL);
        press(&mut p, &c, K::Esc, Mod::NONE);
        assert!(p.edit.is_some() && p.selected_text().is_none());
        press(&mut p, &c, K::Esc, Mod::NONE);
        assert!(p.edit.is_none());
        assert_eq!(p.saved.drafts[&rid(&c, 0)], "Replace\ngy");
        assert_eq!(p.checks, checks);
    }
    assert_eq!(
        git(&c.repos[0].path, &["ls-files", "--stage"]).unwrap(),
        before
    );
}

#[test]
fn text_selection_commit_preview_copies_patch_and_keeps_file_dragging_separate() {
    let (_d, c) = fixture();
    for (w, h) in [(120, 36), (80, 24), (44, 12)] {
        let mut p = CommitPanel::default();
        p.refresh(&c).unwrap();
        let checks = p.checks.clone();
        p.cursor = p.view.iter().position(|r| r.file().is_some()).unwrap();
        p.launch(Hit::Show(Focus::Diff), &c);
        let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
        draw(&mut p, &mut t, &c);
        p.diff_scroll = u16::MAX;
        draw(&mut p, &mut t, &c);
        let expected = "+model body";
        let (x, y) = (p.diff.y..p.diff.bottom())
            .find_map(|y| {
                let row = read(
                    t.backend().buffer(),
                    Rect::new(p.diff.x, y, p.diff.width, 1),
                );
                row.find(expected)
                    .map(|i| (p.diff.x + row[..i].width() as u16, y))
            })
            .unwrap();
        assert_eq!(
            copy(drag(&mut p, &c, x, y, x + expected.width() as u16)),
            expected
        );
        assert!(p.selected_text().is_none());
        assert!(p.dragged.is_none());
        press(&mut p, &c, K::Char('a'), Mod::CONTROL);
        let all = copy(press(&mut p, &c, K::Insert, Mod::CONTROL));
        assert!(all.contains(expected) && !all.contains('─'));
        key(&mut p, &c, K::Char('v'));
        draw(&mut p, &mut t, &c);
        assert!(p.diff_selection.selected_text().is_none());
        assert_eq!(p.checks, checks);
    }
}

#[test]
fn text_selection_message_drag_can_cross_a_scrolled_viewport_and_vertical_keys_keep_columns() {
    let (_d, c) = fixture();
    let mut p = CommitPanel::default();
    p.refresh(&c).unwrap();
    let message = (0..40)
        .map(|i| format!("line{i:02} ＵＩ"))
        .collect::<Vec<_>>()
        .join("\n");
    p.saved.drafts.insert(rid(&c, 0), message.clone());
    p.edit_message(&c);
    p.caret = 0;
    let mut t = Terminal::new(TestBackend::new(80, 24)).unwrap();
    draw(&mut p, &mut t, &c);
    let area = p.edit_view.0;
    click(&mut p, &c, area.x, area.y);
    p.event(
        &Event::Mouse(MouseEvent {
            kind: M::ScrollDown,
            column: area.x,
            row: area.y,
            modifiers: Mod::NONE,
        }),
        &c,
    );
    draw(&mut p, &mut t, &c);
    assert_eq!(p.edit_view.1, 3);
    let mut action = Action::None;
    for kind in [M::Drag(MouseButton::Left), M::Up(MouseButton::Left)] {
        action = p.event(
            &Event::Mouse(MouseEvent {
                kind,
                column: area.x + 6,
                row: area.y + 1,
                modifiers: Mod::NONE,
            }),
            &c,
        );
    }
    let end = message.find("line04").unwrap() + 6;
    assert_eq!(copy(action), message[..end]);
    assert!(p.selected_text().is_none());
    press(&mut p, &c, K::Up, Mod::SHIFT);
    assert_eq!(
        copy(press(&mut p, &c, K::Char('c'), Mod::CONTROL)),
        " ＵＩ\nline04"
    );
    press(&mut p, &c, K::Down, Mod::SHIFT);
    assert!(p.selected_text().is_none());
    assert_eq!(p.buf, message);
}
