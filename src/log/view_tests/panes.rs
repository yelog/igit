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
fn enter_from_commit_list_focuses_diff_files_before_opening_a_comparison() {
    let d = init();
    commit(d.path(), "file", "first commit");
    commit(d.path(), "file", "second commit");
    let c = Context::discover(d.path()).unwrap();
    for (width, height) in [(120, 36), (80, 24), (44, 12)] {
        for inspector_visible in [true, false] {
            let mut p = LogPanel::default();
            p.refresh(&c).unwrap();
            finish_preview(&mut p, &c);
            p.show_info = inspector_visible;
            let selected = p.rev().unwrap().oid.clone();
            let file = p.file_index();
            let mut t = Terminal::new(TestBackend::new(width, height)).unwrap();
            render(&mut p, &mut t, &c);

            assert!(matches!(
                key(&mut p, K::Enter, Mods::NONE, &c),
                Action::None
            ));
            render(&mut p, &mut t, &c);
            assert_eq!(p.focused_pane(), PaneId::Files);
            assert_frames(&p, &t, Some(PaneId::Files));
            assert!(p.diff.is_none() && !p.editing());
            assert_eq!(p.rev().unwrap().oid, selected);
            assert_eq!(p.file_index(), file);

            key(&mut p, K::Enter, Mods::NONE, &c);
            assert!(p.diff.as_ref().unwrap().comparison.is_some());
            key(&mut p, K::Esc, Mods::NONE, &c);
            assert!(p.diff.is_none());
            assert_eq!(p.focused_pane(), PaneId::Files);
        }
    }
}

#[test]
fn escape_keeps_log_side_panes_focused_without_changing_selection() {
    let d = init();
    commit(d.path(), "file", "first commit");
    let c = Context::discover(d.path()).unwrap();
    let mut p = LogPanel::default();
    p.refresh(&c).unwrap();
    finish_preview(&mut p, &c);
    let selected = p.rev().unwrap().oid.clone();
    let file = p.file_index();
    for (width, height) in [(120, 36), (80, 24), (44, 12)] {
        let mut t = Terminal::new(TestBackend::new(width, height)).unwrap();
        for pane in [PaneId::Refs, PaneId::Files, PaneId::Info] {
            p.focus_pane(pane);
            render(&mut p, &mut t, &c);
            for _ in 0..2 {
                assert!(matches!(key(&mut p, K::Esc, Mods::NONE, &c), Action::None));
                render(&mut p, &mut t, &c);
                assert_eq!(p.focused_pane(), pane);
                assert_frames(&p, &t, Some(pane));
                assert_eq!(p.rev().unwrap().oid, selected);
                assert_eq!(p.file_index(), file);
                assert!(!p.editing());
            }
        }
    }
}

#[test]
fn log_panes_stay_tiled_with_one_focus_frame_at_all_sizes() {
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
    assert_frames(&p, &t, Some(PaneId::Files));
    key(&mut p, K::Esc, Mods::NONE, &c);
    p.focus_pane(PaneId::Graph);
    key(&mut p, K::Char('/'), Mods::NONE, &c);
    render(&mut p, &mut t, &c);
    assert_frames(&p, &t, None);
    key(&mut p, K::Esc, Mods::NONE, &c);

    for (width, height) in [(120, 33), (80, 21), (44, 9)] {
        t.backend_mut().resize(width, height);
        for id in Pane::ORDER.map(Pane::id) {
            p.focus_pane(id);
            render(&mut p, &mut t, &c);
            let [refs, graph, files, info] =
                Pane::ORDER.map(|pane| p.panes.rect(pane.id()).unwrap());
            assert!([refs, graph, files, info].iter().all(|r| !r.is_empty()));
            assert_eq!(graph.y, 0, "no pane tab strip above the graph");
            assert_eq!(refs.right(), graph.x);
            assert_eq!(graph.right(), files.x);
            assert_eq!(files.bottom(), info.y);
            assert_frames(&p, &t, Some(id));
            assert_eq!(p.rev().unwrap().oid, selected);
            assert_eq!(p.file_index(), file);
        }
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
fn hidden_log_keeps_pane_order_and_clears_selection_and_mouse_geometry() {
    let d = init();
    commit(d.path(), "file", "first commit");
    let c = Context::discover(d.path()).unwrap();
    let mut p = LogPanel::default();
    p.refresh(&c).unwrap();
    finish_preview(&mut p, &c);
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
