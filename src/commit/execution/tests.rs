use super::*;
use crate::core::git;
use std::path::PathBuf;

fn finish(panel: &mut CommitPanel, c: &Context) -> Action {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        let action = panel.poll_execution(c);
        if matches!(action, Action::Committed { .. }) {
            return action;
        }
        assert!(Instant::now() < deadline, "Commit worker did not finish");
        std::thread::yield_now();
    }
}

#[test]
fn repository_progress_redraws_without_emitting_notices() {
    let (_dir, c, mut panel) = super::super::message_tests::fixture();
    let (sender, receiver) = mpsc::channel();
    panel.execution.pending = Some(Pending {
        receiver,
        stop: Arc::new(AtomicBool::new(false)),
        completed: Vec::new(),
        push: false,
        message: panel.saved.message.clone(),
    });
    for label in ["Committing app · 1/2", "Committing api · 2/2"] {
        sender.send(Report::Started(label.into())).unwrap();
        assert!(matches!(panel.poll_execution(&c), Action::Redraw));
        assert_eq!(panel.execution.progress, label);
        assert!(panel.execution_running());
        assert!(matches!(panel.poll_execution(&c), Action::None));
    }
    assert!(matches!(
        panel.stop_execution(),
        Action::Notice(NoticeLevel::Info, _)
    ));
}

#[test]
fn background_commits_preserve_mixed_staging_and_shared_message_behavior() {
    let (_dir, c, mut panel) = super::super::message_tests::fixture();
    panel.enable_background_execution();
    let action = panel.submit(&c, false).unwrap();
    assert!(matches!(action, Action::Notice(NoticeLevel::Info, _)));
    assert!(panel.execution_running());
    assert!(panel.submit(&c, false).is_err());
    let Action::Committed {
        repos,
        level: NoticeLevel::Success,
        ..
    } = finish(&mut panel, &c)
    else {
        unreachable!()
    };
    assert_eq!(repos, [0, 1]);
    assert!(!panel.execution_running());
    assert_eq!(panel.saved.message.as_deref(), Some(""));
    for repo in &c.repos {
        assert_eq!(
            git(&repo.path, &["show", "HEAD:mixed.txt"]).unwrap(),
            format!("{} staged", repo.name)
        );
        assert_eq!(
            git(&repo.path, &["show", ":unselected.txt"]).unwrap(),
            "unselected staged marker"
        );
        assert_eq!(
            fs::read_to_string(repo.path.join("mixed.txt")).unwrap(),
            "unstaged private marker\n"
        );
    }
}

#[cfg(unix)]
#[test]
fn slow_filter_preparation_returns_immediately_and_stopping_preserves_drafts_and_index() {
    let (_dir, c, mut panel) = super::super::message_tests::fixture();
    for repo in &c.repos {
        git(&repo.path, &["restore", "--staged", "--", "mixed.txt"]).unwrap();
        fs::write(repo.path.join(".gitattributes"), "mixed.txt filter=slow\n").unwrap();
        git(
            &repo.path,
            &["config", "filter.slow.clean", "sleep 0.2; cat"],
        )
        .unwrap();
    }
    panel.refresh(&c).unwrap();
    let original_message = panel.saved.message.clone();
    let original: Vec<_> = c
        .repos
        .iter()
        .map(|repo| {
            (
                git(&repo.path, &["rev-parse", "HEAD"]).unwrap(),
                fs::read(repo.path.join(".git/index")).unwrap(),
            )
        })
        .collect();
    panel.enable_background_execution();
    let started = Instant::now();
    panel.submit(&c, false).unwrap();
    assert!(started.elapsed() < Duration::from_millis(150));
    panel.stop_execution();
    let deadline = Instant::now() + Duration::from_secs(10);
    while panel.execution_running() {
        panel.poll_execution(&c);
        assert!(Instant::now() < deadline, "Preparation did not stop");
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(panel.saved.message, original_message);
    for (repo, (head, index)) in c.repos.iter().zip(original) {
        assert_eq!(git(&repo.path, &["rev-parse", "HEAD"]).unwrap(), head);
        assert_eq!(fs::read(repo.path.join(".git/index")).unwrap(), index);
        assert_eq!(
            fs::read(repo.path.join("mixed.txt")).unwrap(),
            b"unstaged private marker\n"
        );
    }
}

#[cfg(unix)]
#[test]
fn interrupted_post_commit_keeps_recovery_lock_and_unselected_staged_content() {
    use crate::core::process::{with_write_control, WriteControl};
    use std::os::unix::fs::PermissionsExt;
    let (_dir, c, mut panel) = super::super::message_tests::fixture();
    let plan = panel.prepare_submission(&c).unwrap().remove(0);
    let repo = plan.repo.clone();
    let index = fs::read(repo.join(".git/index")).unwrap();
    let before = git(&repo, &["rev-parse", "HEAD"]).unwrap();
    let hook = repo.join(".git/hooks/post-commit");
    fs::write(&hook, "#!/bin/sh\n: > .git/post-commit-started\nsleep 30\n").unwrap();
    fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).unwrap();
    git(&repo, &["config", "core.hooksPath", ".git/hooks"]).unwrap();
    let control = WriteControl::default();
    let worker_control = control.clone();
    let worker =
        std::thread::spawn(move || with_write_control(&worker_control, || backend::execute(&plan)));
    let deadline = Instant::now() + Duration::from_secs(8);
    while !repo.join(".git/post-commit-started").exists() {
        if Instant::now() >= deadline {
            control.interrupt();
            panic!("Post-commit hook did not start");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    control.interrupt();
    let error = worker
        .join()
        .unwrap()
        .err()
        .expect("Interrupted commit must require verification");
    assert!(
        format!("{error:#}").contains("Keep index.lock"),
        "{error:#}"
    );
    assert_ne!(git(&repo, &["rev-parse", "HEAD"]).unwrap(), before);
    assert!(repo.join(".git/index.lock").exists());
    assert_eq!(fs::read(repo.join(".git/index")).unwrap(), index);
    assert_eq!(
        git(&repo, &["show", "HEAD:mixed.txt"]).unwrap(),
        "app staged"
    );
    assert_eq!(
        git(&repo, &["show", ":unselected.txt"]).unwrap(),
        "unselected staged marker"
    );
    assert_eq!(
        fs::read(repo.join("mixed.txt")).unwrap(),
        b"unstaged private marker\n"
    );
}

#[test]
fn completed_commits_report_warnings_for_cleanup_save_and_worker_failures() {
    for (cleanup_warning, save_failure, interrupted) in [
        (true, false, None),
        (false, true, None),
        (false, false, Some("Worker stopped")),
    ] {
        let (_dir, mut c, mut panel) = super::super::message_tests::fixture();
        let original = panel.saved.message.clone();
        let completed = panel
            .prepare_submission(&c)
            .unwrap()
            .into_iter()
            .map(|plan| Completed {
                plan,
                outcome: Ok(backend::Outcome {
                    hash: "created-commit".into(),
                    warning: cleanup_warning.then(|| "Inspect the index".into()),
                }),
            })
            .collect();
        if save_failure {
            c.state_dir = c.workspace.join("blocked-state");
            fs::write(&c.state_dir, "not a directory").unwrap();
        }
        let Action::Committed {
            repos,
            push,
            level: NoticeLevel::Warning,
            message,
        } = panel.finish_submission(&c, completed, true, original, interrupted)
        else {
            panic!("Completed commits with follow-up failures must report a warning");
        };
        assert_eq!(repos, [0, 1]);
        assert_eq!(push, !cleanup_warning);
        if save_failure {
            assert!(message.contains("Failed to save local state"), "{message}");
        }
        if let Some(reason) = interrupted {
            assert!(message.contains(reason), "{message}");
        }
    }
}

#[cfg(unix)]
#[test]
fn slow_hook_keeps_browsing_responsive_and_stop_skips_only_unstarted_repositories() {
    use std::os::unix::fs::PermissionsExt;
    let (dir, c, mut panel) = super::super::message_tests::fixture();
    let marker = dir.path().join("hook-ready");
    let release = dir.path().join("hook-release");
    struct Release(PathBuf);
    impl Drop for Release {
        fn drop(&mut self) {
            let _ = fs::write(&self.0, "go");
        }
    }
    let _release = Release(release.clone());
    let hooks = c.repos[0].path.join(".git/hooks");
    fs::create_dir_all(&hooks).unwrap();
    let hook = hooks.join("pre-commit");
    let quote = |path: &Path| path.to_string_lossy().replace('\'', "'\\''");
    fs::write(
        &hook,
        format!(
            "#!/bin/sh\nprintf ready > '{}'\nwhile [ ! -f '{}' ]; do sleep 0.01; done\n",
            quote(&marker),
            quote(&release)
        ),
    )
    .unwrap();
    fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).unwrap();
    git(
        &c.repos[0].path,
        &["config", "core.hooksPath", hooks.to_str().unwrap()],
    )
    .unwrap();
    let second_head = git(&c.repos[1].path, &["rev-parse", "HEAD"]).unwrap();
    let message = panel.saved.message.clone();
    panel.enable_background_execution();
    assert!(matches!(
        panel.submit(&c, false).unwrap(),
        Action::Notice(NoticeLevel::Info, _)
    ));
    let deadline = Instant::now() + Duration::from_secs(10);
    while !marker.exists() {
        panel.poll_execution(&c);
        assert!(Instant::now() < deadline, "Hook did not start");
        std::thread::yield_now();
    }
    let cursor = panel.cursor;
    panel.event(
        &Event::Key(crossterm::event::KeyEvent::new(K::Down, Mod::NONE)),
        &c,
    );
    assert!(panel.cursor != cursor);
    assert!(panel.execution_running());
    let checks = panel.checks.clone();
    panel.event(
        &Event::Key(crossterm::event::KeyEvent::new(K::Char(' '), Mod::NONE)),
        &c,
    );
    assert_eq!(panel.checks, checks);
    panel.stop_execution();
    fs::write(&release, "go").unwrap();
    let Action::Committed {
        repos,
        message: result,
        level: NoticeLevel::Warning,
        ..
    } = finish(&mut panel, &c)
    else {
        unreachable!()
    };
    assert_eq!(repos, [0]);
    assert!(result.contains("batch stopped"), "{result}");
    assert_eq!(panel.saved.message, message);
    assert_eq!(
        git(&c.repos[1].path, &["rev-parse", "HEAD"]).unwrap(),
        second_head
    );
    assert_eq!(
        git(&c.repos[0].path, &["show", ":unselected.txt"]).unwrap(),
        "unselected staged marker"
    );
}
