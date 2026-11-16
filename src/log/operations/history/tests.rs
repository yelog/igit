use super::*;
use crate::log::tests::{commit, git as run_git, init};
use tempfile::TempDir;

mod editor;

fn fixture() -> (TempDir, Context, [String; 3]) {
    let dir = init();
    let repo = dir.path();
    for (key, value) in [
        ("core.autocrlf", "false"),
        ("core.eol", "lf"),
        ("commit.gpgsign", "false"),
        ("rerere.enabled", "false"),
    ] {
        run_git(repo, &["config", key, value]);
    }
    let ids = ["base", "first", "second"].map(|name| {
        commit(repo, name, name);
        run_git(repo, &["rev-parse", "HEAD"])
    });
    let context = Context::discover(repo).unwrap();
    (dir, context, ids)
}

fn draft(c: &Context, oid: &str) -> Result<Draft> {
    Draft::read(
        c,
        Target {
            root: 0,
            path: c.repos[0].path.clone(),
            oid: oid.to_owned(),
            subject: "Selected commit".into(),
        },
    )
}

fn fail<T>(result: Result<T>) -> String {
    format!("{:#}", result.err().expect("operation must be refused"))
}

fn backups(repo: &Path) -> String {
    run_git(
        repo,
        &["for-each-ref", "--format=%(refname)", "refs/igit/backups"],
    )
}

fn snapshot(repo: &Path) -> (String, Vec<u8>, Vec<u8>, Vec<u8>) {
    (
        run_git(repo, &["rev-parse", "HEAD"]),
        fs::read(repo.join(".git/index")).unwrap(),
        refs(repo).unwrap(),
        git_bytes(
            repo,
            &["diff", "--no-ext-diff", "--no-textconv", "HEAD", "--"],
        )
        .unwrap(),
    )
}

#[test]
fn squash_uses_reviewed_multiline_message_and_preserves_other_refs_and_data() {
    let (dir, c, [base, first, head]) = fixture();
    let repo = dir.path();
    run_git(repo, &["branch", "keep-first", &first]);
    run_git(repo, &["branch", "keep-tip", &head]);
    run_git(repo, &["tag", "keep-tag", &head]);
    run_git(repo, &["update-ref", "refs/remotes/origin/main", &base]);
    for (key, value) in [
        ("rebase.autostash", "true"),
        ("rebase.updateRefs", "true"),
        ("rebase.autosquash", "true"),
        ("rebase.forkPoint", "true"),
        ("rebase.rebaseMerges", "true"),
        ("rebase.missingCommitsCheck", "error"),
        ("merge.directoryRenames", "true"),
        ("merge.renormalize", "true"),
        ("commit.gpgSign", "true"),
    ] {
        run_git(repo, &["config", key, value]);
    }
    fs::write(repo.join(".git/info/exclude"), "ignored.log\n").unwrap();
    fs::write(repo.join("ignored.log"), b"ignored\0bytes").unwrap();
    fs::write(repo.join("untracked"), b"untracked\0bytes").unwrap();
    let base_bytes = fs::read(repo.join("base")).unwrap();
    let base_index = run_git(repo, &["ls-files", "--stage", "--", "base"]);
    let original_tree = run_git(repo, &["rev-parse", "HEAD^{tree}"]);
    let message = "Combined 中文\n\n# literal body\n  keep indentation\n$(touch message-ran)";
    let mut plan = draft(&c, &first).unwrap();
    plan.set_step(1, Step::Squash).unwrap();
    plan.entries[1].message = message.into();
    let before = snapshot(repo);
    let (review, body) = Reviewed::prepare(&c, &plan).unwrap();
    assert!(body.contains("Branch: refs/heads/main"));
    assert!(body.contains(&format!("1. pick {first}")));
    assert!(body.contains(&format!("2. squash {head}")));
    assert!(body.contains(message));
    assert!(body.contains("Affected paths across the entire plan:\n  first\n  second\n"));
    assert!(body.contains("Other branches/tags") && body.contains("remove their separate IDs"));
    let backup = review.backup.clone();
    assert!(body.contains(&backup));
    assert!(backups(repo).is_empty());
    assert_eq!(snapshot(repo), before);
    let (level, notice) = review.execute(&c).unwrap();
    assert_eq!(level, NoticeLevel::Success, "{notice}");
    assert!(notice.contains(&backup));
    assert_eq!(run_git(repo, &["rev-list", "--count", "HEAD"]), "2");
    assert_eq!(run_git(repo, &["rev-parse", "HEAD^"]), base);
    assert_eq!(run_git(repo, &["rev-parse", "HEAD^{tree}"]), original_tree);
    assert_eq!(super::message(repo, "HEAD").unwrap(), message);
    assert!(!repo.join("message-ran").exists());
    for (reference, oid) in [
        ("keep-first", first.as_str()),
        ("keep-tip", head.as_str()),
        ("keep-tag", head.as_str()),
        ("refs/remotes/origin/main", base.as_str()),
        (backup.as_str(), head.as_str()),
    ] {
        assert_eq!(run_git(repo, &["rev-parse", reference]), oid);
    }
    assert_eq!(fs::read(repo.join("base")).unwrap(), base_bytes);
    assert_eq!(
        run_git(repo, &["ls-files", "--stage", "--", "base"]),
        base_index
    );
    assert_eq!(
        fs::read(repo.join("ignored.log")).unwrap(),
        b"ignored\0bytes"
    );
    assert_eq!(
        fs::read(repo.join("untracked")).unwrap(),
        b"untracked\0bytes"
    );
    assert!(
        git_bytes(repo, &["status", "--porcelain", "--untracked-files=no"])
            .unwrap()
            .is_empty()
    );
}

#[test]
fn fixup_retains_destination_message_and_grouped_fixup_retains_edited_squash_message() {
    for squash in [false, true] {
        let (dir, c, [_, first, _]) = fixture();
        let repo = dir.path();
        commit(repo, "third", "Third message must be discarded");
        let mut plan = draft(&c, &first).unwrap();
        let expected = if squash {
            plan.set_step(1, Step::Squash).unwrap();
            plan.entries[1].message = "Reviewed combined message\n\nBody".into();
            "Reviewed combined message\n\nBody"
        } else {
            plan.set_step(1, Step::Fixup).unwrap();
            "first"
        };
        plan.set_step(2, Step::Fixup).unwrap();
        let (review, body) = Reviewed::prepare(&c, &plan).unwrap();
        assert!(body.contains(expected));
        assert!(body.contains("3. fixup"));
        let (level, notice) = review.execute(&c).unwrap();
        assert_eq!(level, NoticeLevel::Success, "{notice}");
        assert_eq!(super::message(repo, "HEAD").unwrap(), expected);
        assert_eq!(run_git(repo, &["rev-list", "--count", "HEAD"]), "2");
        assert!(
            repo.join("first").exists()
                && repo.join("second").exists()
                && repo.join("third").exists()
        );
    }
}

#[test]
fn reorder_replays_exact_order_without_autosquashing_fixup_subjects() {
    let (dir, c, [_, first, head]) = fixture();
    let repo = dir.path();
    commit(repo, "third", "fixup! first");
    run_git(repo, &["config", "rebase.autosquash", "true"]);
    let original_tree = run_git(repo, &["rev-parse", "HEAD^{tree}"]);
    let mut plan = draft(&c, &first).unwrap();
    plan.move_by(1, -1).unwrap();
    let (review, body) = Reviewed::prepare(&c, &plan).unwrap();
    assert!(
        body.find(&format!("1. pick {head}")).unwrap()
            < body.find(&format!("2. pick {first}")).unwrap()
    );
    let (level, notice) = review.execute(&c).unwrap();
    assert_eq!(level, NoticeLevel::Success, "{notice}");
    assert_eq!(
        run_git(repo, &["log", "--reverse", "--format=%s"]),
        "base\nsecond\nfirst\nfixup! first"
    );
    assert_eq!(run_git(repo, &["rev-parse", "HEAD^{tree}"]), original_tree);
    assert_eq!(run_git(repo, &["rev-list", "--count", "HEAD"]), "4");
}

#[test]
fn cancel_drops_plan_and_review_without_touching_head_index_worktree_or_refs() {
    let (dir, c, [_, first, _]) = fixture();
    let before = snapshot(dir.path());
    let mut plan = draft(&c, &first).unwrap();
    plan.set_step(1, Step::Fixup).unwrap();
    drop(Reviewed::prepare(&c, &plan).unwrap());
    drop(plan);
    assert_eq!(snapshot(dir.path()), before);
    assert!(backups(dir.path()).is_empty());
}

#[test]
fn stale_review_refuses_branch_head_publication_index_config_and_scope_changes() {
    for mutation in [
        "branch",
        "head",
        "published",
        "remote",
        "index",
        "working",
        "config",
        "scope",
        "path",
    ] {
        let (dir, mut c, [base, first, head]) = fixture();
        let repo = dir.path();
        let mut plan = draft(&c, &first).unwrap();
        plan.set_step(1, Step::Fixup).unwrap();
        let (review, _) = Reviewed::prepare(&c, &plan).unwrap();
        match mutation {
            "branch" => {
                run_git(repo, &["switch", "-c", "external"]);
            }
            "head" => {
                commit(repo, "later", "External later commit");
            }
            "published" => {
                run_git(repo, &["update-ref", "refs/remotes/origin/main", &head]);
            }
            "remote" => {
                run_git(repo, &["update-ref", "refs/remotes/origin/main", &base]);
            }
            "index" => {
                fs::write(repo.join("base"), "staged after review").unwrap();
                run_git(repo, &["add", "base"]);
                fs::write(repo.join("base"), "working after staging").unwrap();
            }
            "working" => fs::write(repo.join("base"), "working after review").unwrap(),
            "config" => {
                run_git(repo, &["config", "core.autocrlf", "true"]);
            }
            "scope" => {
                c.scope.remove(&0);
            }
            "path" => c.repos[0].path = repo.join("missing"),
            _ => unreachable!(),
        }
        let before = snapshot(repo);
        let error = fail(review.execute(&c));
        assert!(
            error.contains("review again")
                || error.contains("outside Roots")
                || error.contains("Workspace changed"),
            "{mutation}: {error}"
        );
        assert_eq!(snapshot(repo), before, "{mutation}");
        assert!(backups(repo).is_empty(), "{mutation}");
    }
}

#[test]
fn published_merge_root_detached_and_active_operation_are_refused() {
    let (dir, c, [base, first, head]) = fixture();
    let repo = dir.path();
    assert!(fail(draft(&c, &base)).contains("non-root"));
    assert!(fail(draft(&c, &head)).contains("between 2"));
    run_git(repo, &["update-ref", "refs/remotes/origin/main", &first]);
    assert!(fail(draft(&c, &first)).contains("remote-tracking branch"));
    run_git(repo, &["update-ref", "-d", "refs/remotes/origin/main"]);
    run_git(repo, &["switch", "--detach", &head]);
    assert!(fail(draft(&c, &first)).contains("local branch"));
    run_git(repo, &["switch", "main"]);
    fs::write(repo.join(".git/CHERRY_PICK_HEAD"), format!("{first}\n")).unwrap();
    assert!(fail(draft(&c, &first)).contains("current Git operation"));
    fs::remove_file(repo.join(".git/CHERRY_PICK_HEAD")).unwrap();
    run_git(repo, &["switch", "-c", "side", &base]);
    commit(repo, "side", "Side change");
    run_git(repo, &["switch", "main"]);
    run_git(repo, &["merge", "--no-ff", "-qm", "Merge side", "side"]);
    assert!(fail(draft(&c, &first)).contains("Merge topology"));
    assert!(backups(repo).is_empty());
}

#[test]
fn dirty_tracked_state_is_rejected_even_when_autostash_is_enabled() {
    for staged in [false, true] {
        let (dir, c, [_, first, _]) = fixture();
        let repo = dir.path();
        run_git(repo, &["config", "rebase.autostash", "true"]);
        fs::write(repo.join("base"), "staged state").unwrap();
        if staged {
            run_git(repo, &["add", "base"]);
            fs::write(repo.join("base"), "working state").unwrap();
        }
        let before = snapshot(repo);
        assert!(fail(draft(&c, &first)).contains("tracked working tree and index"));
        assert_eq!(snapshot(repo), before);
        assert!(backups(repo).is_empty());
    }
}

#[test]
fn ignored_and_flagged_path_collisions_are_rechecked_before_backup_or_rebase() {
    for obstruction in ["ignored", "untracked", "skip", "assume"] {
        let (dir, c, [_, first, _]) = fixture();
        let repo = dir.path();
        if matches!(obstruction, "ignored" | "untracked") {
            run_git(repo, &["rm", "first"]);
            run_git(repo, &["commit", "-qm", "Remove first"]);
        }
        let mut plan = draft(&c, &first).unwrap();
        plan.set_step(1, Step::Fixup).unwrap();
        let (review, _) = Reviewed::prepare(&c, &plan).unwrap();
        match obstruction {
            "ignored" => fs::write(repo.join(".git/info/exclude"), "first\n").unwrap(),
            "skip" => {
                run_git(repo, &["update-index", "--skip-worktree", "--", "first"]);
            }
            "assume" => {
                run_git(repo, &["update-index", "--assume-unchanged", "--", "first"]);
            }
            _ => (),
        }
        fs::write(repo.join("first"), "private local bytes").unwrap();
        let head = run_git(repo, &["rev-parse", "HEAD"]);
        let index = fs::read(repo.join(".git/index")).unwrap();
        assert!(fail(review.execute(&c)).contains("may be overwritten"));
        assert_eq!(run_git(repo, &["rev-parse", "HEAD"]), head);
        assert_eq!(fs::read(repo.join(".git/index")).unwrap(), index);
        assert_eq!(
            fs::read(repo.join("first")).unwrap(),
            b"private local bytes"
        );
        assert!(backups(repo).is_empty());
    }
}

#[test]
fn historical_and_working_attributes_are_rejected_without_running_filters() {
    for historical in [false, true] {
        let (dir, c, [_, first, _]) = fixture();
        let repo = dir.path();
        let attributes = "first filter=unsafe\n";
        if historical {
            fs::write(repo.join(".gitattributes"), attributes).unwrap();
            run_git(repo, &["add", ".gitattributes"]);
            run_git(repo, &["commit", "-qm", "Historical conversion"]);
            run_git(repo, &["rm", ".gitattributes"]);
            run_git(repo, &["commit", "-qm", "Remove conversion"]);
        } else {
            fs::write(repo.join(".gitattributes"), attributes).unwrap();
        }
        for setting in [
            "filter.unsafe.clean",
            "filter.unsafe.smudge",
            "filter.unsafe.process",
        ] {
            run_git(repo, &["config", setting, "touch filter-ran; cat"]);
        }
        let head = run_git(repo, &["rev-parse", "HEAD"]);
        let index = fs::read(repo.join(".git/index")).unwrap();
        assert!(fail(draft(&c, &first)).contains("filter attribute"));
        assert!(!repo.join("filter-ran").exists());
        assert_eq!(run_git(repo, &["rev-parse", "HEAD"]), head);
        assert_eq!(fs::read(repo.join(".git/index")).unwrap(), index);
        assert!(backups(repo).is_empty());
    }
}

#[test]
fn changed_attributes_expire_a_prepared_review_before_filter_execution() {
    let (dir, c, [_, first, _]) = fixture();
    let repo = dir.path();
    let mut plan = draft(&c, &first).unwrap();
    plan.set_step(1, Step::Fixup).unwrap();
    let (review, _) = Reviewed::prepare(&c, &plan).unwrap();
    fs::write(repo.join(".git/info/attributes"), "first filter=unsafe\n").unwrap();
    run_git(
        repo,
        &["config", "filter.unsafe.clean", "touch filter-ran; cat"],
    );
    assert!(fail(review.execute(&c)).contains("filter attribute"));
    assert!(!repo.join("filter-ran").exists());
    assert!(backups(repo).is_empty());
}

#[test]
fn conversions_in_any_historical_tree_and_current_rules_are_refused() {
    for rule in [
        "* text=auto",
        "* eol=crlf",
        "* working-tree-encoding=UTF-16",
        "* ident",
        "* merge=custom",
    ] {
        let (dir, c, [_, first, head]) = fixture();
        let repo = dir.path();
        fs::write(repo.join(".git/info/attributes"), format!("{rule}\n")).unwrap();
        let index = fs::read(repo.join(".git/index")).unwrap();
        assert!(fail(draft(&c, &first)).contains("attribute"));
        assert_eq!(run_git(repo, &["rev-parse", "HEAD"]), head);
        assert_eq!(fs::read(repo.join(".git/index")).unwrap(), index);
        assert!(backups(repo).is_empty());
    }
}

#[test]
fn unselected_hidden_working_data_and_index_flags_survive_cleanup() {
    for flag in ["--assume-unchanged", "--skip-worktree"] {
        let (dir, c, [_, first, _]) = fixture();
        let repo = dir.path();
        run_git(repo, &["update-index", flag, "--", "base"]);
        fs::write(repo.join("base"), "unselected hidden bytes").unwrap();
        let entry = run_git(repo, &["ls-files", "--stage", "-v", "--", "base"]);
        let mut plan = draft(&c, &first).unwrap();
        plan.set_step(1, Step::Fixup).unwrap();
        let (level, notice) = Reviewed::prepare(&c, &plan).unwrap().0.execute(&c).unwrap();
        assert_eq!(level, NoticeLevel::Success, "{notice}");
        assert_eq!(
            run_git(repo, &["ls-files", "--stage", "-v", "--", "base"]),
            entry
        );
        assert_eq!(
            fs::read(repo.join("base")).unwrap(),
            b"unselected hidden bytes"
        );
    }
}

#[test]
fn todo_has_only_reviewed_oids_and_fixed_message_restore_commands() {
    let (_dir, c, [_, first, head]) = fixture();
    let mut plan = draft(&c, &first).unwrap();
    plan.set_step(1, Step::Squash).unwrap();
    plan.entries[1].message = "Literal $(touch message-ran)\n\n# keep".into();
    let (review, _) = Reviewed::prepare(&c, &plan).unwrap();
    let todo = review.todo().unwrap();
    let lines: Vec<_> = todo.lines().collect();
    assert_eq!(lines[0], format!("pick {first}"));
    assert_eq!(lines[1], format!("fixup {head}"));
    assert_eq!(lines.len(), 3);
    assert!(lines[2].starts_with("exec GIT_NO_REPLACE_OBJECTS=1 GIT_GRAFT_FILE= git "));
    assert!(lines[2].contains("--amend --only"));
    assert!(lines[2].contains("--cleanup=verbatim -C "));
    assert!(!todo.contains("message-ran") && !todo.contains("Literal"));
}

fn conflicting() -> (TempDir, Context, String, String) {
    let dir = init();
    let repo = dir.path();
    run_git(repo, &["config", "core.autocrlf", "false"]);
    run_git(repo, &["config", "commit.gpgSign", "false"]);
    run_git(repo, &["config", "rerere.enabled", "false"]);
    commit(repo, "file", "base");
    commit(repo, "file", "first");
    let first = run_git(repo, &["rev-parse", "HEAD"]);
    commit(repo, "file", "second");
    let head = run_git(repo, &["rev-parse", "HEAD"]);
    let c = Context::discover(repo).unwrap();
    (dir, c, first, head)
}

#[test]
fn conflict_keeps_native_state_backup_and_persistent_message_for_continue() {
    let (dir, c, first, head) = conflicting();
    let repo = dir.path();
    let mut plan = draft(&c, &first).unwrap();
    plan.move_by(1, -1).unwrap();
    plan.set_step(1, Step::Squash).unwrap();
    let message = "Reviewed conflict squash\n\n# literal";
    plan.entries[1].message = message.into();
    let (review, _) = Reviewed::prepare(&c, &plan).unwrap();
    let backup = review.backup.clone();
    let (level, notice) = review.execute(&c).unwrap();
    assert_eq!(level, NoticeLevel::Warning, "{notice}");
    assert!(notice.contains("Conflicts: review Continue or Abort"));
    assert!(repo.join(".git/rebase-merge").is_dir());
    assert!(!git_bytes(repo, &["ls-files", "--unmerged", "-z"])
        .unwrap()
        .is_empty());
    assert_eq!(run_git(repo, &["rev-parse", &backup]), head);
    assert_eq!(run_git(repo, &["rev-parse", "refs/heads/main"]), head);
    assert!(!repo.join(".git/rebase-merge/autostash").exists());
    let mut stops = 0;
    while repo.join(".git/rebase-merge").is_dir() {
        stops += 1;
        assert!(
            stops <= 3,
            "rebase did not complete after resolving conflicts"
        );
        fs::write(
            repo.join("file"),
            if stops == 1 { "second" } else { "combined" },
        )
        .unwrap();
        run_git(repo, &["add", "file"]);
        let output = command(repo)
            .args(["rebase", "--continue"])
            .output()
            .unwrap();
        assert!(
            output.status.success()
                || !git_bytes(repo, &["ls-files", "--unmerged", "-z"])
                    .unwrap()
                    .is_empty(),
            "{}",
            String::from_utf8_lossy(&output.stderr),
        );
    }
    assert_eq!(super::message(repo, "HEAD").unwrap(), message);
    assert_eq!(run_git(repo, &["symbolic-ref", "HEAD"]), "refs/heads/main");
    assert_eq!(run_git(repo, &["rev-list", "--count", "HEAD"]), "2");
    assert_eq!(run_git(repo, &["rev-parse", &backup]), head);
    assert!(!repo.join(".git/rebase-merge").exists());
}

#[test]
fn conflict_can_be_aborted_using_native_recovery_without_moving_other_refs() {
    let (dir, c, first, head) = conflicting();
    let repo = dir.path();
    run_git(repo, &["branch", "keep-original", &head]);
    let mut plan = draft(&c, &first).unwrap();
    plan.move_by(1, -1).unwrap();
    let (review, _) = Reviewed::prepare(&c, &plan).unwrap();
    let backup = review.backup.clone();
    let (level, _) = review.execute(&c).unwrap();
    assert_eq!(level, NoticeLevel::Warning);
    let output = command(repo).args(["rebase", "--abort"]).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(run_git(repo, &["symbolic-ref", "HEAD"]), "refs/heads/main");
    assert_eq!(run_git(repo, &["rev-parse", "HEAD"]), head);
    assert_eq!(run_git(repo, &["rev-parse", "keep-original"]), head);
    assert_eq!(run_git(repo, &["rev-parse", &backup]), head);
    assert_eq!(fs::read(repo.join("file")).unwrap(), b"second");
    assert!(run_git(repo, &["status", "--porcelain"]).is_empty());
}

#[test]
fn cleanup_writes_only_the_selected_repository_and_preserves_unrelated_staged_data() {
    let (dir, mut c, [_, first, _]) = fixture();
    let other = init();
    commit(other.path(), "file", "Other repository");
    fs::write(other.path().join("file"), "staged in unselected repository").unwrap();
    run_git(other.path(), &["add", "file"]);
    fs::write(
        other.path().join("file"),
        "working in unselected repository",
    )
    .unwrap();
    let other_context = Context::discover(other.path()).unwrap();
    c.repos.push(other_context.repos[0].clone());
    c.scope.insert(1);
    let before = snapshot(other.path());
    let mut plan = draft(&c, &first).unwrap();
    plan.set_step(1, Step::Fixup).unwrap();
    let (level, notice) = Reviewed::prepare(&c, &plan).unwrap().0.execute(&c).unwrap();
    assert_eq!(level, NoticeLevel::Success, "{notice}");
    assert_eq!(snapshot(other.path()), before);
    assert!(backups(other.path()).is_empty());
    assert_eq!(run_git(dir.path(), &["rev-list", "--count", "HEAD"]), "2");
}

#[test]
fn invalid_first_fold_duplicate_oid_and_unchanged_plan_are_refused() {
    let (_dir, c, [_, first, _]) = fixture();
    let mut plan = draft(&c, &first).unwrap();
    assert!(fail(Reviewed::prepare(&c, &plan)).contains("unchanged"));
    assert!(fail(plan.set_step(0, Step::Squash)).contains("first row"));
    plan.set_step(1, Step::Fixup).unwrap();
    assert!(fail(plan.move_by(1, -1)).contains("first row"));
    plan.entries[1].oid = first;
    assert!(fail(Reviewed::prepare(&c, &plan)).contains("exactly once"));
}
