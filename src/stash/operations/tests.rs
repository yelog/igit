use super::*;
use std::fs;

fn run(repo: &Path, args: &[&str]) -> String {
    git(repo, args).unwrap()
}

fn fixture() -> (tempfile::TempDir, Context) {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path();
    for args in [
        &["init", "-q", "-b", "main"][..],
        &["config", "user.name", "Stash tester"],
        &["config", "user.email", "stash@example.invalid"],
        &["config", "commit.gpgsign", "false"],
        &["config", "core.hooksPath", ".git/hooks"],
    ] {
        run(repo, args);
    }
    for file in ["mixed.txt", "local.txt", "working.txt"] {
        fs::write(repo.join(file), "base\n").unwrap();
    }
    fs::write(repo.join(".gitignore"), "ignored.txt\n").unwrap();
    run(repo, &["add", "."]);
    run(repo, &["commit", "-qm", "base"]);
    for n in 1..=2 {
        fs::write(repo.join("mixed.txt"), format!("staged {n}\n")).unwrap();
        run(repo, &["add", "mixed.txt"]);
        fs::write(repo.join("mixed.txt"), format!("working {n}\n")).unwrap();
        fs::write(repo.join("new 中文 file.txt"), format!("untracked {n}\n")).unwrap();
        run(repo, &["stash", "push", "-u", "-m", &format!("saved {n}")]);
    }
    let mut c = Context::discover(repo).unwrap();
    c.state_dir = repo.join("state");
    (dir, c)
}

fn prepare(c: &Context, index: usize, operation: Operation) -> Review {
    let entries = model::entries(&c.repos[0].path).unwrap();
    Review::prepare(c, 0, &entries[index], &entries, operation).unwrap()
}

fn state(repo: &Path) -> Vec<Vec<u8>> {
    let mut state: Vec<_> = [
        &["rev-parse", "HEAD"][..],
        &["ls-files", "--stage", "-z"],
        &[
            "--no-optional-locks",
            "diff",
            "--binary",
            "--no-ext-diff",
            "--no-textconv",
        ],
        &[
            "--no-optional-locks",
            "diff",
            "--cached",
            "--binary",
            "--no-ext-diff",
            "--no-textconv",
        ],
        &[
            "--no-optional-locks",
            "status",
            "--porcelain=v1",
            "-z",
            "--untracked-files=all",
        ],
        &["stash", "list", "--format=%H%x00%gd%x00%gs", "-z"],
    ]
    .into_iter()
    .map(|args| git_bytes(repo, args).unwrap())
    .collect();
    state.push(fs::read(revpath(repo, "--absolute-git-dir").unwrap().join("index")).unwrap());
    for file in [
        "mixed.txt",
        "local.txt",
        "working.txt",
        "new 中文 file.txt",
        "ignored.txt",
    ] {
        state.push(fs::read(repo.join(file)).unwrap_or_default());
    }
    state
}

#[test]
fn stash_operation_reviews_are_read_only_and_describe_the_exact_entry() {
    let (_dir, c) = fixture();
    let repo = &c.repos[0].path;
    let before = state(repo);
    for operation in [Operation::Apply, Operation::Pop, Operation::Drop] {
        let review = prepare(&c, 1, operation);
        let body = review.body();
        for text in [
            "stash@{1}",
            "saved 1",
            &review.source.oid,
            &review.source.saved,
        ] {
            assert!(body.contains(text), "{text}");
        }
        assert_eq!(state(repo), before);
    }
}

#[test]
fn stash_apply_restores_saved_content_and_preserves_entry_and_unrelated_local_changes() {
    let (_dir, c) = fixture();
    let repo = &c.repos[0].path;
    fs::write(repo.join("local.txt"), "local staged\n").unwrap();
    run(repo, &["add", "local.txt"]);
    fs::write(repo.join("working.txt"), "local working\n").unwrap();
    fs::write(repo.join("ignored.txt"), "ignored\n").unwrap();
    let entries = model::entries(repo).unwrap();
    let index = git_bytes(repo, &["ls-files", "--stage", "-z"]).unwrap();
    let result = prepare(&c, 0, Operation::Apply).execute(&c).unwrap();
    assert!(result.starts_with("Applied stash"));
    assert_eq!(
        fs::read_to_string(repo.join("mixed.txt")).unwrap(),
        "working 2\n"
    );
    assert_eq!(
        fs::read_to_string(repo.join("new 中文 file.txt")).unwrap(),
        "untracked 2\n"
    );
    assert_eq!(
        fs::read_to_string(repo.join("working.txt")).unwrap(),
        "local working\n"
    );
    assert_eq!(
        fs::read_to_string(repo.join("ignored.txt")).unwrap(),
        "ignored\n"
    );
    assert_eq!(
        git_bytes(repo, &["ls-files", "--stage", "-z"]).unwrap(),
        index
    );
    assert_eq!(model::entries(repo).unwrap(), entries);
}

#[test]
fn stash_apply_honors_reviewed_saved_staging_option() {
    let (_dir, c) = fixture();
    let repo = &c.repos[0].path;
    run(repo, &["config", "stash.index", "true"]);
    let review = prepare(&c, 0, Operation::Apply);
    assert!(review.body().contains("Staging: Restore (stash.index)"));
    review.execute(&c).unwrap();
    assert_eq!(run(repo, &["show", ":mixed.txt"]), "staged 2");
    assert_eq!(
        fs::read_to_string(repo.join("mixed.txt")).unwrap(),
        "working 2\n"
    );
    assert_eq!(model::entries(repo).unwrap().len(), 2);
}

#[test]
fn stash_pop_applies_and_drops_only_the_selected_older_entry() {
    let (_dir, c) = fixture();
    let repo = &c.repos[0].path;
    let before = model::entries(repo).unwrap();
    let result = prepare(&c, 1, Operation::Pop).execute(&c).unwrap();
    assert!(result.starts_with("Popped stash"));
    assert_eq!(
        fs::read_to_string(repo.join("mixed.txt")).unwrap(),
        "working 1\n"
    );
    assert_eq!(
        fs::read_to_string(repo.join("new 中文 file.txt")).unwrap(),
        "untracked 1\n"
    );
    assert_eq!(model::entries(repo).unwrap(), before[..1]);
}

#[test]
fn stash_drop_only_removes_selected_reflog_entry_and_allows_dirty_or_conflicted_worktrees() {
    let (_dir, c) = fixture();
    let repo = &c.repos[0].path;
    fs::write(repo.join("local.txt"), "local staged\n").unwrap();
    run(repo, &["add", "local.txt"]);
    fs::write(repo.join("mixed.txt"), "local working\n").unwrap();
    fs::write(repo.join("new 中文 file.txt"), "untracked local\n").unwrap();
    fs::write(
        repo.join(".git/MERGE_HEAD"),
        run(repo, &["rev-parse", "HEAD"]),
    )
    .unwrap();
    let before = state(repo);
    let top = model::entries(repo).unwrap()[0].clone();
    prepare(&c, 1, Operation::Drop).execute(&c).unwrap();
    assert_eq!(model::entries(repo).unwrap(), [top]);
    let after = state(repo);
    for i in (0..before.len()).filter(|i| *i != 5) {
        assert_eq!(after[i], before[i], "snapshot {i}");
    }
    prepare(&c, 0, Operation::Drop).execute(&c).unwrap();
    assert!(model::entries(repo).unwrap().is_empty());
    assert!(git(repo, &["rev-parse", "--verify", "refs/stash"]).is_err());
    assert_eq!(
        fs::read_to_string(repo.join("mixed.txt")).unwrap(),
        "local working\n"
    );
}

#[test]
fn stash_apply_and_pop_conflicts_keep_the_entry_and_index_stages() {
    for operation in [Operation::Apply, Operation::Pop] {
        let (_dir, c) = fixture();
        let repo = &c.repos[0].path;
        fs::write(repo.join("mixed.txt"), "committed divergence\n").unwrap();
        run(repo, &["add", "mixed.txt"]);
        run(repo, &["commit", "-qm", "diverge"]);
        let entries = model::entries(repo).unwrap();
        let error = prepare(&c, 0, operation)
            .execute(&c)
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("Stash kept") && error.contains("Conflicts tab"),
            "{error}"
        );
        assert!(!git_bytes(repo, &["ls-files", "-u", "-z"])
            .unwrap()
            .is_empty());
        assert!(fs::read_to_string(repo.join("mixed.txt"))
            .unwrap()
            .contains("<<<<<<<"));
        assert_eq!(model::entries(repo).unwrap(), entries);
        assert!(Review::prepare(&c, 0, &entries[0], &entries, operation).is_err());
    }
}

#[test]
fn stash_pop_refuses_to_overwrite_dirty_tracked_and_existing_untracked_files() {
    for file in ["mixed.txt", "new 中文 file.txt"] {
        let (_dir, c) = fixture();
        let repo = &c.repos[0].path;
        fs::write(repo.join(file), "keep local bytes\n").unwrap();
        let entries = model::entries(repo).unwrap();
        assert!(prepare(&c, 0, Operation::Pop).execute(&c).is_err());
        assert_eq!(
            fs::read_to_string(repo.join(file)).unwrap(),
            "keep local bytes\n"
        );
        assert_eq!(model::entries(repo).unwrap(), entries);
    }
}

#[test]
fn stash_operations_reject_changed_reflogs_before_review_or_execution() {
    for operation in [Operation::Apply, Operation::Pop, Operation::Drop] {
        let (_dir, c) = fixture();
        let repo = &c.repos[0].path;
        let entries = model::entries(repo).unwrap();
        let review = prepare(&c, 1, operation);
        // Storing the current tip is a Git no-op; use an older saved object.
        run(
            repo,
            &["stash", "store", "-m", "external entry", &entries[1].oid],
        );
        let before = state(repo);
        assert!(Review::prepare(&c, 0, &entries[0], &entries, operation).is_err());
        assert!(review.execute(&c).is_err());
        assert_eq!(state(repo), before);
    }
}

#[test]
fn stash_drop_distinguishes_duplicate_object_ids_by_reviewed_selector() {
    let (_dir, c) = fixture();
    let repo = &c.repos[0].path;
    let original = model::entries(repo).unwrap();
    run(
        repo,
        &["stash", "store", "-m", "older duplicate", &original[1].oid],
    );
    run(
        repo,
        &["stash", "store", "-m", "newer duplicate", &original[0].oid],
    );
    let entries = model::entries(repo).unwrap();
    assert_eq!(entries[0].oid, entries[2].oid);
    prepare(&c, 2, Operation::Drop).execute(&c).unwrap();
    let after = model::entries(repo).unwrap();
    assert_eq!(after.len(), 3);
    assert_eq!(after[0], entries[0]);
    assert_eq!(after[1], entries[1]);
    assert_eq!(after[2].oid, entries[3].oid);
}

#[test]
fn stash_operations_recheck_roots_workspace_branch_head_and_index() {
    for change in [
        "scope",
        "workspace",
        "branch",
        "head",
        "index",
        "staging option",
        "operation",
    ] {
        let (_dir, mut c) = fixture();
        let repo = c.repos[0].path.clone();
        let review = prepare(&c, 0, Operation::Apply);
        match change {
            "scope" => {
                c.scope.clear();
            }
            "workspace" => {
                c.repos[0].path = repo.join("missing");
            }
            "branch" => {
                run(&repo, &["checkout", "-qb", "other"]);
            }
            "head" => {
                run(&repo, &["commit", "--allow-empty", "-qm", "external"]);
            }
            "index" => {
                fs::write(repo.join("local.txt"), "new staging\n").unwrap();
                run(&repo, &["add", "local.txt"]);
            }
            "staging option" => {
                run(&repo, &["config", "stash.index", "true"]);
            }
            "operation" => {
                fs::write(
                    repo.join(".git/MERGE_HEAD"),
                    run(&repo, &["rev-parse", "HEAD"]),
                )
                .unwrap();
            }
            _ => unreachable!(),
        }
        let before = state(&repo);
        assert!(review.execute(&c).is_err(), "{change}");
        assert_eq!(state(&repo), before, "{change}");
    }
}

#[test]
fn stash_operations_report_git_lock_failures_without_dropping_unapplied_content() {
    for operation in [Operation::Apply, Operation::Pop, Operation::Drop] {
        let (_dir, c) = fixture();
        let repo = &c.repos[0].path;
        let review = prepare(&c, 0, operation);
        let before = state(repo);
        let lock = if operation == Operation::Drop {
            ".git/refs/stash.lock"
        } else {
            ".git/index.lock"
        };
        fs::write(repo.join(lock), "held by another process").unwrap();
        assert!(review.execute(&c).is_err());
        assert_eq!(state(repo), before);
    }
}

#[test]
fn stash_pop_does_not_repeat_successful_apply_after_drop_failure() {
    let (_dir, c) = fixture();
    let repo = &c.repos[0].path;
    let review = prepare(&c, 0, Operation::Pop);
    let entries = model::entries(repo).unwrap();
    fs::write(repo.join(".git/refs/stash.lock"), "held").unwrap();
    let result = review.execute(&c).unwrap();
    assert!(
        result.contains("Applied stash") && result.contains("Do not apply again"),
        "{result}"
    );
    assert_eq!(model::entries(repo).unwrap(), entries);
    assert_eq!(
        fs::read_to_string(repo.join("mixed.txt")).unwrap(),
        "working 2\n"
    );
}

#[cfg(unix)]
#[test]
fn stash_pop_keeps_entries_when_a_hook_changes_the_reflog_during_apply() {
    use std::os::unix::fs::PermissionsExt;
    let (_dir, c) = fixture();
    let repo = &c.repos[0].path;
    let review = prepare(&c, 0, Operation::Pop);
    let entries = model::entries(repo).unwrap();
    let hook = repo.join(".git/hooks/post-index-change");
    fs::write(&hook, format!("#!/bin/sh\nif [ ! -e .git/stash-hook-ran ]; then\n: > .git/stash-hook-ran\ngit stash store -m concurrent {}\nfi\n", entries[1].oid)).unwrap();
    fs::set_permissions(hook, fs::Permissions::from_mode(0o755)).unwrap();
    let result = review.execute(&c).unwrap();
    assert!(
        result.contains("Applied stash") && result.contains("Do not apply again"),
        "{result}"
    );
    assert_eq!(model::entries(repo).unwrap().len(), 3);
    assert_eq!(
        fs::read_to_string(repo.join("mixed.txt")).unwrap(),
        "working 2\n"
    );
}

#[test]
fn stash_operations_use_linked_worktree_destination_and_shared_stash_list() {
    let (_dir, c) = fixture();
    let repo = &c.repos[0].path;
    let holder = tempfile::tempdir().unwrap();
    let linked = holder.path().join("linked");
    run(
        repo,
        &[
            "worktree",
            "add",
            "--detach",
            linked.to_str().unwrap(),
            "HEAD",
        ],
    );
    let linked_context = Context::discover(&linked).unwrap();
    prepare(&linked_context, 0, Operation::Pop)
        .execute(&linked_context)
        .unwrap();
    assert_eq!(
        fs::read_to_string(linked.join("mixed.txt")).unwrap(),
        "working 2\n"
    );
    assert_eq!(
        fs::read_to_string(repo.join("mixed.txt")).unwrap(),
        "base\n"
    );
    assert_eq!(model::entries(repo).unwrap().len(), 1);
}

#[test]
fn stash_apply_refuses_saved_untracked_paths_now_inside_a_hidden_submodule() {
    let (_dir, c) = fixture();
    let (_origin_dir, origin) = fixture();
    let repo = &c.repos[0].path;
    fs::create_dir(repo.join("module")).unwrap();
    fs::write(repo.join("module/new.txt"), "saved untracked\n").unwrap();
    run(repo, &["stash", "push", "-u", "-m", "untracked directory"]);
    run(
        repo,
        &[
            "-c",
            "protocol.file.allow=always",
            "submodule",
            "add",
            origin.repos[0].path.to_str().unwrap(),
            "module",
        ],
    );
    run(repo, &["commit", "-qm", "add submodule"]);
    fs::write(repo.join("module/local.txt"), "local child content\n").unwrap();
    let entries = model::entries(repo).unwrap();
    let before = state(repo);
    for operation in [Operation::Apply, Operation::Pop] {
        let error = Review::prepare(&c, 0, &entries[0], &entries, operation)
            .err()
            .unwrap()
            .to_string();
        assert!(error.contains("submodule"), "{error}");
        assert!(!repo.join("module/new.txt").exists());
        assert_eq!(
            fs::read_to_string(repo.join("module/local.txt")).unwrap(),
            "local child content\n"
        );
        assert_eq!(state(repo), before);
    }
}

#[cfg(unix)]
#[test]
fn stash_apply_rechecks_new_nested_repositories_and_directory_symlinks_after_review() {
    use std::os::unix::fs::symlink;
    for boundary in ["repository", "symlink"] {
        let (_dir, c) = fixture();
        let outside = tempfile::tempdir().unwrap();
        let repo = &c.repos[0].path;
        fs::create_dir(repo.join("nested")).unwrap();
        fs::write(repo.join("nested/new.txt"), "saved\n").unwrap();
        run(repo, &["stash", "push", "-u", "-m", "saved nested file"]);
        let review = prepare(&c, 0, Operation::Pop);
        if boundary == "repository" {
            fs::create_dir(repo.join("nested")).unwrap();
            run(&repo.join("nested"), &["init", "-q"]);
        } else {
            symlink(outside.path(), repo.join("nested")).unwrap();
        }
        let before = state(repo);
        assert!(review.execute(&c).is_err(), "{boundary}");
        assert!(!repo.join("nested/new.txt").exists());
        assert!(!outside.path().join("new.txt").exists());
        assert_eq!(state(repo), before);
    }
}
