use super::*;

fn fixture(path: &str) -> (TempDir, Context, String, String) {
    let dir = init();
    let repo = dir.path();
    git(repo, &["config", "commit.gpgsign", "false"]);
    git(repo, &["config", "rerere.enabled", "false"]);
    commit(repo, "base", "Base content");
    git(repo, &["switch", "-c", "source"]);
    fs::create_dir_all(repo.join(path).parent().unwrap()).unwrap();
    commit(
        repo,
        path,
        "Selected change\n\nKeep the original body: 修复",
    );
    let picked = git(repo, &["rev-parse", "HEAD"]);
    commit(repo, "source-tip", "Unselected descendant");
    git(repo, &["switch", "main"]);
    commit(repo, "destination", "Destination content");
    let head = git(repo, &["rev-parse", "HEAD"]);
    let c = Context::discover(repo).unwrap();
    (dir, c, picked, head)
}

fn index(repo: &Path) -> Vec<u8> {
    fs::read(revpath(repo, "--absolute-git-dir").unwrap().join("index")).unwrap()
}

#[test]
fn applies_only_the_reviewed_oid_even_when_its_source_ref_moves() {
    let (dir, c, picked, head) = fixture("picked");
    let repo = dir.path();
    fs::write(repo.join("untracked"), "Keep this local file").unwrap();
    fs::write(repo.join(".git/info/exclude"), "ignored\n").unwrap();
    fs::write(repo.join("ignored"), "Keep this ignored file").unwrap();
    let before = index(repo);
    let (review, body) = prepare(&c, &picked, Kind::CherryPick, "").unwrap();
    assert!(body.contains("Repository:") && body.contains("new commit on main"));
    assert!(body.contains(&format!("Source commit: {picked}")));
    assert!(body.contains(&format!("Current HEAD: {head}")));
    assert!(body.contains("Continue or Abort separately in Conflicts"));
    assert_eq!(index(repo), before);
    assert_eq!(git(repo, &["rev-parse", "HEAD"]), head);
    git(repo, &["update-ref", "refs/heads/source", &head]);
    assert!(review
        .execute(&c)
        .unwrap()
        .contains("Cherry-pick Commit completed"));
    assert_eq!(git(repo, &["rev-parse", "HEAD^"]), head);
    assert_eq!(
        message(repo, "HEAD").unwrap(),
        message(repo, &picked).unwrap()
    );
    assert_eq!(git(repo, &["symbolic-ref", "HEAD"]), "refs/heads/main");
    assert_eq!(
        git(repo, &["show", "HEAD:picked"]),
        message(repo, &picked).unwrap()
    );
    assert_eq!(git(repo, &["show", ":base"]), "Base content");
    assert_eq!(git(repo, &["show", ":destination"]), "Destination content");
    assert!(!repo.join("source-tip").exists());
    assert_eq!(
        fs::read_to_string(repo.join("untracked")).unwrap(),
        "Keep this local file"
    );
    assert_eq!(
        fs::read_to_string(repo.join("ignored")).unwrap(),
        "Keep this ignored file"
    );
    assert!(git(repo, &["diff", "--cached"]).is_empty());
    assert!(git(repo, &["diff"]).is_empty());
}

#[test]
fn cancelling_review_leaves_head_index_and_working_files_unchanged() {
    let (dir, c, picked, head) = fixture("picked");
    let repo = dir.path();
    let before = index(repo);
    let (review, _) = prepare(&c, &picked, Kind::CherryPick, "").unwrap();
    drop(review);
    assert_eq!(git(repo, &["rev-parse", "HEAD"]), head);
    assert_eq!(index(repo), before);
    assert_eq!(
        fs::read_to_string(repo.join("base")).unwrap(),
        "Base content"
    );
    assert!(!repo.join("picked").exists());
    assert!(!repo.join(".git/CHERRY_PICK_HEAD").exists());
}

#[test]
fn changed_head_or_branch_requires_another_review() {
    for change_branch in [false, true] {
        let (dir, c, picked, head) = fixture("picked");
        let repo = dir.path();
        let (review, _) = prepare(&c, &picked, Kind::CherryPick, "").unwrap();
        if change_branch {
            git(repo, &["switch", "-c", "external"]);
        } else {
            git(repo, &["commit", "--allow-empty", "-qm", "External commit"]);
        }
        let current = git(repo, &["rev-parse", "HEAD"]);
        let before = index(repo);
        assert!(format!("{:#}", review.execute(&c).unwrap_err()).contains("HEAD changed"));
        assert_eq!(git(repo, &["rev-parse", "HEAD"]), current);
        assert_eq!(index(repo), before);
        assert!(!repo.join("picked").exists());
        if change_branch {
            assert_eq!(current, head);
            assert_eq!(git(repo, &["symbolic-ref", "HEAD"]), "refs/heads/external");
        }
    }
}

#[test]
fn roots_changes_reject_the_hidden_repository() {
    let (dir, mut c, picked, head) = fixture("picked");
    let repo = dir.path();
    let (review, _) = prepare(&c, &picked, Kind::CherryPick, "").unwrap();
    let before = index(repo);
    c.scope.clear();
    assert!(review
        .execute(&c)
        .unwrap_err()
        .to_string()
        .contains("outside Roots"));
    assert!(error(prepare(&c, &picked, Kind::CherryPick, "")).contains("outside Roots"));
    assert_eq!(git(repo, &["rev-parse", "HEAD"]), head);
    assert_eq!(index(repo), before);
    assert!(!repo.join("picked").exists());
}

#[test]
fn same_oid_in_two_repositories_never_expands_the_write_scope() {
    let (source, _, picked, head) = fixture("picked");
    let dir = tempfile::tempdir().unwrap();
    let left = dir.path().join("left");
    let right = dir.path().join("right");
    for repo in [&left, &right] {
        git(
            dir.path(),
            &[
                "clone",
                "-q",
                source.path().to_str().unwrap(),
                repo.to_str().unwrap(),
            ],
        );
        git(repo, &["config", "user.name", "Log tester"]);
        git(repo, &["config", "user.email", "test@example.invalid"]);
        git(repo, &["config", "commit.gpgsign", "false"]);
    }
    let mut c = Context::discover(dir.path()).unwrap();
    let right_id = c
        .repos
        .iter()
        .position(|repo| repo.name == "right")
        .unwrap();
    let left_index = dirty_index(&left);
    let left_bytes = index(&left);
    let target = Target::new(
        &c,
        &Rev {
            root: right_id,
            oid: picked.clone(),
            subject: "Selected change".into(),
            ..Default::default()
        },
    )
    .unwrap();
    let (stale, _) = Review::prepare(&c, target.clone(), Kind::CherryPick, String::new()).unwrap();
    let (review, body) = Review::prepare(&c, target, Kind::CherryPick, String::new()).unwrap();
    assert!(body.contains("Repository: right"));
    c.repos.swap(0, 1);
    assert!(stale
        .execute(&c)
        .unwrap_err()
        .to_string()
        .contains("Workspace changed"));
    c.repos.swap(0, 1);
    review.execute(&c).unwrap();
    assert_eq!(git(&right, &["rev-parse", "HEAD^"]), head);
    assert!(right.join("picked").exists());
    assert_eq!(git(&left, &["rev-parse", "HEAD"]), head);
    assert_eq!(git(&left, &["ls-files", "--stage"]), left_index);
    assert_eq!(index(&left), left_bytes);
    assert_eq!(
        fs::read_to_string(left.join("base")).unwrap(),
        "working content"
    );
    assert!(!left.join("picked").exists());
}

#[test]
fn dirty_index_and_working_tree_are_preserved_before_and_after_review() {
    for after_review in [false, true] {
        let (dir, c, picked, head) = fixture("picked");
        let repo = dir.path();
        git(repo, &["config", "merge.autoStash", "true"]);
        git(repo, &["config", "rebase.autoStash", "true"]);
        let review = after_review.then(|| prepare(&c, &picked, Kind::CherryPick, "").unwrap().0);
        dirty_index(repo);
        fs::write(repo.join("destination"), "Unrelated staged sibling").unwrap();
        git(repo, &["add", "destination"]);
        let before = index(repo);
        let failure = if let Some(review) = review {
            format!("{:#}", review.execute(&c).unwrap_err())
        } else {
            error(prepare(&c, &picked, Kind::CherryPick, ""))
        };
        assert!(failure.contains("tracked working tree and index changes"));
        assert_eq!(git(repo, &["rev-parse", "HEAD"]), head);
        assert_eq!(index(repo), before);
        assert_eq!(git(repo, &["show", ":base"]), "staged content");
        assert_eq!(
            git(repo, &["show", ":destination"]),
            "Unrelated staged sibling"
        );
        assert_eq!(
            fs::read_to_string(repo.join("base")).unwrap(),
            "working content"
        );
        assert_eq!(
            fs::read_to_string(repo.join("untracked")).unwrap(),
            "untracked content"
        );
        assert!(
            optional(repo, &["rev-parse", "--verify", "--quiet", "refs/stash"])
                .unwrap()
                .is_none()
        );
        assert!(!repo.join("picked").exists());
    }
}

#[test]
fn local_untracked_and_ignored_collisions_are_rechecked_with_directory_boundaries() {
    for (picked_path, local_path) in [
        ("picked", "picked"),
        ("picked", "picked/local"),
        ("picked/file", "picked"),
    ] {
        for ignored in [false, true] {
            for after_review in [false, true] {
                let (dir, c, picked, head) = fixture(picked_path);
                let repo = dir.path();
                let review =
                    after_review.then(|| prepare(&c, &picked, Kind::CherryPick, "").unwrap().0);
                if ignored {
                    fs::write(repo.join(".git/info/exclude"), "picked\n").unwrap();
                }
                fs::create_dir_all(repo.join(local_path).parent().unwrap()).unwrap();
                fs::write(repo.join(local_path), "Local bytes must survive").unwrap();
                let before = index(repo);
                let failure = if let Some(review) = review {
                    format!("{:#}", review.execute(&c).unwrap_err())
                } else {
                    error(prepare(&c, &picked, Kind::CherryPick, ""))
                };
                assert!(failure.contains("may be overwritten"), "{failure}");
                assert_eq!(git(repo, &["rev-parse", "HEAD"]), head);
                assert_eq!(index(repo), before);
                assert_eq!(
                    fs::read_to_string(repo.join(local_path)).unwrap(),
                    "Local bytes must survive"
                );
                assert!(!repo.join(".git/CHERRY_PICK_HEAD").exists());
            }
        }
    }
}

#[test]
fn directory_rename_configuration_cannot_redirect_a_pick_into_an_ignored_file() {
    let dir = init();
    let repo = dir.path();
    fs::create_dir_all(repo.join("old")).unwrap();
    commit(repo, "old/existing", "Original file");
    git(repo, &["switch", "-c", "source"]);
    commit(repo, "old/added", "Selected addition");
    let picked = git(repo, &["rev-parse", "HEAD"]);
    git(repo, &["switch", "main"]);
    git(repo, &["mv", "old", "new"]);
    git(repo, &["commit", "-qm", "Rename directory"]);
    git(repo, &["config", "merge.directoryRenames", "true"]);
    fs::write(repo.join(".git/info/exclude"), "new/added\n").unwrap();
    fs::write(repo.join("new/added"), "Ignored local content").unwrap();
    let head = git(repo, &["rev-parse", "HEAD"]);
    let c = Context::discover(repo).unwrap();
    execute(&c, &picked, Kind::CherryPick, "");
    assert_eq!(git(repo, &["rev-parse", "HEAD^"]), head);
    assert_eq!(git(repo, &["show", "HEAD:old/added"]), "Selected addition");
    assert_eq!(
        fs::read_to_string(repo.join("new/added")).unwrap(),
        "Ignored local content"
    );
    assert_eq!(git(repo, &["show", "HEAD:new/existing"]), "Original file");
}

#[test]
fn skip_worktree_and_assume_unchanged_paths_cannot_hide_local_overwrites() {
    for flag in ["--skip-worktree", "--assume-unchanged"] {
        for after_review in [false, true] {
            let (dir, c, picked, head) = fixture("base");
            let repo = dir.path();
            let review =
                after_review.then(|| prepare(&c, &picked, Kind::CherryPick, "").unwrap().0);
            git(repo, &["update-index", flag, "base"]);
            fs::write(repo.join("base"), "Hidden local edit").unwrap();
            let before = index(repo);
            let failure = if let Some(review) = review {
                format!("{:#}", review.execute(&c).unwrap_err())
            } else {
                error(prepare(&c, &picked, Kind::CherryPick, ""))
            };
            assert!(failure.contains("may be overwritten"), "{failure}");
            assert_eq!(git(repo, &["rev-parse", "HEAD"]), head);
            assert_eq!(index(repo), before);
            assert_eq!(
                fs::read_to_string(repo.join("base")).unwrap(),
                "Hidden local edit"
            );
        }
    }
}

#[test]
fn sparse_index_paths_require_review_after_restoring_normal_checkout() {
    let dir = init();
    let repo = dir.path();
    fs::create_dir_all(repo.join("hidden")).unwrap();
    fs::create_dir_all(repo.join("visible")).unwrap();
    commit(repo, "hidden/file", "Original hidden file");
    commit(repo, "visible/file", "Visible file");
    git(repo, &["switch", "-c", "source"]);
    commit(repo, "hidden/file", "Selected hidden edit");
    let picked = git(repo, &["rev-parse", "HEAD"]);
    git(repo, &["switch", "main"]);
    let head = git(repo, &["rev-parse", "HEAD"]);
    let c = Context::discover(repo).unwrap();
    let (review, _) = prepare(&c, &picked, Kind::CherryPick, "").unwrap();
    git(
        repo,
        &["sparse-checkout", "init", "--cone", "--sparse-index"],
    );
    git(repo, &["sparse-checkout", "set", "visible"]);
    let before = index(repo);
    assert!(!repo.join("hidden/file").exists());
    assert!(error(prepare(&c, &picked, Kind::CherryPick, "")).contains("may be overwritten"));
    assert!(format!("{:#}", review.execute(&c).unwrap_err()).contains("may be overwritten"));
    assert_eq!(git(repo, &["rev-parse", "HEAD"]), head);
    assert_eq!(index(repo), before);
    assert!(!repo.join("hidden/file").exists());
    assert_eq!(
        fs::read_to_string(repo.join("visible/file")).unwrap(),
        "Visible file"
    );
}

#[test]
fn merge_commits_are_refused_without_mainline_selection() {
    let (dir, c, picked, head) = fixture("picked");
    let repo = dir.path();
    git(repo, &["merge", "--no-ff", "-qm", "Merge source", "source"]);
    let merge = git(repo, &["rev-parse", "HEAD"]);
    git(repo, &["switch", "-c", "destination", &head]);
    let before = index(repo);
    assert!(error(prepare(&c, &merge, Kind::CherryPick, "")).contains("mainline parent"));
    assert_eq!(git(repo, &["rev-parse", "HEAD"]), head);
    assert_eq!(index(repo), before);
    assert!(prepare(&c, &picked, Kind::CherryPick, "").is_ok());
}

#[test]
fn a_root_commit_can_be_picked_without_replaying_its_descendants() {
    let (dir, c, [base, _, head]) = history();
    let repo = dir.path();
    git(repo, &["switch", "--orphan", "destination"]);
    commit(repo, "destination", "Independent history");
    let destination = git(repo, &["rev-parse", "HEAD"]);
    execute(&c, &base, Kind::CherryPick, "");
    assert_eq!(git(repo, &["rev-parse", "HEAD^"]), destination);
    assert_eq!(git(repo, &["rev-parse", "refs/heads/main"]), head);
    assert_eq!(
        git(repo, &["ls-tree", "--name-only", "HEAD"]),
        "base\ndestination"
    );
    assert!(git(repo, &["status", "--porcelain"]).is_empty());
}

#[test]
fn conflict_state_stays_native_for_a_separate_continue_or_abort() {
    for abort in [false, true] {
        let (dir, c, picked, _) = fixture("picked");
        let repo = dir.path();
        commit(repo, "picked", "Conflicting destination content");
        let head = git(repo, &["rev-parse", "HEAD"]);
        let (stale, _) = prepare(&c, &picked, Kind::CherryPick, "").unwrap();
        let (review, _) = prepare(&c, &picked, Kind::CherryPick, "").unwrap();
        assert!(
            format!("{:#}", review.execute(&c).unwrap_err()).contains("Cherry-pick Commit failed")
        );
        assert_eq!(git(repo, &["rev-parse", "CHERRY_PICK_HEAD"]), picked);
        assert_eq!(git(repo, &["rev-parse", "HEAD"]), head);
        assert!(!git(repo, &["ls-files", "--unmerged"]).is_empty());
        let conflict_index = index(repo);
        let conflict_file = fs::read(repo.join("picked")).unwrap();
        assert!(error(prepare(&c, &picked, Kind::CherryPick, "")).contains("current Git operation"));
        assert!(format!("{:#}", stale.execute(&c).unwrap_err()).contains("current Git operation"));
        assert_eq!(index(repo), conflict_index);
        assert_eq!(fs::read(repo.join("picked")).unwrap(), conflict_file);
        assert_eq!(git(repo, &["show", ":base"]), "Base content");
        assert_eq!(git(repo, &["show", ":destination"]), "Destination content");
        if abort {
            git(repo, &["cherry-pick", "--abort"]);
            assert_eq!(git(repo, &["rev-parse", "HEAD"]), head);
            assert_eq!(
                fs::read_to_string(repo.join("picked")).unwrap(),
                "Conflicting destination content"
            );
        } else {
            fs::write(repo.join("picked"), "Reviewed resolution").unwrap();
            git(repo, &["add", "picked"]);
            assert_eq!(git(repo, &["rev-parse", "HEAD"]), head);
            assert_eq!(git(repo, &["rev-parse", "CHERRY_PICK_HEAD"]), picked);
            assert!(git(repo, &["ls-files", "--unmerged"]).is_empty());
            assert!(
                error(prepare(&c, &picked, Kind::CherryPick, "")).contains("current Git operation")
            );
            git(repo, &["cherry-pick", "--continue"]);
            assert_eq!(git(repo, &["rev-parse", "HEAD^"]), head);
            assert_eq!(git(repo, &["show", "HEAD:picked"]), "Reviewed resolution");
        }
        assert!(!repo.join(".git/CHERRY_PICK_HEAD").exists());
        assert!(git(repo, &["status", "--porcelain"]).is_empty());
    }
}

#[test]
fn recorded_rerere_resolutions_do_not_automatically_stage_conflicts() {
    let (dir, c, picked, _) = fixture("picked");
    let repo = dir.path();
    commit(repo, "picked", "Conflicting destination content");
    git(repo, &["config", "rerere.enabled", "true"]);
    git(repo, &["config", "rerere.autoupdate", "true"]);
    let output = command(repo)
        .args([
            "cherry-pick",
            "--no-edit",
            "--no-rerere-autoupdate",
            &picked,
        ])
        .output()
        .unwrap();
    assert!(!output.status.success());
    fs::write(repo.join("picked"), "Previously recorded resolution").unwrap();
    git(repo, &["rerere"]);
    git(repo, &["add", "picked"]);
    git(repo, &["cherry-pick", "--abort"]);
    let head = git(repo, &["rev-parse", "HEAD"]);
    let (review, _) = prepare(&c, &picked, Kind::CherryPick, "").unwrap();
    assert!(review.execute(&c).is_err());
    assert_eq!(git(repo, &["rev-parse", "HEAD"]), head);
    assert_eq!(git(repo, &["rev-parse", "CHERRY_PICK_HEAD"]), picked);
    assert!(!git(repo, &["ls-files", "--unmerged"]).is_empty());
    assert_eq!(
        fs::read_to_string(repo.join("picked")).unwrap(),
        "Previously recorded resolution"
    );
    git(repo, &["cherry-pick", "--abort"]);
}

#[test]
fn submodule_gitlinks_change_without_recursing_into_the_child_worktree_or_index() {
    let child = init();
    commit(child.path(), "file", "Child original");
    let old_child = git(child.path(), &["rev-parse", "HEAD"]);
    let dir = init();
    let repo = dir.path();
    git(repo, &["config", "commit.gpgsign", "false"]);
    commit(repo, "base", "Parent base");
    git(
        repo,
        &[
            "-c",
            "protocol.file.allow=always",
            "submodule",
            "add",
            "-q",
            child.path().to_str().unwrap(),
            "child",
        ],
    );
    git(repo, &["commit", "-qam", "Add child"]);
    git(repo, &["switch", "-c", "source"]);
    commit(child.path(), "file", "Child new commit");
    let new_child = git(child.path(), &["rev-parse", "HEAD"]);
    let worktree = repo.join("child");
    git(&worktree, &["fetch", "-q"]);
    git(&worktree, &["switch", "--detach", &new_child]);
    git(repo, &["add", "child"]);
    git(repo, &["commit", "-qm", "Update child gitlink"]);
    let picked = git(repo, &["rev-parse", "HEAD"]);
    git(repo, &["switch", "main"]);
    git(&worktree, &["switch", "--detach", &old_child]);
    fs::write(worktree.join("file"), "Child staged version").unwrap();
    git(&worktree, &["add", "file"]);
    fs::write(worktree.join("file"), "Child working version").unwrap();
    let child_index = index(&worktree);
    git(repo, &["config", "submodule.recurse", "true"]);
    git(repo, &["config", "merge.autoStash", "true"]);
    let c = Context::discover(repo).unwrap();
    assert_eq!(c.repos[0].path, fs::canonicalize(repo).unwrap());
    execute(&c, &picked, Kind::CherryPick, "");
    assert_eq!(git(repo, &["rev-parse", "HEAD:child"]), new_child);
    assert_eq!(git(&worktree, &["rev-parse", "HEAD"]), old_child);
    assert_eq!(index(&worktree), child_index);
    assert_eq!(git(&worktree, &["show", ":file"]), "Child staged version");
    assert_eq!(
        fs::read_to_string(worktree.join("file")).unwrap(),
        "Child working version"
    );
}
