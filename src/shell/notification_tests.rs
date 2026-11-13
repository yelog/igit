use super::{test_support::*, *};
use crossterm::event::MouseEvent;

fn mouse(kind: MouseEventKind, column: u16, row: u16) -> Event {
    Event::Mouse(MouseEvent {
        kind,
        column,
        row,
        modifiers: KeyModifiers::NONE,
    })
}

#[test]
fn notifications_keep_the_footer_and_complete_history_after_expiry() {
    let (_dir, mut app) = fixture();
    let before = snapshot(&app.ctx.repos[0].path);
    let text = "Changed file was not applied.\nReview its new content before retrying.";
    app.action(Action::Notice(NoticeLevel::Error, text.into()));
    let rendered = screen(&mut app, 120, 36);
    assert!(rendered.contains("ERROR Commit"));
    assert!(rendered.contains("Changed file was not applied."));
    let footer = rendered.lines().last().unwrap();
    assert!(footer.contains("g n Notices"));
    assert!(!footer.contains("Changed file"));
    app.go(Log);
    let entry = app.notifications.history().next().unwrap();
    assert_eq!(entry.source, "Commit");
    assert_eq!(entry.level, NoticeLevel::Error);
    assert_eq!(entry.body, text);
    assert!(app
        .notifications
        .expire(Instant::now() + Duration::from_secs(9)));
    assert!(app.notifications.live().is_empty());
    app.dispatch(&key(KeyCode::Char('g')));
    assert!(app.notification_prefix.is_some());
    assert_eq!(app.key_sequence(), Some(KeySequence::LogGoto));
    app.dispatch(&key(KeyCode::Char('n')));
    assert!(matches!(app.pop, Some(Popup::Notifications(_))));
    assert!(screen(&mut app, 120, 36).contains("NOTIFICATION HISTORY"));
    app.dispatch(&key(KeyCode::Enter));
    assert!(screen(&mut app, 120, 36).contains("Review its new content"));
    assert_eq!(snapshot(&app.ctx.repos[0].path), before);
}

#[test]
fn g_n_opens_history_from_every_normal_workspace_panel() {
    let (_dir, mut app) = fixture();
    for tab in [Commit, Log, Push, Branch, Stash, Remote] {
        app.go(tab);
        app.dispatch(&key(KeyCode::Char('g')));
        assert!(app.notification_prefix.is_some(), "{tab:?}");
        assert!(app.pop.is_none(), "{tab:?}");
        app.dispatch(&key(KeyCode::Char('n')));
        assert!(matches!(app.pop, Some(Popup::Notifications(_))), "{tab:?}");
        app.dispatch(&key(KeyCode::Esc));
    }
    app.go(Commit);
    app.dispatch(&key(KeyCode::Char('g')));
    app.dispatch(&key(KeyCode::Char('N')));
    assert!(app.pop.is_none());
}

#[test]
fn g_n_does_not_override_review_dialog_input() {
    let (_dir, mut app) = fixture();
    open_review(&mut app);
    app.dispatch(&key(KeyCode::Char('g')));
    assert!(matches!(app.pop, Some(Popup::Confirm { .. })));
    app.dispatch(&key(KeyCode::Char('n')));
    assert!(app.pop.is_none());
}

#[test]
fn notification_prefix_waits_indefinitely_and_space_toggles_immediately() {
    let (_dir, mut app) = fixture();
    let before = screen(&mut app, 120, 36);
    assert!(screen(&mut app, 120, 36).contains("g n Notices"));
    app.dispatch(&key(KeyCode::Char('g')));
    assert!(app.notification_prefix.is_some());
    assert!(app.pop.is_none());
    assert!(!screen(&mut app, 120, 36).contains("n Notifications"));
    assert!(app.poll_timeout() <= app.config.hint_delay);
    app.notification_prefix.as_mut().unwrap().started = Instant::now() - app.config.hint_delay;
    assert!(app.poll());
    assert!(screen(&mut app, 120, 36).contains("n Notifications"));
    app.notification_prefix.as_mut().unwrap().started = Instant::now() - Duration::from_secs(30);
    app.poll();
    assert!(app.notification_prefix.is_some());
    assert!(app.pop.is_none());
    app.dispatch(&key(KeyCode::Char('n')));
    assert!(app.notification_prefix.is_none());
    assert!(matches!(app.pop, Some(Popup::Notifications(_))));
    app.dispatch(&key(KeyCode::Esc));

    app.dispatch(&key(KeyCode::Char(' ')));
    assert!(screen(&mut app, 120, 36).contains("0 files selected"));
    assert!(app.notification_prefix.is_none());
    app.dispatch(&key(KeyCode::Char(' ')));
    assert!(app.notification_prefix.is_none());
    assert!(app.pop.is_none());
    assert_eq!(screen(&mut app, 120, 36), before);
    app.dispatch(&key(KeyCode::Char('n')));
    assert!(app.pop.is_none());
}

#[test]
fn g_n_opens_history_in_an_empty_workspace_and_escape_cancels_the_chord() {
    let (_dir, mut app) = fixture();
    let empty = tempfile::tempdir().unwrap();
    app.pop = Some(Popup::Open {
        path: empty.path().display().to_string(),
    });
    app.dispatch(&key(KeyCode::Enter));
    assert!(app.ctx.repos.is_empty());
    app.dispatch(&key(KeyCode::Char('g')));
    assert!(app.notification_prefix.is_some());
    app.dispatch(&key(KeyCode::Esc));
    assert!(app.notification_prefix.is_none());
    assert!(app.pop.is_none());
    app.dispatch(&key(KeyCode::Char('g')));
    assert!(app.notification_prefix.is_some());
    app.dispatch(&key(KeyCode::Char('n')));
    assert!(matches!(app.pop, Some(Popup::Notifications(_))));
}

#[test]
fn notification_space_prefix_uses_custom_suffixes_with_other_bindings_enabled() {
    let (_dir, mut app) = fixture();
    app.config = AppConfig::from_toml(
        "[keybindings.global]\nnotification-history=['Space H']\nquit=['Q Q']",
    )
    .unwrap();
    app.dispatch(&key(KeyCode::Char(' ')));
    assert!(app.notification_prefix_hint().is_none());
    app.notification_prefix.as_mut().unwrap().started = Instant::now() - app.config.hint_delay;
    assert!(app.poll());
    assert_eq!(
        app.notification_prefix_hint().as_deref(),
        Some("H Notifications  Esc cancel")
    );
    app.dispatch(&key(KeyCode::Char('n')));
    assert!(app.pop.is_none());
    app.dispatch(&key(KeyCode::Char(' ')));
    app.dispatch(&key(KeyCode::Char('H')));
    assert!(matches!(app.pop, Some(Popup::Notifications(_))));
    app.dispatch(&key(KeyCode::Esc));
    let before = screen(&mut app, 120, 36);
    app.dispatch(&key(KeyCode::Char(' ')));
    assert_ne!(screen(&mut app, 120, 36), before);
    app.dispatch(&key(KeyCode::Char(' ')));
    assert!(app.notification_prefix.is_none());
    assert_eq!(screen(&mut app, 120, 36), before);
}

#[test]
fn notification_prefix_honors_custom_hint_delay_and_multi_step_suffixes() {
    let (_dir, mut app) = fixture();
    app.config = AppConfig::from_toml(
        "[keybindings]\nhint_delay_ms=900\n[keybindings.global]\nnotification-history=['Space H x']\nquit=['Q Q']",
    )
    .unwrap();
    app.dispatch(&key(KeyCode::Char(' ')));
    assert!(app.notification_prefix_hint().is_none());
    app.notification_prefix.as_mut().unwrap().started = Instant::now() - app.config.hint_delay;
    assert!(app.poll());
    assert_eq!(
        app.notification_prefix_hint().as_deref(),
        Some("H x Notifications  Esc cancel")
    );
    app.notification_prefix.as_mut().unwrap().started = Instant::now() - Duration::from_secs(30);
    app.dispatch(&key(KeyCode::Char('H')));
    let pending = app.notification_prefix.as_ref().unwrap();
    assert!(!pending.hint_shown);
    assert!(pending.started.elapsed() < app.config.hint_delay);
    assert!(app.notification_prefix_hint().is_none());
    assert!(app.poll_timeout() <= app.config.hint_delay);
    app.notification_prefix.as_mut().unwrap().started = Instant::now() - app.config.hint_delay;
    assert!(app.poll());
    assert_eq!(
        app.notification_prefix_hint().as_deref(),
        Some("x Notifications  Esc cancel")
    );
    app.dispatch(&key(KeyCode::Char('x')));
    assert!(matches!(app.pop, Some(Popup::Notifications(_))));
}

#[test]
fn notification_space_suffix_does_not_claim_second_space_replays_standalone_action() {
    let (_dir, mut app) = fixture();
    app.config = AppConfig::from_toml(
        "[keybindings.global]\nnotification-history=['Space Space']\nquit=['Q Q']",
    )
    .unwrap();
    app.dispatch(&key(KeyCode::Char(' ')));
    app.notification_prefix.as_mut().unwrap().started = Instant::now() - app.config.hint_delay;
    assert!(app.poll());
    assert_eq!(
        app.notification_prefix_hint().as_deref(),
        Some("Space Notifications  Esc cancel")
    );
    app.dispatch(&key(KeyCode::Char(' ')));
    assert!(app.notification_prefix.is_none());
    assert!(matches!(app.pop, Some(Popup::Notifications(_))));
}

#[test]
fn notification_prefix_replays_standalone_space_through_log_filter_routing() {
    let (_dir, mut app) = fixture();
    app.config = AppConfig::from_toml(
        "[keybindings.global]\nnotification-history=['Space H']\nquit=['Q Q']",
    )
    .unwrap();
    app.go(Log);
    screen(&mut app, 120, 36);
    app.dispatch(&key(KeyCode::Char(' ')));
    app.dispatch(&key(KeyCode::Char('b')));
    assert!(app.notification_prefix.is_none());
    assert!(app.pending_keys.is_none());
    assert_eq!(app.help_content().context(), "Log / Branch Filter");
    assert!(app.pop.is_none());
}

#[test]
fn notification_chord_does_not_toggle_commit_checks_or_log_cherry_picks() {
    let (_dir, mut app) = fixture();
    let before = screen(&mut app, 120, 36);
    assert!(before.contains("2 files selected"), "{before}");
    app.dispatch(&key(KeyCode::Char('g')));
    assert!(screen(&mut app, 120, 36).contains("2 files selected"));
    app.dispatch(&key(KeyCode::Char('n')));
    assert!(matches!(app.pop, Some(Popup::Notifications(_))));
    app.dispatch(&key(KeyCode::Esc));
    assert!(screen(&mut app, 120, 36).contains("2 files selected"));

    app.go(Log);
    screen(&mut app, 120, 36);
    app.dispatch(&key(KeyCode::Char('v')));
    app.dispatch(&key(KeyCode::Char('g')));
    app.dispatch(&key(KeyCode::Char('n')));
    assert!(matches!(app.pop, Some(Popup::Notifications(_))));
    app.dispatch(&key(KeyCode::Esc));
    app.dispatch(&key(KeyCode::Char(' ')));
    assert!(screen(&mut app, 120, 36).contains("Pick 1"));
}

#[test]
fn commit_space_toggles_immediately_without_replaying_on_the_next_motion() {
    let (_dir, mut app) = fixture();
    screen(&mut app, 120, 36);
    app.dispatch(&key(KeyCode::Char(' ')));
    assert!(screen(&mut app, 120, 36).contains("0 files selected"));
    assert!(app.notification_prefix.is_none());
    app.dispatch(&key(KeyCode::Char('j')));
    assert!(app.notification_prefix.is_none());
    let moved = screen(&mut app, 120, 36);
    assert!(moved.contains("0 files selected"), "{moved}");
    app.dispatch(&key(KeyCode::Char('u')));
    assert!(screen(&mut app, 120, 36).contains("2 files selected"));
}

#[test]
fn background_messages_keep_origin_without_interrupting_input_or_review() {
    let (_dir, mut app) = fixture();
    screen(&mut app, 120, 36);
    app.dispatch(&key(KeyCode::Char('i')));
    app.dispatch(&Event::Paste("draft 中文".into()));
    let focused = app.commit.focused_pane();
    app.action_from(
        "Push",
        Action::Notice(NoticeLevel::Warning, "One remote failed".into()),
    );
    assert!(app.commit.message_editing());
    assert_eq!(app.commit.focused_pane(), focused);
    app.dispatch(&key(KeyCode::Char(' ')));
    app.dispatch(&key(KeyCode::Char('N')));
    assert!(app.pop.is_none());
    assert!(app.commit.message_editing());
    app.dispatch(&key(KeyCode::Esc));
    assert!(screen(&mut app, 120, 36).contains("draft 中文"));
    open_review(&mut app);
    app.action_from(
        "Commit",
        Action::Notice(NoticeLevel::Error, "AI request failed".into()),
    );
    assert!(matches!(&app.pop, Some(Popup::Confirm { token, .. }) if token == "test-review"));
    app.dispatch(&key(KeyCode::Char(' ')));
    app.dispatch(&key(KeyCode::Char('N')));
    assert!(matches!(app.pop, Some(Popup::Confirm { .. })));
    let messages: Vec<_> = app.notifications.history().collect();
    assert_eq!(messages[0].source, "Commit");
    assert_eq!(messages[1].source, "Push");
}

#[test]
fn notification_shortcuts_are_configurable_and_actions_open_the_same_history() {
    let (_dir, mut app) = fixture();
    app.go(Log);
    app.dispatch(&key(KeyCode::Char('g')));
    app.dispatch(&key(KeyCode::Char('n')));
    assert!(matches!(app.pop, Some(Popup::Notifications(_))));
    app.dispatch(&key(KeyCode::Esc));
    app.dispatch(&key(KeyCode::F(8)));
    assert!(app.pop.is_none());
    app.config =
        AppConfig::from_toml("[keybindings.global]\nnotification-history=['F10']").unwrap();
    app.dispatch(&key(KeyCode::Char('g')));
    app.dispatch(&key(KeyCode::Char('n')));
    assert!(app.pop.is_none() && !app.log.editing());
    app.dispatch(&key(KeyCode::F(10)));
    assert!(matches!(app.pop, Some(Popup::Notifications(_))));
    app.dispatch(&key(KeyCode::Esc));
    let row = app
        .menu_actions()
        .position(|action| matches!(action, MenuAction::Notifications))
        .unwrap();
    app.menu_action(row);
    assert!(matches!(app.pop, Some(Popup::Notifications(_))));
    app.dispatch(&key(KeyCode::F(1)));
    assert!(matches!(app.pop, Some(Popup::Help { .. })));
    app.dispatch(&key(KeyCode::Esc));
    assert!(matches!(app.pop, Some(Popup::Notifications(_))));
}

#[test]
fn card_mouse_events_do_not_activate_underlying_controls_or_replace_reviews() {
    let (_dir, mut app) = fixture();
    let before = snapshot(&app.ctx.repos[0].path);
    for (width, height) in [(120, 36), (44, 12)] {
        app.notifications.dismiss_all_live();
        app.notify(
            NoticeLevel::Error,
            "Commit",
            "Preserve the reviewed operation".into(),
        );
        open_review(&mut app);
        screen(&mut app, width, height);
        app.dispatch(&mouse(
            MouseEventKind::Down(MouseButton::Left),
            width - 8,
            3,
        ));
        app.dispatch(&mouse(
            MouseEventKind::Drag(MouseButton::Left),
            1,
            height - 2,
        ));
        app.dispatch(&mouse(MouseEventKind::Up(MouseButton::Left), 1, height - 2));
        assert!(matches!(&app.pop, Some(Popup::Confirm { token, .. }) if token == "test-review"));
        app.dispatch(&mouse(MouseEventKind::ScrollDown, width - 8, 3));
        assert!(matches!(&app.pop, Some(Popup::Confirm { scroll: 0, .. })));
        app.dispatch(&mouse(
            MouseEventKind::Down(MouseButton::Left),
            width - 3,
            2,
        ));
        app.dispatch(&mouse(MouseEventKind::Up(MouseButton::Left), width - 3, 2));
        assert!(app.notifications.live().is_empty());
        assert!(matches!(app.pop, Some(Popup::Confirm { .. })));
        app.pop = None;
    }
    assert_eq!(snapshot(&app.ctx.repos[0].path), before);
}

#[test]
fn card_opens_its_message_and_history_copies_full_text_on_narrow_terminals() {
    let (_dir, mut app) = fixture();
    let capture = clipboard::CopyCapture::start();
    let body = format!("Long error 中文\n{}\nFinal line", "diagnostic\n".repeat(40));
    app.notify(NoticeLevel::Error, "Commit", body.clone());
    screen(&mut app, 44, 12);
    app.dispatch(&mouse(MouseEventKind::Down(MouseButton::Left), 40, 3));
    app.dispatch(&mouse(MouseEventKind::Up(MouseButton::Left), 40, 3));
    assert!(matches!(app.pop, Some(Popup::Notifications(_))));
    app.dispatch(&key(KeyCode::Enter));
    assert!(screen(&mut app, 44, 12).contains("NOTIFICATION DETAIL"));
    app.dispatch(&key(KeyCode::End));
    assert!(screen(&mut app, 44, 12).contains("Final line"));
    app.dispatch(&key(KeyCode::Char('y')));
    assert_eq!(capture.texts(), vec![body]);
    assert!(matches!(app.pop, Some(Popup::Notifications(_))));
    app.dispatch(&key(KeyCode::Esc));
    app.dispatch(&key(KeyCode::Char('c')));
    app.dispatch(&key(KeyCode::Char('n')));
    assert_eq!(app.notifications.history().count(), 2);
    app.dispatch(&key(KeyCode::Char('c')));
    app.dispatch(&key(KeyCode::Char('y')));
    assert_eq!(app.notifications.history().count(), 0);
    assert!(app.notifications.live().is_empty());
}

#[test]
fn workspace_rescan_retains_notifications() {
    let (_dir, mut app) = fixture();
    app.notify(
        NoticeLevel::Success,
        "Clipboard",
        "Copied before rescan".into(),
    );
    let id = app.notifications.history().next().unwrap().id;
    app.pop = Some(Popup::Open {
        path: app.ctx.repos[0].path.to_string_lossy().into_owned(),
    });
    app.dispatch(&key(KeyCode::Enter));
    assert!(app.pop.is_none());
    assert!(app
        .notifications
        .history()
        .any(|entry| entry.id == id && entry.body == "Copied before rescan"));
    assert_eq!(app.notifications.latest_body(), "Workspace opened");
}

#[test]
fn notifications_leave_the_search_footer_available() {
    let (_dir, mut app) = fixture();
    screen(&mut app, 120, 36);
    app.dispatch(&key(KeyCode::Char('/')));
    app.dispatch(&Event::Paste("mixed".into()));
    app.action_from(
        "Push",
        Action::Notice(NoticeLevel::Success, "Push complete".into()),
    );
    let text = screen(&mut app, 120, 36);
    assert!(text.lines().last().unwrap().contains("mixed"));
    assert!(app.focused_search().unwrap().editing());
    assert!(text.contains("SUCCESS Push"));
}

#[test]
fn a_full_notification_stack_keeps_the_bottom_status_line_visible() {
    let (_dir, mut app) = fixture();
    for (width, height) in [(120, 36), (80, 24), (44, 12)] {
        app.notifications.dismiss_all_live();
        let initial = screen(&mut app, width, height);
        let footer = initial.lines().last().unwrap();
        for _ in 0..4 {
            app.notify(NoticeLevel::Error, "Commit", "Long diagnostic\n".repeat(12));
        }
        let text = screen(&mut app, width, height);
        assert_eq!(text.lines().last().unwrap(), footer);
    }
}

#[test]
fn notification_cards_take_mouse_priority_over_configured_key_hints() {
    let (_dir, mut app) = fixture();
    app.config = AppConfig::from_toml("[keybindings.global]\nquit=['Q Q']").unwrap();
    app.dispatch(&key(KeyCode::Char('Q')));
    app.key_picker.poll(Instant::now() + Duration::from_secs(1));
    let hints = screen(&mut app, 44, 12);
    let (row, line) = hints
        .lines()
        .enumerate()
        .find(|(_, line)| line.contains("Quit IGit"))
        .unwrap();
    let column = line[..line.find("Quit IGit").unwrap()].width() as u16;
    assert!(app.key_picker.contains(column, row as u16));
    for _ in 0..4 {
        app.notify(
            NoticeLevel::Error,
            "Commit",
            "Operation failed; review its details.".into(),
        );
    }
    screen(&mut app, 44, 12);
    assert!(app.notifications.contains(column, row as u16));
    app.dispatch(&mouse(
        MouseEventKind::Down(MouseButton::Left),
        column,
        row as u16,
    ));
    assert!(!app.quit);
    assert!(matches!(app.pop, Some(Popup::Notifications(_))));
    assert!(app.pending_keys.is_none());
}

#[test]
fn releasing_a_file_drag_over_a_notification_allows_automatic_refresh() {
    let (_dir, mut app) = fixture();
    let capture = clipboard::CopyCapture::start();
    let repo = app.ctx.repos[0].path.clone();
    let before = snapshot(&repo);
    let rendered = screen(&mut app, 120, 36);
    let (column, row) = rendered
        .lines()
        .enumerate()
        .find_map(|(row, line)| {
            line.find("mixed.txt")
                .map(|position| (line[..position].width() as u16, row as u16))
        })
        .unwrap();
    app.dispatch(&mouse(MouseEventKind::Down(MouseButton::Left), column, row));
    app.dispatch(&mouse(
        MouseEventKind::Drag(MouseButton::Left),
        column + 1,
        row,
    ));
    app.notify(
        NoticeLevel::Info,
        "Workspace",
        "Background operation finished".into(),
    );
    screen(&mut app, 120, 36);
    assert!(app.notifications.contains(112, 3));
    app.dispatch(&mouse(MouseEventKind::Up(MouseButton::Left), 112, 3));
    assert!(app.pop.is_none());
    assert_eq!(snapshot(&repo), before);
    assert!(capture.texts().is_empty());
    std::fs::write(repo.join("after-notification.txt"), "new content\n").unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        app.poll();
        if screen(&mut app, 120, 36).contains("after-notification.txt") {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "notification left a pending file drag"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(
        crate::core::git(&repo, &["ls-files", "--stage"]).unwrap(),
        before[1]
    );
}

#[test]
fn notification_help_cannot_replay_copy_as_clear_confirmation() {
    let (_dir, mut app) = fixture();
    let capture = clipboard::CopyCapture::start();
    app.notify(
        NoticeLevel::Info,
        "Workspace",
        "Keep this complete message".into(),
    );
    app.go(Log);
    app.dispatch(&key(KeyCode::Char('g')));
    app.dispatch(&key(KeyCode::Char('n')));
    app.dispatch(&key(KeyCode::Char('c')));
    app.dispatch(&key(KeyCode::F(1)));
    app.dispatch(&Event::Paste("Copy complete notification".into()));
    app.dispatch(&key(KeyCode::Enter));
    assert!(matches!(app.pop, Some(Popup::Help { .. })));
    assert_eq!(app.notifications.history().count(), 1);
    assert!(capture.texts().is_empty());
    app.dispatch(&key(KeyCode::Esc));
    app.dispatch(&key(KeyCode::Char('n')));
    app.dispatch(&key(KeyCode::F(1)));
    app.dispatch(&Event::Paste("Copy complete notification".into()));
    app.dispatch(&key(KeyCode::Enter));
    assert!(matches!(app.pop, Some(Popup::Notifications(_))));
    assert_eq!(capture.texts(), ["Keep this complete message"]);
}

#[test]
fn resizing_help_below_minimum_discards_hidden_history_mouse_targets() {
    let (_dir, mut app) = fixture();
    let capture = clipboard::CopyCapture::start();
    app.notify(NoticeLevel::Info, "Older", "Older notification".into());
    app.notify(
        NoticeLevel::Info,
        "Workspace",
        "Selected notification".into(),
    );
    app.go(Log);
    app.dispatch(&key(KeyCode::Char('g')));
    app.dispatch(&key(KeyCode::Char('n')));
    let rendered = screen(&mut app, 80, 24);
    let (column, row) = rendered
        .lines()
        .enumerate()
        .find_map(|(row, line)| {
            line.find("Older")
                .map(|position| (line[..position].width() as u16, row as u16))
        })
        .unwrap();
    app.dispatch(&key(KeyCode::F(1)));
    app.dispatch(&Event::Resize(43, 11));
    assert!(screen(&mut app, 43, 11).contains("Terminal too small"));
    app.dispatch(&key(KeyCode::Esc));
    app.dispatch(&mouse(MouseEventKind::Down(MouseButton::Left), column, row));
    app.dispatch(&mouse(MouseEventKind::Up(MouseButton::Left), column, row));
    app.dispatch(&Event::Resize(80, 24));
    assert!(screen(&mut app, 80, 24).contains("NOTIFICATION HISTORY"));
    app.dispatch(&key(KeyCode::Char('y')));
    assert_eq!(capture.texts(), ["Selected notification"]);
}

#[test]
fn closing_a_notification_preserves_a_reviewed_merge_continue() {
    let (_dir, mut app) = fixture();
    let repo = app.ctx.repos[0].path.clone();
    merge_conflict(&repo);
    app.dispatch(&key(KeyCode::F(5)));
    app.go(Conflicts);
    screen(&mut app, 120, 36);
    for character in ['Y', 'A', 'y'] {
        app.dispatch(&key(KeyCode::Char(character)));
    }
    assert!(crate::core::git(&repo, &["ls-files", "--unmerged"])
        .unwrap()
        .is_empty());
    app.dispatch(&key(KeyCode::Char('C')));
    let before = snapshot(&repo);
    assert!(matches!(app.pop, Some(Popup::Confirm { .. })));
    app.notifications.dismiss_all_live();
    app.notify(
        NoticeLevel::Info,
        "Workspace",
        "Review is still pending".into(),
    );
    screen(&mut app, 120, 36);
    app.dispatch(&mouse(MouseEventKind::Down(MouseButton::Left), 117, 2));
    app.dispatch(&mouse(MouseEventKind::Up(MouseButton::Left), 117, 2));
    assert!(app.notifications.live().is_empty());
    assert!(matches!(app.pop, Some(Popup::Confirm { .. })));
    assert_eq!(snapshot(&repo), before);
    app.dispatch(&key(KeyCode::Char('y')));
    assert_ne!(
        crate::core::git(&repo, &["rev-parse", "HEAD"]).unwrap(),
        before[0]
    );
    assert!(!repo.join(".git/MERGE_HEAD").exists());
}
