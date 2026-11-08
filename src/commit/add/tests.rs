use super::*;
use crate::core::{git, Repo};
use crossterm::event::KeyEvent;

fn fixture() -> (tempfile::TempDir, Context, CommitPanel) {
    let dir = tempfile::tempdir().unwrap();
    let mut repos = vec![];
    for name in ["app", "api"] {
        let repo = dir.path().join(name);
        fs::create_dir_all(repo.join("src")).unwrap();
        git(&repo, &["init", "-q", "-b", "main"]).unwrap();
        for (key, value) in [
            ("user.name", "Add tester"),
            ("user.email", "test@example.invalid"),
            ("commit.gpgsign", "false"),
            ("core.hooksPath", "/dev/null"),
            ("core.autocrlf", "false"),
        ] {
            git(&repo, &["config", key, value]).unwrap();
        }
        for file in ["mixed", "other"] {
            fs::write(repo.join(file), "base\n").unwrap();
        }
        fs::write(repo.join(".gitignore"), "ignored\n").unwrap();
        git(&repo, &["add", "."]).unwrap();
        git(&repo, &["commit", "-qm", "base"]).unwrap();
        fs::write(repo.join("mixed"), "staged\n").unwrap();
        fs::write(repo.join("other"), "other staged\n").unwrap();
        git(&repo, &["add", "mixed", "other"]).unwrap();
        fs::write(repo.join("mixed"), "working\n").unwrap();
        for file in [
            "src/new 中文.txt",
            "src/another[1].txt",
            "outside",
            "ignored",
        ] {
            fs::write(repo.join(file), format!("new {file}\n")).unwrap();
        }
        repos.push(Repo {
            path: repo,
            name: name.into(),
            branch: "main".into(),
            parent: None,
            submodule: false,
        });
    }
    let ctx = Context {
        workspace: dir.path().into(),
        state_dir: dir.path().join("state"),
        repos,
        scope: [0, 1].into(),
    };
    let mut panel = CommitPanel::default();
    panel.refresh(&ctx).unwrap();
    panel.saved.message = Some("keep this draft".into());
    (dir, ctx, panel)
}

fn press(panel: &mut CommitPanel, ctx: &Context, code: K) -> Action {
    panel.event(&Event::Key(KeyEvent::new(code, Mod::NONE)), ctx)
}

fn select_file(panel: &mut CommitPanel, root: usize, path: &str) {
    panel.cursor = panel
        .view
        .iter()
        .position(|row| {
            row.file().is_some_and(|i| {
                panel.files[i].root == root && panel.files[i].change.path == Path::new(path)
            })
        })
        .unwrap();
}

#[derive(Debug, PartialEq, Eq)]
struct Snapshot {
    head: String,
    index: Vec<u8>,
    working: BTreeMap<std::path::PathBuf, Vec<u8>>,
}

fn snapshot(repo: &Path) -> Snapshot {
    let paths = backend::status(repo).unwrap();
    Snapshot {
        head: git(repo, &["rev-parse", "HEAD"]).unwrap(),
        index: fs::read(repo.join(".git/index")).unwrap(),
        working: paths
            .into_iter()
            .map(|file| {
                let bytes = fs::read(repo.join(&file.path)).unwrap();
                (file.path, bytes)
            })
            .collect(),
    }
}

#[test]
fn add_to_git_shortcut_reviews_only_the_highlighted_untracked_file() {
    let (_dir, ctx, mut panel) = fixture();
    select_file(&mut panel, 0, "src/new 中文.txt");
    let before: Vec<_> = ctx.repos.iter().map(|repo| snapshot(&repo.path)).collect();
    let checks = panel.checks.clone();
    let message = panel.saved.message.clone();
    let Action::Confirm { title, body, token } = press(&mut panel, &ctx, K::Char('a')) else {
        panic!("a must open Add to Git review");
    };
    assert!(title.contains("Add to Git"));
    assert!(body.contains("src/new 中文.txt"));
    assert!(!body.contains("another[1].txt") && !body.contains("other staged"));
    for (repo, before) in ctx.repos.iter().zip(&before) {
        assert_eq!(&snapshot(&repo.path), before);
    }
    assert_eq!(panel.checks, checks);
    assert_eq!(panel.saved.message, message);
    assert!(matches!(panel.confirm(&token, &ctx), Action::Refresh(_)));
    let repo = &ctx.repos[0].path;
    assert_eq!(
        git(repo, &["show", ":src/new 中文.txt"]).unwrap(),
        "new src/new 中文.txt"
    );
    assert_eq!(git(repo, &["show", ":mixed"]).unwrap(), "staged");
    assert_eq!(git(repo, &["show", ":other"]).unwrap(), "other staged");
    let after = snapshot(repo);
    assert_eq!(after.head, before[0].head);
    assert_eq!(after.working, before[0].working);
    assert_eq!(snapshot(&ctx.repos[1].path), before[1]);
    panel.refresh(&ctx).unwrap();
    let file = panel
        .files
        .iter()
        .find(|file| file.root == 0 && file.change.path == Path::new("src/new 中文.txt"))
        .unwrap();
    assert_eq!(file.group, DEF);
    assert_eq!(file.change.xy, *b"A ");
    assert_eq!(panel.checks, checks);
    assert_eq!(panel.saved.message, message);
    assert!(matches!(panel.confirm(&token, &ctx), Action::None));
}

#[test]
fn add_to_git_folded_directory_keeps_exact_members_in_each_layout() {
    for modules in [false, true] {
        let (_dir, ctx, mut panel) = fixture();
        panel.options.modules = modules;
        panel.sync(&ctx);
        panel.cursor = panel
            .view
            .iter()
            .position(|row| {
                matches!(&row.kind, Kind::Directory(path) if path.ends_with("src"))
                    && row
                        .files
                        .iter()
                        .all(|&i| panel.files[i].root == 0 && panel.files[i].group == NEW)
            })
            .unwrap();
        press(&mut panel, &ctx, K::Char('h'));
        assert!(panel.fold.contains(&panel.active().unwrap().id));
        let before = snapshot(&ctx.repos[1].path);
        let Action::Confirm { body, token, .. } = press(&mut panel, &ctx, K::Insert) else {
            panic!("Insert must review collapsed members");
        };
        assert!(body.contains("new 中文.txt") && body.contains("another[1].txt"));
        assert!(!body.contains("outside") && !body.contains("ignored"));
        fs::write(
            ctx.repos[0].path.join("src/late.txt"),
            "created after review\n",
        )
        .unwrap();
        panel.confirm(&token, &ctx);
        let changes = backend::status(&ctx.repos[0].path).unwrap();
        for path in ["src/new 中文.txt", "src/another[1].txt"] {
            assert_eq!(
                changes
                    .iter()
                    .find(|f| f.path == Path::new(path))
                    .unwrap()
                    .xy,
                *b"A "
            );
        }
        assert_eq!(
            changes
                .iter()
                .find(|f| f.path == Path::new("outside"))
                .unwrap()
                .xy,
            *b"??"
        );
        assert_eq!(snapshot(&ctx.repos[1].path), before);
        assert!(changes
            .iter()
            .any(|file| file.path == Path::new("src/late.txt") && file.xy == *b"??"));
    }
}

#[test]
fn add_to_git_group_uses_roots_and_reports_partial_failure_without_repeating_success() {
    let (_dir, mut ctx, mut panel) = fixture();
    panel.cursor = panel
        .view
        .iter()
        .position(|row| row.kind == Kind::Group && row.label == NEW)
        .unwrap();
    let Action::Confirm { token, .. } = press(&mut panel, &ctx, K::Char('a')) else {
        panic!("group review");
    };
    fs::write(ctx.repos[1].path.join("outside"), "changed after review\n").unwrap();
    let Action::Refresh(results) = panel.confirm(&token, &ctx) else {
        panic!("results");
    };
    assert!(
        results.contains("ADDED · app") && results.contains("FAILED · api"),
        "{results}"
    );
    let completed = snapshot(&ctx.repos[0].path);
    panel.refresh(&ctx).unwrap();
    panel.cursor = panel
        .view
        .iter()
        .position(|row| row.kind == Kind::Group && row.label == NEW)
        .unwrap();
    let Action::Confirm { body, token, .. } = press(&mut panel, &ctx, K::Char('a')) else {
        panic!("retry review");
    };
    assert_eq!(panel.add_plans.len(), 1);
    assert!(body.contains("api · 3 files"));
    // A Roots change after review must leave the reviewed repository untouched.
    let pending = snapshot(&ctx.repos[1].path);
    ctx.scope = [0].into();
    let Action::Refresh(results) = panel.confirm(&token, &ctx) else {
        panic!("scope result");
    };
    assert!(results.contains("outside Roots"));
    assert_eq!(snapshot(&ctx.repos[1].path), pending);
    ctx.scope = [1].into();
    panel.refresh(&ctx).unwrap();
    panel.cursor = panel
        .view
        .iter()
        .position(|row| row.kind == Kind::Group && row.label == NEW)
        .unwrap();
    press(&mut panel, &ctx, K::Char('a'));
    panel.confirm("commit-add", &ctx);
    assert_eq!(snapshot(&ctx.repos[0].path), completed);
    assert!(backend::status(&ctx.repos[1].path)
        .unwrap()
        .iter()
        .all(|f| f.xy != *b"??"));
}

#[test]
fn add_to_git_rejects_tracked_rows_and_preserves_preview_and_message_input() {
    let (_dir, ctx, mut panel) = fixture();
    select_file(&mut panel, 0, "mixed");
    assert!(
        matches!(press(&mut panel, &ctx, K::Char('a')), Action::Notice(message) if message.contains("No untracked files"))
    );
    select_file(&mut panel, 0, "src/new 中文.txt");
    panel.focus = Focus::Diff;
    assert!(matches!(
        press(&mut panel, &ctx, K::Char('a')),
        Action::None
    ));
    panel.edit_message(&ctx);
    let before = panel.buf.clone();
    assert!(matches!(
        press(&mut panel, &ctx, K::Char('a')),
        Action::None
    ));
    assert_eq!(panel.buf, format!("{before}a"));
    assert!(panel.add_plans.is_empty());
}

#[test]
fn add_to_git_repository_scope_excludes_hidden_roots_and_amend_uses_m() {
    let (_dir, mut ctx, mut panel) = fixture();
    let hidden = snapshot(&ctx.repos[1].path);
    ctx.scope = [0].into();
    panel.options.modules = true;
    panel.refresh(&ctx).unwrap();
    panel.cursor = panel
        .view
        .iter()
        .position(|row| {
            row.kind == Kind::Repo(0) && row.files.iter().all(|&i| panel.files[i].group == NEW)
        })
        .unwrap();
    press(&mut panel, &ctx, K::Char('M'));
    assert_eq!(panel.amend, [0].into());
    press(&mut panel, &ctx, K::Char('M'));
    assert!(panel.amend.is_empty());
    assert!(matches!(
        press(&mut panel, &ctx, K::Char('I')),
        Action::None
    ));
    let Action::Confirm { body, token, .. } = press(&mut panel, &ctx, K::Char('a')) else {
        panic!("a must review the repository's untracked members");
    };
    assert!(body.contains("app · 3 files") && !body.contains("api ·"));
    assert!(panel.amend.is_empty(), "Add to Git must never toggle Amend");
    panel.confirm(&token, &ctx);
    assert_eq!(snapshot(&ctx.repos[1].path), hidden);
    assert!(backend::status(&ctx.repos[0].path)
        .unwrap()
        .iter()
        .all(|f| f.xy != *b"??"));
}
