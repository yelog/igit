use super::{test_support::*, *};
use crate::config::keys::parse_sequence;
use crate::core::git;
use crossterm::event::MouseEvent;
use std::fs;

fn send(app: &mut Shell, sequence: &str) {
    for key in parse_sequence(sequence).unwrap() {
        app.dispatch(&Event::Key(key));
    }
}

fn edit_field(app: &mut Shell, text: &str) {
    send(app, "Ctrl-u");
    app.dispatch(&Event::Paste(text.into()));
}

fn add_remote(app: &mut Shell) {
    send(app, "6 a");
    edit_field(app, "origin");
    send(app, "Tab");
    edit_field(app, "../local remote.git");
    send(app, "Ctrl-s");
}

#[test]
fn remote_tab_shortcut_mouse_and_help_work_without_changing_git_state() {
    let (_dir, mut app) = fixture();
    let before = snapshot(&app.ctx.repos[0].path);
    for (width, height) in [(120, 36), (80, 24), (44, 12)] {
        send(&mut app, "6");
        assert_eq!(app.tab, Remote);
        assert_eq!(app.main_tab, Remote);
        let text = screen(&mut app, width, height);
        assert!(
            text.contains("No remotes") && text.contains("Name"),
            "{text}"
        );
        assert_eq!(app.help_content().context(), "Remote / Remotes");
        send(&mut app, "1");
        screen(&mut app, width, height);
        let rect = app.regions.tab(Remote);
        assert!(!rect.is_empty());
        app.dispatch(&Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: rect.x,
            row: rect.y,
            modifiers: KeyModifiers::NONE,
        }));
        assert_eq!(app.tab, Remote);
        send(&mut app, "F1");
        app.dispatch(&Event::Paste("Add remote to selected repository".into()));
        send(&mut app, "Enter");
        assert!(app.remote.editing());
        assert_eq!(app.help_content().context(), "Remote / Editor");
        send(&mut app, "Esc");
    }
    assert_eq!(snapshot(&app.ctx.repos[0].path), before);
}

#[test]
fn remote_add_cancel_and_confirm_preserve_mixed_staging_and_index_bytes() {
    let (_dir, mut app) = fixture();
    let repo = app.ctx.repos[0].path.clone();
    let before = snapshot(&repo);
    let index = fs::read(repo.join(".git/index")).unwrap();
    add_remote(&mut app);
    assert!(matches!(
        app.pop,
        Some(Popup::Confirm {
            owner: Remote,
            yes: false,
            ..
        })
    ));
    assert!(git(&repo, &["remote"]).unwrap().is_empty());
    send(&mut app, "Esc");
    assert!(app.remote.editing());
    send(&mut app, "Ctrl-s");
    assert!(matches!(
        app.pop,
        Some(Popup::Confirm { owner: Remote, .. })
    ));
    send(&mut app, "y");
    assert!(app.pop.is_none() && !app.remote.editing());
    assert_eq!(
        git(&repo, &["remote", "get-url", "origin"]).unwrap(),
        "../local remote.git"
    );
    assert_eq!(snapshot(&repo), before);
    assert_eq!(fs::read(repo.join(".git/index")).unwrap(), index);
    send(&mut app, "d");
    assert!(matches!(
        app.pop,
        Some(Popup::Confirm { owner: Remote, .. })
    ));
    send(&mut app, "Esc");
    assert_eq!(git(&repo, &["remote"]).unwrap(), "origin");
}

#[test]
fn remapped_remote_commands_and_editor_input_retain_their_own_keys() {
    let (_dir, mut app) = fixture();
    app.config = AppConfig::from_toml(
        "[keybindings.global]\nfocus-remote=['F8']\n[keybindings.remote]\nadd=['A']\nedit=['E']\nremove=['D']",
    ).unwrap();
    send(&mut app, "6");
    assert_eq!(app.tab, Commit);
    send(&mut app, "F8");
    assert_eq!(app.tab, Remote);
    send(&mut app, "a");
    assert!(!app.remote.editing());
    send(&mut app, "A");
    assert!(app.remote.editing());
    edit_field(&mut app, "q7");
    send(&mut app, "Tab");
    edit_field(&mut app, "../local.git");
    send(&mut app, "F1");
    app.dispatch(&Event::Paste("Review remote changes".into()));
    send(&mut app, "Enter");
    assert!(matches!(
        app.pop,
        Some(Popup::Confirm { owner: Remote, .. })
    ));
    assert!(!app.quit);
    send(&mut app, "y");
    assert_eq!(git(&app.ctx.repos[0].path, &["remote"]).unwrap(), "q7");
    let text = screen(&mut app, 120, 36);
    assert!(
        text.contains("A Add") && text.contains("E Edit") && text.contains("D Remove"),
        "{text}"
    );
    send(&mut app, "D y");
    assert!(git(&app.ctx.repos[0].path, &["remote"]).unwrap().is_empty());
}

#[test]
fn roots_refresh_and_stale_review_never_modify_a_different_repository() {
    let (dir, _) = fixture();
    let second = dir.path().join("second");
    fs::create_dir(&second).unwrap();
    git(&second, &["init", "-q", "-b", "main"]).unwrap();
    let mut ctx = Context::discover(dir.path()).unwrap();
    ctx.state_dir = dir.path().join("state");
    let mut app = Shell::new(ctx, IconMode::Ascii);
    send(&mut app, "6");
    let first = app.ctx.repos[0].path.clone();
    send(&mut app, "a Tab");
    edit_field(&mut app, "../reviewed.git");
    send(&mut app, "Ctrl-s");
    let first_config = fs::read(first.join(".git/config")).unwrap();
    app.ctx.scope = [1].into();
    send(&mut app, "y");
    assert_eq!(fs::read(first.join(".git/config")).unwrap(), first_config);
    assert!(git(&app.ctx.repos[1].path, &["remote"]).unwrap().is_empty());
    send(&mut app, "Esc F5");
    let text = screen(&mut app, 120, 36);
    assert!(text.contains("1 repository"), "{text}");
}

#[test]
fn remote_review_is_blocked_during_git_execution_and_conflicts_keeps_key_seven() {
    let (_dir, mut app) = fixture();
    let complete = app.commit.hold_execution();
    add_remote(&mut app);
    assert!(app.pop.is_none());
    assert!(app.notifications.latest_body().contains("Wait for"));
    assert!(git(&app.ctx.repos[0].path, &["remote"]).unwrap().is_empty());
    send(&mut app, "Esc");
    complete();
    app.poll();
    merge_conflict(&app.ctx.repos[0].path);
    app.refresh();
    for (width, height) in [(120, 36), (80, 24), (44, 12)] {
        let text = screen(&mut app, width, height);
        let header = text.lines().next().unwrap();
        assert!(header.contains("Conflicts"), "{header}");
        for tab in Tab::DISPLAY_ORDER {
            let rect = app.regions.tab(tab);
            assert!(
                !rect.is_empty() && rect.right() <= width,
                "{tab:?}: {header}"
            );
        }
        send(&mut app, "7");
        assert_eq!(app.tab, Conflicts);
        send(&mut app, "Esc");
        assert_eq!(app.tab, Remote);
    }
}
