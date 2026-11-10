use super::*;
use std::fs;

fn g(repo: &Path, args: &[&str]) -> String {
    git(repo, args).unwrap()
}

fn fixture(conflict: bool) -> (tempfile::TempDir, Context, Job, PathBuf, String) {
    let directory = tempfile::tempdir().unwrap();
    let repo = crate::push::tests::make(directory.path(), "app");
    let writer = directory.path().join("writer");
    g(
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
        g(&writer, &["config", key, value]);
    }
    fs::write(
        writer.join(if conflict { "a" } else { "remote.txt" }),
        "remote first\n",
    )
    .unwrap();
    g(&writer, &["add", "."]);
    g(&writer, &["commit", "-qm", "remote first"]);
    fs::write(writer.join("second.txt"), "remote second\n").unwrap();
    g(&writer, &["add", "."]);
    g(&writer, &["commit", "-qm", "remote second"]);
    g(&writer, &["push", "-q", "origin", "main"]);
    let remote = g(&writer, &["rev-parse", "HEAD"]);
    let context = Context::discover(&repo).unwrap();
    let planned = job(&context.repos[0], &entry(0, &context.repos[0], None)).unwrap();
    (directory, context, planned, writer, remote)
}

#[test]
fn diverged_target_updates_then_pushes_with_each_strategy() {
    for strategy in [Strategy::Merge, Strategy::Rebase] {
        let (directory, context, planned, _, remote) = fixture(false);
        let repo = &context.repos[0].path;
        let mut phases = Vec::new();
        assert_eq!(
            execute(&planned, strategy, false, false, |phase| phases.push(phase)).unwrap(),
            PushOutcome::Updated(strategy)
        );
        let head = g(repo, &["rev-parse", "HEAD"]);
        assert_eq!(
            g(&directory.path().join("app.git"), &["rev-parse", "main"]),
            head
        );
        assert!(is_ancestor(repo, &remote, &head).unwrap());
        assert_eq!(fs::read_to_string(repo.join("a")).unwrap(), "next");
        assert_eq!(
            fs::read_to_string(repo.join("second.txt")).unwrap(),
            "remote second\n"
        );
        let parents = g(repo, &["show", "-s", "--format=%P", "HEAD"]);
        if strategy == Strategy::Merge {
            assert_eq!(parents, format!("{} {remote}", planned.head));
        } else {
            assert_eq!(parents, remote);
        }
        assert_eq!(phases.first(), Some(&"Fetching…"));
        assert_eq!(phases.last(), Some(&"Pushing…"));
        assert!(g(repo, &["status", "--porcelain"]).is_empty());
    }
}

#[cfg(unix)]
#[test]
fn server_message_policy_rejection_preserves_the_merge_and_retry_does_not_merge_again() {
    use std::os::unix::fs::PermissionsExt;

    let (directory, context, _, _, remote) = fixture(false);
    let repo = &context.repos[0].path;
    g(
        repo,
        &["commit", "--amend", "-qm", "feat(TEST-42): local work"],
    );
    let bare = directory.path().join("app.git");
    let hooks = bare.join("policy-hooks");
    fs::create_dir(&hooks).unwrap();
    let hook = hooks.join("pre-receive");
    fs::write(
        &hook,
        "#!/bin/sh\nread old new ref\ngit log -1 --format=%s \"$new\" > rejected-subject\ncase \"$(cat rejected-subject)\" in\n  'feat(TEST-42):'*) exit 0 ;;\nesac\necho '[01-commit-message-check] executed' >&2\necho 'GL-HOOK-ERR: Commit blocked: missing issueKey.' >&2\nexit 1\n",
    )
    .unwrap();
    fs::set_permissions(hook, fs::Permissions::from_mode(0o755)).unwrap();
    g(
        &bare,
        &["config", "core.hooksPath", hooks.to_str().unwrap()],
    );
    let planned = job(&context.repos[0], &entry(0, &context.repos[0], None)).unwrap();
    let error = execute(&planned, Strategy::Merge, false, false, |_| {})
        .unwrap_err()
        .to_string();
    assert!(
        error.starts_with("Updated successfully; push failed:"),
        "{error}"
    );
    assert!(
        error.contains("Commit blocked: missing issueKey."),
        "{error}"
    );
    let merged = g(repo, &["rev-parse", "HEAD"]);
    assert_eq!(
        g(repo, &["show", "-s", "--format=%P", "HEAD"]),
        format!("{} {remote}", planned.head)
    );
    assert_eq!(
        fs::read_to_string(bare.join("rejected-subject"))
            .unwrap()
            .trim(),
        g(repo, &["log", "-1", "--format=%s"])
    );
    assert_eq!(g(&bare, &["rev-parse", "main"]), remote);
    assert!(g(repo, &["status", "--porcelain"]).is_empty());
    let retry = job(&context.repos[0], &entry(0, &context.repos[0], None)).unwrap();
    let mut phases = Vec::new();
    let error = execute(&retry, Strategy::Merge, false, true, |phase| {
        phases.push(phase)
    })
    .unwrap_err()
    .to_string();
    assert!(error.starts_with("Push failed:"), "{error}");
    assert!(
        error.contains("Commit blocked: missing issueKey."),
        "{error}"
    );
    assert_eq!(phases, ["Fetching…", "Pushing…"]);
    assert_eq!(g(repo, &["rev-parse", "HEAD"]), merged);
    assert_eq!(g(&bare, &["rev-parse", "main"]), remote);
}

#[test]
fn conflicts_preserve_native_operation_and_never_push() {
    for strategy in [Strategy::Merge, Strategy::Rebase] {
        let (directory, context, planned, _, remote) = fixture(true);
        let error = execute(&planned, strategy, false, false, |_| {})
            .unwrap_err()
            .to_string();
        assert!(error.contains("Update stopped: conflicts"), "{error}");
        assert!(error.contains("Nothing pushed"), "{error}");
        assert_eq!(
            g(&directory.path().join("app.git"), &["rev-parse", "main"]),
            remote
        );
        assert!(crate::core::update::conflicts(&context.repos[0].path));
        assert!(ready(&context.repos[0].path).is_err());
        assert!(execute(&planned, strategy, false, false, |_| {}).is_err());
    }
}

#[test]
fn updating_preserves_dirty_index_and_working_tree_without_autostash() {
    for staged in [false, true] {
        let (directory, context, planned, _, remote) = fixture(false);
        let repo = &context.repos[0].path;
        fs::write(repo.join("a"), "staged local\n").unwrap();
        if staged {
            g(repo, &["add", "a"]);
            fs::write(repo.join("a"), "unstaged local\n").unwrap();
        }
        let before = (
            g(repo, &["diff", "--cached"]),
            g(repo, &["diff"]),
            fs::read(repo.join(".git/index")).unwrap(),
        );
        let error = execute(&planned, Strategy::Rebase, false, false, |_| {})
            .unwrap_err()
            .to_string();
        assert!(error.contains("clean index"), "{error}");
        assert_eq!(g(repo, &["rev-parse", "HEAD"]), planned.head);
        assert_eq!(
            g(&directory.path().join("app.git"), &["rev-parse", "main"]),
            remote
        );
        assert_eq!(g(repo, &["diff", "--cached"]), before.0);
        assert_eq!(g(repo, &["diff"]), before.1);
        assert_eq!(fs::read(repo.join(".git/index")).unwrap(), before.2);
        assert!(g(repo, &["stash", "list"]).is_empty());
    }
}

#[test]
fn ahead_only_push_keeps_dirty_content_and_skips_integration() {
    let directory = tempfile::tempdir().unwrap();
    let repo = crate::push::tests::make(directory.path(), "app");
    let context = Context::discover(&repo).unwrap();
    let planned = job(&context.repos[0], &entry(0, &context.repos[0], None)).unwrap();
    fs::write(repo.join("a"), "uncommitted").unwrap();
    let mut phases = Vec::new();
    assert_eq!(
        execute(&planned, Strategy::Rebase, false, false, |phase| phases
            .push(phase))
        .unwrap(),
        PushOutcome::Complete
    );
    assert_eq!(fs::read_to_string(repo.join("a")).unwrap(), "uncommitted");
    assert_eq!(g(&repo, &["rev-parse", "HEAD"]), planned.head);
    assert_eq!(phases, ["Fetching…", "Pushing…"]);
}

#[test]
fn target_comparison_reports_both_counts_and_unknown_new_targets() {
    let (_directory, context, _, _, _) = fixture(false);
    let repo = &context.repos[0];
    g(&repo.path, &["fetch", "-q", "origin"]);
    let compared = entry(0, repo, None);
    assert_eq!((compared.n, compared.behind), (1, Some(2)));
    let new = entry(
        0,
        repo,
        Some(&Target {
            remote: "origin".into(),
            dst: "new-target".into(),
        }),
    );
    assert!(new.n > 0);
    assert_eq!(new.behind, None);
}

#[test]
fn remote_race_after_update_stops_without_repeating_the_update() {
    let (directory, context, planned, writer, _) = fixture(false);
    let error = execute(&planned, Strategy::Rebase, false, false, |phase| {
        if phase == "Pushing…" {
            g(&writer, &["commit", "--allow-empty", "-qm", "remote race"]);
            g(&writer, &["push", "-q", "origin", "main"]);
        }
    })
    .unwrap_err()
    .to_string();
    assert!(
        error.contains("Updated successfully; push failed"),
        "{error}"
    );
    assert!(error.contains("remote tip differs"), "{error}");
    let remote = g(&writer, &["rev-parse", "HEAD"]);
    assert_eq!(
        g(&directory.path().join("app.git"), &["rev-parse", "main"]),
        remote
    );
    let repo = &context.repos[0].path;
    assert!(!is_ancestor(repo, &remote, &g(repo, &["rev-parse", "HEAD"])).unwrap_or(false));
    assert!(ready(repo).is_ok());
}

#[test]
fn source_changed_during_fetch_cannot_be_updated_or_published() {
    let (directory, context, planned, _, remote) = fixture(false);
    let repo = &context.repos[0].path;
    let error = execute(&planned, Strategy::Merge, false, false, |phase| {
        if phase == "Fetching…" {
            g(repo, &["commit", "--allow-empty", "-qm", "external commit"]);
        }
    })
    .unwrap_err()
    .to_string();
    assert!(error.contains("changed"), "{error}");
    assert_eq!(
        g(&directory.path().join("app.git"), &["rev-parse", "main"]),
        remote
    );
    assert!(ready(repo).is_ok());
}

#[test]
fn custom_target_is_integrated_instead_of_the_configured_upstream() {
    let (directory, context, _, writer, remote) = fixture(false);
    g(
        &writer,
        &["push", "-q", "origin", "HEAD:refs/heads/reviewed"],
    );
    let target = Target {
        remote: "origin".into(),
        dst: "reviewed".into(),
    };
    let planned = job(
        &context.repos[0],
        &entry(0, &context.repos[0], Some(&target)),
    )
    .unwrap();
    execute(&planned, Strategy::Rebase, false, false, |_| {}).unwrap();
    let bare = directory.path().join("app.git");
    assert_eq!(g(&bare, &["rev-parse", "main"]), remote);
    assert_eq!(
        g(&bare, &["rev-parse", "reviewed"]),
        g(&context.repos[0].path, &["rev-parse", "HEAD"])
    );
    assert_eq!(
        g(&context.repos[0].path, &["config", "branch.main.merge"]),
        "refs/heads/main"
    );
}

#[test]
fn changed_strategy_invalidates_the_review_without_git_writes() {
    let (_directory, context, _, _, remote) = fixture(false);
    let mut panel = PushPanel::default();
    panel.refresh(&context).unwrap();
    panel.finish_loading(&context);
    assert!(matches!(
        panel.review(&context, false),
        Action::Confirm { .. }
    ));
    panel.set_strategy(Strategy::Rebase);
    assert!(panel.review.is_empty());
    assert!(matches!(
        panel.confirm("push", &context),
        Action::Notice(NoticeLevel::Warning, _)
    ));
    assert_eq!(
        g(
            &context.repos[0].path,
            &["ls-remote", "--refs", "origin", "refs/heads/main"]
        )
        .split_whitespace()
        .next(),
        Some(remote.as_str())
    );
}

#[test]
fn local_ignored_untracked_and_index_flagged_files_cannot_be_overwritten() {
    for strategy in [Strategy::Merge, Strategy::Rebase] {
        for kind in ["ignored", "untracked", "skip-worktree", "assume-unchanged"] {
            let (directory, context, planned, _, remote) = fixture(false);
            let repo = &context.repos[0].path;
            let path = if kind == "ignored" || kind == "untracked" {
                "remote.txt"
            } else {
                "a"
            };
            fs::write(repo.join(path), "private local content").unwrap();
            match kind {
                "ignored" => fs::write(repo.join(".git/info/exclude"), "remote.txt\n").unwrap(),
                "skip-worktree" => {
                    g(repo, &["update-index", "--skip-worktree", "a"]);
                }
                "assume-unchanged" => {
                    g(repo, &["update-index", "--assume-unchanged", "a"]);
                }
                _ => (),
            }
            let error = execute(&planned, strategy, false, false, |_| {})
                .unwrap_err()
                .to_string();
            assert!(error.contains("may be overwritten"), "{kind}: {error}");
            assert_eq!(
                fs::read_to_string(repo.join(path)).unwrap(),
                "private local content"
            );
            assert_eq!(g(repo, &["rev-parse", "HEAD"]), planned.head);
            assert_eq!(
                g(&directory.path().join("app.git"), &["rev-parse", "main"]),
                remote
            );
        }
    }
}

#[test]
fn a_distinct_push_endpoint_is_updated_without_overwriting_fetch_tracking() {
    let (directory, context, _, writer, remote) = fixture(false);
    let repo = &context.repos[0].path;
    let fetch_remote = directory.path().join("fetch-only.git");
    g(
        directory.path(),
        &[
            "clone",
            "--bare",
            "-q",
            directory.path().join("app.git").to_str().unwrap(),
            fetch_remote.to_str().unwrap(),
        ],
    );
    g(
        repo,
        &[
            "remote",
            "set-url",
            "origin",
            fetch_remote.to_str().unwrap(),
        ],
    );
    g(
        repo,
        &[
            "remote",
            "set-url",
            "--push",
            "origin",
            directory.path().join("app.git").to_str().unwrap(),
        ],
    );
    let tracking = remote_head(
        repo,
        &Target {
            remote: "origin".into(),
            dst: "main".into(),
        },
    )
    .unwrap();
    let planned = job(&context.repos[0], &entry(0, &context.repos[0], None)).unwrap();
    let mut checked_fetch = false;
    execute(&planned, Strategy::Rebase, false, false, |phase| {
        if phase == "Rebasing…" {
            assert_eq!(
                g(repo, &["rev-parse", "refs/remotes/origin/main"]),
                tracking
            );
            checked_fetch = true;
        }
    })
    .unwrap();
    assert!(checked_fetch);
    assert_eq!(g(&fetch_remote, &["rev-parse", "main"]), remote);
    assert_eq!(g(&writer, &["rev-parse", "HEAD"]), remote);
    assert_eq!(
        g(&directory.path().join("app.git"), &["rev-parse", "main"]),
        g(repo, &["rev-parse", "HEAD"])
    );
}

#[test]
fn rebasing_preserves_local_merge_topology() {
    let (directory, context, _, _, remote) = fixture(false);
    let repo = &context.repos[0].path;
    g(repo, &["switch", "-qc", "side"]);
    fs::write(repo.join("side.txt"), "side").unwrap();
    g(repo, &["add", "side.txt"]);
    g(repo, &["commit", "-qm", "side"]);
    g(repo, &["switch", "-q", "main"]);
    fs::write(repo.join("main.txt"), "main").unwrap();
    g(repo, &["add", "main.txt"]);
    g(repo, &["commit", "-qm", "main"]);
    g(repo, &["merge", "--no-ff", "-qm", "merge side", "side"]);
    let side = g(repo, &["rev-parse", "side"]);
    let planned = job(&context.repos[0], &entry(0, &context.repos[0], None)).unwrap();
    execute(&planned, Strategy::Rebase, false, false, |_| {}).unwrap();
    assert_eq!(
        g(
            repo,
            &[
                "rev-list",
                "--count",
                "--merges",
                &format!("{remote}..HEAD")
            ]
        ),
        "1"
    );
    assert_eq!(g(repo, &["rev-parse", "side"]), side);
    assert_eq!(
        g(&directory.path().join("app.git"), &["rev-parse", "main"]),
        g(repo, &["rev-parse", "HEAD"])
    );
}

#[test]
fn integration_rechecks_the_exact_reviewed_source_instead_of_adopting_a_new_one() {
    for strategy in [Strategy::Merge, Strategy::Rebase] {
        for switched in [false, true] {
            let (_directory, context, planned, _, remote) = fixture(false);
            let repo = &context.repos[0].path;
            g(repo, &["fetch", "-q", "origin"]);
            if switched {
                g(repo, &["switch", "-qc", "external"]);
            } else {
                g(repo, &["commit", "--allow-empty", "-qm", "external"]);
            }
            let changed = g(repo, &["rev-parse", "HEAD"]);
            let error =
                integrate_reviewed(repo, &remote, strategy, "refs/heads/main", &planned.head)
                    .unwrap_err()
                    .to_string();
            assert!(error.contains("Branch or HEAD changed"), "{error}");
            assert_eq!(g(repo, &["rev-parse", "HEAD"]), changed);
            assert!(ready(repo).is_ok());
        }
    }
}

#[test]
fn an_already_published_target_still_applies_the_reviewed_upstream_choice() {
    let directory = tempfile::tempdir().unwrap();
    let repo = crate::push::tests::make(directory.path(), "app");
    g(&repo, &["push", "-q", "origin", "HEAD:refs/heads/reviewed"]);
    g(&repo, &["branch", "--unset-upstream"]);
    let context = Context::discover(&repo).unwrap();
    let target = Target {
        remote: "origin".into(),
        dst: "reviewed".into(),
    };
    let mut planned = job(
        &context.repos[0],
        &entry(0, &context.repos[0], Some(&target)),
    )
    .unwrap();
    planned.set_upstream = true;
    assert_eq!(
        execute(&planned, Strategy::Merge, false, false, |_| {}).unwrap(),
        PushOutcome::Complete
    );
    assert_eq!(
        g(&repo, &["config", "branch.main.merge"]),
        "refs/heads/reviewed"
    );
    assert_eq!(g(&repo, &["config", "branch.main.remote"]), "origin");
}

#[test]
fn advertised_targets_match_the_full_ref_instead_of_a_suffix() {
    let (directory, context, planned, writer, remote) = fixture(false);
    g(&writer, &["commit", "--allow-empty", "-qm", "other target"]);
    let other = g(&writer, &["rev-parse", "HEAD"]);
    let other_ref = "refs/heads/a/refs/heads/main";
    g(
        &writer,
        &["push", "-q", "origin", &format!("HEAD:{other_ref}")],
    );
    assert_eq!(
        advertised_tip(&planned.path, &planned.url, "refs/heads/main").unwrap(),
        Some(remote.clone())
    );
    execute(&planned, Strategy::Merge, false, false, |_| {}).unwrap();
    let repo = &context.repos[0].path;
    let head = g(repo, &["rev-parse", "HEAD"]);
    assert!(is_ancestor(repo, &remote, &head).unwrap());
    assert!(!is_ancestor(repo, &other, &head).unwrap_or(false));
    let bare = directory.path().join("app.git");
    assert_eq!(g(&bare, &["rev-parse", "main"]), head);
    assert_eq!(g(&bare, &["rev-parse", other_ref]), other);
}

#[test]
fn chained_url_rewrites_are_rejected_before_fetch_or_integration() {
    let (directory, context, planned, _, remote) = fixture(false);
    let repo = &context.repos[0].path;
    let destination = directory.path().join("other.git");
    let original = directory.path().join("app.git");
    g(
        repo,
        &["remote", "set-url", "origin", "reviewed-endpoint:app"],
    );
    g(
        repo,
        &[
            "config",
            &format!("url.{}.insteadOf", original.display()),
            "reviewed-endpoint:app",
        ],
    );
    g(
        repo,
        &[
            "config",
            &format!("url.{}.insteadOf", destination.display()),
            original.to_str().unwrap(),
        ],
    );
    let error = job(&context.repos[0], &entry(0, &context.repos[0], None))
        .unwrap_err()
        .to_string();
    assert!(error.contains("different endpoint"), "{error}");
    assert_eq!(g(repo, &["rev-parse", "HEAD"]), planned.head);
    assert_eq!(g(&original, &["rev-parse", "main"]), remote);
    assert!(!destination.exists());
}

#[test]
fn replacement_refs_cannot_hide_local_ignored_collisions_from_rebase() {
    let (directory, context, planned, _, remote) = fixture(false);
    let repo = &context.repos[0].path;
    g(repo, &["fetch", "-q", "origin"]);
    let replacement = g(
        repo,
        &[
            "commit-tree",
            &format!("{}^{{tree}}", planned.head),
            "-p",
            planned.tracking.as_deref().unwrap(),
            "-m",
            "replacement hides incoming files",
        ],
    );
    g(repo, &["replace", &remote, &replacement]);
    fs::write(repo.join(".git/info/exclude"), "remote.txt\n").unwrap();
    fs::write(repo.join("remote.txt"), "private ignored content").unwrap();
    let error = execute(&planned, Strategy::Rebase, false, false, |_| {})
        .unwrap_err()
        .to_string();
    assert!(error.contains("may be overwritten"), "{error}");
    assert_eq!(
        fs::read_to_string(repo.join("remote.txt")).unwrap(),
        "private ignored content"
    );
    assert_eq!(g(repo, &["rev-parse", "HEAD"]), planned.head);
    assert_eq!(
        g(&directory.path().join("app.git"), &["rev-parse", "main"]),
        remote
    );
}

#[cfg(unix)]
#[test]
fn post_merge_commits_cannot_expand_the_automatically_published_history() {
    use std::os::unix::fs::PermissionsExt;

    let (directory, context, _, _, remote) = fixture(false);
    let repo = &context.repos[0].path;
    let hooks = repo.join(".git/reviewed-hooks");
    fs::create_dir(&hooks).unwrap();
    let hook = hooks.join("post-merge");
    fs::write(
        &hook,
        "#!/bin/sh\ngit -c core.hooksPath=/dev/null commit --allow-empty -qm 'hook extra commit'\n",
    )
    .unwrap();
    fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).unwrap();
    g(repo, &["config", "core.hooksPath", hooks.to_str().unwrap()]);
    let planned = job(&context.repos[0], &entry(0, &context.repos[0], None)).unwrap();
    let error = execute(&planned, Strategy::Merge, false, false, |_| {})
        .unwrap_err()
        .to_string();
    assert!(error.contains("Merge result changed outside"), "{error}");
    assert_eq!(
        g(&directory.path().join("app.git"), &["rev-parse", "main"]),
        remote
    );
    assert_eq!(g(repo, &["log", "-1", "--format=%s"]), "hook extra commit");
}
