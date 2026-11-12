use super::{test_support::*, *};
use crate::{core::git, ui::panes::PaneId};
use crossterm::event::MouseEvent;
use ratatui::style::Modifier;

fn pull_fixture(source: &str, target: &str) -> (tempfile::TempDir, Shell) {
    let (temp, original) = fixture();
    let repo = &original.ctx.repos[0].path;
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
    if source == "main" {
        git(
            repo,
            &["branch", &format!("--set-upstream-to=origin/{target}")],
        )
        .unwrap();
    } else {
        git(
            repo,
            &["branch", "--track", source, &format!("origin/{target}")],
        )
        .unwrap();
    }
    advance_remote(repo, target);

    let tools = temp.path().join("tools");
    let tools_remote = temp.path().join("tools.git");
    std::fs::create_dir(&tools).unwrap();
    std::fs::create_dir(&tools_remote).unwrap();
    git(&tools, &["init", "-q", "-b", "main"]).unwrap();
    for (name, value) in [
        ("user.name", "Pull tester"),
        ("user.email", "pull@example.invalid"),
        ("commit.gpgSign", "false"),
        ("core.hooksPath", "/dev/null"),
    ] {
        git(&tools, &["config", name, value]).unwrap();
    }
    git(&tools, &["commit", "--allow-empty", "-qm", "tools base"]).unwrap();
    git(&tools_remote, &["init", "--bare", "-q"]).unwrap();
    git(
        &tools,
        &["remote", "add", "origin", tools_remote.to_str().unwrap()],
    )
    .unwrap();
    git(&tools, &["push", "--set-upstream", "origin", "main"]).unwrap();
    advance_remote(&tools, "main");

    let mut context = Context::discover(temp.path()).unwrap();
    context.state_dir = temp.path().join("pull-state");
    let mut app = Shell::unloaded(
        context,
        AppConfig {
            icons: IconMode::Ascii,
            ..AppConfig::default()
        },
    );
    app.go(Branch);
    app.branch.finish_loading(&app.ctx);
    screen(&mut app, 120, 36);
    if source != "main" {
        app.dispatch(&key(KeyCode::Char('j')));
    }
    (temp, app)
}

fn advance_remote(repo: &std::path::Path, target: &str) {
    let tree = git(repo, &["rev-parse", "HEAD^{tree}"]).unwrap();
    let base = git(repo, &["rev-parse", "HEAD"]).unwrap();
    let incoming = git(repo, &["commit-tree", &tree, "-p", &base, "-m", "incoming"]).unwrap();
    git(
        repo,
        &["push", "origin", &format!("{incoming}:refs/heads/{target}")],
    )
    .unwrap();
    git(
        repo,
        &[
            "update-ref",
            &format!("refs/remotes/origin/{target}"),
            &base,
        ],
    )
    .unwrap();
}

fn assert_pull_review(app: &Shell) {
    assert!(
        matches!(&app.pop, Some(Popup::Confirm {
            owner: Branch, title, yes: true, ..
        }) if title == "Pull local branch"),
        "Expected focused Branch Pull review; context: {}; notice: {}",
        app.help_content().context(),
        app.notifications.latest_body()
    );
}

fn finish_pull(app: &mut Shell) {
    assert!(app.branch.syncing(), "{}", app.notifications.latest_body());
    let deadline = Instant::now() + Duration::from_secs(10);
    while app.branch.syncing() {
        app.poll();
        assert!(
            Instant::now() < deadline,
            "pull timed out: {}",
            app.notifications.latest_body()
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(
        app.notifications.latest_body().contains("completed"),
        "{}",
        app.notifications.latest_body()
    );
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
fn branch_pull_reviews_only_the_focused_branch_and_preserves_other_repositories() {
    let (temp, mut app) = pull_fixture("topic", "published-topic");
    let repo = app.ctx.repos[0].path.clone();
    let tools = app.ctx.repos[1].path.clone();
    let before = snapshot(&repo);
    let refs = git(&repo, &["show-ref"]).unwrap();
    let tools_refs = git(&tools, &["show-ref"]).unwrap();
    let tools_before = snapshot(&tools);
    let index = std::fs::read(repo.join(".git/index")).unwrap();
    let remote = temp.path().join("remote.git");
    let remote_refs = git(&remote, &["show-ref"]).unwrap();
    let incoming = git(&remote, &["rev-parse", "refs/heads/published-topic"]).unwrap();

    app.dispatch(&key(KeyCode::Char('p')));
    assert_pull_review(&app);
    let text = screen(&mut app, 120, 36);
    assert!(
        text.contains("From  origin/published-topic") && text.contains("To    topic"),
        "{text}"
    );
    assert_eq!(git(&repo, &["show-ref"]).unwrap(), refs);
    assert_eq!(snapshot(&repo), before);
    app.dispatch(&key(KeyCode::Enter));
    finish_pull(&mut app);

    assert_eq!(git(&repo, &["rev-parse", "topic"]).unwrap(), incoming);
    assert_eq!(snapshot(&repo), before);
    assert_eq!(std::fs::read(repo.join(".git/index")).unwrap(), index);
    assert_eq!(git(&tools, &["show-ref"]).unwrap(), tools_refs);
    assert_eq!(snapshot(&tools), tools_before);
    assert_eq!(git(&remote, &["show-ref"]).unwrap(), remote_refs);
}

#[test]
fn branch_pull_updates_the_current_branch_in_the_selected_repository() {
    for second in [false, true] {
        let (temp, mut app) = pull_fixture("main", "published-main");
        if second {
            app.dispatch(&key(KeyCode::Tab));
            app.dispatch(&key(KeyCode::Char('j')));
            app.dispatch(&key(KeyCode::Enter));
        }
        let selected = usize::from(second);
        let repo = app.ctx.repos[selected].path.clone();
        let other = app.ctx.repos[1 - selected].path.clone();
        let before = snapshot(&repo);
        let other_refs = git(&other, &["show-ref"]).unwrap();
        let other_before = snapshot(&other);
        let remote = temp
            .path()
            .join(if second { "tools.git" } else { "remote.git" });
        let target = if second { "main" } else { "published-main" };
        let incoming = git(&remote, &["rev-parse", &format!("refs/heads/{target}")]).unwrap();
        app.dispatch(&key(KeyCode::Char('p')));
        assert_pull_review(&app);
        app.dispatch(&key(KeyCode::Char('y')));
        finish_pull(&mut app);
        assert_eq!(git(&repo, &["rev-parse", "HEAD"]).unwrap(), incoming);
        assert_eq!(
            git(&repo, &["symbolic-ref", "--short", "HEAD"]).unwrap(),
            "main"
        );
        assert_eq!(&snapshot(&repo)[1..], &before[1..]);
        assert_eq!(git(&other, &["show-ref"]).unwrap(), other_refs);
        assert_eq!(snapshot(&other), other_before);
    }
}

#[test]
fn branch_pull_alias_remapping_disabling_and_help_keep_their_scope() {
    let (_temp, original) = pull_fixture("main", "published-main");
    for (config, bindings) in [
        ("", vec![KeyCode::Char('p'), KeyCode::Char('U')]),
        (
            "[keybindings.log]\nsearch=['F8']",
            vec![KeyCode::Char('p'), KeyCode::Char('U')],
        ),
        ("[keybindings.branch]\npull=['F8']", vec![KeyCode::F(8)]),
        ("[keybindings.branch]\npull=[]", vec![]),
    ] {
        let mut app = Shell::unloaded(original.ctx.clone(), AppConfig::from_toml(config).unwrap());
        app.go(Branch);
        app.branch.finish_loading(&app.ctx);
        for binding in [KeyCode::Char('p'), KeyCode::Char('U'), KeyCode::F(8)] {
            app.dispatch(&key(binding));
            if bindings.contains(&binding) {
                assert_pull_review(&app);
                app.dispatch(&key(KeyCode::Esc));
            } else {
                assert!(app.pop.is_none(), "{config}: {binding:?}");
            }
        }
        app.dispatch(&key(KeyCode::F(1)));
        app.dispatch(&Event::Paste("pull".into()));
        let text = screen(&mut app, 180, 36);
        assert!(!text.contains("Pull for all repositories"), "{text}");
        if !bindings.is_empty() {
            assert!(text.contains("focused local branch"), "{text}");
            let label = app.config.label("branch", "pull");
            assert!(text.contains(&label), "{text}");
            app.dispatch(&key(KeyCode::Enter));
            assert_pull_review(&app);
        } else {
            assert!(!text.contains("fast-forward pull"), "{text}");
        }
    }
}

#[test]
fn branch_pull_does_not_fall_back_to_global_pull_for_nonlocal_selection() {
    let (_temp, mut app) = pull_fixture("topic", "published-topic");
    let repo = app.ctx.repos[0].path.clone();
    let before = snapshot(&repo);
    let refs = git(&repo, &["show-ref"]).unwrap();
    app.dispatch(&key(KeyCode::Tab));
    assert_eq!(app.branch.focused_pane(), PaneId::BranchRepos);
    app.dispatch(&key(KeyCode::Char('p')));
    assert!(app.pop.is_none() && !app.branch.syncing());
    app.dispatch(&key(KeyCode::Enter));
    for selection in [
        KeyCode::Home,
        KeyCode::End,
        KeyCode::Home,
        KeyCode::Char('j'),
    ] {
        app.dispatch(&key(selection));
        app.dispatch(&key(KeyCode::Char('p')));
        assert!(app.pop.is_none() && !app.branch.syncing());
    }
    assert_eq!(git(&repo, &["show-ref"]).unwrap(), refs);
    assert_eq!(snapshot(&repo), before);
    let action = app
        .menu_actions()
        .position(|action| matches!(action, MenuAction::Update(update::Kind::Pull)))
        .unwrap();
    app.menu_action(action);
    assert!(matches!(app.pop, Some(Popup::Update(_))));
    app.dispatch(&key(KeyCode::Esc));
    app.go(Commit);
    app.dispatch(&key(KeyCode::Char('p')));
    assert!(matches!(app.pop, Some(Popup::Update(_))));
}

#[test]
fn branch_pull_review_matches_push_layout_focus_and_mouse_controls() {
    let (temp, mut app) = pull_fixture("topic", "published-topic");
    let repo = app.ctx.repos[0].path.clone();
    let before = snapshot(&repo);
    let refs = git(&repo, &["show-ref"]).unwrap();
    for icons in [IconMode::Ascii, IconMode::Unicode, IconMode::NerdFont] {
        app.icons = icons;
        app.branch.icons = icons;
        for (width, height) in [(44, 12), (80, 24), (120, 36), (184, 48)] {
            app.dispatch(&key(KeyCode::Char('p')));
            assert_pull_review(&app);
            let text = screen(&mut app, width, height);
            assert!(
                text.contains("From  origin/published-topic") && text.contains("To    topic"),
                "{text}"
            );
            if height >= 24 {
                for label in ["Fast-forward only", "Repository", "Checkout  main"] {
                    assert!(text.contains(label), "{label}\n{text}");
                }
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
                    (
                        "From  origin/published-topic",
                        "From  ",
                        crate::ui::idea::REMOTE_REF,
                    ),
                    ("To    topic", "To    ", crate::ui::idea::LOCAL_REF),
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
            assert_eq!(git(&repo, &["show-ref"]).unwrap(), refs);
            assert_eq!(snapshot(&repo), before);
        }
    }
    for navigation in [
        KeyCode::Tab,
        KeyCode::BackTab,
        KeyCode::Left,
        KeyCode::Right,
    ] {
        app.dispatch(&key(KeyCode::Char('p')));
        app.dispatch(&key(navigation));
        assert!(matches!(app.pop, Some(Popup::Confirm { yes: false, .. })));
        app.dispatch(&key(navigation));
        assert_pull_review(&app);
        app.dispatch(&key(KeyCode::Char('n')));
    }
    app.dispatch(&key(KeyCode::Char('p')));
    screen(&mut app, 80, 24);
    click_button(&mut app, usize::MAX);
    assert!(app.pop.is_none() && !app.branch.syncing());
    assert_eq!(git(&repo, &["show-ref"]).unwrap(), refs);
    app.dispatch(&key(KeyCode::Char('p')));
    screen(&mut app, 80, 24);
    click_button(&mut app, 1);
    finish_pull(&mut app);
    assert_eq!(
        git(&repo, &["rev-parse", "topic"]).unwrap(),
        git(
            &temp.path().join("remote.git"),
            &["rev-parse", "refs/heads/published-topic"],
        )
        .unwrap()
    );
    assert_eq!(snapshot(&repo), before);
}

#[test]
fn branch_pull_review_wraps_unicode_and_keeps_controls_visible_while_scrolling() {
    let source = format!("feature-{}", "拉取分支".repeat(12));
    let target = format!("upstream-{}", "远端来源".repeat(12));
    let (_temp, mut app) = pull_fixture(&source, &target);
    let repo = app.ctx.repos[0].path.clone();
    let refs = git(&repo, &["show-ref"]).unwrap();
    let before = snapshot(&repo);
    app.dispatch(&key(KeyCode::Char('p')));
    assert_pull_review(&app);
    let text = screen(&mut app, 44, 12);
    assert!(
        text.contains("From") && text.contains("origin/upstream-远端来源"),
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
    assert!(matches!(
        app.pop,
        Some(Popup::Confirm {
            scroll: 0,
            yes: true,
            ..
        })
    ));
    app.dispatch(&key(KeyCode::Esc));
    assert_eq!(git(&repo, &["show-ref"]).unwrap(), refs);
    assert_eq!(snapshot(&repo), before);
}
