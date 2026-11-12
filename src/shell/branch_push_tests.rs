use super::{test_support::*, *};
use crate::core::git;
use crossterm::event::MouseEvent;
use ratatui::style::Modifier;

fn push_fixture(source: &str, target: &str) -> (tempfile::TempDir, Shell) {
    let (temp, mut app) = fixture();
    let repo = &app.ctx.repos[0].path;
    let remote = temp.path().join("remote.git");
    std::fs::create_dir(&remote).unwrap();
    git(&remote, &["init", "--bare", "-q"]).unwrap();
    git(repo, &["remote", "add", "origin", remote.to_str().unwrap()]).unwrap();
    git(
        repo,
        &[
            "push",
            "origin",
            &format!("main:refs/heads/{target}"),
            "main:refs/heads/untouched",
        ],
    )
    .unwrap();
    git(
        repo,
        &["branch", "--track", source, &format!("origin/{target}")],
    )
    .unwrap();
    let tree = git(repo, &["rev-parse", "HEAD^{tree}"]).unwrap();
    let outgoing = git(
        repo,
        &["commit-tree", &tree, "-p", "HEAD", "-m", "outgoing"],
    )
    .unwrap();
    git(
        repo,
        &["update-ref", &format!("refs/heads/{source}"), &outgoing],
    )
    .unwrap();
    app.go(Branch);
    app.branch.finish_loading(&app.ctx);
    app.dispatch(&key(KeyCode::Char('j')));
    app.dispatch(&key(KeyCode::Char('P')));
    assert!(
        matches!(&app.pop, Some(Popup::Confirm { owner: Branch, .. })),
        "{}",
        app.notifications.latest_body()
    );
    (temp, app)
}

fn finish_push(app: &mut Shell) {
    assert!(app.branch.syncing(), "{}", app.notifications.latest_body());
    let deadline = Instant::now() + Duration::from_secs(10);
    while app.branch.syncing() {
        app.poll();
        assert!(
            Instant::now() < deadline,
            "push timed out: {}",
            app.notifications.latest_body()
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn click_button(app: &mut Shell, id: usize) {
    let rect = app.pop_hits.iter().find(|(_, hit)| *hit == id).unwrap().0;
    app.dispatch(&Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: rect.x + 1,
        row: rect.y,
        modifiers: KeyModifiers::NONE,
    }));
}

#[test]
fn branch_push_review_defaults_to_confirm_and_enter_pushes_only_reviewed_branch() {
    let (temp, mut app) = push_fixture("topic", "published-topic");
    let repo = app.ctx.repos[0].path.clone();
    let remote = temp.path().join("remote.git");
    let before = snapshot(&repo);
    let index = std::fs::read(repo.join(".git/index")).unwrap();
    let head = git(&repo, &["rev-parse", "HEAD"]).unwrap();
    let outgoing = git(&repo, &["rev-parse", "topic"]).unwrap();
    assert_ne!(head, outgoing);
    assert!(matches!(app.pop, Some(Popup::Confirm { yes: true, .. })));
    assert_eq!(
        git(&remote, &["rev-parse", "refs/heads/published-topic"]).unwrap(),
        head
    );
    app.dispatch(&key(KeyCode::Enter));
    assert!(app.pop.is_none());
    finish_push(&mut app);
    assert_eq!(
        git(&remote, &["rev-parse", "refs/heads/published-topic"]).unwrap(),
        outgoing
    );
    assert_eq!(
        git(&remote, &["rev-parse", "refs/heads/untouched"]).unwrap(),
        head
    );
    assert_eq!(snapshot(&repo), before);
    assert_eq!(std::fs::read(repo.join(".git/index")).unwrap(), index);
}

#[test]
fn branch_push_review_focus_cancel_help_and_letter_shortcuts() {
    let (temp, mut app) = push_fixture("topic", "published-topic");
    let repo = app.ctx.repos[0].path.clone();
    let remote = temp.path().join("remote.git");
    let before = snapshot(&repo);
    let refs = git(&remote, &["show-ref"]).unwrap();
    let index = std::fs::read(repo.join(".git/index")).unwrap();
    app.dispatch(&key(KeyCode::F(1)));
    assert!(screen(&mut app, 120, 36).contains("Confirm reviewed action"));
    app.dispatch(&key(KeyCode::Esc));
    assert!(matches!(app.pop, Some(Popup::Confirm { yes: true, .. })));
    for navigation in [
        KeyCode::Tab,
        KeyCode::BackTab,
        KeyCode::Left,
        KeyCode::Right,
    ] {
        app.dispatch(&key(navigation));
        assert!(matches!(app.pop, Some(Popup::Confirm { yes: false, .. })));
        app.dispatch(&key(navigation));
        assert!(matches!(app.pop, Some(Popup::Confirm { yes: true, .. })));
    }
    for cancel in [KeyCode::Esc, KeyCode::Char('n'), KeyCode::Enter] {
        if cancel == KeyCode::Enter {
            app.dispatch(&key(KeyCode::Tab));
        }
        app.dispatch(&key(cancel));
        assert!(app.pop.is_none() && !app.branch.syncing());
        assert_eq!(snapshot(&repo), before);
        assert_eq!(std::fs::read(repo.join(".git/index")).unwrap(), index);
        assert_eq!(git(&remote, &["show-ref"]).unwrap(), refs);
        app.dispatch(&key(KeyCode::Char('P')));
        assert!(matches!(app.pop, Some(Popup::Confirm { yes: true, .. })));
    }
    app.dispatch(&key(KeyCode::Char('y')));
    finish_push(&mut app);
    assert_eq!(
        git(&remote, &["rev-parse", "refs/heads/published-topic"]).unwrap(),
        git(&repo, &["rev-parse", "topic"]).unwrap()
    );
    assert_eq!(snapshot(&repo), before);
    assert_eq!(std::fs::read(repo.join(".git/index")).unwrap(), index);
}

#[test]
fn branch_push_review_layout_emphasizes_direction_and_keeps_mouse_controls_visible() {
    let (temp, mut app) = push_fixture("topic", "published-topic");
    let repo = app.ctx.repos[0].path.clone();
    let remote = temp.path().join("remote.git");
    let before = snapshot(&repo);
    let refs = git(&remote, &["show-ref"]).unwrap();
    let index = std::fs::read(repo.join(".git/index")).unwrap();
    for icons in [IconMode::Ascii, IconMode::Unicode, IconMode::NerdFont] {
        app.icons = icons;
        app.branch.icons = icons;
        for (width, height) in [(44, 12), (80, 24), (120, 36), (184, 48)] {
            let text = screen(&mut app, width, height);
            assert!(text.contains("From  topic"), "{text}");
            assert!(text.contains("To    origin/published-topic"), "{text}");
            if height >= 24 {
                for label in [
                    "Fast-forward only",
                    "No force push",
                    "Repository",
                    "Checkout  main",
                ] {
                    assert!(text.contains(label), "{label}\n{text}");
                }
                let head = git(&repo, &["rev-parse", "HEAD"]).unwrap();
                assert!(
                    text.contains(&head[..12]) && !text.contains(&head),
                    "{text}"
                );
            }
            for focused in [true, false] {
                let mut terminal =
                    Terminal::new(ratatui::backend::TestBackend::new(width, height)).unwrap();
                terminal.draw(|frame| app.draw(frame)).unwrap();
                let buffer = terminal.backend().buffer();
                for (id, label) in [(0, " n Cancel "), (1, " y Confirm ")] {
                    let rect = app.pop_hits.iter().find(|(_, hit)| *hit == id).unwrap().0;
                    assert_eq!(rect.width, label.len() as u16);
                    assert!(rect.right() <= width && rect.bottom() <= height);
                    assert_eq!(
                        buffer[(rect.x, rect.y)].bg,
                        if (id == 1) == focused { SEL } else { TAG }
                    );
                }
                for (label, prefix, color) in [
                    ("From  topic", "From  ", crate::ui::idea::LOCAL_REF),
                    (
                        "To    origin/published-topic",
                        "To    ",
                        crate::ui::idea::REMOTE_REF,
                    ),
                ] {
                    let (row, line) = text
                        .lines()
                        .enumerate()
                        .find(|(_, line)| line.contains(label))
                        .unwrap();
                    let column = line[..line.find(label).unwrap()].width() + prefix.width();
                    let cell = &buffer[(column as u16, row as u16)];
                    assert_eq!(cell.fg, color);
                    assert!(cell.modifier.contains(Modifier::BOLD));
                }
                if focused {
                    app.dispatch(&key(KeyCode::Tab));
                }
            }
            click_button(&mut app, 0);
            assert!(app.pop.is_none() && !app.branch.syncing());
            assert_eq!(snapshot(&repo), before);
            assert_eq!(std::fs::read(repo.join(".git/index")).unwrap(), index);
            assert_eq!(git(&remote, &["show-ref"]).unwrap(), refs);
            app.dispatch(&key(KeyCode::Char('P')));
        }
    }
    screen(&mut app, 80, 24);
    click_button(&mut app, usize::MAX);
    assert!(app.pop.is_none());
    assert_eq!(snapshot(&repo), before);
    assert_eq!(git(&remote, &["show-ref"]).unwrap(), refs);
    app.dispatch(&key(KeyCode::Char('P')));
    screen(&mut app, 80, 24);
    click_button(&mut app, 1);
    finish_push(&mut app);
    assert_eq!(
        git(&remote, &["rev-parse", "refs/heads/published-topic"]).unwrap(),
        git(&repo, &["rev-parse", "topic"]).unwrap()
    );
    assert_eq!(snapshot(&repo), before);
    assert_eq!(std::fs::read(repo.join(".git/index")).unwrap(), index);
}

#[test]
fn branch_push_review_wraps_unicode_and_scrolls_without_hiding_buttons() {
    let source = format!("feature-{}", "推送分支".repeat(12));
    let target = format!("publish-{}", "远端目标".repeat(12));
    let (temp, mut app) = push_fixture(&source, &target);
    let repo = app.ctx.repos[0].path.clone();
    let remote = temp.path().join("remote.git");
    let before = snapshot(&repo);
    let refs = git(&remote, &["show-ref"]).unwrap();
    let text = screen(&mut app, 44, 12);
    assert!(
        text.contains("From") && text.contains("feature-推送分支"),
        "{text}"
    );
    for _ in 0..60 {
        app.dispatch(&key(KeyCode::Char('j')));
    }
    let text = screen(&mut app, 44, 12);
    assert!(text.contains("Checkout  main"), "{text}");
    assert!(
        text.contains("Cancel") && text.contains("Confirm"),
        "{text}"
    );
    for _ in 0..60 {
        app.dispatch(&key(KeyCode::Char('k')));
    }
    let text = screen(&mut app, 44, 12);
    assert!(text.contains("feature-推送分支"), "{text}");
    assert!(matches!(
        app.pop,
        Some(Popup::Confirm {
            scroll: 0,
            yes: true,
            ..
        })
    ));
    screen(&mut app, 120, 36);
    app.dispatch(&key(KeyCode::Esc));
    assert!(app.pop.is_none() && !app.branch.syncing());
    assert_eq!(snapshot(&repo), before);
    assert_eq!(git(&remote, &["show-ref"]).unwrap(), refs);
}
