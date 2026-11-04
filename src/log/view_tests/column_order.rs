use super::*;
use display::{Column, Item, MenuHit};
use Column::{Author, Date, Graph, Hash, Message, Signature};

fn open_columns(p: &mut LogPanel, c: &Context) {
    key(p, K::Char('o'), Mods::NONE, c);
    for _ in 0..Item::Columns as usize {
        key(p, K::Down, Mods::NONE, c);
    }
    key(p, K::Right, Mods::NONE, c);
}

fn column_row(p: &LogPanel, column: Column) -> Rect {
    target(
        p,
        |h| matches!(h, Hit::ViewMenu(MenuHit::Column(c)) if *c == column),
    )
}

fn assert_menu_order(p: &LogPanel, columns: [Column; Column::ALL.len()]) {
    let rows = columns.map(|col| column_row(p, col).y);
    assert!(rows.windows(2).all(|r| r[0] < r[1]), "menu rows: {rows:?}");
}

fn assert_text_order(text: &str, labels: &[&str]) {
    let mut rest = text;
    for &label in labels {
        rest = rest
            .split_once(label)
            .unwrap_or_else(|| panic!("missing or misplaced {label:?} in {text:?}"))
            .1;
    }
}

#[test]
fn view_options_reorder_graph_and_message_with_long_titles_and_refs() {
    let d = init();
    commit(
        d.path(),
        "one",
        &format!("列布局 {}", "很长的标题".repeat(40)),
    );
    git(d.path(), &["tag", "layout-tag"]);
    let c = Context::discover(d.path()).unwrap();
    let mut p = LogPanel {
        icons: IconMode::Unicode,
        show_refs: false,
        show_info: false,
        ..Default::default()
    };
    p.options.tag_names = true;
    p.refresh(&c).unwrap();
    let selected = p.rev().unwrap().oid.clone();
    let mut t = Terminal::new(TestBackend::new(160, 24)).unwrap();
    open_columns(&mut p, &c);
    render(&mut p, &mut t, &c);
    for col in [Graph, Message] {
        let text = row_text(&t, column_row(&p, col));
        assert!(
            text.contains(col.label()) && text.contains(p.icons.check(true)),
            "{text}"
        );
    }
    // Move Message from the last row to the first, then one place to the right.
    key(&mut p, K::Up, Mods::NONE, &c);
    key(&mut p, K::Down, Mods::SHIFT, &c); // the last column cannot move farther
    for _ in 0..5 {
        key(&mut p, K::Up, Mods::SHIFT, &c);
    }
    key(&mut p, K::Char('J'), Mods::NONE, &c);
    key(&mut p, K::Char('o'), Mods::NONE, &c);
    let line = |p: &mut LogPanel, t: &mut Terminal<TestBackend>| {
        render(p, t, &c);
        row_text(t, target(p, |h| matches!(h, Hit::Commit(0))))
    };
    assert_text_order(
        &line(&mut p, &mut t),
        &["Log tester", "列布局", "…", "layout-tag", "2026/", "◉"],
    );
    view_item(&mut p, Item::RefsLeft, &c);
    assert_text_order(
        &line(&mut p, &mut t),
        &["Log tester", "layout-tag", "列布局", "…", "2026/", "◉"],
    );

    // Both columns remain available even in a short menu.
    t.backend_mut().resize(48, 7);
    open_columns(&mut p, &c);
    key(&mut p, K::Up, Mods::NONE, &c); // Graph is now last
    render(&mut p, &mut t, &c);
    let graph = column_row(&p, Graph);
    click_at(&mut p, graph, &c);
    key(&mut p, K::Char('o'), Mods::NONE, &c);
    let text = line(&mut p, &mut t);
    assert!(text.contains("列布局") && !text.contains('◉'), "{text}");
    open_columns(&mut p, &c);
    key(&mut p, K::Down, Mods::NONE, &c); // Message, between Author and Hash
    key(&mut p, K::Char(' '), Mods::NONE, &c);
    key(&mut p, K::Char('o'), Mods::NONE, &c);
    let text = line(&mut p, &mut t);
    assert!(
        !text.contains("列布局") && !text.contains("layout-tag") && !text.contains('◉'),
        "{text}"
    );
    assert!(
        text.contains("Log test"),
        "hidden Graph and Message should free space for author: {text}"
    );
    view_column(&mut p, Graph, &c);
    let text = line(&mut p, &mut t);
    assert!(
        text.contains('◉') && !text.contains("列布局") && !text.contains("layout-tag"),
        "{text}"
    );
    view_column(&mut p, Message, &c);
    t.backend_mut().resize(160, 24);
    assert_text_order(
        &line(&mut p, &mut t),
        &["Log tester", "layout-tag", "列布局", "…", "2026/", "◉"],
    );
    assert_eq!(p.rev().unwrap().oid, selected);
}

#[test]
fn view_options_reorder_keys_update_log_rows_and_share_order_across_tabs() {
    let d = init();
    commit(d.path(), "one", "ordered first");
    commit(d.path(), "two", "ordered second");
    let c = Context::discover(d.path()).unwrap();
    let mut p = LogPanel {
        show_refs: false,
        show_info: false,
        icons: IconMode::Ascii,
        ..Default::default()
    };
    p.filter.query = "ordered".into();
    p.refresh(&c).unwrap();
    p.select_row(1);
    view_column(&mut p, Hash, &c);
    view_column(&mut p, Signature, &c);
    let selected = p.rev().unwrap().oid.clone();
    // A presentation-only move must not reload history, even when it moves the signature column.
    commit(d.path(), "three", "ordered later");
    let mut t = Terminal::new(TestBackend::new(180, 24)).unwrap();
    open_columns(&mut p, &c);
    for _ in 0..3 {
        key(&mut p, K::Char('j'), Mods::NONE, &c); // select Signature
    }
    key(&mut p, K::Char('K'), Mods::SHIFT, &c);
    key(&mut p, K::Up, Mods::SHIFT, &c);
    key(&mut p, K::Char('K'), Mods::NONE, &c);
    key(&mut p, K::Up, Mods::SHIFT, &c); // already first
    render(&mut p, &mut t, &c);
    assert_menu_order(&p, [Signature, Author, Hash, Date, Graph, Message]);
    let sig = column_row(&p, Signature);
    assert_eq!(t.backend().buffer()[(sig.x, sig.y)].bg, SELECT);
    key(&mut p, K::Char('J'), Mods::NONE, &c);
    key(&mut p, K::Down, Mods::SHIFT, &c);
    key(&mut p, K::Esc, Mods::NONE, &c);
    key(&mut p, K::Esc, Mods::NONE, &c);
    assert_eq!(p.visible.len(), 2);
    assert_eq!(p.filter.query, "ordered");
    assert_eq!(p.rev().unwrap().oid, selected);

    let check_row = |p: &mut LogPanel, t: &mut Terminal<TestBackend>| {
        render(p, t, &c);
        let text = row_text(
            t,
            target(p, |h| matches!(h, Hit::Commit(i) if *i == p.cursor)),
        );
        let rev = p.rev().unwrap();
        assert_text_order(
            &text,
            &[
                "Log tester",
                &rev.oid[..8],
                "Unsigned",
                "2026/",
                &rev.subject,
            ],
        );
    };
    check_row(&mut p, &mut t);
    key(&mut p, K::Char('t'), Mods::CONTROL, &c);
    check_row(&mut p, &mut t);
    key(&mut p, K::Char('['), Mods::NONE, &c);
    check_row(&mut p, &mut t);
    assert_eq!(p.rev().unwrap().oid, selected);
}

#[test]
fn view_options_reorder_mouse_moves_hidden_columns_without_toggling_them() {
    let d = init();
    commit(d.path(), "one", "short title");
    let c = Context::discover(d.path()).unwrap();
    for (mode, width, height) in [
        (IconMode::Ascii, 48, 12),
        (IconMode::Unicode, 80, 24),
        (IconMode::NerdFont, 180, 36),
    ] {
        let mut p = LogPanel {
            icons: mode,
            show_refs: false,
            show_info: false,
            ..Default::default()
        };
        p.refresh(&c).unwrap();
        let mut t = Terminal::new(TestBackend::new(width, height)).unwrap();
        open_columns(&mut p, &c);
        render(&mut p, &mut t, &c);
        let click_arrow = |p: &mut LogPanel, t: &mut Terminal<TestBackend>, icon: Icon| {
            let row = column_row(p, Hash);
            let child = target(p, |h| matches!(h, Hit::ViewMenu(MenuHit::Surface)));
            let x = (row.x..child.right())
                .find(|&x| t.backend().buffer()[(x, row.y)].symbol() == mode.glyph(icon))
                .expect("visible reorder arrow on the Hash row");
            click_at(p, Rect::new(x, row.y, 1, 1), &c);
            render(p, t, &c);
        };
        click_arrow(&mut p, &mut t, Icon::Up);
        click_arrow(&mut p, &mut t, Icon::Up); // disabled at the first row
        assert_menu_order(&p, [Hash, Author, Date, Signature, Graph, Message]);
        assert!(row_text(&t, column_row(&p, Hash)).contains(mode.check(false)));
        click_arrow(&mut p, &mut t, Icon::Down);
        click_arrow(&mut p, &mut t, Icon::Down);
        assert_menu_order(&p, [Author, Date, Hash, Signature, Graph, Message]);
        let row = column_row(&p, Hash);
        click_at(&mut p, row, &c);
        render(&mut p, &mut t, &c);
        assert!(row_text(&t, column_row(&p, Hash)).contains(mode.check(true)));
        key(&mut p, K::Char('o'), Mods::NONE, &c);
        render(&mut p, &mut t, &c);
        let text = row_text(&t, target(&p, |h| matches!(h, Hit::Commit(0))));
        assert!(text.contains("short title"), "{width}: {text}");
        t.backend_mut().resize(180, 24);
        render(&mut p, &mut t, &c);
        let text = row_text(&t, target(&p, |h| matches!(h, Hit::Commit(0))));
        assert_text_order(
            &text,
            &[
                "Log tester",
                "2026/",
                &p.rev().unwrap().oid[..8],
                "short title",
            ],
        );
    }
}
