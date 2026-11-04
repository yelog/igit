use super::{test_support::*, *};
use crate::core::git;
use crate::ui::panes::PaneId;
use crossterm::event::MouseEvent;
use std::fs;

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

#[test]
fn pane_navigation_crosses_the_workspace_and_keeps_git_state() {
    let (_dir, mut app) = fixture();
    let before = snapshot(&app.ctx.repos[0].path);
    screen(&mut app, 120, 36);
    let info = app.log.panes.rect(PaneId::Info).unwrap();
    app.dispatch(&mouse(MouseEventKind::ScrollDown, info.x + 1, info.y + 1));
    assert_eq!(
        app.log.focused_pane(),
        PaneId::Graph,
        "scrolling under the pointer keeps keyboard focus"
    );
    ctrl_w(&mut app, 'h');
    assert_eq!(app.help_content().context(), "Log / 分支树");
    ctrl_w(&mut app, 'h');
    assert_eq!(app.help_content().context(), "Log / 分支树");
    ctrl_w(&mut app, 'k');
    assert_eq!(app.help_content().context(), "Commit / 提交说明");
    ctrl_w(&mut app, 'k');
    assert_eq!(app.help_content().context(), "Commit / 变更清单");
    app.dispatch(&key(KeyCode::BackTab));
    assert_eq!(app.help_content().context(), "Log / 提交详情");
    app.dispatch(&key(KeyCode::Tab));
    assert_eq!(app.help_content().context(), "Commit / 变更清单");
    assert_eq!(snapshot(&app.ctx.repos[0].path), before);
}

#[test]
fn pane_navigation_keyboard_and_drag_resize_the_drawn_splits_and_reset() {
    use MouseEventKind::{Down, Drag, Up};
    let (_dir, mut app) = fixture();
    let before = snapshot(&app.ctx.repos[0].path);
    let size = |app: &mut Shell| {
        screen(app, 120, 36);
        (
            app.log.panes.rect(PaneId::Graph).unwrap().width,
            app.regions.log.height,
        )
    };
    let (w, h) = size(&mut app);
    window_prefix(&mut app);
    for ch in "5>".chars() {
        app.dispatch(&key(KeyCode::Char(ch)));
    }
    assert_eq!(size(&mut app), (w + 5, h));
    ctrl_w(&mut app, '+');
    assert_eq!(size(&mut app), (w + 5, h + 1));
    let border = app.regions.commit.bottom() - 1;
    let left = MouseButton::Left;
    app.dispatch(&mouse(Down(left), 10, border));
    app.dispatch(&mouse(Drag(left), 0, border + 4));
    assert_eq!(size(&mut app), (w + 5, h - 3));
    app.dispatch(&mouse(Up(left), 100, border + 5));
    app.dispatch(&mouse(Drag(left), 100, border)); // No new press.
    assert_eq!(size(&mut app), (w + 5, h - 4));
    assert_eq!(app.log.focused_pane(), PaneId::Graph);
    ctrl_w(&mut app, '=');
    assert_eq!(size(&mut app), (w, h));

    // Only the border press captures the drag; the active Push dialog shields
    // the entire workspace, and closing it restores the same geometry.
    app.dispatch(&key(KeyCode::Char('3')));
    screen(&mut app, 120, 36);
    app.dispatch(&mouse(Down(left), 0, app.regions.commit.bottom() - 1));
    app.dispatch(&mouse(Drag(left), 0, 3));
    app.dispatch(&mouse(Up(left), 0, 3));
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
    let tabs = |app: &mut Shell| {
        screen(app, 120, 36)
            .lines()
            .find(|s| s.contains("Git Log [1]"))
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
    assert!(screen(&mut app, 120, 36).contains("聚焦右侧 pane"));
    app.dispatch(&key(KeyCode::Esc));
    assert!(app.key_sequence().is_none() && app.pop.is_none());
    app.dispatch(&key(KeyCode::F(1)));
    app.dispatch(&Event::Paste("聚焦右侧 pane".into()));
    app.dispatch(&key(KeyCode::Enter));
    assert_eq!(app.log.focused_pane(), PaneId::Files);
    ctrl_w(&mut app, 'c');
    assert_eq!(tabs(&mut app), initial);

    window_prefix(&mut app);
    app.dispatch(&Event::FocusLost);
    assert!(app.key_sequence().is_none());
    app.dispatch(&key(KeyCode::Char('2')));
    app.dispatch(&key(KeyCode::Char('i')));
    window_prefix(&mut app);
    app.dispatch(&key(KeyCode::Char('l')));
    assert!(app.commit.editing() && app.key_sequence().is_none());
    app.dispatch(&key(KeyCode::Esc));
    assert_eq!(app.surface(), Split(Commit));
}

#[test]
fn split_workspace_shows_changes_and_history_together_at_each_size() {
    let (_dir, mut app) = fixture();
    for (w, h) in [(120, 36), (80, 25), (44, 12)] {
        let text = screen(&mut app, w, h);
        let lines: Vec<_> = text.lines().collect();
        let commit = lines.iter().position(|s| s.contains("Commit [2]")).unwrap();
        let log = lines
            .iter()
            .position(|s| s.contains("Git Log [1]"))
            .unwrap();
        assert!(
            (log - commit).abs_diff(lines.len() - 1 - log) <= 1,
            "{text}"
        );
        assert!(!lines[0].contains("Push") && !lines[0].contains("Merge"));
        assert!(
            lines[commit..log].iter().any(|s| s.contains("Changes")),
            "{text}"
        );
        assert!(lines[log..].iter().any(|s| s.contains("base")), "{text}");
        if w >= 80 {
            assert!(
                lines[commit..log].iter().any(|s| s.contains("mixed.txt")),
                "{text}"
            );
        }
    }
}

#[test]
fn split_workspace_tool_windows_block_background_and_restore_the_original_pane() {
    let (_dir, mut app) = fixture();
    let before = snapshot(&app.ctx.repos[0].path);
    let text = screen(&mut app, 120, 36);
    let top = text.lines().position(|s| s.contains("Commit [2]")).unwrap() as u16;
    let click = |row| {
        Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 1,
            row,
            modifiers: KeyModifiers::NONE,
        })
    };
    app.dispatch(&click(top));
    assert_eq!(app.help_content().context(), "Commit / 变更清单");
    for (open, expected) in [('3', "Push / 仓库与提交"), ('4', "Merge / 冲突文件")] {
        app.dispatch(&key(KeyCode::Char(open)));
        screen(&mut app, 120, 36);
        app.dispatch(&click(0));
        app.dispatch(&click(top));
        assert_eq!(app.help_content().context(), expected);
        app.dispatch(&key(KeyCode::F(1)));
        assert!(screen(&mut app, 120, 36).contains(&expected.to_uppercase()));
        app.dispatch(&key(KeyCode::Esc));
        assert_eq!(app.help_content().context(), expected);
        app.dispatch(&key(KeyCode::Esc));
        assert_eq!(app.help_content().context(), "Commit / 变更清单");
    }
    app.dispatch(&key(KeyCode::Char('1')));
    app.dispatch(&key(KeyCode::Char('3')));
    app.dispatch(&key(KeyCode::Char('d')));
    app.dispatch(&key(KeyCode::Esc)); // Close Details before the Push window.
    assert!(app.help_content().context().starts_with("Push /"));
    app.dispatch(&key(KeyCode::Esc));
    assert_eq!(app.help_content().context(), "Log / 提交列表");
    assert_eq!(snapshot(&app.ctx.repos[0].path), before);
}

#[test]
fn split_workspace_commit_and_push_refreshes_both_panes_and_keeps_reviews_separate() {
    let (dir, mut app) = fixture();
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
    for (name, value) in [
        ("user.name", "Split tester"),
        ("user.email", "split@example.invalid"),
        ("commit.gpgsign", "false"),
        ("core.hooksPath", "/dev/null"),
    ] {
        git(&repo, &["config", name, value]).unwrap();
    }
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
    app.dispatch(&key(KeyCode::Char('i')));
    app.dispatch(&Event::Paste("split-sync".into()));
    app.dispatch(&key(KeyCode::F(2)));
    app.dispatch(&key(KeyCode::Char('P')));
    assert!(
        matches!(&app.pop, Some(Popup::Confirm { owner: Split(Commit), body, .. }) if body.contains("mixed.txt") && !body.contains("other.txt"))
    );
    app.dispatch(&key(KeyCode::Char('y')));
    assert_eq!(app.surface(), Dialog(Push));
    assert!(screen(&mut app, 140, 36).contains("Push Commits"));
    app.dispatch(&key(KeyCode::Char('p')));
    assert!(matches!(
        &app.pop,
        Some(Popup::Confirm {
            owner: Dialog(Push),
            ..
        })
    ));
    app.dispatch(&key(KeyCode::Esc));
    assert_eq!(app.surface(), Dialog(Push));
    app.dispatch(&key(KeyCode::Esc));
    assert_eq!(app.surface(), Split(Commit));
    let text = screen(&mut app, 140, 36);
    assert!(
        text.lines()
            .skip(app.regions.log.y.into())
            .any(|s| s.contains("split-sync")),
        "{text}"
    );
    assert!(
        text.lines()
            .take(app.regions.log.y.into())
            .any(|s| s.contains("0 files selected")),
        "{text}"
    );
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
}
