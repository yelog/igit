use super::{test_support::*, *};
use crate::core::git;
use crossterm::event::MouseEvent;
use std::fs;

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
