use super::*;
use crate::ui::panes::PaneId;

#[test]
fn commit_revision_controls_keep_global_keys_and_panel_focus_in_their_contexts() {
    let (_dir, mut app) = fixture();
    let repo = app.ctx.repos[0].path.clone();
    let before = snapshot(&repo);
    app.dispatch(&key(KeyCode::Home));
    app.dispatch(&key(KeyCode::Down));
    screen(&mut app, 160, 36);
    app.commit.focus_pane(PaneId::Preview);
    let text = screen(&mut app, 160, 36);
    assert!(text.contains("Before") && text.contains("After"), "{text}");
    assert!(
        text.contains("Revisions") && text.contains("n/p Changes"),
        "{text}"
    );
    for code in ['f', 'p'] {
        app.dispatch(&key(KeyCode::Char(code)));
        assert!(app.pop.is_none(), "{code} opened a workspace review");
        app.dispatch_builtin(&key(KeyCode::Char(code)));
        assert!(
            app.pop.is_none(),
            "replayed {code} opened a workspace review"
        );
    }
    assert!(app.configured_footer().contains("n/p Changes"));
    let capture = clipboard::CopyCapture::start();
    app.dispatch(&Event::Key(KeyEvent::new(
        KeyCode::Char('a'),
        KeyModifiers::CONTROL,
    )));
    let selected = app.panel().selected_text().unwrap().to_owned();
    assert_eq!(selected, "staged\n");
    app.dispatch(&Event::Key(KeyEvent::new(
        KeyCode::Char('c'),
        KeyModifiers::CONTROL,
    )));
    assert!(!app.quit && app.pop.is_none());
    assert_eq!(capture.texts(), [selected]);
    app.dispatch(&key(KeyCode::Char('[')));
    screen(&mut app, 160, 36);
    app.dispatch(&key(KeyCode::Char('y')));
    assert_eq!(capture.texts().last().unwrap(), "base\n");
    app.dispatch(&key(KeyCode::F(1)));
    app.dispatch(&Event::Paste("Focus next revision".into()));
    app.dispatch(&key(KeyCode::Enter));
    assert!(app.pop.is_none());
    app.dispatch(&key(KeyCode::Char('y')));
    assert_eq!(capture.texts().last().unwrap(), "staged\n");
    app.dispatch(&key(KeyCode::Tab));
    assert_eq!(app.commit.focused_pane(), PaneId::Message);
    app.dispatch(&key(KeyCode::Char('p')));
    assert!(matches!(app.pop, Some(Popup::Update(_))));
    app.dispatch(&key(KeyCode::Esc));
    app.commit.focus_pane(PaneId::Preview);
    app.dispatch(&key(KeyCode::Char('2')));
    assert_eq!(app.tab, Log);
    assert_eq!(snapshot(&repo), before);
}

#[test]
fn configured_tree_bindings_do_not_take_keys_from_the_revision_preview() {
    let (_dir, mut app) = fixture();
    app.config = AppConfig::from_toml(
        "[keybindings.commit]\ncollapse-all=['Alt-z']\nprevious-repo=['Alt-[']\nnext-repo=['Alt-]']",
    )
    .unwrap();
    let repo = app.ctx.repos[0].path.clone();
    let before = snapshot(&repo);
    app.dispatch(&key(KeyCode::Home));
    app.dispatch(&key(KeyCode::Down));
    screen(&mut app, 160, 36);
    app.commit.focus_pane(PaneId::Preview);
    let capture = clipboard::CopyCapture::start();
    app.dispatch(&key(KeyCode::Char('[')));
    app.dispatch(&key(KeyCode::Char('y')));
    assert_eq!(capture.texts().last().unwrap(), "base\n");
    app.dispatch(&key(KeyCode::Char(']')));
    app.dispatch(&key(KeyCode::Char('y')));
    assert_eq!(capture.texts().last().unwrap(), "staged\n");
    app.dispatch(&key(KeyCode::Char('z')));
    let zoomed = screen(&mut app, 160, 36);
    assert!(
        zoomed.contains("After") && !zoomed.contains("Before"),
        "{zoomed}"
    );
    app.dispatch(&key(KeyCode::Char('z')));
    let split = screen(&mut app, 160, 36);
    assert!(
        split.contains("Before") && split.contains("After"),
        "{split}"
    );
    assert_eq!(snapshot(&repo), before);
}
