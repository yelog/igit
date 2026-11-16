use super::*;
use std::fs;

fn g(repo: &Path, args: &[&str]) -> String {
    git(repo, args).unwrap()
}

fn fixture() -> (tempfile::TempDir, PathBuf, String) {
    let directory = tempfile::tempdir().unwrap();
    let writer = directory.path().join("writer");
    let remote = directory.path().join("remote.git");
    let local = directory.path().join("local");
    fs::create_dir(&writer).unwrap();
    g(&writer, &["init", "-qb", "main"]);
    for (name, value) in [
        ("user.name", "Workspace Pull tester"),
        ("user.email", "workspace-pull@example.invalid"),
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
        ("user.name", "Workspace Pull tester"),
        ("user.email", "workspace-pull@example.invalid"),
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
    (directory, local.canonicalize().unwrap(), incoming)
}

#[test]
fn pull_fetches_the_exact_differently_named_upstream_without_other_ref_updates() {
    for strategy in [Strategy::Merge, Strategy::Rebase] {
        let (_directory, repo, incoming) = fixture();
        let base = g(&repo, &["rev-parse", "HEAD"]);
        let fetch_head = fs::read(repo.join(".git/FETCH_HEAD")).ok();
        g(&repo, &["config", "fetch.prune", "true"]);
        g(&repo, &["config", "fetch.pruneTags", "true"]);
        g(&repo, &["tag", "local-only"]);
        g(&repo, &["update-ref", "refs/remotes/origin/stale", &base]);
        Plan::read(&repo, Kind::Pull)
            .unwrap()
            .execute(&repo, Kind::Pull, strategy)
            .unwrap();
        assert_eq!(g(&repo, &["rev-parse", "HEAD"]), incoming);
        assert_eq!(
            g(&repo, &["rev-parse", "refs/remotes/origin/release/next"]),
            incoming
        );
        assert_eq!(g(&repo, &["rev-parse", "refs/remotes/origin/main"]), base);
        assert_eq!(g(&repo, &["rev-parse", "refs/remotes/origin/stale"]), base);
        assert_eq!(g(&repo, &["tag", "--list"]), "local-only");
        assert_eq!(fs::read(repo.join(".git/FETCH_HEAD")).ok(), fetch_head);
    }
}

#[test]
fn pull_preserves_local_upstream_refs_and_does_not_contact_configured_remotes() {
    for strategy in [Strategy::Merge, Strategy::Rebase] {
        let (directory, repo, _) = fixture();
        let base = g(&repo, &["rev-parse", "HEAD"]);
        g(&repo, &["switch", "-qc", "local/upstream"]);
        fs::write(repo.join("local-upstream.txt"), "local upstream\n").unwrap();
        g(&repo, &["add", "."]);
        g(&repo, &["commit", "-qm", "local upstream"]);
        let captured = g(&repo, &["rev-parse", "HEAD"]);
        g(&repo, &["switch", "-q", "main"]);
        g(&repo, &["config", "branch.main.remote", "."]);
        g(
            &repo,
            &["config", "branch.main.merge", "refs/heads/local/upstream"],
        );
        g(
            &repo,
            &[
                "remote",
                "set-url",
                "origin",
                directory.path().join("unavailable.git").to_str().unwrap(),
            ],
        );
        let fetch_head = fs::read(repo.join(".git/FETCH_HEAD")).ok();
        Plan::read(&repo, Kind::Pull)
            .unwrap()
            .execute(&repo, Kind::Pull, strategy)
            .unwrap();
        assert_eq!(g(&repo, &["rev-parse", "HEAD"]), captured);
        assert_eq!(g(&repo, &["rev-parse", "local/upstream"]), captured);
        assert_eq!(
            g(&repo, &["rev-parse", "refs/remotes/origin/release/next"]),
            base
        );
        assert_eq!(fs::read(repo.join(".git/FETCH_HEAD")).ok(), fetch_head);
        assert_eq!(
            fs::read_to_string(repo.join("local-upstream.txt")).unwrap(),
            "local upstream\n"
        );
    }
}

#[test]
fn pull_preserves_ignored_and_untracked_files_in_the_captured_target() {
    for strategy in [Strategy::Merge, Strategy::Rebase] {
        for ignored in [true, false] {
            let (_directory, repo, _) = fixture();
            let head = g(&repo, &["rev-parse", "HEAD"]);
            fs::write(repo.join("incoming.txt"), "local content\n").unwrap();
            if ignored {
                fs::write(repo.join(".git/info/exclude"), "incoming.txt\n").unwrap();
            }
            let index = fs::read(repo.join(".git/index")).unwrap();
            let error = Plan::read(&repo, Kind::Pull)
                .unwrap()
                .execute(&repo, Kind::Pull, strategy)
                .unwrap_err()
                .to_string();
            assert!(error.contains("may be overwritten"), "{error}");
            assert_eq!(g(&repo, &["rev-parse", "HEAD"]), head);
            assert_eq!(fs::read(repo.join(".git/index")).unwrap(), index);
            assert_eq!(
                fs::read_to_string(repo.join("incoming.txt")).unwrap(),
                "local content\n"
            );
            assert!(g(&repo, &["stash", "list"]).is_empty());
        }
    }
}

#[cfg(unix)]
fn after_tracking_update(repo: &Path, action: &str) {
    use std::os::unix::fs::PermissionsExt;

    let hooks = repo.join(".git/pull-hooks");
    fs::create_dir(&hooks).unwrap();
    let hook = hooks.join("reference-transaction");
    fs::write(
        &hook,
        format!(
            "#!/bin/sh\n[ \"$1\" = committed ] || exit 0\nwhile read old new ref; do\n  if [ \"$ref\" = refs/remotes/origin/release/next ]; then\n    {action} || exit 1\n  fi\ndone\n"
        ),
    )
    .unwrap();
    fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).unwrap();
    g(repo, &["config", "core.hooksPath", hooks.to_str().unwrap()]);
}

#[cfg(unix)]
#[test]
fn pull_integrates_its_captured_oid_when_shared_tracking_changes_after_fetch() {
    for strategy in [Strategy::Merge, Strategy::Rebase] {
        let (_directory, repo, incoming) = fixture();
        let base = g(&repo, &["rev-parse", "HEAD"]);
        let shared = g(
            &repo,
            &[
                "commit-tree",
                "HEAD^{tree}",
                "-p",
                &base,
                "-m",
                "another fetch",
            ],
        );
        let plan = Plan::read(&repo, Kind::Pull).unwrap();
        after_tracking_update(
            &repo,
            &format!("git -c core.hooksPath=disabled-hooks update-ref \"$ref\" {shared}"),
        );
        plan.execute(&repo, Kind::Pull, strategy).unwrap();
        assert_eq!(g(&repo, &["rev-parse", "HEAD"]), incoming);
        assert_eq!(
            g(&repo, &["rev-parse", "refs/remotes/origin/release/next"]),
            shared
        );
        assert_eq!(g(&repo, &["rev-parse", "refs/remotes/origin/main"]), base);
        assert_eq!(
            fs::read_to_string(repo.join("incoming.txt")).unwrap(),
            "incoming\n"
        );
    }
}

#[cfg(unix)]
#[test]
fn pull_refuses_remote_or_upstream_changes_between_fetch_and_integration() {
    for change_url in [true, false] {
        let (directory, repo, incoming) = fixture();
        let head = g(&repo, &["rev-parse", "HEAD"]);
        let plan = Plan::read(&repo, Kind::Pull).unwrap();
        let private = directory.path().join("private-changed-endpoint.git");
        let action = if change_url {
            format!(
                "git config remote.origin.url '{}'",
                private.to_str().unwrap().replace('\'', "'\\''")
            )
        } else {
            "git config branch.main.merge refs/heads/main".to_owned()
        };
        after_tracking_update(&repo, &action);
        let error = plan
            .execute(&repo, Kind::Pull, Strategy::Merge)
            .unwrap_err()
            .to_string();
        assert!(error.contains("review again"), "{error}");
        assert!(!error.contains("private-changed-endpoint"), "{error}");
        assert_eq!(g(&repo, &["rev-parse", "HEAD"]), head);
        assert_eq!(
            g(&repo, &["rev-parse", "refs/remotes/origin/release/next"]),
            incoming
        );
        assert!(!repo.join("incoming.txt").exists());
    }
}
