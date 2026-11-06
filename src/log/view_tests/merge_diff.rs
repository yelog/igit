use super::*;
use std::fs;

#[test]
fn merge_inspector_opens_combined_and_each_parent_without_changing_git_state() {
    let (_tmp, path, rev) = model::merge_diff::tests::fixture();
    fs::write(path.join("example.js"), "unrelated staged content\n").unwrap();
    git(&path, &["add", "example.js"]);
    fs::write(path.join("example.js"), "unrelated working content\n").unwrap();
    let before = fs::read(path.join(".git/index")).unwrap();
    let c = Context::discover(&path).unwrap();
    let mut p = LogPanel::default();
    p.refresh(&c).unwrap();
    let n = p
        .visible
        .iter()
        .position(|&i| p.hist.revs[i].oid == rev.oid)
        .unwrap();
    p.select_row(n);
    finish_preview(&mut p, &c);
    p.focus = Pane::Files;
    let mut t = Terminal::new(TestBackend::new(220, 40)).unwrap();
    render(&mut p, &mut t, &c);
    let screen = (0..40)
        .map(|y| row_text(&t, Rect::new(0, y, 220, 1)))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(screen.contains(&format!("Changes to {}", &rev.parents[0][..8])));
    assert!(screen.contains(&format!("Changes to {}", &rev.parents[1][..8])));
    assert_eq!(p.details.files.len(), 4);
    assert_eq!(p.file_index(), Some(0));
    key(&mut p, K::Enter, Mods::NONE, &c);
    assert!(p.diff.as_ref().unwrap().comparison.is_some());
    render(&mut p, &mut t, &c);
    let screen = (0..40)
        .map(|y| row_text(&t, Rect::new(0, y, 220, 1)))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(screen.contains("Repository Diff: example.js"));
    assert!(screen.contains("resolved from both"));
    assert!(screen.contains(&rev.parents[0][..8]) && screen.contains(&rev.parents[1][..8]));
    assert!(screen.contains("No conflicts") && !screen.contains("Apply"));
    key(&mut p, K::Char(']'), Mods::NONE, &c);
    let diff = p.diff.as_ref().unwrap();
    assert!(diff.comparison.as_ref().unwrap().is_three_way());
    render(&mut p, &mut t, &c);
    assert!(
        matches!(key(&mut p, K::Char('y'), Mods::NONE, &c), Action::Copy { text, .. } if text == "resolved from both\n")
    );
    key(&mut p, K::Char(']'), Mods::NONE, &c);
    let diff = p.diff.as_ref().unwrap();
    assert!(diff.name.contains("README.md"));
    assert!(diff.comparison.as_ref().unwrap().is_three_way());
    render(&mut p, &mut t, &c);
    key(&mut p, K::Tab, Mods::NONE, &c);
    assert!(
        matches!(key(&mut p, K::Char('y'), Mods::NONE, &c), Action::Copy { text, .. } if text == "base readme\n")
    );
    key(&mut p, K::Esc, Mods::NONE, &c);
    p.file_fold_all(true);
    assert!(p.file_index().is_none());
    p.file_fold_all(false);
    render(&mut p, &mut t, &c);
    // The repeated filename under parent 2 must retain its own parent ID.
    let n = p
        .hits
        .iter()
        .rev()
        .find_map(|(_, h)| match h {
            Hit::File(n) => Some(*n),
            _ => None,
        })
        .unwrap();
    p.file_activate(n, false);
    assert_eq!(p.file_index(), Some(3));
    key(&mut p, K::Enter, Mods::NONE, &c);
    assert!(p
        .diff
        .as_ref()
        .unwrap()
        .comparison
        .as_ref()
        .unwrap()
        .is_three_way());
    render(&mut p, &mut t, &c);
    key(&mut p, K::Tab, Mods::NONE, &c);
    assert!(
        matches!(key(&mut p, K::Char('y'), Mods::NONE, &c), Action::Copy { text, .. } if text == "right\n")
    );
    assert_eq!(before, fs::read(path.join(".git/index")).unwrap());
    assert_eq!(
        fs::read_to_string(path.join("example.js")).unwrap(),
        "unrelated working content\n"
    );
    assert_eq!(git(&path, &["rev-parse", "HEAD"]), rev.oid);
}

#[test]
fn ordinary_commits_open_before_after_for_root_modify_add_and_delete() {
    let tmp = init();
    let path = tmp.path();
    commit(path, "sample.js", "original\n");
    let first = git(path, &["rev-parse", "HEAD"]);
    commit(path, "sample.js", "\tupdated ＵＩ\n");
    let second = git(path, &["rev-parse", "HEAD"]);
    commit(path, "new.js", "new file\n");
    let added = git(path, &["rev-parse", "HEAD"]);
    git(path, &["rm", "sample.js"]);
    git(path, &["commit", "-qm", "Delete sample"]);
    let deleted = git(path, &["rev-parse", "HEAD"]);
    fs::write(path.join("new.js"), "unrelated staged\n").unwrap();
    git(path, &["add", "new.js"]);
    fs::write(path.join("new.js"), "unrelated unstaged\n").unwrap();
    let before_index = fs::read(path.join(".git/index")).unwrap();
    let c = Context::discover(path).unwrap();
    let mut p = LogPanel::default();
    p.refresh(&c).unwrap();
    let mut t = Terminal::new(TestBackend::new(140, 32)).unwrap();
    for (oid, before, after) in [
        (&first, "", "original\n"),
        (&second, "original\n", "\tupdated ＵＩ\n"),
        (&added, "", "new file\n"),
        (&deleted, "\tupdated ＵＩ\n", ""),
    ] {
        let n = p
            .visible
            .iter()
            .position(|&i| &p.hist.revs[i].oid == oid)
            .unwrap();
        p.select_row(n);
        finish_preview(&mut p, &c);
        p.focus = Pane::Files;
        key(&mut p, K::Enter, Mods::NONE, &c);
        assert!(!p
            .diff
            .as_ref()
            .unwrap()
            .comparison
            .as_ref()
            .unwrap()
            .is_three_way());
        render(&mut p, &mut t, &c);
        let heading = row_text(&t, Rect::new(0, 2, 140, 1));
        assert!(heading.contains("Before") && heading.contains("After"));
        assert!(!heading.contains("Parent 2"));
        assert!(
            matches!(key(&mut p, K::Char('y'), Mods::NONE, &c), Action::Copy { text, .. } if text == after)
        );
        key(&mut p, K::Tab, Mods::NONE, &c);
        assert!(
            matches!(key(&mut p, K::Char('y'), Mods::NONE, &c), Action::Copy { text, .. } if text == before)
        );
        key(&mut p, K::Tab, Mods::NONE, &c);
        assert!(
            matches!(key(&mut p, K::Char('y'), Mods::NONE, &c), Action::Copy { text, .. } if text == after)
        );
        key(&mut p, K::Esc, Mods::NONE, &c);
    }
    assert_eq!(fs::read(path.join(".git/index")).unwrap(), before_index);
    assert_eq!(
        fs::read_to_string(path.join("new.js")).unwrap(),
        "unrelated unstaged\n"
    );
    assert_eq!(git(path, &["rev-parse", "HEAD"]), deleted);
}
