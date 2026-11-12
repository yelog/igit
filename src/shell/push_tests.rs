use super::{test_support::*, *};
use crate::core::git;

fn push_fixture() -> (tempfile::TempDir, Shell) {
    let (directory, mut app) = fixture();
    let remote = directory.path().join("remote.git");
    let repo = &app.ctx.repos[0].path;
    git(repo, &["init", "--bare", "-q", remote.to_str().unwrap()]).unwrap();
    git(repo, &["remote", "add", "origin", remote.to_str().unwrap()]).unwrap();
    app.go(Push);
    app.push.finish_loading(&app.ctx);
    (directory, app)
}

fn repository_checked(app: &mut Shell) -> bool {
    let rendered = screen(app, 120, 36);
    let row = rendered
        .lines()
        .find(|line| line.contains("origin:main"))
        .expect("Push must show the outgoing repository");
    row.contains(app.icons.check(true))
}

#[test]
fn push_shortcuts_distinguish_target_fetch_from_tags_hooks_and_global_fetch() {
    let (_directory, mut app) = push_fixture();
    let before = snapshot(&app.ctx.repos[0].path);
    for (character, label) in [('F', "[x] Follow tags"), ('H', "[ ] Hooks")] {
        app.dispatch(&key(KeyCode::Char(character)));
        assert!(app.pop.is_none() && !app.push.busy());
        assert!(screen(&mut app, 120, 36).contains(label));
    }
    app.dispatch(&key(KeyCode::Char('f')));
    assert!(app.pop.is_none() && app.push.busy());
    let deadline = Instant::now() + Duration::from_secs(10);
    while app.push.busy() || app.push.reads_pending() {
        app.poll();
        assert!(Instant::now() < deadline, "Push Fetch timed out");
        std::thread::yield_now();
    }
    assert!(repository_checked(&mut app));
    let rendered = screen(&mut app, 120, 36);
    assert!(rendered.contains(&format!("{} Follow tags", app.icons.check(true))));
    assert!(rendered.contains(&format!("{} Hooks", app.icons.check(false))));
    assert_eq!(snapshot(&app.ctx.repos[0].path), before);
    app.go(Commit);
    app.dispatch(&key(KeyCode::Char('f')));
    assert!(matches!(app.pop, Some(Popup::Update(_))));
    app.dispatch(&key(KeyCode::Esc));
    assert_eq!(snapshot(&app.ctx.repos[0].path), before);
}

#[test]
fn push_action_shortcuts_can_be_remapped_without_reaching_global_fetch() {
    let (_directory, original) = push_fixture();
    let config = AppConfig::from_toml(
        "[keybindings.push]\nfetch=['g f']\nfollow-tags=['T']\nskip-hooks=['K']\nreview=['Ctrl-p']",
    )
    .unwrap();
    let mut app = Shell::with_config(original.ctx.clone(), config);
    app.go(Push);
    app.push.finish_loading(&app.ctx);
    let before = snapshot(&app.ctx.repos[0].path);
    for character in ['f', 'F', 'H', 'P'] {
        app.dispatch(&key(KeyCode::Char(character)));
        assert!(app.pop.is_none() && !app.push.busy());
    }
    for character in ['T', 'K'] {
        app.dispatch(&key(KeyCode::Char(character)));
    }
    let rendered = screen(&mut app, 120, 36);
    assert!(rendered.contains(&format!("{} Follow tags", app.icons.check(true))));
    assert!(rendered.contains(&format!("{} Hooks", app.icons.check(false))));
    assert!(rendered.contains("g f Fetch"));
    for character in ['g', 'f'] {
        app.dispatch(&key(KeyCode::Char(character)));
    }
    assert!(app.pop.is_none() && app.push.busy());
    let deadline = Instant::now() + Duration::from_secs(10);
    while app.push.busy() || app.push.reads_pending() {
        app.poll();
        assert!(Instant::now() < deadline, "Remapped Push Fetch timed out");
        std::thread::yield_now();
    }
    app.dispatch(&key(KeyCode::Char('P')));
    assert!(app.pop.is_none());
    app.dispatch(&Event::Key(KeyEvent::new(
        KeyCode::Char('p'),
        KeyModifiers::CONTROL,
    )));
    assert!(matches!(app.pop, Some(Popup::Confirm { .. })));
    app.dispatch(&key(KeyCode::Esc));
    assert_eq!(snapshot(&app.ctx.repos[0].path), before);
}

#[cfg(unix)]
#[test]
fn push_animation_tracks_the_active_repository_and_requests_100ms_redraws() {
    use std::os::unix::fs::PermissionsExt;

    let (directory, mut app) = push_fixture();
    let remote = directory.path().join("remote.git");
    let hooks = remote.join("slow-hooks");
    std::fs::create_dir(&hooks).unwrap();
    let hook = hooks.join("pre-receive");
    std::fs::write(&hook, "#!/bin/sh\nsleep 1\n").unwrap();
    std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
    git(
        &remote,
        &["config", "core.hooksPath", hooks.to_str().unwrap()],
    )
    .unwrap();

    app.dispatch(&key(KeyCode::Char('P')));
    app.dispatch(&key(KeyCode::Enter));
    assert!(app.push.busy());
    let deadline = Instant::now() + Duration::from_secs(10);
    while !app.push.animation_running() {
        app.poll();
        assert!(
            Instant::now() < deadline,
            "push animation did not start: {}",
            app.notifications.latest_body()
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(app.poll_timeout() <= Duration::from_millis(100));
    let text = screen(&mut app, 120, 36);
    let row = text
        .lines()
        .find(|line| line.contains("origin:main"))
        .expect("active Push row");
    assert!(
        (0..8).any(|frame| row.contains(app.push.icons.spinner(frame))),
        "{text}"
    );

    while app.push.busy() {
        app.poll();
        assert!(Instant::now() < deadline, "push timed out");
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(!app.push.animation_running());
}

#[test]
fn push_space_toggles_immediately_and_movement_never_replays_it() {
    let (_directory, mut app) = push_fixture();
    let before = snapshot(&app.ctx.repos[0].path);
    assert!(repository_checked(&mut app));

    app.dispatch(&key(KeyCode::Char(' ')));
    assert!(!repository_checked(&mut app));
    app.dispatch(&key(KeyCode::Char('j')));
    assert!(!repository_checked(&mut app));
    app.dispatch(&key(KeyCode::Char('k')));
    assert!(!repository_checked(&mut app));

    for checked in [true, false, true, false] {
        app.dispatch(&key(KeyCode::Char(' ')));
        assert_eq!(repository_checked(&mut app), checked);
    }
    app.push.refresh(&app.ctx).unwrap();
    app.push.finish_loading(&app.ctx);
    assert!(!repository_checked(&mut app));
    app.go(Commit);
    app.go(Push);
    app.push.finish_loading(&app.ctx);
    assert!(!repository_checked(&mut app));
    app.dispatch(&key(KeyCode::Char('P')));
    assert!(app.pop.is_none());
    assert!(!app.push.busy());
    assert_eq!(snapshot(&app.ctx.repos[0].path), before);
}

#[test]
fn push_notification_chords_restore_only_their_own_checkbox_toggle() {
    for (configuration, suffix) in [
        ("", vec!['n']),
        (
            "[keybindings.global]\nnotification-history=['Space H x']",
            vec!['H', 'x'],
        ),
        (
            "[keybindings.global]\nnotification-history=['Space Space']",
            vec![' '],
        ),
    ] {
        let (_directory, mut app) = push_fixture();
        app.config = AppConfig::from_toml(configuration).unwrap();
        let before = snapshot(&app.ctx.repos[0].path);
        for checked in [true, false] {
            if repository_checked(&mut app) != checked {
                app.push.event(&key(KeyCode::Char(' ')), &app.ctx);
            }
            assert_eq!(repository_checked(&mut app), checked);
            app.dispatch(&key(KeyCode::Char(' ')));
            assert_eq!(repository_checked(&mut app), !checked);
            for character in &suffix {
                app.dispatch(&key(KeyCode::Char(*character)));
            }
            assert!(matches!(app.pop, Some(Popup::Notifications(_))));
            app.dispatch(&key(KeyCode::Esc));
            assert_eq!(repository_checked(&mut app), checked);
        }
        assert_eq!(snapshot(&app.ctx.repos[0].path), before);
    }
}

#[test]
fn push_space_respects_a_remapped_repository_toggle() {
    let (_directory, mut app) = push_fixture();
    app.config = AppConfig::from_toml("[keybindings.push]\ntoggle=['z']").unwrap();
    app.dispatch(&key(KeyCode::Char(' ')));
    assert!(repository_checked(&mut app));
    app.dispatch(&key(KeyCode::Char('j')));
    assert!(repository_checked(&mut app));
    app.dispatch(&key(KeyCode::Char('z')));
    assert!(!repository_checked(&mut app));
    app.dispatch(&key(KeyCode::Char(' ')));
    app.dispatch(&key(KeyCode::Char('n')));
    assert!(matches!(app.pop, Some(Popup::Notifications(_))));
    app.dispatch(&key(KeyCode::Esc));
    assert!(!repository_checked(&mut app));
}

#[test]
fn push_space_before_initial_loading_cannot_toggle_a_later_row() {
    let (_directory, mut app) = push_fixture();
    app.push = PushPanel::default();
    app.push.icons = app.icons;
    app.push.refresh(&app.ctx).unwrap();
    app.dispatch(&key(KeyCode::Char(' ')));
    app.push.finish_loading(&app.ctx);
    assert!(repository_checked(&mut app));
    app.dispatch(&key(KeyCode::Char('j')));
    assert!(repository_checked(&mut app));
}

#[test]
fn push_review_layout_matches_branch_and_keeps_controls_visible() {
    let (_directory, mut app) = push_fixture();
    let repo = app.ctx.repos[0].path.clone();
    let before = snapshot(&repo);
    let index = std::fs::read(repo.join(".git/index")).unwrap();
    let head = git(&repo, &["rev-parse", "HEAD"]).unwrap();
    app.dispatch(&key(KeyCode::Char('P')));
    assert!(matches!(app.pop, Some(Popup::Confirm { yes: true, .. })));
    for icons in [IconMode::Ascii, IconMode::Unicode, IconMode::NerdFont] {
        app.icons = icons;
        app.push.icons = icons;
        for (width, height) in [(44, 12), (80, 24), (120, 36), (184, 48)] {
            let text = screen(&mut app, width, height);
            assert!(text.contains("From  main"), "{text}");
            assert!(text.contains("To    origin/main"), "{text}");
            if height >= 36 {
                for label in [
                    &head[..12],
                    "Repository",
                    "Update: Merge",
                    "No force push",
                    "Hooks",
                    "Follow tags",
                ] {
                    assert!(text.contains(label), "{label}\n{text}");
                }
                assert!(!text.contains(&head), "{text}");
                let close = app
                    .pop_hits
                    .iter()
                    .find(|(_, id)| *id == usize::MAX)
                    .unwrap()
                    .0;
                let confirm = app.pop_hits.iter().find(|(_, id)| *id == 1).unwrap().0;
                assert!(confirm.y - close.y < 26, "{text}");
            }
            for focused in [true, false] {
                let mut terminal =
                    Terminal::new(ratatui::backend::TestBackend::new(width, height)).unwrap();
                terminal.draw(|frame| app.draw(frame)).unwrap();
                let buffer = terminal.backend().buffer();
                assert!(
                    buffer[(0, 1)]
                        .modifier
                        .contains(ratatui::style::Modifier::DIM),
                    "{text}"
                );
                let cancel = app.pop_hits.iter().find(|(_, id)| *id == 0).unwrap().0;
                let confirm = app.pop_hits.iter().find(|(_, id)| *id == 1).unwrap().0;
                assert!(cancel.right() < confirm.x && cancel.y == confirm.y);
                assert!(confirm.x > width / 2 && confirm.bottom() <= height);
                for (rect, selected) in [(cancel, !focused), (confirm, focused)] {
                    assert_eq!(
                        buffer[(rect.x, rect.y)].bg,
                        if selected { SEL } else { crate::ui::idea::TAG }
                    );
                    assert_eq!(buffer[(rect.x, rect.y - 1)].fg, LINE);
                }
                for (label, prefix, color) in [
                    ("From  main", "From  ", crate::ui::idea::LOCAL_REF),
                    ("To    origin/main", "To    ", crate::ui::idea::REMOTE_REF),
                ] {
                    let (row, line) = text
                        .lines()
                        .enumerate()
                        .find(|(_, line)| line.contains(label))
                        .unwrap();
                    let column = line[..line.find(label).unwrap()].width() + prefix.width();
                    let cell = &buffer[(column as u16, row as u16)];
                    assert_eq!(cell.fg, color);
                    assert!(cell.modifier.contains(ratatui::style::Modifier::BOLD));
                }
                app.dispatch(&key(KeyCode::Tab));
            }
        }
    }
    app.dispatch(&key(KeyCode::Esc));
    assert!(app.pop.is_none() && !app.push.busy());
    assert_eq!(snapshot(&repo), before);
    assert_eq!(std::fs::read(repo.join(".git/index")).unwrap(), index);
}

#[test]
fn push_review_cancel_controls_preserve_git_state_and_enter_publishes() {
    let (directory, mut app) = push_fixture();
    let repo = app.ctx.repos[0].path.clone();
    let remote = directory.path().join("remote.git");
    let before = snapshot(&repo);
    let index = std::fs::read(repo.join(".git/index")).unwrap();
    let refs = git(&remote, &["for-each-ref"]).unwrap();
    for button in [0, usize::MAX] {
        app.dispatch(&key(KeyCode::Char('P')));
        screen(&mut app, 80, 24);
        let rect = app.pop_hits.iter().find(|(_, id)| *id == button).unwrap().0;
        app.dispatch(&Event::Mouse(crossterm::event::MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: rect.x,
            row: rect.y,
            modifiers: KeyModifiers::NONE,
        }));
        assert!(app.pop.is_none() && !app.push.busy());
        assert_eq!(snapshot(&repo), before);
        assert_eq!(std::fs::read(repo.join(".git/index")).unwrap(), index);
        assert_eq!(git(&remote, &["for-each-ref"]).unwrap(), refs);
    }
    app.dispatch(&key(KeyCode::Char('P')));
    app.dispatch(&key(KeyCode::F(1)));
    assert!(!screen(&mut app, 120, 36).contains("Confirm reviewed action"));
    app.dispatch(&key(KeyCode::Esc));
    for letter in ['y', 'n'] {
        app.dispatch(&key(KeyCode::Char(letter)));
        assert!(matches!(app.pop, Some(Popup::Confirm { yes: true, .. })));
    }
    app.dispatch(&key(KeyCode::Tab));
    app.dispatch(&key(KeyCode::Enter));
    assert!(app.pop.is_none() && !app.push.busy());
    assert_eq!(snapshot(&repo), before);
    assert_eq!(std::fs::read(repo.join(".git/index")).unwrap(), index);
    assert_eq!(git(&remote, &["for-each-ref"]).unwrap(), refs);
    app.dispatch(&key(KeyCode::Char('P')));
    screen(&mut app, 80, 24);
    app.dispatch(&key(KeyCode::Enter));
    assert!(app.pop.is_none() && app.push.busy());
    let deadline = Instant::now() + Duration::from_secs(10);
    while app.push.busy() {
        app.poll();
        assert!(
            Instant::now() < deadline,
            "push timed out: {}",
            app.notifications.latest_body()
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(
        git(&remote, &["rev-parse", "refs/heads/main"]).unwrap(),
        git(&repo, &["rev-parse", "HEAD"]).unwrap()
    );
    assert_eq!(snapshot(&repo), before);
    assert_eq!(std::fs::read(repo.join(".git/index")).unwrap(), index);
}
