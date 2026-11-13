use super::*;
use std::collections::BTreeSet;

fn open(context: &Context, oid: Option<&str>) -> LogPanel {
    let mut panel = LogPanel::default();
    panel.refresh(context).unwrap();
    if let Some(oid) = oid {
        let row = panel
            .visible
            .iter()
            .position(|&index| panel.hist.revs[index].oid == oid)
            .unwrap();
        panel.select_row(row);
    }
    key(&mut panel, K::Char('D'), context);
    finish(&mut panel, context);
    key(&mut panel, K::Enter, context);
    panel
}

fn select_file(panel: &mut LogPanel, context: &Context, path: &[u8]) {
    let comparison = panel.workspace_comparison.as_mut().unwrap();
    let index = comparison
        .panel
        .details
        .files
        .iter()
        .position(|file| file.raw == path)
        .unwrap();
    comparison.panel.select_file(index);
    comparison.request_preview(Duration::ZERO);
    finish(panel, context);
}

fn assert_only_file_changed(
    repo: &Path,
    mut before: BTreeMap<PathBuf, Vec<u8>>,
    path: &str,
    text: &str,
) {
    assert_eq!(fs::read_to_string(repo.join(path)).unwrap(), text);
    before.insert(PathBuf::from(path), text.as_bytes().to_vec());
    let after = snapshot(repo);
    let changed: BTreeSet<_> = before
        .keys()
        .chain(after.keys())
        .filter(|path| before.get(*path) != after.get(*path))
        .collect();
    assert!(changed.is_empty(), "Unexpected file changes: {changed:?}");
}

#[test]
fn workspace_compare_gutter_applies_one_hunk_in_all_icon_modes_and_narrow_views() {
    for icons in [IconMode::NerdFont, IconMode::Unicode, IconMode::Ascii] {
        for (width, height) in [(184, 36), (80, 25), (44, 12)] {
            let temp = init();
            let repo = temp.path();
            commit(repo, "file", "first\nkeep\nlast\n");
            fs::write(repo.join("file"), "FIRST\nkeep\nLAST\n").unwrap();
            let context = Context::discover(repo).unwrap();
            let mut panel = open(&context, None);
            panel.workspace_comparison.as_mut().unwrap().panel.icons = icons;
            key(&mut panel, K::Char('['), &context);
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            terminal
                .draw(|frame| panel.draw(frame, frame.area(), &context))
                .unwrap();
            let diff = panel
                .panes
                .nodes()
                .iter()
                .find(|(id, _)| *id == PaneId::Info)
                .unwrap()
                .1;
            let arrow = (diff.y..diff.bottom())
                .flat_map(|row| (diff.x..diff.right()).map(move |column| (column, row)))
                .find(|&(column, row)| {
                    matches!(
                        terminal.backend().buffer()[(column, row)].symbol(),
                        "»" | ">"
                    )
                })
                .expect("current hunk has a right-pointing gutter control");
            let before = snapshot(repo);
            panel.focus_pane(PaneId::Files);
            key(&mut panel, K::Char('L'), &context);
            assert_eq!(snapshot(repo), before);
            assert!(matches!(
                panel.event(
                    &Event::Mouse(MouseEvent {
                        kind: M::Down(MouseButton::Left),
                        column: arrow.0,
                        row: arrow.1,
                        modifiers: Mods::NONE,
                    }),
                    &context,
                ),
                Action::None
            ));
            assert_only_file_changed(repo, before, "file", "first\nkeep\nLAST\n");
            finish(&mut panel, &context);
            let before = snapshot(repo);
            key(&mut panel, K::Char('A'), &context);
            assert_eq!(snapshot(repo), before);
            assert_eq!(panel.focused_pane(), PaneId::Info);
        }
    }
}

#[test]
fn workspace_compare_does_not_apply_another_hunk_from_repeated_or_modified_keys() {
    let temp = init();
    let repo = temp.path();
    commit(repo, "file", "first\nkeep\nlast\n");
    fs::write(repo.join("file"), "FIRST\nkeep\nLAST\n").unwrap();
    let context = Context::discover(repo).unwrap();
    let mut panel = open(&context, None);
    key(&mut panel, K::Char('L'), &context);
    let before = snapshot(repo);
    for (kind, modifiers) in [
        (crossterm::event::KeyEventKind::Repeat, Mods::NONE),
        (crossterm::event::KeyEventKind::Release, Mods::NONE),
        (crossterm::event::KeyEventKind::Press, Mods::CONTROL),
        (crossterm::event::KeyEventKind::Press, Mods::ALT),
        (crossterm::event::KeyEventKind::Press, Mods::SUPER),
    ] {
        assert!(matches!(
            panel.event(
                &Event::Key(KeyEvent::new_with_kind(K::Char('L'), modifiers, kind)),
                &context,
            ),
            Action::None
        ));
        assert_eq!(snapshot(repo), before);
    }
}

#[test]
fn workspace_compare_restores_the_selected_older_commit_and_refreshes_completed_files() {
    let temp = init();
    let repo = temp.path();
    let revision = "first\nkeep\nlast\n";
    commit(repo, "file", revision);
    let oid = git(repo, &["rev-parse", "HEAD"]);
    commit(repo, "file", "FIRST\nkeep\nLAST\n");
    fs::write(repo.join("sibling"), "unrelated\n").unwrap();
    let context = Context::discover(repo).unwrap();
    let mut panel = open(&context, Some(&oid));
    key(&mut panel, K::Char('n'), &context);
    let before = snapshot(repo);
    key(&mut panel, K::Char('L'), &context);
    assert_only_file_changed(repo, before, "file", "FIRST\nkeep\nlast\n");
    finish(&mut panel, &context);
    key(&mut panel, K::Char('L'), &context);
    finish(&mut panel, &context);
    assert_eq!(fs::read_to_string(repo.join("file")).unwrap(), revision);
    let comparison = panel.workspace_comparison.as_ref().unwrap();
    assert!(comparison
        .panel
        .details
        .files
        .iter()
        .all(|file| file.raw != b"file"));
    assert_eq!(comparison.path().as_deref(), Some(b"sibling".as_slice()));
    assert!(matches!(
        key(&mut panel, K::Esc, &context),
        Action::Refresh(NoticeLevel::Info, _)
    ));
    assert!(!panel.comparing_workspace());
    assert_eq!(panel.rev().unwrap().oid, oid);
}

#[test]
fn workspace_compare_preserves_equivalent_context_indentation_when_applying_a_hunk() {
    let temp = init();
    let repo = temp.path();
    commit(repo, "file", "top\nold\n\ttabbed\nend\n");
    fs::write(repo.join("file"), "top\nnew\n    tabbed\nend\n").unwrap();
    let context = Context::discover(repo).unwrap();
    let mut panel = open(&context, None);
    let before = snapshot(repo);
    assert!(matches!(
        key(&mut panel, K::Char('L'), &context),
        Action::None
    ));
    assert_only_file_changed(repo, before, "file", "top\nold\n    tabbed\nend\n");
}

#[test]
fn workspace_compare_rebuilds_an_empty_comparison_after_the_last_hunk() {
    let temp = init();
    commit(temp.path(), "file", "revision\n");
    fs::write(temp.path().join("file"), "local\n").unwrap();
    let context = Context::discover(temp.path()).unwrap();
    let mut panel = open(&context, None);
    key(&mut panel, K::Char('L'), &context);
    finish(&mut panel, &context);
    let screen = render(&mut panel, &context, 120, 25);
    assert!(screen.contains("No differences"), "{screen}");
    let before = snapshot(temp.path());
    key(&mut panel, K::Char('L'), &context);
    assert_eq!(snapshot(temp.path()), before);
}

#[test]
fn workspace_compare_explains_unsupported_hunk_writes_and_keeps_the_source_preview() {
    let temp = init();
    let repo = temp.path();
    git(repo, &["config", "core.autocrlf", "false"]);
    commit(repo, "file", "first\nkeep\nlast\n");
    let working = "FIRST\r\nkeep\nLAST\n";
    fs::write(repo.join("file"), working).unwrap();
    let context = Context::discover(repo).unwrap();
    let mut panel = open(&context, None);
    let comparison = panel.workspace_comparison.as_mut().unwrap();
    let diff = comparison.panel.diff.as_mut().unwrap();
    assert!(diff.local.is_none());
    assert_eq!(source(diff, 1), working);
    for (width, height) in [(120, 25), (80, 24), (44, 12)] {
        let screen = render(&mut panel, &context, width, height);
        assert!(screen.contains("Read-only:"), "{screen}");
        assert!(!screen.contains(" Apply "), "{screen}");
    }
    let before = snapshot(repo);
    key(&mut panel, K::Char('L'), &context);
    key(&mut panel, K::Char('A'), &context);
    assert_eq!(snapshot(repo), before);
}

#[test]
fn workspace_compare_applies_literal_paths_and_preserves_local_crlf_and_permissions() {
    let temp = init();
    let repo = temp.path();
    git(repo, &["config", "core.autocrlf", "false"]);
    let name = if cfg!(windows) {
        "源 [x].txt"
    } else {
        "源 [x]*.txt"
    };
    commit(repo, name, "一\n二\n三\n");
    commit(repo, "源 x.txt", "sibling\n");
    let path = repo.join(name);
    fs::write(&path, "一\r\n本地\r\n三\r\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    }
    let context = Context::discover(repo).unwrap();
    let mut panel = open(&context, None);
    select_file(&mut panel, &context, name.as_bytes());
    let before = snapshot(repo);
    key(&mut panel, K::Char('L'), &context);
    assert_only_file_changed(repo, before, name, "一\r\n二\r\n三\r\n");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o755
        );
    }
}

#[test]
fn workspace_compare_restores_missing_files_and_removes_a_local_addition_as_text() {
    for missing in [true, false] {
        let temp = init();
        let repo = temp.path();
        commit(repo, "file", "historical without newline");
        let oid = git(repo, &["rev-parse", "HEAD"]);
        let (path, expected) = if missing {
            fs::remove_file(repo.join("file")).unwrap();
            ("file", "historical without newline")
        } else {
            fs::write(repo.join("added"), "local addition\n").unwrap();
            ("added", "")
        };
        let context = Context::discover(repo).unwrap();
        let mut panel = open(&context, Some(&oid));
        select_file(&mut panel, &context, path.as_bytes());
        let before = snapshot(repo);
        key(&mut panel, K::Char('L'), &context);
        assert_only_file_changed(repo, before, path, expected);
        assert!(repo.join(path).is_file());
    }
}

#[test]
fn workspace_compare_refuses_external_bytes_permissions_and_conversion_changes() {
    for change in ["bytes", "permissions", "config", "attributes"] {
        let temp = init();
        let repo = temp.path();
        git(repo, &["config", "core.autocrlf", "false"]);
        commit(repo, "file", "revision\n");
        fs::write(repo.join("file"), "local\n").unwrap();
        let context = Context::discover(repo).unwrap();
        let mut panel = open(&context, None);
        match change {
            "bytes" => fs::write(repo.join("file"), "external\n").unwrap(),
            "permissions" => {
                let path = repo.join("file");
                let mut permissions = fs::metadata(&path).unwrap().permissions();
                permissions.set_readonly(true);
                fs::set_permissions(path, permissions).unwrap();
            }
            "config" => {
                git(repo, &["config", "core.autocrlf", "input"]);
            }
            "attributes" => {
                fs::write(repo.join(".gitattributes"), "file filter=custom\n").unwrap();
            }
            _ => unreachable!(),
        }
        let before = snapshot(repo);
        let action = key(&mut panel, K::Char('L'), &context);
        assert!(
            matches!(action, Action::Notice(NoticeLevel::Error, _)),
            "{change} must invalidate the working snapshot"
        );
        assert_eq!(snapshot(repo), before);
    }
}

#[test]
fn workspace_compare_refuses_stale_scope_and_preview_file_identity() {
    let temp = init();
    let repo = temp.path();
    commit(repo, "first", "first revision\n");
    commit(repo, "second", "second revision\n");
    fs::write(repo.join("first"), "first local\n").unwrap();
    fs::write(repo.join("second"), "second local\n").unwrap();
    let mut context = Context::discover(repo).unwrap();
    let mut panel = open(&context, None);
    let comparison = panel.workspace_comparison.as_mut().unwrap();
    let change = comparison
        .panel
        .diff
        .as_ref()
        .unwrap()
        .comparison
        .as_ref()
        .unwrap()
        .selected_change()
        .unwrap();
    comparison.panel.select_file(1);
    let before = snapshot(repo);
    assert!(comparison.apply_change(&context, change).is_err());
    comparison.panel.select_file(0);
    context.scope.clear();
    assert!(comparison.apply_change(&context, change).is_err());
    assert_eq!(snapshot(repo), before);
}

#[test]
fn workspace_compare_keeps_search_and_revision_focus_after_applying() {
    let temp = init();
    let repo = temp.path();
    commit(repo, "file", "first\nkeep\nlast\n");
    fs::write(repo.join("file"), "FIRST\nkeep\nLAST\n").unwrap();
    let context = Context::discover(repo).unwrap();
    let mut panel = open(&context, None);
    key(&mut panel, K::Char('['), &context);
    key(&mut panel, K::Char('/'), &context);
    for character in "keep".chars() {
        key(&mut panel, K::Char(character), &context);
    }
    key(&mut panel, K::Enter, &context);
    key(&mut panel, K::Char('L'), &context);
    let view = panel
        .workspace_comparison
        .as_ref()
        .unwrap()
        .panel
        .diff
        .as_ref()
        .unwrap()
        .comparison
        .as_ref()
        .unwrap();
    assert_eq!(view.side(), 0);
    assert!(view.search_prompt().visible());
    assert!(panel.focused_search().is_some());
    let mut terminal = Terminal::new(TestBackend::new(40, 1)).unwrap();
    terminal
        .draw(|frame| view.search_prompt().draw(frame, frame.area()))
        .unwrap();
    let prompt: String = (0..40)
        .map(|column| terminal.backend().buffer()[(column, 0)].symbol())
        .collect();
    assert!(prompt.contains("/keep"), "{prompt}");
}

#[test]
#[cfg(unix)]
fn workspace_compare_refuses_symlink_and_nested_repository_replacements_before_writing() {
    use std::os::unix::fs::symlink;

    for replacement in ["file", "parent", "repository"] {
        let temp = init();
        let repo = temp.path();
        fs::create_dir(repo.join("directory")).unwrap();
        commit(repo, "directory/file", "revision\n");
        fs::write(repo.join("directory/file"), "local\n").unwrap();
        let context = Context::discover(repo).unwrap();
        let mut panel = open(&context, None);
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("file"), "outside\n").unwrap();
        match replacement {
            "file" => {
                fs::remove_file(repo.join("directory/file")).unwrap();
                symlink(outside.path().join("file"), repo.join("directory/file")).unwrap();
            }
            "parent" => {
                fs::remove_file(repo.join("directory/file")).unwrap();
                fs::remove_dir(repo.join("directory")).unwrap();
                symlink(outside.path(), repo.join("directory")).unwrap();
            }
            "repository" => {
                fs::write(repo.join("directory/.git"), "gitdir: elsewhere\n").unwrap();
            }
            _ => unreachable!(),
        }
        let before = snapshot(repo);
        assert!(matches!(
            key(&mut panel, K::Char('L'), &context),
            Action::Notice(NoticeLevel::Error, _)
        ));
        assert_eq!(snapshot(repo), before);
        assert_eq!(
            fs::read_to_string(outside.path().join("file")).unwrap(),
            "outside\n"
        );
    }
}

#[test]
#[cfg(unix)]
fn workspace_compare_rechecks_repository_identity_before_writing() {
    use std::os::unix::fs::symlink;

    for replacement in ["symlink", "directory"] {
        let temp = init();
        commit(temp.path(), "file", "revision\n");
        fs::write(temp.path().join("file"), "local\n").unwrap();
        let context = Context::discover(temp.path()).unwrap();
        let mut panel = open(&context, None);
        let root = context.repos[0].path.clone();
        let outside = tempfile::tempdir().unwrap();
        let moved = outside.path().join("original");
        let copy = outside.path().join("replacement");
        fs::rename(&root, &moved).unwrap();
        git(
            outside.path(),
            &[
                "clone",
                "-q",
                moved.to_str().unwrap(),
                copy.to_str().unwrap(),
            ],
        );
        fs::write(copy.join("file"), "local\n").unwrap();
        let destination = if replacement == "symlink" {
            symlink(&copy, &root).unwrap();
            copy
        } else {
            fs::rename(&copy, &root).unwrap();
            root
        };
        let before = snapshot(&destination);
        assert!(matches!(
            key(&mut panel, K::Char('L'), &context),
            Action::Notice(NoticeLevel::Error, _)
        ));
        assert_eq!(snapshot(&destination), before);
        assert_eq!(fs::read_to_string(moved.join("file")).unwrap(), "local\n");
    }
}

#[test]
fn workspace_compare_applies_only_to_the_captured_repository_with_identical_commit_ids() {
    let source = init();
    commit(source.path(), "file", "shared\n");
    let temp = tempfile::tempdir().unwrap();
    for name in ["left", "right"] {
        let path = temp.path().join(name);
        git(
            temp.path(),
            &[
                "clone",
                "-q",
                source.path().to_str().unwrap(),
                path.to_str().unwrap(),
            ],
        );
        fs::write(path.join("file"), format!("{name} local\n")).unwrap();
    }
    let context = Context::discover(temp.path()).unwrap();
    let mut panel = LogPanel::default();
    panel.refresh(&context).unwrap();
    let root = context
        .repos
        .iter()
        .position(|repo| repo.name == "right")
        .unwrap();
    let row = panel
        .visible
        .iter()
        .position(|&index| panel.hist.revs[index].root == root)
        .unwrap();
    panel.select_row(row);
    key(&mut panel, K::Char('D'), &context);
    finish(&mut panel, &context);
    key(&mut panel, K::Enter, &context);
    let before = snapshot(temp.path());
    key(&mut panel, K::Char('L'), &context);
    assert_only_file_changed(temp.path(), before, "right/file", "shared\n");
}
