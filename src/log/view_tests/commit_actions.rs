use super::*;
use crate::ui::icons::IconMode;

fn mouse(panel: &mut LogPanel, button: MouseButton, x: u16, y: u16, c: &Context) -> Action {
    panel.event(
        &Event::Mouse(MouseEvent {
            kind: M::Down(button),
            column: x,
            row: y,
            modifiers: Mods::NONE,
        }),
        c,
    )
}

fn label_at(t: &Terminal<TestBackend>, label: &str) -> (u16, u16) {
    let area = *t.backend().buffer().area();
    for y in area.y..area.bottom() {
        let text = row_text(t, Rect::new(area.x, y, area.width, 1));
        if let Some((left, _)) = text.split_once(label) {
            return (area.x + cells(left), y);
        }
    }
    panic!("Missing rendered label: {label}");
}

fn select_commit(panel: &mut LogPanel, oid: &str) {
    let row = panel
        .visible
        .iter()
        .position(|&i| panel.hist.revs[i].oid == oid)
        .unwrap();
    panel.select_row(row);
}

#[test]
fn right_click_targets_the_actual_row_even_when_another_root_has_the_same_oid() {
    let d = tempfile::tempdir().unwrap();
    let source = init();
    commit(source.path(), "file", "Shared commit");
    let left = d.path().join("left");
    let right = d.path().join("right");
    for repo in [&left, &right] {
        git(
            d.path(),
            &[
                "clone",
                "-q",
                source.path().to_str().unwrap(),
                repo.to_str().unwrap(),
            ],
        );
    }
    let c = Context::discover(d.path()).unwrap();
    let mut panel = LogPanel {
        icons: IconMode::Ascii,
        ..LogPanel::default()
    };
    panel.refresh(&c).unwrap();
    let mut t = Terminal::new(TestBackend::new(44, 12)).unwrap();
    render(&mut panel, &mut t, &c);
    let right_id = c.repos.iter().position(|r| r.name == "right").unwrap();
    let n = panel
        .visible
        .iter()
        .position(|&i| panel.hist.revs[i].root == right_id)
        .unwrap();
    let row = target(&panel, |hit| matches!(hit, Hit::Commit(i) if *i == n));
    mouse(&mut panel, MouseButton::Right, row.x, row.y, &c);
    assert_eq!(panel.rev().unwrap().root, right_id);
    assert_eq!(panel.keymap().context(), "Log / Commit Actions");
    render(&mut panel, &mut t, &c);
    for kind in operations::Kind::ALL {
        label_at(&t, kind.title());
    }
    let (x, y) = label_at(&t, "New Tag");
    mouse(&mut panel, MouseButton::Left, x, y, &c);
    render(&mut panel, &mut t, &c);
    panel.event(&Event::Paste("归档".into()), &c);
    let (x, y) = label_at(&t, "Review F2");
    let Action::Confirm { title, body, token } = mouse(&mut panel, MouseButton::Left, x, y, &c)
    else {
        panic!("New Tag should open review");
    };
    assert_eq!(title, "New Tag");
    assert!(body.contains("Repository: right") && body.contains("Create lightweight tag: 归档"));
    assert!(git(&right, &["tag"]).is_empty());
    assert!(matches!(
        panel.confirm(&token, &c),
        Action::Refresh(NoticeLevel::Success, _)
    ));
    assert!(matches!(panel.confirm(&token, &c), Action::None));
    assert!(git(&left, &["tag"]).is_empty());
    assert_eq!(
        git(&right, &["rev-parse", "refs/tags/归档"]),
        panel.rev().unwrap().oid
    );
}

#[test]
fn message_editor_owns_input_and_previews_multiline_unicode_before_writing() {
    let d = init();
    let repo = d.path();
    commit(repo, "file", "Original title\n\nOriginal body");
    let head = git(repo, &["rev-parse", "HEAD"]);
    let c = Context::discover(repo).unwrap();
    let mut panel = LogPanel::default();
    panel.refresh(&c).unwrap();
    let mut t = Terminal::new(TestBackend::new(80, 24)).unwrap();
    key(&mut panel, K::F(2), Mods::NONE, &c);
    assert_eq!(panel.keymap().context(), "Log / Edit Commit Message");
    render(&mut panel, &mut t, &c);
    label_at(&t, "Original body");
    key(&mut panel, K::Char('u'), Mods::CONTROL, &c);
    panel.event(&Event::Paste("新标题".into()), &c);
    key(&mut panel, K::Enter, Mods::NONE, &c);
    panel.event(&Event::Paste("正文".into()), &c);
    let Action::Confirm { body, .. } = key(&mut panel, K::Char('s'), Mods::CONTROL, &c) else {
        panic!("F2 editor should review the new message");
    };
    assert!(body.contains("新标题\n正文"));
    assert_eq!(git(repo, &["rev-parse", "HEAD"]), head);
    assert_eq!(
        git(repo, &["show", "-s", "--format=%B", "HEAD"]),
        "Original title\n\nOriginal body"
    );
}

#[test]
fn graph_shortcuts_open_reviews_for_the_highlighted_commit_without_writing() {
    let d = init();
    let repo = d.path();
    for (file, message) in [
        ("base.txt", "Base"),
        ("middle.txt", "Middle"),
        ("tip.txt", "Tip"),
    ] {
        commit(repo, file, message);
    }
    let base = git(repo, &["rev-parse", "HEAD^"]);
    let tip = git(repo, &["rev-parse", "HEAD"]);
    let c = Context::discover(repo).unwrap();
    let mut panel = LogPanel::default();
    panel.refresh(&c).unwrap();
    for (code, oid, title) in [
        (K::Char('d'), tip.as_str(), "Drop Commit"),
        (K::Char('u'), tip.as_str(), "Undo Commit"),
        (K::Char('R'), base.as_str(), "Revert Commit"),
    ] {
        select_commit(&mut panel, oid);
        let Action::Confirm {
            title: actual,
            body,
            token,
        } = key(&mut panel, code, Mods::NONE, &c)
        else {
            panic!("{title} should open a review");
        };
        assert_eq!(actual, title);
        assert_eq!(token, REVIEW_TOKEN);
        assert!(body.contains(&oid[..8]), "{body}");
        assert_eq!(git(repo, &["rev-parse", "HEAD"]), tip);
        assert_eq!(git(repo, &["status", "--porcelain"]), "");
        assert!(panel.commit_review.is_some());
        assert!(panel.commit_dialog.is_none());
    }
    assert!(matches!(
        panel.event(
            &Event::Key(KeyEvent::new_with_kind(
                K::Char('d'),
                Mods::NONE,
                crossterm::event::KeyEventKind::Release,
            )),
            &c
        ),
        Action::None
    ));
    assert_eq!(git(repo, &["rev-parse", "HEAD"]), tip);
}

#[test]
fn shortcut_revert_targets_the_highlighted_repository_when_oids_match() {
    let d = tempfile::tempdir().unwrap();
    let source = init();
    commit(source.path(), "file", "Shared commit");
    let left = d.path().join("left");
    let right = d.path().join("right");
    for repo in [&left, &right] {
        git(
            d.path(),
            &[
                "clone",
                "-q",
                source.path().to_str().unwrap(),
                repo.to_str().unwrap(),
            ],
        );
    }
    let c = Context::discover(d.path()).unwrap();
    let mut panel = LogPanel::default();
    panel.refresh(&c).unwrap();
    let right_id = c
        .repos
        .iter()
        .position(|repo| repo.name == "right")
        .unwrap();
    let right_row = panel
        .visible
        .iter()
        .position(|&i| panel.hist.revs[i].root == right_id)
        .unwrap();
    panel.select_row(right_row);
    let oid = panel.rev().unwrap().oid.clone();
    let Action::Confirm { title, body, .. } = key(&mut panel, K::Char('R'), Mods::NONE, &c) else {
        panic!("R should review a revert");
    };
    assert_eq!(title, "Revert Commit");
    assert!(body.contains("Repository: right"), "{body}");
    assert!(body.contains(&oid[..8]), "{body}");
    for repo in [&left, &right] {
        assert!(git(repo, &["status", "--porcelain"]).is_empty());
        assert_eq!(git(repo, &["rev-parse", "HEAD"]), oid);
    }
}

#[test]
fn filter_prefix_and_control_modifiers_take_priority_over_commit_shortcuts() {
    let (_d, c, mut panel) = navigation_history();
    let mut t = Terminal::new(TestBackend::new(80, 24)).unwrap();
    render(&mut panel, &mut t, &c);
    let start = panel.cursor;
    assert!(matches!(
        key(&mut panel, K::Char('d'), Mods::CONTROL, &c),
        Action::None
    ));
    assert!(panel.commit_review.is_none());
    assert!(panel.cursor > start);
    let paged = panel.cursor;
    assert!(matches!(
        key(&mut panel, K::Char('u'), Mods::CONTROL, &c),
        Action::None
    ));
    assert!(panel.commit_review.is_none());
    assert_eq!(panel.cursor, start);
    assert!(paged > start);

    assert!(matches!(filter_key(&mut panel, 'd', &c), Action::None));
    assert!(matches!(panel.editor, Some((Field::Date, _))));
    assert!(panel.commit_review.is_none());
    key(&mut panel, K::Esc, Mods::NONE, &c);
    assert!(matches!(filter_key(&mut panel, 'u', &c), Action::None));
    assert!(matches!(panel.editor, Some((Field::Author, _))));
    assert!(panel.commit_review.is_none());
    key(&mut panel, K::Esc, Mods::NONE, &c);

    for modifiers in [Mods::CONTROL, Mods::ALT, Mods::SUPER] {
        for code in [K::Char('d'), K::Char('u'), K::Char('R')] {
            assert!(matches!(key(&mut panel, code, modifiers, &c), Action::None));
            assert!(panel.commit_review.is_none());
        }
    }
}

#[test]
fn editors_menu_info_and_diff_own_commit_shortcut_keys() {
    let d = init();
    commit(d.path(), "file", "Original");
    let c = Context::discover(d.path()).unwrap();
    let mut panel = LogPanel::default();
    panel.refresh(&c).unwrap();
    finish_preview(&mut panel, &c);

    key(&mut panel, K::Char('a'), Mods::NONE, &c);
    for code in [K::Char('d'), K::Char('u'), K::Char('R')] {
        assert!(matches!(
            key(&mut panel, code, Mods::NONE, &c),
            Action::None
        ));
        assert!(panel.commit_dialog.is_some());
        assert!(panel.commit_review.is_none());
    }
    key(&mut panel, K::Esc, Mods::NONE, &c);
    assert!(panel.commit_dialog.is_none());

    key(&mut panel, K::F(2), Mods::NONE, &c);
    assert_eq!(panel.keymap().context(), "Log / Edit Commit Message");
    for code in [K::Char('d'), K::Char('u'), K::Char('R')] {
        assert!(matches!(
            key(&mut panel, code, Mods::NONE, &c),
            Action::None
        ));
        assert!(panel.commit_dialog.is_some());
        assert!(panel.commit_review.is_none());
    }
    key(&mut panel, K::Esc, Mods::NONE, &c);

    panel.focus = Pane::Info;
    for code in [K::Char('d'), K::Char('u'), K::Char('R')] {
        assert!(matches!(
            key(&mut panel, code, Mods::NONE, &c),
            Action::None
        ));
        assert!(panel.commit_review.is_none());
    }
    panel.focus = Pane::Graph;

    key(&mut panel, K::Enter, Mods::NONE, &c);
    assert_eq!(panel.focused_pane(), PaneId::Files);
    assert!(panel.diff.is_none());
    key(&mut panel, K::Enter, Mods::NONE, &c);
    assert!(panel.diff.is_some());
    for code in [K::Char('d'), K::Char('u'), K::Char('R')] {
        assert!(matches!(
            key(&mut panel, code, Mods::NONE, &c),
            Action::None
        ));
        assert!(panel.diff.is_some());
        assert!(panel.commit_review.is_none());
    }
    key(&mut panel, K::Esc, Mods::NONE, &c);
}

#[test]
fn invalid_shortcut_surfaces_the_backend_notice_without_a_review() {
    let d = init();
    let repo = d.path();
    for (file, message) in [("base", "Base"), ("middle", "Middle"), ("tip", "Tip")] {
        commit(repo, file, message);
    }
    let head = git(repo, &["rev-parse", "HEAD"]);
    let middle = git(repo, &["rev-parse", "HEAD^"]);
    let c = Context::discover(repo).unwrap();
    let mut panel = LogPanel::default();
    panel.refresh(&c).unwrap();
    select_commit(&mut panel, &middle);
    let Action::Notice(level, message) = key(&mut panel, K::Char('u'), Mods::NONE, &c) else {
        panic!("Undo should report an ineligible highlighted commit");
    };
    assert_eq!(level, NoticeLevel::Error);
    assert!(message.contains("current branch tip"), "{message}");
    assert!(panel.commit_review.is_none());
    assert!(panel.commit_dialog.is_none());
    assert_eq!(git(repo, &["rev-parse", "HEAD"]), head);
}

#[test]
fn shortcuts_target_the_highlighted_commit_instead_of_checked_cherry_picks() {
    let d = init();
    let repo = d.path();
    for (file, message) in [("base", "Base"), ("middle", "Middle"), ("tip", "Tip")] {
        commit(repo, file, message);
    }
    let middle = git(repo, &["rev-parse", "HEAD^"]);
    let tip = git(repo, &["rev-parse", "HEAD"]);
    let c = Context::discover(repo).unwrap();
    let mut panel = LogPanel::default();
    panel.refresh(&c).unwrap();
    panel.set_pick_mode(true);
    select_commit(&mut panel, &middle);
    panel.toggle_pick(&c).unwrap();
    select_commit(&mut panel, &tip);
    let Action::Confirm { title, body, .. } = key(&mut panel, K::Char('R'), Mods::NONE, &c) else {
        panic!("R should review the highlighted commit");
    };
    assert_eq!(title, "Revert Commit");
    assert!(body.contains(&tip[..8]), "{body}");
    assert_eq!(panel.picks.len(), 1);
    assert_eq!(panel.picks[0].oid, middle);
    assert!(panel.pick_mode);
}

#[test]
fn action_menu_and_graph_help_show_commit_shortcuts() {
    let d = init();
    let repo = d.path();
    commit(repo, "file", "Commit");
    let c = Context::discover(repo).unwrap();
    let mut panel = LogPanel::default();
    panel.refresh(&c).unwrap();
    let bindings = panel
        .keymap()
        .sections
        .iter()
        .flat_map(|section| section.bindings.iter())
        .map(|(key, _)| *key)
        .collect::<Vec<_>>();
    for key in ["d", "u", "R", "F2"] {
        assert!(bindings.contains(&key), "{key}");
    }

    key(&mut panel, K::Char('a'), Mods::NONE, &c);
    let mut t = Terminal::new(TestBackend::new(44, 12)).unwrap();
    render(&mut panel, &mut t, &c);
    for label in [
        "Drop Commit  d",
        "Undo Commit  u",
        "Revert Commit  R",
        "Edit Commit Message  F2",
    ] {
        label_at(&t, label);
    }
}

fn cherry_pick_panel(conflict: bool) -> (tempfile::TempDir, Context, LogPanel, String, String) {
    let dir = init();
    let repo = dir.path();
    git(repo, &["config", "commit.gpgsign", "false"]);
    git(repo, &["config", "rerere.enabled", "false"]);
    commit(repo, "base", "Base");
    git(repo, &["switch", "-c", "source"]);
    commit(repo, "picked", "Selected source change");
    let picked = git(repo, &["rev-parse", "HEAD"]);
    git(repo, &["switch", "main"]);
    commit(
        repo,
        if conflict { "picked" } else { "destination" },
        "Destination change",
    );
    let head = git(repo, &["rev-parse", "HEAD"]);
    let c = Context::discover(repo).unwrap();
    let mut panel = LogPanel::default();
    panel.refresh(&c).unwrap();
    let selected = panel
        .visible
        .iter()
        .position(|&i| panel.hist.revs[i].oid == picked)
        .unwrap();
    panel.select_row(selected);
    (dir, c, panel, picked, head)
}

#[test]
fn cherry_pick_keyboard_menu_reviews_at_narrow_and_wide_sizes_without_writing() {
    let (dir, c, mut panel, picked, head) = cherry_pick_panel(false);
    let before = std::fs::read(dir.path().join(".git/index")).unwrap();
    for (width, height) in [(44, 12), (80, 24)] {
        let mut t = Terminal::new(TestBackend::new(width, height)).unwrap();
        key(&mut panel, K::Char('a'), Mods::NONE, &c);
        let row = operations::Kind::ALL
            .iter()
            .position(|kind| *kind == operations::Kind::CherryPick)
            .unwrap();
        for _ in 0..row {
            key(&mut panel, K::Char('j'), Mods::NONE, &c);
        }
        render(&mut panel, &mut t, &c);
        label_at(&t, "Cherry-pick Commit");
        let Action::Confirm { title, body, token } = key(&mut panel, K::Enter, Mods::NONE, &c)
        else {
            panic!("Cherry-pick should open a write review");
        };
        assert_eq!(title, "Cherry-pick 1 commit");
        assert!(body.contains(&picked[..8]));
        assert!(body.lines().next().unwrap().ends_with(" / main"));
        assert_eq!(git(dir.path(), &["rev-parse", "HEAD"]), head);
        assert_eq!(
            std::fs::read(dir.path().join(".git/index")).unwrap(),
            before
        );
        assert!(!dir.path().join("picked").exists());
        // Canceling Shell's review never calls confirm. Opening a new menu replaces its plan.
        key(&mut panel, K::Char('a'), Mods::NONE, &c);
        key(&mut panel, K::Esc, Mods::NONE, &c);
        assert!(matches!(panel.confirm(&token, &c), Action::None));
    }
    assert_eq!(git(dir.path(), &["rev-parse", "HEAD"]), head);
    assert_eq!(
        std::fs::read(dir.path().join(".git/index")).unwrap(),
        before
    );
}

#[test]
fn cherry_pick_mouse_confirmation_refreshes_after_success_or_native_conflict() {
    for conflict in [false, true] {
        let (dir, c, mut panel, picked, head) = cherry_pick_panel(conflict);
        let mut t = Terminal::new(TestBackend::new(44, 12)).unwrap();
        render(&mut panel, &mut t, &c);
        let row = target(
            &panel,
            |hit| matches!(hit, Hit::Commit(i) if *i == panel.cursor),
        );
        mouse(&mut panel, MouseButton::Right, row.x, row.y, &c);
        render(&mut panel, &mut t, &c);
        let (x, y) = label_at(&t, "Cherry-pick Commit");
        let Action::Confirm { title, body, token } = mouse(&mut panel, MouseButton::Left, x, y, &c)
        else {
            panic!("Cherry-pick should open a write review");
        };
        assert_eq!(title, "Cherry-pick 1 commit");
        assert!(body.contains(&picked[..8]));
        assert_eq!(git(dir.path(), &["rev-parse", "HEAD"]), head);
        let Action::Refresh(level, message) = panel.confirm(&token, &c) else {
            panic!("Every attempted cherry-pick must refresh workspace and Conflicts");
        };
        assert!(matches!(panel.confirm(&token, &c), Action::None));
        if conflict {
            assert_eq!(level, NoticeLevel::Error);
            assert!(message.contains("0 applied · 1 conflicted · 0 remaining"));
            assert_eq!(git(dir.path(), &["rev-parse", "HEAD"]), head);
            assert_eq!(git(dir.path(), &["rev-parse", "CHERRY_PICK_HEAD"]), picked);
            assert!(!git(dir.path(), &["ls-files", "--unmerged"]).is_empty());
            git(dir.path(), &["cherry-pick", "--abort"]);
        } else {
            assert_eq!(level, NoticeLevel::Success);
            assert!(message.contains("Cherry-picked 1 onto"));
            assert_eq!(git(dir.path(), &["rev-parse", "HEAD^"]), head);
            assert_eq!(
                git(dir.path(), &["show", "HEAD:picked"]),
                "Selected source change"
            );
        }
    }
}

#[test]
fn multi_pick_reviews_hidden_commits_in_application_order_and_leaves_native_queue() {
    let dir = init();
    let repo = dir.path();
    commit(repo, "base", "Base");
    git(repo, &["switch", "-c", "source"]);
    commit(repo, "first", "First source");
    let first = git(repo, &["rev-parse", "HEAD"]);
    commit(repo, "clash", "Second source");
    let second = git(repo, &["rev-parse", "HEAD"]);
    commit(repo, "third", "Third source");
    let third = git(repo, &["rev-parse", "HEAD"]);
    git(repo, &["switch", "main"]);
    commit(repo, "clash", "Destination change");
    let head = git(repo, &["rev-parse", "HEAD"]);
    let c = Context::discover(repo).unwrap();
    let mut panel = LogPanel::default();
    panel.refresh(&c).unwrap();
    key(&mut panel, K::Char('v'), Mods::NONE, &c);
    for oid in [&third, &second, &first] {
        let row = panel
            .visible
            .iter()
            .position(|&i| &panel.hist.revs[i].oid == oid)
            .unwrap();
        panel.select_row(row);
        key(&mut panel, K::Char(' '), Mods::NONE, &c);
    }
    panel.visible.retain(|&i| panel.hist.revs[i].oid == third);
    assert!(panel
        .pick_hint(&c, "e", "D")
        .unwrap()
        .starts_with("3 selected · 2 hidden"));
    let Action::Confirm { title, body, token } = key(&mut panel, K::Char('C'), Mods::SHIFT, &c)
    else {
        panic!("expected a batch review");
    };
    assert_eq!(title, "Cherry-pick 3 commits");
    assert!(body.lines().next().unwrap().ends_with(" / main"));
    let list = body.lines().skip(2).collect::<Vec<_>>();
    for (i, oid) in [&first, &second, &third].iter().enumerate() {
        assert!(list[i].starts_with(&format!("{}  {}", i + 1, &oid[..8])));
    }
    assert_eq!(git(repo, &["rev-parse", "HEAD"]), head);
    let Action::Refresh(level, message) = panel.confirm(&token, &c) else {
        panic!("expected a batch result");
    };
    assert_eq!(level, NoticeLevel::Warning);
    assert!(
        message.contains("1 applied · 1 conflicted · 1 remaining"),
        "{message}"
    );
    assert!(repo.join("first").exists());
    assert!(!repo.join("third").exists());
    assert_eq!(git(repo, &["rev-parse", "CHERRY_PICK_HEAD"]), second);
    git(repo, &["cherry-pick", "--skip"]);
    assert!(repo.join("third").exists());
    assert!(!panel.pick_mode);
}
