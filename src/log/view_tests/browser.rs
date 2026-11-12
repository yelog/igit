use super::*;
use std::fs;

#[test]
fn browser_shortcut_uses_the_highlighted_root_and_full_oid_independently_of_preview_and_checks() {
    let directory = tempfile::tempdir().unwrap();
    let source = init();
    commit(source.path(), "file", "Shared commit");
    let left = directory.path().join("left");
    let right = directory.path().join("right");
    for repo in [&left, &right] {
        git(
            directory.path(),
            &[
                "clone",
                "-q",
                source.path().to_str().unwrap(),
                repo.to_str().unwrap(),
            ],
        );
        fs::write(repo.join("file"), "staged\n").unwrap();
        git(repo, &["add", "."]);
        fs::write(repo.join("file"), "working\n").unwrap();
    }
    let left = left.canonicalize().unwrap();
    let right = right.canonicalize().unwrap();
    let context = Context::discover(directory.path()).unwrap();
    let mut panel = LogPanel::default();
    panel.refresh(&context).unwrap();
    let left_root = context
        .repos
        .iter()
        .position(|repo| repo.path == left)
        .unwrap();
    let right_root = context
        .repos
        .iter()
        .position(|repo| repo.path == right)
        .unwrap();
    let left_rev = panel
        .hist
        .revs
        .iter()
        .find(|rev| rev.root == left_root)
        .unwrap();
    panel
        .picks
        .push(operations::Target::new(&context, left_rev).unwrap());
    panel.select_row(
        panel
            .visible
            .iter()
            .position(|&index| panel.hist.revs[index].root == right_root)
            .unwrap(),
    );
    panel.details = Details::default();
    let oid = panel.rev().unwrap().oid.clone();
    let indexes = [&left, &right].map(|repo| fs::read(repo.join(".git/index")).unwrap());
    let diffs = [&left, &right].map(|repo| git(repo, &["diff", "--binary"]));
    let cached = [&left, &right].map(|repo| git(repo, &["diff", "--cached", "--binary"]));
    let Action::OpenCommit { repo, oid: actual } =
        key(&mut panel, K::Char('O'), Mods::SHIFT, &context)
    else {
        panic!("O should request a commit browser page");
    };
    assert_eq!(repo, right);
    assert_eq!(actual, oid);
    assert_eq!(panel.picks.len(), 1);
    assert_eq!(panel.picks[0].root, left_root);
    for (index, repo) in [&left, &right].into_iter().enumerate() {
        assert_eq!(fs::read(repo.join(".git/index")).unwrap(), indexes[index]);
        assert_eq!(git(repo, &["rev-parse", "HEAD"]), oid);
        assert_eq!(git(repo, &["diff", "--binary"]), diffs[index]);
        assert_eq!(git(repo, &["diff", "--cached", "--binary"]), cached[index]);
    }
}

#[test]
fn browser_shortcut_respects_focus_modals_modifiers_and_key_event_kinds() {
    let directory = init();
    commit(directory.path(), "file", "Base");
    let context = Context::discover(directory.path()).unwrap();
    let mut panel = LogPanel::default();
    panel.refresh(&context).unwrap();
    for pane in [Pane::Refs, Pane::Files, Pane::Info] {
        panel.focus = pane;
        assert!(matches!(
            key(&mut panel, K::Char('O'), Mods::NONE, &context),
            Action::None
        ));
    }
    panel.focus = Pane::Graph;
    for modifiers in [Mods::CONTROL, Mods::ALT, Mods::SUPER] {
        assert!(matches!(
            key(&mut panel, K::Char('O'), modifiers, &context),
            Action::None
        ));
    }
    for kind in [
        crossterm::event::KeyEventKind::Repeat,
        crossterm::event::KeyEventKind::Release,
    ] {
        assert!(matches!(
            panel.event(
                &Event::Key(KeyEvent::new_with_kind(K::Char('O'), Mods::NONE, kind)),
                &context
            ),
            Action::None
        ));
    }
    panel.begin(Field::Text, &context).unwrap();
    assert!(matches!(
        key(&mut panel, K::Char('O'), Mods::SHIFT, &context),
        Action::None
    ));
    assert_eq!(panel.editor.as_ref().unwrap().1, "O");
    key(&mut panel, K::Esc, Mods::NONE, &context);
    key(&mut panel, K::Char('o'), Mods::NONE, &context);
    assert!(panel.view_menu.is_some());
    assert!(matches!(
        key(&mut panel, K::Char('O'), Mods::SHIFT, &context),
        Action::None
    ));
    key(&mut panel, K::Esc, Mods::NONE, &context);
    panel.diff = Some(DiffView::default());
    assert!(matches!(
        key(&mut panel, K::Char('O'), Mods::SHIFT, &context),
        Action::None
    ));
}

#[test]
fn browser_shortcut_rejects_empty_history_and_repositories_outside_roots() {
    let directory = init();
    commit(directory.path(), "file", "Base");
    let mut context = Context::discover(directory.path()).unwrap();
    let mut panel = LogPanel::default();
    assert!(matches!(
        key(&mut panel, K::Char('O'), Mods::NONE, &context),
        Action::Notice(_, _)
    ));
    panel.refresh(&context).unwrap();
    context.scope.clear();
    assert!(!matches!(
        key(&mut panel, K::Char('O'), Mods::NONE, &context),
        Action::OpenCommit { .. }
    ));
}

#[test]
fn browser_shortcut_is_available_in_file_history_and_branch_comparison_lists() {
    let directory = init();
    commit(directory.path(), "file", "Base");
    commit(directory.path(), "file", "Next");
    let context = Context::discover(directory.path()).unwrap();
    let mut panel = LogPanel::default();
    panel.refresh(&context).unwrap();
    let oid = panel.rev().unwrap().oid.clone();
    panel.filter.comparison = Some(compare::Range {
        repository: directory.path().to_owned(),
        include: oid.clone(),
        exclude: git(directory.path(), &["rev-parse", "HEAD^"]),
        include_name: "topic".into(),
        exclude_name: "main".into(),
    });
    assert!(matches!(
        key(&mut panel, K::Char('O'), Mods::NONE, &context),
        Action::OpenCommit { oid: actual, .. } if actual == oid
    ));
    assert!(panel.keymap().sections[0]
        .bindings
        .iter()
        .any(|binding| binding.0 == "O"));
    panel.filter.comparison = None;
    panel.filter.file_history = Some(file_history::Target {
        root: 0,
        path: "file".into(),
    });
    panel.refresh(&context).unwrap();
    assert!(matches!(
        key(&mut panel, K::Char('O'), Mods::NONE, &context),
        Action::OpenCommit { oid: actual, .. } if actual == oid
    ));
    panel.enable_background_loading();
    panel.refresh(&context).unwrap();
    assert!(matches!(
        key(&mut panel, K::Char('O'), Mods::NONE, &context),
        Action::Notice(NoticeLevel::Warning, _)
    ));
}
