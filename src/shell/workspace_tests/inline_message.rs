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
    assert_eq!(state["message"], message);
}

#[test]
fn inline_message_keyboard_edits_saves_and_commits_only_on_enter() {
    let (_dir, mut app) = fixture();
    let repo = app.ctx.repos[0].path.clone();
    let before = snapshot(&repo);
    screen(&mut app, 120, 36);
    for code in [KeyCode::End, KeyCode::Char(' '), KeyCode::Char('i')] {
        app.dispatch(&key(code));
    }
    app.dispatch(&Event::Paste("Keyboard subject".into()));
    for code in [KeyCode::F(2), KeyCode::Tab] {
        app.dispatch(&key(code));
    }
    assert_eq!(app.commit.focused_pane(), PaneId::Changes);

    app.dispatch(&key(KeyCode::Char('c')));
    assert!(app.commit.message_editing(), "c must open the editor");
    assert_eq!(app.commit.focused_pane(), PaneId::Message);
    app.dispatch(&key(KeyCode::Enter));
    app.dispatch(&Event::Paste("Body 中文".into()));
    app.dispatch(&Event::Key(KeyEvent::new(
        KeyCode::Char('a'),
        KeyModifiers::CONTROL,
    )));
    app.dispatch(&key(KeyCode::Esc));
    assert!(!app.commit.message_editing());
    assert_eq!(app.commit.focused_pane(), PaneId::Message);
    saved(&app, "Keyboard subject\nBody 中文");

    app.dispatch(&key(KeyCode::Char('c')));
    assert!(!app.commit.message_editing());
    app.dispatch(&key(KeyCode::Char('i')));
    assert!(app.commit.message_editing());
    app.dispatch(&Event::Paste(" updated".into()));
    app.dispatch(&key(KeyCode::Esc));
    assert_eq!(snapshot(&repo), before);

    app.dispatch(&key(KeyCode::Enter));
    assert!(app.pop.is_none());
    assert_eq!(git(&repo, &["rev-parse", "HEAD^"]).unwrap(), before[0]);
    assert_eq!(
        git(&repo, &["log", "-1", "--format=%B"]).unwrap(),
        "Keyboard subject\nBody 中文 updated"
    );
    assert_eq!(git(&repo, &["show", "HEAD:mixed.txt"]).unwrap(), "staged");
    assert_eq!(
        git(&repo, &["show", ":other.txt"]).unwrap(),
        "unselected staged"
    );
    assert_eq!(git(&repo, &["diff", "--binary"]).unwrap(), before[2]);
}

#[test]
fn inline_message_mouse_and_tab_leave_the_field_with_a_saved_draft() {
    let (_dir, mut app) = fixture();
    let before = snapshot(&app.ctx.repos[0].path);
    for (w, h) in [(120, 36), (80, 24), (44, 12)] {
        app.go(Commit);
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
        click_tab(&mut app, Log);
        assert_eq!(app.tab, Log);
        assert!(!app.commit.editing());
        saved(&app, "inline draft");

        for code in [KeyCode::Tab, KeyCode::BackTab, KeyCode::F(6)] {
            app.go(Commit);
            screen(&mut app, w, h);
            app.dispatch(&key(KeyCode::Char('i')));
            app.dispatch(&Event::Key(KeyEvent::new(
                KeyCode::Char('a'),
                KeyModifiers::CONTROL,
            )));
            app.dispatch(&Event::Paste("saved by navigation".into()));
            app.dispatch(&key(code));
            assert!(!app.commit.editing());
            assert_eq!(app.tab, Commit);
            assert_eq!(
                app.commit.focused_pane(),
                if code == KeyCode::BackTab {
                    PaneId::Preview
                } else {
                    PaneId::Changes
                }
            );
            // Return from the full-width preview before the next inline edit.
            app.commit.focus_pane(PaneId::Changes);
            saved(&app, "saved by navigation");
        }
    }
    assert_eq!(snapshot(&app.ctx.repos[0].path), before);
}

#[test]
fn inline_message_click_commits_directly_and_preserves_unselected_staging() {
    let (_dir, mut app) = fixture();
    app.commit.enable_background_execution();
    let repo = app.ctx.repos[0].path.clone();
    let before = snapshot(&repo);
    app.go(Commit);
    screen(&mut app, 120, 36);
    // Last row is other.txt: leave its independently staged contents unchecked.
    for code in [KeyCode::End, KeyCode::Char(' '), KeyCode::Home] {
        app.dispatch(&key(code));
    }
    let text = screen(&mut app, 120, 36);
    let (x, y) = position(&text, "Commit message…");
    app.dispatch(&mouse(MouseEventKind::Down(MouseButton::Left), x, y));
    let message = "Commit inline ＵＩ\n\nBody";
    app.dispatch(&Event::Paste(message.into()));
    let text = screen(&mut app, 120, 36);
    let (x, y) = position(&text, "Commit   Commit and Push...");
    app.dispatch(&mouse(MouseEventKind::Down(MouseButton::Left), x, y));
    assert!(app.pop.is_none(), "Commit should not open a confirmation");
    finish_commit(&mut app);
    assert!(app.pop.is_none());
    assert_eq!(app.tab, Commit);
    assert_eq!(git(&repo, &["rev-parse", "HEAD^"]).unwrap(), before[0]);
    assert_eq!(git(&repo, &["log", "-1", "--format=%B"]).unwrap(), message);
    assert_eq!(git(&repo, &["show", "HEAD:mixed.txt"]).unwrap(), "staged");
    assert_eq!(git(&repo, &["show", "HEAD:other.txt"]).unwrap(), "base");
    assert_eq!(
        git(&repo, &["show", ":other.txt"]).unwrap(),
        "unselected staged"
    );
    assert_eq!(
        git(&repo, &["diff", "--cached", "--name-only"]).unwrap(),
        "other.txt"
    );
    assert_eq!(git(&repo, &["diff", "--binary"]).unwrap(), before[2]);
}

#[test]
fn inline_message_failed_save_blocks_workspace_navigation_and_reports_the_error() {
    let (dir, mut app) = fixture();
    app.go(Commit);
    screen(&mut app, 120, 36);
    app.dispatch(&key(KeyCode::Char('i')));
    app.dispatch(&Event::Paste("unsaved input".into()));
    screen(&mut app, 120, 36);
    app.ctx.state_dir = dir.path().join("blocked-state");
    fs::write(&app.ctx.state_dir, "not a directory").unwrap();
    click_tab(&mut app, Log);
    assert!(app.commit.editing() && app.tab == Commit);
    assert!(app.msg.starts_with("Commit message:"), "{}", app.msg);
    app.dispatch(&Event::Key(KeyEvent::new(
        KeyCode::Char('a'),
        KeyModifiers::CONTROL,
    )));
    assert_eq!(app.commit.selected_text(), Some("unsaved input"));
}
