use super::*;
use crate::core::git;
use ratatui::{backend::TestBackend, Terminal};

fn fixture() -> (tempfile::TempDir, Context, CommitPanel) {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("repo");
    fs::create_dir(&repo).unwrap();
    git(&repo, &["init", "-q", "-b", "main"]).unwrap();
    for path in ["mixed.txt", "other.txt", "working.txt", "deleted.txt"] {
        fs::write(repo.join(path), "base\n").unwrap();
    }
    git(&repo, &["add", "."]).unwrap();
    git(
        &repo,
        &[
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
        ],
    )
    .unwrap();
    fs::write(repo.join("mixed.txt"), "selected staged\n").unwrap();
    fs::write(repo.join("other.txt"), "unselected staged\n").unwrap();
    git(&repo, &["add", "."]).unwrap();
    fs::write(repo.join("mixed.txt"), "selected working\n").unwrap();
    fs::write(repo.join("working.txt"), "working v1\n").unwrap();
    let mut c = Context::discover(&repo).unwrap();
    c.state_dir = dir.path().join("state");
    let mut p = CommitPanel::default();
    p.refresh(&c).unwrap();
    let other = p
        .files
        .iter()
        .find(|f| f.change.path == Path::new("other.txt"))
        .unwrap();
    p.checks.insert(other.key.clone(), false);
    (dir, c, p)
}

fn draw(p: &mut CommitPanel, c: &Context) {
    let mut t = Terminal::new(TestBackend::new(120, 36)).unwrap();
    t.draw(|f| p.draw(f, f.area(), c)).unwrap();
}

fn after(p: &CommitPanel) -> &str {
    &p.diff_sources.as_ref().unwrap().sources[1].text
}

fn select(p: &mut CommitPanel, path: &str) {
    p.cursor = p
        .view
        .iter()
        .position(|r| {
            r.file()
                .is_some_and(|i| p.files[i].change.path == Path::new(path))
        })
        .unwrap();
}

fn scan(p: &mut CommitPanel, c: &mut Context) -> bool {
    p.request_refresh();
    let mut changed = p.poll_refresh(c).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while p.refresh.pending.is_some() {
        changed |= p.poll_refresh(c).unwrap();
        assert!(Instant::now() < deadline, "background status timed out");
        thread::sleep(Duration::from_millis(5));
    }
    changed
}

#[test]
fn background_changes_refresh_status_and_same_status_diff_without_touching_index() {
    let (_dir, mut c, mut p) = fixture();
    let repo = c.repos[0].path.clone();
    let index = git(&repo, &["ls-files", "--stage"]).unwrap();
    let head = git(&repo, &["rev-parse", "HEAD"]).unwrap();
    select(&mut p, "working.txt");
    draw(&mut p, &c);
    assert_eq!(after(&p), "working v1\n");
    let target = p.active().unwrap().id.clone();
    let checks = p.checks.clone();
    p.fold.insert("unrelated-fold".into());
    fs::write(repo.join("working.txt"), "working v2\n").unwrap();
    fs::write(repo.join("new.txt"), "new\n").unwrap();
    fs::remove_file(repo.join("deleted.txt")).unwrap();
    assert!(scan(&mut p, &mut c));
    assert_eq!(p.active().unwrap().id, target);
    assert!(p.fold.contains("unrelated-fold"));
    assert!(checks
        .iter()
        .all(|(key, value)| p.checks.get(key) == Some(value)));
    let new = p
        .files
        .iter()
        .find(|f| f.change.path == Path::new("new.txt"))
        .unwrap();
    assert_eq!(p.checks.get(&new.key), Some(&false));
    assert!(p
        .files
        .iter()
        .any(|f| f.change.path == Path::new("deleted.txt") && f.change.xy == *b" D"));
    assert_eq!(after(&p), "working v2\n");
    assert_eq!(git(&repo, &["ls-files", "--stage"]).unwrap(), index);
    assert_eq!(git(&repo, &["rev-parse", "HEAD"]).unwrap(), head);
    // A second content change has exactly the same paths and XY status.
    fs::write(repo.join("working.txt"), "working v3\n").unwrap();
    assert!(scan(&mut p, &mut c));
    assert_eq!(after(&p), "working v3\n");
    git(&repo, &["add", "working.txt"]).unwrap();
    fs::write(repo.join("working.txt"), "working v4\n").unwrap();
    let staged = git(&repo, &["ls-files", "--stage"]).unwrap();
    assert!(scan(&mut p, &mut c));
    assert_eq!(after(&p), "working v3\n");
    fs::remove_file(repo.join("new.txt")).unwrap();
    assert!(scan(&mut p, &mut c));
    assert!(!p
        .files
        .iter()
        .any(|f| f.change.path == Path::new("new.txt")));
    assert_eq!(git(&repo, &["ls-files", "--stage"]).unwrap(), staged);
    assert_eq!(
        fs::read_to_string(repo.join("mixed.txt")).unwrap(),
        "selected working\n"
    );
    assert!(p
        .selection(&c)
        .iter()
        .all(|f| f.change.path != Path::new("other.txt")));
}

#[test]
fn unchanged_scan_keeps_preview_selection_scroll_and_checkbox_undo() {
    let (_dir, mut c, mut p) = fixture();
    select(&mut p, "mixed.txt");
    draw(&mut p, &c);
    p.focus = Focus::Diff;
    p.event(
        &Event::Key(crossterm::event::KeyEvent::new(K::Char('a'), Mod::CONTROL)),
        &c,
    );
    let text = p.selected_text().unwrap().to_owned();
    let comparison = &**p.diff_comparison.as_ref().unwrap() as *const _;
    p.last_check = vec![(p.files[0].key.clone(), false)];
    let undo = p.last_check.clone();
    assert!(!scan(&mut p, &mut c));
    assert_eq!(p.selected_text(), Some(text.as_str()));
    assert!(std::ptr::eq(
        comparison,
        &**p.diff_comparison.as_ref().unwrap()
    ));
    assert_eq!(p.last_check, undo);
}

#[test]
fn hidden_preview_reloads_same_status_content_after_a_scan() {
    let (_dir, mut c, mut p) = fixture();
    let path = c.repos[0].path.join("working.txt");
    select(&mut p, "working.txt");
    draw(&mut p, &c);
    assert_eq!(after(&p), "working v1\n");
    p.launch(Hit::TogglePreview, &c);
    assert!(!p.preview_open);
    fs::write(&path, "working v2\n").unwrap();
    scan(&mut p, &mut c);
    assert!(!p.preview_open);
    p.launch(Hit::TogglePreview, &c);
    draw(&mut p, &c);
    assert_eq!(after(&p), "working v2\n");
}

#[test]
fn preview_hidden_during_a_scan_does_not_keep_the_in_flight_snapshot() {
    let (_dir, mut c, mut p) = fixture();
    let path = c.repos[0].path.join("working.txt");
    select(&mut p, "working.txt");
    draw(&mut p, &c);
    assert_eq!(after(&p), "working v1\n");
    let key = p.active().unwrap().id.clone();
    let stale = Snapshot::read(
        c.clone(),
        false,
        Some((key, 0, "working.txt".into())),
        [0].into(),
    );
    let (send, receive) = mpsc::channel();
    p.refresh.pending = Some(Pending {
        generation: p.refresh.generation,
        roots: [0].into(),
        receiver: receive,
    });
    p.launch(Hit::TogglePreview, &c);
    assert!(!p.preview_open);
    fs::write(&path, "working v2\n").unwrap();
    send.send(stale).ok().unwrap();
    p.poll_refresh(&mut c).unwrap();
    p.launch(Hit::TogglePreview, &c);
    draw(&mut p, &c);
    assert_eq!(after(&p), "working v2\n");
}

#[test]
fn refresh_keeps_shared_draft_when_one_repository_becomes_clean() {
    let (dir, mut c, mut p) = fixture();
    let second = dir.path().join("second");
    fs::create_dir(&second).unwrap();
    git(&second, &["init", "-q", "-b", "main"]).unwrap();
    fs::write(second.join("file.txt"), "second\n").unwrap();
    git(&second, &["add", "."]).unwrap();
    c.repos.push(crate::core::Repo {
        path: second,
        name: "second".into(),
        ..c.repos[0].clone()
    });
    c.scope.insert(1);
    p.refresh(&c).unwrap();
    p.edit_message(&c);
    p.buf = "draft 中文".into();
    p.caret = "draft ".len();
    p.message_selection.sync_source(&p.buf);
    p.message_selection.extend(p.caret, p.buf.len());
    let selection = p.message_selection.range();
    git(
        &c.repos[0].path,
        &["restore", "--source=HEAD", "--staged", "--worktree", "."],
    )
    .unwrap();
    assert!(scan(&mut p, &mut c));
    draw(&mut p, &c);
    assert_eq!(p.buf, "draft 中文");
    assert_eq!(p.caret, "draft ".len());
    assert_eq!(p.message_selection.range(), selection);
    p.finish_message(&c).unwrap();
    assert_eq!(p.saved.message.as_deref().unwrap(), "draft 中文");
}

#[test]
fn late_scan_cannot_replace_an_explicit_refresh_or_ignored_option_change() {
    let (_dir, mut c, mut p) = fixture();
    let stale = Snapshot::read(c.clone(), false, None, [0].into());
    let (send, receive) = mpsc::channel();
    p.refresh.pending = Some(Pending {
        generation: p.refresh.generation,
        roots: [0].into(),
        receiver: receive,
    });
    fs::write(c.repos[0].path.join("new.txt"), "new\n").unwrap();
    p.refresh(&c).unwrap();
    send.send(stale).ok().unwrap();
    assert!(!p.poll_refresh(&mut c).unwrap());
    assert!(p
        .files
        .iter()
        .any(|f| f.change.path == Path::new("new.txt")));
    fs::write(c.repos[0].path.join(".gitignore"), "ignored.txt\n").unwrap();
    fs::write(c.repos[0].path.join("ignored.txt"), "ignored\n").unwrap();
    let stale = Snapshot::read(c.clone(), false, None, [0].into());
    let (send, receive) = mpsc::channel();
    p.refresh.pending = Some(Pending {
        generation: p.refresh.generation,
        roots: [0].into(),
        receiver: receive,
    });
    p.toggle_view_item(display::Item::Ignored, &c).unwrap();
    send.send(stale).ok().unwrap();
    p.poll_refresh(&mut c).unwrap();
    assert!(p.files.iter().any(|f| f.change.ignored()));
    // Drain the replacement scan before the disposable repository is removed.
    scan(&mut p, &mut c);
    assert!(p.files.iter().any(|f| f.change.ignored()));
}

#[test]
fn add_completion_requeues_pending_repositories_it_did_not_refresh() {
    let (dir, mut c, mut p) = fixture();
    let second = dir.path().join("second");
    fs::create_dir(&second).unwrap();
    git(&second, &["init", "-q", "-b", "main"]).unwrap();
    fs::write(second.join("file.txt"), "second\n").unwrap();
    git(&second, &["add", "."]).unwrap();
    c.repos.push(crate::core::Repo {
        path: second,
        name: "second".into(),
        ..c.repos[0].clone()
    });
    c.scope.insert(1);
    p.refresh(&c).unwrap();
    p.refresh.watch = None;
    let status = backend::status(&c.repos[0].path).map_err(|error| error.to_string());
    let (_send, receive) = mpsc::channel();
    p.refresh.pending = Some(Pending {
        generation: p.refresh.generation,
        roots: [0, 1].into(),
        receiver: receive,
    });
    p.apply_add_statuses(&c, [(0, status)].into());
    assert!(p.refresh.pending.is_none());
    assert_eq!(p.refresh.deferred, [1].into());
    assert_eq!(next_scan(&mut p, &mut c), [1].into());
}

#[test]
fn unchanged_scan_keeps_the_row_projection_but_roots_changes_rebuild_it() {
    let (_dir, mut c, mut p) = fixture();
    let roots = (0..c.repos.len()).collect();
    let view = p.view.as_ptr();
    assert!(!p.apply_statuses(&c, statuses(&c, false, &roots)));
    assert_eq!(p.view.as_ptr(), view);
    let file = p
        .view
        .iter()
        .find(|row| matches!(row.kind, Kind::File(_)))
        .unwrap()
        .id
        .clone();
    c.scope.clear();
    assert!(p.apply_statuses(&c, statuses(&c, false, &roots)));
    assert!(p.view.iter().all(|row| row.id != file));
    assert!(p.view.iter().all(|row| row.files.is_empty()));
}

#[test]
fn refresh_pending_tracks_only_explicit_requests_and_active_workers() {
    let (_dir, _c, mut p) = fixture();
    assert!(!p.refresh_pending());
    p.refresh.next = Some(Instant::now() + FALLBACK);
    p.refresh.deferred.insert(0);
    assert!(!p.refresh_pending());
    p.request_refresh();
    assert!(p.refresh_pending());
    p.refresh.requested = false;
    let (_send, receive) = mpsc::channel();
    p.refresh.pending = Some(Pending {
        generation: p.refresh.generation,
        roots: [0].into(),
        receiver: receive,
    });
    assert!(p.refresh_pending());
}

#[test]
fn refresh_keeps_last_successful_files_and_reports_a_repeated_error_once() {
    let (_dir, mut c, mut p) = fixture();
    let files = p.files.clone();
    for report in [true, false] {
        let (send, receive) = mpsc::channel();
        send.send(Snapshot {
            statuses: [(0, Err("repo unavailable".into()))].into(),
            branches: [(0, "main".into())].into(),
            preview: None,
        })
        .ok()
        .unwrap();
        p.refresh.pending = Some(Pending {
            generation: p.refresh.generation,
            roots: [0].into(),
            receiver: receive,
        });
        assert_eq!(p.poll_refresh(&mut c).is_err(), report);
        assert!(p.files == files);
    }
}

#[test]
fn invalid_saved_state_does_not_block_the_first_scan_and_is_preserved_before_save() {
    let (_dir, c, _) = fixture();
    fs::create_dir_all(&c.state_dir).unwrap();
    let path = c.state_dir.join("commit.json");
    let original = b"{\"message\":".to_vec();
    fs::write(&path, &original).unwrap();
    let mut p = CommitPanel::default();
    let error = p.refresh(&c).unwrap_err();
    assert!(error.to_string().contains("Saved Commit state"));
    assert!(p.loaded && p.refresh.watch.is_some());
    assert!(!p.files.is_empty());
    assert_eq!(fs::read(&path).unwrap(), original);
    p.saved.message = Some("recovered draft".into());
    p.save(&c).unwrap();
    let backup = c.state_dir.join("commit.json.invalid");
    assert_eq!(fs::read(backup).unwrap(), original);
    let mut reopened = CommitPanel::default();
    reopened.refresh(&c).unwrap();
    assert_eq!(reopened.saved.message.as_deref(), Some("recovered draft"));
}

#[test]
fn unreadable_saved_state_does_not_block_the_first_scan() {
    let (_dir, c, _) = fixture();
    fs::create_dir_all(&c.state_dir).unwrap();
    fs::create_dir(c.state_dir.join("commit.json")).unwrap();
    let mut p = CommitPanel::default();
    let error = p.refresh(&c).unwrap_err();
    assert!(error.to_string().contains("Saved Commit state"));
    assert!(p.loaded && p.refresh.watch.is_some());
    assert!(!p.files.is_empty());
}

fn settle(p: &mut CommitPanel, c: &mut Context) {
    let deadline = Instant::now() + Duration::from_secs(5);
    let quiet_after = Instant::now() + Duration::from_millis(700);
    while Instant::now() < quiet_after || p.refresh.pending.is_some() {
        p.poll_refresh(c).unwrap();
        assert!(Instant::now() < deadline, "watcher did not settle");
        thread::sleep(Duration::from_millis(10));
    }
}

/// Inspect the real worker's scanned repository IDs, then apply that same result.
fn next_scan(p: &mut CommitPanel, c: &mut Context) -> BTreeSet<usize> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(pending) = p.refresh.pending.take() {
            let snapshot = pending
                .receiver
                .recv_timeout(Duration::from_secs(5))
                .unwrap();
            let roots = snapshot.statuses.keys().copied().collect();
            let (send, receive) = mpsc::channel();
            send.send(snapshot).ok().unwrap();
            p.refresh.pending = Some(Pending {
                generation: pending.generation,
                roots: pending.roots,
                receiver: receive,
            });
            p.poll_refresh(c).unwrap();
            return roots;
        }
        p.poll_refresh(c).unwrap();
        assert!(
            Instant::now() < deadline,
            "native event did not start a scan"
        );
        thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn native_watching_stays_idle_and_scans_only_the_changed_repository() {
    let (_dir, mut c, mut p) = fixture();
    let (_other, other, other_panel) = fixture();
    drop(other_panel);
    c.repos.push(other.repos[0].clone());
    c.scope.insert(1);
    p.refresh(&c).unwrap();
    settle(&mut p, &mut c);
    // More than the old one-second interval, including repeated focus/tab resumes.
    let until = Instant::now() + Duration::from_millis(1200);
    while Instant::now() < until {
        p.resume_refresh();
        assert!(!p.poll_refresh(&mut c).unwrap());
        assert!(
            p.refresh.pending.is_none(),
            "idle focus changes started Git commands"
        );
        thread::sleep(Duration::from_millis(10));
    }
    select(&mut p, "working.txt");
    draw(&mut p, &c);
    let preview = p.diff_sources.clone();
    let files = p
        .files
        .iter()
        .filter(|f| f.root == 0)
        .cloned()
        .collect::<Vec<_>>();
    fs::write(c.repos[1].path.join("native.txt"), "native\n").unwrap();
    assert_eq!(next_scan(&mut p, &mut c), [1].into());
    assert!(p
        .files
        .iter()
        .any(|f| f.root == 1 && f.change.path == Path::new("native.txt")));
    assert!(p.files.iter().filter(|f| f.root == 0).eq(files.iter()));
    assert_eq!(
        p.diff_sources, preview,
        "another repository must not reload this Diff"
    );
}

#[test]
fn native_edit_atomic_save_delete_and_index_changes_update_current_diff() {
    let (_dir, mut c, mut p) = fixture();
    settle(&mut p, &mut c);
    let repo = c.repos[0].path.clone();
    let head = git(&repo, &["rev-parse", "HEAD"]).unwrap();
    select(&mut p, "working.txt");
    draw(&mut p, &c);
    for body in ["v2\n", "v3\n"] {
        fs::write(repo.join("working.txt"), body).unwrap();
        assert_eq!(next_scan(&mut p, &mut c), [0].into());
        assert_eq!(after(&p), body);
    }
    fs::write(repo.join("temporary"), "atomic\n").unwrap();
    fs::rename(repo.join("temporary"), repo.join("atomic.txt")).unwrap();
    assert_eq!(next_scan(&mut p, &mut c), [0].into());
    assert!(p
        .files
        .iter()
        .any(|f| f.change.path == Path::new("atomic.txt")));
    assert!(!p
        .files
        .iter()
        .any(|f| f.change.path == Path::new("temporary")));
    fs::remove_file(repo.join("atomic.txt")).unwrap();
    assert_eq!(next_scan(&mut p, &mut c), [0].into());
    assert!(!p
        .files
        .iter()
        .any(|f| f.change.path == Path::new("atomic.txt")));
    git(&repo, &["add", "working.txt"]).unwrap();
    fs::write(repo.join("working.txt"), "unstaged v4\n").unwrap();
    let index = git(&repo, &["ls-files", "--stage"]).unwrap();
    assert_eq!(next_scan(&mut p, &mut c), [0].into());
    assert_eq!(after(&p), "v3\n");
    assert_eq!(git(&repo, &["ls-files", "--stage"]).unwrap(), index);
    assert_eq!(git(&repo, &["rev-parse", "HEAD"]).unwrap(), head);
    assert!(!p
        .selection(&c)
        .iter()
        .any(|f| f.change.path == Path::new("other.txt")));
}

#[test]
fn native_events_during_a_scan_are_not_lost_and_fallback_covers_missing_notifications() {
    let (_dir, mut c, mut p) = fixture();
    settle(&mut p, &mut c);
    let stale = Snapshot::read(c.clone(), false, None, [0].into());
    let (send, receive) = mpsc::channel();
    p.refresh.pending = Some(Pending {
        generation: p.refresh.generation,
        roots: [0].into(),
        receiver: receive,
    });
    fs::write(c.repos[0].path.join("during.txt"), "during scan\n").unwrap();
    // The worker is still busy when the filesystem delivers this event.
    thread::sleep(Duration::from_millis(400));
    assert!(!p.poll_refresh(&mut c).unwrap());
    send.send(stale).ok().unwrap();
    p.poll_refresh(&mut c).unwrap();
    assert_eq!(next_scan(&mut p, &mut c), [0].into());
    assert!(p
        .files
        .iter()
        .any(|f| f.change.path == Path::new("during.txt")));
    p.refresh.watch = None; // Model a filesystem where native notifications are unavailable.
    fs::write(c.repos[0].path.join("missed.txt"), "missed event\n").unwrap();
    assert!(!p.poll_refresh(&mut c).unwrap());
    assert!(p.refresh.pending.is_none());
    p.refresh.next = Some(Instant::now()); // Advance the 30-second fallback deadline.
    assert_eq!(next_scan(&mut p, &mut c), [0].into());
    assert!(p
        .files
        .iter()
        .any(|f| f.change.path == Path::new("missed.txt")));
    assert!(p.refresh.next.unwrap() > Instant::now() + Duration::from_secs(29));
}

#[test]
fn linked_worktree_native_index_changes_do_not_scan_the_main_worktree() {
    let (_dir, mut c, mut p) = fixture();
    let linked_dir = tempfile::tempdir().unwrap();
    let linked = linked_dir.path().join("linked");
    git(
        &c.repos[0].path,
        &["worktree", "add", "--detach", linked.to_str().unwrap()],
    )
    .unwrap();
    c.repos.push(crate::core::Repo {
        path: fs::canonicalize(&linked).unwrap(),
        name: "linked".into(),
        ..c.repos[0].clone()
    });
    c.scope.insert(1);
    p.refresh(&c).unwrap();
    settle(&mut p, &mut c);
    fs::write(linked.join("working.txt"), "linked staged\n").unwrap();
    assert_eq!(next_scan(&mut p, &mut c), [1].into());
    git(&linked, &["add", "working.txt"]).unwrap();
    assert_eq!(next_scan(&mut p, &mut c), [1].into());
    let file = p
        .files
        .iter()
        .find(|f| f.root == 1 && f.change.path == Path::new("working.txt"))
        .unwrap();
    assert!(file.change.staged());
}
