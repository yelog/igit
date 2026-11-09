use super::{test_support::*, *};
use crate::core::git;
use crate::ui::panes::PaneId;
use crossterm::event::MouseEvent;
use std::fs;
mod inline_message;
mod refresh;

#[test]
fn push_entry_renders_loading_before_reads_and_keeps_navigation_and_quit_available() {
    let (_dir, mut app) = fixture();
    let before = snapshot(&app.ctx.repos[0].path);
    app.go(Push);
    assert!(app.push.reads_pending());
    assert!(
        !app.push.busy(),
        "read loading must not acquire the remote write guard"
    );
    assert!(screen(&mut app, 120, 36).contains("Loading outgoing commits"));
    assert!(app.poll_timeout() <= Duration::from_millis(16));
    app.dispatch(&key(KeyCode::Char('P')));
    assert!(app.pop.is_none());
    app.dispatch(&key(KeyCode::Char('F')));
    assert!(!app.push.busy());
    app.go(Commit);
    assert_eq!(app.tab, Commit);
    app.quit_box();
    assert!(matches!(&app.pop, Some(Popup::Confirm { token, .. }) if token == "__quit"));
    app.dispatch(&key(KeyCode::Esc));
    app.push.finish_loading(&app.ctx);
    assert_eq!(snapshot(&app.ctx.repos[0].path), before);
}

#[test]
fn conflicts_tab_stays_available_until_the_git_operation_is_complete() {
    let (_dir, mut app) = fixture();
    let repo = app.ctx.repos[0].path.clone();
    let git = |args: &[&str]| git(&repo, args).unwrap();
    let header = |app: &mut Shell| screen(app, 44, 12).lines().next().unwrap().to_owned();
    assert!(!header(&mut app).contains("Conflicts"));
    merge_conflict(&repo);
    let head = git(&["rev-parse", "HEAD"]);

    app.dispatch(&key(KeyCode::F(5)));
    assert!(header(&mut app).contains("Commit Log Push Branch Stash Conflicts"));
    assert_eq!(app.tab, Commit, "refresh must not steal focus");
    let mut terminal = Terminal::new(ratatui::backend::TestBackend::new(44, 12)).unwrap();
    terminal.draw(|f| app.draw(f)).unwrap();
    let conflict_tab = app.regions.tab(Conflicts);
    let label = &terminal.backend().buffer()[(conflict_tab.x + 1, conflict_tab.y)];
    assert_eq!(label.fg, crate::ui::idea::CONFLICT);
    assert!(label.modifier.contains(ratatui::style::Modifier::BOLD));
    click_tab(&mut app, Conflicts);
    assert_eq!(app.tab, Conflicts);
    assert!(screen(&mut app, 120, 36).contains("mixed.txt"));
    for ch in ['Y', 'A'] {
        app.dispatch(&key(KeyCode::Char(ch)));
    }
    assert!(screen(&mut app, 120, 36).contains("Stage resolved file"));
    assert!(!git(&["ls-files", "--unmerged"]).is_empty());
    app.dispatch(&key(KeyCode::Char('y')));
    assert!(git(&["ls-files", "--unmerged"]).is_empty());
    assert_eq!(git(&["rev-parse", "HEAD"]), head);
    assert!(header(&mut app).contains("Conflicts"));
    assert!(screen(&mut app, 120, 36).contains("Ready to Continue"));
    app.dispatch(&key(KeyCode::Char('C')));
    assert!(screen(&mut app, 120, 36).contains("Continue merge"));
    app.dispatch(&key(KeyCode::Char('y')));
    assert_ne!(git(&["rev-parse", "HEAD"]), head);
    assert!(!header(&mut app).contains("Conflicts"));
    assert_eq!(app.help_content().context(), "Commit / Changes");
}

fn window_prefix(app: &mut Shell) {
    app.dispatch(&Event::Key(KeyEvent::new(
        KeyCode::Char('w'),
        KeyModifiers::CONTROL,
    )));
}
fn ctrl_w(app: &mut Shell, suffix: char) {
    window_prefix(app);
    app.dispatch(&key(KeyCode::Char(suffix)));
}

fn mouse(kind: MouseEventKind, x: u16, y: u16) -> Event {
    Event::Mouse(MouseEvent {
        kind,
        column: x,
        row: y,
        modifiers: KeyModifiers::NONE,
    })
}

fn click_tab(app: &mut Shell, tab: Tab) {
    let r = app.regions.tab(tab);
    assert!(!r.is_empty(), "{tab:?} tab is hidden");
    app.dispatch(&mouse(MouseEventKind::Down(MouseButton::Left), r.x, r.y));
}

#[test]
fn commit_resolve_opens_conflicts_without_writing_to_git() {
    let (_dir, mut app) = fixture();
    let repo = app.ctx.repos[0].path.clone();
    merge_conflict(&repo);
    let before = snapshot(&repo);
    let worktree = fs::read(repo.join("mixed.txt")).unwrap();
    app.dispatch(&key(KeyCode::F(5)));

    let commit = screen(&mut app, 80, 24);
    assert!(commit.contains("Changes 1 file"));
    assert!(commit.contains("mixed.txt repo"));
    assert!(!commit.contains("UU conflict"));
    let (y, row) = commit
        .lines()
        .enumerate()
        .find(|(_, line)| line.contains("Merge Conflicts Resolve"))
        .expect("visible Merge Conflicts group and Resolve link");
    let column = |label| row[..row.find(label).unwrap()].width() as u16;
    let click = |app: &mut Shell, x| {
        app.dispatch(&mouse(MouseEventKind::Down(MouseButton::Left), x, y as u16));
    };
    click(&mut app, column("Merge Conflicts"));
    assert_eq!(app.tab, Commit, "the label only selects the row");
    click(&mut app, column("Resolve") + 6);
    assert_eq!(app.tab, Conflicts);
    assert!(app.pop.is_none());
    assert!(screen(&mut app, 80, 24).contains("mixed.txt"));
    app.dispatch(&key(KeyCode::Esc));
    assert_eq!(app.tab, Commit);
    app.dispatch(&key(KeyCode::Enter));
    assert_eq!(
        app.tab, Conflicts,
        "Enter activates the selected group's link"
    );
    assert_eq!(snapshot(&repo), before);
    assert_eq!(fs::read(repo.join("mixed.txt")).unwrap(), worktree);

    app.go(Commit);
    fs::write(repo.join("mixed.txt"), "resolved\n").unwrap();
    git(&repo, &["add", "--", "mixed.txt"]).unwrap();
    app.dispatch(&key(KeyCode::F(5)));
    assert!(!screen(&mut app, 80, 24).contains("Merge Conflicts"));
    assert!(
        app.tab_available(Conflicts),
        "the merge still needs Continue"
    );
}

#[test]
fn pane_navigation_stays_in_the_active_tab_and_keeps_git_state() {
    let (_dir, mut app) = fixture();
    let before = snapshot(&app.ctx.repos[0].path);
    app.go(Log);
    screen(&mut app, 120, 36);
    let info = app.log.panes.rect(PaneId::Info).unwrap();
    app.dispatch(&mouse(MouseEventKind::ScrollDown, info.x + 1, info.y + 1));
    assert_eq!(
        app.log.focused_pane(),
        PaneId::Graph,
        "scrolling under the pointer keeps keyboard focus"
    );
    ctrl_w(&mut app, 'h');
    assert_eq!(app.help_content().context(), "Log / Branch Tree");
    ctrl_w(&mut app, 'k');
    assert_eq!(app.help_content().context(), "Log / Branch Tree");
    app.dispatch(&key(KeyCode::BackTab));
    assert_eq!(app.help_content().context(), "Log / Commit Details");
    app.dispatch(&key(KeyCode::Tab));
    assert_eq!(app.help_content().context(), "Log / Branch Tree");
    click_tab(&mut app, Commit);
    screen(&mut app, 120, 36);
    assert_eq!(app.help_content().context(), "Commit / Changes");
    app.dispatch(&key(KeyCode::Tab));
    assert_eq!(app.help_content().context(), "Commit / Preview");
    app.dispatch(&key(KeyCode::Tab));
    assert_eq!(app.help_content().context(), "Commit / Message");
    click_tab(&mut app, Log);
    assert_eq!(app.help_content().context(), "Log / Branch Tree");
    assert_eq!(snapshot(&app.ctx.repos[0].path), before);
}

#[test]
fn pane_navigation_keyboard_and_drag_resize_the_drawn_splits_and_reset() {
    use MouseEventKind::{Down, Drag, Up};
    let (_dir, mut app) = fixture();
    let before = snapshot(&app.ctx.repos[0].path);
    app.go(Log);
    let size = |app: &mut Shell| {
        screen(app, 120, 36);
        (
            app.log.panes.rect(PaneId::Graph).unwrap().width,
            app.log.panes.rect(PaneId::Files).unwrap().height,
        )
    };
    let (w, h) = size(&mut app);
    window_prefix(&mut app);
    for ch in "5>".chars() {
        app.dispatch(&key(KeyCode::Char(ch)));
    }
    assert_eq!(size(&mut app), (w + 5, h));
    ctrl_w(&mut app, 'l');
    assert_eq!(app.log.focused_pane(), PaneId::Files);
    ctrl_w(&mut app, '+');
    assert_eq!(size(&mut app), (w + 5, h + 1));
    let files = app.log.panes.rect(PaneId::Files).unwrap();
    let border = files.bottom() - 1;
    let left = MouseButton::Left;
    app.dispatch(&mouse(Down(left), files.x + 3, border));
    app.dispatch(&mouse(Drag(left), 0, border + 4));
    assert_eq!(size(&mut app), (w + 5, h + 5));
    app.dispatch(&mouse(Up(left), 0, border + 5));
    app.dispatch(&mouse(Drag(left), 0, border)); // No new press.
    assert_eq!(size(&mut app), (w + 5, h + 6));
    assert_eq!(app.log.focused_pane(), PaneId::Files);
    ctrl_w(&mut app, '=');
    assert_eq!(size(&mut app), (w, h));

    // Tab switches retain each panel's geometry and pane focus.
    app.dispatch(&key(KeyCode::Char('3')));
    screen(&mut app, 120, 36);
    ctrl_w(&mut app, 'l');
    assert_eq!(app.push.focused_pane(), PaneId::PushFiles);
    app.dispatch(&key(KeyCode::Tab));
    assert_eq!(app.push.focused_pane(), PaneId::Outgoing);
    app.dispatch(&key(KeyCode::Esc));
    assert_eq!(size(&mut app), (w, h));
    assert_eq!(snapshot(&app.ctx.repos[0].path), before);
}

#[test]
fn pane_navigation_help_prefix_and_log_close_are_distinct_from_editing() {
    let (_dir, mut app) = fixture();
    app.go(Log);
    let tabs = |app: &mut Shell| {
        screen(app, 120, 36)
            .lines()
            .next()
            .unwrap()
            .trim_end()
            .to_owned()
    };
    let initial = tabs(&mut app);
    app.dispatch(&Event::Key(KeyEvent::new(
        KeyCode::Char('t'),
        KeyModifiers::CONTROL,
    )));
    let two = tabs(&mut app);
    assert_ne!(initial, two);
    window_prefix(&mut app);
    app.key_picker.poll(Instant::now() + Duration::from_secs(1));
    assert_eq!(
        tabs(&mut app),
        two,
        "the prefix alone must leave both Log tabs open"
    );
    assert!(screen(&mut app, 120, 36).contains("Focus right pane"));
    app.dispatch(&key(KeyCode::Esc));
    assert!(app.key_sequence().is_none() && app.pop.is_none());
    app.dispatch(&key(KeyCode::F(1)));
    app.dispatch(&Event::Paste("Focus right pane".into()));
    app.dispatch(&key(KeyCode::Enter));
    assert_eq!(app.log.focused_pane(), PaneId::Files);
    ctrl_w(&mut app, 'c');
    assert_eq!(tabs(&mut app), initial);

    window_prefix(&mut app);
    app.dispatch(&Event::FocusLost);
    assert!(app.key_sequence().is_none());
    app.dispatch(&key(KeyCode::Char('1')));
    app.dispatch(&key(KeyCode::Char('i')));
    window_prefix(&mut app);
    app.dispatch(&key(KeyCode::Char('l')));
    assert!(app.commit.editing() && app.key_sequence().is_none());
    app.dispatch(&key(KeyCode::Esc));
    assert_eq!(app.tab, Commit);
}

#[test]
fn workspace_tabs_open_on_commit_and_switch_in_header_order_at_each_size() {
    let (_dir, mut app) = fixture();
    assert_eq!(app.tab, Commit);
    assert_eq!(app.commit.focused_pane(), PaneId::Changes);
    assert!(screen(&mut app, 120, 36).contains("mixed.txt"));
    for (w, h) in [(120, 36), (80, 25), (44, 12)] {
        app.dispatch(&key(KeyCode::Char('2')));
        let log = screen(&mut app, w, h);
        let header = log.lines().next().unwrap();
        assert!(header.starts_with(" Log  + "), "{header}");
        assert!(
            header.ends_with(" Commit  Log  Push  Branch  Stash ")
                || header.ends_with(" Commit Log Push Branch Stash"),
            "{header}"
        );
        assert_eq!(app.regions.body, Rect::new(0, 1, w, h - 2));
        assert_eq!(app.regions.log_tabs.y, app.regions.tab(Log).y);
        assert_eq!(app.regions.log_tabs.right(), app.regions.tab(Commit).x);
        assert!(log
            .lines()
            .last()
            .unwrap()
            .starts_with(&format!(" IGit  v{}", env!("CARGO_PKG_VERSION"))));
        for removed in ["IGit", "Console", "Roots", "Actions", "Help"] {
            assert!(!header.contains(removed), "{header}");
        }
        assert!(app.regions.tab(Conflicts).is_empty());
        assert_eq!(app.tab, Log);
        assert!(log.contains("base"), "{log}");
        assert!(!app.regions.log_tabs.is_empty());
        app.dispatch(&key(KeyCode::Char('6')));
        assert_eq!(app.tab, Log);

        let add = header.find('+').unwrap() as u16;
        app.dispatch(&mouse(MouseEventKind::Down(MouseButton::Left), add, 0));
        let with_new_tab = screen(&mut app, w, h);
        assert!(with_new_tab.lines().next().unwrap().contains(" x "));
        assert_eq!(app.tab, Log);
        ctrl_w(&mut app, 'c');
        assert_eq!(screen(&mut app, w, h).lines().next().unwrap(), header);

        app.dispatch(&key(KeyCode::Char('1')));
        let commit = screen(&mut app, w, h);
        assert!(
            commit.contains("mixed.txt") && !commit.contains("base"),
            "{commit}"
        );
        assert!(app.log.panes.nodes().is_empty());
        assert!(app.regions.log_tabs.is_empty());

        click_tab(&mut app, Push);
        let push = screen(&mut app, w, h);
        assert!(
            push.contains("Push Commits") && !push.contains("mixed.txt"),
            "{push}"
        );
        assert!(app.commit.panes.nodes().is_empty());
        assert_eq!(app.regions.body.width, w);
        app.dispatch(&key(KeyCode::Esc));
        assert_eq!(app.tab, Commit);
    }
}

#[test]
fn push_cancel_returns_to_the_last_main_tab_even_with_details_open() {
    let (_dir, mut app) = fixture();
    let before = snapshot(&app.ctx.repos[0].path);
    for (width, height) in [(44, 12), (80, 24), (120, 36)] {
        for (shortcut, tab) in [('1', Commit), ('2', Log)] {
            for details in [false, true] {
                app.dispatch(&key(KeyCode::Char(shortcut)));
                app.dispatch(&key(KeyCode::Char('3')));
                if details {
                    app.dispatch(&key(KeyCode::Char('d')));
                }
                let text = screen(&mut app, width, height);
                assert_eq!(text.matches("Push Commits").count(), 1, "{text}");
                let (y, line) = text
                    .lines()
                    .enumerate()
                    .find(|(_, s)| s.contains(" Cancel "))
                    .unwrap();
                let x = line[..line.find("Cancel").unwrap()].width() as u16;
                app.dispatch(&mouse(MouseEventKind::Down(MouseButton::Left), x, y as u16));
                assert_eq!(app.tab, tab);
                assert!(app.pop.is_none());
            }
        }
    }
    assert_eq!(snapshot(&app.ctx.repos[0].path), before);
}

#[test]
fn commit_and_push_keyboard_action_commits_once_and_requires_separate_push_confirmation() {
    let (dir, mut app) = fixture();
    app.commit.enable_background_execution();
    let repo = app.ctx.repos[0].path.clone();
    let remote = dir.path().join("remote.git");
    git(&repo, &["clone", "--bare", ".", remote.to_str().unwrap()]).unwrap();
    git(
        &repo,
        &["remote", "add", "origin", remote.to_str().unwrap()],
    )
    .unwrap();
    git(&repo, &["fetch", "origin"]).unwrap();
    git(&repo, &["branch", "--set-upstream-to=origin/main"]).unwrap();
    let original_head = git(&remote, &["rev-parse", "main"]).unwrap();
    let text = screen(&mut app, 140, 36);
    let (row, line) = text
        .lines()
        .enumerate()
        .find(|(_, s)| s.contains("other.txt"))
        .unwrap();
    let column = line[..line.find("other.txt").unwrap()].width();
    app.dispatch(&Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: column as u16,
        row: row as u16,
        modifiers: KeyModifiers::NONE,
    }));
    app.dispatch(&key(KeyCode::Char(' '))); // Leave the other staged file unchecked.
    let before_commit = snapshot(&repo);
    app.dispatch(&key(KeyCode::Tab));
    assert_eq!(app.commit.focused_pane(), PaneId::Preview);
    screen(&mut app, 80, 24);
    app.dispatch(&key(KeyCode::Char('c')));
    assert!(app.commit.message_editing());
    assert_eq!(app.commit.focused_pane(), PaneId::Message);
    app.dispatch(&Event::Paste("tab-sync".into()));
    app.dispatch(&key(KeyCode::Esc));
    assert!(!app.commit.message_editing());
    assert_eq!(app.commit.focused_pane(), PaneId::Message);
    app.dispatch(&key(KeyCode::Right));
    assert_eq!(snapshot(&repo), before_commit);
    app.dispatch(&key(KeyCode::Enter));
    assert!(app.pop.is_none());
    finish_commit(&mut app);
    assert_eq!(app.tab, Push);
    assert!(app.pop.is_none());
    assert!(screen(&mut app, 140, 36).contains("Push Commits"));
    app.push.finish_loading(&app.ctx);
    app.dispatch(&key(KeyCode::Char('P')));
    let text = screen(&mut app, 80, 24);
    assert!(text.contains("Confirm") && !text.contains("y Confirm") && !text.contains("n Cancel"));
    for ch in ['y', 'n'] {
        app.dispatch(&key(KeyCode::Char(ch)));
        assert!(matches!(&app.pop, Some(Popup::Confirm { owner: Push, .. })));
        assert!(!app.push.busy());
    }
    app.dispatch(&key(KeyCode::Esc));
    assert_eq!(app.tab, Push);
    app.dispatch(&key(KeyCode::Esc));
    assert_eq!(app.tab, Commit);
    let text = screen(&mut app, 140, 36);
    assert!(text.contains("0 files selected"), "{text}");
    click_tab(&mut app, Log);
    let text = screen(&mut app, 140, 36);
    assert!(text.contains("tab-sync"), "{text}");
    assert_eq!(
        git(&repo, &["show", "HEAD:mixed.txt"]).unwrap().trim(),
        "staged"
    );
    assert_eq!(
        fs::read_to_string(repo.join("mixed.txt")).unwrap(),
        "working tree\n"
    );
    assert_eq!(
        git(&repo, &["show", "HEAD:other.txt"]).unwrap().trim(),
        "base"
    );
    assert_eq!(
        git(&repo, &["show", ":other.txt"]).unwrap().trim(),
        "unselected staged"
    );
    assert_eq!(git(&remote, &["rev-parse", "main"]).unwrap(), original_head);

    let before_push = snapshot(&repo);
    app.go(Push);
    app.push.finish_loading(&app.ctx);
    app.dispatch(&key(KeyCode::Char('P')));
    app.dispatch(&key(KeyCode::Enter));
    assert!(app.pop.is_none() && app.push.busy());
    app.go(Commit);
    app.commit.focus_pane(PaneId::Changes);
    app.dispatch(&key(KeyCode::Char('G')));
    app.dispatch(&key(KeyCode::Char(' ')));
    app.dispatch(&key(KeyCode::Char('i')));
    app.dispatch(&Event::Paste("must wait for push".into()));
    app.dispatch(&key(KeyCode::Esc));
    app.dispatch(&key(KeyCode::Enter));
    assert!(app.pop.is_none() && !app.commit.execution_running());
    assert!(
        app.msg.contains("Wait for the remote operation"),
        "{}",
        app.msg
    );
    let deadline = Instant::now() + Duration::from_secs(10);
    while app.push.busy() {
        app.poll();
        assert!(Instant::now() < deadline, "push timed out: {}", app.msg);
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(
        git(&remote, &["rev-parse", "main"]).unwrap(),
        git(&repo, &["rev-parse", "HEAD"]).unwrap()
    );
    assert_eq!(snapshot(&repo), before_push);
}
