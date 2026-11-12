use super::*;
use crate::core::{git, Repo};
use crossterm::event::KeyEvent;
use ratatui::{backend::TestBackend, Terminal};

fn fixture() -> (tempfile::TempDir, Context, CommitPanel) {
    let dir = tempfile::tempdir().unwrap();
    let mut repos = vec![];
    for name in ["api", "web"] {
        let repo = dir.path().join(name);
        fs::create_dir_all(repo.join("src")).unwrap();
        git(&repo, &["init", "-q", "-b", "main"]).unwrap();
        for name in ["src/mixed", "src/other", "outside"] {
            fs::write(repo.join(name), "base\n").unwrap();
        }
        git(&repo, &["add", "."]).unwrap();
        git(
            &repo,
            &[
                "-c",
                "user.name=Tester",
                "-c",
                "user.email=test@invalid.test",
                "-c",
                "core.hooksPath=/dev/null",
                "commit",
                "--no-gpg-sign",
                "-qm",
                "base",
            ],
        )
        .unwrap();
        fs::write(repo.join("src/mixed"), "staged\n").unwrap();
        fs::write(repo.join("src/other"), "other staged\n").unwrap();
        git(&repo, &["add", "."]).unwrap();
        fs::write(repo.join("src/mixed"), "working\n").unwrap();
        fs::write(repo.join("src/new"), "untracked\n").unwrap();
        fs::write(repo.join("outside"), "outside change\n").unwrap();
        repos.push(Repo {
            path: repo,
            name: name.into(),
            branch: "main".into(),
            parent: None,
            submodule: false,
        });
    }
    let c = Context {
        workspace: dir.path().into(),
        state_dir: dir.path().join("state"),
        scope: [0, 1].into(),
        repos,
    };
    let mut panel = CommitPanel::default();
    panel.refresh(&c).unwrap();
    (dir, c, panel)
}

fn press(panel: &mut CommitPanel, c: &Context, ch: char) -> Action {
    panel.event(&Event::Key(KeyEvent::new(K::Char(ch), Mod::NONE)), c)
}

fn review(panel: &mut CommitPanel, c: &Context) -> Box<Rollback> {
    match press(panel, c, 'd') {
        Action::ReviewRollback => panel.take_rollback().unwrap(),
        Action::Notice(_, message) => panic!("{message}"),
        _ => panic!("missing rollback review"),
    }
}

fn render(rollback: &mut Rollback, c: &Context) -> String {
    let mut terminal = Terminal::new(TestBackend::new(110, 36)).unwrap();
    terminal.draw(|f| rollback.draw(f, f.area(), c)).unwrap();
    terminal
        .backend()
        .buffer()
        .content
        .chunks(110)
        .map(|row| row.iter().map(|cell| cell.symbol()).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}

fn select_file(panel: &mut CommitPanel, root: usize, path: &str) {
    panel.cursor = panel
        .view
        .iter()
        .position(|r| {
            r.file().is_some_and(|i| {
                panel.files[i].root == root && panel.files[i].change.path == Path::new(path)
            })
        })
        .unwrap();
}

#[test]
fn discard_targets_unchecked_cursor_and_keeps_other_checks_and_staged_content() {
    let (_dir, c, mut panel) = fixture();
    select_file(&mut panel, 0, "src/mixed");
    let file = panel.active().unwrap().file().unwrap();
    panel.checks.insert(panel.files[file].key.clone(), false);
    let checks = panel.checks.clone();
    let mut rollback = review(&mut panel, &c);
    let body = render(&mut rollback, &c);
    assert!(
        body.contains("mixed")
            && body.contains("other")
            && body.contains("web")
            && body.contains("1 modified"),
        "{body}"
    );
    assert!(matches!(
        rollback.event(&Event::Key(KeyEvent::new(K::Char('v'), Mod::NONE)), &c),
        RollbackEvent::Keep
    ));
    let patch = render(&mut rollback, &c);
    assert!(
        patch.contains("src/mixed") && patch.contains("+staged") && patch.contains("+working"),
        "{patch}"
    );
    assert_eq!(
        fs::read(c.repos[0].path.join("src/mixed")).unwrap(),
        b"working\n"
    );
    assert_eq!(panel.checks, checks);
    assert!(matches!(
        rollback.apply(&c),
        Action::Refresh(NoticeLevel::Success, _)
    ));
    assert_eq!(
        git(&c.repos[0].path, &["show", ":src/mixed"]).unwrap(),
        "base"
    );
    assert_eq!(
        fs::read(c.repos[0].path.join("src/mixed")).unwrap(),
        b"base\n"
    );
    assert_eq!(
        git(&c.repos[0].path, &["show", ":src/other"]).unwrap(),
        "other staged"
    );
    assert_eq!(
        fs::read(c.repos[1].path.join("src/mixed")).unwrap(),
        b"working\n"
    );
    assert_eq!(panel.checks, checks);
    assert!(panel.take_rollback().is_none());
}

#[test]
fn discard_directory_crosses_changelists_and_unversioned_group_in_both_layouts() {
    for (repositories, modules) in [(true, false), (false, false), (false, true), (true, true)] {
        let (_dir, mut c, mut panel) = fixture();
        c.scope = [0].into();
        panel.options.repositories = repositories;
        panel.options.modules = modules;
        panel.options.directories = true;
        panel.saved.groups.push("Later".into());
        let file = panel
            .files
            .iter_mut()
            .find(|f| f.root == 0 && f.change.path == Path::new("src/other"))
            .unwrap();
        file.group = "Later".into();
        panel.sync(&c);
        panel.cursor = panel
            .view
            .iter()
            .position(|r| {
                matches!(&r.kind, Kind::Directory(path) if path.ends_with("src"))
                    && r.files.iter().any(|&i| panel.files[i].root == 0)
            })
            .unwrap();
        let mut rollback = review(&mut panel, &c);
        let body = render(&mut rollback, &c);
        for path in ["mixed", "other", "new"] {
            assert!(body.contains(path));
        }
        assert!(body.contains("outside") && !body.contains("web"), "{body}");
        assert!(
            body.contains("1 untracked to delete · 2 modified"),
            "{body}"
        );
        rollback.apply(&c);
        assert!(!c.repos[0].path.join("src/new").exists());
        assert_eq!(git(&c.repos[0].path, &["diff", "--cached"]).unwrap(), "");
        assert_eq!(
            fs::read(c.repos[0].path.join("outside")).unwrap(),
            b"outside change\n"
        );
        assert_eq!(
            fs::read(c.repos[1].path.join("src/mixed")).unwrap(),
            b"working\n"
        );
    }
}

#[test]
fn discard_is_inactive_in_preview_message_and_text_input_and_rechecks_scope() {
    let (_dir, mut c, mut panel) = fixture();
    select_file(&mut panel, 0, "src/mixed");
    for focus in [Focus::Diff, Focus::Message] {
        panel.focus = focus;
        assert!(matches!(press(&mut panel, &c, 'd'), Action::None));
    }
    panel.edit = Some(Edit::Message);
    assert!(matches!(press(&mut panel, &c, 'd'), Action::None));
    assert_eq!(panel.buf, "d");
    panel.edit = None;
    panel.focus = Focus::Tree;
    let rollback = review(&mut panel, &c);
    c.scope = [1].into();
    let Action::Refresh(NoticeLevel::Error, message) = rollback.apply(&c) else {
        panic!("missing scope result")
    };
    assert!(message.contains("outside the current scope"));
    assert_eq!(
        fs::read(c.repos[0].path.join("src/mixed")).unwrap(),
        b"working\n"
    );
}

#[test]
fn discard_repository_row_includes_all_its_groups_and_keeps_other_repositories() {
    let (_dir, c, mut panel) = fixture();
    panel.set_module_grouping(true, &c);
    panel.cursor = panel
        .view
        .iter()
        .position(|row| row.kind == Kind::Repo(0))
        .unwrap();
    let mut rollback = review(&mut panel, &c);
    let body = render(&mut rollback, &c);
    for path in ["mixed", "other", "new", "outside"] {
        assert!(body.contains(path));
    }
    assert!(
        body.contains("web") && body.contains("1 untracked to delete · 3 modified"),
        "{body}"
    );
    rollback.apply(&c);
    assert!(backend::status(&c.repos[0].path).unwrap().is_empty());
    assert_eq!(
        fs::read(c.repos[1].path.join("src/mixed")).unwrap(),
        b"working\n"
    );
}

#[test]
fn discard_root_groups_precheck_only_their_members_and_keep_hidden_repositories_out_of_scope() {
    let (_dir, mut c, mut panel) = fixture();
    c.scope = [0].into();
    let repo = &c.repos[0].path;
    let head = git(repo, &["rev-parse", "HEAD"]).unwrap();
    panel.saved.groups.push("Later".into());
    let other = panel
        .files
        .iter_mut()
        .find(|f| f.root == 0 && f.change.path == Path::new("src/other"))
        .unwrap();
    other.group = "Later".into();
    panel
        .saved
        .membership
        .insert(other.key.clone(), "Later".into());
    panel.fold.insert("g:Changes".into());
    panel.sync(&c);
    panel.cursor = 0;
    let mut rollback = review(&mut panel, &c);
    let body = render(&mut rollback, &c);
    assert!(
        body.contains("2 modified") && body.contains("mixed") && body.contains("outside"),
        "{body}"
    );
    assert!(
        body.contains("other") && body.contains("Unversioned Files") && !body.contains("web"),
        "{body}"
    );
    rollback.apply(&c);
    assert_eq!(fs::read(repo.join("src/mixed")).unwrap(), b"base\n");
    assert_eq!(git(repo, &["show", ":src/mixed"]).unwrap(), "base");
    assert_eq!(git(repo, &["show", ":src/other"]).unwrap(), "other staged");
    assert_eq!(fs::read(repo.join("src/new")).unwrap(), b"untracked\n");
    assert_eq!(
        fs::read(c.repos[1].path.join("src/mixed")).unwrap(),
        b"working\n"
    );

    for group in ["Later", NEW] {
        panel.refresh(&c).unwrap();
        panel.cursor = panel
            .view
            .iter()
            .position(|r| r.kind == Kind::Group && r.label == group)
            .unwrap();
        review(&mut panel, &c).apply(&c);
    }
    assert!(backend::status(repo).unwrap().is_empty());
    assert_eq!(git(repo, &["rev-parse", "HEAD"]).unwrap(), head);
}

#[test]
fn discard_full_tree_keeps_unsupported_and_ignored_rows_read_only() {
    let (_dir, mut c, mut panel) = fixture();
    c.scope = [0].into();
    let repo = &c.repos[0].path;
    fs::remove_file(repo.join("outside")).unwrap();
    fs::create_dir(repo.join("outside")).unwrap();
    fs::write(repo.join("outside/keep"), "replacement bytes").unwrap();
    fs::create_dir_all(repo.join(".git/info")).unwrap();
    fs::write(repo.join(".git/info/exclude"), "ignored.log\n").unwrap();
    fs::write(repo.join("ignored.log"), "ignored bytes").unwrap();
    panel.options.ignored = true;
    panel.refresh(&c).unwrap();

    select_file(&mut panel, 0, "outside");
    let mut rollback = review(&mut panel, &c);
    assert!(matches!(
        rollback.event(&Event::Key(KeyEvent::new(K::Enter, Mod::NONE)), &c),
        RollbackEvent::Keep
    ));
    let body = render(&mut rollback, &c);
    assert!(
        body.contains("outside") && body.contains("ignored.log") && body.contains("mixed"),
        "{body}"
    );
    let key = |ch| Event::Key(KeyEvent::new(K::Char(ch), Mod::NONE));
    rollback.event(&key(' '), &c);
    assert!(matches!(rollback.event(&key('y'), &c), RollbackEvent::Keep));
    assert!(render(&mut rollback, &c).contains("No files selected"));
    rollback.event(&key('v'), &c);
    assert!(render(&mut rollback, &c).contains("Cannot roll back:"));

    select_file(&mut panel, 0, "src/mixed");
    let Action::Refresh(NoticeLevel::Success, message) = review(&mut panel, &c).apply(&c) else {
        panic!("missing rollback result")
    };
    assert!(message.contains("rolled back 1 file"), "{message}");
    assert_eq!(
        fs::read(repo.join("outside/keep")).unwrap(),
        b"replacement bytes"
    );
    assert_eq!(
        fs::read(repo.join("ignored.log")).unwrap(),
        b"ignored bytes"
    );
    assert_eq!(fs::read(repo.join("src/mixed")).unwrap(), b"base\n");
    assert_eq!(git(repo, &["show", ":src/other"]).unwrap(), "other staged");
}
