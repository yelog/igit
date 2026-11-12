use super::*;
use crate::{branch::operations::checkout::Checkout, core::git_bytes};

fn blocked(c: &Context, name: &str) -> Checkout {
    let review = review(c, name, Operation::Checkout, "");
    let error = review.execute(c).unwrap_err();
    Checkout::prepare(review, c, error).unwrap()
}

fn diverged(c: &Context) {
    let repo = &c.repos[0].path;
    run(repo, &["switch", "-c", "topic"]);
    fs::write(repo.join("file"), "topic\n").unwrap();
    run(repo, &["commit", "-am", "topic"]);
    run(repo, &["switch", "main"]);
    fs::write(repo.join("file"), "local\n").unwrap();
}

#[test]
fn smart_checkout_preserves_mixed_staging_untracked_files_and_existing_stashes() {
    let (_temp, c, _) = fixture();
    let repo = &c.repos[0].path;
    let lines: Vec<_> = (0..60).map(|number| format!("line {number}\n")).collect();
    fs::write(repo.join("file"), lines.concat()).unwrap();
    fs::write(repo.join("sibling"), "base sibling\n").unwrap();
    run(repo, &["add", "."]);
    run(repo, &["commit", "-qm", "long base"]);
    fs::write(repo.join("sibling"), "older stash\n").unwrap();
    run(repo, &["stash", "push", "-m", "keep existing"]);
    let previous_stashes = git_bytes(repo, &["stash", "list", "--format=%H %gs"]).unwrap();
    run(repo, &["switch", "-c", "topic"]);
    let mut target = lines.clone();
    target[0] = "branch edit\n".into();
    fs::write(repo.join("file"), target.concat()).unwrap();
    run(repo, &["commit", "-am", "target"]);
    run(repo, &["switch", "main"]);
    let mut local = lines;
    local[20] = "staged edit\n".into();
    fs::write(repo.join("file"), local.concat()).unwrap();
    fs::write(repo.join("sibling"), "unselected staged sibling\n").unwrap();
    run(repo, &["add", "."]);
    local[40] = "unstaged edit\n".into();
    fs::write(repo.join("file"), local.concat()).unwrap();
    fs::write(repo.join("new 中文.txt"), "untracked\n").unwrap();
    let other = state(&c.repos[1].path);
    let (level, message) = blocked(&c, "refs/heads/topic").execute(&c, false);
    assert_eq!(level, NoticeLevel::Success, "{message}");
    assert_eq!(run(repo, &["branch", "--show-current"]), "topic");
    target[20] = "staged edit\n".into();
    assert_eq!(run(repo, &["show", ":file"]), target.concat().trim_end());
    target[40] = "unstaged edit\n".into();
    assert_eq!(
        fs::read_to_string(repo.join("file")).unwrap(),
        target.concat()
    );
    assert_eq!(
        run(repo, &["show", ":sibling"]),
        "unselected staged sibling"
    );
    assert_eq!(
        fs::read_to_string(repo.join("new 中文.txt")).unwrap(),
        "untracked\n"
    );
    assert_eq!(
        git_bytes(repo, &["stash", "list", "--format=%H %gs"]).unwrap(),
        previous_stashes
    );
    assert_eq!(state(&c.repos[1].path), other);
}

#[test]
fn smart_checkout_conflicts_keep_backup_and_unrelated_staging() {
    let (_temp, c, _) = fixture();
    let repo = &c.repos[0].path;
    diverged(&c);
    fs::write(repo.join("sibling"), "staged sibling\n").unwrap();
    run(repo, &["add", "sibling"]);
    let (level, message) = blocked(&c, "refs/heads/topic").execute(&c, false);
    assert_eq!(level, NoticeLevel::Warning, "{message}");
    assert!(
        message.contains("Conflicts") && message.contains("Stash"),
        "{message}"
    );
    assert_eq!(run(repo, &["branch", "--show-current"]), "topic");
    assert!(!run(repo, &["ls-files", "--unmerged"]).is_empty());
    assert_eq!(run(repo, &["show", ":sibling"]), "staged sibling");
    assert_eq!(run(repo, &["show", "stash@{0}:file"]), "local");
    assert_eq!(
        run(repo, &["show", "stash@{0}^2:sibling"]),
        "staged sibling"
    );
    assert!(git(repo, &["rev-parse", "--verify", "MERGE_HEAD"]).is_err());
}

#[test]
fn smart_checkout_rejects_stale_content_index_head_scope_and_target() {
    for change in ["content", "index", "head", "scope", "target", "workspace"] {
        let (_temp, mut c, _) = fixture();
        let repo = c.repos[0].path.clone();
        diverged(&c);
        let checkout = blocked(&c, "refs/heads/topic");
        match change {
            "content" => fs::write(repo.join("file"), "later\n").unwrap(),
            "index" => {
                run(&repo, &["add", "file"]);
            }
            "head" => {
                run(&repo, &["commit", "--allow-empty", "-qm", "external"]);
            }
            "scope" => {
                c.scope.remove(&0);
            }
            "target" => {
                run(&repo, &["update-ref", "refs/heads/topic", "main"]);
            }
            "workspace" => c.repos.swap(0, 1),
            _ => unreachable!(),
        }
        let before = state(&repo);
        let (level, message) = checkout.execute(&c, false);
        assert_eq!(level, NoticeLevel::Error, "{change}: {message}");
        assert_eq!(state(&repo), before, "{change}");
        assert!(run(&repo, &["stash", "list"]).is_empty(), "{change}");
    }
}

#[test]
fn smart_checkout_force_requires_reviewed_state_and_preserves_other_repositories() {
    let (_temp, c, _) = fixture();
    let repo = &c.repos[0].path;
    diverged(&c);
    fs::write(repo.join("staged"), "staged\n").unwrap();
    run(repo, &["add", "staged"]);
    fs::write(repo.join("untracked"), "keep untracked\n").unwrap();
    let other = state(&c.repos[1].path);
    let (level, message) = blocked(&c, "refs/heads/topic").execute(&c, true);
    assert_eq!(level, NoticeLevel::Success, "{message}");
    assert_eq!(fs::read_to_string(repo.join("file")).unwrap(), "topic\n");
    assert!(!repo.join("staged").exists());
    assert_eq!(
        fs::read_to_string(repo.join("untracked")).unwrap(),
        "keep untracked\n"
    );
    assert!(run(repo, &["diff", "--cached"]).is_empty());
    assert!(run(repo, &["stash", "list"]).is_empty());
    assert_eq!(state(&c.repos[1].path), other);
}

#[test]
fn smart_checkout_compatible_edits_switch_directly_without_a_stash() {
    let (_temp, c, mut panel) = fixture();
    let repo = &c.repos[0].path;
    run(repo, &["switch", "-c", "topic"]);
    fs::write(repo.join("target-only"), "target\n").unwrap();
    run(repo, &["add", "."]);
    run(repo, &["commit", "-qm", "target"]);
    run(repo, &["switch", "main"]);
    fs::write(repo.join("file"), "staged\n").unwrap();
    run(repo, &["add", "file"]);
    fs::write(repo.join("file"), "working\n").unwrap();
    let entries = run(repo, &["ls-files", "--stage", "file"]);
    reload(&mut panel, &c);
    select(&mut panel, 0, "refs/heads/topic");
    assert!(matches!(
        key(&mut panel, &c, K::Enter),
        Action::Notice(NoticeLevel::Info, _)
    ));
    finish_checkout(&mut panel);
    assert!(panel.checkout.is_none());
    assert_eq!(run(repo, &["ls-files", "--stage", "file"]), entries);
    assert_eq!(fs::read_to_string(repo.join("file")).unwrap(), "working\n");
    assert!(run(repo, &["stash", "list"]).is_empty());
}

#[test]
fn smart_checkout_untracked_collision_keeps_both_versions_and_the_backup() {
    let (_temp, c, _) = fixture();
    let repo = &c.repos[0].path;
    run(repo, &["switch", "-c", "topic"]);
    fs::write(repo.join("new"), "target tracked\n").unwrap();
    run(repo, &["add", "new"]);
    run(repo, &["commit", "-qm", "target"]);
    run(repo, &["switch", "main"]);
    fs::write(repo.join("new"), "local untracked\n").unwrap();
    let (level, message) = blocked(&c, "refs/heads/topic").execute(&c, false);
    assert_eq!(level, NoticeLevel::Warning, "{message}");
    assert!(
        message.contains("Restore incomplete") && message.contains("Stash"),
        "{message}"
    );
    assert_eq!(
        fs::read_to_string(repo.join("new")).unwrap(),
        "target tracked\n"
    );
    assert_eq!(run(repo, &["show", "stash@{0}^3:new"]), "local untracked");
}

#[test]
fn smart_checkout_remote_tracking_uses_the_same_review_and_keeps_remote_refs() {
    let (_temp, c, mut panel) = fixture();
    let repo = &c.repos[0].path;
    diverged(&c);
    run(repo, &["remote", "add", "origin", "../unused"]);
    run(
        repo,
        &["update-ref", "refs/remotes/origin/remote-topic", "topic"],
    );
    let remote = run(repo, &["rev-parse", "refs/remotes/origin/remote-topic"]);
    reload(&mut panel, &c);
    select(&mut panel, 0, "refs/remotes/origin/remote-topic");
    key(&mut panel, &c, K::Enter);
    assert!(matches!(
        key(&mut panel, &c, K::Enter),
        Action::Notice(NoticeLevel::Info, _)
    ));
    wait_for_checkout_prompt(&mut panel);
    let checkout = panel.checkout.take().unwrap();
    let (level, message) = checkout.execute(&c, false);
    assert_eq!(level, NoticeLevel::Warning, "{message}");
    assert_eq!(run(repo, &["branch", "--show-current"]), "remote-topic");
    assert_eq!(
        run(repo, &["rev-parse", "--symbolic-full-name", "@{upstream}"]),
        "refs/remotes/origin/remote-topic"
    );
    assert_eq!(
        run(repo, &["rev-parse", "refs/remotes/origin/remote-topic"]),
        remote
    );
}

#[test]
fn smart_checkout_rejects_ignored_collisions_and_hidden_index_flags_before_saving() {
    for protected in ["ignored", "skip-worktree", "assume-unchanged"] {
        let (_temp, c, _) = fixture();
        let repo = &c.repos[0].path;
        fs::write(repo.join(".gitignore"), "protected\n").unwrap();
        fs::write(repo.join("flagged"), "base\n").unwrap();
        run(repo, &["add", "."]);
        run(repo, &["commit", "-qm", "protection"]);
        run(repo, &["switch", "-c", "topic"]);
        fs::write(repo.join("file"), "target\n").unwrap();
        if protected == "ignored" {
            fs::write(repo.join("protected"), "target ignored\n").unwrap();
            run(repo, &["add", "-f", "protected"]);
        }
        run(repo, &["add", "file"]);
        run(repo, &["commit", "-qm", "target"]);
        run(repo, &["switch", "main"]);
        fs::write(repo.join("file"), "local\n").unwrap();
        if protected == "ignored" {
            fs::write(repo.join("protected"), "keep ignored\n").unwrap();
        } else {
            run(
                repo,
                &["update-index", &format!("--{protected}"), "flagged"],
            );
        }
        let before = state(repo);
        let review = review(&c, "refs/heads/topic", Operation::Checkout, "");
        let error = review.execute(&c).unwrap_err();
        assert!(Checkout::prepare(review, &c, error).is_err(), "{protected}");
        assert_eq!(state(repo), before, "{protected}");
        assert!(run(repo, &["stash", "list"]).is_empty(), "{protected}");
        if protected == "ignored" {
            assert_eq!(
                fs::read_to_string(repo.join("protected")).unwrap(),
                "keep ignored\n"
            );
        }
    }
}

#[test]
fn smart_checkout_rejects_nested_repositories_and_target_submodule_boundaries() {
    for (nested, module) in [(true, "module"), (false, "module"), (false, "Module")] {
        let (_temp, c, _) = fixture();
        let repo = &c.repos[0].path;
        run(repo, &["switch", "-c", "topic"]);
        fs::write(repo.join("file"), "target\n").unwrap();
        if !nested {
            let oid = run(repo, &["rev-parse", "HEAD"]);
            run(
                repo,
                &[
                    "update-index",
                    "--add",
                    "--cacheinfo",
                    &format!("160000,{oid},{module}"),
                ],
            );
        }
        run(repo, &["add", "file"]);
        run(repo, &["commit", "-qm", "target"]);
        run(repo, &["switch", "main"]);
        run(repo, &["config", "core.ignoreCase", "true"]);
        fs::write(repo.join("file"), "local\n").unwrap();
        fs::create_dir_all(repo.join("module")).unwrap();
        if nested {
            run(&repo.join("module"), &["init", "-q"]);
        }
        fs::write(repo.join("module/local"), "keep child\n").unwrap();
        let before = state(repo);
        let review = review(&c, "refs/heads/topic", Operation::Checkout, "");
        let error = review.execute(&c).unwrap_err();
        assert!(
            Checkout::prepare(review, &c, error).is_err(),
            "nested repository: {nested}"
        );
        assert_eq!(state(repo), before);
        assert_eq!(
            fs::read_to_string(repo.join("module/local")).unwrap(),
            "keep child\n"
        );
        assert!(run(repo, &["stash", "list"]).is_empty());
    }
}

#[test]
fn smart_checkout_does_not_change_a_dirty_hidden_submodule() {
    let (_temp, c, _) = fixture();
    let repo = &c.repos[0].path;
    run(
        repo,
        &[
            "-c",
            "protocol.file.allow=always",
            "submodule",
            "add",
            "../web",
            "module",
        ],
    );
    run(repo, &["commit", "-am", "add submodule"]);
    run(repo, &["config", "submodule.recurse", "true"]);
    diverged(&c);
    let child = repo.join("module");
    fs::write(child.join("file"), "child staged\n").unwrap();
    run(&child, &["add", "file"]);
    fs::write(child.join("file"), "child working\n").unwrap();
    let head = run(&child, &["rev-parse", "HEAD"]);
    let index = git_bytes(&child, &["ls-files", "--stage", "-z"]).unwrap();
    let (level, message) = blocked(&c, "refs/heads/topic").execute(&c, false);
    assert_eq!(level, NoticeLevel::Warning, "{message}");
    assert!(message.contains("Conflicts"), "{message}");
    assert_eq!(run(&child, &["rev-parse", "HEAD"]), head);
    assert_eq!(
        git_bytes(&child, &["ls-files", "--stage", "-z"]).unwrap(),
        index
    );
    assert_eq!(
        fs::read_to_string(child.join("file")).unwrap(),
        "child working\n"
    );
    assert_eq!(
        fs::read_to_string(&c.repos[1].path.join("file")).unwrap(),
        "base\n"
    );
}

#[test]
fn smart_checkout_force_rejects_edits_made_after_the_review() {
    let (_temp, c, _) = fixture();
    let repo = &c.repos[0].path;
    diverged(&c);
    let checkout = blocked(&c, "refs/heads/topic");
    fs::write(repo.join("file"), "edited after review\n").unwrap();
    let before = state(repo);
    let (level, message) = checkout.execute(&c, true);
    assert_eq!(level, NoticeLevel::Error, "{message}");
    assert_eq!(state(repo), before);
    assert!(run(repo, &["stash", "list"]).is_empty());
}

#[test]
fn smart_checkout_keeps_backup_when_only_the_staged_version_conflicts() {
    let (_temp, c, _) = fixture();
    let repo = &c.repos[0].path;
    let mut lines: Vec<_> = (0..40).map(|number| format!("line {number}\n")).collect();
    fs::write(repo.join("file"), lines.concat()).unwrap();
    run(repo, &["commit", "-am", "long base"]);
    run(repo, &["switch", "-c", "topic"]);
    lines[0] = "target first line\n".into();
    fs::write(repo.join("file"), lines.concat()).unwrap();
    run(repo, &["commit", "-am", "target"]);
    run(repo, &["switch", "main"]);
    lines[0] = "staged first line\n".into();
    fs::write(repo.join("file"), lines.concat()).unwrap();
    fs::write(repo.join("sibling"), "staged sibling\n").unwrap();
    run(repo, &["add", "."]);
    lines[0] = "line 0\n".into();
    lines[39] = "local last line\n".into();
    fs::write(repo.join("file"), lines.concat()).unwrap();
    let (level, message) = blocked(&c, "refs/heads/topic").execute(&c, false);
    assert_eq!(level, NoticeLevel::Warning, "{message}");
    assert!(
        message.contains("staging could not be restored"),
        "{message}"
    );
    assert!(run(repo, &["ls-files", "--unmerged"]).is_empty());
    assert_eq!(run(repo, &["show", ":sibling"]), "staged sibling");
    assert!(run(repo, &["show", "stash@{0}^2:file"]).starts_with("staged first line"));
    lines[0] = "target first line\n".into();
    assert_eq!(
        fs::read_to_string(repo.join("file")).unwrap(),
        lines.concat()
    );
}

#[cfg(unix)]
#[test]
fn smart_checkout_rejects_a_target_symlink_above_saved_files() {
    let (_temp, c, _) = fixture();
    let repo = &c.repos[0].path;
    run(repo, &["switch", "-c", "topic"]);
    std::os::unix::fs::symlink(&c.repos[1].path, repo.join("link")).unwrap();
    fs::write(repo.join("file"), "target\n").unwrap();
    run(repo, &["add", "."]);
    run(repo, &["commit", "-qm", "target"]);
    run(repo, &["switch", "main"]);
    fs::create_dir(repo.join("link")).unwrap();
    fs::write(repo.join("link/local"), "saved local\n").unwrap();
    fs::write(repo.join("file"), "local\n").unwrap();
    let before = state(&c.repos[1].path);
    let review = review(&c, "refs/heads/topic", Operation::Checkout, "");
    let error = review.execute(&c).unwrap_err();
    assert!(Checkout::prepare(review, &c, error).is_err());
    assert_eq!(state(&c.repos[1].path), before);
    assert_eq!(
        fs::read_to_string(repo.join("link/local")).unwrap(),
        "saved local\n"
    );
    assert!(run(repo, &["stash", "list"]).is_empty());
}

#[cfg(unix)]
fn hook(repo: &Path, script: &str) {
    use std::os::unix::fs::PermissionsExt;
    let directory = repo.join(".git/test-hooks");
    fs::create_dir_all(&directory).unwrap();
    let path = directory.join("post-checkout");
    fs::write(&path, script).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    run(
        repo,
        &["config", "core.hooksPath", directory.to_str().unwrap()],
    );
}

#[cfg(unix)]
#[test]
fn smart_checkout_hook_failure_after_switch_keeps_backup_without_replaying_changes() {
    let (_temp, c, _) = fixture();
    let repo = &c.repos[0].path;
    diverged(&c);
    let checkout = blocked(&c, "refs/heads/topic");
    hook(repo, "#!/bin/sh\nexit 1\n");
    let (level, message) = checkout.execute(&c, false);
    assert_eq!(level, NoticeLevel::Error, "{message}");
    assert!(message.contains("Stash"), "{message}");
    assert_eq!(run(repo, &["branch", "--show-current"]), "topic");
    assert_eq!(fs::read_to_string(repo.join("file")).unwrap(), "topic\n");
    assert_eq!(run(repo, &["show", "stash@{0}:file"]), "local");
}

#[cfg(unix)]
#[test]
fn smart_checkout_failed_switch_on_original_branch_restores_changes_and_staging() {
    let (_temp, c, _) = fixture();
    let repo = &c.repos[0].path;
    diverged(&c);
    fs::write(repo.join("sibling"), "staged sibling\n").unwrap();
    run(repo, &["add", "sibling"]);
    fs::write(repo.join("untracked"), "untracked\n").unwrap();
    let before = state(repo);
    let checkout = blocked(&c, "refs/heads/topic");
    hook(
        repo,
        "#!/bin/sh\ngit -c core.hooksPath=/dev/null switch main || exit 1\nexit 1\n",
    );
    let (level, message) = checkout.execute(&c, false);
    assert_eq!(level, NoticeLevel::Error, "{message}");
    assert!(
        message.contains("Original branch and local changes restored"),
        "{message}"
    );
    assert_eq!(state(repo), before);
    assert_eq!(
        fs::read_to_string(repo.join("untracked")).unwrap(),
        "untracked\n"
    );
    assert!(run(repo, &["stash", "list"]).is_empty());
}

#[cfg(unix)]
#[test]
fn smart_checkout_external_stash_change_does_not_drop_an_entry_after_restoring() {
    let (_temp, c, _) = fixture();
    let repo = &c.repos[0].path;
    let mut lines: Vec<_> = (0..30).map(|number| format!("line {number}\n")).collect();
    fs::write(repo.join("file"), lines.concat()).unwrap();
    run(repo, &["commit", "-am", "long base"]);
    run(repo, &["switch", "-c", "topic"]);
    lines[0] = "branch start\n".into();
    fs::write(repo.join("file"), lines.concat()).unwrap();
    run(repo, &["commit", "-am", "target"]);
    run(repo, &["switch", "main"]);
    lines[0] = "line 0\n".into();
    lines[29] = "local end\n".into();
    fs::write(repo.join("file"), lines.concat()).unwrap();
    let checkout = blocked(&c, "refs/heads/topic");
    hook(repo, "#!/bin/sh\nstash_oid=$(git rev-parse refs/stash) || exit 1\nstash_tree=$(git rev-parse \"$stash_oid^{tree}\") || exit 1\nstash_base=$(git rev-parse \"$stash_oid^1\") || exit 1\nstash_index=$(git rev-parse \"$stash_oid^2\") || exit 1\nexternal=$(git commit-tree \"$stash_tree\" -p \"$stash_base\" -p \"$stash_index\" -m external) || exit 1\ngit stash store -m 'external backup' \"$external\"\n");
    let (level, message) = checkout.execute(&c, false);
    assert_eq!(level, NoticeLevel::Warning, "{message}");
    assert!(message.contains("do not apply again"), "{message}");
    assert!(run(repo, &["ls-files", "--unmerged"]).is_empty());
    assert_eq!(
        run(repo, &["stash", "list", "--format=%gs"])
            .lines()
            .count(),
        2
    );
    assert!(run(repo, &["stash", "list", "--format=%gs"]).starts_with("external backup"));
    assert_eq!(
        run(repo, &["show", "stash@{1}:file"]),
        lines.concat().trim_end()
    );
}
