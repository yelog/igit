use super::{test_support::*, *};
use crate::core::git;

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
