use super::*;

fn press(p: &mut PushPanel, c: &Context, code: KeyCode, modifiers: KeyModifiers) -> Action {
    p.event(&Event::Key(KeyEvent::new(code, modifiers)), c)
}
fn copy(action: Action) -> String {
    match action {
        Action::Copy { text, .. } => text,
        _ => panic!("expected copy"),
    }
}

#[test]
fn text_selection_push_details_wrap_scroll_and_close_keep_push_scope() {
    for (w, h) in [(120, 36), (80, 24), (44, 12)] {
        let (mut p, c) = fixture(IconMode::Unicode);
        p.show_details = true;
        p.focus = 2;
        let text = format!(
            "+ＵＩ e\u{301}\n\n  {}\n",
            "long wrapped original text ".repeat(15)
        );
        p.preview.body = text.clone();
        p.preview.is_patch = true;
        let checked = p.checked.clone();
        draw(&mut p, &c, w, h);
        let area = pane("", true).inner(p.panes.rect(PaneId::PushDetails).unwrap());
        let first = "+ＵＩ e\u{301}";
        let mut action = Action::None;
        for (kind, x) in [
            (MouseEventKind::Down(MouseButton::Left), area.x),
            (
                MouseEventKind::Drag(MouseButton::Left),
                area.x + first.width() as u16,
            ),
            (
                MouseEventKind::Up(MouseButton::Left),
                area.x + first.width() as u16,
            ),
        ] {
            action = p.event(
                &Event::Mouse(MouseEvent {
                    kind,
                    column: x,
                    row: area.y,
                    modifiers: KeyModifiers::NONE,
                }),
                &c,
            );
        }
        let b = draw(&mut p, &c, w, h);
        assert_ne!(b[(area.x, area.y)].bg, SEL);
        assert_eq!(copy(action), first);
        assert!(p.selected_text().is_none());
        press(&mut p, &c, KeyCode::Char('a'), KeyModifiers::CONTROL);
        key(&mut p, &c, KeyCode::Char('j'));
        draw(&mut p, &c, w, h);
        assert_eq!(p.selected_text(), Some(text.as_str()));
        press(&mut p, &c, KeyCode::Char('a'), KeyModifiers::CONTROL);
        assert_eq!(
            copy(press(&mut p, &c, KeyCode::Insert, KeyModifiers::CONTROL)),
            text
        );
        key(&mut p, &c, KeyCode::Esc);
        assert!(p.show_details && p.selected_text().is_none());
        key(&mut p, &c, KeyCode::Esc);
        assert!(!p.show_details);
        draw(&mut p, &c, w, h);
        assert!(p.detail_selection.selected_text().is_none());
        assert_eq!(p.checked, checked);
        assert!(p.review.is_empty() && p.inflight.is_none());
    }
}
