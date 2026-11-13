use super::{test_support::*, *};
use crate::{core::git, ui::panes::PaneId};

fn wait_browser_notice(app: &mut Shell, expected: &str) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        app.poll();
        if app.notifications.latest_body().contains(expected) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "Browser request timed out: {}",
            app.notifications.latest_body()
        );
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[test]
fn log_browser_remapping_and_help_open_the_selected_commit_without_git_writes() {
    let (_temp, mut app) = fixture();
    let repo = app.ctx.repos[0].path.clone();
    git(
        &repo,
        &[
            "remote",
            "add",
            "origin",
            "git@gitlab.example.invalid:team/subgroup/repo.git",
        ],
    )
    .unwrap();
    let oid = git(&repo, &["rev-parse", "HEAD"]).unwrap();
    let before = snapshot(&repo);
    let index = std::fs::read(repo.join(".git/index")).unwrap();
    app.config = AppConfig::from_toml(
        "[keybindings.global]\nnotification-history=[]\n[keybindings.log]\nopen-in-browser=['F8']",
    )
    .unwrap();
    app.go(Log);
    app.dispatch(&key(KeyCode::F(5)));
    screen(&mut app, 80, 24);
    let capture = browser::BrowserCapture::start(false);
    app.dispatch(&key(KeyCode::Char('O')));
    assert!(capture.urls().is_empty());
    app.dispatch(&key(KeyCode::F(8)));
    wait_browser_notice(&mut app, "Opened commit in browser");
    let expected = format!("https://gitlab.example.invalid/team/subgroup/repo/-/commit/{oid}");
    assert_eq!(capture.urls(), vec![expected.clone()]);
    assert_eq!(app.log.focused_pane(), PaneId::Graph);
    assert!(app.pop.is_none());
    for kind in [KeyEventKind::Repeat, KeyEventKind::Release] {
        app.dispatch(&Event::Key(KeyEvent::new_with_kind(
            KeyCode::F(8),
            KeyModifiers::NONE,
            kind,
        )));
        assert!(!app.browser.pending());
    }
    app.notifications = Default::default();
    app.dispatch(&key(KeyCode::F(1)));
    app.dispatch(&Event::Paste("Open commit in browser".into()));
    assert!(screen(&mut app, 80, 24).contains("F8"));
    app.dispatch(&key(KeyCode::Enter));
    wait_browser_notice(&mut app, "Opened commit in browser");
    assert_eq!(capture.urls(), vec![expected.clone(), expected]);
    assert!(app.pop.is_none());
    assert_eq!(snapshot(&repo), before);
    assert_eq!(std::fs::read(repo.join(".git/index")).unwrap(), index);
}

#[test]
fn log_browser_failures_use_notifications_and_keep_input_and_repository_state() {
    let (_temp, mut app) = fixture();
    let repo = app.ctx.repos[0].path.clone();
    git(
        &repo,
        &[
            "config",
            "igit.webCommitUrl",
            "https://forge.example.invalid/commit/{commit}",
        ],
    )
    .unwrap();
    let before = snapshot(&repo);
    let index = std::fs::read(repo.join(".git/index")).unwrap();
    app.go(Log);
    app.dispatch(&key(KeyCode::F(5)));
    screen(&mut app, 44, 12);
    let capture = browser::BrowserCapture::start(true);
    app.dispatch(&key(KeyCode::Char('/')));
    app.dispatch(&key(KeyCode::Char('O')));
    assert!(app.log.editing());
    assert!(capture.urls().is_empty());
    app.dispatch(&key(KeyCode::Esc));
    app.dispatch(&key(KeyCode::Char('O')));
    wait_browser_notice(&mut app, "Test browser could not open the URL");
    assert!(app
        .notifications
        .latest_body()
        .contains("Cannot open commit in browser"));
    assert_eq!(capture.urls().len(), 1);
    assert_eq!(app.log.focused_pane(), PaneId::Graph);
    assert!(app.pop.is_none());
    assert!(!app.log.editing());
    git(&repo, &["config", "--unset", "igit.webCommitUrl"]).unwrap();
    git(
        &repo,
        &[
            "remote",
            "add",
            "origin",
            "https://user:secret@unknown.example.invalid/team/repo",
        ],
    )
    .unwrap();
    app.dispatch(&key(KeyCode::Char('O')));
    wait_browser_notice(&mut app, "Unrecognized Git hosting service");
    assert!(!app.notifications.latest_body().contains("secret"));
    assert_eq!(capture.urls().len(), 1);
    assert_eq!(snapshot(&repo), before);
    assert_eq!(std::fs::read(repo.join(".git/index")).unwrap(), index);
}

#[test]
fn log_branch_search_uses_the_header_input_help_and_text_guards_without_git_writes() {
    let (_temp, mut app) = fixture();
    let repo = app.ctx.repos[0].path.clone();
    let before = snapshot(&repo);
    let index = std::fs::read(repo.join(".git/index")).unwrap();
    app.go(Log);
    app.dispatch(&key(KeyCode::F(5)));
    for (width, height) in [(120, 36), (80, 24), (44, 12)] {
        app.log.focus_pane(PaneId::Refs);
        screen(&mut app, width, height);
        let help = app.help_content();
        assert_eq!(help.context(), "Log / Branch Tree");
        assert!(!help
            .sections
            .iter()
            .any(|section| section.title == "Pane Search"));
        app.dispatch(&key(KeyCode::Char('/')));
        assert!(app.log.editing());
        assert_eq!(app.help_content().context(), "Log / Branch Search");
        assert!(app.log.focused_search().is_none());
        for character in ['q', 'r', '3', '/', 'n'] {
            app.dispatch(&key(KeyCode::Char(character)));
        }
        assert_eq!(app.tab, Log);
        assert!(!app.quit && app.pop.is_none());
        let text = screen(&mut app, width, height);
        if width >= 80 {
            assert!(text.contains("qr3/n"), "{width}×{height}: {text}");
        }
        app.dispatch(&key(KeyCode::F(1)));
        assert!(
            matches!(&app.pop, Some(Popup::Help { state, .. }) if state.content.context() == "Log / Branch Search")
        );
        app.dispatch(&key(KeyCode::Esc));
        assert!(app.log.editing());
        app.dispatch(&key(KeyCode::Esc));
        assert_eq!(app.help_content().context(), "Log / Branch Tree");
        assert_eq!(app.log.focused_pane(), PaneId::Refs);
    }
    assert_eq!(snapshot(&repo), before);
    assert_eq!(std::fs::read(repo.join(".git/index")).unwrap(), index);
}

#[test]
fn log_branch_search_remaps_independently_of_file_tree_find() {
    let (_temp, mut app) = fixture();
    app.config = AppConfig::from_toml(
        "[keybindings.global]\nnotification-history=[]\n[keybindings.log]\nsearch-refs=['F8']\nfind=['F7']",
    )
    .unwrap();
    app.go(Log);
    app.dispatch(&key(KeyCode::F(5)));
    screen(&mut app, 120, 36);
    app.log.focus_pane(PaneId::Refs);
    for shortcut in [KeyCode::Char('/'), KeyCode::Char('S'), KeyCode::F(7)] {
        app.dispatch(&key(shortcut));
        assert!(!app.log.editing());
    }
    app.dispatch(&key(KeyCode::F(1)));
    app.dispatch(&Event::Paste("Search branches or tags".into()));
    assert!(screen(&mut app, 120, 36).contains("F8"));
    app.dispatch(&key(KeyCode::Enter));
    assert!(app.pop.is_none());
    assert_eq!(app.help_content().context(), "Log / Branch Search");
    app.dispatch(&key(KeyCode::Esc));
    app.dispatch(&key(KeyCode::F(8)));
    assert_eq!(app.help_content().context(), "Log / Branch Search");
    app.dispatch(&key(KeyCode::Esc));
    app.log.focus_pane(PaneId::Files);
    app.dispatch(&key(KeyCode::F(7)));
    assert_eq!(app.help_content().context(), "Pane Search");
    app.dispatch(&key(KeyCode::Esc));
    app.log.focus_pane(PaneId::Graph);
    app.dispatch(&key(KeyCode::Char('/')));
    assert_eq!(app.help_content().context(), "Log / Text Search");
}

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

#[test]
fn workspace_compare_shell_applies_the_current_hunk_and_refreshes_without_staging() {
    let (_temp, mut app) = fixture();
    let repo = app.ctx.repos[0].path.clone();
    app.go(Log);
    screen(&mut app, 184, 36);
    let oid = git(&repo, &["rev-parse", "HEAD"]).unwrap();
    let index = std::fs::read(repo.join(".git/index")).unwrap();
    app.dispatch(&key(KeyCode::Char('D')));
    let deadline = Instant::now() + Duration::from_secs(10);
    while app.log.preview_wakeup().is_some() {
        app.poll();
        assert!(Instant::now() < deadline, "workspace comparison timed out");
        std::thread::sleep(Duration::from_millis(1));
    }
    app.dispatch(&key(KeyCode::Enter));
    let text = screen(&mut app, 184, 36);
    assert!(
        text.lines().last().unwrap().contains("L Apply hunk"),
        "{text}"
    );
    app.dispatch(&key(KeyCode::Char('L')));
    assert_eq!(
        std::fs::read_to_string(repo.join("mixed.txt")).unwrap(),
        "base\n"
    );
    assert_eq!(
        std::fs::read_to_string(repo.join("other.txt")).unwrap(),
        "unselected staged\n"
    );
    assert_eq!(std::fs::read(repo.join(".git/index")).unwrap(), index);
    let deadline = Instant::now() + Duration::from_secs(10);
    while app.log.preview_wakeup().is_some() {
        app.poll();
        assert!(Instant::now() < deadline, "workspace refresh timed out");
        std::thread::sleep(Duration::from_millis(1));
    }
    app.dispatch(&key(KeyCode::Esc));
    assert!(!app.log.comparing_workspace());
    assert!(app
        .notifications
        .latest_body()
        .contains("Applied commit hunks"));
    assert_eq!(app.tab, Log);
    assert_eq!(std::fs::read(repo.join(".git/index")).unwrap(), index);
    assert_eq!(git(&repo, &["rev-parse", "HEAD"]).unwrap(), oid);
}
