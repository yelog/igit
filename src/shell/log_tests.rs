use super::{test_support::*, *};
use crate::{core::git, ui::panes::PaneId};

#[test]
fn log_enter_focuses_diff_files_and_escape_preserves_each_pane_and_git_state() {
    let (_temp, mut app) = fixture();
    let repo = app.ctx.repos[0].path.clone();
    let before = snapshot(&repo);
    let index = std::fs::read(repo.join(".git/index")).unwrap();
    app.go(Log);
    app.dispatch(&key(KeyCode::F(5)));
    for (width, height) in [(120, 36), (80, 24), (44, 12)] {
        app.log.focus_pane(PaneId::Graph);
        screen(&mut app, width, height);
        app.dispatch(&key(KeyCode::Enter));
        screen(&mut app, width, height);
        assert_eq!(app.log.focused_pane(), PaneId::Files);
        assert!(!app.log.editing());
        assert!(app.pop.is_none());
        for pane in [PaneId::Refs, PaneId::Files, PaneId::Info] {
            app.log.focus_pane(pane);
            screen(&mut app, width, height);
            app.dispatch(&key(KeyCode::Esc));
            assert_eq!(app.log.focused_pane(), pane);
            assert_eq!(app.tab, Log);
            assert!(!app.log.editing());
            assert!(app.pop.is_none());
        }
    }
    app.log.focus_pane(PaneId::Graph);
    app.dispatch(&key(KeyCode::F(1)));
    app.dispatch(&Event::Paste("Focus Diff pane".into()));
    app.dispatch(&key(KeyCode::Enter));
    assert_eq!(app.log.focused_pane(), PaneId::Files);
    assert!(!app.log.editing());
    assert!(app.pop.is_none());
    assert_eq!(snapshot(&repo), before);
    assert_eq!(std::fs::read(repo.join(".git/index")).unwrap(), index);
}

#[test]
fn log_actions_review_cancels_by_default_and_revert_conflict_refreshes_workspace() {
    let (_temp, mut app) = fixture();
    let repo = app.ctx.repos[0].path.clone();
    let run = |args: &[&str]| git(&repo, args).unwrap();
    for (key, value) in [
        ("user.name", "Log action tester"),
        ("user.email", "log@example.invalid"),
        ("commit.gpgsign", "false"),
        ("core.hooksPath", "/dev/null"),
    ] {
        run(&["config", key, value]);
    }
    run(&["restore", "--staged", "--worktree", "."]);
    for text in ["first change", "later change"] {
        std::fs::write(repo.join("mixed.txt"), text).unwrap();
        run(&["commit", "-am", text]);
    }
    let before = snapshot(&repo);
    app.config = AppConfig::from_toml(
        "[keybindings.global]\nnotification-history=[]\n[keybindings.log]\ncommit-actions=['F8']",
    )
    .unwrap();
    app.go(Log);
    app.dispatch(&key(KeyCode::F(5)));
    screen(&mut app, 44, 12);
    app.dispatch(&key(KeyCode::Char('g')));
    app.dispatch(&key(KeyCode::Char('g')));
    app.dispatch(&key(KeyCode::Char('j')));
    app.dispatch(&key(KeyCode::Char('a')));
    assert!(!app.log.editing());
    let open_review = |app: &mut Shell| {
        app.dispatch(&key(KeyCode::F(8)));
        for _ in 0..4 {
            app.dispatch(&key(KeyCode::Down));
        }
        app.dispatch(&key(KeyCode::Enter));
        let Some(Popup::Confirm {
            owner: Log,
            yes: false,
            title,
            body,
            ..
        }) = &app.pop
        else {
            panic!("Expected Log review: {}", app.notifications.latest_body());
        };
        assert_eq!(title, "Revert Commit");
        assert!(body.contains("first change"), "{body}");
    };
    open_review(&mut app);
    app.dispatch(&key(KeyCode::Enter));
    assert!(app.pop.is_none());
    assert_eq!(snapshot(&repo), before);
    assert!(!app.tab_available(Conflicts));

    open_review(&mut app);
    let review = screen(&mut app, 44, 12);
    assert!(
        review.contains("Revert Commit") && review.contains("Cancel") && review.contains("Confirm")
    );
    app.dispatch(&key(KeyCode::Tab));
    app.dispatch(&key(KeyCode::Enter));
    assert!(app.pop.is_none());
    assert!(app.tab_available(Conflicts));
    assert_eq!(run(&["rev-parse", "HEAD"]), before[0]);
    assert!(repo.join(".git/REVERT_HEAD").is_file());
    assert!(!run(&["ls-files", "--unmerged"]).is_empty());
}

#[test]
fn log_shortcut_remap_opens_review_and_cancellation_keeps_repository_unchanged() {
    let (_temp, mut app) = fixture();
    let repo = app.ctx.repos[0].path.clone();
    let run = |args: &[&str]| git(&repo, args).unwrap();
    for (key, value) in [
        ("user.name", "Log shortcut tester"),
        ("user.email", "shortcut@example.invalid"),
        ("commit.gpgsign", "false"),
        ("core.hooksPath", "/dev/null"),
    ] {
        run(&["config", key, value]);
    }
    run(&["restore", "--staged", "--worktree", "."]);
    for text in ["base change", "middle change", "tip change"] {
        std::fs::write(repo.join("mixed.txt"), text).unwrap();
        run(&["commit", "-am", text]);
    }
    let before = snapshot(&repo);
    app.config = AppConfig::from_toml(
        "[keybindings.global]\nnotification-history=[]\n[keybindings.log]\ndrop-commit=['F8']",
    )
    .unwrap();
    app.go(Log);
    app.dispatch(&key(KeyCode::F(5)));
    screen(&mut app, 44, 12);
    app.dispatch(&key(KeyCode::Char('g')));
    app.dispatch(&key(KeyCode::Char('g')));

    app.dispatch(&key(KeyCode::Char('d')));
    assert!(app.pop.is_none());
    assert_eq!(snapshot(&repo), before);

    app.dispatch(&key(KeyCode::F(8)));
    let Some(Popup::Confirm {
        owner: Log,
        yes: false,
        title,
        ..
    }) = &app.pop
    else {
        panic!(
            "Expected remapped Drop review: {}",
            app.notifications.latest_body()
        );
    };
    assert_eq!(title, "Drop Commit");
    assert_eq!(snapshot(&repo), before);
    app.dispatch(&key(KeyCode::Esc));
    assert!(app.pop.is_none());
    assert_eq!(snapshot(&repo), before);

    app.dispatch(&key(KeyCode::Char('u')));
    assert!(matches!(
        &app.pop,
        Some(Popup::Confirm { owner: Log, yes: false, title, .. })
            if title == "Undo Commit"
    ));
    app.dispatch(&key(KeyCode::Esc));
    assert!(app.pop.is_none());
    assert_eq!(snapshot(&repo), before);

    app.dispatch(&key(KeyCode::Char('R')));
    assert!(matches!(
        &app.pop,
        Some(Popup::Confirm { owner: Log, yes: false, title, .. })
            if title == "Revert Commit"
    ));
    app.dispatch(&key(KeyCode::Enter));
    assert!(app.pop.is_none());
    assert_eq!(snapshot(&repo), before);
}

#[test]
fn workspace_compare_remapped_entry_help_panes_refresh_and_return_keep_log_and_git_state() {
    use crate::ui::panes::PaneId;

    let (_temp, mut app) = fixture();
    let repo = app.ctx.repos[0].path.clone();
    let before = snapshot(&repo);
    app.config = AppConfig::from_toml(
        "[keybindings.global]\nnotification-history=[]\n[keybindings.log]\ncompare-with-local=['F8']",
    )
    .unwrap();
    app.go(Log);
    screen(&mut app, 120, 36);
    app.dispatch(&key(KeyCode::Char('D')));
    assert!(!app.log.comparing_workspace());
    app.dispatch(&key(KeyCode::F(8)));
    assert!(app.log.comparing_workspace());
    let deadline = Instant::now() + Duration::from_secs(10);
    while app.log.preview_wakeup().is_some() {
        app.poll();
        assert!(Instant::now() < deadline, "workspace comparison timed out");
        std::thread::sleep(Duration::from_millis(1));
    }
    for (width, height) in [(44, 12), (80, 25), (120, 36), (184, 48)] {
        let text = screen(&mut app, width, height);
        assert!(text.contains("Working tree"), "{text}");
        if width == 184 {
            let footer = text.lines().last().unwrap();
            assert!(
                footer.contains("J/K Scroll") && footer.contains("Esc Log"),
                "workspace comparison footer missing: {text}"
            );
        }
    }
    assert_eq!(app.help_content().context(), "Log / Workspace Files");
    assert_eq!(app.log.focused_pane(), PaneId::Files);
    app.dispatch(&key(KeyCode::Tab));
    assert_eq!(app.log.focused_pane(), PaneId::Info);
    app.dispatch(&key(KeyCode::Char('f')));
    app.dispatch(&key(KeyCode::Char('p')));
    assert!(app.pop.is_none());
    app.dispatch(&key(KeyCode::F(1)));
    let help = screen(&mut app, 120, 36);
    assert!(help.contains("WORKSPACE DIFF"), "{help}");
    assert!(
        !help.contains("Drop commit") && !help.contains("Cherry-pick"),
        "{help}"
    );
    app.dispatch(&key(KeyCode::Esc));
    app.dispatch(&key(KeyCode::Tab));
    assert_eq!(app.log.focused_pane(), PaneId::Files);
    app.dispatch(&key(KeyCode::F(5)));
    let deadline = Instant::now() + Duration::from_secs(10);
    while app.log.preview_wakeup().is_some() {
        app.poll();
        assert!(Instant::now() < deadline, "workspace refresh timed out");
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(app.log.comparing_workspace());
    app.dispatch(&key(KeyCode::Esc));
    assert!(!app.log.comparing_workspace());
    assert_eq!(app.tab, Log);
    assert_eq!(app.log.focused_pane(), PaneId::Graph);
    assert_eq!(snapshot(&repo), before);
}
