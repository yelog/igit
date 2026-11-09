use super::*;

fn many_branches(p: &mut BranchPanel) {
    let state = p.repos[0].state.as_mut().unwrap();
    let base = state.branches["refs/heads/main"].clone();
    for i in 0..80 {
        let name = format!("topic-{i:02}");
        let reference = format!("refs/heads/{name}");
        state.branches.insert(
            reference.clone(),
            Branch {
                name,
                reference,
                current: false,
                worktree: String::new(),
                ..base.clone()
            },
        );
    }
    p.tree.rebuild(p.repos.first());
}

fn ctrl(p: &mut BranchPanel, c: &Context, ch: char) {
    assert!(matches!(
        p.event(&Event::Key(KeyEvent::new(K::Char(ch), Mods::CONTROL)), c),
        Action::None
    ));
}

#[test]
fn switching_repositories_during_refresh_keeps_the_latest_selection_and_folds() {
    let (_temp, c, mut p) = fixture();
    let other = &c.repos[1].path;
    run(other, &["branch", "remember-this"]);
    run(other, &["update-ref", "refs/remotes/origin/other", "HEAD"]);
    reload(&mut p, &c);
    p.focus_pane(PaneId::BranchRepos);
    key(&mut p, &c, K::Down);
    key(&mut p, &c, K::Enter);
    select(&mut p, 1, "refs/heads/remember-this");
    p.focus_pane(PaneId::BranchRepos);
    key(&mut p, &c, K::Up);

    run(other, &["branch", "new-since-refresh"]);
    p.refresh(&c).unwrap();
    let release = p.hold_loading(&c);
    key(&mut p, &c, K::Down);
    key(&mut p, &c, K::Enter);
    assert_eq!(p.selection().unwrap().1.name, "remember-this");
    assert!(!screen(&mut p, &c, 120, 36).contains("new-since-refresh"));
    let origin = "1:refs/remotes/origin/";
    p.tree.cursor = folder_index(&p, origin);
    key(&mut p, &c, K::Char('h'));
    select(&mut p, 1, "refs/heads/remember-this");
    release();
    p.finish_loading(&c);

    assert_eq!(p.selection().unwrap().1.name, "remember-this");
    assert!(p.tree.rows.iter().all(|row| row.root == 1));
    assert!(matches!(
        p.tree.rows[folder_index(&p, origin)].kind,
        Kind::Folder { open: false, .. }
    ));
    assert!(!p.tree.rows.iter().any(
        |row| matches!(&row.kind, Kind::Branch(b) if b.reference == "refs/remotes/origin/other")
    ));
    assert!(screen(&mut p, &c, 120, 36).contains("new-since-refresh"));
}

#[test]
fn branch_groups_start_open_and_keep_explicit_folds_across_refresh_and_scope() {
    let (_temp, mut c, mut p) = fixture();
    let repo = &c.repos[0].path;
    tracking(&c, "main", "main");
    run(repo, &["branch", "feature/team/local"]);
    run(
        repo,
        &[
            "update-ref",
            "refs/remotes/origin/topic/team/remote",
            "HEAD",
        ],
    );
    reload(&mut p, &c);
    let remote = "0:refs/remotes/";
    let topic = "0:refs/remotes/origin/topic/";
    let leaf = "0:refs/remotes/origin/topic/team/remote";
    let is_open = |p: &BranchPanel, id| {
        matches!(
            p.tree.rows[folder_index(p, id)].kind,
            Kind::Folder { open: true, .. }
        )
    };
    assert!(p
        .tree
        .rows
        .iter()
        .all(|r| !matches!(r.kind, Kind::Folder { open: false, .. })));
    assert!(p
        .tree
        .rows
        .iter()
        .any(|r| r.id == "0:refs/heads/feature/team/local"));
    assert!(p.tree.rows.iter().any(|r| r.id == leaf));

    p.tree.cursor = folder_index(&p, topic);
    key(&mut p, &c, K::Char('h'));
    p.tree.cursor = folder_index(&p, remote);
    key(&mut p, &c, K::Char('h'));
    reload(&mut p, &c);
    assert!(!is_open(&p, remote));
    c.scope.remove(&0);
    reload(&mut p, &c);
    c.scope.insert(0);
    reload(&mut p, &c);
    p.select_repository(0);
    assert!(!is_open(&p, remote));
    p.tree.cursor = folder_index(&p, remote);
    key(&mut p, &c, K::Char('l'));
    assert!(is_open(&p, remote));
    assert!(!is_open(&p, topic));
    assert!(p
        .tree
        .rows
        .iter()
        .any(|row| row.id == "0:refs/remotes/origin/main"));
    assert!(!p.tree.rows.iter().any(|r| r.id == leaf));
    p.tree.cursor = folder_index(&p, topic);
    key(&mut p, &c, K::Char('l'));
    assert!(p.tree.rows.iter().any(|r| r.id == leaf));
}

#[test]
fn first_row_requires_gg_and_interrupted_prefixes_do_not_move() {
    let (_temp, c, mut p) = fixture();
    many_branches(&mut p);
    screen(&mut p, &c, 80, 18);
    p.tree.cursor = 40;
    key(&mut p, &c, K::Char('g'));
    assert_eq!(p.tree.cursor, 40);
    p.event(
        &Event::Key(KeyEvent::new_with_kind(
            K::Char('g'),
            Mods::NONE,
            KeyEventKind::Release,
        )),
        &c,
    );
    key(&mut p, &c, K::Char('g'));
    assert_eq!(p.tree.cursor, 0);
    for interrupt in [
        Event::Key(KeyEvent::new(K::Esc, Mods::NONE)),
        Event::Key(KeyEvent::new(K::Char('k'), Mods::NONE)),
        Event::Resize(60, 14),
        Event::FocusLost,
        Event::Mouse(MouseEvent {
            kind: M::ScrollDown,
            column: p.viewport.area.x,
            row: p.viewport.area.y,
            modifiers: Mods::NONE,
        }),
    ] {
        p.tree.cursor = 40;
        key(&mut p, &c, K::Char('g'));
        p.event(&interrupt, &c);
        let cursor = p.tree.cursor;
        key(&mut p, &c, K::Char('g'));
        assert_eq!(p.tree.cursor, cursor, "{interrupt:?}");
        key(&mut p, &c, K::Esc);
    }
    key(&mut p, &c, K::Home);
    assert_eq!(p.tree.cursor, 0);
    key(&mut p, &c, K::Char('G'));
    assert_eq!(p.tree.cursor, p.tree.rows.len() - 1);
}

#[test]
fn screen_motions_target_visible_rows_without_scrolling() {
    let (_temp, c, mut p) = fixture();
    many_branches(&mut p);
    for (width, height) in [(120, 36), (80, 18), (44, 12)] {
        p.tree.cursor = 40;
        screen(&mut p, &c, width, height);
        let top = p.viewport.top;
        let count = usize::from(p.viewport.area.height);
        for (ch, expected) in [
            ('H', top),
            ('M', top + (count - 1) / 2),
            ('L', top + count - 1),
        ] {
            p.event(&Event::Key(KeyEvent::new(K::Char(ch), Mods::SHIFT)), &c);
            assert_eq!(p.tree.cursor, expected);
            screen(&mut p, &c, width, height);
            assert_eq!(p.viewport.top, top);
        }
    }
}

#[test]
fn full_and_half_pages_move_viewport_and_cursor_and_clamp_at_edges() {
    let (_temp, c, mut p) = fixture();
    many_branches(&mut p);
    for (width, height) in [(120, 36), (80, 18), (44, 12)] {
        p.tree.cursor = 0;
        p.viewport.top = 0;
        screen(&mut p, &c, width, height);
        let rows = usize::from(p.viewport.area.height);
        ctrl(&mut p, &c, 'f');
        assert_eq!((p.tree.cursor, p.viewport.top), (rows, rows));
        ctrl(&mut p, &c, 'd');
        assert_eq!(
            (p.tree.cursor, p.viewport.top),
            (rows + rows / 2, rows + rows / 2)
        );
        ctrl(&mut p, &c, 'u');
        ctrl(&mut p, &c, 'b');
        assert_eq!((p.tree.cursor, p.viewport.top), (0, 0));
        ctrl(&mut p, &c, 'b');
        assert_eq!((p.tree.cursor, p.viewport.top), (0, 0));
        key(&mut p, &c, K::PageDown);
        assert_eq!((p.tree.cursor, p.viewport.top), (rows, rows));
        key(&mut p, &c, K::PageUp);
        assert_eq!((p.tree.cursor, p.viewport.top), (0, 0));
        key(&mut p, &c, K::End);
        ctrl(&mut p, &c, 'f');
        let len = p.tree.rows.len();
        assert_eq!((p.tree.cursor, p.viewport.top), (len - 1, len - rows));
        assert!(p.input.is_none() && p.review.is_none());
    }
}

#[test]
fn batched_moves_and_resize_keep_cursor_visible() {
    let (_temp, c, mut p) = fixture();
    many_branches(&mut p);
    p.tree.cursor = 0;
    screen(&mut p, &c, 80, 18);
    let rows = usize::from(p.viewport.area.height);
    for _ in 0..rows + 5 {
        key(&mut p, &c, K::Char('j'));
    }
    for _ in 0..rows - 1 {
        key(&mut p, &c, K::Char('k'));
    }
    assert_eq!((p.tree.cursor, p.viewport.top), (6, 6));
    screen(&mut p, &c, 44, 10);
    assert_eq!((p.tree.cursor, p.viewport.top), (6, 6));
    p.tree.cursor = p.tree.rows.len() - 1;
    screen(&mut p, &c, 80, 25);
    key(&mut p, &c, K::Char('H'));
    assert_eq!(p.tree.cursor, p.viewport.top);
}

#[test]
fn screen_and_page_motions_handle_short_empty_and_zero_height_lists() {
    let (_temp, c, mut p) = fixture();
    screen(&mut p, &c, 120, 36);
    key(&mut p, &c, K::Char('L'));
    assert_eq!(p.tree.cursor, p.tree.rows.len() - 1);
    key(&mut p, &c, K::Char('M'));
    assert_eq!(p.tree.cursor, (p.tree.rows.len() - 1) / 2);
    for (width, height) in [(1, 1), (3, 3), (44, 6)] {
        screen(&mut p, &c, width, height);
        for ch in ['f', 'b', 'd', 'u'] {
            ctrl(&mut p, &c, ch);
        }
    }
    p.repos.clear();
    p.tree.rebuild(p.repos.first());
    screen(&mut p, &c, 44, 12);
    for ch in ['H', 'M', 'L', 'G', 'g', 'g'] {
        key(&mut p, &c, K::Char(ch));
    }
    for ch in ['f', 'b', 'd', 'u'] {
        ctrl(&mut p, &c, ch);
    }
    assert_eq!(p.tree.cursor, 0);
}
