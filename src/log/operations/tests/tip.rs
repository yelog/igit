use super::*;

#[test]
fn tip_removal_rechecks_external_commits_at_the_ref_write_boundary() {
    for kind in [Kind::Drop, Kind::Undo] {
        let (dir, _, [_, middle, _]) = history();
        let repo = dir.path();
        let head = Head::read(repo).unwrap();
        commit(repo, "external", "External commit after validation");
        let current = git(repo, &["rev-parse", "HEAD"]);
        let index = fs::read(repo.join(".git/index")).unwrap();
        assert!(super::super::tip::execute(repo, &head, Some(&middle), kind).is_err());
        assert_eq!(git(repo, &["rev-parse", "HEAD"]), current);
        assert_eq!(fs::read(repo.join(".git/index")).unwrap(), index);
        assert_eq!(
            fs::read_to_string(repo.join("external")).unwrap(),
            "External commit after validation"
        );
        assert!(git(repo, &["status", "--porcelain"]).is_empty());
    }
}

#[test]
fn prepared_transaction_locks_head_and_branch_and_aborts_without_writes() {
    for parent in [true, false] {
        let (dir, _, [_, middle, _]) = history();
        let repo = dir.path();
        let head = Head::read(repo).unwrap();
        let transaction = super::super::transaction::Transaction::prepare(
            repo,
            &head,
            parent.then_some(middle.as_str()),
            "test reviewed operation",
        )
        .unwrap();
        assert!(!command(repo)
            .args(["symbolic-ref", "HEAD", "refs/heads/external"])
            .output()
            .unwrap()
            .status
            .success());
        assert!(!command(repo)
            .args(["update-ref", "refs/heads/main", &middle, &head.oid])
            .output()
            .unwrap()
            .status
            .success());
        drop(transaction);
        assert_eq!(Head::read(repo).unwrap(), head);
        assert!(git(repo, &["status", "--porcelain"]).is_empty());
        assert!(!repo.join(".git/HEAD.lock").exists());
        assert!(!repo.join(".git/refs/heads/main.lock").exists());
        assert!(
            optional(repo, &["rev-parse", "--verify", "--quiet", "ORIG_HEAD"])
                .unwrap()
                .is_none()
        );
    }
}

#[test]
fn changing_branch_without_moving_head_cancels_the_prepared_transaction() {
    let (dir, _, [_, middle, _]) = history();
    let repo = dir.path();
    let head = Head::read(repo).unwrap();
    git(repo, &["switch", "-c", "external"]);
    let result = super::super::transaction::Transaction::prepare(
        repo,
        &head,
        Some(&middle),
        "test stale branch",
    );
    assert!(result.is_err());
    assert_eq!(git(repo, &["rev-parse", "main"]), head.oid);
    assert_eq!(git(repo, &["rev-parse", "external"]), head.oid);
    assert_eq!(git(repo, &["symbolic-ref", "HEAD"]), "refs/heads/external");
}

#[test]
#[cfg(unix)]
fn publication_during_ref_preparation_stops_removal_before_changing_files() {
    use std::os::unix::fs::PermissionsExt;
    for initial in [false, true] {
        for kind in [Kind::Drop, Kind::Undo, Kind::Edit] {
            let dir = init();
            let repo = dir.path();
            commit(repo, "base", "Base");
            if !initial {
                commit(repo, "tip", "Tip");
            }
            let c = Context::discover(repo).unwrap();
            let head = git(repo, &["rev-parse", "HEAD"]);
            let (review, _) = prepare(&c, &head, kind, "New message").unwrap();
            let index = fs::read(repo.join(".git/index")).unwrap();
            let hooks = repo.join(".git/test-hooks");
            fs::create_dir(&hooks).unwrap();
            let hook = hooks.join("reference-transaction");
            fs::write(&hook, "#!/bin/sh\nif [ \"$1\" = prepared ]; then\n  while read old new ref; do\n    if [ \"$ref\" = refs/heads/main ]; then\n      git -c core.hooksPath=/dev/null update-ref refs/remotes/other/published \"$old\" || exit 1\n    fi\n  done\nfi\n").unwrap();
            fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).unwrap();
            git(repo, &["config", "core.hooksPath", hooks.to_str().unwrap()]);
            assert!(
                format!("{:#}", review.execute(&c).unwrap_err()).contains("remote-tracking branch")
            );
            assert_eq!(git(repo, &["rev-parse", "HEAD"]), head);
            assert_eq!(fs::read(repo.join(".git/index")).unwrap(), index);
            assert_eq!(fs::read_to_string(repo.join("base")).unwrap(), "Base");
            assert_eq!(repo.join("tip").exists(), !initial);
            assert!(git(repo, &["status", "--porcelain"]).is_empty());
            assert!(!repo.join(".git/HEAD.lock").exists());
            assert!(!repo.join(".git/refs/heads/main.lock").exists());
        }
    }
}
