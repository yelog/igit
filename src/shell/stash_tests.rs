use super::{test_support::*, *};
use crate::{core::git, ui::panes::PaneId};
use crossterm::event::MouseEvent;

#[test]
fn stash_operations_confirm_cancel_remap_and_refresh_the_current_entry() {
    let (_temp, mut app) = fixture();
    save(&app);
    let repo = app.ctx.repos[0].path.clone();
    app.go(Stash);
    finish(&mut app);
    let before = snapshot(&repo);
    let entries = git(&repo, &["stash", "list"]).unwrap();
    for (shortcut, title, consequence, button) in [
        ('a', "Apply stash", "Keeps the stash", "Apply"),
        ('P', "Pop stash", "Drops after a successful apply", "Pop"),
        ('d', "Drop stash", "Cannot be undone from IGit.", "Drop"),
    ] {
        app.dispatch(&key(KeyCode::Char(shortcut)));
        assert!(
            matches!(&app.pop, Some(Popup::Confirm { title: actual, yes: true, owner: Stash, .. }) if actual == title)
        );
        for (width, height) in [(120, 36), (80, 24), (44, 12)] {
            let text = screen(&mut app, width, height);
            let (primary, _) = app.pop_hits.iter().find(|(_, id)| *id == 1).unwrap();
            let footer = text.lines().nth(usize::from(primary.y)).unwrap();
            assert!(
                text.contains(title)
                    && text.contains(consequence)
                    && text.contains("Repository")
                    && footer.contains("Cancel")
                    && footer.contains(button)
                    && !footer.contains("Confirm"),
                "{text}"
            );
        }
        assert!(app
            .help_content()
            .sections
            .iter()
            .flat_map(|section| section.bindings)
            .all(|(shortcut, _)| !matches!(*shortcut, "y" | "n")));
        for ch in ['y', 'n'] {
            app.dispatch(&key(KeyCode::Char(ch)));
            assert!(matches!(
                &app.pop,
                Some(Popup::Confirm { owner: Stash, .. })
            ));
        }
        app.dispatch(&key(KeyCode::Tab));
        app.dispatch(&key(KeyCode::Enter));
        assert!(app.pop.is_none());
        assert_eq!(snapshot(&repo), before);
        assert_eq!(git(&repo, &["stash", "list"]).unwrap(), entries);
    }
    app.config =
        AppConfig::from_toml("[keybindings.global]\nupdate=[]\n[keybindings.stash]\napply=['F8']\npop=['F9']\ndrop=['F10']\n")
            .unwrap();
    assert!(app
        .configured_footer()
        .contains("F8 Apply  F9 Pop  F10 Drop"));
    for shortcut in ['a', 'P', 'd'] {
        app.dispatch(&key(KeyCode::Char(shortcut)));
        assert!(app.pop.is_none());
    }
    for (shortcut, title) in [(8, "Apply stash"), (9, "Pop stash"), (10, "Drop stash")] {
        app.dispatch(&key(KeyCode::F(shortcut)));
        assert!(matches!(&app.pop, Some(Popup::Confirm { title: actual, .. }) if actual == title));
        app.dispatch(&key(KeyCode::Esc));
    }
    app.dispatch(&key(KeyCode::F(8)));
    app.dispatch(&key(KeyCode::Enter));
    assert!(app.msg.contains("Applied stash"), "{}", app.msg);
    assert_eq!(git(&repo, &["stash", "list"]).unwrap(), entries);
    let applied = snapshot(&repo);
    assert_ne!(applied, before);

    app.dispatch(&key(KeyCode::F(10)));
    app.dispatch(&key(KeyCode::Enter));
    assert!(app.msg.contains("Dropped stash"), "{}", app.msg);
    assert!(git(&repo, &["stash", "list"]).unwrap().is_empty());
    assert!(screen(&mut app, 80, 24).contains("No stashes"));
    assert_eq!(snapshot(&repo), applied);
}

#[test]
fn stash_review_scrolls_to_full_details_with_the_consequence_and_cancel_still_visible() {
    let (_temp, mut app) = fixture();
    save(&app);
    let repo = app.ctx.repos[0].path.clone();
    let head = git(&repo, &["rev-parse", "HEAD"]).unwrap();
    let before = snapshot(&repo);
    let entries = git(&repo, &["stash", "list"]).unwrap();
    app.go(Stash);
    app.dispatch(&key(KeyCode::Char('a')));
    let first = screen(&mut app, 44, 12);
    assert!(first.contains("j/k Scroll") && !first.contains("Current HEAD"));
    for _ in 0..32 {
        app.dispatch(&key(KeyCode::Char('j')));
        screen(&mut app, 44, 12);
    }
    let last = screen(&mut app, 44, 12);
    assert!(
        last.contains("Keeps the stash") && last.contains("Current HEAD"),
        "{last}"
    );
    assert!(last.contains(&head[head.len() - 8..]), "{last}");

    let (cancel, _) = app.pop_hits.iter().find(|(_, id)| *id == 0).unwrap();
    app.dispatch(&Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: cancel.x + 1,
        row: cancel.y,
        modifiers: KeyModifiers::NONE,
    }));
    assert!(app.pop.is_none());
    assert_eq!(snapshot(&repo), before);
    assert_eq!(git(&repo, &["stash", "list"]).unwrap(), entries);
}

#[test]
fn stash_pop_conflicts_refresh_commit_and_conflicts_and_keep_the_saved_entry() {
    let (_temp, mut app) = fixture();
    save(&app);
    let repo = app.ctx.repos[0].path.clone();
    std::fs::write(repo.join("mixed.txt"), "divergent committed change\n").unwrap();
    git(&repo, &["add", "mixed.txt"]).unwrap();
    git(
        &repo,
        &[
            "-c",
            "user.name=Stash tester",
            "-c",
            "user.email=stash@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "-c",
            "core.hooksPath=/dev/null",
            "commit",
            "-qm",
            "diverge",
        ],
    )
    .unwrap();
    let entries = git(&repo, &["stash", "list"]).unwrap();
    app.go(Stash);
    app.dispatch(&key(KeyCode::Char('P')));
    app.dispatch(&key(KeyCode::Enter));
    assert!(
        app.msg.contains("Stash kept") && app.msg.contains("Conflicts tab"),
        "{}",
        app.msg
    );
    assert_eq!(git(&repo, &["stash", "list"]).unwrap(), entries);
    assert!(app.tab_available(Conflicts));
    app.go(Commit);
    assert!(screen(&mut app, 120, 36).contains("Merge Conflicts"));
    app.go(Conflicts);
    assert!(screen(&mut app, 120, 36).contains("mixed.txt"));
}

#[test]
fn commit_stash_binding_review_cancel_and_saved_entry_work_through_shell() {
    let (_temp, mut app) = fixture();
    let repo = app.ctx.repos[0].path.clone();
    for (name, value) in [
        ("user.name", "Stash tester"),
        ("user.email", "stash@example.invalid"),
    ] {
        git(&repo, &["config", name, value]).unwrap();
    }
    app.config = AppConfig::from_toml(
        "[keybindings.global]\nupdate=[]\n[keybindings.commit]\nstash=['F9']\n",
    )
    .unwrap();
    let before = snapshot(&repo);
    app.dispatch(&key(KeyCode::Char('s')));
    assert!(!app.commit.editing(), "original binding was remapped");
    app.dispatch(&key(KeyCode::F(9)));
    assert!(app.commit.editing());
    assert_eq!(app.help_content().context(), "Commit / Create Stash");
    app.dispatch(&Event::Paste("save from Commit 中文".into()));
    app.dispatch(&key(KeyCode::Enter));
    assert!(matches!(&app.pop, Some(Popup::Confirm { token, .. }) if token == "commit-stash"));
    app.dispatch(&key(KeyCode::Esc));
    assert_eq!(snapshot(&repo), before);
    assert!(git(&repo, &["stash", "list"]).unwrap().is_empty());
    app.dispatch(&key(KeyCode::F(9)));
    app.dispatch(&key(KeyCode::Enter));
    app.dispatch(&key(KeyCode::Char('y')));
    assert!(app.msg.contains("SAVED"), "{}", app.msg);
    assert!(git(&repo, &["status", "--porcelain"]).unwrap().is_empty());
    app.dispatch(&key(KeyCode::Char('5')));
    finish(&mut app);
    let text = screen(&mut app, 120, 36);
    assert!(text.contains("save from Commit 中文"), "{text}");
    assert!(text.contains("mixed.txt"), "{text}");
}

fn save(app: &Shell) {
    git(
        &app.ctx.repos[0].path,
        &[
            "-c",
            "user.name=Stash tester",
            "-c",
            "user.email=stash@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "stash",
            "push",
            "-m",
            "saved mixed changes",
        ],
    )
    .unwrap();
}

fn finish(app: &mut Shell) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while app.stash.preview_wakeup().is_some() {
        app.poll();
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[test]
fn stash_tab_header_keyboard_mouse_panes_and_help_work_at_supported_sizes() {
    let (_temp, mut app) = fixture();
    save(&app);
    let before = snapshot(&app.ctx.repos[0].path);
    for (w, h) in [(120, 36), (80, 25), (44, 12)] {
        app.dispatch(&key(KeyCode::Char('5')));
        assert_eq!(app.tab, Stash);
        finish(&mut app);
        let text = screen(&mut app, w, h);
        assert!(
            text.lines().next().unwrap().contains("Push  Branch  Stash"),
            "{text}"
        );
        assert!(
            text.contains("stash@{0}") && text.contains("mixed.txt"),
            "{text}"
        );
        assert_eq!(app.help_content().context(), "Stash / Repositories");
        app.dispatch(&key(KeyCode::Tab));
        assert_eq!(app.stash.focused_pane(), PaneId::Files);
        app.dispatch(&key(KeyCode::Enter));
        assert!(app.stash.editing());
        assert_eq!(app.help_content().context(), "Stash / Diff (read-only)");
        app.dispatch(&key(KeyCode::Char('1')));
        assert_eq!(app.tab, Stash, "Diff owns its input");
        app.dispatch(&key(KeyCode::F(1)));
        assert!(matches!(app.pop, Some(Popup::Help { .. })));
        app.dispatch(&key(KeyCode::Esc));
        app.dispatch(&key(KeyCode::Esc));
        assert!(!app.stash.editing());
        app.dispatch(&key(KeyCode::BackTab));
        app.dispatch(&key(KeyCode::Char('1')));
        screen(&mut app, w, h);
        let rect = app.regions.tab(Stash);
        app.dispatch(&Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: rect.x + 1,
            row: rect.y,
            modifiers: KeyModifiers::NONE,
        }));
        assert_eq!(app.tab, Stash);
    }
    assert_eq!(snapshot(&app.ctx.repos[0].path), before);
    app.go(Push);
    click_push_cancel(&mut app, 80, 24);
    assert_eq!(app.tab, Stash);
}

#[test]
fn stash_global_and_panel_bindings_can_be_remapped() {
    let (_temp, fixture) = fixture();
    save(&fixture);
    let config = AppConfig::from_toml(
        "[keybindings.global]\nupdate=[]\nfocus-stash=['F9']\n[keybindings.stash]\nopen=['v']\n",
    )
    .unwrap();
    let mut app = Shell::with_config(fixture.ctx.clone(), config);
    app.dispatch(&key(KeyCode::Char('5')));
    assert_eq!(app.tab, Commit);
    app.dispatch(&key(KeyCode::F(9)));
    assert_eq!(app.tab, Stash);
    finish(&mut app);
    assert!(screen(&mut app, 120, 36).contains("v Diff"));
    app.dispatch(&key(KeyCode::Char('v')));
    assert_eq!(app.stash.focused_pane(), PaneId::Files);
    app.dispatch(&key(KeyCode::Enter));
    assert!(!app.stash.editing());
    app.dispatch(&key(KeyCode::Char('v')));
    assert!(app.stash.editing());
}

#[test]
fn stash_header_remains_clickable_with_conflicts_on_a_narrow_terminal() {
    let (_temp, mut app) = fixture();
    save(&app);
    merge_conflict(&app.ctx.repos[0].path);
    app.refresh();
    let text = screen(&mut app, 44, 12);
    for tab in Tab::DISPLAY_ORDER {
        assert!(text.lines().next().unwrap().contains(tab.name()), "{text}");
        assert!(app.regions.tab(tab).width >= tab.name().len() as u16);
    }
}

#[test]
fn stash_focus_refresh_and_roots_update_list_and_maximize_keeps_panes_reachable() {
    let (_temp, mut app) = fixture();
    app.go(Stash);
    assert!(screen(&mut app, 80, 25).contains("No stashes"));
    save(&app);
    app.dispatch(&Event::FocusGained);
    finish(&mut app);
    assert!(screen(&mut app, 80, 25).contains("stash@{0}"));
    app.set_pane_maximized(true);
    screen(&mut app, 80, 25);
    assert_eq!(
        app.stash
            .panes
            .nodes()
            .iter()
            .filter(|(_, r)| !r.is_empty())
            .count(),
        1
    );
    app.dispatch(&key(KeyCode::Tab));
    screen(&mut app, 80, 25);
    assert_eq!(app.stash.focused_pane(), PaneId::Files);
    assert!(!app.stash.panes.rect(PaneId::Files).unwrap().is_empty());
    app.set_pane_maximized(false);
    app.ctx.scope.clear();
    app.refresh();
    assert!(screen(&mut app, 80, 25).contains("No repositories in Roots"));
}
