use super::*;
use crate::log::tests::{commit, git, init};
use std::fs;
use tempfile::TempDir;

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
    let index = dirty_index(repo);
    assert!(error(prepare(&c, &middle, Kind::Undo, "")).contains("current branch tip"));
    execute(&c, &head, Kind::Undo, "");
    assert_eq!(git(repo, &["rev-parse", "HEAD"]), middle);
    assert_eq!(git(repo, &["rev-parse", "ORIG_HEAD"]), head);
    assert_eq!(git(repo, &["ls-files", "--stage"]), index);
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
fn drop_preserves_later_commits_and_leaves_other_refs_in_place() {
    let (dir, c, [base, middle, head]) = history();
    let repo = dir.path();
    git(repo, &["config", "rebase.updateRefs", "true"]);
    git(repo, &["config", "rebase.autoSquash", "true"]);
    git(repo, &["branch", "keep-old", &head]);
    git(repo, &["update-ref", "refs/remotes/origin/main", &head]);
    fs::write(repo.join(".git/info/exclude"), "local.log\n").unwrap();
    fs::write(repo.join("local.log"), "unrelated").unwrap();
    let (review, body) = prepare(&c, &middle, Kind::Drop, "").unwrap();
    assert!(body.contains("remote-tracking branch contains"));
    review.execute(&c).unwrap();
    assert_eq!(git(repo, &["rev-parse", "HEAD^"]), base);
    assert_eq!(git(repo, &["log", "-2", "--format=%s"]), "later\nbase");
    assert_eq!(
        git(repo, &["ls-tree", "--name-only", "HEAD"]),
        "base\nlater"
    );
    assert_eq!(git(repo, &["rev-parse", "keep-old"]), head);
    assert_eq!(git(repo, &["rev-parse", "refs/remotes/origin/main"]), head);
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
    let (drop, _) = prepare(&c, &middle, Kind::Drop, "").unwrap();
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
    assert!(error(prepare(&c, &middle, Kind::Drop, "")).contains("may be overwritten"));
    let (checkout, _) = prepare(&c, &middle, Kind::Checkout, "").unwrap();
    assert!(checkout.execute(&c).is_err());
    assert_eq!(git(repo, &["rev-parse", "HEAD"]), head);
    assert_eq!(
        fs::read_to_string(repo.join("middle")).unwrap(),
        "local ignored content"
    );
}

#[test]
fn replay_refuses_to_flatten_a_merge() {
    let (dir, c, [base, middle, _]) = history();
    let repo = dir.path();
    git(repo, &["switch", "-c", "side", &base]);
    commit(repo, "side", "side");
    git(repo, &["switch", "main"]);
    git(repo, &["merge", "--no-ff", "-qm", "Merge side", "side"]);
    let merge = git(repo, &["rev-parse", "HEAD"]);
    assert!(error(prepare(&c, &middle, Kind::Drop, "")).contains("across a merge"));
    assert!(error(prepare(&c, &middle, Kind::Edit, "Changed")).contains("across a merge"));
    assert_eq!(git(repo, &["rev-parse", "HEAD"]), merge);
}

#[test]
fn a_drop_conflict_leaves_native_rebase_state_and_keeps_the_branch_tip() {
    let (dir, c, [_, middle, _]) = history();
    let repo = dir.path();
    commit(repo, "middle", "Edit the file added by the commit to drop");
    let head = git(repo, &["rev-parse", "HEAD"]);
    let (review, _) = prepare(&c, &middle, Kind::Drop, "").unwrap();
    assert!(review.execute(&c).is_err());
    assert!(repo.join(".git/rebase-merge").is_dir());
    assert!(!git(repo, &["ls-files", "--unmerged"]).is_empty());
    assert_eq!(git(repo, &["rev-parse", "refs/heads/main"]), head);
    assert!(error(prepare(&c, &middle, Kind::Checkout, "")).contains("current Git operation"));
    git(repo, &["rebase", "--abort"]);
    assert_eq!(git(repo, &["rev-parse", "HEAD"]), head);
    assert!(git(repo, &["status", "--porcelain"]).is_empty());
}
