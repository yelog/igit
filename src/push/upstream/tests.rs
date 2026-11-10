use super::*;
use crossterm::event::KeyEvent;
use std::time::{Duration, Instant};

fn g(path: &Path, args: &[&str]) -> String {
    git(path, args).unwrap()
}

fn first_push_repo(parent: &Path, name: &str) -> PathBuf {
    let repo = crate::push::tests::make(parent, name);
    g(&repo, &["switch", "--no-track", "-qc", "topic"]);
    // IGit must preserve the explicit choice even when Git's ambient default
    // would automatically establish tracking for a first push.
    g(&repo, &["config", "push.autoSetupRemote", "true"]);
    repo
}

fn planned(repo: &Path, target: Option<&Target>, tracking: bool) -> Job {
    let c = Context::discover(repo).unwrap();
    let mut j = job(&c.repos[0], &entry(0, &c.repos[0], target)).unwrap();
    j.set_upstream = tracking;
    j
}

fn key(panel: &mut PushPanel, c: &Context, code: KeyCode) -> Action {
    panel.event(&Event::Key(KeyEvent::new(code, KeyModifiers::NONE)), c)
}

fn choose_tracking(panel: &mut PushPanel, c: &Context, idx: usize, target: &str) {
    panel.target(idx);
    let editor = panel.target_edit.as_mut().unwrap();
    editor.text = target.into();
    if !editor.set_upstream {
        key(panel, c, KeyCode::Tab);
    }
    key(panel, c, KeyCode::Enter);
    panel.finish_loading(c);
}

fn wait(panel: &mut PushPanel, c: &Context) -> (NoticeLevel, String) {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        if let Some(message) = panel.poll(c) {
            panel.finish_loading(c);
            return message;
        }
        assert!(Instant::now() < deadline, "Push did not complete");
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn first_push_sets_the_reviewed_remote_and_branch_and_allows_pull() {
    let dir = tempfile::tempdir().unwrap();
    let repo = first_push_repo(dir.path(), "app");
    g(&repo, &["branch", "-m", "Topic.测试"]);
    let bare = dir.path().join("publish.git");
    fs::create_dir(&bare).unwrap();
    g(&bare, &["init", "--bare", "-q", "-b", "main"]);
    g(&repo, &["remote", "add", "publish", bare.to_str().unwrap()]);
    let target = Target {
        remote: "publish".into(),
        dst: "reviewed/topic".into(),
    };
    let j = planned(&repo, Some(&target), true);
    assert_eq!(j.config.description(&j.branch), "none");
    assert_eq!(push_one(&j, false, false).unwrap(), PushOutcome::Complete);
    assert_eq!(
        g(&bare, &["rev-parse", "refs/heads/reviewed/topic"]),
        j.head
    );
    assert_eq!(g(&repo, &["config", "branch.Topic.测试.remote"]), "publish");
    assert_eq!(
        g(&repo, &["config", "branch.Topic.测试.merge"]),
        "refs/heads/reviewed/topic"
    );
    assert_eq!(g(&repo, &["rev-parse", "Topic.测试@{upstream}"]), j.head);
    assert_eq!(g(&repo, &["config", "branch.main.remote"]), "origin");
    assert_eq!(
        g(&repo, &["config", "branch.main.merge"]),
        "refs/heads/main"
    );
    g(&repo, &["pull", "--ff-only"]);
    assert_eq!(g(&repo, &["symbolic-ref", "--short", "HEAD"]), "Topic.测试");
    assert_eq!(g(&repo, &["rev-parse", "HEAD"]), j.head);
}

#[test]
fn first_push_preserves_unconfigured_tracking_by_default_despite_git_auto_setup() {
    let dir = tempfile::tempdir().unwrap();
    let repo = first_push_repo(dir.path(), "app");
    let j = planned(&repo, None, false);
    let before = fs::read(repo.join(".git/config")).unwrap();
    assert_eq!(push_one(&j, false, false).unwrap(), PushOutcome::Complete);
    assert_eq!(fs::read(repo.join(".git/config")).unwrap(), before);
    assert!(git(&repo, &["rev-parse", "topic@{upstream}"]).is_err());
    assert_eq!(
        g(
            &dir.path().join("app.git"),
            &["rev-parse", "refs/heads/topic"]
        ),
        j.head
    );
}

#[test]
fn changed_target_keeps_existing_upstream_unless_explicitly_replaced() {
    let dir = tempfile::tempdir().unwrap();
    let repo = crate::push::tests::make(dir.path(), "app");
    g(
        &repo,
        &["config", "--add", "branch.main.pushRemote", "origin"],
    );
    g(
        &repo,
        &["config", "--add", "branch.main.pushRemote", "origin"],
    );
    g(&repo, &["config", "branch.main.rebase", "true"]);
    g(&repo, &["config", "--add", "custom.keep", "first"]);
    g(&repo, &["config", "--add", "custom.keep", "second"]);
    let include = repo.join(".git/extra-config");
    fs::write(&include, "[custom]\n\tincluded = retained\n").unwrap();
    g(&repo, &["config", "include.path", "extra-config"]);
    let target = Target {
        remote: "origin".into(),
        dst: "release".into(),
    };
    let before = fs::read(repo.join(".git/config")).unwrap();
    let j = planned(&repo, Some(&target), false);
    push_one(&j, false, false).unwrap();
    assert_eq!(fs::read(repo.join(".git/config")).unwrap(), before);
    assert_eq!(
        g(
            &repo,
            &["rev-parse", "--symbolic-full-name", "main@{upstream}"]
        ),
        "refs/remotes/origin/main"
    );

    g(&repo, &["commit", "--allow-empty", "-qm", "more outgoing"]);
    let unrelated = || {
        git_bytes(
            &repo,
            &[
                "config",
                "--null",
                "--get-regexp",
                r"^(custom\.|include\.|branch\.main\.(pushremote|rebase))",
            ],
        )
        .unwrap()
    };
    let preserved = unrelated();
    let j = planned(&repo, Some(&target), true);
    push_one(&j, false, false).unwrap();
    assert_eq!(unrelated(), preserved);
    assert_eq!(
        fs::read(&include).unwrap(),
        b"[custom]\n\tincluded = retained\n"
    );
    assert_eq!(g(&repo, &["config", "branch.main.remote"]), "origin");
    assert_eq!(
        g(&repo, &["config", "branch.main.merge"]),
        "refs/heads/release"
    );
    assert_eq!(
        g(
            &repo,
            &["rev-parse", "--symbolic-full-name", "main@{upstream}"]
        ),
        "refs/remotes/origin/release"
    );
}

#[test]
fn config_changes_including_duplicate_values_invalidate_loaded_and_reviewed_pushes() {
    for (key, value) in [
        ("branch.main.remote", "origin"),
        ("branch.main.merge", "refs/heads/main"),
        ("branch.main.pushRemote", "origin"),
        ("remote.pushDefault", "origin"),
    ] {
        for set_upstream in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let repo = crate::push::tests::make(dir.path(), "app");
            let c = Context::discover(&repo).unwrap();
            let e = entry(0, &c.repos[0], None);
            let mut j = job(&c.repos[0], &e).unwrap();
            j.set_upstream = set_upstream;
            let bare = dir.path().join("app.git");
            let remote = g(&bare, &["rev-parse", "refs/heads/main"]);
            g(&repo, &["config", "--add", key, value]);
            let config = fs::read(repo.join(".git/config")).unwrap();
            assert!(job(&c.repos[0], &e)
                .unwrap_err()
                .to_string()
                .contains("config changed"));
            assert!(push_one(&j, false, false)
                .unwrap_err()
                .to_string()
                .contains("config changed"));
            assert_eq!(g(&bare, &["rev-parse", "refs/heads/main"]), remote);
            assert_eq!(fs::read(repo.join(".git/config")).unwrap(), config);
        }
    }
}

#[test]
fn changed_target_or_tracking_choice_cannot_execute_an_old_review() {
    for change_choice in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let repo = first_push_repo(dir.path(), "app");
        let c = Context::discover(&repo).unwrap();
        let mut panel = PushPanel::default();
        panel.refresh(&c).unwrap();
        panel.finish_loading(&c);
        choose_tracking(&mut panel, &c, 0, "origin:reviewed");
        assert!(
            matches!(panel.review(&c, false), Action::Confirm { ref title, ref body, .. }
            if title == "Push 1 repositories?" && body.contains("origin:reviewed after push succeeds") && body.contains("Upstream: none"))
        );
        if change_choice {
            panel.tracking_choices.clear();
        } else {
            panel.targets.insert(
                0,
                Target {
                    remote: "origin".into(),
                    dst: "changed".into(),
                },
            );
        }
        assert!(
            matches!(panel.confirm("push", &c), Action::Notice(NoticeLevel::Warning, ref text) if text.contains("changed; review again"))
        );
        assert!(!panel.busy());
        assert!(git(
            &dir.path().join("app.git"),
            &["rev-parse", "refs/heads/reviewed"]
        )
        .is_err());
        assert!(git(&repo, &["config", "branch.topic.remote"]).is_err());
    }
}

#[test]
fn target_cancel_and_unconfirmed_review_leave_all_git_state_unchanged() {
    let dir = tempfile::tempdir().unwrap();
    let repo = first_push_repo(dir.path(), "app");
    fs::write(repo.join("a"), "staged\n").unwrap();
    g(&repo, &["add", "a"]);
    fs::write(repo.join("a"), "working\n").unwrap();
    let c = Context::discover(&repo).unwrap();
    let mut panel = PushPanel::default();
    panel.refresh(&c).unwrap();
    panel.finish_loading(&c);
    let snapshot = || {
        (
            fs::read(repo.join(".git/config")).unwrap(),
            g(&repo, &["rev-parse", "HEAD"]),
            git_bytes(&repo, &["ls-files", "--stage", "-z"]).unwrap(),
            fs::read(repo.join("a")).unwrap(),
            g(&dir.path().join("app.git"), &["show-ref"]),
        )
    };
    let before = snapshot();
    key(&mut panel, &c, KeyCode::Char('t'));
    assert!(!panel.target_edit.as_ref().unwrap().set_upstream);
    key(&mut panel, &c, KeyCode::Tab);
    panel.target_edit.as_mut().unwrap().text = "origin:discarded".into();
    key(&mut panel, &c, KeyCode::Esc);
    assert!(panel.targets.is_empty() && panel.tracking_choices.is_empty());
    choose_tracking(&mut panel, &c, 0, "origin:reviewed");
    assert!(matches!(panel.review(&c, false), Action::Confirm { .. }));
    assert!(!panel.busy(), "opening review must not start the push");
    assert_eq!(snapshot(), before);
}

#[cfg(unix)]
fn hook(path: &Path, contents: &str) {
    use std::os::unix::fs::PermissionsExt;
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, contents).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

#[cfg(unix)]
#[test]
fn failed_first_push_keeps_tracking_unconfigured() {
    let dir = tempfile::tempdir().unwrap();
    let repo = first_push_repo(dir.path(), "app");
    let bare = dir.path().join("app.git");
    hook(&bare.join("hooks/pre-receive"), "#!/bin/sh\nexit 1\n");
    let j = planned(&repo, None, true);
    let before = fs::read(repo.join(".git/config")).unwrap();
    assert!(push_one(&j, false, false).is_err());
    assert_eq!(fs::read(repo.join(".git/config")).unwrap(), before);
    assert!(git(&bare, &["rev-parse", "refs/heads/topic"]).is_err());
}

#[cfg(unix)]
#[test]
fn first_push_batch_records_independent_tracking_results_and_retries_only_failure() {
    let dir = tempfile::tempdir().unwrap();
    let a = first_push_repo(dir.path(), "a");
    let b = first_push_repo(dir.path(), "b");
    let reject = dir.path().join("b.git/hooks/pre-receive");
    hook(&reject, "#!/bin/sh\nexit 1\n");
    let c = Context::discover(dir.path()).unwrap();
    let mut panel = PushPanel {
        reseed: Some(vec![0, 1]),
        ..Default::default()
    };
    panel.refresh(&c).unwrap();
    panel.finish_loading(&c);
    choose_tracking(&mut panel, &c, 0, "origin:first");
    choose_tracking(&mut panel, &c, 1, "origin:second");
    assert!(matches!(panel.review(&c, false), Action::Confirm { .. }));
    panel.confirm("push", &c);
    assert_eq!(wait(&mut panel, &c).0, NoticeLevel::Warning);
    assert_eq!(panel.results[&0], Ok(PushOutcome::Complete));
    assert!(panel.results[&1].is_err());
    assert_eq!(g(&a, &["config", "branch.topic.merge"]), "refs/heads/first");
    assert!(git(&b, &["config", "branch.topic.merge"]).is_err());
    let published = g(
        &dir.path().join("a.git"),
        &["rev-parse", "refs/heads/first"],
    );
    g(
        &a,
        &[
            "commit",
            "--allow-empty",
            "-qm",
            "later work is not retry scope",
        ],
    );
    fs::remove_file(reject).unwrap();
    panel.refresh(&c).unwrap();
    panel.finish_loading(&c);
    assert!(panel.entries.iter().all(|e| e.n > 0));
    assert!(matches!(panel.review(&c, true), Action::Confirm { .. }));
    assert_eq!(panel.review.iter().map(|j| j.idx).collect::<Vec<_>>(), [1]);
    panel.confirm("push", &c);
    assert_eq!(wait(&mut panel, &c).0, NoticeLevel::Success);
    assert_eq!(
        g(
            &dir.path().join("a.git"),
            &["rev-parse", "refs/heads/first"]
        ),
        published
    );
    assert_eq!(
        g(&b, &["config", "branch.topic.merge"]),
        "refs/heads/second"
    );
    assert!(panel.results.values().all(Result::is_ok));
}

#[test]
fn upstream_config_lock_failure_retains_published_result_and_is_never_retried() {
    let dir = tempfile::tempdir().unwrap();
    let repo = first_push_repo(dir.path(), "app");
    let c = Context::discover(&repo).unwrap();
    let mut panel = PushPanel::default();
    panel.refresh(&c).unwrap();
    panel.finish_loading(&c);
    choose_tracking(&mut panel, &c, 0, "origin:reviewed");
    assert!(matches!(panel.review(&c, false), Action::Confirm { .. }));
    let pushed = panel.review[0].head.clone();
    let before = fs::read(repo.join(".git/config")).unwrap();
    let lock = repo.join(".git/config.lock");
    fs::write(&lock, "another writer's lock\n").unwrap();
    panel.confirm("push", &c);
    let (level, message) = wait(&mut panel, &c);
    assert_eq!(level, NoticeLevel::Warning);
    assert!(message.contains("1 upstream setup failed"), "{message}");
    assert!(
        matches!(&panel.results[&0], Ok(PushOutcome::UpstreamNotSet(text)) if text.contains("origin:reviewed") && text.contains("Retry will not repeat"))
    );
    assert_eq!(
        g(
            &dir.path().join("app.git"),
            &["rev-parse", "refs/heads/reviewed"]
        ),
        pushed
    );
    assert_eq!(fs::read(repo.join(".git/config")).unwrap(), before);
    assert_eq!(fs::read(&lock).unwrap(), b"another writer's lock\n");
    assert!(panel
        .preview
        .summary
        .contains("PUSH SUCCEEDED; UPSTREAM NOT SET"));
    assert!(panel.preview.summary.contains("origin:reviewed"));
    assert_eq!(
        panel.entries[0].n, 0,
        "the warning must remain inspectable without outgoing work"
    );
    g(&repo, &["commit", "--allow-empty", "-qm", "later work"]);
    panel.refresh(&c).unwrap();
    panel.finish_loading(&c);
    assert!(panel.entries[0].n > 0);
    assert!(matches!(
        panel.review(&c, true),
        Action::Notice(NoticeLevel::Warning, _)
    ));
    assert!(!panel.busy());
}

#[cfg(unix)]
#[test]
fn tracking_changed_during_successful_push_is_preserved_as_a_partial_outcome() {
    let dir = tempfile::tempdir().unwrap();
    let repo = first_push_repo(dir.path(), "app");
    g(&repo, &["config", "core.hooksPath", ".git/hooks"]);
    hook(
        &repo.join(".git/hooks/pre-push"),
        "#!/bin/sh\ngit config branch.topic.remote changed-during-push\n",
    );
    let j = planned(&repo, None, true);
    let result = push_one(&j, false, false).unwrap();
    assert!(
        matches!(result, PushOutcome::UpstreamNotSet(ref text) if text.contains("config changed"))
    );
    assert_eq!(
        g(
            &dir.path().join("app.git"),
            &["rev-parse", "refs/heads/topic"]
        ),
        j.head
    );
    assert_eq!(
        g(&repo, &["config", "branch.topic.remote"]),
        "changed-during-push"
    );
    assert!(git(&repo, &["config", "branch.topic.merge"]).is_err());
}

#[cfg(unix)]
#[test]
fn unrelated_config_written_during_push_survives_upstream_publication() {
    let dir = tempfile::tempdir().unwrap();
    let repo = first_push_repo(dir.path(), "app");
    g(&repo, &["config", "custom.keep", "original"]);
    g(&repo, &["config", "core.hooksPath", ".git/hooks"]);
    hook(
        &repo.join(".git/hooks/pre-push"),
        "#!/bin/sh\ngit config --add custom.keep added-during-push\n",
    );
    let j = planned(&repo, None, true);
    assert_eq!(push_one(&j, false, false).unwrap(), PushOutcome::Complete);
    assert_eq!(
        g(&repo, &["config", "--get-all", "custom.keep"]),
        "original\nadded-during-push"
    );
    assert_eq!(
        g(&repo, &["config", "branch.topic.merge"]),
        "refs/heads/topic"
    );
}

#[test]
fn inherited_merge_config_is_preserved_and_explicit_replacement_requires_git() {
    let dir = tempfile::tempdir().unwrap();
    let repo = crate::push::tests::make(dir.path(), "app");
    g(&repo, &["config", "--unset-all", "branch.main.merge"]);
    let include = repo.join(".git/tracking-config");
    fs::write(&include, "[branch \"main\"]\n\tmerge = refs/heads/main\n").unwrap();
    g(&repo, &["config", "include.path", "tracking-config"]);
    let target = Target {
        remote: "origin".into(),
        dst: "reviewed".into(),
    };
    let j = planned(&repo, Some(&target), true);
    assert!(push_one(&j, false, false)
        .unwrap_err()
        .to_string()
        .contains("inherited"));
    assert!(git(
        &dir.path().join("app.git"),
        &["rev-parse", "refs/heads/reviewed"]
    )
    .is_err());
    assert_eq!(
        g(&repo, &["config", "--get-all", "branch.main.merge"]),
        "refs/heads/main"
    );
    let j = planned(&repo, Some(&target), false);
    assert_eq!(push_one(&j, false, false).unwrap(), PushOutcome::Complete);
    assert_eq!(
        fs::read(&include).unwrap(),
        b"[branch \"main\"]\n\tmerge = refs/heads/main\n"
    );
}

#[test]
fn linked_worktree_first_push_uses_worktree_config_when_enabled() {
    let dir = tempfile::tempdir().unwrap();
    let repo = crate::push::tests::make(dir.path(), "app");
    let worktree = dir.path().join("linked");
    g(&repo, &["config", "extensions.worktreeConfig", "true"]);
    g(
        &repo,
        &[
            "worktree",
            "add",
            "--no-track",
            "-qb",
            "topic",
            worktree.to_str().unwrap(),
        ],
    );
    let before = fs::read(repo.join(".git/config")).unwrap();
    let j = planned(&worktree, None, true);
    assert_eq!(push_one(&j, false, false).unwrap(), PushOutcome::Complete);
    assert_eq!(fs::read(repo.join(".git/config")).unwrap(), before);
    assert_eq!(
        g(&worktree, &["config", "--worktree", "branch.topic.remote"]),
        "origin"
    );
    assert_eq!(
        g(&worktree, &["config", "--worktree", "branch.topic.merge"]),
        "refs/heads/topic"
    );
    assert_eq!(g(&worktree, &["rev-parse", "topic@{upstream}"]), j.head);
}

#[test]
fn mouse_can_save_each_repositorys_tracking_choice_without_writing_git_config() {
    use crossterm::event::MouseEvent;
    use ratatui::{backend::TestBackend, Terminal};
    let dir = tempfile::tempdir().unwrap();
    let repo = first_push_repo(dir.path(), "app");
    let c = Context::discover(&repo).unwrap();
    let before = fs::read(repo.join(".git/config")).unwrap();
    for icons in [IconMode::Ascii, IconMode::Unicode, IconMode::NerdFont] {
        for (width, height) in [(44, 12), (80, 24), (120, 36)] {
            let mut panel = PushPanel {
                icons,
                ..Default::default()
            };
            panel.refresh(&c).unwrap();
            panel.finish_loading(&c);
            let mut term = Terminal::new(TestBackend::new(width, height)).unwrap();
            let draw = |panel: &mut PushPanel, term: &mut Terminal<TestBackend>| {
                term.draw(|f| panel.draw(f, f.area(), &c)).unwrap();
            };
            let click = |panel: &mut PushPanel, predicate: fn(&Hit) -> bool| {
                let rect = panel
                    .hits
                    .iter()
                    .rev()
                    .find_map(|(r, h)| predicate(h).then_some(*r))
                    .unwrap();
                panel.event(
                    &Event::Mouse(MouseEvent {
                        kind: MouseEventKind::Down(MouseButton::Left),
                        column: rect.right() - 1,
                        row: rect.y,
                        modifiers: KeyModifiers::NONE,
                    }),
                    &c,
                )
            };
            draw(&mut panel, &mut term);
            click(&mut panel, |h| matches!(h, Hit::EditTarget));
            draw(&mut panel, &mut term);
            click(&mut panel, |h| matches!(h, Hit::Upstream));
            assert!(panel.target_edit.as_ref().unwrap().set_upstream);
            assert!(matches!(
                click(&mut panel, |h| matches!(h, Hit::SaveTarget)),
                Action::Notice(NoticeLevel::Success, _)
            ));
            panel.finish_loading(&c);
            assert!(panel.sets_upstream(&panel.entries[0]));
            assert!(
                matches!(panel.review(&c, false), Action::Confirm { ref body, .. } if body.contains("origin:topic after push succeeds"))
            );
            assert_eq!(fs::read(repo.join(".git/config")).unwrap(), before);
            assert!(git(
                &dir.path().join("app.git"),
                &["rev-parse", "refs/heads/topic"]
            )
            .is_err());
        }
    }
}

#[test]
fn tracking_choice_cannot_follow_an_external_source_branch_switch() {
    let dir = tempfile::tempdir().unwrap();
    let repo = first_push_repo(dir.path(), "app");
    let c = Context::discover(&repo).unwrap();
    let mut panel = PushPanel::default();
    panel.refresh(&c).unwrap();
    panel.finish_loading(&c);
    choose_tracking(&mut panel, &c, 0, "origin:reviewed");
    assert!(matches!(panel.review(&c, false), Action::Confirm { .. }));
    let j = panel.review[0].clone();
    g(&repo, &["switch", "--no-track", "-qc", "other-source"]);
    assert!(push_one(&j, false, false).is_err());
    panel.refresh(&c).unwrap();
    panel.finish_loading(&c);
    assert!(panel.tracking_choices.is_empty());
    assert!(
        matches!(panel.review(&c, false), Action::Confirm { ref body, .. } if body.contains("other-source") && body.contains("keep unchanged"))
    );
    assert!(git(
        &dir.path().join("app.git"),
        &["rev-parse", "refs/heads/reviewed"]
    )
    .is_err());
}

#[test]
fn failed_private_config_edit_cannot_publish_half_an_upstream_or_remove_foreign_locks() {
    let dir = tempfile::tempdir().unwrap();
    let repo = first_push_repo(dir.path(), "app");
    let before = fs::read(repo.join(".git/config")).unwrap();
    let nested_lock = repo.join(".git/config.lock.lock");
    fs::write(&nested_lock, "foreign nested lock\n").unwrap();
    let j = planned(&repo, None, true);
    assert!(matches!(
        push_one(&j, false, false).unwrap(),
        PushOutcome::UpstreamNotSet(_)
    ));
    assert_eq!(fs::read(repo.join(".git/config")).unwrap(), before);
    assert!(!repo.join(".git/config.lock").exists());
    assert_eq!(fs::read(&nested_lock).unwrap(), b"foreign nested lock\n");
    assert_eq!(
        g(
            &dir.path().join("app.git"),
            &["rev-parse", "refs/heads/topic"]
        ),
        j.head
    );
}

#[test]
fn valueless_worktree_config_flag_remains_supported_and_fingerprinted() {
    let dir = tempfile::tempdir().unwrap();
    let repo = first_push_repo(dir.path(), "app");
    let config = repo.join(".git/config");
    fs::OpenOptions::new()
        .append(true)
        .open(&config)
        .unwrap()
        .write_all(b"\n[extensions]\n\tworktreeConfig\n")
        .unwrap();
    let j = planned(&repo, None, true);
    assert_eq!(push_one(&j, false, false).unwrap(), PushOutcome::Complete);
    assert_eq!(
        g(&repo, &["config", "--worktree", "branch.topic.merge"]),
        "refs/heads/topic"
    );
    assert_eq!(g(&repo, &["rev-parse", "topic@{upstream}"]), j.head);
    let before = Snapshot::read(&repo, "topic").unwrap();
    g(
        &repo,
        &["config", "--local", "extensions.worktreeConfig", ""],
    );
    assert_ne!(Snapshot::read(&repo, "topic").unwrap(), before);
}
