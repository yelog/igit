use super::*;
use std::path::Path;

const LITERAL_NAME: &str = if cfg!(windows) { "[x].rs" } else { "[x]*.rs" };

#[test]
fn paths_menu_offers_both_selectors_before_roots() {
    let d = init();
    let c = Context::discover(d.path()).unwrap();
    for (w, h) in [(48, 12), (80, 24), (180, 40)] {
        let mut p = LogPanel::default();
        let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
        filter_key(&mut p, 'p', &c);
        render(&mut p, &mut t, &c);
        let menu = target(&p, |h| matches!(h, Hit::Dropdown(DropdownHit::Surface)));
        let rows: Vec<_> = (menu.y..menu.bottom())
            .map(|y| row_text(&t, Rect::new(menu.x, y, menu.width, 1)))
            .collect();
        assert!(rows[1].contains("Select..."), "{rows:?}");
        assert!(rows[2].contains("Select in Tree..."), "{rows:?}");
        assert!(rows[3].contains("Roots"), "{rows:?}");
    }
}

fn workspace() -> (tempfile::TempDir, Context, LogPanel) {
    let d = tempfile::tempdir().unwrap();
    for name in ["api", "web"] {
        let repo = d.path().join(name);
        std::fs::create_dir_all(repo.join("lib")).unwrap();
        std::fs::create_dir_all(repo.join("docs")).unwrap();
        git(&repo, &["init", "-q", "-b", "main"]);
        git(&repo, &["config", "user.name", "Path Tester"]);
        git(&repo, &["config", "user.email", "test@example.invalid"]);
        let literal = format!("lib/{LITERAL_NAME}");
        for (file, label) in [
            (literal.as_str(), "literal"),
            ("lib/x.rs", "decoy"),
            ("docs/read me.md", "docs"),
            ("web", "namesake"),
        ] {
            commit(&repo, file, &format!("{name} {label}"));
        }
    }
    let c = Context::discover(d.path()).unwrap();
    let mut p = LogPanel {
        icons: IconMode::Ascii,
        ..Default::default()
    };
    p.refresh(&c).unwrap();
    (d, c, p)
}

fn subjects(p: &LogPanel) -> Vec<&str> {
    let mut rows: Vec<_> = p
        .visible
        .iter()
        .map(|i| p.hist.revs[*i].subject.as_str())
        .collect();
    rows.sort_unstable();
    rows
}

fn open(p: &mut LogPanel, c: &Context, tree: bool) {
    filter_key(p, 'p', c);
    if tree {
        key(p, K::Down, Mods::NONE, c);
    }
    key(p, K::Enter, Mods::NONE, c);
}

fn ref_workspace() -> (tempfile::TempDir, Context, LogPanel) {
    let (d, c, mut p) = workspace();
    for repo in &c.repos {
        git(&repo.path, &["branch", &format!("{}-local", repo.name)]);
        git(
            &repo.path,
            &[
                "update-ref",
                &format!("refs/remotes/origin/{}-remote", repo.name),
                "HEAD",
            ],
        );
        git(&repo.path, &["tag", &format!("{}-tag", repo.name)]);
    }
    p.refresh(&c).unwrap();
    (d, c, p)
}

fn assert_ref_roots(
    p: &mut LogPanel,
    t: &mut Terminal<TestBackend>,
    c: &Context,
    expected: &[&str],
) {
    render(p, t, c);
    let rows: Vec<_> = p
        .hits
        .iter()
        .filter(|(_, h)| matches!(h, Hit::Ref(_)))
        .map(|(area, _)| row_text(t, *area))
        .collect();
    for repo in &c.repos {
        for suffix in ["local", "remote", "tag"] {
            let name = format!("{}-{suffix}", repo.name);
            assert_eq!(
                rows.iter().any(|text| text.contains(&name)),
                expected.contains(&repo.name.as_str()),
                "unexpected visibility for {name}: {rows:?}"
            );
        }
    }
}

#[test]
fn paths_ref_tree_tracks_root_toggles_tabs_and_clear() {
    let (_d, mut c, mut p) = ref_workspace();
    let scope = c.scope.clone();
    let mut t = Terminal::new(TestBackend::new(180, 40)).unwrap();
    assert_ref_roots(&mut p, &mut t, &c, &["api", "web"]);
    filter_key(&mut p, 'p', &c);
    render(&mut p, &mut t, &c);
    let web = target(&p, |h| matches!(h, Hit::Dropdown(DropdownHit::Choice(3))));
    click_at(&mut p, web, &c);
    assert!(p.editing());
    assert_ref_roots(&mut p, &mut t, &c, &["api"]);
    for roots in [&["api", "web"][..], &[][..], &["api", "web"][..]] {
        key(&mut p, K::Char('a'), Mods::CONTROL, &c);
        assert_ref_roots(&mut p, &mut t, &c, roots);
    }
    p.event(&Event::Paste("web".into()), &c);
    key(&mut p, K::Enter, Mods::NONE, &c);
    assert_ref_roots(&mut p, &mut t, &c, &["web"]);
    assert_eq!(c.scope, scope);

    key(&mut p, K::Char('t'), Mods::CONTROL, &c);
    assert_ref_roots(&mut p, &mut t, &c, &["api", "web"]);
    key(&mut p, K::Char('['), Mods::NONE, &c);
    assert_ref_roots(&mut p, &mut t, &c, &["web"]);
    c.scope.remove(&1);
    p.refresh(&c).unwrap();
    assert_ref_roots(&mut p, &mut t, &c, &[]);
    c.scope = scope;
    p.refresh(&c).unwrap();
    assert_ref_roots(&mut p, &mut t, &c, &["web"]);

    let clear = target(&p, |h| matches!(h, Hit::Clear(Field::Path)));
    click_at(&mut p, clear, &c);
    assert_ref_roots(&mut p, &mut t, &c, &["api", "web"]);
}

#[test]
fn paths_ref_tree_applies_path_drafts_on_confirmation() {
    let (_d, c, mut p) = ref_workspace();
    let mut t = Terminal::new(TestBackend::new(180, 40)).unwrap();
    open(&mut p, &c, true);
    tree_click(&mut p, &mut t, &c, "api (", TreeHit::Fold);
    tree_click(&mut p, &mut t, &c, "D lib", TreeHit::Check);
    key(&mut p, K::Esc, Mods::NONE, &c);
    assert_ref_roots(&mut p, &mut t, &c, &["api", "web"]);
    open(&mut p, &c, true);
    tree_click(&mut p, &mut t, &c, "api (", TreeHit::Fold);
    tree_click(&mut p, &mut t, &c, "D lib", TreeHit::Check);
    key(&mut p, K::Enter, Mods::NONE, &c);
    assert_ref_roots(&mut p, &mut t, &c, &["api"]);

    for (paths, roots) in [
        ("web/docs", &["web"][..]),
        ("api/lib\nweb/docs", &["api", "web"][..]),
        ("lib", &["api", "web"][..]),
        ("api/missing-file", &["api"][..]),
    ] {
        open(&mut p, &c, false);
        key(&mut p, K::Char('u'), Mods::CONTROL, &c);
        p.event(&Event::Paste(paths.into()), &c);
        key(&mut p, K::Char('s'), Mods::CONTROL, &c);
        assert_ref_roots(&mut p, &mut t, &c, roots);
    }
    // Refs depend on the selected repositories, even when no commit matches the path.
    assert!(p.visible.is_empty());
    let clear = target(&p, |h| matches!(h, Hit::Clear(Field::Path)));
    click_at(&mut p, clear, &c);
    assert_ref_roots(&mut p, &mut t, &c, &["api", "web"]);
}

#[test]
fn paths_multiline_matches_file_or_directory_literally_and_keeps_invalid_drafts() {
    let (_d, c, mut p) = workspace();
    let mut t = Terminal::new(TestBackend::new(48, 12)).unwrap();
    open(&mut p, &c, false);
    // Bracketed paste keeps CRLF as one newline; ordinary Enter adds another path.
    p.event(&Event::Paste(format!("lib/{LITERAL_NAME}\r\ndocs")), &c);
    key(&mut p, K::Enter, Mods::NONE, &c);
    p.event(&Event::Paste("web".into()), &c); // A directory or file can have a repo's name.
    assert_eq!(subjects(&p).len(), 8);
    render(&mut p, &mut t, &c);
    let area = target(&p, |h| matches!(h, Hit::Dropdown(DropdownHit::PathText)));
    assert!(row_text(&t, area).contains(&format!("lib/{LITERAL_NAME}")));
    assert!(row_text(&t, Rect::new(area.x, area.y + 1, area.width, 1)).contains("docs"));
    let apply = target(&p, |h| matches!(h, Hit::Dropdown(DropdownHit::Apply)));
    click_at(&mut p, apply, &c);
    let expected = [
        "api docs",
        "api literal",
        "api namesake",
        "web docs",
        "web literal",
        "web namesake",
    ];
    assert_eq!(subjects(&p), expected);
    assert!(p.filter.roots.is_none());

    open(&mut p, &c, false);
    key(&mut p, K::Char('u'), Mods::CONTROL, &c);
    p.event(&Event::Paste("docs\n../escape".into()), &c);
    let Action::Notice(error) = key(&mut p, K::Char('s'), Mods::CONTROL, &c) else {
        panic!("an escaping path must keep the editor open")
    };
    assert!(error.contains("Line 2"));
    assert!(p.editing());
    key(&mut p, K::Esc, Mods::NONE, &c);
    assert_eq!(subjects(&p), expected);

    open(&mut p, &c, false);
    key(&mut p, K::Char('u'), Mods::CONTROL, &c);
    p.event(
        &Event::Paste(format!("api/lib/{LITERAL_NAME}\nweb/docs")),
        &c,
    );
    key(&mut p, K::Enter, Mods::CONTROL, &c);
    assert_eq!(subjects(&p), ["api literal", "web docs"]);
    key(&mut p, K::Char('t'), Mods::CONTROL, &c);
    assert_eq!(subjects(&p).len(), 8);
    key(&mut p, K::Char('['), Mods::NONE, &c);
    assert_eq!(subjects(&p), ["api literal", "web docs"]);
}

use paths::TreeHit;

fn tree_hit(
    p: &LogPanel,
    t: &Terminal<TestBackend>,
    name: &str,
    kind: fn(usize) -> TreeHit,
) -> Rect {
    let id = p
        .hits
        .iter()
        .find_map(|(r, hit)| match hit {
            Hit::Dropdown(DropdownHit::PathTree(TreeHit::Row(i)))
                if row_text(t, *r).contains(name) =>
            {
                Some(*i)
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("tree row {name:?} is missing"));
    target(
        p,
        |h| matches!(h, Hit::Dropdown(DropdownHit::PathTree(hit)) if *hit == kind(id)),
    )
}

fn tree_click(
    p: &mut LogPanel,
    t: &mut Terminal<TestBackend>,
    c: &Context,
    name: &str,
    kind: fn(usize) -> TreeHit,
) {
    render(p, t, c);
    let hit = tree_hit(p, t, name, kind);
    click_at(p, hit, c);
}

#[test]
fn paths_tree_applies_only_checked_roots_files_and_directories_after_ok() {
    let (_d, c, mut p) = workspace();
    let mut t = Terminal::new(TestBackend::new(80, 24)).unwrap();
    filter_key(&mut p, 'p', &c);
    render(&mut p, &mut t, &c);
    let tree = target(&p, |h| matches!(h, Hit::Dropdown(DropdownHit::Choice(1))));
    click_at(&mut p, tree, &c);
    render(&mut p, &mut t, &c);
    assert_eq!(
        p.hits
            .iter()
            .filter(|(_, h)| matches!(h, Hit::Dropdown(DropdownHit::PathTree(TreeHit::Row(_)))))
            .count(),
        2
    );
    assert!(p
        .hits
        .iter()
        .all(|(_, h)| !matches!(h, Hit::Dropdown(DropdownHit::Apply))));
    assert!(row_text(&t, tree_hit(&p, &t, "api (", TreeHit::Row)).contains("[ ] R api"));

    tree_click(&mut p, &mut t, &c, "api (", TreeHit::Fold);
    tree_click(&mut p, &mut t, &c, "D lib", TreeHit::Fold);
    tree_click(&mut p, &mut t, &c, LITERAL_NAME, TreeHit::Check);
    render(&mut p, &mut t, &c);
    assert!(row_text(&t, tree_hit(&p, &t, "api (", TreeHit::Row)).contains("[-]"));
    tree_click(&mut p, &mut t, &c, "api (", TreeHit::Fold);
    tree_click(&mut p, &mut t, &c, "web (", TreeHit::Fold);
    tree_click(&mut p, &mut t, &c, "D docs", TreeHit::Check);
    assert_eq!(subjects(&p).len(), 8);
    render(&mut p, &mut t, &c);
    let ok = target(&p, |h| matches!(h, Hit::Dropdown(DropdownHit::Apply)));
    click_at(&mut p, ok, &c);
    assert_eq!(subjects(&p), ["api literal", "web docs"]);
    assert_eq!(
        p.filter.paths.text(&c).unwrap(),
        format!(
            "api/{}\nweb/docs",
            Path::new("lib").join(LITERAL_NAME).display()
        )
    );
    assert_eq!(c.scope.len(), 2);

    open(&mut p, &c, true);
    key(&mut p, K::Char(' '), Mods::NONE, &c); // api: partial -> whole repo
    render(&mut p, &mut t, &c);
    let cancel = target(&p, |h| matches!(h, Hit::Dropdown(DropdownHit::Cancel)));
    click_at(&mut p, cancel, &c);
    assert_eq!(subjects(&p), ["api literal", "web docs"]);
    open(&mut p, &c, true);
    key(&mut p, K::Char(' '), Mods::NONE, &c);
    key(&mut p, K::Enter, Mods::NONE, &c);
    assert_eq!(
        subjects(&p),
        [
            "api decoy",
            "api docs",
            "api literal",
            "api namesake",
            "web docs"
        ]
    );
}

#[test]
fn paths_tree_can_uncheck_an_inherited_child_without_unchecking_its_siblings() {
    let (_d, c, mut p) = workspace();
    let mut t = Terminal::new(TestBackend::new(48, 12)).unwrap();
    open(&mut p, &c, true);
    key(&mut p, K::Char(' '), Mods::NONE, &c); // Entire api root
    key(&mut p, K::Right, Mods::NONE, &c);
    tree_click(&mut p, &mut t, &c, "D lib", TreeHit::Fold);
    tree_click(&mut p, &mut t, &c, "x.rs", TreeHit::Check);
    key(&mut p, K::Enter, Mods::NONE, &c);
    assert_eq!(subjects(&p), ["api docs", "api literal", "api namesake"]);
    open(&mut p, &c, false);
    // Both ways of entering paths edit the same applied filter.
    key(&mut p, K::F(2), Mods::NONE, &c);
    assert_eq!(subjects(&p), ["api docs", "api literal", "api namesake"]);
}

#[cfg(unix)]
#[test]
fn paths_tree_keeps_native_names_and_cannot_descend_into_links_or_other_roots() {
    use std::{
        ffi::OsStr,
        os::unix::{ffi::OsStrExt, fs::symlink},
    };
    let d = init();
    commit(d.path(), "plain", "plain change");
    // APFS rejects invalid UTF-8 at create time. A newline exercises the same
    // non-round-trippable text-editor boundary on that filesystem.
    let raw = OsStr::from_bytes(if cfg!(target_os = "linux") {
        b"raw-\xff.txt"
    } else {
        b"raw-\n.txt"
    });
    std::fs::write(d.path().join(raw), b"contents").unwrap();
    git(d.path(), &["add", "-A"]);
    git(d.path(), &["commit", "-qm", "native name"]);
    let nested = d.path().join("nested");
    std::fs::create_dir(&nested).unwrap();
    git(&nested, &["init", "-q", "-b", "main"]);
    symlink(&nested, d.path().join("shortcut")).unwrap();
    let c = Context::discover(d.path()).unwrap();
    let mut p = LogPanel {
        icons: IconMode::Ascii,
        ..Default::default()
    };
    let mut t = Terminal::new(TestBackend::new(100, 24)).unwrap();
    open(&mut p, &c, true);
    key(&mut p, K::Right, Mods::NONE, &c);
    render(&mut p, &mut t, &c);
    let mut foldable = Vec::new();
    for (r, h) in &p.hits {
        if let Hit::Dropdown(DropdownHit::PathTree(TreeHit::Row(i))) = h {
            let row = row_text(&t, *r);
            assert!(!row.contains(".git"));
            if p.hits.iter().any(|(_,h)| matches!(h, Hit::Dropdown(DropdownHit::PathTree(TreeHit::Fold(j))) if i == j)) {
                foldable.push(row);
            }
        }
    }
    assert_eq!(foldable.len(), 2); // Only the two actual top-level repositories.
    assert!(foldable.iter().all(|line| line.contains(" R ")));
    tree_click(&mut p, &mut t, &c, "raw-", TreeHit::Check);
    key(&mut p, K::Enter, Mods::NONE, &c);
    assert_eq!(subjects(&p), ["native name"]);
    filter_key(&mut p, 'p', &c);
    assert!(matches!(
        key(&mut p, K::Enter, Mods::NONE, &c),
        Action::Notice(_)
    ));
    key(&mut p, K::Esc, Mods::NONE, &c);
    assert_eq!(subjects(&p), ["native name"]);
}
