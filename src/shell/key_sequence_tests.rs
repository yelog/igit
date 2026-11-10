//! Prefix hints must follow the active panel without changing its selection or Git state.
use super::{
    test_support::{key, screen},
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
        assert_eq!(app.key_sequence(), Some(KeySequence::LogFilters));
        send(&mut app, suffix);
        assert!(app.pop.is_none());
        assert_eq!(app.help_content().context(), context);
        app.dispatch(&key(KeyCode::Esc));
    }
    send(&mut app, ' ');
    let hints = show_hints(&mut app, 150, 40);
    for name in ["Branch", "User", "Date", "Paths"] {
        assert!(hints.contains(&format!("Open {name} filter")));
    }
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
    assert_eq!(app.help_content().context(), "Pane Search");
    for ch in ['p', ' ', 'f'] {
        send(&mut app, ch);
    }
    assert!(app.pop.is_none());
    assert!(screen(&mut app, 150, 40).contains("p f"));
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
