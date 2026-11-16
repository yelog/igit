use super::*;
use crate::core::Repo;
use crossterm::event::KeyEvent;
use ratatui::{backend::TestBackend, Terminal};
use std::path::PathBuf;

fn fixture() -> (tempfile::TempDir, Context, CommitPanel) {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("repo");
    fs::create_dir(&repo).unwrap();
    let git = |args: &[&str]| crate::core::git(&repo, args).unwrap();
    git(&["init", "-q", "-b", "main"]);
    for path in ["mixed.txt", "other.txt", "working.txt"] {
        fs::write(repo.join(path), "base\n").unwrap();
    }
    git(&["add", "."]);
    git(&[
        "-c",
        "user.name=Fixture",
        "-c",
        "user.email=fixture@example.invalid",
        "-c",
        "core.hooksPath=/dev/null",
        "commit",
        "--no-gpg-sign",
        "-qm",
        "base",
    ]);
    fs::write(repo.join("mixed.txt"), "staged candidate\n").unwrap();
    fs::write(repo.join("other.txt"), "unselected staging\n").unwrap();
    git(&["add", "."]);
    fs::write(repo.join("mixed.txt"), "later working edits\n").unwrap();
    fs::write(repo.join("working.txt"), "working candidate\n").unwrap();
    let context = Context {
        workspace: repo.clone(),
        state_dir: dir.path().join("state"),
        scope: [0].into(),
        repos: vec![Repo {
            name: "fixture".into(),
            path: repo,
            branch: "main".into(),
            submodule: false,
            parent: None,
        }],
    };
    let mut panel = CommitPanel::default();
    panel.refresh(&context).unwrap();
    let other = panel
        .files
        .iter()
        .find(|file| file.change.path == Path::new("other.txt"))
        .unwrap();
    panel.checks.insert(other.key.clone(), false);
    panel.saved.message = Some("Keep the draft".into());
    panel.enable_background_previews();
    (dir, context, panel)
}

fn select(panel: &mut CommitPanel, path: &str) {
    panel.cursor = panel
        .view
        .iter()
        .position(|row| {
            row.file()
                .is_some_and(|index| panel.files[index].change.path == Path::new(path))
        })
        .unwrap();
}

fn draw(panel: &mut CommitPanel, context: &Context) -> String {
    let mut terminal = Terminal::new(TestBackend::new(120, 36)).unwrap();
    terminal
        .draw(|frame| panel.draw(frame, frame.area(), context))
        .unwrap();
    terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|cell| cell.symbol())
        .collect()
}

fn finish(panel: &mut CommitPanel, context: &Context) {
    if panel.preview.due.is_some() {
        panel.preview.due = Some(Instant::now());
    }
    let deadline = Instant::now() + Duration::from_secs(5);
    while panel.preview_wakeup().is_some() {
        panel.poll_preview(context).unwrap();
        assert!(Instant::now() < deadline, "preview timed out");
        std::thread::sleep(Duration::from_millis(5));
    }
    draw(panel, context);
}

fn hold(panel: &mut CommitPanel) -> std::sync::mpsc::Sender<Option<Prepared>> {
    panel.preview.due = None;
    panel.preview.task.hold()
}

fn stale() -> Option<Prepared> {
    Some(Prepared::new(
        "stale".into(),
        Loaded {
            patch: "obsolete preview".into(),
            pair: None,
        },
    ))
}

#[test]
fn navigation_burst_only_schedules_the_latest_file_without_reading_or_building_diffs() {
    let (_dir, context, mut panel) = fixture();
    let checks = panel.checks.clone();
    let index = fs::read(context.repos[0].path.join(".git/index")).unwrap();
    let mut saw_loading = false;
    for direction in ['j', 'k'] {
        for _ in 0..panel.view.len() {
            panel.event(
                &Event::Key(KeyEvent::new(K::Char(direction), Mod::NONE)),
                &context,
            );
            let screen = draw(&mut panel, &context);
            if let Some(target) = &panel.preview.target {
                assert_eq!(target.key, panel.active().unwrap().id);
                assert!(panel.preview.task.needs_start());
                assert!(panel.diff_sources.is_none() && panel.diff_comparison.is_none());
                assert!(screen.contains("Loading diff"));
                saw_loading = true;
            }
        }
    }
    assert!(saw_loading);
    assert_eq!(panel.checks, checks);
    assert_eq!(panel.saved.message.as_deref(), Some("Keep the draft"));
    assert_eq!(
        fs::read(context.repos[0].path.join(".git/index")).unwrap(),
        index
    );
}

#[test]
fn stalled_preview_keeps_navigation_live_and_discards_a_reply_after_revisiting_the_same_file() {
    let (_dir, context, mut panel) = fixture();
    select(&mut panel, "mixed.txt");
    draw(&mut panel, &context);
    let send = hold(&mut panel);
    for _ in 0..20 {
        select(&mut panel, "working.txt");
        assert!(draw(&mut panel, &context).contains("Loading diff"));
        panel.poll_preview(&context).unwrap();
        select(&mut panel, "mixed.txt");
        draw(&mut panel, &context);
        panel.poll_preview(&context).unwrap();
        assert!(panel.diff_key.is_empty());
        assert!(!panel.preview.task.needs_start());
    }
    send.send(stale()).ok().unwrap();
    panel.preview.due = Some(Instant::now() + Duration::from_secs(60));
    assert!(!panel.poll_preview(&context).unwrap());
    assert!(panel.diff_key.is_empty());
    assert!(panel.preview.task.needs_start());
    finish(&mut panel, &context);
    assert_eq!(
        panel.diff_sources.as_ref().unwrap().sources[1].text,
        "staged candidate\n"
    );
    assert!(!panel.diff_text.contains("obsolete"));
}

#[test]
fn changing_files_loads_the_exact_versions_and_preserves_mixed_staging() {
    let (_dir, context, mut panel) = fixture();
    let repo = &context.repos[0].path;
    let index = fs::read(repo.join(".git/index")).unwrap();
    let head = crate::core::git(repo, &["rev-parse", "HEAD"]).unwrap();
    let checks = panel.checks.clone();
    select(&mut panel, "mixed.txt");
    draw(&mut panel, &context);
    let send = hold(&mut panel);
    select(&mut panel, "working.txt");
    draw(&mut panel, &context);
    send.send(stale()).ok().unwrap();
    finish(&mut panel, &context);
    let sources = &panel.diff_sources.as_ref().unwrap().sources;
    assert_eq!(sources[0].text, "base\n");
    assert_eq!(sources[1].text, "working candidate\n");
    select(&mut panel, "mixed.txt");
    draw(&mut panel, &context);
    finish(&mut panel, &context);
    let sources = &panel.diff_sources.as_ref().unwrap().sources;
    assert_eq!(sources[0].text, "base\n");
    assert_eq!(sources[1].text, "staged candidate\n");
    assert_eq!(fs::read(repo.join(".git/index")).unwrap(), index);
    assert_eq!(
        crate::core::git(repo, &["rev-parse", "HEAD"]).unwrap(),
        head
    );
    assert_eq!(
        fs::read_to_string(repo.join("mixed.txt")).unwrap(),
        "later working edits\n"
    );
    assert_eq!(
        fs::read_to_string(repo.join("other.txt")).unwrap(),
        "unselected staging\n"
    );
    assert_eq!(panel.checks, checks);
}

#[test]
fn hiding_or_removing_the_target_scope_cancels_pending_previews() {
    let (_dir, mut context, mut panel) = fixture();
    select(&mut panel, "working.txt");
    draw(&mut panel, &context);
    let send = hold(&mut panel);
    panel.launch(Hit::TogglePreview, &context);
    send.send(stale()).ok().unwrap();
    assert!(!panel.poll_preview(&context).unwrap());
    assert!(panel.diff_key.is_empty());
    assert!(panel.preview_wakeup().is_none());
    panel.launch(Hit::TogglePreview, &context);
    draw(&mut panel, &context);
    let send = hold(&mut panel);
    context.scope.clear();
    panel.sync(&context);
    draw(&mut panel, &context);
    send.send(stale()).ok().unwrap();
    assert!(!panel.poll_preview(&context).unwrap());
    assert!(panel.diff_sources.is_none());
    assert!(!panel.diff_text.contains("obsolete"));
    assert!(panel.preview_wakeup().is_none());
}

#[test]
fn loading_never_displays_or_copies_the_previous_files_sources_and_keeps_new_search_input() {
    let (_dir, context, mut panel) = fixture();
    select(&mut panel, "mixed.txt");
    draw(&mut panel, &context);
    finish(&mut panel, &context);
    assert!(draw(&mut panel, &context).contains("staged candidate"));
    select(&mut panel, "working.txt");
    let screen = draw(&mut panel, &context);
    assert!(screen.contains("Loading diff") && screen.contains("working.txt"));
    assert!(!screen.contains("staged candidate"));
    panel.focus = Focus::Diff;
    panel.event(
        &Event::Key(KeyEvent::new(K::Char('/'), Mod::NONE)),
        &context,
    );
    panel.event(&Event::Paste("candidate".into()), &context);
    assert!(panel.selected_text().is_none());
    finish(&mut panel, &context);
    let prompt = panel.diff_comparison.as_ref().unwrap().search_prompt();
    assert!(prompt.editing());
    let mut terminal = Terminal::new(TestBackend::new(80, 1)).unwrap();
    terminal
        .draw(|frame| prompt.draw(frame, frame.area()))
        .unwrap();
    let query = terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|cell| cell.symbol())
        .collect::<String>();
    assert!(query.contains("candidate"), "{query}");
}

#[test]
fn same_status_refresh_replaces_pending_reads_and_preserves_an_unchanged_viewer() {
    let (_dir, mut context, mut panel) = fixture();
    select(&mut panel, "working.txt");
    draw(&mut panel, &context);
    let send = hold(&mut panel);
    fs::write(
        context.repos[0].path.join("working.txt"),
        "new working contents\n",
    )
    .unwrap();
    let scan = |panel: &mut CommitPanel, context: &mut Context| {
        panel.request_refresh();
        let deadline = Instant::now() + Duration::from_secs(5);
        while panel.refresh_pending() {
            panel.poll_refresh(context).unwrap();
            assert!(Instant::now() < deadline, "status refresh timed out");
            std::thread::sleep(Duration::from_millis(5));
        }
    };
    scan(&mut panel, &mut context);
    assert_eq!(
        panel.diff_sources.as_ref().unwrap().sources[1].text,
        "new working contents\n"
    );
    send.send(stale()).ok().unwrap();
    assert!(!panel.poll_preview(&context).unwrap());
    assert!(!panel.diff_text.contains("obsolete"));
    let comparison = &**panel.diff_comparison.as_ref().unwrap() as *const _;
    scan(&mut panel, &mut context);
    assert!(std::ptr::eq(
        comparison,
        &**panel.diff_comparison.as_ref().unwrap()
    ));
}

#[test]
fn a_disconnected_worker_reports_once_and_does_not_restart_on_each_draw() {
    let (_dir, context, mut panel) = fixture();
    select(&mut panel, "mixed.txt");
    draw(&mut panel, &context);
    drop(hold(&mut panel));
    assert!(panel.poll_preview(&context).is_err());
    assert!(panel.diff_text.contains("Could not load diff"));
    for _ in 0..3 {
        let screen = draw(&mut panel, &context);
        assert!(screen.contains("Could not load diff"));
        assert!(!screen.contains("Loading diff"));
        assert!(!panel.poll_preview(&context).unwrap());
        assert!(panel.preview_wakeup().is_none());
    }
}

#[test]
fn hiding_a_stalled_preview_stops_fast_polling_without_waiting_for_the_worker() {
    let (_dir, context, mut panel) = fixture();
    select(&mut panel, "mixed.txt");
    draw(&mut panel, &context);
    let send = hold(&mut panel);
    assert_eq!(panel.preview_wakeup(), Some(Duration::from_millis(16)));
    panel.launch(Hit::TogglePreview, &context);
    assert!(panel.preview_wakeup().is_none());
    assert!(!panel.poll_preview(&context).unwrap());
    send.send(stale()).ok().unwrap();
    panel.cancel_preview_read();
    assert!(!panel.preview.task.pending());
}

#[test]
#[ignore = "manual read-only navigation profile; requires IGIT_PERF_WORKSPACE"]
fn profile_navigation() {
    let repo = PathBuf::from(std::env::var_os("IGIT_PERF_WORKSPACE").unwrap());
    let state = tempfile::tempdir().unwrap();
    let context = Context {
        workspace: repo.clone(),
        state_dir: state.path().into(),
        scope: [0].into(),
        repos: vec![Repo {
            name: "profile".into(),
            path: repo.clone(),
            branch: String::new(),
            submodule: false,
            parent: None,
        }],
    };
    let head = crate::core::git(&repo, &["rev-parse", "HEAD"]).unwrap();
    let index_path = crate::core::git(&repo, &["rev-parse", "--git-path", "index"]).unwrap();
    let index_path = repo.join(index_path.trim());
    let index = fs::read(&index_path).unwrap();
    let changes = backend::status(&repo).unwrap();
    let working = changes
        .iter()
        .filter_map(|change| {
            fs::read(repo.join(&change.path))
                .ok()
                .map(|bytes| (change.path.clone(), bytes))
        })
        .collect::<Vec<_>>();
    for background in [false, true] {
        let mut panel = CommitPanel::default();
        panel.refresh(&context).unwrap();
        if background {
            panel.enable_background_previews();
        }
        let mut terminal = Terminal::new(TestBackend::new(180, 50)).unwrap();
        let mut samples = Vec::new();
        for direction in ['j', 'k'] {
            for _ in 0..panel.view.len().saturating_sub(1) {
                let started = Instant::now();
                panel.event(
                    &Event::Key(KeyEvent::new(K::Char(direction), Mod::NONE)),
                    &context,
                );
                terminal
                    .draw(|frame| panel.draw(frame, frame.area(), &context))
                    .unwrap();
                if background {
                    panel.poll_preview(&context).unwrap();
                }
                samples.push(started.elapsed().as_secs_f64() * 1000.0);
            }
        }
        samples.sort_by(f64::total_cmp);
        let percentile = |percent: usize| samples[(samples.len() - 1) * percent / 100];
        eprintln!(
            "navigation: background={background} files={} rows={} frames={} total_ms={:.2} p50_ms={:.2} p95_ms={:.2} max_ms={:.2}",
            panel.files.len(),
            panel.view.len(),
            samples.len(),
            samples.iter().sum::<f64>(),
            percentile(50),
            percentile(95),
            samples.last().unwrap()
        );
        if background {
            panel.cursor = panel
                .view
                .iter()
                .position(|row| row.file().is_some())
                .unwrap();
            let stopped = Instant::now();
            terminal
                .draw(|frame| panel.draw(frame, frame.area(), &context))
                .unwrap();
            let deadline = stopped + Duration::from_secs(10);
            while let Some(wakeup) = panel.preview_wakeup() {
                panel.poll_preview(&context).unwrap();
                assert!(Instant::now() < deadline, "settled preview timed out");
                std::thread::sleep(wakeup.min(Duration::from_millis(5)));
            }
            terminal
                .draw(|frame| panel.draw(frame, frame.area(), &context))
                .unwrap();
            eprintln!(
                "settled preview: ready_ms={:.2} comparison={}",
                stopped.elapsed().as_secs_f64() * 1000.0,
                panel.diff_comparison.is_some()
            );
            assert_eq!(panel.diff_key, panel.active().unwrap().id);
        }
    }
    assert_eq!(
        crate::core::git(&repo, &["rev-parse", "HEAD"]).unwrap(),
        head
    );
    assert_eq!(fs::read(index_path).unwrap(), index);
    for (path, bytes) in working {
        assert_eq!(fs::read(repo.join(path)).unwrap(), bytes);
    }
}
