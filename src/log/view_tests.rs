//! Exercise the rendered cells and actual input dispatch together.
mod branch_menu;
mod column_order;
mod panes;
mod path_selection;
mod reference_badges;
mod text_selection;
use super::tests::{commit, finish_preview, git, init};
use super::*;
use crossterm::event::{KeyEvent, MouseEvent};
use ratatui::{backend::TestBackend, Terminal};

fn key(panel: &mut LogPanel, code: K, mods: Mods, c: &Context) -> Action {
    panel.event(&Event::Key(KeyEvent::new(code, mods)), c)
}
fn filter_key(panel: &mut LogPanel, suffix: char, c: &Context) -> Action {
    key(panel, K::Char(' '), Mods::NONE, c);
    key(panel, K::Char(suffix), Mods::NONE, c)
}
fn click_at(panel: &mut LogPanel, r: Rect, c: &Context) {
    let ev = MouseEvent {
        kind: M::Down(MouseButton::Left),
        column: r.x,
        row: r.y,
        modifiers: Mods::NONE,
    };
    panel.event(&Event::Mouse(ev), c);
    panel.event(
        &Event::Mouse(MouseEvent {
            kind: M::Up(MouseButton::Left),
            ..ev
        }),
        c,
    );
}
fn target(p: &LogPanel, matches: impl Fn(&Hit) -> bool) -> Rect {
    p.hits
        .iter()
        .rev()
        .find(|(_, h)| matches(h))
        .map(|(r, _)| *r)
        .unwrap()
}
fn render(p: &mut LogPanel, t: &mut Terminal<TestBackend>, c: &Context) {
    t.draw(|f| p.draw(f, f.area(), c)).unwrap();
}
fn row_text(t: &Terminal<TestBackend>, r: Rect) -> String {
    let mut text = String::new();
    let mut x = r.x;
    while x < r.right() {
        let symbol = t.backend().buffer()[(x, r.y)].symbol();
        text.push_str(symbol);
        // A wide glyph covers the following cell, even if TestBackend still stores its old value.
        x = x.saturating_add(cells(symbol).max(1));
    }
    text
}

fn navigation_history() -> (tempfile::TempDir, Context, LogPanel) {
    let d = init();
    commit(d.path(), "file", "navigation history");
    let c = Context::discover(d.path()).unwrap();
    let mut p = LogPanel::default();
    p.refresh(&c).unwrap();
    let base = p.hist.revs[0].clone();
    p.hist.revs = (0..80)
        .map(|i| model::Rev {
            oid: format!("{i:040x}"),
            parents: if i < 79 {
                vec![format!("{:040x}", i + 1)]
            } else {
                vec![]
            },
            subject: format!("commit {i}"),
            ..base.clone()
        })
        .collect();
    p.rebuild_visible(None).unwrap();
    (d, c, p)
}

fn navigation_rows(p: &LogPanel) -> Vec<(usize, Rect)> {
    p.hits
        .iter()
        .filter_map(|(r, h)| match h {
            Hit::Commit(i) if r.width > 1 => Some((*i, *r)),
            _ => None,
        })
        .collect()
}

#[test]
fn navigation_scrolls_only_after_crossing_the_visible_edges() {
    let (_d, c, mut p) = navigation_history();
    for (width, height) in [(80, 12), (120, 24), (180, 36)] {
        p.select_row(0);
        let mut t = Terminal::new(TestBackend::new(width, height)).unwrap();
        render(&mut p, &mut t, &c);
        let original = navigation_rows(&p);
        let h = original.len();
        for i in 1..h {
            key(&mut p, K::Char('j'), Mods::NONE, &c);
            render(&mut p, &mut t, &c);
            assert_eq!(navigation_rows(&p), original);
            assert_eq!(p.cursor, i);
        }
        for top in 1..=4 {
            key(&mut p, K::Char('j'), Mods::NONE, &c);
            render(&mut p, &mut t, &c);
            let rows = navigation_rows(&p);
            assert_eq!(rows[0].0, top);
            assert_eq!(rows.last().unwrap().0, p.cursor);
            assert_eq!(rows.last().unwrap().1.y, original.last().unwrap().1.y);
        }
        let scrolled = navigation_rows(&p);
        for _ in 1..h {
            key(&mut p, K::Char('k'), Mods::NONE, &c);
            render(&mut p, &mut t, &c);
            assert_eq!(navigation_rows(&p), scrolled);
        }
        assert_eq!(p.cursor, 4);
        key(&mut p, K::Char('k'), Mods::NONE, &c);
        render(&mut p, &mut t, &c);
        let rows = navigation_rows(&p);
        assert_eq!(rows[0].0, 3);
        assert_eq!(rows[0].0, p.cursor);
        assert_eq!(rows[0].1.y, original[0].1.y);
    }
}

#[test]
fn navigation_batched_moves_keep_the_same_viewport_as_separate_frames() {
    let (_d, c, mut p) = navigation_history();
    let mut t = Terminal::new(TestBackend::new(120, 16)).unwrap();
    render(&mut p, &mut t, &c);
    let h = navigation_rows(&p).len();
    for _ in 0..h + 5 {
        key(&mut p, K::Char('j'), Mods::NONE, &c);
    }
    for _ in 0..h - 1 {
        key(&mut p, K::Char('k'), Mods::NONE, &c);
    }
    render(&mut p, &mut t, &c);
    assert_eq!(p.cursor, 6);
    assert_eq!(navigation_rows(&p)[0].0, 6);

    t.backend_mut().resize(80, 10);
    render(&mut p, &mut t, &c);
    assert_eq!(navigation_rows(&p)[0].0, 6);
    p.select_row(79);
    render(&mut p, &mut t, &c);
    assert_eq!(navigation_rows(&p).last().unwrap().0, 79);
    p.select_row(0);
    render(&mut p, &mut t, &c);
    assert_eq!(navigation_rows(&p)[0].0, 0);
}

#[test]
fn navigation_requires_gg_and_cancels_interrupted_sequences() {
    let (_d, c, mut p) = navigation_history();
    p.select_row(40);
    key(&mut p, K::Char('g'), Mods::NONE, &c);
    assert_eq!(p.cursor, 40);
    p.event(
        &Event::Key(KeyEvent::new_with_kind(
            K::Char('g'),
            Mods::NONE,
            crossterm::event::KeyEventKind::Release,
        )),
        &c,
    );
    key(&mut p, K::Char('g'), Mods::NONE, &c);
    assert_eq!(p.cursor, 0);
    key(&mut p, K::Char('G'), Mods::SHIFT, &c);
    assert_eq!(p.cursor, 79);
    key(&mut p, K::Char('g'), Mods::NONE, &c);
    key(&mut p, K::Char('k'), Mods::NONE, &c);
    key(&mut p, K::Char('g'), Mods::NONE, &c);
    assert_eq!(p.cursor, 78);
    key(&mut p, K::Esc, Mods::NONE, &c);
    key(&mut p, K::Char('g'), Mods::NONE, &c);
    assert_eq!(p.cursor, 78);
    key(&mut p, K::Tab, Mods::NONE, &c);
    key(&mut p, K::Esc, Mods::NONE, &c);
    key(&mut p, K::Char('g'), Mods::NONE, &c);
    assert_eq!(p.cursor, 78);
    key(&mut p, K::Char('g'), Mods::NONE, &c);
    assert_eq!(p.cursor, 0);
}

#[test]
fn navigation_hml_select_visible_rows_without_scrolling_or_toggling_display() {
    let (_d, c, mut p) = navigation_history();
    for (width, height) in [(80, 12), (120, 24), (180, 36)] {
        let mut t = Terminal::new(TestBackend::new(width, height)).unwrap();
        p.select_row(40);
        render(&mut p, &mut t, &c);
        let rows = navigation_rows(&p);
        let highlighting = p.options.current_branch;
        let labels = p.show_labels;
        for mods in [Mods::NONE, Mods::SHIFT] {
            for (ch, index) in [('H', 0), ('M', (rows.len() - 1) / 2), ('L', rows.len() - 1)] {
                key(&mut p, K::Char(ch), mods, &c);
                render(&mut p, &mut t, &c);
                assert_eq!(p.cursor, rows[index].0);
                assert_eq!(navigation_rows(&p), rows);
                assert_eq!(p.options.current_branch, highlighting);
                assert_eq!(p.show_labels, labels);
            }
        }
    }
    p.hist.revs.truncate(3);
    p.rebuild_visible(None).unwrap();
    let mut t = Terminal::new(TestBackend::new(120, 24)).unwrap();
    render(&mut p, &mut t, &c);
    for (ch, expected) in [('L', 2), ('M', 1), ('H', 0)] {
        key(&mut p, K::Char(ch), Mods::NONE, &c);
        assert_eq!(p.cursor, expected);
    }
}

#[test]
fn navigation_pages_use_the_actual_visible_height_and_clamp_at_bounds() {
    let (_d, c, mut p) = navigation_history();
    for (width, height) in [(80, 12), (120, 24), (180, 36)] {
        let mut t = Terminal::new(TestBackend::new(width, height)).unwrap();
        render(&mut p, &mut t, &c);
        let h = navigation_rows(&p).len();
        for (code, mods, delta) in [
            (K::Char('f'), Mods::CONTROL, h as isize),
            (K::Char('b'), Mods::CONTROL, -(h as isize)),
            (K::Char('d'), Mods::CONTROL, (h / 2).max(1) as isize),
            (K::Char('u'), Mods::CONTROL, -((h / 2).max(1) as isize)),
            (K::PageDown, Mods::NONE, h as isize),
            (K::PageUp, Mods::NONE, -(h as isize)),
        ] {
            p.select_row(40);
            render(&mut p, &mut t, &c);
            let top = navigation_rows(&p)[0].0;
            key(&mut p, code, mods, &c);
            assert!(!p.editing());
            assert_eq!(p.cursor, 40usize.saturating_add_signed(delta).min(79));
            render(&mut p, &mut t, &c);
            assert_eq!(
                navigation_rows(&p)[0].0,
                top.saturating_add_signed(delta).min(80 - h)
            );
        }
        for _ in 0..20 {
            key(&mut p, K::Char('f'), Mods::CONTROL, &c);
        }
        render(&mut p, &mut t, &c);
        assert_eq!(p.cursor, 79);
        assert_eq!(navigation_rows(&p).last().unwrap().0, 79);
        for _ in 0..20 {
            key(&mut p, K::Char('b'), Mods::CONTROL, &c);
        }
        render(&mut p, &mut t, &c);
        assert_eq!(p.cursor, 0);
        assert_eq!(navigation_rows(&p)[0].0, 0);
    }
}

#[test]
fn navigation_yanks_the_selected_roots_commit_without_waiting_for_preview() {
    let one = init();
    let two = init();
    let message_one =
        "Fix navigation\n\nKeep the full commit body.\nSecond line includes $HOME and `literal`.";
    let message_two = "another repository\n\nA different message body.";
    git(one.path(), &["config", "user.name", "Author One"]);
    git(one.path(), &["config", "user.email", "one@example.invalid"]);
    commit(one.path(), "one", message_one);
    git(two.path(), &["config", "user.name", "Author Two"]);
    git(two.path(), &["config", "user.email", "two@example.invalid"]);
    commit(two.path(), "two", message_two);
    let mut c = Context::discover(one.path()).unwrap();
    let other = Context::discover(two.path()).unwrap();
    c.repos.push(other.repos[0].clone());
    c.scope.insert(1);
    let mut p = LogPanel::default();
    p.refresh(&c).unwrap();
    finish_preview(&mut p, &c);
    for index in (0..p.visible.len()).rev() {
        p.select_row(index);
        let rev = p.rev().unwrap().clone();
        let root = rev.root;
        for (second, expected) in [
            ('y', rev.oid.clone()),
            (
                'm',
                if root == 0 { message_one } else { message_two }.into(),
            ),
            (
                'a',
                if root == 0 {
                    "Author One <one@example.invalid>"
                } else {
                    "Author Two <two@example.invalid>"
                }
                .into(),
            ),
        ] {
            assert!(matches!(
                key(&mut p, K::Char('y'), Mods::NONE, &c),
                Action::None
            ));
            match key(&mut p, K::Char(second), Mods::NONE, &c) {
                Action::Copy { text, .. } => assert_eq!(text, expected),
                _ => panic!("y{second} did not request a clipboard copy"),
            }
            assert_eq!(p.cursor, index);
            assert!(!p.editing());
        }
        assert!(p.preview.loading());
    }
    for (repo, message) in [(&c.repos[0], message_one), (&c.repos[1], message_two)] {
        assert_eq!(git(&repo.path, &["status", "--porcelain"]), "");
        assert_eq!(git(&repo.path, &["log", "-1", "--format=%B"]), message);
    }
}

#[test]
fn navigation_yank_prefix_is_cancelled_by_clicks_editors_and_other_keys() {
    let (_d, c, mut p) = navigation_history();
    let mut t = Terminal::new(TestBackend::new(120, 24)).unwrap();
    render(&mut p, &mut t, &c);
    key(&mut p, K::Char('y'), Mods::NONE, &c);
    let second = navigation_rows(&p)[1].1;
    click_at(&mut p, second, &c);
    assert!(matches!(filter_key(&mut p, 'u', &c), Action::None));
    assert!(matches!(p.editor, Some((Field::Author, _))));
    key(&mut p, K::Esc, Mods::NONE, &c);

    key(&mut p, K::Char('y'), Mods::NONE, &c);
    key(&mut p, K::Char('j'), Mods::NONE, &c);
    assert!(matches!(
        key(&mut p, K::Char('m'), Mods::NONE, &c),
        Action::None
    ));
    key(&mut p, K::Char('y'), Mods::NONE, &c);
    key(&mut p, K::Char('/'), Mods::NONE, &c);
    key(&mut p, K::Char('y'), Mods::NONE, &c);
    key(&mut p, K::Char('a'), Mods::NONE, &c);
    assert_eq!(p.editor.as_ref().unwrap().1, "ya");
    key(&mut p, K::Esc, Mods::NONE, &c);
    assert!(matches!(
        key(&mut p, K::Char('m'), Mods::NONE, &c),
        Action::None
    ));
}

#[test]
fn navigation_empty_and_zero_height_views_are_safe() {
    let (_d, c, mut p) = navigation_history();
    let mut t = Terminal::new(TestBackend::new(120, 1)).unwrap();
    render(&mut p, &mut t, &c);
    assert!(navigation_rows(&p).is_empty());
    for ch in ['H', 'M', 'L'] {
        key(&mut p, K::Char(ch), Mods::NONE, &c);
        assert_eq!(p.cursor, 0);
    }
    for ch in ['f', 'b', 'd', 'u'] {
        key(&mut p, K::Char(ch), Mods::CONTROL, &c);
        assert_eq!(p.cursor, 0);
    }
    t.backend_mut().resize(80, 3);
    render(&mut p, &mut t, &c);
    assert_eq!(navigation_rows(&p).len(), 1);
    key(&mut p, K::Char('d'), Mods::CONTROL, &c);
    render(&mut p, &mut t, &c);
    assert_eq!(p.cursor, 1);
    assert_eq!(navigation_rows(&p)[0].0, 1);
    key(&mut p, K::Char('u'), Mods::CONTROL, &c);
    assert_eq!(p.cursor, 0);
    p.filter.query = "nothing matches".into();
    p.rebuild_visible(None).unwrap();
    t.backend_mut().resize(120, 24);
    render(&mut p, &mut t, &c);
    for second in ['y', 'm', 'a'] {
        key(&mut p, K::Char('y'), Mods::NONE, &c);
        assert!(matches!(
            key(&mut p, K::Char(second), Mods::NONE, &c),
            Action::Notice(_)
        ));
    }
    for ch in ['g', 'g', 'G', 'H', 'M', 'L', 'j', 'k'] {
        key(&mut p, K::Char(ch), Mods::NONE, &c);
        assert_eq!(p.cursor, 0);
    }
    for ch in ['f', 'b', 'd', 'u'] {
        key(&mut p, K::Char(ch), Mods::CONTROL, &c);
        assert_eq!(p.cursor, 0);
        assert!(!p.editing());
    }
}

#[test]
fn view_options_entry_and_columns_support_keyboard_and_pointer() {
    use display::{Column, Item, MenuHit};
    let d = init();
    commit(d.path(), "first", "first commit");
    commit(d.path(), "second", "second commit");
    let c = Context::discover(d.path()).unwrap();
    for mode in [IconMode::NerdFont, IconMode::Unicode, IconMode::Ascii] {
        for width in [48, 80, 120, 180] {
            let mut p = LogPanel {
                icons: mode,
                ..Default::default()
            };
            p.refresh(&c).unwrap();
            let selected = p.rev().unwrap().oid.clone();
            let mut t = Terminal::new(TestBackend::new(width, 24)).unwrap();
            render(&mut p, &mut t, &c);
            let eye = target(&p, |h| matches!(h, Hit::ViewOptions));
            assert_eq!(row_text(&t, eye).trim(), mode.glyph(Icon::Preview));
            click_at(&mut p, eye, &c);
            render(&mut p, &mut t, &c);
            assert!(p.editing());
            for item in Item::ALL {
                let r = target(
                    &p,
                    |h| matches!(h, Hit::ViewMenu(MenuHit::Item(v)) if *v == item),
                );
                assert!(
                    row_text(&t, r).contains(item.label()),
                    "{width}: {}",
                    item.label()
                );
            }
            let roots = target(&p, |h| {
                matches!(h, Hit::ViewMenu(MenuHit::Item(Item::RootNames)))
            });
            click_at(&mut p, roots, &c);
            assert!(p.options.root_names);
            render(&mut p, &mut t, &c);
            let columns = target(&p, |h| {
                matches!(h, Hit::ViewMenu(MenuHit::Item(Item::Columns)))
            });
            click_at(&mut p, columns, &c);
            render(&mut p, &mut t, &c);
            for column in Column::ALL {
                let r = target(
                    &p,
                    |h| matches!(h, Hit::ViewMenu(MenuHit::Column(v)) if *v == column),
                );
                assert!(row_text(&t, r).contains(column.label()));
                assert!(r.right() <= width && r.bottom() <= 24);
            }
            key(&mut p, K::Enter, Mods::NONE, &c);
            assert!(!p.options.shows(Column::Author));
            key(&mut p, K::Down, Mods::NONE, &c);
            key(&mut p, K::Char(' '), Mods::NONE, &c);
            assert!(p.options.shows(Column::Hash));
            key(&mut p, K::Esc, Mods::NONE, &c);
            assert!(p.view_menu.is_some());
            key(&mut p, K::Esc, Mods::NONE, &c);
            assert!(!p.editing());
            assert_eq!(p.rev().unwrap().oid, selected);
            key(&mut p, K::Char('o'), Mods::NONE, &c);
            render(&mut p, &mut t, &c);
            let row = target(&p, |h| matches!(h, Hit::Commit(1)));
            click_at(&mut p, row, &c);
            assert!(p.view_menu.is_none());
            assert_eq!(p.cursor, 1);
        }
    }
}

fn view_item(p: &mut LogPanel, item: display::Item, c: &Context) {
    p.toggle_view_menu();
    p.view_menu_hit(display::MenuHit::Item(item), c).unwrap();
    p.view_menu = None;
}

#[test]
fn view_options_branch_or_tag_reclaims_the_left_edge_and_skips_hidden_focus() {
    let d = init();
    commit(d.path(), "first", "first commit");
    commit(d.path(), "second", "second commit");
    let c = Context::discover(d.path()).unwrap();
    for width in [48, 80, 82, 180] {
        let mut p = LogPanel {
            icons: IconMode::Ascii,
            ref_query: "main".into(),
            ..Default::default()
        };
        p.filter.branch = Some("HEAD".into());
        p.refresh(&c).unwrap();
        p.select_row(1);
        let selected = p.rev().unwrap().oid.clone();
        let area = Rect::new(3, 2, width, 21);
        let mut t = Terminal::new(TestBackend::new(area.right(), area.bottom())).unwrap();
        let draw = |p: &mut LogPanel, t: &mut Terminal<TestBackend>| {
            t.draw(|f| p.draw(f, area, &c)).unwrap();
        };
        let option = |p: &LogPanel| {
            target(p, |h| {
                matches!(h, Hit::ViewMenu(display::MenuHit::Item(Item::BranchOrTag)))
            })
        };

        key(&mut p, K::BackTab, Mods::SHIFT, &c);
        draw(&mut p, &mut t);
        let refs = target(&p, |h| matches!(h, Hit::Filter(Field::Refs)));
        assert_eq!(p.panes.rect(PaneId::Refs).unwrap().x, area.x);
        let eye = target(&p, |h| matches!(h, Hit::ViewOptions));
        click_at(&mut p, eye, &c);
        draw(&mut p, &mut t);
        let row = option(&p);
        assert!(row_text(&t, row).contains("[x] Branch or Tag"));
        click_at(&mut p, row, &c);
        draw(&mut p, &mut t);
        assert!(row_text(&t, option(&p)).contains("[ ] Branch or Tag"));
        assert_eq!(p.focus, Pane::Graph);
        assert!(p.panes.rect(PaneId::Refs).is_none());
        assert_eq!(p.panes.rect(PaneId::Graph).unwrap().x, area.x);
        assert_eq!(
            target(&p, |h| matches!(h, Hit::Filter(Field::Text))).x,
            refs.x
        );
        assert_eq!(target(&p, |h| matches!(h, Hit::ViewOptions)), eye);
        assert!(!p.hits.iter().any(|(_, h)| matches!(
            h,
            Hit::Ref(_) | Hit::Filter(Field::Refs) | Hit::Pane(Pane::Refs)
        )));
        key(&mut p, K::Esc, Mods::NONE, &c);
        key(&mut p, K::BackTab, Mods::SHIFT, &c);
        assert_eq!(p.focus, Pane::Info);

        key(&mut p, K::Char('B'), Mods::NONE, &c);
        key(&mut p, K::Char('o'), Mods::NONE, &c);
        draw(&mut p, &mut t);
        assert!(row_text(&t, option(&p)).contains("[x] Branch or Tag"));
        key(&mut p, K::Char(' '), Mods::NONE, &c);
        key(&mut p, K::Enter, Mods::NONE, &c);
        key(&mut p, K::Esc, Mods::NONE, &c);
        key(&mut p, K::Esc, Mods::NONE, &c);
        key(&mut p, K::BackTab, Mods::SHIFT, &c);
        draw(&mut p, &mut t);
        assert_eq!(p.focus, Pane::Refs);
        assert_eq!(target(&p, |h| matches!(h, Hit::Filter(Field::Refs))), refs);
        assert_eq!(p.ref_query, "main");
        assert_eq!(p.filter.single_branch(), Some("HEAD"));
        assert_eq!(p.rev().unwrap().oid, selected);
    }
}

fn view_column(p: &mut LogPanel, column: display::Column, c: &Context) {
    p.toggle_view_menu();
    p.view_menu_hit(display::MenuHit::Column(column), c)
        .unwrap();
    p.view_menu = None;
}

#[test]
fn view_options_change_rendered_roots_references_and_columns_without_changing_selection() {
    use display::{Column, Item};
    let d = init();
    commit(d.path(), "base", "create workspace");
    commit(d.path(), "next", "repair the selected account");
    git(d.path(), &["branch", "feature/extra"]);
    git(d.path(), &["tag", "v1.2.3"]);
    let c = Context::discover(d.path()).unwrap();
    let mut p = LogPanel {
        show_refs: false,
        show_info: false,
        icons: IconMode::Ascii,
        ..Default::default()
    };
    p.refresh(&c).unwrap();
    let selected = p.rev().unwrap().oid.clone();
    let mut t = Terminal::new(TestBackend::new(200, 24)).unwrap();
    render(&mut p, &mut t, &c);
    let text = row_text(&t, target(&p, |h| matches!(h, Hit::Commit(0))));
    assert!(text.contains("Log tester") && text.contains("2026/"));
    assert!(!text.contains("v1.2.3") && !text.contains(&selected[..8]));
    assert!(text.find("repair the selected account").unwrap() < text.find("main").unwrap());

    view_item(&mut p, Item::RootNames, &c);
    view_item(&mut p, Item::TagNames, &c);
    view_column(&mut p, Column::Hash, &c);
    view_column(&mut p, Column::Signature, &c);
    render(&mut p, &mut t, &c);
    let text = row_text(&t, target(&p, |h| matches!(h, Hit::Commit(0))));
    assert!(text.contains(&c.repos[0].name));
    assert!(text.contains("v1.2.3") && text.contains(&selected[..8]) && text.contains("Unsigned"));

    view_item(&mut p, Item::RefsLeft, &c);
    render(&mut p, &mut t, &c);
    let text = row_text(&t, target(&p, |h| matches!(h, Hit::Commit(0))));
    assert!(text.find("main").unwrap() < text.find("repair the selected account").unwrap());
    view_item(&mut p, Item::CompactRefs, &c);
    view_column(&mut p, Column::Author, &c);
    view_column(&mut p, Column::Date, &c);
    render(&mut p, &mut t, &c);
    let text = row_text(&t, target(&p, |h| matches!(h, Hit::Commit(0))));
    assert!(text.contains("main +2"));
    assert!(
        !text.contains("Log tester") && !text.contains("2026/") && !text.contains("feature/extra")
    );
    assert_eq!(p.rev().unwrap().oid, selected);
    key(&mut p, K::Char('t'), Mods::CONTROL, &c);
    assert!(
        p.options.shows(Column::Hash) && p.options.root_names && !p.options.shows(Column::Date)
    );
    key(&mut p, K::Char('['), Mods::NONE, &c);
    assert_eq!(p.rev().unwrap().oid, selected);
}

#[test]
fn view_options_switch_dates_and_control_each_highlight_independently() {
    use display::Item;
    use ratatui::style::Modifier;
    let d = init();
    let root = d.path();
    commit(root, "base", "base revision");
    git(root, &["checkout", "-qb", "topic"]);
    commit(root, "topic", "topic revision");
    git(root, &["checkout", "-q", "main"]);
    git(root, &["config", "user.email", "other@example.invalid"]);
    commit(root, "other", "another author");
    git(root, &["config", "user.email", "test@example.invalid"]);
    git(root, &["merge", "--no-ff", "-qm", "merge topic", "topic"]);
    let out = crate::core::command(root)
        .args(["commit", "--allow-empty", "-qm", "newest revision"])
        .env("GIT_AUTHOR_DATE", "2024-01-02T01:02:00Z")
        .env("GIT_COMMITTER_DATE", "2025-03-04T03:04:00Z")
        .output()
        .unwrap();
    assert!(out.status.success());
    let c = Context::discover(root).unwrap();
    let mut p = LogPanel {
        icons: IconMode::Ascii,
        show_refs: false,
        show_info: false,
        ..Default::default()
    };
    p.refresh(&c).unwrap();
    let mut t = Terminal::new(TestBackend::new(200, 24)).unwrap();
    let subject_cell = |p: &LogPanel, t: &Terminal<TestBackend>, text: &str| {
        let i = p
            .visible
            .iter()
            .position(|&n| p.hist.revs[n].subject == text)
            .unwrap();
        let row = target(p, |h| matches!(h, Hit::Commit(n) if *n == i));
        let x = row_text(t, row).find(text).unwrap() as u16;
        t.backend().buffer()[(row.x + x, row.y)].clone()
    };
    render(&mut p, &mut t, &c);
    assert!(subject_cell(&p, &t, "newest revision")
        .modifier
        .contains(Modifier::BOLD));
    assert!(!subject_cell(&p, &t, "another author")
        .modifier
        .contains(Modifier::BOLD));
    assert_eq!(subject_cell(&p, &t, "merge topic").fg, DIM);
    assert_ne!(subject_cell(&p, &t, "base revision").bg, PANEL);
    let row = target(&p, |h| matches!(h, Hit::Commit(0)));
    assert!(row_text(&t, row).contains("2024/1/2"));
    view_item(&mut p, Item::CommitTimestamp, &c);
    view_item(&mut p, Item::MyCommits, &c);
    view_item(&mut p, Item::MergeCommits, &c);
    view_item(&mut p, Item::CurrentBranch, &c);
    render(&mut p, &mut t, &c);
    assert!(row_text(&t, row).contains("2025/3/4"));
    assert!(!subject_cell(&p, &t, "newest revision")
        .modifier
        .contains(Modifier::BOLD));
    assert_eq!(subject_cell(&p, &t, "merge topic").fg, INK);
    assert_eq!(subject_cell(&p, &t, "base revision").bg, PANEL);
}

#[test]
fn view_options_short_menus_scroll_hover_and_fit_an_offset_viewport() {
    use display::{Item, MenuHit};
    let d = init();
    let c = Context::discover(d.path()).unwrap();
    for mode in [IconMode::Unicode, IconMode::Ascii] {
        let mut p = LogPanel {
            icons: mode,
            ..Default::default()
        };
        let mut t = Terminal::new(TestBackend::new(60, 20)).unwrap();
        let area = Rect::new(5, 3, 44, 10);
        let draw = |p: &mut LogPanel, t: &mut Terminal<TestBackend>| {
            t.draw(|f| p.draw(f, area, &c)).unwrap();
        };
        draw(&mut p, &mut t);
        key(&mut p, K::Char('o'), Mods::NONE, &c);
        for _ in 0..Item::Columns as usize {
            key(&mut p, K::Down, Mods::NONE, &c);
        }
        draw(&mut p, &mut t);
        let columns = target(&p, |h| {
            matches!(h, Hit::ViewMenu(MenuHit::Item(Item::Columns)))
        });
        p.event(
            &Event::Mouse(MouseEvent {
                kind: M::Moved,
                column: columns.x,
                row: columns.y,
                modifiers: Mods::NONE,
            }),
            &c,
        );
        draw(&mut p, &mut t);
        assert!(p
            .hits
            .iter()
            .any(|(_, h)| matches!(h, Hit::ViewMenu(MenuHit::Column(_)))));
        key(&mut p, K::Left, Mods::NONE, &c);
        for _ in 0..4 {
            key(&mut p, K::Down, Mods::NONE, &c);
        }
        draw(&mut p, &mut t);
        let last = target(&p, |h| {
            matches!(h, Hit::ViewMenu(MenuHit::Item(Item::NotCherryPicked)))
        });
        assert!(row_text(&t, last).contains("Not Cherry-Picked Commits"));
        key(&mut p, K::Enter, Mods::NONE, &c);
        assert!(!p.options.not_cherry_picked);
        for (r, h) in &p.hits {
            if matches!(h, Hit::ViewOptions | Hit::ViewMenu(_)) {
                assert!(
                    r.x >= area.x
                        && r.y >= area.y
                        && r.right() <= area.right()
                        && r.bottom() <= area.bottom()
                );
            }
        }
    }
}

#[test]
fn view_options_long_edges_restore_rails_and_shortcut_arrows_jump_to_the_real_endpoint() {
    let d = init();
    let c = Context::discover(d.path()).unwrap();
    let mut p = LogPanel {
        icons: IconMode::Ascii,
        ..Default::default()
    };
    p.hist.revs.push(model::Rev {
        oid: "child".into(),
        parents: vec!["parent".into()],
        refs: "HEAD".into(),
        ..Default::default()
    });
    p.hist.revs.extend((0..20).map(|i| model::Rev {
        oid: format!("side{i}"),
        ..Default::default()
    }));
    p.hist.revs.push(model::Rev {
        oid: "parent".into(),
        ..Default::default()
    });
    p.rebuild_visible(None).unwrap();
    let mut t = Terminal::new(TestBackend::new(120, 36)).unwrap();
    render(&mut p, &mut t, &c);
    let old = target(&p, |h| matches!(h, Hit::GraphJump(21)));
    assert_eq!(row_text(&t, old), "v");
    p.toggle_view_menu();
    p.view_menu_hit(display::MenuHit::MoveColumn(display::Column::Graph, 1), &c)
        .unwrap();
    p.toggle_view_menu();
    render(&mut p, &mut t, &c);
    let down = target(&p, |h| matches!(h, Hit::GraphJump(21)));
    assert!(down.x > old.x && down.y == old.y);
    assert_eq!(row_text(&t, down), "v");
    click_at(&mut p, old, &c);
    assert_eq!(p.cursor, 2, "former arrow only selects its row");
    view_column(&mut p, display::Column::Graph, &c);
    render(&mut p, &mut t, &c);
    assert!(!p.hits.iter().any(|(_, h)| matches!(h, Hit::GraphJump(_))));
    view_column(&mut p, display::Column::Graph, &c);
    render(&mut p, &mut t, &c);
    assert_eq!(target(&p, |h| matches!(h, Hit::GraphJump(21))), down);
    let middle = target(&p, |h| matches!(h, Hit::Commit(10)));
    assert_eq!(t.backend().buffer()[(down.x, middle.y)].symbol(), " ");
    click_at(&mut p, down, &c);
    assert_eq!(p.rev().unwrap().oid, "parent");
    p.options.long_edges = true;
    render(&mut p, &mut t, &c);
    assert!(!p.hits.iter().any(|(_, h)| matches!(h, Hit::GraphJump(_))));
    assert_eq!(t.backend().buffer()[(down.x, middle.y)].symbol(), "|");

    // A second child joins the shared rail in its middle. Shortening that run
    // would leave the incoming horizontal connection dangling.
    p.options.long_edges = false;
    p.hist.revs[5].parents = vec!["parent".into()];
    p.rebuild_visible(None).unwrap();
    render(&mut p, &mut t, &c);
    assert!(!p.hits.iter().any(|(_, h)| matches!(h, Hit::GraphJump(_))));
    assert_eq!(t.backend().buffer()[(down.x, middle.y)].symbol(), "|");
}

#[test]
fn date_menu_lists_custom_range_before_relative_presets() {
    let d = init();
    commit(d.path(), "first", "dated commit");
    let c = Context::discover(d.path()).unwrap();
    let mut p = LogPanel::default();
    p.refresh(&c).unwrap();
    for width in [48, 80, 120, 180] {
        let mut t = Terminal::new(TestBackend::new(width, 24)).unwrap();
        for keyboard in [false, true] {
            render(&mut p, &mut t, &c);
            let date = target(&p, |h| matches!(h, Hit::Filter(Field::Date)));
            if keyboard {
                filter_key(&mut p, 'd', &c);
            } else {
                click_at(&mut p, date, &c);
            }
            render(&mut p, &mut t, &c);
            let menu = target(&p, |h| matches!(h, Hit::Dropdown(DropdownHit::Surface)));
            assert_eq!(menu.y, date.bottom());
            assert_eq!(p.suggestions.len(), 3);
            for (i, label) in ["Select...", "Last 24 hours", "Last 7 days"]
                .iter()
                .enumerate()
            {
                let row = target(
                    &p,
                    |h| matches!(h, Hit::Dropdown(DropdownHit::Choice(n)) if *n == i),
                );
                assert_eq!(row_text(&t, row).trim(), *label);
                assert_eq!(row.y, menu.y + 1 + i as u16);
            }
            assert_eq!(p.choice, Some(0));
            key(&mut p, K::Esc, Mods::NONE, &c);
        }
    }
}

#[test]
fn date_presets_apply_by_mouse_and_keyboard_and_clear_independently() {
    let d = init();
    commit(d.path(), "first", "dated commit");
    let c = Context::discover(d.path()).unwrap();
    let mut t = Terminal::new(TestBackend::new(120, 24)).unwrap();
    for (keyboard, i, expected) in [
        (false, 1, DateFilter::Last24Hours),
        (true, 2, DateFilter::Last7Days),
    ] {
        let mut p = LogPanel::default();
        p.filter.author = "me".into();
        p.filter.branch = Some("HEAD".into());
        p.filter.paths = model::PathFilter::parse("first", &c).unwrap();
        p.refresh(&c).unwrap();
        filter_key(&mut p, 'd', &c);
        if keyboard {
            key(&mut p, K::Down, Mods::NONE, &c);
            key(&mut p, K::Down, Mods::NONE, &c);
            key(&mut p, K::Enter, Mods::NONE, &c);
        } else {
            render(&mut p, &mut t, &c);
            let row = target(
                &p,
                |h| matches!(h, Hit::Dropdown(DropdownHit::Choice(n)) if *n == i),
            );
            click_at(&mut p, row, &c);
        }
        assert_eq!(p.filter.date, Some(expected.clone()));
        assert_eq!(p.value(Field::Date), expected.label());
        assert!(!p.editing());

        key(&mut p, K::Char('t'), Mods::CONTROL, &c);
        assert!(p.filter.date.is_none());
        key(&mut p, K::Char('['), Mods::NONE, &c);
        assert_eq!(p.filter.date, Some(expected.clone()));
        filter_key(&mut p, 'd', &c);
        assert_eq!(p.choice, Some(i));
        key(&mut p, K::Esc, Mods::NONE, &c);

        render(&mut p, &mut t, &c);
        let clear = target(&p, |h| matches!(h, Hit::Clear(Field::Date)));
        click_at(&mut p, clear, &c);
        assert!(p.filter.date.is_none());
        assert_eq!(p.filter.author, "me");
        assert_eq!(p.filter.single_branch(), Some("HEAD"));
        assert_eq!(p.filter.paths.text(&c).unwrap(), "first");
        assert_eq!(p.visible.len(), 1);
    }
}

#[test]
fn date_range_inputs_apply_at_wide_and_narrow_sizes() {
    let d = init();
    commit(d.path(), "first", "dated commit");
    let c = Context::discover(d.path()).unwrap();
    let mut p = LogPanel::default();
    p.refresh(&c).unwrap();
    for width in [48, 80, 120, 180] {
        let mut t = Terminal::new(TestBackend::new(width, 24)).unwrap();
        for keyboard in [false, true] {
            filter_key(&mut p, 'd', &c);
            render(&mut p, &mut t, &c);
            if keyboard {
                key(&mut p, K::Enter, Mods::NONE, &c);
            } else {
                let select = target(&p, |h| matches!(h, Hit::Dropdown(DropdownHit::Choice(0))));
                click_at(&mut p, select, &c);
            }
            render(&mut p, &mut t, &c);
            let from = target(&p, |h| {
                matches!(h, Hit::Dropdown(DropdownHit::DateInput(0)))
            });
            let to = target(&p, |h| {
                matches!(h, Hit::Dropdown(DropdownHit::DateInput(1)))
            });
            assert!(row_text(&t, from).contains("From"));
            assert!(row_text(&t, to).contains("To"));
            assert!(row_text(&t, from).contains("YYYY-MM-DD"));
            p.event(&Event::Paste("2026-01-01".into()), &c);
            if keyboard {
                key(&mut p, K::Tab, Mods::NONE, &c);
            } else {
                click_at(&mut p, to, &c);
            }
            p.event(&Event::Paste("2026-12-31".into()), &c);
            render(&mut p, &mut t, &c);
            assert!(row_text(&t, from).contains("2026-01-01"));
            assert!(row_text(&t, to).contains("2026-12-31"));
            if keyboard {
                key(&mut p, K::Enter, Mods::NONE, &c);
            } else {
                let apply = target(&p, |h| matches!(h, Hit::Dropdown(DropdownHit::Apply)));
                click_at(&mut p, apply, &c);
            }
            assert_eq!(
                p.filter.date,
                Some(DateFilter::Range {
                    from: "2026-01-01".into(),
                    to: "2026-12-31".into(),
                })
            );
            assert_eq!(p.visible.len(), 1);
            assert!(!p.editing());
            p.clear_field(Field::Date, &c).unwrap();
        }
    }
}

#[test]
fn date_range_rejects_invalid_drafts_and_restores_the_applied_range() {
    let d = init();
    let c = Context::discover(d.path()).unwrap();
    let mut p = LogPanel::default();
    p.filter.date = Some(DateFilter::Last7Days);
    let mut t = Terminal::new(TestBackend::new(80, 24)).unwrap();
    filter_key(&mut p, 'd', &c);
    key(&mut p, K::Char('k'), Mods::NONE, &c);
    key(&mut p, K::Char('k'), Mods::NONE, &c);
    key(&mut p, K::Enter, Mods::NONE, &c);
    // Both dates are required; invalid input leaves the applied preset intact.
    assert!(matches!(
        key(&mut p, K::Enter, Mods::NONE, &c),
        Action::Notice(_)
    ));
    p.event(&Event::Paste("2026-02-29".into()), &c);
    key(&mut p, K::Tab, Mods::NONE, &c);
    p.event(&Event::Paste("2026-03-01".into()), &c);
    assert!(matches!(
        key(&mut p, K::Enter, Mods::NONE, &c),
        Action::Notice(_)
    ));
    assert_eq!(p.filter.date, Some(DateFilter::Last7Days));
    assert!(p.editing());
    key(&mut p, K::BackTab, Mods::SHIFT, &c);
    key(&mut p, K::Char('u'), Mods::CONTROL, &c);
    p.event(&Event::Paste("2026-03-02".into()), &c);
    let Action::Notice(error) = key(&mut p, K::Enter, Mods::NONE, &c) else {
        panic!("reversed range should fail");
    };
    assert!(error.contains("Start date"));
    assert_eq!(p.filter.date, Some(DateFilter::Last7Days));

    render(&mut p, &mut t, &c);
    let cancel = target(&p, |h| matches!(h, Hit::Dropdown(DropdownHit::Cancel)));
    click_at(&mut p, cancel, &c);
    assert!(!p.editing());
    assert!(p.date_range.is_none());
    assert_eq!(p.filter.date, Some(DateFilter::Last7Days));

    filter_key(&mut p, 'd', &c);
    key(&mut p, K::Up, Mods::NONE, &c);
    key(&mut p, K::Up, Mods::NONE, &c);
    key(&mut p, K::Enter, Mods::NONE, &c);
    p.event(&Event::Paste("2024-02-29".into()), &c);
    key(&mut p, K::Tab, Mods::NONE, &c);
    p.event(&Event::Paste("2024-02-29".into()), &c);
    key(&mut p, K::Enter, Mods::NONE, &c);
    let expected = Some(DateFilter::Range {
        from: "2024-02-29".into(),
        to: "2024-02-29".into(),
    });
    assert_eq!(p.filter.date, expected);
    assert_eq!(p.value(Field::Date), "2024-02-29");
    key(&mut p, K::Char('t'), Mods::CONTROL, &c);
    key(&mut p, K::Char('['), Mods::NONE, &c);
    assert_eq!(p.filter.date, expected);
    filter_key(&mut p, 'd', &c);
    key(&mut p, K::Enter, Mods::NONE, &c);
    assert_eq!(
        p.date_range.as_ref().unwrap().values,
        ["2024-02-29", "2024-02-29"]
    );
    key(&mut p, K::Char('u'), Mods::CONTROL, &c);
    p.event(&Event::Paste("unfinished".into()), &c);
    render(&mut p, &mut t, &c);
    let branch = target(&p, |h| matches!(h, Hit::Filter(Field::Branch)));
    click_at(&mut p, branch, &c);
    assert!(matches!(p.editor, Some((Field::Branch, _))));
    assert!(p.date_range.is_none());
    assert_eq!(p.filter.date, expected);
}

#[test]
fn branch_dropdown_opens_below_its_trigger() {
    let d = init();
    let c = Context::discover(d.path()).unwrap();
    let mut p = LogPanel::default();
    for (width, show_refs) in [(180, false), (180, true), (90, true), (48, true)] {
        p.show_refs = show_refs;
        let mut t = Terminal::new(TestBackend::new(width, 24)).unwrap();
        for keyboard in [false, true] {
            render(&mut p, &mut t, &c);
            let branch = target(&p, |h| matches!(h, Hit::Filter(Field::Branch)));
            assert!(row_text(&t, branch).starts_with("Branch"));
            if keyboard {
                filter_key(&mut p, 'b', &c);
            } else {
                click_at(&mut p, branch, &c);
            }
            render(&mut p, &mut t, &c);
            assert_eq!(
                target(&p, |h| matches!(h, Hit::Dropdown(DropdownHit::Surface))).as_position(),
                (branch.x, branch.bottom()).into(),
                "{width} columns, show_refs={show_refs}, keyboard={keyboard}"
            );
            key(&mut p, K::Esc, Mods::NONE, &c);
        }
    }
}

#[test]
fn branch_dropdown_stays_anchored_to_the_toolbar_after_resize() {
    let d = init();
    let c = Context::discover(d.path()).unwrap();
    let mut p = LogPanel::default();
    let mut wide = Terminal::new(TestBackend::new(180, 24)).unwrap();
    render(&mut p, &mut wide, &c);
    let branch = target(&p, |h| matches!(h, Hit::Filter(Field::Branch)));
    click_at(&mut p, branch, &c);
    render(&mut p, &mut wide, &c);
    assert_eq!(
        target(&p, |h| matches!(h, Hit::Dropdown(DropdownHit::Surface))).as_position(),
        (branch.x, branch.bottom()).into()
    );

    let mut narrow = Terminal::new(TestBackend::new(80, 24)).unwrap();
    render(&mut p, &mut narrow, &c);
    let branch = target(&p, |h| matches!(h, Hit::Filter(Field::Branch)));
    assert_eq!(
        target(&p, |h| matches!(h, Hit::Dropdown(DropdownHit::Surface))).as_position(),
        (branch.x, branch.bottom()).into()
    );
    let head = target(&p, |h| {
        matches!(
            h,
            Hit::Dropdown(DropdownHit::Branch(branches::Target::Row(
                branches::Row::Main(1)
            )))
        )
    });
    assert!(row_text(&narrow, head).contains(&format!("{} HEAD", p.icons.glyph(Icon::Favorite))));
    click_at(&mut p, head, &c);
    assert_eq!(p.filter.single_branch(), Some("HEAD"));
}

#[test]
fn outside_click_switches_dropdowns_and_discards_only_the_draft() {
    let d = init();
    let c = Context::discover(d.path()).unwrap();
    let mut p = LogPanel::default();
    p.filter.branch = Some("HEAD".into());
    let mut t = Terminal::new(TestBackend::new(180, 30)).unwrap();
    for field in [Field::Branch, Field::Author, Field::Date, Field::Path] {
        render(&mut p, &mut t, &c);
        let button = target(&p, |h| matches!(h, Hit::Filter(id) if *id == field));
        click_at(&mut p, button, &c);
        assert!(
            matches!(p.editor, Some((open, _)) if open == field),
            "{}",
            field.label()
        );
        render(&mut p, &mut t, &c);
        let popup = target(&p, |h| matches!(h, Hit::Dropdown(DropdownHit::Surface)));
        assert_eq!(popup.y, button.bottom());
        p.event(&Event::Paste("unfinished".into()), &c);
    }
    render(&mut p, &mut t, &c);
    let paths = target(&p, |h| matches!(h, Hit::Filter(Field::Path)));
    click_at(&mut p, paths, &c);
    assert!(!p.editing());
    assert_eq!(p.filter.single_branch(), Some("HEAD"));
    assert!(p.filter.author.is_empty() && p.filter.date.is_none() && p.filter.paths.is_empty());
}

#[test]
fn dropdown_surface_blocks_the_row_beneath_but_an_exposed_row_takes_the_first_click() {
    let d = init();
    commit(d.path(), "first", "first change");
    commit(d.path(), "second", "second change");
    let c = Context::discover(d.path()).unwrap();
    let mut p = LogPanel::default();
    p.refresh(&c).unwrap();
    let mut t = Terminal::new(TestBackend::new(180, 30)).unwrap();
    filter_key(&mut p, 'b', &c);
    render(&mut p, &mut t, &c);
    let branch = target(&p, |h| matches!(h, Hit::Filter(Field::Branch)));
    let second = target(&p, |h| matches!(h, Hit::Commit(1)));
    assert_eq!(second.y, branch.bottom() + 1);
    // The popup border covers the second commit; its author is still exposed.
    for x in [
        branch.x,
        target(&p, |h| matches!(h, Hit::Dropdown(DropdownHit::Surface))).right() - 1,
    ] {
        click_at(&mut p, Rect::new(x, second.y, 1, 1), &c);
        assert!(p.editing());
        assert_eq!(p.cursor, 0);
    }
    click_at(&mut p, second, &c);
    assert!(!p.editing());
    assert_eq!(p.cursor, 1);
    assert!(p.diff.is_none());

    filter_key(&mut p, 'p', &c);
    render(&mut p, &mut t, &c);
    let files = p.panes.rect(PaneId::Files).unwrap();
    click_at(
        &mut p,
        Rect::new(files.x, files.y + files.height / 2, 1, 1),
        &c,
    );
    assert!(!p.editing());
    assert_eq!(p.focus, Pane::Files);
    assert_eq!(p.cursor, 1);
}

#[test]
fn paths_root_checkboxes_filter_the_log_without_closing_or_changing_global_scope() {
    let workspace = tempfile::tempdir().unwrap();
    for name in ["api", "web", "worker"] {
        let dir = workspace.path().join(name);
        std::fs::create_dir(&dir).unwrap();
        git(&dir, &["init", "-q", "-b", "main"]);
        git(&dir, &["config", "user.name", "Log tester"]);
        git(&dir, &["config", "user.email", "test@example.invalid"]);
        commit(&dir, "base.txt", &format!("{name} base"));
        commit(&dir, "file [x].txt", &format!("{name} change"));
    }
    let mut c = Context::discover(workspace.path()).unwrap();
    let scope = c.scope.clone();
    let mut p = LogPanel::default();
    p.refresh(&c).unwrap();
    let mut t = Terminal::new(TestBackend::new(150, 32)).unwrap();
    let subjects = |p: &LogPanel| {
        let mut v: Vec<_> = p
            .visible
            .iter()
            .map(|&i| p.hist.revs[i].subject.clone())
            .collect();
        v.sort();
        v
    };

    filter_key(&mut p, 'p', &c);
    render(&mut p, &mut t, &c);
    let web = target(&p, |h| matches!(h, Hit::Dropdown(DropdownHit::Choice(3))));
    assert!(row_text(&t, web).contains("web") && row_text(&t, web).contains(p.icons.check(true)));
    click_at(&mut p, web, &c);
    render(&mut p, &mut t, &c);
    assert!(p.editing() && row_text(&t, web).contains(p.icons.check(false)));
    assert_eq!(
        subjects(&p),
        ["api base", "api change", "worker base", "worker change"]
    );
    key(&mut p, K::Down, Mods::NONE, &c);
    key(&mut p, K::Char(' '), Mods::NONE, &c);
    assert_eq!(subjects(&p), ["api base", "api change"]);
    key(&mut p, K::Esc, Mods::NONE, &c);
    assert_eq!(c.scope, scope);

    filter_key(&mut p, 'p', &c);
    p.event(&Event::Paste("file [x].txt".into()), &c);
    key(&mut p, K::Enter, Mods::NONE, &c);
    assert_eq!(subjects(&p), ["api change"]);
    key(&mut p, K::Char('t'), Mods::CONTROL, &c);
    assert_eq!(p.visible.len(), 6);
    key(&mut p, K::Char('['), Mods::NONE, &c);
    assert_eq!(subjects(&p), ["api change"]);
    c.scope.remove(&0);
    p.refresh(&c).unwrap();
    assert!(p.visible.is_empty());
    c.scope = scope;
    p.refresh(&c).unwrap();
    assert_eq!(subjects(&p), ["api change"]);
}

#[test]
fn paths_menu_fits_eleven_roots_and_keeps_the_last_reachable_on_a_short_screen() {
    let d = init();
    let mut c = Context::discover(d.path()).unwrap();
    let template = c.repos[0].clone();
    c.repos = (0..11)
        .map(|i| crate::core::Repo {
            name: format!("repo-{i:02}"),
            ..template.clone()
        })
        .collect();
    c.scope = (0..11).collect();
    let mut p = LogPanel {
        icons: IconMode::Ascii,
        ..Default::default()
    };
    p.filter.author = "me".into();
    let mut wide = Terminal::new(TestBackend::new(180, 36)).unwrap();
    filter_key(&mut p, 'p', &c);
    render(&mut p, &mut wide, &c);
    let header = target(&p, |h| matches!(h, Hit::Dropdown(DropdownHit::AllRoots)));
    assert!(row_text(&wide, header).contains("Roots  11/11"));
    for i in 0..11 {
        let row = target(
            &p,
            |h| matches!(h, Hit::Dropdown(DropdownHit::Choice(n)) if *n == i + paths::COMMANDS.len()),
        );
        assert!(row_text(&wide, row).contains(&format!("[x] repo-{i:02}")));
        assert_eq!(wide.backend().buffer()[(row.x, row.y)].fg, repo_tint(i));
    }
    click_at(&mut p, header, &c);
    render(&mut p, &mut wide, &c);
    assert!(row_text(&wide, header).contains("Roots  0/11"));
    let user = target(&p, |h| matches!(h, Hit::Filter(Field::Author)));
    click_at(&mut p, user, &c);
    assert!(matches!(p.editor, Some((Field::Author, _))));
    assert!(p.filter.roots.as_ref().unwrap().is_empty());
    key(&mut p, K::Esc, Mods::NONE, &c);

    let mut short = Terminal::new(TestBackend::new(48, 12)).unwrap();
    filter_key(&mut p, 'p', &c);
    key(&mut p, K::Up, Mods::NONE, &c);
    key(&mut p, K::Char(' '), Mods::NONE, &c);
    render(&mut p, &mut short, &c);
    let last = target(&p, |h| matches!(h, Hit::Dropdown(DropdownHit::Choice(12))));
    assert!(row_text(&short, last).contains("[x] repo-10"));
    key(&mut p, K::Enter, Mods::NONE, &c);
    assert_eq!(p.filter.roots.as_ref().unwrap(), &[10].into());
    render(&mut p, &mut short, &c);
    let clear = target(&p, |h| matches!(h, Hit::Clear(Field::Path)));
    click_at(&mut p, clear, &c);
    assert!(p.filter.roots.is_none());
    assert_eq!(p.filter.author, "me");
}

#[test]
fn toolbar_queries_are_scoped_and_log_tabs_restore_the_selected_commit() {
    let d = init();
    let root = d.path();
    commit(root, "one", "FIX: keep the filter");
    commit(root, "two", "fix: keep the selection");
    commit(root, "three", "docs: toolbar");
    let c = Context::discover(root).unwrap();
    let mut p = LogPanel::default();
    p.refresh(&c).unwrap();
    let mut t = Terminal::new(TestBackend::new(180, 42)).unwrap();
    render(&mut p, &mut t, &c);
    let author = target(&p, |h| matches!(h, Hit::Filter(Field::Author)));
    click_at(&mut p, author, &c);
    render(&mut p, &mut t, &c);
    let me = p.suggestions.iter().position(|(v, _)| v == "me").unwrap();
    let pick = target(
        &p,
        |h| matches!(h, Hit::Dropdown(DropdownHit::Choice(n)) if *n == me),
    );
    click_at(&mut p, pick, &c);
    assert_eq!(p.filter.author, "me");
    assert_eq!(p.visible.len(), 3);
    key(&mut p, K::Char('/'), Mods::NONE, &c);
    p.event(&Event::Paste("^fix:".into()), &c);
    key(&mut p, K::Char('r'), Mods::ALT, &c);
    key(&mut p, K::Enter, Mods::NONE, &c);
    assert_eq!(p.visible.len(), 2);
    render(&mut p, &mut t, &c);
    let cc = target(&p, |h| matches!(h, Hit::ToggleCase));
    click_at(&mut p, cc, &c);
    assert_eq!(p.visible.len(), 1);
    assert_eq!(p.rev().unwrap().subject, "fix: keep the selection");
    let oid = p.rev().unwrap().oid.clone();
    key(&mut p, K::Char('/'), Mods::NONE, &c);
    key(&mut p, K::Char('u'), Mods::CONTROL, &c);
    p.event(&Event::Paste("[".into()), &c);
    assert!(matches!(
        key(&mut p, K::Enter, Mods::NONE, &c),
        Action::Notice(_)
    ));
    assert_eq!(p.filter.query, "^fix:");
    assert_eq!(p.rev().unwrap().oid, oid);
    assert!(p.editing());
    key(&mut p, K::Esc, Mods::NONE, &c);
    key(&mut p, K::Char('t'), Mods::CONTROL, &c);
    assert_eq!(p.visible.len(), 3);
    assert!(!p.regex);
    key(&mut p, K::Char('['), Mods::NONE, &c);
    assert_eq!(p.rev().unwrap().oid, oid);
    assert!(p.regex && p.case_sensitive);
    render(&mut p, &mut t, &c);
    let clear = target(&p, |h| matches!(h, Hit::Clear(Field::Text)));
    click_at(&mut p, clear, &c);
    assert_eq!(p.visible.len(), 3);
    assert_eq!(p.filter.author, "me");
}

#[test]
fn wide_rows_prioritise_subject_before_refs_and_narrow_layout_keeps_details_reachable() {
    let d = init();
    let root = d.path();
    commit(root, "README.md", "create workspace");
    commit(root, "file.rs", "repair the selected account");
    git(root, &["branch", "fix/account"]);
    git(root, &["tag", "v1.0"]);
    let c = Context::discover(root).unwrap();
    let mut p = LogPanel::default();
    p.refresh(&c).unwrap();
    let mut t = Terminal::new(TestBackend::new(180, 42)).unwrap();
    render(&mut p, &mut t, &c);
    let first = target(&p, |h| matches!(h, Hit::Commit(0)));
    let text = row_text(&t, first);
    assert_eq!(t.backend().buffer()[(first.x, first.y)].bg, SELECT);
    assert!(text.find("Log tester").unwrap() < text.find("2026/").unwrap());
    assert!(text.find("repair the selected account").unwrap() < text.find("main").unwrap());
    assert!(!text.contains("HEAD ->"));
    let refs = p.panes.rect(PaneId::Refs).unwrap();
    let press = MouseEvent {
        kind: M::Down(MouseButton::Left),
        column: refs.right() - 1,
        row: refs.y + 3,
        modifiers: Mods::NONE,
    };
    p.event(&Event::Mouse(press), &c);
    p.event(
        &Event::Mouse(MouseEvent {
            kind: M::Drag(MouseButton::Left),
            column: press.column + 7,
            ..press
        }),
        &c,
    );
    p.event(
        &Event::Mouse(MouseEvent {
            kind: M::Up(MouseButton::Left),
            column: press.column + 7,
            ..press
        }),
        &c,
    );
    render(&mut p, &mut t, &c);
    assert!(target(&p, |h| matches!(h, Hit::Commit(0))).x > first.x);
    let mut narrow = Terminal::new(TestBackend::new(80, 24)).unwrap();
    render(&mut p, &mut narrow, &c);
    let files = target(&p, |h| matches!(h, Hit::Pane(Pane::Files)));
    click_at(&mut p, files, &c);
    render(&mut p, &mut narrow, &c);
    key(&mut p, K::Enter, Mods::NONE, &c);
    finish_preview(&mut p, &c);
    assert!(p
        .diff
        .as_ref()
        .unwrap()
        .lines
        .iter()
        .any(|l| l == "+repair the selected account"));
    key(&mut p, K::Esc, Mods::NONE, &c);
    render(&mut p, &mut narrow, &c);
    let info = target(&p, |h| matches!(h, Hit::Pane(Pane::Info)));
    click_at(&mut p, info, &c);
    render(&mut p, &mut narrow, &c);
    assert!(
        (0..24)
            .any(|y| row_text(&narrow, Rect::new(0, y, 80, 1))
                .contains("repair the selected account"))
    );
}

#[test]
fn navigation_updates_the_row_before_git_and_escape_cancels_a_waiting_diff() {
    use crossterm::event::KeyEventKind;
    let d = init();
    for (file, subject) in [
        ("a.txt", "oldest"),
        ("b.txt", "middle"),
        ("c.txt", "latest"),
    ] {
        commit(d.path(), file, subject);
    }
    let c = Context::discover(d.path()).unwrap();
    let mut p = LogPanel::default();
    p.refresh(&c).unwrap();
    finish_preview(&mut p, &c);
    let mut t = Terminal::new(TestBackend::new(140, 36)).unwrap();
    for (code, kind) in [
        ('j', KeyEventKind::Press),
        ('j', KeyEventKind::Repeat),
        ('k', KeyEventKind::Press),
        ('k', KeyEventKind::Release),
    ] {
        p.event(
            &Event::Key(KeyEvent::new_with_kind(K::Char(code), Mods::NONE, kind)),
            &c,
        );
    }
    render(&mut p, &mut t, &c);
    let row = target(&p, |h| matches!(h, Hit::Commit(1)));
    assert_eq!(t.backend().buffer()[(row.x, row.y)].bg, SELECT);
    assert_eq!(p.rev().unwrap().subject, "middle");
    assert!(p.details.files.is_empty() && p.file_index().is_none());
    let files = p.panes.rect(PaneId::Files).unwrap();
    let text = (files.y..files.bottom())
        .map(|y| row_text(&t, Rect::new(files.x, y, files.width, 1)))
        .collect::<String>();
    assert!(text.contains("Loading commit details"));
    assert!(!text.contains("c.txt") && !text.contains("No changed files"));

    key(&mut p, K::Enter, Mods::NONE, &c);
    assert!(p.diff.as_ref().unwrap().waiting_for_files);
    key(&mut p, K::Esc, Mods::NONE, &c);
    finish_preview(&mut p, &c);
    assert!(p.diff.is_none());
    assert_eq!(p.cursor, 1);
    assert_eq!(p.details.files[p.file_index().unwrap()].raw, b"b.txt");
}

#[test]
fn dense_graph_keeps_one_origin_per_frame_and_pans_to_the_selected_node() {
    let d = init();
    let c = Context::discover(d.path()).unwrap();
    let mut p = LogPanel {
        node_glyphs: NodeGlyphs::Connected,
        ..LogPanel::default()
    };
    let heads: Vec<_> = (0..18).map(|i| format!("parent{i}")).collect();
    let rev = |oid: String, parents: Vec<String>, refs: &str| model::Rev {
        root: 0,
        subject: oid.clone(),
        oid,
        parents,
        refs: refs.into(),
        author: "Dev".into(),
        date: String::new(),
        ts: 0,
        ..Default::default()
    };
    p.hist.revs.push(rev("merge".into(), heads.clone(), "HEAD"));
    p.hist
        .revs
        .extend(heads.into_iter().rev().map(|id| rev(id, vec![], "")));
    p.visible = (0..p.hist.revs.len()).collect();
    p.graph = Graph::new(&p.hist.revs);

    // A terminal that supports private connected dots must still respect an
    // explicit fallback for nodes, edges, and both kinds of offscreen marker.
    for (mode, head, dot, rail, bend, right_node, right_rail, left_node) in [
        (IconMode::Unicode, "◉", "●", "│", "╮", '▶', '›', '◀'),
        (IconMode::Ascii, "@", "*", "|", "\\", ']', '>', '['),
    ] {
        p.icons = mode;
        for width in [120, 80, 180] {
            let mut t = Terminal::new(TestBackend::new(width, 36)).unwrap();
            let cell_col = |t: &Terminal<TestBackend>, r: Rect, glyph| {
                (r.x..r.right())
                    .find(|&x| t.backend().buffer()[(x, r.y)].symbol() == glyph)
                    .unwrap()
            };
            p.cursor = 0;
            render(&mut p, &mut t, &c);
            let merge = target(&p, |h| matches!(h, Hit::Commit(0)));
            let right_tip = target(&p, |h| matches!(h, Hit::Commit(1)));
            let trunk = cell_col(&t, merge, head);
            assert_eq!(t.backend().buffer()[(trunk, right_tip.y)].symbol(), rail);
            assert!(row_text(&t, right_tip).contains(right_node));
            assert!(row_text(&t, merge).contains(right_rail));

            p.cursor = 1;
            render(&mut p, &mut t, &c);
            assert!(row_text(&t, merge).contains(left_node));
            let x = cell_col(&t, right_tip, dot);
            assert_eq!(t.backend().buffer()[(x, merge.y)].symbol(), bend);
            let origin = p.graph_origin;
            p.cursor = 2; // next parent is in the adjacent lane already on screen
            render(&mut p, &mut t, &c);
            assert_eq!(p.graph_origin, origin);
            assert!(row_text(&t, target(&p, |h| matches!(h, Hit::Commit(2)))).contains(dot));
        }
    }
}
