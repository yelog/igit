use super::*;

#[test]
fn compare_menu_uses_current_name_and_preserves_branch_selection_on_return() {
    let (_temp, context, mut panel) = fixture();
    let path = &context.repos[0].path;
    run(path, &["branch", "-m", "development"]);
    run(path, &["branch", "feature/目标"]);
    reload(&mut panel, &context);
    select(&mut panel, 0, "refs/heads/feature/目标");
    let selected = panel.tree.rows[panel.tree.cursor].id.clone();
    for icons in [IconMode::Ascii, IconMode::Unicode, IconMode::NerdFont] {
        panel.icons = icons;
        let mut terminal = Terminal::new(TestBackend::new(80, 25)).unwrap();
        terminal
            .draw(|frame| panel.draw(frame, frame.area(), &context))
            .unwrap();
        let row = panel
            .hits
            .iter()
            .find_map(|(rect, hit)| {
                matches!(hit, Hit::Row(index) if *index == panel.tree.cursor).then_some(*rect)
            })
            .unwrap();
        panel.event(
            &Event::Mouse(MouseEvent {
                kind: M::Down(MouseButton::Right),
                column: row.x,
                row: row.y,
                modifiers: Mods::NONE,
            }),
            &context,
        );
        assert_eq!(
            panel.comparison_label().as_deref(),
            Some("Compare with development")
        );
        terminal
            .draw(|frame| panel.draw(frame, frame.area(), &context))
            .unwrap();
        let hit = panel
            .hits
            .iter()
            .find(|(_, hit)| matches!(hit, Hit::Compare))
            .unwrap()
            .0;
        panel.event(
            &Event::Mouse(MouseEvent {
                kind: M::Down(MouseButton::Left),
                column: hit.x,
                row: hit.y,
                modifiers: Mods::NONE,
            }),
            &context,
        );
        assert!(panel.comparing());
        assert!(panel.comparison_pending());
        panel.event(&Event::Key(KeyEvent::new(K::Esc, Mods::NONE)), &context);
        assert!(!panel.comparing());
        assert_eq!(panel.tree.rows[panel.tree.cursor].id, selected);
    }
}

#[test]
fn compare_rejects_current_detached_unborn_and_loading_but_accepts_remote() {
    let (_temp, context, mut panel) = fixture();
    let path = &context.repos[0].path;
    select(&mut panel, 0, "refs/heads/main");
    assert!(panel.comparison_label().is_none());
    assert!(matches!(
        panel.event(
            &Event::Key(KeyEvent::new(K::Char('c'), Mods::NONE)),
            &context
        ),
        Action::Notice(NoticeLevel::Warning, _)
    ));
    run(path, &["update-ref", "refs/remotes/origin/feature", "HEAD"]);
    reload(&mut panel, &context);
    select(&mut panel, 0, "refs/remotes/origin/feature");
    assert_eq!(
        panel.comparison_label().as_deref(),
        Some("Compare with main")
    );
    panel.refresh(&context).unwrap();
    assert!(panel.comparison_label().is_none());
    panel.finish_loading(&context);
    run(path, &["checkout", "--detach", "-q"]);
    reload(&mut panel, &context);
    select(&mut panel, 0, "refs/remotes/origin/feature");
    assert!(panel.comparison_label().is_none());
    run(path, &["checkout", "--orphan", "unborn", "-q"]);
    reload(&mut panel, &context);
    select(&mut panel, 0, "refs/remotes/origin/feature");
    assert!(panel.comparison_label().is_none());
}
