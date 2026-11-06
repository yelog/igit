use super::{test_support::*, *};
use crate::core::git;
use crossterm::event::MouseEvent;

#[test]
fn branch_tab_keyboard_mouse_help_and_name_input_keep_existing_global_keys() {
    let (_temp, mut app) = fixture();
    let repo = app.ctx.repos[0].path.clone();
    let before = snapshot(&repo);
    for (w, h) in [(120, 36), (80, 25), (44, 12)] {
        app.dispatch(&key(KeyCode::Char('5')));
        assert_eq!(app.tab, Branch);
        let text = screen(&mut app, w, h);
        assert!(text.lines().next().unwrap().contains("Branch"));
        assert!(app.regions.tab(Branch).width > 0);
        assert!(
            text.contains("Local Branches") && text.contains("Remote Branches"),
            "{text}"
        );
        assert_eq!(app.help_content().context(), "Branch / Branch Tree");
        assert!(!app
            .help_content()
            .sections
            .iter()
            .flat_map(|s| s.bindings)
            .any(|(_, label)| *label == "Open Roots repo scope"));
        app.dispatch(&key(KeyCode::Char('1')));
        let rect = app.regions.tab(Branch);
        app.dispatch(&Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: rect.x,
            row: rect.y,
            modifiers: KeyModifiers::NONE,
        }));
        assert_eq!(app.tab, Branch);
    }
    app.dispatch(&key(KeyCode::Char('r')));
    assert!(app.branch.editing());
    assert!(app.pop.is_none(), "r must rename, not open Roots");
    app.dispatch(&key(KeyCode::Char('1')));
    assert_eq!(app.tab, Branch, "digits belong to the branch name input");
    app.dispatch(&key(KeyCode::F(1)));
    assert!(matches!(app.pop, Some(Popup::Help { .. })));
    app.dispatch(&key(KeyCode::Esc));
    app.dispatch(&key(KeyCode::Esc));
    assert!(!app.branch.editing());
    app.dispatch(&key(KeyCode::Char(':')));
    let roots = app
        .menu_actions()
        .position(|action| matches!(action, MenuAction::Roots))
        .unwrap();
    app.menu_action(roots);
    assert!(matches!(app.pop, Some(Popup::Scope { .. })));
    app.dispatch(&key(KeyCode::Esc));
    assert_eq!(snapshot(&repo), before);
}

#[test]
fn branch_review_cancel_and_confirm_refresh_current_branch_without_staging_changes() {
    let (_temp, mut app) = fixture();
    let repo = app.ctx.repos[0].path.clone();
    let before = snapshot(&repo);
    app.go(Branch);
    app.dispatch(&key(KeyCode::Char('a')));
    app.dispatch(&Event::Paste("feature/new".into()));
    app.dispatch(&key(KeyCode::Enter));
    assert!(matches!(
        app.pop,
        Some(Popup::Confirm {
            owner: Branch,
            yes: false,
            ..
        })
    ));
    app.dispatch(&key(KeyCode::Esc));
    assert!(git(&repo, &["show-ref", "--verify", "refs/heads/feature/new"]).is_err());
    app.dispatch(&key(KeyCode::Char('r')));
    app.dispatch(&Event::Key(KeyEvent::new(
        KeyCode::Char('u'),
        KeyModifiers::CONTROL,
    )));
    app.dispatch(&Event::Paste("feature/renamed".into()));
    app.dispatch(&key(KeyCode::Enter));
    app.dispatch(&key(KeyCode::Char('y')));
    assert_eq!(app.ctx.repos[0].branch, "feature/renamed");
    assert!(screen(&mut app, 80, 25).contains("renamed (current)"));
    assert_eq!(
        snapshot(&repo),
        before,
        "rename must keep HEAD, index and working changes"
    );
    app.go(Push);
    app.dispatch(&key(KeyCode::Esc));
    assert_eq!(app.tab, Branch);
}

#[test]
fn branch_commands_can_be_remapped_without_leaking_into_name_input_or_roots() {
    let (_temp, fixture) = fixture();
    let config = AppConfig::from_toml("[keybindings.global]\nfocus-branch=['F8']\n[keybindings.branch]\nrename=['R']\ncreate=['n']").unwrap();
    let mut app = Shell::with_config(fixture.ctx.clone(), config);
    app.dispatch(&key(KeyCode::Char('5')));
    assert_eq!(app.tab, Commit);
    app.dispatch(&key(KeyCode::F(8)));
    assert_eq!(app.tab, Branch);
    app.dispatch(&key(KeyCode::Char('r')));
    assert!(!app.branch.editing() && app.pop.is_none());
    app.dispatch(&key(KeyCode::Char('R')));
    assert!(app.branch.editing());
    app.dispatch(&key(KeyCode::Esc));
    let text = screen(&mut app, 120, 36);
    assert!(text.contains("R Rename") && text.contains("n New"));
    app.go(Commit);
    app.dispatch(&key(KeyCode::Char('r')));
    assert!(matches!(app.pop, Some(Popup::Scope { .. })));

    let config = AppConfig::from_toml("[keybindings.global]\nroots=['F9']").unwrap();
    let mut app = Shell::with_config(fixture.ctx.clone(), config);
    app.go(Branch);
    assert!(screen(&mut app, 120, 36).contains("r Rename"));
    app.dispatch(&key(KeyCode::Char('r')));
    assert!(app.branch.editing());
}

#[test]
fn branch_merge_conflicts_refresh_the_conflicts_tab_and_return_to_branch() {
    let (_temp, mut app) = fixture();
    let repo = app.ctx.repos[0].path.clone();
    merge_conflict(&repo);
    git(&repo, &["merge", "--abort"]).unwrap();
    app.go(Branch);
    assert!(!app.tab_available(Conflicts));
    app.dispatch(&key(KeyCode::Char('j'))); // main (current) -> incoming
    app.dispatch(&key(KeyCode::Char('m')));
    let Some(Popup::Confirm {
        body,
        owner: Branch,
        ..
    }) = &app.pop
    else {
        panic!(
            "Expected Branch merge review: {}",
            screen(&mut app, 120, 36)
        );
    };
    assert!(body.contains("Merge source: incoming") && body.contains("Into current branch: main"));
    app.dispatch(&key(KeyCode::Char('y')));
    assert_eq!(app.tab, Branch);
    assert!(app.tab_available(Conflicts));
    assert!(app.msg.contains("CONFLICT"), "{}", app.msg);
    assert!(!git(&repo, &["ls-files", "--unmerged"]).unwrap().is_empty());
    app.dispatch(&key(KeyCode::Char('4')));
    assert_eq!(app.tab, Conflicts);
    app.dispatch(&key(KeyCode::Esc));
    assert_eq!(app.tab, Branch);
}

#[test]
fn branch_refresh_reads_external_refs_without_refreshing_unopened_history() {
    let (_temp, fixture) = fixture();
    let mut app = Shell::unloaded(fixture.ctx.clone(), AppConfig::default());
    app.go(Branch);
    assert!(matches!(app.log_load, startup::LogLoad::Dormant));
    let repo = &app.ctx.repos[0].path;
    git(repo, &["remote", "add", "origin", "../unused-local-remote"]).unwrap();
    git(repo, &["update-ref", "refs/remotes/origin/fetched", "HEAD"]).unwrap();
    app.dispatch(&Event::FocusGained);
    assert!(screen(&mut app, 120, 36).contains("fetched"));
    assert!(matches!(app.log_load, startup::LogLoad::Dormant));
}
