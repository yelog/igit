use super::*;
use crate::ui::search::CURRENT;

#[test]
fn pane_search_commit_reveals_changes_and_pans_staged_patch_without_touching_git() {
    let (_dir, c) = fixture();
    let repo = &c.repos[0].path;
    git(
        repo,
        &[
            "-c",
            "user.name=Tester",
            "-c",
            "user.email=test@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "-c",
            "core.hooksPath=/dev/null",
            "commit",
            "-qm",
            "base",
        ],
    )
    .unwrap();
    let model = "src/ｕｉ/model/page.rs";
    let view = "src/ｕｉ/view/page.rs";
    let staged = format!(
        "{}匹配 匹配\n{}\n最后匹配\n",
        "x".repeat(90),
        "plain\n".repeat(28)
    );
    fs::write(repo.join(model), &staged).unwrap();
    fs::write(repo.join(view), "unselected staged").unwrap();
    git(repo, &["add", "--", "src"]).unwrap();
    fs::write(repo.join(model), "unstaged-only\n").unwrap();
    let index = git(repo, &["ls-files", "--stage"]).unwrap();
    let head = git(repo, &["rev-parse", "HEAD^{tree}"]).unwrap();

    let mut p = CommitPanel::default();
    p.refresh(&c).unwrap();
    let other = p
        .files
        .iter()
        .find(|f| f.change.path == Path::new(view))
        .unwrap()
        .key
        .clone();
    p.checks.insert(other, false);
    let checks = p.checks.clone();
    p.set_all_expanded(false, &c);
    let mut t = Terminal::new(TestBackend::new(120, 14)).unwrap();
    let find = |p: &mut CommitPanel, term: &str| {
        key(p, &c, K::Char('/'));
        assert!(p.editing());
        p.event(&Event::Paste(term.into()), &c);
        key(p, &c, K::Enter);
        assert!(!p.editing());
    };
    find(&mut p, "PAGE.RS");
    let path = |p: &CommitPanel| {
        p.files[p.active().unwrap().file().unwrap()]
            .change
            .path
            .clone()
    };
    assert_eq!(path(&p), Path::new(model));
    assert!(p
        .view
        .iter()
        .find(|r| r.label == "view/")
        .is_none_or(|r| p.fold.contains(&r.id)));
    key(&mut p, &c, K::Char('n'));
    assert_eq!(path(&p), Path::new(view));
    key(&mut p, &c, K::Char('N'));
    assert_eq!(path(&p), Path::new(model));
    draw(&mut p, &mut t, &c);
    assert!(read(t.backend().buffer(), row_area(&p, "page.rs")).contains("page.rs"));

    p.focus_pane(PaneId::Preview);
    draw(&mut p, &mut t, &c);
    let select_all = Event::Key(KeyEvent::new(K::Char('a'), Mod::CONTROL));
    p.event(&select_all, &c);
    let preview = p.selected_text().unwrap().to_owned();
    key(&mut p, &c, K::Esc);
    find(&mut p, "匹配");
    draw(&mut p, &mut t, &c);
    assert!(p.diff_left > 0);
    let at = |p: &CommitPanel, t: &Terminal<TestBackend>| {
        let buffer = t.backend().buffer();
        (p.diff.y..p.diff.bottom())
            .flat_map(|y| (p.diff.x..p.diff.right()).map(move |x| (x, y)))
            .find(|&(x, y)| {
                buffer[(x, y)].symbol() == "匹"
                    && (buffer[(x, y)].bg == CURRENT.bg.unwrap()
                        || buffer[(x, y)]
                            .modifier
                            .contains(ratatui::style::Modifier::REVERSED))
            })
            .expect("the current hit is visible")
    };
    at(&p, &t);
    let first = p.diff_anchor;
    key(&mut p, &c, K::Char('n'));
    draw(&mut p, &mut t, &c);
    assert_eq!(
        p.diff_anchor, first,
        "n visits the second occurrence in one line"
    );
    at(&p, &t);
    key(&mut p, &c, K::Char('n'));
    draw(&mut p, &mut t, &c);
    assert_ne!(p.diff_anchor, first);
    assert!(read(t.backend().buffer(), p.diff).contains("最后匹配"));
    at(&p, &t);
    p.event(&select_all, &c);
    assert_eq!(p.selected_text(), Some(preview.as_str()));
    key(&mut p, &c, K::Esc);
    assert_eq!(p.focused_pane(), PaneId::Preview);
    assert!(p.changes_search.prompt().visible());
    assert!(!p.diff_search.prompt().visible());

    assert_eq!(p.checks, checks);
    assert_eq!(git(repo, &["ls-files", "--stage"]).unwrap(), index);
    assert_eq!(git(repo, &["rev-parse", "HEAD^{tree}"]).unwrap(), head);
    assert_eq!(
        git(repo, &["show", &format!(":{model}")]).unwrap(),
        staged.trim_end()
    );
    assert_eq!(
        fs::read_to_string(repo.join(model)).unwrap(),
        "unstaged-only\n"
    );
    assert_eq!(
        git(repo, &["show", &format!(":{view}")]).unwrap(),
        "unselected staged"
    );
}
