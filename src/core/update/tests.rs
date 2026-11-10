use super::*;
use std::{fs, path::PathBuf};

fn g(repo: &Path, args: &[&str]) -> String {
    git(repo, args).unwrap()
}

fn commit(repo: &Path, path: &str, contents: &str) -> String {
    fs::write(repo.join(path), contents).unwrap();
    g(repo, &["add", "--", path]);
    g(repo, &["commit", "-qm", contents]);
    g(repo, &["rev-parse", "HEAD"])
}

fn fixture(reflogs: bool) -> (tempfile::TempDir, PathBuf, String, String) {
    let directory = tempfile::tempdir().unwrap();
    let repo = directory.path().join("reviewed repository");
    fs::create_dir(&repo).unwrap();
    g(&repo, &["init", "-qb", "main"]);
    for (key, value) in [
        ("user.name", "Update ownership tester"),
        ("user.email", "update-ownership@example.invalid"),
        ("commit.gpgsign", "false"),
        ("core.hooksPath", "disabled-hooks"),
        (
            "core.logAllRefUpdates",
            if reflogs { "true" } else { "false" },
        ),
    ] {
        g(&repo, &["config", key, value]);
    }
    commit(&repo, "base.txt", "base");
    g(&repo, &["switch", "-qc", "incoming"]);
    let target = commit(&repo, "incoming.txt", "incoming");
    g(&repo, &["switch", "-q", "main"]);
    commit(&repo, "local.txt", "local");
    let head = commit(&repo, "second-local.txt", "second local");
    (directory, repo, head, target)
}

#[cfg(unix)]
fn hook(repo: &Path, name: &str, contents: &str) {
    use std::os::unix::fs::PermissionsExt;

    let hooks = repo.join(".git/reviewed-hooks");
    fs::create_dir_all(&hooks).unwrap();
    let path = hooks.join(name);
    fs::write(&path, contents).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    g(repo, &["config", "core.hooksPath", hooks.to_str().unwrap()]);
}

#[cfg(unix)]
#[test]
fn post_rewrite_commits_cannot_expand_the_reviewed_rebase_result() {
    for reflogs in [true, false] {
        let (_directory, repo, head, target) = fixture(reflogs);
        hook(
            &repo,
            "post-rewrite",
            "#!/bin/sh\n[ \"$1\" = rebase ] || exit 0\ncat > .git/reviewed-rewrites\ngit rev-parse HEAD > .git/reviewed-rebase-tip\ngit -c core.hooksPath=/dev/null commit --allow-empty -qm 'post-rewrite extra commit'\n",
        );
        let result = integrate_reviewed(&repo, &target, Strategy::Rebase, "refs/heads/main", &head);
        let current = g(&repo, &["rev-parse", "HEAD"]);
        let owned = fs::read_to_string(repo.join(".git/reviewed-rebase-tip")).unwrap();
        assert_ne!(current, owned.trim());
        assert_eq!(g(&repo, &["rev-parse", "ORIG_HEAD"]), head);
        assert_eq!(
            g(&repo, &["log", "-1", "--format=%s"]),
            "post-rewrite extra commit"
        );
        assert!(g(&repo, &["status", "--porcelain"]).is_empty());
        assert!(ready(&repo).is_ok());
        let error = result.unwrap_err().to_string();
        assert!(error.contains("Rebase result changed outside"), "{error}");
        assert_eq!(g(&repo, &["rev-parse", "HEAD"]), current);
    }
}

#[test]
fn automatic_rebase_returns_the_owned_tip_even_without_existing_reflogs() {
    for reflogs in [true, false] {
        let (_directory, repo, head, target) = fixture(reflogs);
        if !reflogs {
            assert!(git(&repo, &["reflog", "exists", "HEAD"]).is_err());
        }
        let updated =
            integrate_reviewed(&repo, &target, Strategy::Rebase, "refs/heads/main", &head).unwrap();
        assert_eq!(g(&repo, &["rev-parse", "HEAD"]), updated);
        assert_ne!(updated, head);
        assert!(is_ancestor(&repo, &target, &updated).unwrap());
        assert_eq!(
            g(
                &repo,
                &["rev-list", "--count", &format!("{target}..{updated}")]
            ),
            "2"
        );
        assert_eq!(fs::read_to_string(repo.join("local.txt")).unwrap(), "local");
        assert_eq!(
            fs::read_to_string(repo.join("incoming.txt")).unwrap(),
            "incoming"
        );
        assert_eq!(
            g(&repo, &["config", "core.logAllRefUpdates"]),
            reflogs.to_string()
        );
        assert!(g(&repo, &["status", "--porcelain"]).is_empty());
        assert!(ready(&repo).is_ok());
    }
}

#[test]
fn reviewed_rebase_preserves_merge_topology_and_other_branch_refs() {
    let (_directory, repo, _, target) = fixture(true);
    g(&repo, &["switch", "-qc", "side"]);
    let side = commit(&repo, "side.txt", "side");
    g(&repo, &["switch", "-q", "main"]);
    commit(&repo, "main.txt", "main");
    g(&repo, &["merge", "--no-ff", "-qm", "local merge", "side"]);
    let head = g(&repo, &["rev-parse", "HEAD"]);
    let updated =
        integrate_reviewed(&repo, &target, Strategy::Rebase, "refs/heads/main", &head).unwrap();
    assert_eq!(g(&repo, &["rev-parse", "HEAD"]), updated);
    assert_eq!(g(&repo, &["rev-parse", "side"]), side);
    assert_eq!(
        g(
            &repo,
            &[
                "rev-list",
                "--count",
                "--merges",
                &format!("{target}..{updated}")
            ]
        ),
        "1"
    );
    assert_eq!(
        g(&repo, &["show", "-s", "--format=%P", &updated])
            .split_whitespace()
            .count(),
        2
    );
    assert!(is_ancestor(&repo, &target, &updated).unwrap());
    assert!(g(&repo, &["status", "--porcelain"]).is_empty());
}

#[test]
fn reviewed_rebase_fast_forward_returns_the_exact_target() {
    for reflogs in [true, false] {
        let (_directory, repo, _, target) = fixture(reflogs);
        g(&repo, &["switch", "-qc", "behind", &format!("{target}^")]);
        let head = g(&repo, &["rev-parse", "HEAD"]);
        assert_eq!(
            integrate_reviewed(&repo, &target, Strategy::Rebase, "refs/heads/behind", &head)
                .unwrap(),
            target
        );
        assert_eq!(g(&repo, &["rev-parse", "HEAD"]), target);
        assert!(ready(&repo).is_ok());
    }
}
