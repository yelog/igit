use super::{startup::*, test_support::*, *};
use crate::core::git;
use ratatui::backend::TestBackend;
use std::{fs, path::Path, sync::mpsc, thread};

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
    while app.log_load.pending() {
        app.poll_log_load();
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
    let mut missing =
        WorkspaceLoad::start(dir.path().join("missing"), AppConfig::default()).unwrap();
    assert!(wait_workspace(&mut missing)
        .err()
        .unwrap()
        .to_string()
        .contains("Workspace:"));
    let file = dir.path().join("file");
    fs::write(&file, "not a directory").unwrap();
    let mut invalid = WorkspaceLoad::start(file, AppConfig::default()).unwrap();
    assert!(wait_workspace(&mut invalid)
        .err()
        .unwrap()
        .to_string()
        .contains("Not a directory"));
    let mut empty = WorkspaceLoad::start(dir.path().into(), AppConfig::default()).unwrap();
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
    let mut loading = WorkspaceLoad::start(dir.path().into(), previous.config.clone()).unwrap();
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
        WorkspaceLoad::start(previous.ctx.workspace.clone(), previous.config.clone()).unwrap();
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
