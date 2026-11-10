use super::{test_support::*, *};
use crate::{core::git, ui::panes::PaneId};
use crossterm::event::MouseEvent;

fn ready(app: &mut Shell) {
    app.branch.finish_loading(&app.ctx);
}

#[test]
fn branch_layout_tracks_roots_and_keeps_actions_on_the_selected_repository() {
    let (temp, single) = fixture();
    let first = single.ctx.repos[0].path.clone();
    let second = first.parent().unwrap().join("tools");
    std::fs::create_dir(&second).unwrap();
    git(&second, &["init", "-q", "-b", "main"]).unwrap();
    git(
        &second,
        &[
            "-c",
            "user.name=Branch tester",
            "-c",
            "user.email=branch@example.invalid",
            "-c",
            "core.hooksPath=/dev/null",
            "commit",
            "--no-gpg-sign",
            "--allow-empty",
            "-qm",
            "second base",
        ],
    )
    .unwrap();
    git(&first, &["branch", "first-only"]).unwrap();
    git(&second, &["branch", "second-only"]).unwrap();
    let first_before = snapshot(&first);
    let second_before = snapshot(&second);
    let mut ctx = Context::discover(temp.path()).unwrap();
    ctx.state_dir = temp.path().join("branch-layout-state");
    let mut app = Shell::unloaded(
        ctx,
        AppConfig {
            icons: IconMode::Ascii,
            ..AppConfig::default()
        },
    );
    app.go(Branch);
    ready(&mut app);
    for (w, h) in [(120, 36), (80, 25), (44, 12)] {
        let initial = screen(&mut app, w, h);
        assert!(!initial.contains("second-only"), "{initial}");
        assert!(
            initial.contains("first-only") && initial.contains("Repositories"),
            "{initial}"
        );
        assert!(
            initial.contains("R repo") && initial.contains("R tools"),
            "{initial}"
        );
        let left = app.branch.panes.rect(PaneId::BranchRepos).unwrap();
        let right = app.branch.panes.rect(PaneId::Branches).unwrap();
        assert_eq!(left.right(), right.x);
    }
    app.dispatch(&key(KeyCode::Tab));
    assert_eq!(app.help_content().context(), "Branch / Repositories");
    app.dispatch(&key(KeyCode::Char('j')));
    let other = screen(&mut app, 120, 36);
    assert!(other.contains("second-only") && !other.contains("first-only"));
    app.dispatch(&key(KeyCode::Char('a')));
    assert!(!app.branch.editing() && app.pop.is_none());
    app.dispatch(&key(KeyCode::Enter));
    assert_eq!(app.help_content().context(), "Branch / Branch Tree");
    app.dispatch(&key(KeyCode::Char('a')));
    app.dispatch(&Event::Paste("from-tools".into()));
    app.dispatch(&key(KeyCode::Enter));
    let Some(Popup::Confirm { body, .. }) = app.pop.as_ref() else {
        panic!("Expected review: {}", app.notifications.latest_body());
    };
    assert!(body.contains(&format!("Path: {}", second.display())));
    app.dispatch(&key(KeyCode::Char('y')));
    assert!(git(&first, &["show-ref", "--verify", "refs/heads/from-tools"]).is_err());
    assert_eq!(
        git(&second, &["rev-parse", "from-tools"]).unwrap(),
        second_before[0]
    );
    assert_eq!(snapshot(&first), first_before);
    assert_eq!(snapshot(&second), second_before);
    ready(&mut app);
    screen(&mut app, 80, 25);
    let left = app.branch.panes.rect(PaneId::BranchRepos).unwrap();
    app.dispatch(&Event::Mouse(MouseEvent {
        kind: MouseEventKind::ScrollUp,
        column: left.x + 2,
        row: left.y + 2,
        modifiers: KeyModifiers::NONE,
    }));
    let first_again = screen(&mut app, 80, 25);
    assert!(first_again.contains("first-only") && !first_again.contains("second-only"));
    app.dispatch(&Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: left.x + 2,
        row: left.y + 3,
        modifiers: KeyModifiers::NONE,
    }));
    let second_again = screen(&mut app, 80, 25);
    assert!(second_again.contains("second-only") && !second_again.contains("first-only"));
    assert_eq!(app.help_content().context(), "Branch / Repositories");

    app.global(0); // Roots
    app.dispatch(&key(KeyCode::Char(' ')));
    app.dispatch(&key(KeyCode::Enter));
    assert_eq!(app.ctx.scope, BTreeSet::from([1]));
    ready(&mut app);
    let only = screen(&mut app, 120, 36);
    assert!(only.contains("second-only"));
    assert!(!only.contains("Repositories") && !only.contains("R tools"));
    assert!(!only.contains(second.to_str().unwrap()));
    assert!(app.branch.panes.rect(PaneId::BranchRepos).is_none());
    assert_eq!(app.help_content().context(), "Branch / Branch Tree");
}

#[test]
fn branch_entry_paints_before_git_and_leaves_navigation_available() {
    let (_temp, mut app) = fixture();
    app.dispatch(&key(KeyCode::Char('4')));
    assert_eq!(app.tab, Branch);
    for (width, height) in [(120, 36), (80, 25), (44, 12)] {
        let first_frame = screen(&mut app, width, height);
        assert!(first_frame.contains("Loading branches"), "{first_frame}");
        assert!(!first_frame.contains("Local Branches"), "{first_frame}");
    }
    let release = app.branch.hold_loading(&app.ctx);
    app.poll();
    assert!(app.poll_timeout() <= Duration::from_millis(16));
    app.dispatch(&key(KeyCode::F(1)));
    assert!(matches!(app.pop, Some(Popup::Help { .. })));
    app.dispatch(&key(KeyCode::Esc));
    app.dispatch(&key(KeyCode::Char('1')));
    assert_eq!(app.tab, Commit);
    app.dispatch(&key(KeyCode::Char('4')));
    assert!(screen(&mut app, 80, 25).contains("Loading branches"));
    release();
    assert!(app.poll(), "the completed read should request a redraw");
    assert!(
        !app.branch.reads_pending(),
        "tab switching must not restart the read"
    );
    assert!(screen(&mut app, 80, 25).contains("Local Branches"));
}

#[test]
fn branch_refresh_keeps_cached_browsing_and_the_latest_fold_but_gates_actions() {
    let (_temp, mut app) = fixture();
    let repo = app.ctx.repos[0].path.clone();
    git(&repo, &["update-ref", "refs/remotes/origin/seen", "HEAD"]).unwrap();
    app.go(Branch);
    ready(&mut app);
    app.go(Commit);
    git(&repo, &["branch", "new-since-last-visit"]).unwrap();
    let before = snapshot(&repo);
    app.go(Branch);
    let release = app.branch.hold_loading(&app.ctx);
    let cached = screen(&mut app, 120, 36);
    assert!(cached.contains("Refreshing branches") && cached.contains("main (current)"));
    assert!(cached.contains("seen"));
    assert!(!cached.contains("new-since-last-visit"));
    app.dispatch(&key(KeyCode::Char('a')));
    assert!(!app.branch.editing() && app.pop.is_none());
    assert!(app
        .notifications
        .latest_body()
        .contains("Refreshing branches"));
    app.dispatch(&key(KeyCode::Char('j'))); // main -> Remote Branches
    app.dispatch(&key(KeyCode::Char('h')));
    let folded = screen(&mut app, 120, 36);
    assert!(folded.contains("Remote Branches  1 branches") && !folded.contains("seen"));
    release();
    assert!(app.poll());
    app.notifications
        .expire(Instant::now() + Duration::from_secs(9));
    let fresh = screen(&mut app, 120, 36);
    assert!(fresh.contains("new-since-last-visit"));
    assert!(fresh.contains("Remote Branches  1 branches") && !fresh.contains("seen"));
    assert!(!fresh.contains("Refreshing branches"));
    assert_eq!(snapshot(&repo), before);
}

#[test]
fn branch_loading_rejects_old_roots_even_if_scope_changes_on_another_tab() {
    let (temp, fixture) = fixture();
    let repo = &fixture.ctx.repos[0].path;
    let worktree = temp.path().join("tools");
    git(
        repo,
        &[
            "-c",
            "core.hooksPath=/dev/null",
            "worktree",
            "add",
            "-b",
            "visible-only",
            worktree.to_str().unwrap(),
        ],
    )
    .unwrap();
    let mut ctx = Context::discover(temp.path()).unwrap();
    ctx.state_dir = temp.path().join("branch-test-state");
    assert_eq!(ctx.scope.len(), 2);
    assert_eq!(ctx.repos[0].path, *repo);
    let mut app = Shell::unloaded(
        ctx,
        AppConfig {
            icons: IconMode::Ascii,
            ..AppConfig::default()
        },
    );
    app.go(Branch);
    let release = app.branch.hold_loading(&app.ctx);
    app.go(Commit);
    app.dispatch(&key(KeyCode::Char('r')));
    app.dispatch(&key(KeyCode::Char(' ')));
    app.dispatch(&key(KeyCode::Enter));
    assert_eq!(app.ctx.scope, BTreeSet::from([1]));
    app.go(Branch);
    release();
    app.poll();
    let pending = screen(&mut app, 120, 36);
    assert!(pending.contains("Loading branches") && !pending.contains("Repositories"));
    ready(&mut app);
    let fresh = screen(&mut app, 120, 36);
    assert!(fresh.contains("visible-only (current)") && !fresh.contains("Repositories"));
}

#[test]
fn branch_gg_hint_cancels_on_context_changes_and_completes_without_git_writes() {
    let (_temp, mut app) = fixture();
    app.go(Branch);
    ready(&mut app);
    let before = snapshot(&app.ctx.repos[0].path);
    app.dispatch(&key(KeyCode::Char('G')));
    let last = screen(&mut app, 80, 25);
    app.dispatch(&key(KeyCode::Char('g')));
    assert_eq!(app.key_sequence(), Some(KeySequence::BranchGoto));
    assert_eq!(screen(&mut app, 80, 25), last);
    app.key_picker
        .poll(Instant::now() + Duration::from_millis(200));
    assert!(screen(&mut app, 80, 25).contains("Go to first row"));
    app.dispatch(&key(KeyCode::Enter));
    assert!(app.key_sequence().is_none());
    assert_ne!(screen(&mut app, 80, 25), last);
    for interrupt in [
        key(KeyCode::Esc),
        key(KeyCode::F(1)),
        key(KeyCode::F(5)),
        key(KeyCode::Char('1')),
        Event::Resize(80, 25),
    ] {
        app.go(Branch);
        ready(&mut app);
        app.dispatch(&key(KeyCode::Char('g')));
        app.dispatch(&interrupt);
        assert!(app.key_sequence().is_none(), "{interrupt:?}");
        app.dispatch(&key(KeyCode::Esc));
    }
    app.go(Branch);
    ready(&mut app);
    for ch in ['f', 'b', 'd', 'u'] {
        app.dispatch(&Event::Key(KeyEvent::new(
            KeyCode::Char(ch),
            KeyModifiers::CONTROL,
        )));
        assert!(app.pop.is_none() && !app.branch.editing());
    }
    assert_eq!(snapshot(&app.ctx.repos[0].path), before);
}

#[test]
fn branch_navigation_remapping_updates_help_and_executes_sequences() {
    let (_temp, fixture) = fixture();
    let config = AppConfig::from_toml(
        "[keybindings.global]\nupdate=[]\nnotification-history=[]\n[keybindings.branch]\nfirst-item=['F9']\nhalf-page-down=['J']\nscreen-middle=['F8']",
    )
    .unwrap();
    let mut app = Shell::with_config(fixture.ctx.clone(), config);
    app.go(Branch);
    ready(&mut app);
    app.dispatch(&key(KeyCode::Char('G')));
    let last = screen(&mut app, 80, 25);
    app.dispatch(&key(KeyCode::Char('g')));
    app.dispatch(&key(KeyCode::Char('g')));
    assert_eq!(screen(&mut app, 80, 25), last);
    app.dispatch(&key(KeyCode::F(9)));
    assert_ne!(screen(&mut app, 80, 25), last);
    assert!(app.key_sequence().is_none());
    app.dispatch(&key(KeyCode::F(1)));
    for (query, binding) in [
        ("Select first row", "F9"),
        ("Move down half a page", "J"),
        ("Select middle visible row", "F8"),
    ] {
        app.dispatch(&Event::Key(KeyEvent::new(
            KeyCode::Char('u'),
            KeyModifiers::CONTROL,
        )));
        app.dispatch(&Event::Paste(query.into()));
        let help = screen(&mut app, 120, 36);
        assert!(help.contains(binding) && help.contains(query), "{help}");
    }
    app.dispatch(&key(KeyCode::Esc));
    app.dispatch(&key(KeyCode::Char('J')));
    app.dispatch(&key(KeyCode::F(8)));
    assert!(app.pop.is_none() && !app.branch.editing());
}

#[test]
fn branch_tab_keyboard_mouse_help_and_name_input_keep_existing_global_keys() {
    let (_temp, mut app) = fixture();
    let repo = app.ctx.repos[0].path.clone();
    let before = snapshot(&repo);
    for (w, h) in [(120, 36), (80, 25), (44, 12)] {
        app.dispatch(&key(KeyCode::Char('4')));
        assert_eq!(app.tab, Branch);
        ready(&mut app);
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
    ready(&mut app);
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
    ready(&mut app);
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
    ready(&mut app);
    app.notifications
        .expire(Instant::now() + Duration::from_secs(9));
    assert!(screen(&mut app, 80, 25).contains("renamed (current)"));
    assert_eq!(
        snapshot(&repo),
        before,
        "rename must keep HEAD, index and working changes"
    );
    app.go(Push);
    click_push_cancel(&mut app, 80, 24);
    assert_eq!(app.tab, Branch);
}

#[test]
fn branch_commands_can_be_remapped_without_leaking_into_name_input_or_roots() {
    let (_temp, fixture) = fixture();
    let config = AppConfig::from_toml(
        "[keybindings.global]\nupdate=[]\nnotification-history=[]\nfocus-branch=['F8']\n[keybindings.branch]\nrename=['R']\ncreate=['n']\nnext-match=['F7']\nview-options=['F9']",
    )
    .unwrap();
    let mut app = Shell::with_config(fixture.ctx.clone(), config);
    app.dispatch(&key(KeyCode::Char('4')));
    assert_eq!(app.tab, Commit);
    app.dispatch(&key(KeyCode::F(8)));
    assert_eq!(app.tab, Branch);
    ready(&mut app);
    app.dispatch(&key(KeyCode::Char('r')));
    assert!(!app.branch.editing() && app.pop.is_none());
    app.dispatch(&key(KeyCode::Char('R')));
    assert!(app.branch.editing());
    app.dispatch(&key(KeyCode::Esc));
    let text = screen(&mut app, 120, 36);
    assert!(text.contains("R Rename") && text.contains("n New"));
    app.dispatch(&key(KeyCode::Char('o')));
    assert!(!app.branch.editing());
    app.dispatch(&key(KeyCode::F(9)));
    assert_eq!(app.help_content().context(), "Branch / Group By");
    app.dispatch(&key(KeyCode::Char('n')));
    assert_eq!(app.help_content().context(), "Branch / Group By");
    screen(&mut app, 120, 36);
    let commit = app.regions.tab(Commit);
    app.dispatch(&Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: commit.x + 1,
        row: commit.y,
        modifiers: KeyModifiers::NONE,
    }));
    assert_eq!(app.tab, Commit);
    assert!(!app.branch.editing());
    app.dispatch(&key(KeyCode::Char('r')));
    assert!(matches!(app.pop, Some(Popup::Scope { .. })));

    let config = AppConfig::from_toml("[keybindings.global]\nupdate=[]\nroots=['F9']").unwrap();
    let mut app = Shell::with_config(fixture.ctx.clone(), config);
    app.go(Branch);
    ready(&mut app);
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
    ready(&mut app);
    assert!(!app.tab_available(Conflicts));
    app.dispatch(&key(KeyCode::Char('j'))); // main (current) -> incoming
    app.dispatch(&key(KeyCode::Char('m')));
    let Some(Popup::Confirm {
        body,
        owner: Branch,
        yes: true,
        ..
    }) = &app.pop
    else {
        panic!(
            "Expected Branch merge review: {}",
            screen(&mut app, 120, 36)
        );
    };
    assert!(body.contains("Merge source: incoming") && body.contains("Into current branch: main"));
    app.dispatch(&key(KeyCode::Enter));
    assert_eq!(app.tab, Branch);
    assert!(app.tab_available(Conflicts));
    assert!(
        app.notifications.latest_body().contains("CONFLICT"),
        "{}",
        app.notifications.latest_body()
    );
    assert!(!git(&repo, &["ls-files", "--unmerged"]).unwrap().is_empty());
    app.dispatch(&key(KeyCode::Char('6')));
    assert_eq!(app.tab, Conflicts);
    app.dispatch(&key(KeyCode::Esc));
    assert_eq!(app.tab, Branch);
}

#[test]
fn branch_merge_review_focus_letters_and_cancel_preserve_git_state() {
    let (_temp, mut app) = fixture();
    let repo = app.ctx.repos[0].path.clone();
    merge_conflict(&repo);
    git(&repo, &["merge", "--abort"]).unwrap();
    let before = snapshot(&repo);
    app.go(Branch);
    ready(&mut app);
    app.dispatch(&key(KeyCode::Char('j')));
    app.dispatch(&key(KeyCode::Char('m')));
    assert!(matches!(app.pop, Some(Popup::Confirm { yes: true, .. })));
    for letter in ['n', 'y'] {
        app.dispatch(&key(KeyCode::Char(letter)));
        assert!(matches!(app.pop, Some(Popup::Confirm { yes: true, .. })));
        assert_eq!(snapshot(&repo), before);
    }
    app.dispatch(&key(KeyCode::F(1)));
    let help = screen(&mut app, 120, 36);
    assert!(!help.contains("Review shortcuts"), "{help}");
    app.dispatch(&key(KeyCode::Esc));
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
    app.dispatch(&key(KeyCode::Tab));
    app.dispatch(&key(KeyCode::Enter));
    assert!(app.pop.is_none());
    assert_eq!(snapshot(&repo), before);
    app.dispatch(&key(KeyCode::Char('m')));
    assert!(matches!(app.pop, Some(Popup::Confirm { yes: true, .. })));
    app.dispatch(&key(KeyCode::Esc));
    assert!(app.pop.is_none());
    assert_eq!(snapshot(&repo), before);
}

#[test]
fn branch_merge_review_layout_focus_and_mouse_targets() {
    let (_temp, fixture) = fixture();
    let repo = fixture.ctx.repos[0].path.clone();
    merge_conflict(&repo);
    git(&repo, &["merge", "--abort"]).unwrap();
    let before = snapshot(&repo);
    for icons in [IconMode::Ascii, IconMode::Unicode, IconMode::NerdFont] {
        let mut app = Shell::with_config(
            fixture.ctx.clone(),
            AppConfig {
                icons,
                ..AppConfig::default()
            },
        );
        app.go(Branch);
        ready(&mut app);
        app.dispatch(&key(KeyCode::Char('j')));
        for (width, height) in [(44, 12), (80, 24), (120, 36), (184, 48)] {
            app.dispatch(&key(KeyCode::Char('m')));
            let text = screen(&mut app, width, height);
            assert!(
                text.contains("From  incoming") && text.contains("Into  main"),
                "{text}"
            );
            assert!(
                !text.contains("n Cancel") && !text.contains("y Confirm"),
                "{text}"
            );
            if height >= 24 {
                assert!(
                    text.contains("Repository") && text.contains("What happens"),
                    "{text}"
                );
            }
            for focused in [true, false] {
                let mut terminal =
                    Terminal::new(ratatui::backend::TestBackend::new(width, height)).unwrap();
                terminal.draw(|frame| app.draw(frame)).unwrap();
                for (id, label) in [(0, " Cancel "), (1, " Confirm ")] {
                    let rect = app.pop_hits.iter().find(|(_, hit)| *hit == id).unwrap().0;
                    assert_eq!(rect.width, label.len() as u16);
                    assert!(rect.right() <= width && rect.bottom() <= height);
                    let buffer = terminal.backend().buffer();
                    assert_eq!(
                        buffer[(rect.x, rect.y)].bg,
                        if (id == 1) == focused { SEL } else { TAG }
                    );
                    assert!(!buffer[(rect.x, rect.y)]
                        .modifier
                        .contains(ratatui::style::Modifier::UNDERLINED));
                }
                if focused {
                    app.dispatch(&key(KeyCode::Tab));
                }
            }
            let cancel = app.pop_hits.iter().find(|(_, id)| *id == 0).unwrap().0;
            app.dispatch(&Event::Mouse(MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: cancel.x + 1,
                row: cancel.y,
                modifiers: KeyModifiers::NONE,
            }));
            assert!(app.pop.is_none());
            assert_eq!(snapshot(&repo), before);
        }
    }
}

#[test]
fn branch_merge_review_wraps_unicode_and_scrolls_without_hiding_buttons() {
    let (_temp, mut app) = fixture();
    let repo = app.ctx.repos[0].path.clone();
    merge_conflict(&repo);
    git(&repo, &["merge", "--abort"]).unwrap();
    let source = format!("feature-{}", "合并测试".repeat(12));
    git(&repo, &["branch", "-m", "incoming", &source]).unwrap();
    let before = snapshot(&repo);
    app.go(Branch);
    ready(&mut app);
    app.dispatch(&key(KeyCode::Char('j')));
    app.dispatch(&key(KeyCode::Char('m')));
    let text = screen(&mut app, 44, 12);
    assert!(
        text.contains("From") && text.contains("feature-合并测试"),
        "{text}"
    );
    for _ in 0..40 {
        app.dispatch(&key(KeyCode::Char('j')));
    }
    let text = screen(&mut app, 44, 12);
    assert!(
        text.contains("What happens") && text.contains("tab."),
        "{text}"
    );
    assert!(
        text.contains("Cancel") && text.contains("Confirm"),
        "{text}"
    );
    for _ in 0..40 {
        app.dispatch(&key(KeyCode::Char('k')));
    }
    let text = screen(&mut app, 44, 12);
    assert!(
        text.contains("From") && text.contains("feature-合并测试"),
        "{text}"
    );
    assert!(matches!(
        app.pop,
        Some(Popup::Confirm {
            scroll: 0,
            yes: true,
            ..
        })
    ));
    screen(&mut app, 80, 24);
    let close = app
        .pop_hits
        .iter()
        .find(|(_, id)| *id == usize::MAX)
        .unwrap()
        .0;
    app.dispatch(&Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: close.x,
        row: close.y,
        modifiers: KeyModifiers::NONE,
    }));
    assert!(app.pop.is_none());
    assert_eq!(snapshot(&repo), before);
}

#[test]
fn branch_refresh_reads_external_refs_without_refreshing_unopened_history() {
    let (_temp, fixture) = fixture();
    let mut app = Shell::unloaded(fixture.ctx.clone(), AppConfig::default());
    app.go(Branch);
    ready(&mut app);
    assert!(matches!(app.log_load, startup::LogLoad::Dormant));
    let repo = &app.ctx.repos[0].path;
    git(repo, &["remote", "add", "origin", "../unused-local-remote"]).unwrap();
    git(repo, &["update-ref", "refs/remotes/origin/fetched", "HEAD"]).unwrap();
    app.dispatch(&Event::FocusGained);
    ready(&mut app);
    let fresh = screen(&mut app, 120, 36);
    assert!(fresh.contains("Remote Branches  1") && fresh.contains("fetched"));
    assert!(matches!(app.log_load, startup::LogLoad::Dormant));
}
