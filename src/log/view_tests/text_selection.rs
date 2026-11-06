use super::*;
use unicode_width::UnicodeWidthStr;

fn drag(p: &mut LogPanel, c: &Context, x: u16, y: u16, end: u16) -> Action {
    let mut action = Action::None;
    for (kind, column) in [
        (M::Down(MouseButton::Left), x),
        (M::Drag(MouseButton::Left), end),
        (M::Up(MouseButton::Left), end),
    ] {
        action = p.event(
            &Event::Mouse(MouseEvent {
                kind,
                column,
                row: y,
                modifiers: Mods::NONE,
            }),
            c,
        );
    }
    action
}
fn copied(action: Action) -> String {
    match action {
        Action::Copy { text, .. } => text,
        _ => panic!("expected selected text copy"),
    }
}

#[test]
fn text_selection_details_and_scrolled_diff_copy_without_navigation_or_git_writes() {
    let d = init();
    commit(d.path(), "file", "base");
    commit(d.path(), "file", "Copy ＵＩ");
    let c = Context::discover(d.path()).unwrap();
    let snapshot = || {
        [
            git(d.path(), &["rev-parse", "HEAD"]),
            git(d.path(), &["ls-files", "--stage"]),
            git(d.path(), &["status", "--porcelain=v1"]),
        ]
    };
    let before = snapshot();
    for (w, h) in [(120, 36), (80, 24), (44, 12)] {
        let mut p = LogPanel::default();
        p.refresh(&c).unwrap();
        finish_preview(&mut p, &c);
        p.focus = Pane::Info;
        let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
        render(&mut p, &mut t, &c);
        let info = p.panes.rect(PaneId::Info).unwrap();
        let subject = p.rev().unwrap().subject.clone();
        let (x, y) = (info.y..info.bottom())
            .find_map(|y| {
                (info.x..info.right())
                    .find(|&x| {
                        row_text(&t, Rect::new(x, y, info.right() - x, 1)).starts_with(&subject)
                    })
                    .map(|x| (x, y))
            })
            .expect("subject is visible inside the details frame");
        assert_eq!(
            copied(drag(&mut p, &c, x, y, x + subject.width() as u16)),
            subject
        );
        render(&mut p, &mut t, &c);
        assert_ne!(t.backend().buffer()[(x, y)].bg, SELECT);
        assert!(p.selected_text().is_none());
        key(&mut p, K::Char('a'), Mods::CONTROL, &c);
        let selected = p.selected_text().unwrap().to_owned();
        assert_eq!(
            copied(key(&mut p, K::Char('c'), Mods::CONTROL, &c)),
            selected
        );
        if w == 120 {
            let mut shorter = Terminal::new(TestBackend::new(w, 24)).unwrap();
            render(&mut p, &mut shorter, &c);
            assert_eq!(
                p.selected_text(),
                Some(selected.as_str()),
                "responsive spacing is decoration, not a source edit"
            );
            render(&mut p, &mut t, &c);
        }
        key(&mut p, K::Esc, Mods::NONE, &c);
        assert!(p.selected_text().is_none() && p.focus == Pane::Info);

        p.scroll_info(isize::MAX);
        render(&mut p, &mut t, &c);
        let parent = target(&p, |h| matches!(h, Hit::Parent(0)));
        let cursor = p.cursor;
        let parent_copy = drag(&mut p, &c, parent.right() - 8, parent.y, parent.right());
        assert_eq!(
            p.cursor, cursor,
            "dragging a parent hash must not follow its link"
        );
        assert_eq!(copied(parent_copy), p.rev().unwrap().parents[0][..8]);
        assert!(p.selected_text().is_none());
        click_at(&mut p, parent, &c);
        assert_ne!(
            p.cursor, cursor,
            "an ordinary click still follows the parent"
        );

        // Return to the newest commit and select its actual After revision.
        p.select_row(0);
        finish_preview(&mut p, &c);
        p.focus = Pane::Graph;
        p.open_diff(&c).unwrap();
        assert!(!p
            .diff
            .as_ref()
            .unwrap()
            .comparison
            .as_ref()
            .unwrap()
            .is_three_way());
        render(&mut p, &mut t, &c);
        let expected = "Copy ＵＩ";
        let (x, y) = (0..h)
            .find_map(|y| {
                (0..w)
                    .find(|&x| row_text(&t, Rect::new(x, y, w - x, 1)).starts_with(expected))
                    .map(|x| (x, y))
            })
            .expect("After revision is visible");
        assert_eq!(
            copied(drag(&mut p, &c, x, y, x + expected.width() as u16)),
            expected
        );
        assert!(p.selected_text().is_none());
        key(&mut p, K::Char('a'), Mods::CONTROL, &c);
        let selected = p.selected_text().unwrap().to_owned();
        key(&mut p, K::Char('k'), Mods::NONE, &c);
        render(&mut p, &mut t, &c);
        assert_eq!(p.selected_text(), Some(selected.as_str()));
        key(&mut p, K::Esc, Mods::NONE, &c);
        assert!(p.diff.is_some() && p.selected_text().is_none());
        key(&mut p, K::Esc, Mods::NONE, &c);
        assert!(p.diff.is_none());
    }
    assert_eq!(snapshot(), before);
}
