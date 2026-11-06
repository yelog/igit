use super::*;

fn position(text: &str, needle: &str) -> (u16, u16) {
    text.lines()
        .enumerate()
        .find_map(|(y, line)| {
            line.find(needle)
                .map(|x| (line[..x].width() as u16, y as u16))
        })
        .unwrap_or_else(|| panic!("{needle:?} not visible:\n{text}"))
}

fn saved(app: &Shell, message: &str) {
    let state: serde_json::Value =
        serde_json::from_slice(&fs::read(app.ctx.state_dir.join("commit.json")).unwrap()).unwrap();
    assert!(state["drafts"]
        .as_object()
        .unwrap()
        .values()
        .any(|v| v == message));
}

#[test]
fn inline_message_mouse_and_tab_leave_the_field_with_a_saved_draft() {
    let (_dir, mut app) = fixture();
    let before = snapshot(&app.ctx.repos[0].path);
    for (w, h) in [(120, 36), (80, 24), (44, 12)] {
        app.log.focus_pane(PaneId::Graph);
        app.go(Split(Commit));
        screen(&mut app, w, h);
        app.dispatch(&key(KeyCode::Char('i')));
        app.dispatch(&Event::Key(KeyEvent::new(
            KeyCode::Char('a'),
            KeyModifiers::CONTROL,
        )));
        app.dispatch(&Event::Paste("inline draft".into()));
        let text = screen(&mut app, w, h);
        let (x, y) = position(&text, "inline draft");
        let composer = app.commit.panes.rect(PaneId::Message).unwrap();
        assert!(inside(composer, x, y));
        assert!(!text.contains("Edit commit message"));
        assert!(text.contains("Git Log [1]") && text.contains("base"));
        let log = app.log.panes.rect(PaneId::Graph).unwrap();
        app.dispatch(&mouse(
            MouseEventKind::Down(MouseButton::Left),
            log.x + 1,
            log.y + 1,
        ));
        assert_eq!(app.surface(), Split(Log));
        assert!(!app.commit.editing());
        saved(&app, "inline draft");

        for code in [KeyCode::Tab, KeyCode::BackTab, KeyCode::F(6)] {
            app.go(Split(Commit));
            screen(&mut app, w, h);
            app.dispatch(&key(KeyCode::Char('i')));
            app.dispatch(&Event::Key(KeyEvent::new(
                KeyCode::Char('a'),
                KeyModifiers::CONTROL,
            )));
            app.dispatch(&Event::Paste("saved by navigation".into()));
            app.dispatch(&key(code));
            assert!(!app.commit.editing());
            assert_eq!(
                app.surface(),
                Split(if code == KeyCode::BackTab {
                    Commit
                } else {
                    Log
                })
            );
            saved(&app, "saved by navigation");
        }
    }
    assert_eq!(snapshot(&app.ctx.repos[0].path), before);
}

#[test]
fn inline_message_mouse_review_preserves_mixed_and_unselected_staging() {
    let (_dir, mut app) = fixture();
    let before = snapshot(&app.ctx.repos[0].path);
    app.go(Split(Commit));
    screen(&mut app, 120, 36);
    // Last row is other.txt: leave its independently staged contents unchecked.
    for code in [KeyCode::End, KeyCode::Char(' '), KeyCode::Home] {
        app.dispatch(&key(code));
    }
    let text = screen(&mut app, 120, 36);
    let (x, y) = position(&text, "Commit message…");
    app.dispatch(&mouse(MouseEventKind::Down(MouseButton::Left), x, y));
    app.dispatch(&Event::Paste("Reviewed inline ＵＩ\n\nBody".into()));
    let text = screen(&mut app, 120, 36);
    let (x, y) = position(&text, "Commit…");
    app.dispatch(&mouse(MouseEventKind::Down(MouseButton::Left), x, y));
    let Some(Popup::Confirm { body, .. }) = &app.pop else {
        panic!("expected commit review");
    };
    assert!(body.contains("Reviewed inline ＵＩ") && body.contains("mixed.txt"));
    assert!(body.contains("+staged") && !body.contains("working tree"));
    assert!(!body.contains("other.txt") && !body.contains("unselected staged"));
    assert_eq!(snapshot(&app.ctx.repos[0].path), before);
    app.dispatch(&key(KeyCode::Esc));
    assert!(app.pop.is_none());
    assert_eq!(snapshot(&app.ctx.repos[0].path), before);
}

#[test]
fn inline_message_failed_save_blocks_workspace_navigation_and_reports_the_error() {
    let (dir, mut app) = fixture();
    app.go(Split(Commit));
    screen(&mut app, 120, 36);
    app.dispatch(&key(KeyCode::Char('i')));
    app.dispatch(&Event::Paste("unsaved input".into()));
    screen(&mut app, 120, 36);
    app.ctx.state_dir = dir.path().join("blocked-state");
    fs::write(&app.ctx.state_dir, "not a directory").unwrap();
    let log = app.log.panes.rect(PaneId::Graph).unwrap();
    app.dispatch(&mouse(
        MouseEventKind::Down(MouseButton::Left),
        log.x + 1,
        log.y + 1,
    ));
    assert!(app.commit.editing() && app.surface() == Split(Commit));
    assert!(app.msg.starts_with("Commit message:"), "{}", app.msg);
    app.dispatch(&Event::Key(KeyEvent::new(
        KeyCode::Char('a'),
        KeyModifiers::CONTROL,
    )));
    assert_eq!(app.commit.selected_text(), Some("unsaved input"));
}
