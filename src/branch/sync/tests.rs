use super::*;
use crate::branch::model::Snapshot;
use std::{fs, path::Path};

fn g(repo: &Path, args: &[&str]) -> String {
    git(repo, args).unwrap()
}

fn fixture() -> (tempfile::TempDir, Repo, String) {
    let directory = tempfile::tempdir().unwrap();
    let writer = directory.path().join("writer");
    let remote = directory.path().join("remote.git");
    let local = directory.path().join("local");
    fs::create_dir(&writer).unwrap();
    g(&writer, &["init", "-qb", "main"]);
    for (name, value) in [
        ("user.name", "Pull tester"),
        ("user.email", "pull@example.invalid"),
        ("commit.gpgsign", "false"),
        ("core.hooksPath", "disabled-hooks"),
    ] {
        g(&writer, &["config", name, value]);
    }
    fs::write(writer.join("base.txt"), "base\n").unwrap();
    g(&writer, &["add", "."]);
    g(&writer, &["commit", "-qm", "base"]);
    g(&writer, &["branch", "release/next"]);
    g(
        &writer,
        &["clone", "--bare", "-q", ".", remote.to_str().unwrap()],
    );
    g(
        directory.path(),
        &[
            "clone",
            "-q",
            remote.to_str().unwrap(),
            local.to_str().unwrap(),
        ],
    );
    for (name, value) in [
        ("user.name", "Pull tester"),
        ("user.email", "pull@example.invalid"),
        ("commit.gpgsign", "false"),
        ("core.hooksPath", "disabled-hooks"),
    ] {
        g(&local, &["config", name, value]);
    }
    g(
        &local,
        &["branch", "--set-upstream-to=origin/release/next", "main"],
    );
    g(&writer, &["switch", "-q", "release/next"]);
    fs::write(writer.join("incoming.txt"), "incoming\n").unwrap();
    g(&writer, &["add", "."]);
    g(&writer, &["commit", "-qm", "incoming"]);
    let incoming = g(&writer, &["rev-parse", "HEAD"]);
    g(
        &writer,
        &["remote", "add", "origin", remote.to_str().unwrap()],
    );
    g(&writer, &["push", "-q", "origin", "release/next"]);
    let repo = Repo {
        path: local.canonicalize().unwrap(),
        name: "local".into(),
        branch: "main".into(),
        submodule: false,
        parent: None,
    };
    (directory, repo, incoming)
}

fn branch(repo: &Repo, name: &str) -> Branch {
    Snapshot::read(repo).unwrap().branches[&format!("refs/heads/{name}")].clone()
}

#[test]
fn pull_integrates_the_captured_oid_after_another_fetch_moves_tracking() {
    for current in [true, false] {
        let (_directory, repo, incoming) = fixture();
        let path = &repo.path;
        let base = g(path, &["rev-parse", "HEAD"]);
        let name = if current { "main" } else { "topic" };
        if !current {
            g(path, &["branch", "--track", "topic", "origin/release/next"]);
            fs::write(path.join("base.txt"), "staged\n").unwrap();
            fs::write(path.join("staged.txt"), "other staged\n").unwrap();
            g(path, &["add", "base.txt", "staged.txt"]);
            fs::write(path.join("base.txt"), "unstaged\n").unwrap();
            fs::write(path.join("untracked.txt"), "untracked\n").unwrap();
            fs::write(path.join(".git/info/exclude"), "ignored.txt\n").unwrap();
            fs::write(path.join("ignored.txt"), "ignored\n").unwrap();
        }
        let shared = g(
            path,
            &[
                "commit-tree",
                "HEAD^{tree}",
                "-p",
                &base,
                "-m",
                "another fetch",
            ],
        );
        let source = branch(&repo, name);
        let sync = Sync::prepare(&repo, 0, &source, Operation::Pull).unwrap();
        let index = fs::read(path.join(".git/index")).unwrap();
        let staged = g(path, &["diff", "--cached", "--binary"]);
        let unstaged = g(path, &["diff", "--binary"]);
        let fetch_head = fs::read(path.join(".git/FETCH_HEAD")).ok();
        sync.execute(&repo, &source, || {
            g(
                path,
                &["update-ref", "refs/remotes/origin/release/next", &shared],
            );
            Ok(())
        })
        .unwrap();
        assert_eq!(g(path, &["rev-parse", &source.reference]), incoming);
        assert_eq!(
            g(path, &["rev-parse", "refs/remotes/origin/release/next"]),
            shared
        );
        assert_eq!(fs::read(path.join(".git/FETCH_HEAD")).ok(), fetch_head);
        if current {
            assert_eq!(g(path, &["rev-parse", "HEAD"]), incoming);
            assert_eq!(
                fs::read_to_string(path.join("incoming.txt")).unwrap(),
                "incoming\n"
            );
        } else {
            assert_eq!(g(path, &["rev-parse", "HEAD"]), base);
            assert_eq!(fs::read(path.join(".git/index")).unwrap(), index);
            assert_eq!(g(path, &["diff", "--cached", "--binary"]), staged);
            assert_eq!(g(path, &["diff", "--binary"]), unstaged);
            assert_eq!(
                fs::read_to_string(path.join("untracked.txt")).unwrap(),
                "untracked\n"
            );
            assert_eq!(
                fs::read_to_string(path.join("ignored.txt")).unwrap(),
                "ignored\n"
            );
            assert!(!path.join("incoming.txt").exists());
        }
    }
}

#[test]
fn pull_rejects_remote_or_upstream_changes_after_fetch() {
    for change_url in [true, false] {
        let (directory, repo, incoming) = fixture();
        let source = branch(&repo, "main");
        let sync = Sync::prepare(&repo, 0, &source, Operation::Pull).unwrap();
        let private = directory.path().join("private-changed-endpoint.git");
        let error = sync
            .execute(&repo, &source, || {
                if change_url {
                    g(
                        &repo.path,
                        &["remote", "set-url", "origin", private.to_str().unwrap()],
                    );
                } else {
                    g(
                        &repo.path,
                        &["config", "branch.main.merge", "refs/heads/main"],
                    );
                }
                Ok(())
            })
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("changed") && error.contains("review again"),
            "{error}"
        );
        assert!(!error.contains("private-changed-endpoint"), "{error}");
        assert_eq!(g(&repo.path, &["rev-parse", "HEAD"]), source.oid);
        assert_eq!(
            g(
                &repo.path,
                &["rev-parse", "refs/remotes/origin/release/next"]
            ),
            incoming
        );
        assert!(!repo.path.join("incoming.txt").exists());
    }
}

#[test]
fn pull_does_not_write_a_local_branch_through_symbolic_tracking() {
    let (_directory, repo, incoming) = fixture();
    let base = g(&repo.path, &["rev-parse", "HEAD"]);
    g(&repo.path, &["branch", "untouched"]);
    g(
        &repo.path,
        &[
            "symbolic-ref",
            "refs/remotes/origin/release/next",
            "refs/heads/untouched",
        ],
    );
    let source = branch(&repo, "main");
    Sync::prepare(&repo, 0, &source, Operation::Pull)
        .unwrap()
        .execute(&repo, &source, || Ok(()))
        .unwrap();
    assert_eq!(g(&repo.path, &["rev-parse", "HEAD"]), incoming);
    assert_eq!(g(&repo.path, &["rev-parse", "untouched"]), base);
    assert_eq!(
        g(
            &repo.path,
            &["symbolic-ref", "refs/remotes/origin/release/next"]
        ),
        "refs/heads/untouched"
    );
}
