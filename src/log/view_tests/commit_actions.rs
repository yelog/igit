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
