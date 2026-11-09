use super::*;

fn wait_for(app: &mut Shell, label: &str) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let redraw = app.poll();
        if redraw && screen(app, 120, 36).contains(label) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "automatic refresh did not show {label}: {}",
            app.msg
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn commit_auto_refresh_runs_while_typing_and_resumes_after_dialog_closes() {
    let (_dir, mut app) = fixture();
    let repo = app.ctx.repos[0].path.clone();
    let index = git(&repo, &["ls-files", "--stage"]).unwrap();
    screen(&mut app, 120, 36);
    app.dispatch(&key(KeyCode::Char('i')));
    app.dispatch(&Event::Paste("draft keep word".into()));
    app.dispatch(&Event::Key(KeyEvent::new(
        KeyCode::Char('w'),
        KeyModifiers::CONTROL,
    )));
    assert!(app.key_sequence().is_none() && app.commit.message_editing());
    assert!(screen(&mut app, 120, 36).contains("draft keep "));
    app.dispatch(&Event::Key(KeyEvent::new(
        KeyCode::Char('u'),
        KeyModifiers::CONTROL,
    )));
    app.dispatch(&Event::Paste("saved draft".into()));
    fs::write(repo.join("automatic.txt"), "automatic\n").unwrap();
    // No focus event, manual refresh or request: exercise native file notifications.
    wait_for(&mut app, "automatic.txt");
    assert!(app.commit.message_editing());
    assert!(screen(&mut app, 120, 36).contains("saved draft"));
    app.dispatch(&key(KeyCode::F(2)));
    open_review(&mut app);
    assert!(matches!(&app.pop, Some(Popup::Confirm { token, .. }) if token == "test-review"));
    fs::write(repo.join("after-dialog.txt"), "after dialog\n").unwrap();
    app.dispatch(&Event::FocusGained);
    app.poll();
    assert!(matches!(&app.pop, Some(Popup::Confirm { token, .. }) if token == "test-review"));
    app.dispatch(&key(KeyCode::Esc));
    wait_for(&mut app, "after-dialog.txt");
    assert_eq!(git(&repo, &["ls-files", "--stage"]).unwrap(), index);
}

#[test]
fn returning_to_commit_and_terminal_focus_process_queued_changes() {
    let (_dir, mut app) = fixture();
    let repo = app.ctx.repos[0].path.clone();
    app.go(Log);
    fs::write(repo.join("return.txt"), "return\n").unwrap();
    app.go(Commit);
    wait_for(&mut app, "return.txt");
    fs::write(repo.join("focus.txt"), "focus\n").unwrap();
    app.dispatch(&Event::FocusGained);
    wait_for(&mut app, "focus.txt");
    assert_eq!(app.tab, Commit);
}

#[test]
fn automatically_discovered_conflicts_can_open_resolution_without_f5() {
    let (_dir, mut app) = fixture();
    let repo = app.ctx.repos[0].path.clone();
    merge_conflict(&repo);
    let before = snapshot(&repo);
    app.dispatch(&Event::FocusGained);
    wait_for(&mut app, "Merge Conflicts");
    app.dispatch(&key(KeyCode::Home));
    app.dispatch(&key(KeyCode::Down));
    app.dispatch(&key(KeyCode::Enter));
    assert_eq!(app.tab, Conflicts);
    assert!(screen(&mut app, 120, 36).contains("mixed.txt"));
    assert_eq!(snapshot(&repo), before);
}
