use super::{test_support::*, *};
use crate::core::git;
use std::{fs, thread};

fn settle(app: &mut Shell) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        app.poll();
        if !app.commit.refresh_pending()
            && !app.log_load.pending()
            && !app.log.history_pending()
            && !app.push.reads_pending()
            && !app.branch.reads_pending()
            && !app.stash.reads_pending()
            && !app.remote.reads_pending()
            && !app.merge.reads_pending()
            && !app.refresh_state.pending(app.tab)
            && !app.refresh_state.pending(Conflicts)
        {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "refresh timed out: {}",
            app.notifications.latest_body()
        );
        thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn runtime_f5_queues_changes_and_keeps_navigation_and_mixed_staging() {
    let (_directory, mut app) = fixture();
    let repo = app.ctx.repos[0].path.clone();
    let before = snapshot(&repo);
    let index = fs::read(repo.join(".git/index")).unwrap();
    app.enable_background_loading();
    fs::write(repo.join("queued-refresh.txt"), "new file\n").unwrap();
    app.dispatch(&key(KeyCode::F(5)));
    assert!(app.commit.refresh_pending());
    assert!(!screen(&mut app, 120, 36).contains("queued-refresh.txt"));
    assert!(app
        .notifications
        .latest_body()
        .contains("Refreshing workspace"));
    app.dispatch(&key(KeyCode::Char('6')));
    assert_eq!(app.tab, Remote);
    settle(&mut app);
    app.dispatch(&key(KeyCode::Char('1')));
    assert!(screen(&mut app, 120, 36).contains("queued-refresh.txt"));
    assert_eq!(fs::read(repo.join(".git/index")).unwrap(), index);
    let after = snapshot(&repo);
    assert_eq!(after[..3], before[..3]);
}

#[test]
fn focus_refresh_waits_for_review_and_reloads_cached_log_after_dismissal() {
    let (_directory, mut app) = fixture();
    app.go(Log);
    app.enable_background_loading();
    screen(&mut app, 120, 36);
    open_review(&mut app);
    git(
        &app.ctx.repos[0].path,
        &["commit", "--allow-empty", "-qm", "external-focus-commit"],
    )
    .unwrap();
    let before = snapshot(&app.ctx.repos[0].path);
    app.dispatch(&Event::FocusGained);
    app.poll();
    assert!(matches!(app.pop, Some(Popup::Confirm { .. })));
    assert!(app.refresh_state.pending(Log));
    assert!(!app.log.history_pending());
    app.dispatch(&key(KeyCode::Esc));
    settle(&mut app);
    assert!(screen(&mut app, 120, 36).contains("external-focus-commit"));
    assert_eq!(snapshot(&app.ctx.repos[0].path), before);
}

#[test]
fn returning_to_cached_log_reloads_external_history_without_f5() {
    let (_directory, mut app) = fixture();
    app.enable_background_loading();
    app.go(Log);
    settle(&mut app);
    assert!(screen(&mut app, 120, 36).contains("base"));
    git(
        &app.ctx.repos[0].path,
        &["commit", "--allow-empty", "-qm", "external-entry-commit"],
    )
    .unwrap();
    app.dispatch(&key(KeyCode::Char('1')));
    settle(&mut app);
    app.dispatch(&key(KeyCode::Char('2')));
    settle(&mut app);
    assert!(screen(&mut app, 120, 36).contains("external-entry-commit"));
}

#[test]
fn entering_dormant_log_consumes_the_queued_invalidation() {
    let (_directory, mut app) = fixture();
    app.log_load = startup::LogLoad::Dormant;
    app.refresh_state.request(Log);
    app.go(Log);
    assert!(!app.refresh_state.pending(Log));
    assert!(app.log_load.pending());
}

#[test]
fn remote_focus_refresh_survives_an_open_review() {
    let (_directory, mut app) = fixture();
    app.enable_background_loading();
    app.go(Remote);
    settle(&mut app);
    open_review(&mut app);
    git(
        &app.ctx.repos[0].path,
        &["remote", "add", "external-origin", "../unused.git"],
    )
    .unwrap();
    let before = snapshot(&app.ctx.repos[0].path);
    app.dispatch(&Event::FocusGained);
    app.poll();
    assert!(app.refresh_state.pending(Remote));
    assert!(matches!(app.pop, Some(Popup::Confirm { .. })));
    app.dispatch(&key(KeyCode::Esc));
    settle(&mut app);
    assert!(screen(&mut app, 120, 36).contains("external-origin"));
    assert_eq!(snapshot(&app.ctx.repos[0].path), before);
}

#[test]
fn opening_a_workspace_restores_runtime_background_loading() {
    let (_directory, mut app) = fixture();
    app.enable_background_loading();
    app.pop = Some(Popup::Open {
        path: app.ctx.workspace.display().to_string(),
    });
    app.dispatch(&key(KeyCode::Enter));
    assert!(app.pop.is_none());
    assert!(app.refresh_state.background);

    app.go(Remote);
    settle(&mut app);
    open_review(&mut app);
    git(
        &app.ctx.repos[0].path,
        &["remote", "add", "rescan-origin", "../unused.git"],
    )
    .unwrap();
    let before = snapshot(&app.ctx.repos[0].path);
    app.dispatch(&Event::FocusGained);
    app.poll();
    assert!(app.refresh_state.pending(Remote));
    assert!(matches!(app.pop, Some(Popup::Confirm { .. })));
    app.dispatch(&key(KeyCode::Esc));
    settle(&mut app);
    assert!(screen(&mut app, 120, 36).contains("rescan-origin"));
    assert_eq!(snapshot(&app.ctx.repos[0].path), before);
}

#[test]
fn terminal_focus_discovers_and_clears_external_conflicts() {
    let (_directory, mut app) = fixture();
    app.enable_background_loading();
    let repo = app.ctx.repos[0].path.clone();
    merge_conflict(&repo);
    app.dispatch(&Event::FocusGained);
    settle(&mut app);
    assert!(app.tab_available(Conflicts));
    app.go(Conflicts);
    settle(&mut app);
    assert_eq!(app.tab, Conflicts);
    git(&repo, &["merge", "--abort"]).unwrap();
    app.dispatch(&Event::FocusGained);
    settle(&mut app);
    assert!(!app.tab_available(Conflicts));
    assert_eq!(app.tab, Commit);
}

#[test]
fn push_focus_refresh_updates_outgoing_commits_without_changing_checks() {
    let (directory, mut app) = fixture();
    let repo = app.ctx.repos[0].path.clone();
    let remote = directory.path().join("remote.git");
    git(&repo, &["init", "--bare", "-q", remote.to_str().unwrap()]).unwrap();
    git(
        &repo,
        &["remote", "add", "origin", remote.to_str().unwrap()],
    )
    .unwrap();
    app.enable_background_loading();
    app.go(Push);
    settle(&mut app);
    app.dispatch(&key(KeyCode::Char(' ')));
    git(
        &repo,
        &["commit", "--allow-empty", "-qm", "external-push-commit"],
    )
    .unwrap();
    let before = snapshot(&repo);
    app.dispatch(&Event::FocusGained);
    settle(&mut app);
    let text = screen(&mut app, 140, 40);
    assert!(text.contains("external-push-commit"), "{text}");
    let repository = text
        .lines()
        .find(|line| line.contains("origin:main"))
        .unwrap();
    assert!(repository.contains(app.icons.check(false)), "{text}");
    assert_eq!(snapshot(&repo), before);
}
