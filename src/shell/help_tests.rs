use super::{test_support::*, *};
use crate::core::git;
use crossterm::event::{KeyEvent, MouseEvent};
use ratatui::backend::{Backend, TestBackend};
use std::fs;

fn context(app: &Shell) -> &'static str {
    match &app.pop {
        Some(Popup::Help { state, .. }) => state.content.context(),
        _ => panic!("expected contextual help"),
    }
}

fn check_context(app: &mut Shell, expected: &str) {
    app.dispatch(&key(KeyCode::F(1)));
    assert_eq!(context(app), expected);
    if let Some(Popup::Help { state, .. }) = &app.pop {
        for section in &state.content.sections {
            for (sequence, _) in section.bindings {
                assert!(
                    crate::ui::help::HelpCommand {
                        action: CommandAction::Keys(sequence)
                    }
                    .events()
                    .is_some(),
                    "non-executable shortcut: {sequence}"
                );
            }
        }
    }
    let text = screen(app, 120, 36);
    assert!(
        text.contains(&format!("KEYMAP // {}", expected.to_uppercase())),
        "{text}"
    );
    app.dispatch(&key(KeyCode::Esc));
    assert!(app.pop.is_none());
}

#[test]
fn text_selection_mouse_release_copies_without_shortcut_and_keeps_message_caret() {
    let (_dir, mut app) = fixture();
    let capture = clipboard::CopyCapture::start();
    let before = snapshot(&app.ctx.repos[0].path);
    app.go(Commit);
    screen(&mut app, 120, 36);
    app.dispatch(&key(KeyCode::Char('i')));
    let first = "auto ＵＩ copy";
    let message = format!("{first}\nbody");
    app.dispatch(&Event::Paste(message.clone()));
    let text = screen(&mut app, 120, 36);
    let (x, y) = text
        .lines()
        .enumerate()
        .find_map(|(y, line)| {
            line.find(first)
                .map(|i| (line[..i].width() as u16, y as u16))
        })
        .unwrap();
    let mouse = |kind, column| {
        Event::Mouse(MouseEvent {
            kind,
            column,
            row: y,
            modifiers: KeyModifiers::NONE,
        })
    };
    app.dispatch(&mouse(MouseEventKind::Down(MouseButton::Left), x));
    app.dispatch(&mouse(MouseEventKind::Up(MouseButton::Left), x));
    assert!(
        capture.texts().is_empty(),
        "a click only positions the caret"
    );
    let end = x + first.width() as u16;
    app.dispatch(&mouse(MouseEventKind::Down(MouseButton::Left), x));
    app.dispatch(&mouse(MouseEventKind::Drag(MouseButton::Left), end));
    assert_eq!(app.panel().selected_text(), Some(first));
    assert!(
        capture.texts().is_empty(),
        "dragging does not copy before release"
    );
    app.dispatch(&mouse(MouseEventKind::Up(MouseButton::Left), end));
    assert_eq!(capture.texts(), [first]);
    assert!(app.panel().selected_text().is_none());
    app.dispatch(&mouse(MouseEventKind::Up(MouseButton::Left), end));
    assert_eq!(
        capture.texts().len(),
        1,
        "a repeated release cannot duplicate copying"
    );
    app.dispatch(&key(KeyCode::Char('!')));
    let ctrl = |code| Event::Key(KeyEvent::new(code, KeyModifiers::CONTROL));
    app.dispatch(&ctrl(KeyCode::Char('a')));
    app.dispatch(&ctrl(KeyCode::Char('c')));
    assert_eq!(capture.texts().last().unwrap(), &format!("{first}!\nbody"));
    app.dispatch(&key(KeyCode::Esc));
    app.dispatch(&key(KeyCode::Esc));
    assert_eq!(snapshot(&app.ctx.repos[0].path), before);
}

#[test]
fn text_selection_click_after_copy_survives_notice_clear_and_resize() {
    let (_dir, mut app) = fixture();
    let capture = clipboard::CopyCapture::start();
    let before = snapshot(&app.ctx.repos[0].path);
    for (width, height) in [(120, 35), (80, 25), (44, 13)] {
        app.go(Commit);
        screen(&mut app, width, height);
        app.dispatch(&key(KeyCode::Char('i')));
        app.dispatch(&Event::Paste("first line\nsecond line\nthird line".into()));
        // The inline composer may show only one line; navigate its viewport.
        for code in [KeyCode::Home, KeyCode::Up, KeyCode::Up] {
            app.dispatch(&key(code));
        }
        let find = |text: String, needle: &str| {
            text.lines()
                .enumerate()
                .find_map(|(y, line)| {
                    line.find(needle)
                        .map(|i| (line[..i].width() as u16, y as u16))
                })
                .unwrap()
        };
        let (x, y) = find(screen(&mut app, width, height), "first line");
        let mouse = |kind, column, row| {
            Event::Mouse(MouseEvent {
                kind,
                column,
                row,
                modifiers: KeyModifiers::NONE,
            })
        };
        let copies = capture.texts().len();
        app.dispatch(&mouse(MouseEventKind::Down(MouseButton::Left), x, y));
        app.dispatch(&mouse(MouseEventKind::Drag(MouseButton::Left), x + 5, y));
        app.dispatch(&mouse(MouseEventKind::Up(MouseButton::Left), x + 5, y));
        assert_eq!(capture.texts().len(), copies + 1);
        assert_eq!(capture.texts().last().unwrap(), "first");
        assert!(app.panel().selected_text().is_none());
        assert!(app.msg.contains("Copied"));
        app.dispatch(&key(KeyCode::Down));
        let (x, y) = find(screen(&mut app, width, height), "second line");
        app.dispatch(&mouse(MouseEventKind::Down(MouseButton::Left), x + 2, y));
        assert!(app.msg.is_empty());
        // Moving the editor between Down and Up must still count as a click.
        screen(&mut app, width, height + 2);
        app.dispatch(&mouse(MouseEventKind::Up(MouseButton::Left), x + 2, y));
        assert_eq!(
            capture.texts().len(),
            copies + 1,
            "{width}x{height}: a click after copying changed the clipboard"
        );
        assert!(app.panel().selected_text().is_none());
        app.dispatch(&key(KeyCode::Esc));
    }
    assert_eq!(snapshot(&app.ctx.repos[0].path), before);
}

#[test]
fn text_selection_ctrl_c_copies_before_quit_and_help_preserves_the_editor_selection() {
    let (_dir, mut app) = fixture();
    app.go(Log);
    let capture = clipboard::CopyCapture::start();
    let repo = app.ctx.repos[0].path.clone();
    let before = snapshot(&repo);
    screen(&mut app, 120, 36);
    app.dispatch(&key(KeyCode::Tab));
    app.dispatch(&key(KeyCode::Tab));
    screen(&mut app, 120, 36);
    let ctrl = |code| Event::Key(KeyEvent::new(code, KeyModifiers::CONTROL));
    assert!(!app
        .help_content()
        .sections
        .iter()
        .flat_map(|s| s.bindings)
        .any(|(key, _)| *key == "Ctrl-c"));
    app.dispatch(&ctrl(KeyCode::Char('a')));
    assert!(app
        .help_content()
        .sections
        .iter()
        .flat_map(|s| s.bindings)
        .any(|(key, _)| *key == "Ctrl-c"));
    let details = app.panel().selected_text().unwrap().to_owned();
    assert!(details.contains("base"));
    app.dispatch(&ctrl(KeyCode::Char('c')));
    assert!(app.pop.is_none());
    assert_eq!(capture.texts(), [details]);
    app.dispatch(&key(KeyCode::Esc));
    app.dispatch(&ctrl(KeyCode::Char('c')));
    assert!(matches!(app.pop, Some(Popup::Confirm { .. })));
    app.dispatch(&key(KeyCode::Esc));

    app.go(Commit);
    screen(&mut app, 80, 24);
    app.dispatch(&key(KeyCode::Char('i')));
    let text = "Edit message ＵＩ e\u{301} 👩‍💻\nKeep original newlines\n";
    app.dispatch(&Event::Paste(text.into()));
    app.dispatch(&ctrl(KeyCode::Char('a')));
    screen(&mut app, 80, 24);
    app.dispatch(&key(KeyCode::F(1)));
    assert_eq!(context(&app), "Commit / Edit Message");
    app.dispatch(&ctrl(KeyCode::Char('c')));
    assert_eq!(
        capture.texts().len(),
        1,
        "help never copies from the covered panel"
    );
    assert!(
        app.pop.is_none(),
        "Ctrl-C closes help without touching its origin"
    );
    assert_eq!(app.panel().selected_text(), Some(text));
    app.dispatch(&ctrl(KeyCode::Char('c')));
    assert_eq!(capture.texts().last().unwrap(), text);
    assert!(app.pop.is_none());
    app.dispatch(&key(KeyCode::F(1)));
    app.dispatch(&Event::Paste("Ctrl-c".into()));
    app.dispatch(&key(KeyCode::Enter));
    assert!(app.pop.is_none());
    assert_eq!(capture.texts().len(), 3);
    assert_eq!(capture.texts().last().unwrap(), text);
    app.dispatch(&key(KeyCode::Esc));
    app.dispatch(&key(KeyCode::Esc));
    assert_eq!(snapshot(&repo), before);
}

#[test]
fn help_tracks_keyboard_focus_across_log_commit_and_push_without_changing_git() {
    let (_dir, mut app) = fixture();
    app.go(Log);
    let repo = app.ctx.repos[0].path.clone();
    let before = snapshot(&repo);
    screen(&mut app, 120, 36);
    for title in [
        "Log / Commits",
        "Log / File Tree",
        "Log / Commit Details",
        "Log / Branch Tree",
    ] {
        check_context(&mut app, title);
        app.dispatch(&key(KeyCode::Tab));
    }
    app.go(Commit);
    check_context(&mut app, "Commit / Changes");
    app.dispatch(&key(KeyCode::Char('v')));
    screen(&mut app, 120, 36);
    app.dispatch(&key(KeyCode::Tab));
    check_context(&mut app, "Commit / Preview");
    app.dispatch(&key(KeyCode::Tab));
    check_context(&mut app, "Commit / Message");
    app.go(Push);
    check_context(&mut app, "Push / Repos and Commits");
    app.dispatch(&key(KeyCode::Tab));
    check_context(&mut app, "Push / Changed Files");
    app.dispatch(&key(KeyCode::Char('d')));
    screen(&mut app, 120, 36);
    app.dispatch(&key(KeyCode::Tab));
    check_context(&mut app, "Push / Diff and Details");
    assert_eq!(snapshot(&repo), before);
}

#[test]
fn help_shortcut_preserves_mouse_focus_and_text_input() {
    let (_dir, mut app) = fixture();
    app.go(Log);
    screen(&mut app, 120, 36);
    let click = |x, y| {
        Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: x,
            row: y,
            modifiers: KeyModifiers::NONE,
        })
    };
    app.dispatch(&click(117, app.regions.body.y + 3));
    app.dispatch(&key(KeyCode::F(1)));
    assert_eq!(context(&app), "Log / File Tree");
    app.dispatch(&key(KeyCode::Esc));

    app.dispatch(&key(KeyCode::Char('/')));
    app.dispatch(&Event::Paste("Query ?12".into()));
    let before = screen(&mut app, 120, 36);
    app.dispatch(&key(KeyCode::F(1)));
    assert_eq!(context(&app), "Log / Text Search");
    assert!(app.log.editing());
    app.dispatch(&Event::Paste("different help query".into()));
    app.dispatch(&key(KeyCode::Enter));
    assert!(matches!(app.pop, Some(Popup::Help { .. })));
    app.dispatch(&key(KeyCode::F(1)));
    assert_eq!(screen(&mut app, 120, 36), before);
}

#[test]
fn help_f1_in_commit_editor_keeps_draft_focus_and_blocks_underlying_shortcuts() {
    let (_dir, mut app) = fixture();
    let before_git = snapshot(&app.ctx.repos[0].path);
    app.go(Commit);
    app.dispatch(&key(KeyCode::Char('i')));
    app.dispatch(&Event::Paste("My draft ?12".into()));
    let before = screen(&mut app, 80, 24);
    app.dispatch(&key(KeyCode::Char('?')));
    assert!(
        app.pop.is_none(),
        "a question mark remains ordinary editor text"
    );
    let with_question = screen(&mut app, 80, 24);
    assert_ne!(before, with_question);
    app.dispatch(&key(KeyCode::F(1)));
    assert_eq!(context(&app), "Commit / Edit Message");
    if let Some(Popup::Help { state, .. }) = &app.pop {
        assert!(!state
            .content
            .sections
            .iter()
            .any(|s| s.title.contains("Global Actions")));
    }
    for ch in ['2', 'y', 'c', 'P'] {
        app.dispatch(&key(KeyCode::Char(ch)));
    }
    app.dispatch(&Event::Paste("help only".into()));
    app.dispatch(&key(KeyCode::Enter));
    assert_eq!(app.tab, Commit);
    assert!(!app.quit);
    assert!(matches!(app.pop, Some(Popup::Help { .. })));
    app.dispatch(&key(KeyCode::Esc));
    assert!(app.commit.editing());
    assert_eq!(screen(&mut app, 80, 24), with_question);
    assert_eq!(snapshot(&app.ctx.repos[0].path), before_git);
}

#[test]
fn help_restores_roots_review_and_workspace_input_exactly_after_closing() {
    let (_dir, mut app) = fixture();
    let popups = [
        (
            Popup::Scope {
                picked: BTreeSet::new(),
                row: 0,
            },
            "Roots / Repository scope",
        ),
        (
            Popup::Confirm {
                title: "Keep review".into(),
                body: "Pending review body".into(),
                token: "__quit".into(),
                owner: Commit,
                scroll: 7,
                yes: true,
            },
            "Review / Confirm action",
        ),
        (
            Popup::Open {
                path: "/temporary/my draft".into(),
            },
            "Workspace / Path input",
        ),
    ];
    for (popup, expected) in popups {
        app.pop = Some(popup);
        let before = screen(&mut app, 80, 24);
        app.dispatch(&key(KeyCode::F(1)));
        assert_eq!(context(&app), expected);
        screen(&mut app, 80, 24);
        app.dispatch(&Event::Paste("no matching shortcut".into()));
        app.dispatch(&key(KeyCode::Enter));
        assert!(matches!(app.pop, Some(Popup::Help { .. })));
        assert!(!app.quit);
        app.dispatch(&key(KeyCode::Esc));
        assert_eq!(screen(&mut app, 80, 24), before);
        match &app.pop {
            Some(Popup::Scope { picked, row }) => {
                assert!(picked.is_empty());
                assert_eq!(*row, 0);
            }
            Some(Popup::Confirm {
                scroll, yes, owner, ..
            }) => {
                assert_eq!(*scroll, 7);
                assert!(*yes);
                assert_eq!(*owner, Commit);
            }
            Some(Popup::Open { path }) => assert_eq!(path, "/temporary/my draft"),
            _ => panic!("origin was not restored"),
        }
    }
}

#[test]
fn help_f1_from_diff_and_all_supported_window_sizes_remains_read_only() {
    let (_dir, mut app) = fixture();
    app.go(Log);
    let before = snapshot(&app.ctx.repos[0].path);
    app.dispatch(&key(KeyCode::Enter));
    assert!(app.log.editing());
    for (width, height) in [(120, 36), (80, 24), (44, 12)] {
        app.dispatch(&key(KeyCode::F(1)));
        assert_eq!(context(&app), "Log / Diff");
        assert!(screen(&mut app, width, height).contains("KEYMAP //"));
        app.dispatch(&key(KeyCode::PageDown));
        app.dispatch(&key(KeyCode::Esc));
        assert!(app.log.editing());
    }
    app.dispatch(&key(KeyCode::Esc));
    assert!(!app.log.editing());
    assert_eq!(snapshot(&app.ctx.repos[0].path), before);
}

#[test]
fn help_follows_all_merge_columns_and_preserves_the_result_editor() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path();
    git(repo, &["init", "-q", "-b", "main"]).unwrap();
    let save = |text: &str| {
        fs::write(repo.join("conflict.txt"), text).unwrap();
        git(repo, &["add", "--", "conflict.txt"]).unwrap();
        git(
            repo,
            &[
                "-c",
                "user.name=Help tester",
                "-c",
                "user.email=help@example.invalid",
                "-c",
                "core.hooksPath=/dev/null",
                "commit",
                "--no-gpg-sign",
                "-qm",
                text,
            ],
        )
        .unwrap();
    };
    save("base\n");
    git(repo, &["checkout", "-qb", "topic"]).unwrap();
    save("incoming\n");
    git(repo, &["checkout", "-q", "main"]).unwrap();
    save("current\n");
    assert!(git(repo, &["merge", "--no-commit", "topic"]).is_err());
    let before_git = snapshot(repo);
    let before_file = fs::read(repo.join("conflict.txt")).unwrap();
    let mut ctx = Context::discover(repo).unwrap();
    ctx.state_dir = repo.join("state");
    let mut app = Shell::new(ctx, IconMode::Ascii);
    app.go(Conflicts);
    app.dispatch(&key(KeyCode::Enter));
    for expected in [
        "Merge / Result",
        "Merge / Right Source",
        "Merge / Left Source",
    ] {
        check_context(&mut app, expected);
        app.dispatch(&key(KeyCode::Tab));
    }
    app.dispatch(&key(KeyCode::Char('i')));
    app.dispatch(&Event::Paste("My merge draft".into()));
    let before = screen(&mut app, 80, 24);
    app.dispatch(&key(KeyCode::F(1)));
    assert_eq!(context(&app), "Merge / Edit Result");
    app.dispatch(&Event::Paste("help query".into()));
    app.dispatch(&key(KeyCode::Enter));
    app.dispatch(&key(KeyCode::Esc));
    assert!(app.merge.editing());
    assert_eq!(screen(&mut app, 80, 24), before);
    assert_eq!(snapshot(repo), before_git);
    assert_eq!(fs::read(repo.join("conflict.txt")).unwrap(), before_file);
}

#[test]
fn help_focuses_search_immediately_over_the_commit_editor_and_restores_it() {
    let (_dir, mut app) = fixture();
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    let assert_cursor = |terminal: &Terminal<TestBackend>, visible| {
        let mut expected = terminal.backend().clone();
        if visible {
            expected.show_cursor().unwrap();
        } else {
            expected.hide_cursor().unwrap();
        }
        assert_eq!(terminal.backend(), &expected);
    };
    app.go(Commit);
    app.dispatch(&key(KeyCode::Char('i')));
    terminal.draw(|f| app.draw(f)).unwrap();
    assert_cursor(&terminal, true);
    app.dispatch(&key(KeyCode::F(1)));
    terminal.draw(|f| app.draw(f)).unwrap();
    assert_cursor(&terminal, true);
    app.dispatch(&key(KeyCode::Char('/')));
    terminal.draw(|f| app.draw(f)).unwrap();
    assert_cursor(&terminal, true);
    app.dispatch(&key(KeyCode::Esc));
    terminal.draw(|f| app.draw(f)).unwrap();
    assert_cursor(&terminal, true);
}

fn run_help(app: &mut Shell, query: &str) {
    app.dispatch(&key(KeyCode::F(1)));
    app.dispatch(&Event::Paste(query.into()));
    app.dispatch(&key(KeyCode::Enter));
}

#[test]
fn help_enter_executes_navigation_filters_and_global_panel_switches() {
    let (_dir, mut app) = fixture();
    let repo = app.ctx.repos[0].path.clone();
    git(&repo, &["add", "."]).unwrap();
    git(
        &repo,
        &[
            "-c",
            "user.name=Help tester",
            "-c",
            "user.email=help@example.invalid",
            "-c",
            "core.hooksPath=/dev/null",
            "commit",
            "--no-gpg-sign",
            "-qm",
            "second",
        ],
    )
    .unwrap();
    app.refresh();
    app.go(Log);
    let before = snapshot(&repo);
    let selected_oid = |app: &mut Shell| {
        app.log.event(&key(KeyCode::Char('y')), &app.ctx);
        match app.log.event(&key(KeyCode::Char('y')), &app.ctx) {
            Action::Copy { text, .. } => text,
            _ => panic!("expected a copy action without touching the system clipboard"),
        }
    };
    run_help(&mut app, "Select last commit");
    assert!(app.pop.is_none());
    let last = selected_oid(&mut app);
    assert_ne!(last, git(&repo, &["rev-parse", "HEAD"]).unwrap().trim());
    run_help(&mut app, "gg");
    assert!(app.pop.is_none());
    assert!(app.key_sequence().is_none());
    assert_eq!(
        selected_oid(&mut app),
        git(&repo, &["rev-parse", "HEAD"]).unwrap().trim()
    );
    run_help(&mut app, "Open Date filter");
    assert_eq!(app.log.keymap().context(), "Log / Date Filter");
    app.dispatch(&key(KeyCode::Esc));
    run_help(&mut app, "Switch to Commit tab");
    assert_eq!(app.tab, Commit);
    assert!(app.pop.is_none());
    run_help(&mut app, "Switch to Log tab");
    assert_eq!(app.tab, Log);
    assert_eq!(snapshot(&repo), before);
}

#[test]
fn help_enter_runs_the_selected_candidate_after_down_and_keeps_typing_in_search() {
    let (_dir, mut app) = fixture();
    app.go(Log);
    app.dispatch(&key(KeyCode::F(1)));
    app.dispatch(&Event::Paste("Select commit".into()));
    let text = screen(&mut app, 80, 24);
    assert!(text.contains("> j"), "{text}");
    app.dispatch(&key(KeyCode::Down));
    let text = screen(&mut app, 80, 24);
    assert!(text.contains("> k"), "{text}");
    app.dispatch(&key(KeyCode::Enter));
    assert!(app.pop.is_none());
    assert!(!app.log.editing());
    app.go(Commit);
    run_help(&mut app, "Edit current repo message");
    assert!(app.commit.editing());
    app.dispatch(&Event::Paste("Original draft".into()));
    run_help(&mut app, "Move to line start");
    assert!(app.pop.is_none());
    assert!(app.commit.editing());
    app.dispatch(&Event::Paste("Prefix ".into()));
    assert!(screen(&mut app, 80, 24).contains("Prefix Original draft"));
}

#[test]
fn help_enter_opens_commit_review_and_cancel_preserves_head_index_and_worktree() {
    let (_dir, mut app) = fixture();
    let before = snapshot(&app.ctx.repos[0].path);
    app.go(Commit);
    app.dispatch(&key(KeyCode::Char('i')));
    app.dispatch(&Event::Paste("review from keymap".into()));
    app.dispatch(&key(KeyCode::F(2)));
    run_help(&mut app, "Review commit of visible checked files");
    assert!(matches!(app.pop, Some(Popup::Confirm { .. })));
    assert_eq!(snapshot(&app.ctx.repos[0].path), before);
    run_help(&mut app, "Cancel review");
    assert!(app.pop.is_none());
    assert_eq!(snapshot(&app.ctx.repos[0].path), before);
}

#[test]
fn help_executes_against_the_restored_roots_and_confirmation_dialogs() {
    let (_dir, mut app) = fixture();
    let before = snapshot(&app.ctx.repos[0].path);
    let scope = app.ctx.scope.clone();
    app.global(0);
    run_help(&mut app, "Space");
    match &app.pop {
        Some(Popup::Scope { picked, .. }) => assert!(picked.is_empty()),
        _ => panic!("Roots must remain open after running its Space shortcut"),
    }
    assert_eq!(app.ctx.scope, scope);
    run_help(&mut app, "Discard scope changes");
    assert!(app.pop.is_none());
    assert_eq!(app.ctx.scope, scope);
    app.quit_box();
    run_help(&mut app, "Confirm reviewed action");
    assert!(
        app.quit,
        "the explicitly selected y command reaches the original review"
    );
    assert_eq!(snapshot(&app.ctx.repos[0].path), before);
}

#[test]
fn help_revalidates_the_origin_before_replaying_a_shortcut() {
    let (_dir, mut app) = fixture();
    app.go(Commit);
    app.dispatch(&key(KeyCode::F(1)));
    // A background result can open an editor while Help still owns the input.
    app.commit.event(&key(KeyCode::Char('i')), &app.ctx);
    app.dispatch(&key(KeyCode::Enter));
    assert_eq!(context(&app), "Commit / Edit Message");
    assert!(app.commit.editing());
    assert!(app.msg.contains("Context changed"));
    app.dispatch(&key(KeyCode::Esc));
    assert!(!screen(&mut app, 80, 24).contains("Search"));
}

#[test]
fn help_executes_commit_view_options_against_the_active_menu() {
    let (_dir, mut app) = fixture();
    let before = snapshot(&app.ctx.repos[0].path);
    app.go(Commit);
    run_help(&mut app, "Open View Options");
    assert_eq!(app.commit.keymap().context(), "Commit / View Options");
    check_context(&mut app, "Commit / View Options");
    run_help(&mut app, "Toggle view option");
    assert!(app.pop.is_none());
    assert_eq!(app.commit.keymap().context(), "Commit / View Options");
    app.dispatch(&key(KeyCode::Esc));
    assert!(!app.commit.editing());
    assert_eq!(snapshot(&app.ctx.repos[0].path), before);
}

#[test]
fn help_executes_push_grouping_and_switching_tabs_preserves_it() {
    let (_dir, mut app) = fixture();
    app.go(Push);
    run_help(&mut app, "Open Group By");
    check_context(&mut app, "Push / File Grouping");
    run_help(&mut app, "Toggle grouping option");
    let text = screen(&mut app, 44, 12);
    assert!(
        text.contains("[ ] Directory")
            && text.contains("[x] Repository")
            && text.contains("[ ] Module")
    );
    app.dispatch(&key(KeyCode::Char('p')));
    app.dispatch(&key(KeyCode::Char('2')));
    assert!(app.pop.is_none() && app.tab == Push && app.push.editing());
    let log = app.regions.tab(Log);
    app.dispatch(&Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: log.x,
        row: log.y,
        modifiers: KeyModifiers::NONE,
    }));
    assert_eq!(app.tab, Log);
    assert!(!app.push.editing());
    app.go(Push);
    app.dispatch(&key(KeyCode::Char('o')));
    assert!(screen(&mut app, 80, 24).contains("[ ] Directory"));
}
