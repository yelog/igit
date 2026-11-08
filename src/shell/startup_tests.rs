use super::{startup::*, test_support::*, *};
use crate::{core::git, ui::panes::PaneId};
use ratatui::backend::TestBackend;
use std::{ffi::OsString, fs, path::Path, sync::mpsc, thread};

fn start_workspace(path: PathBuf, config: AppConfig) -> Result<WorkspaceLoad> {
    WorkspaceLoad::start(
        cli::Launch {
            workspace: path,
            tab: Commit,
            file_history: None,
        },
        config,
    )
}

fn start_command(cwd: &Path, args: &[OsString], config: AppConfig) -> Result<WorkspaceLoad> {
    let cli::Command::Open(options) = cli::parse(args.iter().cloned())? else {
        panic!("Expected startup options");
    };
    WorkspaceLoad::start(options.launch(cwd)?, config)
}

fn wait_workspace(loading: &mut WorkspaceLoad) -> Result<Shell> {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(app) = loading.poll()? {
            return Ok(app);
        }
        assert!(Instant::now() < deadline, "workspace worker timed out");
        thread::sleep(Duration::from_millis(5));
    }
}

fn wait_log(app: &mut Shell) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while app.log_load.pending() || app.log.history_pending() {
        app.poll();
        assert!(Instant::now() < deadline, "log worker timed out");
        thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn pending_discovery_can_render_resize_and_cancel_without_a_context() {
    let (disconnect, mut loading) = pending_workspace();
    for (width, height) in [(120, 36), (80, 24), (44, 12), (20, 5), (1, 1)] {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal
            .draw(|f| draw_loading(f, Path::new("/workspace/模块"), loading.stage))
            .unwrap();
        if width >= 44 {
            let text: String = terminal
                .backend()
                .buffer()
                .content
                .iter()
                .map(|c| c.symbol())
                .collect();
            assert!(text.contains("IGit"));
            assert!(text.contains(DISCOVERING));
            assert!(text.contains("Esc / q Quit"));
            assert!(!text.contains("No Git working trees"));
        }
        assert!(loading.poll().unwrap().is_none());
    }
    for event in [
        key(KeyCode::Esc),
        key(KeyCode::Char('q')),
        Event::Key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)),
    ] {
        assert!(cancelled(&event));
    }
    for event in [
        key(KeyCode::Char('c')),
        Event::Paste("q".into()),
        Event::Resize(80, 24),
        Event::Key(KeyEvent::new_with_kind(
            KeyCode::Char('q'),
            KeyModifiers::NONE,
            KeyEventKind::Release,
        )),
    ] {
        assert!(!cancelled(&event));
    }
    disconnect();
    assert!(loading
        .poll()
        .err()
        .unwrap()
        .to_string()
        .contains("stopped"));
}

#[test]
fn workspace_errors_and_empty_workspaces_finish_loading() {
    let dir = tempfile::tempdir().unwrap();
    let mut missing = start_workspace(dir.path().join("missing"), AppConfig::default()).unwrap();
    assert!(wait_workspace(&mut missing)
        .err()
        .unwrap()
        .to_string()
        .contains("Workspace:"));
    let file = dir.path().join("file");
    fs::write(&file, "not a directory").unwrap();
    let mut invalid = start_workspace(file, AppConfig::default()).unwrap();
    assert!(wait_workspace(&mut invalid)
        .err()
        .unwrap()
        .to_string()
        .contains("Not a directory"));
    let mut empty = start_workspace(dir.path().into(), AppConfig::default()).unwrap();
    let mut app = wait_workspace(&mut empty).unwrap();
    assert!(app.ctx.repos.is_empty());
    assert!(screen(&mut app, 80, 24).contains("No Git working trees found"));
}

#[test]
fn startup_loads_all_repositories_and_preserves_index_and_worktree() {
    let (dir, previous) = fixture();
    let repo = &previous.ctx.repos[0].path;
    let other = dir.path().join("second");
    fs::create_dir(&other).unwrap();
    git(&other, &["init", "-q", "-b", "main"]).unwrap();
    fs::write(other.join("untracked.txt"), "untracked\n").unwrap();
    let cached = git(repo, &["diff", "--cached"]).unwrap();
    let working = git(repo, &["diff"]).unwrap();
    let head = git(repo, &["rev-parse", "HEAD"]).unwrap();
    let mut loading = start_workspace(dir.path().into(), previous.config.clone()).unwrap();
    let mut app = wait_workspace(&mut loading).unwrap();
    assert_eq!(app.ctx.repos.len(), 2);
    assert_eq!(app.ctx.scope.len(), 2);
    assert!(app.ctx.repos.iter().all(|repo| repo.branch == "main"));
    assert_eq!(app.tab, Commit);
    assert!(matches!(app.log_load, LogLoad::Dormant));
    let text = screen(&mut app, 120, 36);
    assert!(text.contains("mixed.txt"));
    assert!(text.contains("other.txt"));
    assert!(text.contains("untracked.txt"));
    assert_eq!(git(repo, &["diff", "--cached"]).unwrap(), cached);
    assert_eq!(git(repo, &["diff"]).unwrap(), working);
    assert_eq!(git(repo, &["rev-parse", "HEAD"]).unwrap(), head);
    assert_eq!(
        fs::read_to_string(other.join("untracked.txt")).unwrap(),
        "untracked\n"
    );
}

#[test]
fn startup_detects_conflicts_before_enabling_workspace_actions() {
    let (_dir, previous) = fixture();
    merge_conflict(&previous.ctx.repos[0].path);
    let mut loading =
        start_workspace(previous.ctx.workspace.clone(), previous.config.clone()).unwrap();
    let mut app = wait_workspace(&mut loading).unwrap();
    assert!(app.tab_available(Conflicts));
    assert!(screen(&mut app, 120, 36).contains("Conflicts"));
    assert!(matches!(app.log_load, LogLoad::Dormant));
}

#[test]
fn log_loads_only_on_entry_and_keeps_workspace_navigation_available() {
    let (_dir, previous) = fixture();
    let mut app = Shell::unloaded(previous.ctx, previous.config);
    app.commit.refresh(&app.ctx).unwrap();
    app.refresh_log();
    assert!(matches!(app.log_load, LogLoad::Dormant));
    app.go(Log);
    assert!(app.log_load.pending());
    assert!(screen(&mut app, 120, 36).contains("Loading history..."));
    app.dispatch(&key(KeyCode::Char('/')));
    assert!(
        !app.log.editing(),
        "pending history must not accept filter edits"
    );
    app.go(Commit);
    assert_eq!(app.tab, Commit);
    assert!(screen(&mut app, 120, 36).contains("mixed.txt"));
    app.dispatch(&key(KeyCode::Char('q')));
    assert!(matches!(app.pop, Some(Popup::Confirm { .. })));
    wait_log(&mut app);
    assert!(app.log_load.ready());
    assert!(matches!(app.pop, Some(Popup::Confirm { .. })));
    app.dispatch(&key(KeyCode::Esc));
    app.go(Log);
    assert!(app.log_load.ready());
    let text = screen(&mut app, 120, 36);
    assert!(text.contains("base"));
    assert!(!text.contains("Loading history..."));
}

#[test]
fn refresh_discards_pending_history_from_an_old_scope() {
    let (_dir, mut app) = fixture();
    let (old, receiver) = mpsc::channel();
    app.log_load = LogLoad::Pending(receiver);
    app.ctx.scope.clear();
    app.refresh_log();
    assert!(matches!(app.log_load, LogLoad::Dormant));
    assert!(old.send((LogPanel::default(), Ok(()))).is_err());
    app.go(Log);
    wait_log(&mut app);
    assert!(!screen(&mut app, 120, 36).contains("base"));

    let (old, receiver) = mpsc::channel();
    app.log_load = LogLoad::Pending(receiver);
    app.ctx.scope.insert(0);
    app.refresh_log();
    assert!(app.log_load.pending());
    assert!(old.send((LogPanel::default(), Ok(()))).is_err());
    wait_log(&mut app);
    assert!(screen(&mut app, 120, 36).contains("base"));
}

#[test]
fn disconnected_log_worker_reports_error_and_allows_retry() {
    let (_dir, mut app) = fixture();
    let (sender, receiver) = mpsc::channel();
    app.log_load = LogLoad::Pending(receiver);
    drop(sender);
    assert!(app.poll_log_load());
    assert!(app.log_load.ready());
    assert!(app.msg.contains("Log loader stopped"));
    app.refresh_log();
    app.go(Log);
    assert!(screen(&mut app, 120, 36).contains("base"));
}

#[test]
fn direct_panel_startup_uses_normal_activation_and_keeps_git_state() {
    let (_dir, previous) = fixture();
    let repo = &previous.ctx.repos[0].path;
    git(
        repo,
        &[
            "-c",
            "user.name=Startup tester",
            "-c",
            "user.email=startup@example.invalid",
            "stash",
            "push",
            "-m",
            "startup stash",
        ],
    )
    .unwrap();
    let before = snapshot(repo);
    for (name, tab) in [
        ("commit", Commit),
        ("log", Log),
        ("push", Push),
        ("branch", Branch),
        ("stash", Stash),
    ] {
        let mut loading = start_command(repo, &[name.into()], previous.config.clone()).unwrap();
        let mut app = wait_workspace(&mut loading).unwrap();
        assert_eq!(app.tab, tab, "{name}");
        assert_eq!(app.main_tab, if tab == Push { Commit } else { tab });
        match tab {
            Log => {
                assert!(app.log_load.pending());
                assert!(screen(&mut app, 120, 36).contains("Loading history..."));
                wait_log(&mut app);
                assert!(screen(&mut app, 120, 36).contains("base"));
            }
            Branch => {
                assert!(app.branch.reads_pending());
                app.branch.finish_loading(&app.ctx);
                assert!(screen(&mut app, 120, 36).contains("main"));
            }
            Push => {
                assert!(app.push.reads_pending());
                app.push.finish_loading(&app.ctx);
                assert!(screen(&mut app, 120, 36).contains("repo"));
            }
            Stash => assert!(screen(&mut app, 120, 36).contains("startup stash")),
            Commit => assert!(matches!(app.log_load, LogLoad::Dormant)),
            Conflicts => unreachable!(),
        }
        if tab != Log {
            assert!(matches!(app.log_load, LogLoad::Dormant));
        }
        assert_eq!(snapshot(repo), before, "{name} startup changed Git state");
    }
}

#[test]
fn direct_file_history_is_configured_before_background_load_and_keeps_normal_log() {
    let (dir, previous) = fixture();
    let repo = &previous.ctx.repos[0].path;
    git(
        repo,
        &[
            "-c",
            "user.name=Startup tester",
            "-c",
            "user.email=startup@example.invalid",
            "-c",
            "core.hooksPath=/dev/null",
            "commit",
            "--no-gpg-sign",
            "--only",
            "-qm",
            "unrelated change",
            "--",
            "other.txt",
        ],
    )
    .unwrap();
    let before = snapshot(repo);
    let mut loading = start_command(
        dir.path(),
        &["log".into(), "repo/mixed.txt".into()],
        previous.config,
    )
    .unwrap();
    let mut app = wait_workspace(&mut loading).unwrap();
    assert_eq!(app.tab, Log);
    assert_eq!(app.ctx.workspace, *repo);
    assert!(app.log_load.pending());
    assert!(screen(&mut app, 120, 36).contains("Loading history..."));
    wait_log(&mut app);
    let text = screen(&mut app, 160, 36);
    assert!(text.contains("History: mixed.txt"), "{text}");
    assert!(text.contains("base"), "{text}");
    assert!(!text.contains("unrelated change"), "{text}");
    assert!(app.log.can_close_tab());
    app.log.close_tab(&app.ctx).unwrap();
    assert!(!app.log.can_close_tab());
    wait_log(&mut app);
    assert!(screen(&mut app, 160, 36).contains("unrelated change"));
    assert_eq!(snapshot(repo), before);
}

#[test]
fn deleted_file_history_and_directory_log_startup_resolve_the_right_workspace() {
    let (dir, previous) = fixture();
    let repo = &previous.ctx.repos[0].path;
    let folder = repo.join("removed folder");
    fs::create_dir(&folder).unwrap();
    fs::write(folder.join("文件.txt"), "tracked\n").unwrap();
    git(repo, &["add", "--", "removed folder/文件.txt"]).unwrap();
    git(
        repo,
        &[
            "-c",
            "user.name=Startup tester",
            "-c",
            "user.email=startup@example.invalid",
            "-c",
            "core.hooksPath=/dev/null",
            "commit",
            "--no-gpg-sign",
            "--only",
            "-qm",
            "tracked deleted path",
            "--",
            "removed folder/文件.txt",
        ],
    )
    .unwrap();
    fs::remove_dir_all(&folder).unwrap();
    let before = snapshot(repo);
    let mut loading = start_command(
        dir.path(),
        &[
            "log".into(),
            "--".into(),
            "repo/removed folder/文件.txt".into(),
        ],
        previous.config.clone(),
    )
    .unwrap();
    let mut app = wait_workspace(&mut loading).unwrap();
    assert_eq!(app.ctx.workspace, *repo);
    assert!(app.log_load.pending());
    wait_log(&mut app);
    let text = screen(&mut app, 160, 36);
    assert!(text.contains("History: 文件.txt"), "{text}");
    assert!(text.contains("tracked deleted path"), "{text}");

    let mut loading =
        start_command(dir.path(), &["log".into(), "repo".into()], previous.config).unwrap();
    let mut app = wait_workspace(&mut loading).unwrap();
    assert_eq!(app.ctx.workspace, *repo);
    assert_eq!(app.tab, Log);
    wait_log(&mut app);
    assert!(!app.log.can_close_tab());
    assert!(!screen(&mut app, 160, 36).contains("History:"));
    assert_eq!(snapshot(repo), before);
}

#[test]
fn file_history_outside_git_and_non_log_file_arguments_report_startup_errors() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("file.txt");
    fs::write(&file, "outside Git\n").unwrap();
    let mut loading = start_command(
        dir.path(),
        &["log".into(), "file.txt".into()],
        AppConfig::default(),
    )
    .unwrap();
    assert!(wait_workspace(&mut loading)
        .err()
        .unwrap()
        .to_string()
        .contains("outside the discovered Git worktrees"));
    for panel in ["commit", "branch", "push", "stash"] {
        let mut loading = start_command(
            dir.path(),
            &[panel.into(), "file.txt".into()],
            AppConfig::default(),
        )
        .unwrap();
        assert!(wait_workspace(&mut loading)
            .err()
            .unwrap()
            .to_string()
            .contains("Not a directory"));
    }
}

#[test]
fn inline_file_diff_keys_do_not_open_workspace_pull_or_fetch_reviews() {
    let (_dir, previous) = fixture();
    let mut loading = start_command(
        &previous.ctx.workspace,
        &["log".into(), "mixed.txt".into()],
        previous.config,
    )
    .unwrap();
    let mut app = wait_workspace(&mut loading).unwrap();
    wait_log(&mut app);
    screen(&mut app, 160, 36);
    app.log.focus_pane(PaneId::Info);
    for code in ['p', 'f'] {
        app.dispatch(&key(KeyCode::Char(code)));
        assert!(app.pop.is_none(), "{code} opened a workspace review");
        assert_eq!(app.tab, Log);
        assert_eq!(app.log.focused_pane(), PaneId::Info);
        app.dispatch_builtin(&key(KeyCode::Char(code)));
        assert!(
            app.pop.is_none(),
            "replayed {code} opened a workspace review"
        );
    }
    assert!(app.configured_footer().contains("n/p Changes"));
    app.dispatch(&Event::Key(KeyEvent::new(
        KeyCode::Char('c'),
        KeyModifiers::CONTROL,
    )));
    assert!(matches!(app.pop, Some(Popup::Confirm { .. })));
    app.dispatch(&key(KeyCode::Esc));
    app.dispatch(&key(KeyCode::Tab));
    assert_eq!(app.log.focused_pane(), PaneId::Graph);
    app.dispatch(&key(KeyCode::Char('p')));
    assert!(matches!(app.pop, Some(Popup::Update(_))));
    app.dispatch(&key(KeyCode::Esc));
    app.log.focus_pane(PaneId::Info);
    app.dispatch(&key(KeyCode::Char('1')));
    assert_eq!(app.tab, Commit);
}
