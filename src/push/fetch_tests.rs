use super::*;
use ratatui::{backend::TestBackend, buffer::Buffer, Terminal};
use std::fs;

fn fixture() -> (tempfile::TempDir, Context, PushPanel) {
    let directory = tempfile::tempdir().unwrap();
    let path = tests::make(directory.path(), "app");
    let context = Context::discover(&path).unwrap();
    let mut panel = PushPanel::default();
    panel.refresh(&context).unwrap();
    panel.finish_loading(&context);
    (directory, context, panel)
}

fn run(path: &Path, args: &[&str]) -> String {
    git(path, args).unwrap()
}

fn endpoint(directory: &Path, context: &Context, name: &str, tip: &str) -> PathBuf {
    let path = directory.join(name);
    run(
        directory,
        &["init", "--bare", "-q", "-b", "main", path.to_str().unwrap()],
    );
    run(
        &context.repos[0].path,
        &[
            "push",
            "-q",
            "--",
            path.to_str().unwrap(),
            &format!("{tip}:refs/heads/release"),
        ],
    );
    path
}

fn use_endpoint(context: &Context, panel: &mut PushPanel, path: &Path) {
    run(
        &context.repos[0].path,
        &[
            "remote",
            "set-url",
            "--push",
            "origin",
            path.to_str().unwrap(),
        ],
    );
    panel.targets.insert(
        0,
        Target {
            remote: "origin".into(),
            dst: "release".into(),
        },
    );
}

fn observe(context: &Context, branch: &str, tip: Option<String>) -> fetch_backend::Observation {
    let target = fetch_backend::push_target(&context.repos[0].path, "origin", branch).unwrap();
    fetch_backend::Observation {
        path: target.path,
        remote: target.key.remote,
        endpoint: target.endpoint,
        branch: target.key.branch,
        tip,
    }
}

fn finish_operation(panel: &mut PushPanel, context: &Context) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while panel.busy() {
        panel.poll(context);
        assert!(
            Instant::now() < deadline,
            "Push remote operation did not finish"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
    panel.finish_loading(context);
}

fn draw(panel: &mut PushPanel, context: &Context, width: u16, height: u16) -> Buffer {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal
        .draw(|frame| panel.draw(frame, frame.area(), context))
        .unwrap();
    terminal.backend().buffer().clone()
}

fn text_at(buffer: &Buffer, area: Rect) -> String {
    (area.x..area.right())
        .map(|column| buffer[(column, area.y)].symbol())
        .collect()
}

#[test]
fn fetch_targets_use_current_scoped_targets_without_git_or_checkboxes() {
    let context = Context {
        workspace: "/workspace".into(),
        state_dir: "/state".into(),
        scope: [0].into(),
        repos: ["app", "hidden"]
            .into_iter()
            .map(|name| Repo {
                path: Path::new("/workspace").join(name),
                name: name.into(),
                branch: "main".into(),
                submodule: false,
                parent: None,
            })
            .collect(),
    };
    let panel = PushPanel {
        load_scope: Some(Scope::new(&context)),
        entries: (0..2)
            .map(|idx| Entry {
                idx,
                t: Target {
                    remote: "origin".into(),
                    dst: "main".into(),
                },
                ..Default::default()
            })
            .collect(),
        targets: [(
            0,
            Target {
                remote: "publish".into(),
                dst: "release".into(),
            },
        )]
        .into(),
        ..Default::default()
    };
    let control = ReadControl::default();
    control.cancel();
    assert_eq!(
        with_read_control(&control, || panel.fetch_targets(&context)),
        [(
            context.repos[0].path.clone(),
            vec![("publish".into(), "release".into())],
        )]
        .into()
    );
    let mut changed = context.clone();
    changed.repos[0].path = "/different-workspace/app".into();
    assert!(panel.fetch_targets(&changed).is_empty());
}

#[test]
fn an_uncached_push_endpoint_does_not_reuse_another_endpoints_tracking_ref() {
    let (directory, context, mut panel) = fixture();
    let path = &context.repos[0].path;
    let head = run(path, &["rev-parse", "HEAD"]);
    let base = run(path, &["rev-parse", "HEAD~1"]);
    let push = endpoint(directory.path(), &context, "publish.git", &head);
    use_endpoint(&context, &mut panel, &push);
    run(path, &["update-ref", "refs/remotes/origin/release", &base]);
    panel.refresh(&context).unwrap();
    panel.finish_loading(&context);

    assert!(panel.entries[0].tracking.is_none());
    assert!(panel.entries[0].behind.is_none());
    assert_eq!(panel.entries[0].n, 2);
    assert_eq!(
        run(path, &["rev-parse", "refs/remotes/origin/release"]),
        base
    );
}

#[test]
fn observed_push_tip_overrides_the_fetch_endpoints_tracking_ref() {
    let (directory, context, mut panel) = fixture();
    let path = &context.repos[0].path;
    let head = run(path, &["rev-parse", "HEAD"]);
    let base = run(path, &["rev-parse", "HEAD~1"]);
    let push = endpoint(directory.path(), &context, "publish.git", &head);
    use_endpoint(&context, &mut panel, &push);
    run(path, &["update-ref", "refs/remotes/origin/release", &base]);
    panel.remember_fetch(observe(&context, "other-branch", Some(base.clone())));
    panel.remember_fetch(observe(&context, "release", Some(head.clone())));
    panel.finish_loading(&context);

    assert_eq!(panel.entries[0].tracking.as_ref(), Some(&head));
    assert_eq!(panel.entries[0].n, 0);
    assert_eq!(panel.entries[0].behind, Some(0));
    assert_eq!(
        run(path, &["rev-parse", "refs/remotes/origin/release"]),
        base
    );
    assert!(panel.take_fetch_observations().is_empty());
}

#[test]
fn a_missing_observed_push_branch_does_not_fall_back_to_tracking() {
    let (directory, context, mut panel) = fixture();
    let path = &context.repos[0].path;
    let base = run(path, &["rev-parse", "HEAD~1"]);
    run(
        &directory.path().join("app.git"),
        &["update-ref", "-d", "refs/heads/main"],
    );
    panel.remember_fetch(observe(&context, "main", None));
    panel.finish_loading(&context);

    assert!(panel.entries[0].tracking.is_none());
    assert!(panel.entries[0].behind.is_none());
    assert_eq!(panel.entries[0].n, 2);
    assert_eq!(run(path, &["rev-parse", "refs/remotes/origin/main"]), base);
}

#[test]
fn changing_the_effective_push_url_invalidates_the_previous_observation() {
    let (directory, context, mut panel) = fixture();
    let path = &context.repos[0].path;
    let head = run(path, &["rev-parse", "HEAD"]);
    let base = run(path, &["rev-parse", "HEAD~1"]);
    let first = endpoint(directory.path(), &context, "first.git", &head);
    let second = endpoint(directory.path(), &context, "second.git", &base);
    use_endpoint(&context, &mut panel, &first);
    run(path, &["update-ref", "refs/remotes/origin/release", &base]);
    panel.remember_fetch(observe(&context, "release", Some(head)));
    use_endpoint(&context, &mut panel, &second);
    panel.finish_loading(&context);

    assert!(panel.fetched.is_empty());
    assert!(panel.entries[0].tracking.is_none());
    assert!(panel.entries[0].behind.is_none());
    use_endpoint(&context, &mut panel, &first);
    panel.refresh(&context).unwrap();
    panel.finish_loading(&context);
    assert!(panel.entries[0].tracking.is_none());
    assert_eq!(panel.entries[0].n, 2);
}

#[test]
fn relative_push_endpoints_match_the_workers_effective_endpoint() {
    let (directory, context, mut panel) = fixture();
    let head = run(&context.repos[0].path, &["rev-parse", "HEAD"]);
    endpoint(directory.path(), &context, "publish.git", &head);
    use_endpoint(&context, &mut panel, Path::new("../publish.git"));
    let observation = observe(&context, "release", Some(head.clone()));
    assert!(Path::new(&observation.endpoint).is_absolute());
    panel.remember_fetch(observation);
    panel.finish_loading(&context);
    assert_eq!(panel.entries[0].tracking.as_ref(), Some(&head));
    assert_eq!(panel.entries[0].n, 0);
}

#[test]
fn successful_push_replaces_a_cached_target_tip() {
    let (directory, context, mut panel) = fixture();
    let path = &context.repos[0].path;
    let head = run(path, &["rev-parse", "HEAD"]);
    let base = run(path, &["rev-parse", "HEAD~1"]);
    fs::write(path.join("a"), "staged content\n").unwrap();
    run(path, &["add", "a"]);
    fs::write(path.join("a"), "working content\n").unwrap();
    let index = fs::read(path.join(".git/index")).unwrap();
    panel.remember_fetch(observe(&context, "main", Some(base)));
    panel.finish_loading(&context);
    panel.pos = 1;
    panel.inspect(&context);
    panel.finish_loading(&context);
    assert_eq!(panel.entries[0].n, 1);
    assert!(!panel.preview.files.is_empty());
    let checked = panel.checked.clone();
    let expanded = panel.expanded.clone();
    assert!(matches!(
        panel.review(&context, false),
        Action::Confirm { .. }
    ));
    panel.start(&context, false);
    panel.load.request();
    let stale = panel.load.hold();
    stale.send(panel.entries.clone()).unwrap();
    finish_operation(&mut panel, &context);

    assert_eq!(
        run(&directory.path().join("app.git"), &["rev-parse", "main"]),
        head
    );
    assert_eq!(panel.results[&0], Ok(PushOutcome::Complete));
    assert_eq!(panel.entries[0].tracking.as_ref(), Some(&head));
    assert_eq!((panel.entries[0].n, panel.entries[0].behind), (0, Some(0)));
    assert!(panel.entries[0].logs.is_empty());
    assert_eq!(panel.selected(), Some((0, None)));
    assert!(panel.preview.files.is_empty());
    assert_eq!(panel.checked, checked);
    assert_eq!(panel.expanded, expanded);
    assert!(panel.take_fetch_observations().is_empty());
    assert_eq!(fs::read(path.join(".git/index")).unwrap(), index);
    assert_eq!(fs::read(path.join("a")).unwrap(), b"working content\n");
    assert_eq!(run(path, &["rev-parse", "HEAD"]), head);
}

#[test]
fn successful_push_updates_only_its_effective_endpoint_and_branch() {
    for relative in [false, true] {
        let (directory, context, mut panel) = fixture();
        let path = &context.repos[0].path;
        let head = run(path, &["rev-parse", "HEAD"]);
        let base = run(path, &["rev-parse", "HEAD~1"]);
        let push = endpoint(directory.path(), &context, "publish.git", &base);
        use_endpoint(
            &context,
            &mut panel,
            if relative {
                Path::new("../publish.git")
            } else {
                &push
            },
        );
        run(
            path,
            &[
                "config",
                "remote.origin.fetch",
                "+refs/heads/main:refs/remotes/origin/main",
            ],
        );
        run(path, &["update-ref", "refs/remotes/origin/release", &base]);
        let other = observe(&context, "other-branch", Some(base.clone()));
        let other_key = (
            other.path.clone(),
            other.remote.clone(),
            other.branch.clone().unwrap(),
        );
        let other_value = (other.endpoint.clone(), other.tip.clone());
        panel.remember_fetch(other);
        panel.remember_fetch(observe(&context, "release", Some(base.clone())));
        panel.finish_loading(&context);
        assert_eq!(panel.entries[0].n, 1);
        assert!(matches!(
            panel.review(&context, false),
            Action::Confirm { .. }
        ));
        panel.start(&context, false);
        finish_operation(&mut panel, &context);

        assert_eq!(panel.results[&0], Ok(PushOutcome::Complete));
        assert_eq!(panel.entries[0].tracking.as_ref(), Some(&head));
        assert_eq!((panel.entries[0].n, panel.entries[0].behind), (0, Some(0)));
        assert!(panel.entries[0].logs.is_empty());
        assert_eq!(panel.fetched[&other_key], other_value);
        assert_eq!(run(&push, &["rev-parse", "release"]), head);
        assert_eq!(
            run(&directory.path().join("app.git"), &["rev-parse", "main"]),
            base
        );
        assert_eq!(
            run(path, &["rev-parse", "refs/remotes/origin/release"]),
            base
        );
        assert!(panel.take_fetch_observations().is_empty());
    }
}

#[test]
fn successful_first_push_replaces_an_observed_missing_target() {
    let (directory, context, mut panel) = fixture();
    let path = &context.repos[0].path;
    let head = run(path, &["rev-parse", "HEAD"]);
    panel.targets.insert(
        0,
        Target {
            remote: "origin".into(),
            dst: "release".into(),
        },
    );
    panel.remember_fetch(observe(&context, "release", None));
    panel.finish_loading(&context);
    assert_eq!(panel.entries[0].n, 2);
    assert!(matches!(
        panel.review(&context, false),
        Action::Confirm { .. }
    ));
    panel.start(&context, false);
    finish_operation(&mut panel, &context);

    assert_eq!(panel.results[&0], Ok(PushOutcome::Complete));
    assert_eq!(panel.entries[0].tracking.as_ref(), Some(&head));
    assert_eq!((panel.entries[0].n, panel.entries[0].behind), (0, Some(0)));
    assert!(panel.preview.files.is_empty());
    assert_eq!(
        run(&directory.path().join("app.git"), &["rev-parse", "release"]),
        head
    );
}

#[test]
fn integrated_push_refreshes_against_the_published_merge_or_rebase() {
    for strategy in [Strategy::Merge, Strategy::Rebase] {
        let (directory, context, mut panel) = fixture();
        let path = &context.repos[0].path;
        let original = run(path, &["rev-parse", "HEAD"]);
        let base = run(path, &["rev-parse", "HEAD~1"]);
        let writer = directory.path().join("writer");
        run(
            directory.path(),
            &[
                "clone",
                "-q",
                directory.path().join("app.git").to_str().unwrap(),
                writer.to_str().unwrap(),
            ],
        );
        for (key, value) in [
            ("user.name", "IGit test"),
            ("user.email", "test@invalid"),
            ("commit.gpgSign", "false"),
            ("core.hooksPath", "/dev/null"),
        ] {
            run(&writer, &["config", key, value]);
        }
        run(&writer, &["commit", "--allow-empty", "-qm", "remote work"]);
        run(&writer, &["push", "-q", "origin", "main"]);
        panel.set_strategy(strategy);
        panel.remember_fetch(observe(&context, "main", Some(base)));
        panel.finish_loading(&context);
        assert!(matches!(
            panel.review(&context, false),
            Action::Confirm { .. }
        ));
        panel.start(&context, false);
        finish_operation(&mut panel, &context);

        let published = run(&directory.path().join("app.git"), &["rev-parse", "main"]);
        assert_ne!(published, original);
        assert_eq!(run(path, &["rev-parse", "HEAD"]), published);
        assert_eq!(panel.results[&0], Ok(PushOutcome::Updated(strategy)));
        assert_eq!(panel.entries[0].tracking.as_ref(), Some(&published));
        assert_eq!((panel.entries[0].n, panel.entries[0].behind), (0, Some(0)));
        assert!(panel.entries[0].logs.is_empty());
        assert!(panel.preview.files.is_empty());
    }
}

#[test]
fn an_up_to_date_result_replaces_a_cached_target_tip() {
    let (directory, context, mut panel) = fixture();
    let path = &context.repos[0].path;
    let head = run(path, &["rev-parse", "HEAD"]);
    let base = run(path, &["rev-parse", "HEAD~1"]);
    panel.remember_fetch(observe(&context, "main", Some(base)));
    panel.finish_loading(&context);
    assert!(matches!(
        panel.review(&context, false),
        Action::Confirm { .. }
    ));
    run(path, &["push", "-q", "origin", "main"]);
    panel.start(&context, false);
    finish_operation(&mut panel, &context);

    assert_eq!(panel.results[&0], Ok(PushOutcome::UpToDate));
    assert_eq!(panel.entries[0].tracking.as_ref(), Some(&head));
    assert_eq!((panel.entries[0].n, panel.entries[0].behind), (0, Some(0)));
    assert!(panel.entries[0].logs.is_empty());
    assert_eq!(
        run(&directory.path().join("app.git"), &["rev-parse", "main"]),
        head
    );
}

#[test]
fn commits_created_after_publication_remain_outgoing() {
    let (directory, context, mut panel) = fixture();
    let path = &context.repos[0].path;
    let published = run(path, &["rev-parse", "HEAD"]);
    let base = run(path, &["rev-parse", "HEAD~1"]);
    panel.remember_fetch(observe(&context, "main", Some(base)));
    panel.finish_loading(&context);
    assert!(matches!(
        panel.review(&context, false),
        Action::Confirm { .. }
    ));
    panel.start(&context, false);
    let reports = panel.inflight.take().unwrap();
    let (sender, receiver) = mpsc::channel();
    loop {
        let report = reports.recv_timeout(Duration::from_secs(10)).unwrap();
        let done = matches!(report, Report::Done);
        sender.send(report).unwrap();
        if done {
            break;
        }
    }
    panel.inflight = Some(receiver);
    run(path, &["commit", "--allow-empty", "-qm", "later work"]);
    let later = run(path, &["rev-parse", "HEAD"]);
    finish_operation(&mut panel, &context);

    assert_eq!(panel.results[&0], Ok(PushOutcome::Complete));
    assert_eq!(panel.entries[0].tracking.as_ref(), Some(&published));
    assert_eq!((panel.entries[0].n, panel.entries[0].behind), (1, Some(0)));
    assert_eq!(
        panel.entries[0].logs,
        vec![Out {
            id: later,
            msg: "later work".into(),
        }]
    );
    assert_eq!(
        run(&directory.path().join("app.git"), &["rev-parse", "main"]),
        published
    );
}

#[test]
fn manual_fetch_reads_only_the_push_endpoint_branch_and_emits_an_observation() {
    let (directory, context, mut panel) = fixture();
    let path = &context.repos[0].path;
    let head = run(path, &["rev-parse", "HEAD"]);
    let base = run(path, &["rev-parse", "HEAD~1"]);
    let push = endpoint(directory.path(), &context, "publish.git", &head);
    use_endpoint(&context, &mut panel, &push);
    run(&push, &["update-ref", "refs/heads/unreviewed", &head]);
    run(&push, &["update-ref", "refs/tags/unreviewed-tag", &head]);
    run(path, &["update-ref", "refs/remotes/origin/release", &base]);
    fs::write(path.join("a"), "staged content\n").unwrap();
    run(path, &["add", "a"]);
    fs::write(path.join("a"), "working content\n").unwrap();
    let index = fs::read(path.join(".git/index")).unwrap();
    let refs = run(path, &["show-ref"]);
    let fetch_head = fs::read(path.join(".git/FETCH_HEAD")).ok();
    let expected = observe(&context, "release", Some(head.clone()));
    panel.checked.clear();
    panel.refresh(&context).unwrap();
    panel.finish_loading(&context);

    assert!(matches!(
        panel.start(&context, true),
        Action::Notice(NoticeLevel::Info, _)
    ));
    finish_operation(&mut panel, &context);

    assert_eq!(panel.results[&0], Ok(PushOutcome::Complete));
    assert_eq!(panel.entries[0].tracking.as_ref(), Some(&head));
    assert_eq!(panel.entries[0].n, 0);
    assert!(panel.checked.is_empty());
    let observations = panel.take_fetch_observations();
    assert_eq!(observations.len(), 1);
    let observation = &observations[0];
    assert_eq!(observation.path, expected.path);
    assert_eq!(observation.remote, expected.remote);
    assert_eq!(observation.endpoint, expected.endpoint);
    assert_eq!(observation.branch, Some("release".into()));
    assert_eq!(observation.tip.as_ref(), Some(&head));
    assert!(panel.take_fetch_observations().is_empty());
    assert_eq!(run(path, &["show-ref"]), refs);
    assert_eq!(fs::read(path.join(".git/index")).unwrap(), index);
    assert_eq!(fs::read(path.join(".git/FETCH_HEAD")).ok(), fetch_head);
    assert_eq!(fs::read(path.join("a")).unwrap(), b"working content\n");
}

#[test]
fn a_failed_manual_fetch_does_not_publish_an_observation() {
    let (directory, context, mut panel) = fixture();
    use_endpoint(
        &context,
        &mut panel,
        &directory.path().join("unavailable.git"),
    );
    panel.refresh(&context).unwrap();
    panel.finish_loading(&context);
    panel.start(&context, true);
    finish_operation(&mut panel, &context);
    assert!(panel.results[&0].is_err());
    assert!(panel.take_fetch_observations().is_empty());
    assert!(panel.fetched.is_empty());
}

#[test]
fn an_inflight_snapshot_waits_for_a_push_review_without_changing_it() {
    let (_directory, context, mut panel) = fixture();
    assert!(matches!(
        panel.review(&context, false),
        Action::Confirm { .. }
    ));
    let reviewed = panel.review[0].head.clone();
    let entries = panel.entries.clone();
    let selected = panel.selected_identity();
    let checked = panel.checked.clone();
    panel.load.request();
    let release = panel.load.hold();
    let mut updated = entries.clone();
    updated[0].n += 1;
    release.send(updated).unwrap();
    panel.poll_loading(&context).unwrap();

    assert_eq!(panel.entries, entries);
    assert_eq!(panel.review[0].head, reviewed);
    assert_eq!(panel.selected_identity(), selected);
    assert_eq!(panel.checked, checked);
    assert!(panel.load.pending());
    panel.review.clear();
    panel.poll_loading(&context).unwrap();
    assert_eq!(panel.entries[0].n, entries[0].n + 1);
}

#[test]
fn an_inflight_snapshot_waits_for_selected_details_text() {
    let (_directory, context, mut panel) = fixture();
    panel.show_details = true;
    panel.focus = 2;
    draw(&mut panel, &context, 120, 36);
    panel.detail_selection.select_all();
    let selected = panel.selected_text().unwrap().to_owned();
    let entries = panel.entries.clone();
    panel.load.request();
    let release = panel.load.hold();
    let mut updated = entries.clone();
    updated[0].n += 1;
    release.send(updated).unwrap();
    panel.poll_loading(&context).unwrap();

    assert!(!panel.refresh_allowed());
    assert_eq!(panel.entries, entries);
    assert_eq!(panel.selected_text(), Some(selected.as_str()));
    panel.detail_selection.clear();
    panel.poll_loading(&context).unwrap();
    assert_eq!(panel.entries[0].n, entries[0].n + 1);
}

#[test]
fn automatic_fetch_preserves_an_unchanged_commit_preview_and_diff_state() {
    let (_directory, context, mut panel) = fixture();
    let path = &context.repos[0].path;
    for file in ["folded/a", "visible/b"] {
        fs::create_dir_all(path.join(file).parent().unwrap()).unwrap();
        fs::write(path.join(file), format!("{file}\n")).unwrap();
    }
    run(path, &["add", "."]);
    run(path, &["commit", "-qm", "Add grouped files"]);
    panel.refresh(&context).unwrap();
    panel.finish_loading(&context);
    panel.pos = 1;
    panel.inspect(&context);
    panel.finish_loading(&context);
    let selected = panel.selected_identity();
    assert!(matches!(&selected, Some((_, Some(_)))));
    panel.preview.tree.cursor = panel
        .preview
        .tree
        .visible
        .iter()
        .position(|&id| {
            matches!(
                &panel.preview.tree.nodes[id].kind,
                Kind::Directory { path, .. } if path == Path::new("folded")
            )
        })
        .unwrap();
    panel.preview.tree.toggle();
    panel.preview.tree.cursor = panel
        .preview
        .tree
        .visible
        .iter()
        .position(|&id| {
            matches!(
                panel.preview.tree.nodes[id].kind,
                Kind::File(index) if panel.preview.files[index].path == Path::new("visible/b")
            )
        })
        .unwrap();
    panel.preview.tree.offset = 1;
    panel.show_details = true;
    panel.detail_off = 3;
    let summary = panel.preview.summary.clone();
    let body = panel.preview.body.clone();
    let visible = panel.preview.tree.visible.clone();
    let folded = panel.preview.tree.folded.clone();
    let cursor = panel.preview.tree.cursor;
    let offset = panel.preview.tree.offset;
    let checked = panel.checked.clone();
    let parent = run(path, &["rev-parse", "HEAD~1"]);
    let tree = run(path, &["rev-parse", "HEAD~1^{tree}"]);
    let remote_tip = run(
        path,
        &["commit-tree", &tree, "-p", &parent, "-m", "Remote update"],
    );
    panel.remember_fetch(observe(&context, "main", Some(remote_tip.clone())));
    panel.finish_loading(&context);

    assert_eq!(panel.selected_identity(), selected);
    assert_eq!(panel.entries[0].tracking.as_ref(), Some(&remote_tip));
    assert_eq!(panel.entries[0].n, 1);
    assert_eq!(panel.entries[0].behind, Some(1));
    assert_eq!(panel.preview.summary, summary);
    assert_eq!(panel.preview.body, body);
    assert_eq!(panel.preview.tree.visible, visible);
    assert_eq!(panel.preview.tree.folded, folded);
    assert_eq!(panel.preview.tree.cursor, cursor);
    assert_eq!(panel.preview.tree.offset, offset);
    assert_eq!(panel.detail_off, 3);
    assert_eq!(panel.checked, checked);
    assert!(!panel.preview_load.pending());

    panel.open_file(&context);
    panel.finish_loading(&context);
    panel.diff.as_mut().unwrap().content = Some(preview::FileDiff::Patch(
        (0..80).map(|line| format!("Diff line {line}\n")).collect(),
    ));
    let unscrolled = draw(&mut panel, &context, 120, 36);
    let key = |code| Event::Key(crossterm::event::KeyEvent::new(code, KeyModifiers::NONE));
    panel.diff_event(&key(KeyCode::PageDown), &context);
    let scrolled = draw(&mut panel, &context, 120, 36);
    assert_ne!(scrolled, unscrolled);
    panel.load.request();
    let release = panel.load.hold();
    let mut entries = panel.entries.clone();
    entries[0].behind = Some(2);
    release.send(entries).unwrap();
    panel.poll_loading(&context).unwrap();
    assert!(panel.load.pending());
    assert_eq!(draw(&mut panel, &context, 120, 36), scrolled);
    panel.diff_event(&key(KeyCode::Esc), &context);
    panel.finish_loading(&context);

    assert_eq!(panel.entries[0].behind, Some(2));
    assert_eq!(panel.selected_identity(), selected);
    assert_eq!(panel.preview.tree.folded, folded);
    assert_eq!(panel.preview.tree.cursor, cursor);
    assert_eq!(panel.preview.tree.offset, offset);
    assert_eq!(panel.checked, checked);
    assert!(!panel.preview_load.pending());
}

#[test]
fn repository_previews_rebuild_when_the_fetch_target_changes() {
    let (_directory, context, mut panel) = fixture();
    assert_eq!(panel.selected(), Some((0, None)));
    assert_eq!(panel.preview.files[0].status, b'M');
    let summary = panel.preview.summary.clone();
    panel.detail_off = 3;
    panel.targets.insert(
        0,
        Target {
            remote: "origin".into(),
            dst: "release".into(),
        },
    );
    panel.remember_fetch(observe(&context, "release", None));
    panel.finish_loading(&context);

    assert_eq!(panel.selected(), Some((0, None)));
    assert_ne!(panel.preview.summary, summary);
    assert!(panel.preview.summary.contains("origin:release"));
    assert_eq!(panel.preview.files[0].status, b'A');
    assert_eq!(panel.detail_off, 0);
}

#[test]
fn explicit_selected_target_refresh_closes_the_obsolete_diff() {
    let (_directory, context, mut panel) = fixture();
    panel.pos = 1;
    panel.inspect(&context);
    panel.finish_loading(&context);
    let row = panel
        .preview
        .tree
        .visible
        .iter()
        .position(|&id| matches!(panel.preview.tree.nodes[id].kind, Kind::File(_)))
        .unwrap();
    panel.activate_file(row, false, &context);
    panel.finish_loading(&context);
    assert!(panel.diff.is_some());

    panel.pos = 0;
    panel.targets.insert(
        0,
        Target {
            remote: "origin".into(),
            dst: "new-target".into(),
        },
    );
    panel.refresh(&context).unwrap();
    assert!(panel.diff.is_none());
    assert!(panel.refresh_allowed());
    panel.finish_loading(&context);

    assert_eq!(panel.entries[0].t.dst, "new-target");
    assert_eq!(panel.selected(), Some((0, None)));
    assert!(panel.preview.summary.contains("origin:new-target"));
    assert!(!panel.preview.files.is_empty());
    assert!(panel.preview.files.iter().all(|file| file.status == b'A'));
}

#[test]
fn selected_freshness_keeps_toolbar_controls_and_selection_unchanged() {
    for icons in [IconMode::Ascii, IconMode::Unicode, IconMode::NerdFont] {
        let context = Context {
            workspace: "/workspace".into(),
            state_dir: "/state".into(),
            scope: [0, 1].into(),
            repos: ["app", "tools"]
                .into_iter()
                .map(|name| Repo {
                    path: Path::new("/workspace").join(name),
                    name: name.into(),
                    branch: "main".into(),
                    submodule: false,
                    parent: None,
                })
                .collect(),
        };
        let mut panel = PushPanel {
            icons,
            entries: (0..2)
                .map(|idx| Entry {
                    idx,
                    source: "main".into(),
                    t: Target {
                        remote: "origin".into(),
                        dst: "main".into(),
                    },
                    n: 1,
                    ..Default::default()
                })
                .collect(),
            checked: [0].into(),
            ..Default::default()
        };
        for width in [38, 44, 80, 120, 180] {
            draw(&mut panel, &context, width, 36);
            let controls = panel
                .hits
                .iter()
                .filter(|(_, hit)| matches!(hit, Hit::Fetch | Hit::Strategy | Hit::Send))
                .map(|(area, _)| *area)
                .collect::<Vec<_>>();
            let checked = panel.checked.clone();
            let selected = panel.selected();
            let status = [(0, "Fetched 1m ago".into()), (1, "Not fetched".into())].into();
            assert!(panel.set_fetch_status(status));
            let buffer = draw(&mut panel, &context, width, 36);
            assert_eq!(
                panel
                    .hits
                    .iter()
                    .filter(|(_, hit)| matches!(hit, Hit::Fetch | Hit::Strategy | Hit::Send))
                    .map(|(area, _)| *area)
                    .collect::<Vec<_>>(),
                controls
            );
            assert_eq!(panel.checked, checked);
            assert_eq!(panel.selected(), selected);
            assert!(!panel.set_fetch_status(panel.fetch_status.clone()));
            if width == 180 {
                let toolbar = panel
                    .hits
                    .iter()
                    .find_map(|(area, hit)| matches!(hit, Hit::Fetch).then_some(*area))
                    .unwrap();
                assert!(
                    text_at(&buffer, Rect::new(0, toolbar.y, width, 1)).contains("Fetched 1m ago")
                );
                panel.pos = 1;
                let buffer = draw(&mut panel, &context, width, 36);
                let text = text_at(&buffer, Rect::new(0, toolbar.y, width, 1));
                assert!(text.contains("Not fetched") && !text.contains("Fetched 1m ago"));
                panel.pos = 0;
            }
            assert!(panel.set_fetch_status(BTreeMap::new()));
        }
    }
}
