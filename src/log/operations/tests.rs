use super::*;
use crate::log::tests::{commit, git, init};
use std::fs;
use tempfile::TempDir;

mod cherry_pick;
mod reword;
mod tip;

fn history() -> (TempDir, Context, [String; 3]) {
    let dir = init();
    let repo = dir.path();
    git(repo, &["config", "commit.gpgsign", "false"]);
    let ids = ["base", "middle", "later"].map(|name| {
        commit(repo, name, name);
        git(repo, &["rev-parse", "HEAD"])
    });
    let c = Context::discover(repo).unwrap();
    (dir, c, ids)
}

fn prepare(c: &Context, oid: &str, kind: Kind, value: &str) -> Result<(Review, String)> {
    let target = Target::new(
        c,
        &Rev {
            root: 0,
            oid: oid.into(),
            subject: "Selected commit".into(),
            ..Default::default()
        },
    )?;
    Review::prepare(c, target, kind, value.into())
}

fn execute(c: &Context, oid: &str, kind: Kind, value: &str) {
    prepare(c, oid, kind, value).unwrap().0.execute(c).unwrap();
}

fn error(result: Result<(Review, String)>) -> String {
    format!("{:#}", result.err().expect("review should be refused"))
}

#[test]
fn refs_and_checkout_use_the_selected_object_without_forcing() {
    let (dir, c, [base, _, head]) = history();
    let repo = dir.path();
    fs::write(repo.join("base"), "local edit").unwrap();
    git(repo, &["add", "base"]);
    let (branch, _) = prepare(&c, &base, Kind::Branch, "topic/早期").unwrap();
    assert!(git(repo, &["branch", "--list", "topic/早期"]).is_empty());
    branch.execute(&c).unwrap();
    git(repo, &["config", "tag.gpgSign", "true"]);
    execute(&c, &base, Kind::Tag, "archive/初版");
    assert_eq!(git(repo, &["rev-parse", "refs/heads/topic/早期"]), base);
    assert_eq!(git(repo, &["rev-parse", "refs/tags/archive/初版"]), base);
    assert_eq!(git(repo, &["symbolic-ref", "HEAD"]), "refs/heads/main");
    assert_eq!(git(repo, &["rev-parse", "HEAD"]), head);
    assert!(error(prepare(&c, &head, Kind::Tag, "archive/初版")).contains("already exists"));
    assert!(error(prepare(&c, &base, Kind::Branch, "-oops")).contains("valid name"));

    execute(&c, &base, Kind::Checkout, "");
    assert_eq!(git(repo, &["rev-parse", "HEAD"]), base);
    assert_eq!(git(repo, &["branch", "--show-current"]), "");
    assert_eq!(git(repo, &["rev-parse", "refs/heads/main"]), head);
    assert_eq!(git(repo, &["show", ":base"]), "local edit");
    assert_eq!(fs::read_to_string(repo.join("base")).unwrap(), "local edit");
    assert!(!repo.join("later").exists());
}

fn dirty_index(repo: &Path) -> String {
    fs::write(repo.join("base"), "staged content").unwrap();
    git(repo, &["add", "base"]);
    fs::write(repo.join("base"), "working content").unwrap();
    fs::write(repo.join("untracked"), "untracked content").unwrap();
    git(repo, &["ls-files", "--stage"])
}

#[test]
fn undo_moves_only_the_tip_and_preserves_the_real_index_and_worktree() {
    let (dir, c, [_, middle, head]) = history();
    let repo = dir.path();
    dirty_index(repo);
    fs::write(repo.join("middle"), "Unrelated staged sibling").unwrap();
    git(repo, &["add", "middle"]);
    let index = git(repo, &["ls-files", "--stage"]);
    assert!(error(prepare(&c, &middle, Kind::Undo, "")).contains("current branch tip"));
    execute(&c, &head, Kind::Undo, "");
    assert_eq!(git(repo, &["rev-parse", "HEAD"]), middle);
    assert_eq!(git(repo, &["rev-parse", "ORIG_HEAD"]), head);
    assert_eq!(git(repo, &["ls-files", "--stage"]), index);
    assert_eq!(
        git(repo, &["diff", "--cached", "--name-only"]),
        "base\nlater\nmiddle"
    );
    assert_eq!(git(repo, &["show", ":middle"]), "Unrelated staged sibling");
    assert_eq!(
        fs::read_to_string(repo.join("base")).unwrap(),
        "working content"
    );
    assert_eq!(fs::read_to_string(repo.join("later")).unwrap(), "later");
    assert_eq!(
        fs::read_to_string(repo.join("untracked")).unwrap(),
        "untracked content"
    );
}

#[test]
fn revert_keeps_history_and_creates_a_new_inverse_commit() {
    let (dir, c, [_, middle, head]) = history();
    let repo = dir.path();
    execute(&c, &middle, Kind::Revert, "");
    assert_eq!(git(repo, &["rev-parse", "HEAD^"]), head);
    assert!(git(repo, &["show", "-s", "--format=%B", "HEAD"]).contains(&middle));
    assert_eq!(
        git(repo, &["ls-tree", "--name-only", "HEAD"]),
        "base\nlater"
    );
    assert!(git(repo, &["status", "--porcelain"]).is_empty());
    assert!(!repo.join("middle").exists());
}

#[test]
fn drop_removes_only_the_unpublished_tip_and_leaves_other_refs_in_place() {
    let (dir, c, [_, middle, head]) = history();
    let repo = dir.path();
    git(repo, &["branch", "keep-old", &head]);
    git(repo, &["update-ref", "refs/remotes/origin/main", &middle]);
    fs::write(repo.join(".git/info/exclude"), "local.log\n").unwrap();
    fs::write(repo.join("local.log"), "unrelated").unwrap();
    assert!(error(prepare(&c, &middle, Kind::Drop, "")).contains("current branch tip"));
    let (review, body) = prepare(&c, &head, Kind::Drop, "").unwrap();
    assert!(body.contains("discard its committed changes"));
    review.execute(&c).unwrap();
    assert_eq!(git(repo, &["rev-parse", "HEAD"]), middle);
    assert_eq!(git(repo, &["rev-parse", "ORIG_HEAD"]), head);
    assert_eq!(git(repo, &["log", "-2", "--format=%s"]), "middle\nbase");
    assert_eq!(
        git(repo, &["ls-tree", "--name-only", "HEAD"]),
        "base\nmiddle"
    );
    assert_eq!(git(repo, &["rev-parse", "keep-old"]), head);
    assert_eq!(
        git(repo, &["rev-parse", "refs/remotes/origin/main"]),
        middle
    );
    assert!(!repo.join("later").exists());
    assert_eq!(
        fs::read_to_string(repo.join("local.log")).unwrap(),
        "unrelated"
    );
    assert!(git(repo, &["status", "--porcelain"]).is_empty());
}

#[test]
fn edit_earlier_message_changes_only_messages_and_parent_ids() {
    let (dir, c, [base, middle, head]) = history();
    let repo = dir.path();
    git(repo, &["branch", "keep-middle", &middle]);
    git(
        repo,
        &["commit", "-q", "--allow-empty", "-m", "Empty descendant"],
    );
    let old_tip = git(repo, &["rev-parse", "HEAD"]);
    let value = "新的提交标题\n\n# Keep this line\n  indented body";
    execute(&c, &base, Kind::Edit, value);
    assert_eq!(git(repo, &["rev-list", "--count", "HEAD"]), "4");
    assert_eq!(git(repo, &["rev-parse", "keep-middle"]), middle);
    let revised = git(repo, &["rev-list", "--reverse", "HEAD"]);
    for (i, (oid, original)) in revised
        .lines()
        .zip([base, middle, head, old_tip])
        .enumerate()
    {
        assert_ne!(oid, original);
        assert_eq!(
            git(repo, &["show", "-s", "--format=%T%n%an%n%ae%n%aI", oid]),
            git(
                repo,
                &["show", "-s", "--format=%T%n%an%n%ae%n%aI", &original]
            ),
        );
        let msg = if i == 0 {
            value.to_owned()
        } else {
            message(repo, &original).unwrap()
        };
        assert_eq!(message(repo, oid).unwrap(), msg);
    }
    assert!(git(repo, &["status", "--porcelain"]).is_empty());
}

#[test]
fn edit_tip_message_does_not_commit_any_staged_or_unstaged_changes() {
    let (dir, c, [_, middle, head]) = history();
    let repo = dir.path();
    let index = dirty_index(repo);
    let original_tree = git(repo, &["rev-parse", "HEAD^{tree}"]);
    execute(&c, &head, Kind::Edit, "New subject\n\nBody");
    assert_eq!(git(repo, &["rev-parse", "HEAD^"]), middle);
    assert_eq!(git(repo, &["rev-parse", "HEAD^{tree}"]), original_tree);
    assert_eq!(message(repo, "HEAD").unwrap(), "New subject\n\nBody");
    assert_eq!(git(repo, &["ls-files", "--stage"]), index);
    assert_eq!(
        fs::read_to_string(repo.join("base")).unwrap(),
        "working content"
    );
}

#[test]
fn stale_reviews_do_not_write_and_publication_is_rechecked() {
    let (dir, c, [_, middle, head]) = history();
    let repo = dir.path();
    let (undo, _) = prepare(&c, &head, Kind::Undo, "").unwrap();
    git(repo, &["switch", "-c", "external"]);
    assert!(undo
        .execute(&c)
        .unwrap_err()
        .to_string()
        .contains("HEAD changed"));
    assert_eq!(git(repo, &["rev-parse", "refs/heads/main"]), head);
    assert_eq!(git(repo, &["rev-parse", "refs/heads/external"]), head);
    let (edit, _) = prepare(&c, &middle, Kind::Edit, "Changed").unwrap();
    git(repo, &["update-ref", "refs/remotes/origin/main", &head]);
    assert!(format!("{:#}", edit.execute(&c).unwrap_err()).contains("remote-tracking branch"));
    assert_eq!(git(repo, &["rev-parse", "HEAD"]), head);
    git(repo, &["update-ref", "refs/remotes/origin/main", &middle]);
    let (drop, _) = prepare(&c, &head, Kind::Drop, "").unwrap();
    fs::write(repo.join("base"), "dirty after review").unwrap();
    assert!(format!("{:#}", drop.execute(&c).unwrap_err()).contains("tracked working tree"));
    assert_eq!(git(repo, &["rev-parse", "HEAD"]), head);
}

#[test]
fn revert_and_drop_protect_ignored_paths_and_checkout_defers_to_git() {
    let (dir, c, [_, middle, _]) = history();
    let repo = dir.path();
    git(repo, &["rm", "middle"]);
    git(repo, &["commit", "-qm", "Remove middle"]);
    let head = git(repo, &["rev-parse", "HEAD"]);
    fs::write(repo.join(".git/info/exclude"), "middle\n").unwrap();
    fs::write(repo.join("middle"), "local ignored content").unwrap();
    assert!(error(prepare(&c, &head, Kind::Revert, "")).contains("may be overwritten"));
    assert!(error(prepare(&c, &head, Kind::Drop, "")).contains("may be overwritten"));
    let (checkout, _) = prepare(&c, &middle, Kind::Checkout, "").unwrap();
    assert!(checkout.execute(&c).is_err());
    assert_eq!(git(repo, &["rev-parse", "HEAD"]), head);
    assert_eq!(
        fs::read_to_string(repo.join("middle")).unwrap(),
        "local ignored content"
    );
}

#[test]
fn tip_removal_uses_a_merges_first_parent_and_revert_refuses_merges() {
    for kind in [Kind::Drop, Kind::Undo] {
        let (dir, c, [base, middle, _]) = history();
        let repo = dir.path();
        let main = git(repo, &["rev-parse", "HEAD"]);
        git(repo, &["switch", "-c", "side", &base]);
        commit(repo, "side", "side");
        let side = git(repo, &["rev-parse", "HEAD"]);
        git(repo, &["switch", "main"]);
        git(repo, &["merge", "--no-ff", "-qm", "Merge side", "side"]);
        let merge = git(repo, &["rev-parse", "HEAD"]);
        assert!(error(prepare(&c, &middle, kind, "")).contains("current branch tip"));
        assert!(error(prepare(&c, &merge, Kind::Revert, "")).contains("mainline parent"));
        let (review, body) = prepare(&c, &merge, kind, "").unwrap();
        assert!(body.contains("relative to its first parent"));
        assert_eq!(git(repo, &["rev-parse", "HEAD"]), merge);
        review.execute(&c).unwrap();
        assert_eq!(git(repo, &["rev-parse", "HEAD"]), main);
        assert_eq!(git(repo, &["rev-parse", "side"]), side);
        assert_eq!(repo.join("side").exists(), kind == Kind::Undo);
        assert_eq!(
            git(repo, &["diff", "--cached", "--name-only"]),
            if kind == Kind::Undo { "side" } else { "" }
        );
    }
}

#[test]
fn published_tips_are_refused_on_any_remote_before_and_after_review() {
    for kind in [Kind::Drop, Kind::Undo, Kind::Edit] {
        let (dir, c, [_, middle, head]) = history();
        let repo = dir.path();
        git(repo, &["update-ref", "refs/remotes/origin/main", &middle]);
        let (review, _) = prepare(&c, &head, kind, "Changed").unwrap();
        git(
            repo,
            &["update-ref", "refs/remotes/upstream/release", &head],
        );
        let index = fs::read(repo.join(".git/index")).unwrap();
        assert!(error(prepare(&c, &head, kind, "Changed")).contains("remote-tracking branch"));
        assert!(format!("{:#}", review.execute(&c).unwrap_err()).contains("remote-tracking branch"));
        assert_eq!(git(repo, &["rev-parse", "HEAD"]), head);
        assert_eq!(fs::read(repo.join(".git/index")).unwrap(), index);
        assert_eq!(fs::read_to_string(repo.join("later")).unwrap(), "later");
    }
}

#[test]
fn published_commits_are_revertible_and_revert_conflicts_remain_native() {
    let (dir, c, [_, middle, _]) = history();
    let repo = dir.path();
    commit(
        repo,
        "middle",
        "Edit the file added by the commit to revert",
    );
    let head = git(repo, &["rev-parse", "HEAD"]);
    git(repo, &["update-ref", "refs/remotes/origin/main", &head]);
    let (review, _) = prepare(&c, &middle, Kind::Revert, "").unwrap();
    assert!(review.execute(&c).is_err());
    assert!(repo.join(".git/REVERT_HEAD").is_file());
    assert!(!git(repo, &["ls-files", "--unmerged"]).is_empty());
    assert_eq!(git(repo, &["rev-parse", "refs/heads/main"]), head);
    assert!(error(prepare(&c, &middle, Kind::Checkout, "")).contains("current Git operation"));
    git(repo, &["revert", "--abort"]);
    assert_eq!(git(repo, &["rev-parse", "HEAD"]), head);
    assert!(git(repo, &["status", "--porcelain"]).is_empty());
    execute(&c, &head, Kind::Revert, "");
    assert_eq!(git(repo, &["rev-parse", "HEAD^"]), head);
    assert_eq!(git(repo, &["show", "HEAD:middle"]), "middle");
}

#[test]
fn drop_rechecks_dirty_files_and_index_flags_without_losing_local_content() {
    for change in [
        "staged",
        "unstaged",
        "--skip-worktree",
        "--assume-unchanged",
    ] {
        let (dir, c, [_, _, head]) = history();
        let repo = dir.path();
        let (review, _) = prepare(&c, &head, Kind::Drop, "").unwrap();
        if change.starts_with("--") {
            git(repo, &["update-index", change, "later"]);
        }
        fs::write(repo.join("later"), "Keep local content").unwrap();
        if change == "staged" {
            git(repo, &["add", "later"]);
        }
        let index = fs::read(repo.join(".git/index")).unwrap();
        assert!(prepare(&c, &head, Kind::Drop, "").is_err());
        assert!(review.execute(&c).is_err());
        assert_eq!(git(repo, &["rev-parse", "HEAD"]), head);
        assert_eq!(fs::read(repo.join(".git/index")).unwrap(), index);
        assert_eq!(
            fs::read_to_string(repo.join("later")).unwrap(),
            "Keep local content"
        );
    }
}

#[test]
fn initial_tip_can_be_removed_or_restored_to_staging_without_detaching() {
    for kind in [Kind::Undo, Kind::Drop] {
        let dir = init();
        let repo = dir.path();
        commit(repo, "file", "Initial commit");
        let head = git(repo, &["rev-parse", "HEAD"]);
        fs::write(repo.join("untracked"), "Keep untracked").unwrap();
        let c = Context::discover(repo).unwrap();
        execute(&c, &head, kind, "");
        assert!(
            optional(repo, &["rev-parse", "--verify", "--quiet", "HEAD"])
                .unwrap()
                .is_none()
        );
        assert_eq!(git(repo, &["symbolic-ref", "HEAD"]), "refs/heads/main");
        assert_eq!(git(repo, &["rev-parse", "ORIG_HEAD"]), head);
        assert_eq!(repo.join("file").exists(), kind == Kind::Undo);
        assert_eq!(
            git(repo, &["ls-files"]),
            if kind == Kind::Undo { "file" } else { "" }
        );
        assert_eq!(
            fs::read_to_string(repo.join("untracked")).unwrap(),
            "Keep untracked"
        );
    }
}

#[test]
fn initial_drop_restores_files_if_the_branch_cannot_be_removed() {
    let dir = init();
    let repo = dir.path();
    commit(repo, "file", "Initial commit");
    let head = git(repo, &["rev-parse", "HEAD"]);
    let c = Context::discover(repo).unwrap();
    let (review, _) = prepare(&c, &head, Kind::Drop, "").unwrap();
    let index = fs::read(repo.join(".git/index")).unwrap();
    fs::write(repo.join(".git/refs/heads/main.lock"), "External lock").unwrap();
    assert!(review.execute(&c).is_err());
    assert_eq!(git(repo, &["rev-parse", "HEAD"]), head);
    assert_eq!(fs::read(repo.join(".git/index")).unwrap(), index);
    assert_eq!(git(repo, &["show", ":file"]), "Initial commit");
    assert_eq!(
        fs::read_to_string(repo.join("file")).unwrap(),
        "Initial commit"
    );
    assert!(git(repo, &["status", "--porcelain"]).is_empty());
}

#[test]
fn replacement_refs_cannot_hide_publication_or_change_the_parent_to_remove() {
    for published in [false, true] {
        let (dir, c, [base, middle, head]) = history();
        let repo = dir.path();
        if published {
            git(repo, &["update-ref", "refs/remotes/origin/main", &head]);
            git(repo, &["replace", &head, &base]);
            assert!(error(prepare(&c, &middle, Kind::Edit, "Changed"))
                .contains("remote-tracking branch"));
            assert!(error(prepare(&c, &head, Kind::Drop, "")).contains("remote-tracking branch"));
        } else {
            git(repo, &["replace", &head, &base]);
            execute(&c, &head, Kind::Undo, "");
            assert_eq!(git(repo, &["rev-parse", "HEAD"]), middle);
        }
    }
}
