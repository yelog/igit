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
    assert!(matches!(panel.confirm(&token, &c), Action::Refresh(_)));
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
        assert_eq!(title, "Cherry-pick Commit");
        assert!(body.contains(&picked) && body.contains(&head));
        assert!(body.contains("new commit on main"));
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
        assert_eq!(title, "Cherry-pick Commit");
        assert!(body.contains(&picked));
        assert_eq!(git(dir.path(), &["rev-parse", "HEAD"]), head);
        let Action::Refresh(message) = panel.confirm(&token, &c) else {
            panic!("Every attempted cherry-pick must refresh workspace and Conflicts");
        };
        assert!(matches!(panel.confirm(&token, &c), Action::None));
        if conflict {
            assert!(message.contains("Cherry-pick Commit failed"));
            assert_eq!(git(dir.path(), &["rev-parse", "HEAD"]), head);
            assert_eq!(git(dir.path(), &["rev-parse", "CHERRY_PICK_HEAD"]), picked);
            assert!(!git(dir.path(), &["ls-files", "--unmerged"]).is_empty());
            git(dir.path(), &["cherry-pick", "--abort"]);
        } else {
            assert!(message.contains("Cherry-pick Commit completed"));
            assert_eq!(git(dir.path(), &["rev-parse", "HEAD^"]), head);
            assert_eq!(
                git(dir.path(), &["show", "HEAD:picked"]),
                "Selected source change"
            );
        }
    }
}
