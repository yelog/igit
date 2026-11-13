//! Prefix hints must follow the active panel without changing its selection or Git state.
use super::{
    test_support::{fixture as workspace_fixture, key, screen},
    *,
};
use crate::core::git;
use crossterm::event::MouseEvent;
use std::{fs, path::Path};

fn save(repo: &Path, message: &str) {
    git(repo, &["add", "--", "file"]).unwrap();
    git(
        repo,
        &[
            "-c",
            "user.name=Hint tester",
            "-c",
            "user.email=hints@example.invalid",
            "-c",
            "core.hooksPath=/dev/null",
            "commit",
            "--no-gpg-sign",
            "-qm",
            message,
        ],
    )
    .unwrap();
}

fn fixture() -> (tempfile::TempDir, Shell) {
    let repo = tempfile::tempdir().unwrap();
    git(repo.path(), &["init", "-q", "-b", "main"]).unwrap();
    git(repo.path(), &["config", "user.name", "Hint tester"]).unwrap();
    git(
        repo.path(),
        &["config", "user.email", "hints@example.invalid"],
    )
    .unwrap();
    for message in ["first revision", "second revision"] {
        fs::write(repo.path().join("file"), message).unwrap();
        save(repo.path(), message);
    }
    let mut ctx = Context::discover(repo.path()).unwrap();
    ctx.state_dir = repo.path().join("state");
    let mut app = Shell::new(ctx, IconMode::Ascii);
    app.go(Log);
    (repo, app)
}

fn show_hints(app: &mut Shell, width: u16, height: u16) -> String {
    assert!(app
        .key_picker
        .poll(Instant::now() + Duration::from_millis(200)));
    screen(app, width, height)
}

#[test]
fn notification_prefix_shares_g_with_commit_goto_without_moving_on_one_g() {
    let (_repo, mut app) = workspace_fixture();
    app.go(Commit);
    app.dispatch(&key(KeyCode::Char('j')));
    let moved = screen(&mut app, 120, 36);

    app.dispatch(&key(KeyCode::Char('g')));
    assert!(app.notification_prefix.is_some());
    assert_eq!(app.key_sequence(), Some(KeySequence::FirstItem));
    assert_eq!(screen(&mut app, 120, 36), moved);

    app.dispatch(&key(KeyCode::Char('n')));
    assert!(matches!(app.pop, Some(Popup::Notifications(_))));
    app.dispatch(&key(KeyCode::Esc));
    assert_eq!(screen(&mut app, 120, 36), moved);

    app.dispatch(&key(KeyCode::Char('g')));
    app.dispatch(&key(KeyCode::Char('g')));
    assert!(app.notification_prefix.is_none());
    assert!(app.key_sequence().is_none());
    let first = screen(&mut app, 120, 36);
    assert_ne!(first, moved);

    app.dispatch(&key(KeyCode::Char('G')));
    let last = screen(&mut app, 120, 36);
    assert_ne!(last, first);
}

#[test]
fn first_space_in_log_shows_filter_prefix_without_claiming_notifications() {
    let (_repo, mut app) = fixture();
    screen(&mut app, 120, 36);
    app.dispatch(&key(KeyCode::Char(' ')));
    assert!(app.notification_prefix.is_none());
    assert_eq!(app.key_sequence(), Some(KeySequence::LogFilters));
    let hints = screen(&mut app, 120, 36);
    assert!(hints.contains("Filters: b/u/d/p open"), "{hints}");
    assert!(!hints.contains("n Notifications"), "{hints}");
    let narrow = screen(&mut app, 44, 12);
    assert!(narrow.contains("b/u/d/p Filters"), "{narrow}");
    app.dispatch(&key(KeyCode::Char('n')));
    assert!(app.pop.is_none());
    assert!(app.key_sequence().is_none());
}

#[test]
fn first_space_highlights_log_filters_from_searchable_panes() {
    for pane in [
        crate::ui::panes::PaneId::Refs,
        crate::ui::panes::PaneId::Files,
    ] {
        let (_repo, mut app) = fixture();
        app.log.focus_pane(pane);
        screen(&mut app, 120, 36);
        app.dispatch(&key(KeyCode::Char(' ')));
        assert_eq!(
            app.key_sequence(),
            Some(KeySequence::LogFilters),
            "{pane:?}"
        );
        let hints = screen(&mut app, 120, 36);
        assert!(hints.contains("Filters: b/u/d/p open"), "{pane:?}: {hints}");
        app.dispatch(&key(KeyCode::Char('b')));
        assert_eq!(
            app.help_content().context(),
            "Log / Branch Filter",
            "{pane:?}"
        );
    }
}

#[test]
fn remapped_log_filter_uses_the_configured_space_sequence() {
    let (_repo, mut app) = fixture();
    app.config = AppConfig::from_toml("[keybindings.log]\npaths-filter=['Space x']").unwrap();
    screen(&mut app, 120, 36);
    app.dispatch(&key(KeyCode::Char(' ')));
    assert_eq!(app.key_sequence(), None);
    assert!(!screen(&mut app, 120, 36).contains("Filters: b/u/d/p"));
    app.dispatch(&key(KeyCode::Char('p')));
    assert!(!app.log.editing());
    app.dispatch(&key(KeyCode::Char(' ')));
    app.dispatch(&key(KeyCode::Char('x')));
    assert_eq!(app.help_content().context(), "Log / Paths Filter");
}

#[test]
fn push_commit_copy_shares_hints_help_and_cancellation() {
    let (repo, mut app) = fixture();
    git(
        repo.path(),
        &["remote", "add", "origin", repo.path().to_str().unwrap()],
    )
    .unwrap();
    app.go(Push);
    app.push.finish_loading(&app.ctx);
    let capture = clipboard::CopyCapture::start();
    app.dispatch(&key(KeyCode::Char('y')));
    assert!(app.key_sequence().is_none());
    app.dispatch(&key(KeyCode::Char('j')));
    let oid = git(repo.path(), &["rev-parse", "HEAD"]).unwrap();
    for (suffix, expected) in [
        ('y', oid.as_str()),
        ('m', "second revision"),
        ('a', "Hint tester <hints@example.invalid>"),
    ] {
        app.dispatch(&key(KeyCode::Char('y')));
        assert_eq!(app.key_sequence(), Some(KeySequence::CommitCopy));
        let hints = show_hints(&mut app, 120, 36);
        for label in [
            "Copy full commit ID",
            "Copy full commit message",
            "Copy author name and email",
        ] {
            assert!(hints.contains(label), "{hints}");
        }
        app.dispatch(&key(KeyCode::Char(suffix)));
        assert_eq!(capture.texts().last().map(String::as_str), Some(expected));
        assert!(app.key_sequence().is_none());
    }
    for interrupt in [
        key(KeyCode::Esc),
        key(KeyCode::F(1)),
        key(KeyCode::Tab),
        key(KeyCode::Char('/')),
        key(KeyCode::Char('o')),
        Event::Resize(80, 24),
        Event::FocusLost,
    ] {
        app.push.focus_pane(crate::ui::panes::PaneId::Outgoing);
        app.dispatch(&key(KeyCode::Char('y')));
        assert!(app.key_sequence().is_some());
        app.dispatch(&interrupt);
        assert!(app.key_sequence().is_none(), "{interrupt:?}");
        app.dispatch(&key(KeyCode::Esc));
    }
    assert_eq!(capture.texts().len(), 3);
    app.push.focus_pane(crate::ui::panes::PaneId::Outgoing);
    app.dispatch(&key(KeyCode::F(1)));
    app.dispatch(&Event::Paste("Copy full commit message".into()));
    app.dispatch(&key(KeyCode::Enter));
    assert_eq!(capture.texts().last().unwrap(), "second revision");
    app.dispatch(&key(KeyCode::Char('y')));
    show_hints(&mut app, 80, 24);
    app.dispatch(&key(KeyCode::Down));
    app.dispatch(&key(KeyCode::Enter));
    assert_eq!(capture.texts().last().unwrap(), "second revision");
    assert_eq!(capture.texts().len(), 5);
    assert!(app.pop.is_none());
}

#[test]
fn log_space_filters_take_priority_over_global_pull_and_keep_editor_keys() {
    use crate::ui::panes::PaneId;
    let (_dir, mut app) = fixture();
    screen(&mut app, 150, 40);
    let send = |app: &mut Shell, ch| app.dispatch(&key(KeyCode::Char(ch)));
    for (pane, suffix, context) in [
        (PaneId::Graph, 'b', "Log / Branch Filter"),
        (PaneId::Refs, 'u', "Log / User Filter"),
        (PaneId::Files, 'd', "Log / Date Filter"),
        (PaneId::Info, 'p', "Log / Paths Filter"),
    ] {
        app.log.focus_pane(pane);
        assert_eq!(app.log.focused_pane(), pane);
        send(&mut app, ' ');
        assert!(app.notification_prefix.is_none());
        send(&mut app, suffix);
        assert!(app.pop.is_none());
        assert_eq!(app.help_content().context(), context);
        app.dispatch(&key(KeyCode::Esc));
    }
    send(&mut app, ' ');
    app.flush_notification_prefix();
    let hints = screen(&mut app, 150, 40);
    for name in ["Branch", "User", "Date", "Paths"] {
        assert!(!hints.contains(&format!("Open {name} filter")));
    }
    assert!(app.key_picker.wakeup(Instant::now()).is_none());
    app.dispatch(&key(KeyCode::Esc));
    app.dispatch(&Event::Key(KeyEvent::new(
        KeyCode::Char('f'),
        KeyModifiers::CONTROL,
    )));
    assert!(app.pop.is_none());
    for ch in ['p', 'f'] {
        send(&mut app, ch);
        assert!(matches!(app.pop, Some(Popup::Update(_))));
        app.dispatch(&key(KeyCode::Esc));
    }
    app.log.focus_pane(PaneId::Refs);
    send(&mut app, '/');
    assert_eq!(app.help_content().context(), "Log / Branch Search");
    for ch in ['p', ' ', 'f'] {
        send(&mut app, ch);
    }
    assert!(app.pop.is_none());
    assert!(screen(&mut app, 150, 40).contains("p f"));
}

#[test]
fn log_space_filters_inline_navigation_and_cancellation_preserve_commit() {
    let (repo, mut app) = fixture();
    let head = git(repo.path(), &["rev-parse", "HEAD"]).unwrap();
    let selected_message = |app: &mut Shell| {
        app.log.focus_pane(crate::ui::panes::PaneId::Graph);
        app.log.event(&key(KeyCode::Char('y')), &app.ctx);
        match app.log.event(&key(KeyCode::Char('m')), &app.ctx) {
            Action::Copy { text, .. } => text,
            _ => panic!("Expected selected commit message"),
        }
    };
    let before = selected_message(&mut app);
    for (right, context) in [
        (0, "Log / Branch Filter"),
        (1, "Log / User Filter"),
        (2, "Log / Date Filter"),
        (3, "Log / Paths Filter"),
    ] {
        app.dispatch(&key(KeyCode::Char(' ')));
        app.flush_notification_prefix();
        screen(&mut app, 120, 36);
        for _ in 0..right {
            app.dispatch(&key(KeyCode::Right));
        }
        app.dispatch(&key(KeyCode::Enter));
        assert_eq!(app.help_content().context(), context);
        assert!(app.pop.is_none());
        app.dispatch(&key(KeyCode::Esc));
        assert_eq!(selected_message(&mut app), before);
    }
    for cancel in [
        key(KeyCode::Esc),
        Event::Key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)),
        Event::Resize(80, 24),
        Event::FocusLost,
        key(KeyCode::Tab),
        key(KeyCode::Char('o')),
    ] {
        app.dispatch(&key(KeyCode::Char(' ')));
        app.flush_notification_prefix();
        screen(&mut app, 120, 36);
        app.dispatch(&cancel);
        assert!(app.key_sequence().is_none());
        assert!(!screen(&mut app, 120, 36).contains("Filters: b/u/d/p"));
        app.dispatch(&key(KeyCode::Esc));
    }
    assert_eq!(selected_message(&mut app), before);
    assert_eq!(git(repo.path(), &["rev-parse", "HEAD"]).unwrap(), head);
    assert!(git(repo.path(), &["diff", "--cached"]).unwrap().is_empty());
    assert!(git(repo.path(), &["diff"]).unwrap().is_empty());
}

#[test]
fn log_space_filters_inline_focus_and_mouse_use_existing_labels() {
    use ratatui::{backend::TestBackend, style::Modifier};
    let (_repo, mut app) = fixture();
    for (width, height) in [(44, 12), (80, 24), (120, 36), (180, 36)] {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal.draw(|frame| app.draw(frame)).unwrap();
        let targets = app.log.filter_shortcut_targets().unwrap();
        let original = terminal.backend().buffer().clone();
        app.dispatch(&key(KeyCode::Char(' ')));
        assert!(app.notification_prefix.is_none());
        assert_eq!(app.key_sequence(), Some(KeySequence::LogFilters));
        terminal.draw(|frame| app.draw(frame)).unwrap();
        assert_eq!(app.log.filter_shortcut_targets().unwrap(), targets);
        for (index, rect) in targets.iter().enumerate() {
            let cell = &terminal.backend().buffer()[(rect.x, rect.y)];
            assert_eq!(cell.symbol(), ["b", "u", "d", "p"][index]);
            assert_eq!(cell.fg, crate::ui::ACCENT);
            assert!(cell
                .modifier
                .contains(Modifier::BOLD | Modifier::UNDERLINED));
            assert!(!terminal.backend().buffer()[(rect.x + 1, rect.y)]
                .modifier
                .contains(Modifier::UNDERLINED));
            assert_eq!(
                cell.bg,
                if index == 0 {
                    crate::ui::SEL
                } else {
                    original[(rect.x, rect.y)].bg
                }
            );
        }
        app.dispatch(&key(KeyCode::Right));
        terminal.draw(|frame| app.draw(frame)).unwrap();
        assert_eq!(
            terminal.backend().buffer()[(targets[1].x, targets[1].y)].bg,
            crate::ui::SEL
        );
        app.dispatch(&key(KeyCode::Left));
        app.dispatch(&key(KeyCode::Enter));
        assert_eq!(app.help_content().context(), "Log / Branch Filter");
        app.dispatch(&key(KeyCode::Esc));
        app.dispatch(&key(KeyCode::Char(' ')));
        assert!(app.notification_prefix.is_none());
        assert_eq!(app.key_sequence(), Some(KeySequence::LogFilters));
        terminal.draw(|frame| app.draw(frame)).unwrap();
        app.dispatch(&Event::Mouse(MouseEvent {
            kind: event::MouseEventKind::Down(event::MouseButton::Left),
            column: targets[2].x,
            row: targets[2].y,
            modifiers: KeyModifiers::NONE,
        }));
        assert_eq!(app.help_content().context(), "Log / Date Filter");
        assert!(app.key_sequence().is_none());
        app.dispatch(&key(KeyCode::Esc));
        terminal.draw(|frame| app.draw(frame)).unwrap();
        for rect in targets {
            assert!(!terminal.backend().buffer()[(rect.x, rect.y)]
                .modifier
                .contains(Modifier::UNDERLINED));
        }
    }
}

#[test]
fn log_space_filters_file_history_retains_delayed_overlay() {
    let (repo, mut app) = fixture();
    app.log
        .set_file_history(&app.ctx, &repo.path().join("file"))
        .unwrap();
    app.log.refresh(&app.ctx).unwrap();
    screen(&mut app, 120, 36);
    assert!(app.log.filter_shortcut_targets().is_none());
    app.dispatch(&key(KeyCode::Char(' ')));
    app.flush_notification_prefix();
    assert!(!screen(&mut app, 120, 36).contains("Open Paths filter"));
    let hints = show_hints(&mut app, 120, 36);
    for name in ["Branch", "User", "Date", "Paths"] {
        assert!(hints.contains(&format!("Open {name} filter")));
    }
    app.dispatch(&key(KeyCode::Char('b')));
    assert_eq!(app.help_content().context(), "Log / Branch Filter");
    assert!(app.pop.is_none());
}

#[test]
fn key_sequence_hints_wait_200ms_and_wake_without_more_input() {
    let (_repo, mut app) = fixture();
    app.dispatch(&key(KeyCode::Char('y')));
    let start = Instant::now();
    app.key_picker = KeySequencePicker::pending(start);
    assert!(!screen(&mut app, 80, 24).contains("Copy full commit message"));
    assert!(!app.key_picker.poll(start + Duration::from_millis(199)));
    assert_eq!(
        app.key_picker.wakeup(start + Duration::from_millis(199)),
        Some(Duration::from_millis(1))
    );
    assert!(!screen(&mut app, 80, 24).contains("Copy full commit message"));
    assert!(app.key_picker.poll(start + Duration::from_millis(200)));
    assert!(screen(&mut app, 80, 24).contains("Copy full commit message"));
    assert!(!app.key_picker.poll(start + Duration::from_millis(201)));

    app.dispatch(&key(KeyCode::Esc));
    app.dispatch(&key(KeyCode::Char('g')));
    app.key_picker = KeySequencePicker::pending(Instant::now() - Duration::from_millis(200));
    assert_eq!(app.poll_timeout(), Duration::ZERO);
    assert!(app.poll());
    assert!(screen(&mut app, 80, 24).contains("Go to first commit"));
}

#[test]
fn notification_chord_does_not_start_during_another_pending_prefix() {
    let (_repo, mut app) = fixture();
    screen(&mut app, 120, 36);
    app.dispatch(&Event::Key(KeyEvent::new(
        KeyCode::Char('w'),
        KeyModifiers::CONTROL,
    )));
    assert!(app.window_prefix.is_some());
    app.dispatch(&key(KeyCode::Char(' ')));
    assert!(app.notification_prefix.is_none());
}

#[test]
fn key_sequence_fast_completion_and_cancel_leave_no_delayed_popup() {
    let (_repo, mut app) = fixture();
    app.dispatch(&key(KeyCode::Char('G')));
    app.dispatch(&key(KeyCode::Char('g')));
    assert!(!screen(&mut app, 80, 24).contains("Go to first commit"));
    app.dispatch(&key(KeyCode::Char('g')));
    assert!(app.key_sequence().is_none());
    app.log.event(&key(KeyCode::Char('y')), &app.ctx);
    match app.log.event(&key(KeyCode::Char('m')), &app.ctx) {
        Action::Copy { text, .. } => assert_eq!(text.trim(), "second revision"),
        _ => panic!("gg must execute immediately, before the hint delay"),
    }
    for interrupt in [
        key(KeyCode::Esc),
        Event::Key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)),
        key(KeyCode::Down), // An invisible chooser must not capture navigation.
    ] {
        app.dispatch(&key(KeyCode::Char('y')));
        app.dispatch(&interrupt);
        assert!(app.key_sequence().is_none());
        assert!(app.key_picker.wakeup(Instant::now()).is_none());
        assert!(!app.key_picker.poll(Instant::now() + Duration::from_secs(1)));
        assert!(!screen(&mut app, 80, 24).contains("Copy full commit message"));
        assert!(app.pop.is_none());
    }
}

#[test]
fn key_sequence_changing_prefix_restarts_the_hint_delay() {
    let (_repo, mut app) = fixture();
    app.dispatch(&key(KeyCode::Char('g')));
    app.key_picker = KeySequencePicker::pending(Instant::now() - Duration::from_millis(200));
    app.dispatch(&key(KeyCode::Char('y')));
    assert!(app.key_picker.wakeup(Instant::now()).unwrap() > Duration::ZERO);
    assert!(!screen(&mut app, 80, 24).contains("Copy full commit message"));
    assert!(show_hints(&mut app, 80, 24).contains("Copy full commit message"));
}

#[test]
fn key_sequence_hints_list_only_the_pending_prefix_and_adapt_to_window_width() {
    let (_repo, mut app) = fixture();
    for (width, height) in [(44, 12), (80, 24), (120, 36)] {
        app.dispatch(&key(KeyCode::Char('y')));
        let text = show_hints(&mut app, width, height);
        for description in [
            "Copy full commit ID",
            "Copy full commit message",
            "Copy author name and email",
        ] {
            assert!(text.contains(description), "{width}x{height}: {text}");
        }
        assert!(!text.contains("Go to first commit"));
        app.dispatch(&key(KeyCode::Esc));
        assert!(!screen(&mut app, width, height).contains("Copy full commit message"));

        app.dispatch(&key(KeyCode::Char('g')));
        let text = show_hints(&mut app, width, height);
        assert!(text.contains("Go to first commit"), "{text}");
        assert!(!text.contains("Copy full commit message"));
        app.dispatch(&key(KeyCode::Char('g')));
        assert!(app.key_sequence().is_none());
    }
}

#[test]
fn key_sequence_enter_completes_the_sequence_and_cancel_does_not_open_another_action() {
    let (_repo, mut app) = fixture();
    let oid = |app: &mut Shell| {
        app.log.event(&key(KeyCode::Char('y')), &app.ctx);
        match app.log.event(&key(KeyCode::Char('y')), &app.ctx) {
            Action::Copy { text, .. } => text,
            _ => panic!("expected the selected commit ID"),
        }
    };
    let first = oid(&mut app);
    app.dispatch(&key(KeyCode::Char('G')));
    let last = oid(&mut app);
    assert_ne!(first, last);
    app.dispatch(&key(KeyCode::Char('g')));
    show_hints(&mut app, 120, 36);
    app.dispatch(&key(KeyCode::Down));
    app.dispatch(&key(KeyCode::Right));
    app.dispatch(&key(KeyCode::Enter));
    assert!(app.key_sequence().is_none());
    assert!(!app.log.editing()); // Enter completed gg rather than opening Diff.
    assert_eq!(oid(&mut app), first);

    for cancel in [
        key(KeyCode::Esc),
        Event::Key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)),
    ] {
        app.dispatch(&key(KeyCode::Char('y')));
        app.dispatch(&cancel);
        assert!(app.key_sequence().is_none());
        assert!(app.pop.is_none());
        assert!(!app.quit);
    }
}

#[test]
fn key_sequence_arrows_choose_a_suffix_without_moving_the_commit() {
    use crate::ui::key_sequence::SequenceEvent;
    let (_repo, mut app) = fixture();
    for (width, arrow) in [(44, KeyCode::Down), (120, KeyCode::Right)] {
        app.dispatch(&key(KeyCode::Char('y')));
        show_hints(&mut app, width, 24);
        app.dispatch(&key(arrow));
        let sequence = app.key_sequence().unwrap();
        assert!(matches!(
            app.key_picker.event(&key(KeyCode::Enter), sequence),
            SequenceEvent::Run('m')
        ));
        // Test the chosen operation without writing the user's real clipboard.
        match app.log.event(&key(KeyCode::Char('m')), &app.ctx) {
            Action::Copy { text, .. } => assert_eq!(text.trim(), "second revision"),
            _ => panic!("expected ym to copy the current commit message"),
        }
    }
}

#[test]
fn key_sequence_context_changes_dismiss_hints_and_text_input_keeps_its_letters() {
    let (_repo, mut app) = fixture();
    for interrupt in [
        key(KeyCode::Tab),
        key(KeyCode::Char('?')),
        key(KeyCode::F(1)),
        key(KeyCode::Char('1')),
        key(KeyCode::Char('j')),
        Event::Paste("ya".into()),
        Event::Resize(80, 24),
        Event::FocusLost,
    ] {
        app.go(Log);
        app.dispatch(&key(KeyCode::Esc)); // Each case starts in the commit list.
        app.log.focus_pane(crate::ui::panes::PaneId::Graph);
        app.dispatch(&key(KeyCode::Char('y')));
        assert!(app.key_sequence().is_some(), "{interrupt:?}");
        app.dispatch(&interrupt);
        assert!(app.key_sequence().is_none(), "{interrupt:?}");
        assert!(
            !screen(&mut app, 80, 24).contains("y  Arrows select"),
            "{interrupt:?} must dismiss the sequence overlay, even if Help lists the same action"
        );
        app.pop = None;
    }
    app.go(Log);
    app.dispatch(&key(KeyCode::Char('/')));
    for ch in ['g', 'y', '[', ']'] {
        app.dispatch(&key(KeyCode::Char(ch)));
    }
    let text = screen(&mut app, 120, 36);
    assert!(app.log.editing());
    assert!(text.contains("gy[]"), "{text}");
    assert!(app.key_sequence().is_none());
}

#[test]
fn search_ctrl_w_edits_queries_without_starting_window_shortcuts() {
    let (_repo, mut app) = fixture();
    for tab in [Commit, Log, Push, Branch, Stash] {
        app.go(tab);
        screen(&mut app, 120, 36);
        app.dispatch(&key(KeyCode::Char('/')));
        assert!(app.panel().editing(), "{tab:?}");
        app.dispatch(&Event::Paste("retain remove   ".into()));
        app.dispatch(&Event::Key(KeyEvent::new(
            KeyCode::Char('w'),
            KeyModifiers::CONTROL,
        )));
        let text = screen(&mut app, 120, 36);
        assert!(
            text.contains("retain") && !text.contains("remove"),
            "{tab:?}: {text}"
        );
        assert!(app.window_prefix.is_none());
        assert!(app.pending_keys.is_none());
        assert!(app.key_sequence().is_none());
        app.dispatch(&key(KeyCode::Char('x')));
        assert!(screen(&mut app, 120, 36).contains("retain x"));
        app.dispatch(&key(KeyCode::Esc));
    }
}

#[test]
fn key_sequence_overlay_blocks_mouse_events_and_outside_clicks_keep_their_target() {
    let (_repo, mut app) = fixture();
    app.dispatch(&key(KeyCode::Char('g')));
    show_hints(&mut app, 80, 24);
    // The bottom-right border is drawn over the Log content, but is not a command.
    for kind in [
        MouseEventKind::Down(MouseButton::Left),
        MouseEventKind::Up(MouseButton::Left),
        MouseEventKind::ScrollDown,
    ] {
        app.dispatch(&Event::Mouse(MouseEvent {
            kind,
            column: 78,
            row: app.regions.body.bottom() - 1,
            modifiers: KeyModifiers::NONE,
        }));
        assert!(app.key_sequence().is_some());
    }
    let commit = app.regions.tab(Commit);
    app.dispatch(&Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: commit.x,
        row: commit.y,
        modifiers: KeyModifiers::NONE,
    }));
    assert_eq!(app.tab, Commit);
    assert!(app.key_sequence().is_none());
}

#[test]
fn key_sequence_click_completes_goto_and_a_new_prefix_resets_the_choice() {
    use crate::ui::key_sequence::SequenceEvent;
    let (_repo, mut app) = fixture();
    app.dispatch(&key(KeyCode::Char('G')));
    app.dispatch(&key(KeyCode::Char('g')));
    show_hints(&mut app, 80, 24);
    app.dispatch(&Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 3,
        row: app.regions.body.bottom() - 2,
        modifiers: KeyModifiers::NONE,
    }));
    assert!(app.key_sequence().is_none());
    assert!(!app.log.editing());
    match app.log.event(&key(KeyCode::Char('y')), &app.ctx) {
        Action::None => (),
        _ => panic!("y must wait for a suffix"),
    }
    match app.log.event(&key(KeyCode::Char('m')), &app.ctx) {
        Action::Copy { text, .. } => assert_eq!(text.trim(), "second revision"),
        _ => panic!("clicking the g candidate must select the first commit"),
    }

    app.dispatch(&key(KeyCode::Char('y')));
    show_hints(&mut app, 120, 36);
    app.dispatch(&key(KeyCode::Right));
    app.dispatch(&key(KeyCode::Char('g'))); // A different valid prefix starts a fresh chooser.
    app.dispatch(&key(KeyCode::Esc));
    app.dispatch(&key(KeyCode::Char('y')));
    show_hints(&mut app, 120, 36);
    let sequence = app.key_sequence().unwrap();
    assert!(matches!(
        app.key_picker.event(&key(KeyCode::Enter), sequence),
        SequenceEvent::Run('y')
    ));
}

#[test]
fn key_sequence_merge_hints_cancel_without_leaving_the_document_or_changing_git() {
    let (repo, mut app) = fixture();
    let content = |side: &str| {
        format!(
            "first = {side}\n{}\nsecond = {side}\n",
            (0..12)
                .map(|n| format!("separator {n}\n"))
                .collect::<String>()
        )
    };
    fs::write(repo.path().join("file"), content("base")).unwrap();
    save(repo.path(), "merge base");
    git(repo.path(), &["checkout", "-qb", "topic"]).unwrap();
    fs::write(repo.path().join("file"), content("incoming")).unwrap();
    save(repo.path(), "incoming");
    git(repo.path(), &["checkout", "-q", "main"]).unwrap();
    fs::write(repo.path().join("file"), content("current")).unwrap();
    save(repo.path(), "current");
    assert!(git(repo.path(), &["merge", "--no-commit", "topic"]).is_err());
    assert!(!git(repo.path(), &["ls-files", "--unmerged"])
        .unwrap()
        .is_empty());
    let snapshot = || {
        [
            git(repo.path(), &["rev-parse", "HEAD"]).unwrap(),
            git(repo.path(), &["ls-files", "--stage"]).unwrap(),
            git(repo.path(), &["diff", "--binary"]).unwrap(),
            fs::read_to_string(repo.path().join("file")).unwrap(),
        ]
    };
    let before = snapshot();
    app.refresh_conflicts();
    app.go(Conflicts);
    app.dispatch(&key(KeyCode::Char('[')));
    assert!(app.key_sequence().is_none());
    app.dispatch(&key(KeyCode::Enter));
    for (prefix, description) in [
        ('[', "Previous unresolved conflict"),
        (']', "Next unresolved conflict"),
    ] {
        app.dispatch(&key(KeyCode::Char(prefix)));
        app.key_picker = KeySequencePicker::pending(Instant::now() - Duration::from_millis(200));
        app.dispatch(&key(KeyCode::Char(prefix))); // Repeating a prefix also restarts its delay.
        assert!(!screen(&mut app, 80, 24).contains(description));
        let text = show_hints(&mut app, 80, 24);
        assert!(text.contains(description), "{text}");
        assert!(text
            .lines()
            .rev()
            .nth(1)
            .unwrap()
            .contains("Save and Close"));
        assert!(text.lines().last().unwrap().contains("IGit  v"));
        app.dispatch(&key(KeyCode::Esc));
        assert!(app.key_sequence().is_none());
        assert!(screen(&mut app, 80, 24).contains("Merge Revisions for"));
        app.dispatch(&key(KeyCode::Char(prefix)));
        show_hints(&mut app, 80, 24);
        app.dispatch(&key(KeyCode::Enter));
        assert!(app.key_sequence().is_none());
        let text = screen(&mut app, 80, 24);
        // Conflict navigation wraps: previous from the first goes to the last.
        let selected = if prefix == '[' {
            "second = base"
        } else {
            "first = base"
        };
        assert!(text.contains(selected), "{text}");
    }
    app.dispatch(&key(KeyCode::Char('[')));
    app.dispatch(&key(KeyCode::Char('?')));
    app.dispatch(&key(KeyCode::Esc));
    assert!(app.key_sequence().is_none());
    assert_eq!(snapshot(), before);
}

#[test]
fn file_tree_copy_routes_suffixes_before_global_actions_and_preserves_log_yank() {
    use crate::ui::panes::PaneId;

    let (repo, mut app) = fixture();
    let capture = clipboard::CopyCapture::start();
    let send = |app: &mut Shell, ch| app.dispatch(&key(KeyCode::Char(ch)));
    let copy = |app: &mut Shell, suffix, expected: &str| {
        let before = capture.texts().len();
        send(app, 'y');
        send(app, suffix);
        assert_eq!(capture.texts().len(), before + 1);
        assert_eq!(capture.texts().last().map(String::as_str), Some(expected));
        assert!(app.pop.is_none());
        assert!(app.key_sequence().is_none());
    };

    let deadline = Instant::now() + Duration::from_secs(5);
    while app.log.preview_wakeup().is_some() && Instant::now() < deadline {
        app.poll();
        std::thread::sleep(Duration::from_millis(5));
    }
    app.log.focus_pane(PaneId::Files);
    copy(&mut app, 'r', "file");
    app.log.focus_pane(PaneId::Graph);
    let oid = git(repo.path(), &["rev-parse", "HEAD"]).unwrap();
    copy(&mut app, 'y', &oid);

    let remote = tempfile::tempdir().unwrap();
    git(remote.path(), &["init", "--bare", "-q"]).unwrap();
    git(
        repo.path(),
        &["remote", "add", "origin", remote.path().to_str().unwrap()],
    )
    .unwrap();
    git(repo.path(), &["push", "origin", "HEAD~1:refs/heads/main"]).unwrap();
    git(repo.path(), &["branch", "--set-upstream-to=origin/main"]).unwrap();
    app.go(Push);
    app.push.finish_loading(&app.ctx);
    app.push.focus_pane(PaneId::PushFiles);
    send(&mut app, 'j');
    copy(&mut app, 'r', "file");

    fs::create_dir(repo.path().join("src.v2")).unwrap();
    fs::write(repo.path().join("src.v2/报告.tar.gz"), "new file").unwrap();
    app.go(Commit);
    app.refresh();
    let screen_text = screen(&mut app, 150, 40);
    let (x, y) = screen_text
        .lines()
        .enumerate()
        .find_map(|(y, line)| {
            line.find("报告.tar.gz")
                .map(|x| (unicode_width::UnicodeWidthStr::width(&line[..x]), y))
        })
        .unwrap();
    app.dispatch(&Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: x as u16,
        row: y as u16,
        modifiers: KeyModifiers::NONE,
    }));
    let absolute = app.ctx.repos[0].path.join("src.v2/报告.tar.gz");
    copy(&mut app, 'y', absolute.to_str().unwrap());
    copy(&mut app, 'f', "报告.tar.gz");
    copy(&mut app, 'n', "报告.tar");
    copy(&mut app, 'r', "src.v2/报告.tar.gz");
    send(&mut app, 'h');
    copy(&mut app, 'n', "src.v2");
    copy(&mut app, 'r', "src.v2");

    git(
        repo.path(),
        &["stash", "push", "--include-untracked", "--", "src.v2"],
    )
    .unwrap();
    app.go(Stash);
    app.stash.focus_pane(PaneId::Files);
    let deadline = Instant::now() + Duration::from_secs(5);
    while app.stash.preview_wakeup().is_some() {
        app.poll();
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(5));
    }
    copy(&mut app, 'r', "src.v2/报告.tar.gz");
    send(&mut app, 'k');
    copy(&mut app, 'n', "src.v2");
}
