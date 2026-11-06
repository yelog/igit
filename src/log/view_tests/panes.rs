use super::*;
use crate::ui::{ACCENT, LINE};

fn assert_frames(p: &LogPanel, t: &Terminal<TestBackend>, focused: Option<PaneId>) {
    for &(id, r) in p.panes.nodes().iter().filter(|(_, r)| !r.is_empty()) {
        let edge = &t.backend().buffer()[(r.x, r.y + r.height / 2)];
        assert_eq!(
            edge.fg,
            if Some(id) == focused { ACCENT } else { LINE },
            "{id:?}"
        );
    }
}

#[test]
fn only_the_active_log_pane_has_a_focus_frame_and_compact_tabs_keep_the_commit() {
    let d = init();
    commit(d.path(), "one", "first commit");
    commit(d.path(), "two", "second commit");
    let c = Context::discover(d.path()).unwrap();
    let mut p = LogPanel::default();
    p.refresh(&c).unwrap();
    p.select_row(1);
    finish_preview(&mut p, &c);
    let selected = p.rev().unwrap().oid.clone();
    let file = p.file_index();

    let mut t = Terminal::new(TestBackend::new(120, 36)).unwrap();
    render(&mut p, &mut t, &c);
    assert_frames(&p, &t, Some(PaneId::Graph));
    p.focus_pane(PaneId::Files);
    render(&mut p, &mut t, &c);
    assert_frames(&p, &t, Some(PaneId::Files));
    p.panes.active = false;
    render(&mut p, &mut t, &c);
    assert_frames(&p, &t, None);
    p.panes.active = true;
    key(&mut p, K::Char('/'), Mods::NONE, &c);
    render(&mut p, &mut t, &c);
    assert_frames(&p, &t, None);
    key(&mut p, K::Esc, Mods::NONE, &c);

    t.backend_mut().resize(44, 4); // space available to Log in the minimum split workspace
    for id in Pane::ORDER.map(Pane::id) {
        p.focus_pane(id);
        render(&mut p, &mut t, &c);
        assert_eq!(p.panes.nodes().len(), 4);
        for &(other, rect) in p.panes.nodes() {
            assert_eq!(!rect.is_empty(), other == id);
        }
        assert_frames(&p, &t, Some(id));
        assert_eq!(p.rev().unwrap().oid, selected);
        assert_eq!(p.file_index(), file);
    }
    view_item(&mut p, display::Item::CommitDetails, &c);
    render(&mut p, &mut t, &c);
    assert_eq!(p.focused_pane(), PaneId::Graph);
    assert!(p.panes.rect(PaneId::Files).is_none() && p.panes.rect(PaneId::Info).is_none());
}

#[test]
fn shrinking_the_inspector_leaves_its_last_file_row_clickable() {
    let d = init();
    commit(d.path(), "file", "one change");
    let c = Context::discover(d.path()).unwrap();
    let mut p = LogPanel::default();
    p.refresh(&c).unwrap();
    finish_preview(&mut p, &c);
    let mut t = Terminal::new(TestBackend::new(120, 6)).unwrap();
    render(&mut p, &mut t, &c);
    let files = p.panes.rect(PaneId::Files).unwrap();
    let (row, n) = p
        .hits
        .iter()
        .find_map(|&(r, ref h)| match h {
            Hit::File(n) if r.y == files.bottom() - 1 => Some((r, *n)),
            _ => None,
        })
        .unwrap();
    click_at(&mut p, row, &c);
    assert_eq!(p.focused_pane(), PaneId::Files);
    assert_eq!(p.file_cursor(), n);
    assert!(!p.panes.dragging());
    assert_eq!(
        t.backend().buffer()[(files.right() - 1, files.bottom() - 1)].symbol(),
        "┤"
    );
    let from = MouseEvent {
        kind: M::Down(MouseButton::Left),
        column: files.right() - 1,
        row: files.bottom() - 1,
        modifiers: Mods::NONE,
    };
    p.event(&Event::Mouse(from), &c);
    assert!(p.panes.dragging());
    p.event(
        &Event::Mouse(MouseEvent {
            kind: M::Up(MouseButton::Left),
            row: from.row + 1,
            ..from
        }),
        &c,
    );
    render(&mut p, &mut t, &c);
    assert_eq!(
        p.panes.rect(PaneId::Files).unwrap().height,
        files.height + 1
    );
    assert_eq!(p.file_cursor(), n);
}

#[test]
fn maximized_log_keeps_enabled_panes_and_view_menu_without_hidden_targets() {
    let d = init();
    commit(d.path(), "file", "first commit");
    commit(d.path(), "file", "second commit");
    let c = Context::discover(d.path()).unwrap();
    let mut p = LogPanel::default();
    p.refresh(&c).unwrap();
    finish_preview(&mut p, &c);
    let selected = p.rev().unwrap().oid.clone();
    let mut t = Terminal::new(TestBackend::new(120, 36)).unwrap();
    render(&mut p, &mut t, &c);
    let graph = p.panes.rect(PaneId::Graph).unwrap();
    assert!(p.panes.resize(graph, Axis::Width, 7));
    render(&mut p, &mut t, &c);
    let restored = p.panes.nodes().to_vec();

    p.focus_pane(PaneId::Info);
    render(&mut p, &mut t, &c);
    p.info_selection.select_all();
    assert!(p.info_selection.selected_text().is_some());
    p.panes.maximized = true;
    for pane in Pane::ORDER {
        p.focus_pane(pane.id());
        render(&mut p, &mut t, &c);
        assert_eq!(
            p.panes.nodes(),
            &Pane::ORDER.map(|id| (
                id.id(),
                if id == pane {
                    Rect::new(0, 1, 120, 35)
                } else {
                    Rect::default()
                }
            ))
        );
        let active = p.panes.rect(pane.id()).unwrap();
        assert!(!p.panes.can_resize(active, Axis::Width));
        assert!(!p.panes.can_resize(active, Axis::Height));
        assert_eq!(
            p.hits
                .iter()
                .filter(|(_, hit)| matches!(hit, Hit::ViewOptions))
                .count(),
            1
        );
        for (_, hit) in &p.hits {
            match hit {
                Hit::Ref(_) => assert_eq!(pane, Pane::Refs),
                Hit::Commit(_) | Hit::GraphJump(_) => assert_eq!(pane, Pane::Graph),
                Hit::File(_) | Hit::FileFold(_) => assert_eq!(pane, Pane::Files),
                Hit::Parent(_) => assert_eq!(pane, Pane::Info),
                _ => {}
            }
        }
        assert!(p.info_selection.selected_text().is_none());
        assert_eq!(p.rev().unwrap().oid, selected);
        assert_frames(&p, &t, Some(pane.id()));
    }
    let eye = target(&p, |hit| matches!(hit, Hit::ViewOptions));
    click_at(&mut p, eye, &c);
    render(&mut p, &mut t, &c);
    assert!(p.view_menu.is_some());
    assert!(p
        .hits
        .iter()
        .any(|(_, hit)| matches!(hit, Hit::ViewMenu(_))));
    key(&mut p, K::Esc, Mods::NONE, &c);
    p.panes.maximized = false;
    render(&mut p, &mut t, &c);
    assert_eq!(p.panes.nodes(), restored);
}

#[test]
fn hidden_log_keeps_tab_order_and_clears_selection_and_mouse_geometry() {
    let d = init();
    commit(d.path(), "file", "first commit");
    let c = Context::discover(d.path()).unwrap();
    let mut p = LogPanel::default();
    p.refresh(&c).unwrap();
    finish_preview(&mut p, &c);
    p.panes.maximized = true;
    p.focus_pane(PaneId::Info);
    let mut t = Terminal::new(TestBackend::new(120, 36)).unwrap();
    render(&mut p, &mut t, &c);
    p.info_selection.select_all();
    assert!(p.info_selection.selected_text().is_some());
    t.draw(|f| p.draw_content(f, Rect::default(), &c)).unwrap();
    assert_eq!(
        p.panes.nodes(),
        &Pane::ORDER.map(|pane| (pane.id(), Rect::default()))
    );
    assert!(p.hits.is_empty());
    assert_eq!(p.graph_height, 0);
    assert!(p.info_selection.selected_text().is_none());
    assert!(p.info_click.is_none());

    p.show_refs = false;
    p.show_info = false;
    t.draw(|f| p.draw_content(f, Rect::default(), &c)).unwrap();
    assert_eq!(p.panes.nodes(), &[(PaneId::Graph, Rect::default())]);
}
