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

fn finish(panel: &mut CommitPanel, ctx: &Context) -> String {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        let action = panel.poll_add(ctx);
        if !panel.add_running() {
            let Action::Notice(_, results) = action else {
                panic!("Addition must report its result")
            };
            return results;
        }
        assert!(Instant::now() < deadline, "Addition did not finish");
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn add_files(panel: &mut CommitPanel, ctx: &Context, code: K) -> String {
    assert!(
        matches!(press(panel, ctx, code), Action::Notice(NoticeLevel::Info, message) if message.contains("Adding"))
    );
    assert!(panel.add_running());
    finish(panel, ctx)
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
fn add_to_git_shortcut_immediately_adds_only_the_highlighted_untracked_file() {
    let (_dir, ctx, mut panel) = fixture();
    select_file(&mut panel, 0, "src/new 中文.txt");
    let before: Vec<_> = ctx.repos.iter().map(|repo| snapshot(&repo.path)).collect();
    let checks = panel.checks.clone();
    let message = panel.saved.message.clone();
    let results = add_files(&mut panel, &ctx, K::Char('a'));
    assert!(results.contains("ADDED · app · 1 files"), "{results}");
    assert_eq!(panel.checks, checks);
    assert_eq!(panel.saved.message, message);
    let repo = &ctx.repos[0].path;
    assert_eq!(
        git(repo, &["show", ":src/new 中文.txt"]).unwrap(),
        "new src/new 中文.txt"
    );
    assert_eq!(git(repo, &["show", ":mixed"]).unwrap(), "staged");
    assert_eq!(git(repo, &["show", ":other"]).unwrap(), "other staged");
    for path in ["src/another[1].txt", "outside"] {
        assert!(backend::status(repo)
            .unwrap()
            .iter()
            .any(|file| file.path == Path::new(path) && file.xy == *b"??"));
    }
    let after = snapshot(repo);
    assert_eq!(after.head, before[0].head);
    assert_eq!(after.working, before[0].working);
    assert_eq!(snapshot(&ctx.repos[1].path), before[1]);
    let file = panel
        .files
        .iter()
        .find(|file| file.root == 0 && file.change.path == Path::new("src/new 中文.txt"))
        .unwrap();
    assert_eq!(file.group, DEF);
    assert_eq!(file.change.xy, *b"A ");
    assert_eq!(panel.checks, checks);
    assert_eq!(panel.saved.message, message);
    assert!(matches!(panel.confirm("commit-add", &ctx), Action::None));
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
        let results = add_files(&mut panel, &ctx, K::Insert);
        assert!(results.contains("ADDED · app · 2 files"), "{results}");
        fs::write(
            ctx.repos[0].path.join("src/late.txt"),
            "created after adding\n",
        )
        .unwrap();
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
    let pending = snapshot(&ctx.repos[1].path);
    let lock = ctx.repos[1].path.join(".git/index.lock");
    fs::write(&lock, "another index writer\n").unwrap();
    let results = add_files(&mut panel, &ctx, K::Char('a'));
    assert!(
        results.contains("ADDED · app") && results.contains("FAILED · api"),
        "{results}"
    );
    assert_eq!(snapshot(&ctx.repos[1].path), pending);
    let completed = snapshot(&ctx.repos[0].path);
    panel.cursor = panel
        .view
        .iter()
        .position(|row| row.kind == Kind::Group && row.label == NEW)
        .unwrap();
    assert!(panel
        .active()
        .unwrap()
        .files
        .iter()
        .all(|&i| panel.files[i].root == 1));
    fs::remove_file(lock).unwrap();
    ctx.scope = [1].into();
    panel.refresh(&ctx).unwrap();
    panel.cursor = panel
        .view
        .iter()
        .position(|row| row.kind == Kind::Group && row.label == NEW)
        .unwrap();
    let results = add_files(&mut panel, &ctx, K::Char('a'));
    assert!(
        results.contains("ADDED · api · 3 files") && !results.contains("ADDED · app"),
        "{results}"
    );
    assert_eq!(snapshot(&ctx.repos[0].path), completed);
    assert!(backend::status(&ctx.repos[1].path)
        .unwrap()
        .iter()
        .all(|f| f.xy != *b"??"));
}

#[test]
fn add_to_git_rejects_tracked_rows_and_preserves_preview_and_message_input() {
    let (_dir, ctx, mut panel) = fixture();
    let before: Vec<_> = ctx.repos.iter().map(|repo| snapshot(&repo.path)).collect();
    select_file(&mut panel, 0, "mixed");
    assert!(
        matches!(press(&mut panel, &ctx, K::Char('a')), Action::Notice(NoticeLevel::Warning, message) if message.contains("No untracked files"))
    );
    select_file(&mut panel, 0, "src/new 中文.txt");
    panel.focus = Focus::Diff;
    assert!(matches!(
        press(&mut panel, &ctx, K::Char('a')),
        Action::None
    ));
    panel.edit_message(&ctx);
    let message_before = panel.buf.clone();
    assert!(matches!(
        press(&mut panel, &ctx, K::Char('a')),
        Action::None
    ));
    assert_eq!(panel.buf, format!("{message_before}a"));
    for (repo, before) in ctx.repos.iter().zip(&before) {
        assert_eq!(&snapshot(&repo.path), before);
    }
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
    let results = add_files(&mut panel, &ctx, K::Char('a'));
    assert!(
        results.contains("ADDED · app · 3 files") && !results.contains("api ·"),
        "{results}"
    );
    assert!(panel.amend.is_empty(), "Add to Git must never toggle Amend");
    assert_eq!(snapshot(&ctx.repos[1].path), hidden);
    assert!(backend::status(&ctx.repos[0].path)
        .unwrap()
        .iter()
        .all(|f| f.xy != *b"??"));
}

#[cfg(unix)]
#[test]
fn add_to_git_slow_filter_keeps_first_frame_browsing_and_draft_editing_responsive() {
    use crate::ui::icons::IconMode;
    use ratatui::{backend::TestBackend, Terminal};
    let (_dir, mut ctx, mut panel) = fixture();
    let repo = ctx.repos[0].path.clone();
    let ready = repo.join(".git/add-ready");
    let release = repo.join(".git/add-release");
    struct Release(PathBuf);
    impl Drop for Release {
        fn drop(&mut self) {
            let _ = fs::write(&self.0, "release");
        }
    }
    let _release = Release(release.clone());
    let script = repo.join(".git/add-filter.sh");
    fs::write(
        &script,
        "printf ready > \"$1\"\ni=0\nwhile [ ! -f \"$2\" ] && [ -f \"$0\" ] && [ \"$i\" -lt 1000 ]; do sleep 0.01; i=$((i+1)); done\n[ -f \"$2\" ] || exit 1\ncat\n",
    )
    .unwrap();
    let quote = |path: &Path| format!("'{}'", path.to_string_lossy().replace('\'', "'\\''"));
    git(
        &repo,
        &[
            "config",
            "filter.wait.clean",
            &format!(
                "sh {} {} {}",
                quote(&script),
                quote(&ready),
                quote(&release)
            ),
        ],
    )
    .unwrap();
    fs::write(repo.join(".gitattributes"), "*.txt filter=wait\n").unwrap();
    select_file(&mut panel, 0, "src/new 中文.txt");
    let head = git(&repo, &["rev-parse", "HEAD"]).unwrap();
    let index = fs::read(repo.join(".git/index")).unwrap();
    panel.request_refresh();
    panel.poll_refresh(&mut ctx).unwrap();
    let started = Instant::now();
    assert!(matches!(
        press(&mut panel, &ctx, K::Char('a')),
        Action::Notice(NoticeLevel::Info, _)
    ));
    assert!(
        started.elapsed() < Duration::from_millis(250),
        "Adding must acknowledge input before starting Git"
    );
    assert!(panel.add_running());
    let deadline = Instant::now() + Duration::from_secs(10);
    while !ready.exists() {
        panel.poll_add(&ctx);
        assert!(Instant::now() < deadline, "Filter did not start");
        std::thread::sleep(Duration::from_millis(5));
    }
    for icons in [IconMode::Ascii, IconMode::Unicode, IconMode::NerdFont] {
        panel.icons = icons;
        for (width, height) in [(44, 12), (80, 24), (120, 36)] {
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            terminal
                .draw(|frame| panel.draw(frame, frame.area(), &ctx))
                .unwrap();
            let buffer = terminal.backend().buffer();
            let text = (0..height)
                .map(|y| {
                    (0..width)
                        .map(|x| buffer[(x, y)].symbol())
                        .collect::<String>()
                })
                .collect::<Vec<_>>()
                .join("\n");
            assert!(text.contains("Adding 1 file"), "{text}");
            assert!(
                !text.contains("%"),
                "Progress must not invent a percentage: {text}"
            );
            assert!(!panel
                .hits
                .iter()
                .any(|(_, hit)| matches!(hit, Hit::MessageAction(_) | Hit::StageHunks)));
        }
    }
    let cursor = panel.cursor;
    press(&mut panel, &ctx, K::Up);
    assert_ne!(panel.cursor, cursor);
    select_file(&mut panel, 0, "outside");
    assert!(matches!(
        press(&mut panel, &ctx, K::Char('a')),
        Action::Notice(NoticeLevel::Info, _)
    ));
    assert!(panel
        .submit(&ctx, false)
        .err()
        .unwrap()
        .to_string()
        .contains("Adding files"));
    assert_eq!(fs::read(repo.join(".git/index")).unwrap(), index);
    panel.edit_message(&ctx);
    panel.event(&Event::Paste(" edited while adding".into()), &ctx);
    press(&mut panel, &ctx, K::F(2));
    let message = panel.saved.message.clone();
    fs::write(release, "release").unwrap();
    let results = finish(&mut panel, &ctx);
    assert!(results.contains("1 added · 0 failed"), "{results}");
    assert_eq!(panel.saved.message, message);
    assert_eq!(git(&repo, &["rev-parse", "HEAD"]).unwrap(), head);
    assert_eq!(git(&repo, &["show", ":mixed"]).unwrap(), "staged");
    assert_eq!(git(&repo, &["show", ":other"]).unwrap(), "other staged");
    assert_eq!(fs::read_to_string(repo.join("mixed")).unwrap(), "working\n");
    assert!(backend::status(&repo)
        .unwrap()
        .iter()
        .any(|file| file.path == Path::new("outside") && file.xy == *b"??"));
    for _ in 0..5 {
        panel.poll_refresh(&mut ctx).unwrap();
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(panel.files.iter().any(|file| file.root == 0
        && file.change.path == Path::new("src/new 中文.txt")
        && file.change.xy == *b"A "));
}

#[test]
fn add_to_git_disconnected_worker_reports_unknown_outcomes_without_repeating_writes() {
    let (_dir, ctx, mut panel) = fixture();
    let before: Vec<_> = ctx.repos.iter().map(|repo| snapshot(&repo.path)).collect();
    let (sender, receiver) = mpsc::channel();
    drop(sender);
    panel.add.pending = Some(Pending {
        receiver,
        started: Instant::now(),
        total: 1,
        repositories: 1,
        phase: "Checking app".into(),
        completed: Vec::new(),
    });
    let Action::Notice(NoticeLevel::Error, message) = panel.poll_add(&ctx) else {
        panic!("A stopped worker must report an error")
    };
    assert!(!panel.add_running());
    assert!(
        message.contains("0 added · 1 failed") && message.contains("Inspect Git status"),
        "{message}"
    );
    for (repo, before) in ctx.repos.iter().zip(&before) {
        assert_eq!(&snapshot(&repo.path), before);
    }
}

#[test]
fn add_results_report_partial_failure_and_refresh_warnings_from_outcomes() {
    for (succeeded, refresh_failed, expected) in [
        (2, false, NoticeLevel::Success),
        (1, false, NoticeLevel::Warning),
        (0, false, NoticeLevel::Error),
        (2, true, NoticeLevel::Warning),
    ] {
        let (_dir, ctx, mut panel) = fixture();
        let (sender, receiver) = mpsc::channel();
        for (root, repo) in ctx.repos.iter().enumerate() {
            sender
                .send(Report::Completed(Completed {
                    root,
                    path: repo.path.clone(),
                    name: "FAILED in a repository name".into(),
                    result: if root < succeeded {
                        Ok(1)
                    } else {
                        Err("Success appears in an error message".into())
                    },
                    status: if refresh_failed {
                        Err("Cannot refresh Git status".into())
                    } else {
                        backend::status(&repo.path).map_err(|error| error.to_string())
                    },
                }))
                .unwrap();
        }
        sender.send(Report::Done).unwrap();
        panel.add.pending = Some(Pending {
            receiver,
            started: Instant::now(),
            total: 2,
            repositories: 2,
            phase: "Updating".into(),
            completed: Vec::new(),
        });
        let Action::Notice(level, _) = panel.poll_add(&ctx) else {
            panic!("Completed additions must report a result");
        };
        assert_eq!(level, expected);
        assert!(!panel.add_running());
        assert_eq!(panel.saved.message.as_deref(), Some("keep this draft"));
    }
}
