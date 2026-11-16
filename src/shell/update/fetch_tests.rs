use super::*;
use crate::shell::{test_support, Popup, Shell};
use std::sync::mpsc;

#[test]
fn manual_fetch_observes_only_successful_non_skipped_remotes() {
    let (directory, app) = test_support::fixture();
    let repo = &app.ctx.repos[0].path;
    let remote = directory.path().join("remote.git");
    git(
        directory.path(),
        &[
            "clone",
            "-q",
            "--bare",
            repo.to_str().unwrap(),
            remote.to_str().unwrap(),
        ],
    )
    .unwrap();
    for name in ["origin", "archive"] {
        git(repo, &["remote", "add", name, remote.to_str().unwrap()]).unwrap();
    }
    git(repo, &["config", "remote.archive.skipFetchAll", "true"]).unwrap();
    let plan = Plan::read(repo, Kind::Fetch).unwrap();
    let captured = plan.fetch_targets(repo);
    plan.execute(repo, Kind::Fetch, Strategy::Merge).unwrap();
    let observations = plan.fetch_observations(repo, &captured);
    assert_eq!(observations.len(), 1);
    assert_eq!(observations[0].remote, "origin");
    assert_eq!(observations[0].branch, None);
    assert_eq!(observations[0].tip, None);
    git(
        repo,
        &[
            "config",
            "--replace-all",
            "remote.origin.fetch",
            "+refs/heads/main:refs/remotes/origin/reviewed",
        ],
    )
    .unwrap();
    assert!(plan.fetch_observations(repo, &captured).is_empty());
    git(repo, &["remote", "set-url", "origin", "changed-endpoint"]).unwrap();
    assert!(plan.fetch_observations(repo, &captured).is_empty());
}

#[test]
fn workspace_fetch_is_busy_even_under_help() {
    let (_directory, mut app): (_, Shell) = test_support::fixture();
    let mut dialog = Dialog::new(Kind::Fetch, &app.ctx);
    let (_sender, receiver) = mpsc::channel();
    dialog.stage = Stage::Running(receiver);
    app.pop = Some(Popup::Update(dialog));
    assert!(app.git_busy());
    app.open_help();
    assert!(app.git_busy());
    app.request_quit();
    assert!(!app.quit);
}
