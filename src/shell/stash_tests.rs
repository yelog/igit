use super::{test_support::*, *};
use crate::{core::git, ui::panes::PaneId};
use crossterm::event::MouseEvent;

#[test]
fn commit_stash_binding_review_cancel_and_saved_entry_work_through_shell() {
    let (_temp, mut app) = fixture();
    let repo = app.ctx.repos[0].path.clone();
    for (name, value) in [
        ("user.name", "Stash tester"),
        ("user.email", "stash@example.invalid"),
    ] {
        git(&repo, &["config", name, value]).unwrap();
    }
    app.config = AppConfig::from_toml("[keybindings.commit]\nstash=['F9']\n").unwrap();
    let before = snapshot(&repo);
    app.dispatch(&key(KeyCode::Char('s')));
    assert!(!app.commit.editing(), "original binding was remapped");
    app.dispatch(&key(KeyCode::F(9)));
    assert!(app.commit.editing());
    assert_eq!(app.help_content().context(), "Commit / Create Stash");
    app.dispatch(&Event::Paste("save from Commit 中文".into()));
    app.dispatch(&key(KeyCode::Enter));
    assert!(matches!(&app.pop, Some(Popup::Confirm { token, .. }) if token == "commit-stash"));
    app.dispatch(&key(KeyCode::Esc));
    assert_eq!(snapshot(&repo), before);
    assert!(git(&repo, &["stash", "list"]).unwrap().is_empty());
    app.dispatch(&key(KeyCode::F(9)));
    app.dispatch(&key(KeyCode::Enter));
    app.dispatch(&key(KeyCode::Char('y')));
    assert!(app.msg.contains("SAVED"), "{}", app.msg);
    assert!(git(&repo, &["status", "--porcelain"]).unwrap().is_empty());
    app.dispatch(&key(KeyCode::Char('6')));
    finish(&mut app);
    let text = screen(&mut app, 120, 36);
    assert!(text.contains("save from Commit 中文"), "{text}");
    assert!(text.contains("mixed.txt"), "{text}");
}

fn save(app: &Shell) {
    git(
        &app.ctx.repos[0].path,
        &[
            "-c",
            "user.name=Stash tester",
            "-c",
            "user.email=stash@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "stash",
            "push",
            "-m",
            "saved mixed changes",
        ],
    )
    .unwrap();
}

fn finish(app: &mut Shell) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while app.stash.preview_wakeup().is_some() {
        app.poll();
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[test]
fn stash_tab_header_keyboard_mouse_panes_and_help_work_at_supported_sizes() {
    let (_temp, mut app) = fixture();
    save(&app);
    let before = snapshot(&app.ctx.repos[0].path);
    for (w, h) in [(120, 36), (80, 25), (44, 12)] {
        app.dispatch(&key(KeyCode::Char('6')));
        assert_eq!(app.tab, Stash);
        finish(&mut app);
        let text = screen(&mut app, w, h);
        assert!(
            text.lines().next().unwrap().contains("Branch  Stash  Push"),
            "{text}"
        );
        assert!(
            text.contains("stash@{0}") && text.contains("mixed.txt"),
            "{text}"
        );
        assert_eq!(app.help_content().context(), "Stash / Repositories");
        app.dispatch(&key(KeyCode::Tab));
        assert_eq!(app.stash.focused_pane(), PaneId::Files);
        app.dispatch(&key(KeyCode::Enter));
        assert!(app.stash.editing());
        assert_eq!(app.help_content().context(), "Stash / Diff (read-only)");
        app.dispatch(&key(KeyCode::Char('1')));
        assert_eq!(app.tab, Stash, "Diff owns its input");
        app.dispatch(&key(KeyCode::F(1)));
        assert!(matches!(app.pop, Some(Popup::Help { .. })));
        app.dispatch(&key(KeyCode::Esc));
        app.dispatch(&key(KeyCode::Esc));
        assert!(!app.stash.editing());
        app.dispatch(&key(KeyCode::BackTab));
        app.dispatch(&key(KeyCode::Char('1')));
        screen(&mut app, w, h);
        let rect = app.regions.tab(Stash);
        app.dispatch(&Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: rect.x + 1,
            row: rect.y,
            modifiers: KeyModifiers::NONE,
        }));
        assert_eq!(app.tab, Stash);
    }
    assert_eq!(snapshot(&app.ctx.repos[0].path), before);
    app.go(Push);
    app.dispatch(&key(KeyCode::Esc));
    assert_eq!(app.tab, Stash);
}

#[test]
fn stash_global_and_panel_bindings_can_be_remapped() {
    let (_temp, fixture) = fixture();
    save(&fixture);
    let config = AppConfig::from_toml(
        "[keybindings.global]\nfocus-stash=['F9']\n[keybindings.stash]\nopen=['v']\n",
    )
    .unwrap();
    let mut app = Shell::with_config(fixture.ctx.clone(), config);
    app.dispatch(&key(KeyCode::Char('6')));
    assert_eq!(app.tab, Commit);
    app.dispatch(&key(KeyCode::F(9)));
    assert_eq!(app.tab, Stash);
    finish(&mut app);
    assert!(screen(&mut app, 120, 36).contains("v Diff"));
    app.dispatch(&key(KeyCode::Char('v')));
    assert_eq!(app.stash.focused_pane(), PaneId::Files);
    app.dispatch(&key(KeyCode::Enter));
    assert!(!app.stash.editing());
    app.dispatch(&key(KeyCode::Char('v')));
    assert!(app.stash.editing());
}

#[test]
fn stash_header_remains_clickable_with_conflicts_on_a_narrow_terminal() {
    let (_temp, mut app) = fixture();
    save(&app);
    merge_conflict(&app.ctx.repos[0].path);
    app.refresh();
    let text = screen(&mut app, 44, 12);
    for tab in Tab::DISPLAY_ORDER {
        assert!(text.lines().next().unwrap().contains(tab.name()), "{text}");
        assert!(app.regions.tab(tab).width >= tab.name().len() as u16);
    }
}

#[test]
fn stash_focus_refresh_and_roots_update_list_and_maximize_keeps_panes_reachable() {
    let (_temp, mut app) = fixture();
    app.go(Stash);
    assert!(screen(&mut app, 80, 25).contains("No stashes"));
    save(&app);
    app.dispatch(&Event::FocusGained);
    finish(&mut app);
    assert!(screen(&mut app, 80, 25).contains("stash@{0}"));
    app.set_pane_maximized(true);
    screen(&mut app, 80, 25);
    assert_eq!(
        app.stash
            .panes
            .nodes()
            .iter()
            .filter(|(_, r)| !r.is_empty())
            .count(),
        1
    );
    app.dispatch(&key(KeyCode::Tab));
    screen(&mut app, 80, 25);
    assert_eq!(app.stash.focused_pane(), PaneId::Files);
    assert!(!app.stash.panes.rect(PaneId::Files).unwrap().is_empty());
    app.set_pane_maximized(false);
    app.ctx.scope.clear();
    app.refresh();
    assert!(screen(&mut app, 80, 25).contains("No repositories in Roots"));
}
