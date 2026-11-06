use super::*;
use ratatui::style::Modifier;

#[test]
fn repository_segments_separate_roots_even_when_their_palette_colors_repeat() {
    let (_d, mut c, mut p) = navigation_history();
    let repo = c.repos[0].clone();
    c.repos = (0..9)
        .map(|i| crate::core::Repo {
            name: format!("root-{i}"),
            ..repo.clone()
        })
        .collect();
    c.scope = (0..9).collect();
    let roots = [0, 0, 0, 1, 8, 8, 0, 2, 3];
    p.hist.revs.truncate(roots.len());
    for (rev, root) in p.hist.revs.iter_mut().zip(roots) {
        rev.root = root;
    }
    p.rebuild_visible(None).unwrap();
    p.show_refs = false;
    p.show_info = false;
    let mut t = Terminal::new(TestBackend::new(180, 24)).unwrap();
    for mode in [IconMode::NerdFont, IconMode::Unicode, IconMode::Ascii] {
        p.icons = mode;
        for width in [48, 180] {
            t.backend_mut().resize(width, 24);
            for names in [false, true] {
                p.options.root_names = names;
                render(&mut p, &mut t, &c);
                for (i, symbol) in ["╻", "┃", "╹", "❙", "╻", "╹", "❙", "❙", "❙"]
                    .into_iter()
                    .enumerate()
                {
                    let row = target(&p, |h| matches!(h, Hit::Commit(n) if *n == i));
                    let marker = &t.backend().buffer()[(row.x, row.y)];
                    assert_eq!(marker.symbol(), symbol);
                    assert_eq!(marker.fg, repo_tint(roots[i]));
                    assert_eq!(marker.bg, PANEL);
                    assert!(!marker
                        .modifier
                        .intersects(Modifier::BOLD | Modifier::REVERSED));
                    assert_eq!(row_text(&t, row).contains(&c.repos[roots[i]].name), names);
                }
                let row = target(&p, |h| matches!(h, Hit::Commit(4)));
                if p.cursor != 4 {
                    click_at(&mut p, row, &c);
                }
                render(&mut p, &mut t, &c);
                assert_eq!(p.cursor, 4);
                assert_eq!(t.backend().buffer()[(row.x, row.y)].bg, PANEL);
                assert_eq!(t.backend().buffer()[(row.x + 2, row.y)].bg, SELECT);
            }
        }
        filter_key(&mut p, 'p', &c);
        render(&mut p, &mut t, &c);
        for id in [0, 1, 8] {
            let checkbox = target(
                &p,
                |h| matches!(h, Hit::Dropdown(DropdownHit::Choice(i)) if *i == paths::COMMANDS.len() + id),
            );
            assert_eq!(
                t.backend().buffer()[(checkbox.x, checkbox.y)].fg,
                repo_tint(id)
            );
        }
        key(&mut p, K::Esc, Mods::NONE, &c);
    }
    // Filtering joins the visible root-0 runs; hidden roots must not leave caps.
    p.filter.roots = Some([0].into());
    p.rebuild_visible(None).unwrap();
    render(&mut p, &mut t, &c);
    for (i, symbol) in ["╻", "┃", "┃", "╹"].into_iter().enumerate() {
        let row = target(&p, |h| matches!(h, Hit::Commit(n) if *n == i));
        assert_eq!(t.backend().buffer()[(row.x, row.y)].symbol(), symbol);
    }
}

#[test]
fn repository_segments_keep_continuations_at_scrolled_and_resized_viewport_edges() {
    let (_d, c, mut p) = navigation_history();
    let mut t = Terminal::new(TestBackend::new(180, 24)).unwrap();
    for (width, height) in [(48, 12), (80, 24), (180, 36)] {
        t.backend_mut().resize(width, height);
        p.select_row(35);
        render(&mut p, &mut t, &c);
        let rows = navigation_rows(&p);
        assert!(rows[0].0 > 0);
        for (_, row) in [rows[0], *rows.last().unwrap()] {
            assert_eq!(t.backend().buffer()[(row.x, row.y)].symbol(), "┃");
        }
        key(&mut p, K::Char('G'), Mods::NONE, &c);
        render(&mut p, &mut t, &c);
        let (index, last) = *navigation_rows(&p).last().unwrap();
        assert_eq!(index, 79);
        assert_eq!(t.backend().buffer()[(last.x, last.y)].symbol(), "╹");
        for _ in 0..2 {
            key(&mut p, K::Char('g'), Mods::NONE, &c);
        }
        render(&mut p, &mut t, &c);
        let (index, first) = navigation_rows(&p)[0];
        assert_eq!(index, 0);
        assert_eq!(t.backend().buffer()[(first.x, first.y)].symbol(), "╻");
    }
}
