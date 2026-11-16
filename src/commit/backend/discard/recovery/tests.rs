use super::*;
use crate::commit::backend::{
    discard::{execute_in, review, Target},
    tests::{commit_all, get, put, setup},
};

fn plan(repo: &Path, names: &[&str]) -> Plan {
    let (plan, unavailable) = review(
        0,
        repo,
        Target::Files(names.iter().map(PathBuf::from).collect()),
    )
    .unwrap();
    assert!(unavailable.is_empty(), "{unavailable:?}");
    plan
}

fn restore(repo: &Path, state: &Path) -> RestorePlan {
    RestorePlan::latest(0, repo, state)
        .unwrap()
        .expect("saved rollback")
}

#[test]
fn rollback_recovery_restores_mixed_versions_without_replacing_unselected_index_entries() {
    let dir = setup();
    let state = tempfile::tempdir().unwrap();
    let repo = dir.path();
    put(repo, "mixed", "base\n");
    put(repo, "sibling", "base\n");
    commit_all(repo);
    put(repo, "mixed", "selected staged\n");
    put(repo, "sibling", "unselected staged\n");
    g(repo, None, &["add", "."]).unwrap();
    put(repo, "mixed", "selected working\n");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(repo.join("mixed"), fs::Permissions::from_mode(0o751)).unwrap();
    }
    let before = Snapshot::read(repo, Path::new("mixed")).unwrap();
    let head = get(repo, &["rev-parse", "HEAD"]);
    execute_in(&plan(repo, &["mixed"]), true, state.path()).unwrap();
    assert_eq!(get(repo, &["show", ":mixed"]), "base");
    put(repo, "sibling", "later unrelated staged\n");
    g(repo, None, &["add", "sibling"]).unwrap();
    let sibling = stage(repo, None, Path::new("sibling")).unwrap();
    let reopened = restore(repo, state.path());
    assert!(reopened.description().unwrap().contains("selected working"));
    assert_eq!(reopened.execute().unwrap(), 1);
    assert_eq!(Snapshot::read(repo, Path::new("mixed")).unwrap(), before);
    assert_eq!(get(repo, &["show", ":mixed"]), "selected staged");
    assert_eq!(stage(repo, None, Path::new("sibling")).unwrap(), sibling);
    assert_eq!(get(repo, &["rev-parse", "HEAD"]), head);
    assert!(RestorePlan::latest(0, repo, state.path())
        .unwrap()
        .is_none());
}

#[test]
fn rollback_recovery_rejects_external_content_permissions_index_and_head_changes() {
    for mutation in ["working", "index", "head", "branch", "permissions"] {
        let dir = setup();
        let state = tempfile::tempdir().unwrap();
        let repo = dir.path();
        put(repo, "file", "base\n");
        put(repo, "sibling", "base\n");
        commit_all(repo);
        put(repo, "file", "staged\n");
        g(repo, None, &["add", "file"]).unwrap();
        put(repo, "file", "working\n");
        execute_in(&plan(repo, &["file"]), true, state.path()).unwrap();
        let reviewed = restore(repo, state.path());
        match mutation {
            "working" => put(repo, "file", "external\n"),
            "index" => {
                put(repo, "file", "external staged\n");
                g(repo, None, &["add", "file"]).unwrap();
                put(repo, "file", "base\n");
            }
            "head" => {
                g(repo, None, &["commit", "--allow-empty", "-qm", "external"]).unwrap();
            }
            "branch" => {
                g(repo, None, &["switch", "-qc", "external"]).unwrap();
            }
            "permissions" => {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    fs::set_permissions(repo.join("file"), fs::Permissions::from_mode(0o751))
                        .unwrap();
                }
                #[cfg(not(unix))]
                continue;
            }
            _ => unreachable!(),
        }
        let current = fs::read(repo.join("file")).unwrap();
        let index = get(repo, &["ls-files", "--stage"]);
        assert!(reviewed.execute().is_err(), "{mutation}");
        assert!(
            RestorePlan::latest(0, repo, state.path()).is_err(),
            "{mutation}"
        );
        assert_eq!(fs::read(repo.join("file")).unwrap(), current, "{mutation}");
        assert_eq!(get(repo, &["ls-files", "--stage"]), index, "{mutation}");
        assert!(!repo.join(".git/index.lock").exists());
    }
}

#[test]
fn rollback_recovery_backup_failure_prevents_all_destructive_writes() {
    let dir = setup();
    let state = tempfile::tempdir().unwrap();
    let repo = dir.path();
    put(repo, "file", "base\n");
    commit_all(repo);
    put(repo, "file", "staged\n");
    g(repo, None, &["add", "file"]).unwrap();
    put(repo, "file", "working\n");
    put(repo, "untracked", "never lose this\n");
    let before = get(repo, &["ls-files", "--stage"]);
    let blocker = state.path().join("not-a-directory");
    fs::write(&blocker, "block").unwrap();
    let error = execute_in(&plan(repo, &["file", "untracked"]), true, &blocker).unwrap_err();
    assert!(format!("{error:#}").contains("backup failed"), "{error:#}");
    assert_eq!(get(repo, &["ls-files", "--stage"]), before);
    assert_eq!(fs::read(repo.join("file")).unwrap(), b"working\n");
    assert_eq!(
        fs::read(repo.join("untracked")).unwrap(),
        b"never lose this\n"
    );
    assert!(!repo.join(".git/index.lock").exists());
}

#[test]
fn rollback_recovery_round_trips_added_deleted_untracked_binary_and_ignored_files() {
    let dir = setup();
    let state = tempfile::tempdir().unwrap();
    let repo = dir.path();
    put(repo, "deleted", "base\n");
    put(repo, "replacement", "base\n");
    put(repo, ".gitignore", "ignored\n");
    commit_all(repo);
    fs::remove_file(repo.join("deleted")).unwrap();
    g(repo, None, &["rm", "-q", "replacement"]).unwrap();
    put(repo, "replacement", "replacement bytes\n");
    put(repo, "added", "added staged\n");
    g(repo, None, &["add", "added"]).unwrap();
    put(repo, "added", "added working\n");
    fs::write(repo.join("untracked"), [0, 255, 1, 0, 3]).unwrap();
    put(repo, "ignored", "private ignored\n");
    let names = ["added", "deleted", "replacement", "untracked"];
    let before: Vec<_> = names
        .iter()
        .map(|name| Snapshot::read(repo, Path::new(name)).unwrap())
        .collect();
    let index = get(repo, &["ls-files", "--stage"]);
    execute_in(&plan(repo, &names), true, state.path()).unwrap();
    assert!(!repo.join("added").exists());
    assert!(!repo.join("untracked").exists());
    restore(repo, state.path()).execute().unwrap();
    for (name, expected) in names.iter().zip(before) {
        assert_eq!(
            Snapshot::read(repo, Path::new(name)).unwrap(),
            expected,
            "{name}"
        );
    }
    assert_eq!(get(repo, &["ls-files", "--stage"]), index);
    assert_eq!(
        fs::read(repo.join("ignored")).unwrap(),
        b"private ignored\n"
    );
}

#[cfg(unix)]
#[test]
fn rollback_recovery_preserves_native_paths_symlinks_and_link_targets() {
    #[cfg(not(target_os = "macos"))]
    use std::os::unix::ffi::OsStringExt;
    use std::os::unix::fs::symlink;
    let dir = setup();
    let state = tempfile::tempdir().unwrap();
    let repo = dir.path();
    #[cfg(not(target_os = "macos"))]
    let name = PathBuf::from(OsString::from_vec(b"literal-*\xff\nfile".to_vec()));
    #[cfg(target_os = "macos")]
    let name = PathBuf::from("literal-*文件\nfile");
    fs::write(repo.join(&name), "base\n").unwrap();
    symlink("original-target", repo.join("link")).unwrap();
    commit_all(repo);
    fs::write(repo.join(&name), "working\n").unwrap();
    fs::remove_file(repo.join("link")).unwrap();
    symlink("outside-target", repo.join("link")).unwrap();
    symlink("outside-target", repo.join("untracked-link")).unwrap();
    put(repo, "outside-target", "keep target\n");
    let (reviewed, unavailable) = review(
        0,
        repo,
        Target::Files([name.clone(), "link".into(), "untracked-link".into()].into()),
    )
    .unwrap();
    assert!(unavailable.is_empty());
    execute_in(&reviewed, true, state.path()).unwrap();
    restore(repo, state.path()).execute().unwrap();
    assert_eq!(fs::read(repo.join(name)).unwrap(), b"working\n");
    assert_eq!(
        fs::read_link(repo.join("link")).unwrap(),
        Path::new("outside-target")
    );
    assert_eq!(
        fs::read_link(repo.join("untracked-link")).unwrap(),
        Path::new("outside-target")
    );
    assert_eq!(
        fs::read(repo.join("outside-target")).unwrap(),
        b"keep target\n"
    );
}

#[cfg(unix)]
#[test]
fn rollback_recovery_partial_failure_only_restores_completed_paths() {
    let dir = setup();
    let state = tempfile::tempdir().unwrap();
    let repo = dir.path();
    put(repo, ".gitattributes", "a filter=mutate\n");
    put(repo, "a", "base");
    put(repo, "b", "base");
    commit_all(repo);
    g(repo, None, &["config", "filter.mutate.clean", "cat"]).unwrap();
    g(
        repo,
        None,
        &["config", "filter.mutate.smudge", "printf external > b; cat"],
    )
    .unwrap();
    put(repo, "a", "a staged");
    put(repo, "b", "b staged");
    g(repo, None, &["add", "a", "b"]).unwrap();
    let error = execute_in(&plan(repo, &["a", "b"]), true, state.path()).unwrap_err();
    assert!(
        format!("{error:#}").contains("Discarded 1 of 2 files"),
        "{error:#}"
    );
    let recovery = restore(repo, state.path());
    assert_eq!(recovery.expected.len(), 1);
    recovery.execute().unwrap();
    assert_eq!(get(repo, &["show", ":a"]), "a staged");
    assert_eq!(fs::read(repo.join("a")).unwrap(), b"a staged");
    assert_eq!(get(repo, &["show", ":b"]), "b staged");
    assert_eq!(fs::read(repo.join("b")).unwrap(), b"external");
}

#[test]
fn rollback_recovery_retains_a_bounded_history_and_restores_the_latest_backup() {
    let dir = setup();
    let state = tempfile::tempdir().unwrap();
    let repo = dir.path();
    put(repo, "file", "base\n");
    commit_all(repo);
    for serial in 0..HISTORY + 3 {
        put(repo, "file", &format!("working {serial}\n"));
        execute_in(&plan(repo, &["file"]), true, state.path()).unwrap();
    }
    assert_eq!(
        records(&directory(state.path(), &fs::canonicalize(repo).unwrap()))
            .unwrap()
            .len(),
        HISTORY
    );
    restore(repo, state.path()).execute().unwrap();
    assert_eq!(
        fs::read_to_string(repo.join("file")).unwrap(),
        format!("working {}\n", HISTORY + 2)
    );
}

#[test]
fn rollback_recovery_recovers_an_interrupted_index_publication_using_recorded_post_state() {
    let dir = setup();
    let state = tempfile::tempdir().unwrap();
    let repo = dir.path();
    put(repo, "file", "base\n");
    commit_all(repo);
    put(repo, "file", "staged\n");
    g(repo, None, &["add", "file"]).unwrap();
    put(repo, "file", "working\n");
    let reviewed = plan(repo, &["file"]);
    let before = Snapshot::read(repo, Path::new("file")).unwrap();
    let mut journal = Journal::prepare(&reviewed, state.path()).unwrap();
    fs::write(repo.join("file"), "base\n").unwrap();
    journal.record_working(Path::new("file")).unwrap();
    let next = state.path().join("next-index");
    copy_index(repo, &git_path(repo, "index").unwrap(), &next).unwrap();
    reset_index_paths(repo, &next, &reviewed.base, [Path::new("file")]).unwrap();
    journal.record_index(&next, &[Path::new("file")]).unwrap();
    drop(journal);
    restore(repo, state.path()).execute().unwrap();
    assert_eq!(Snapshot::read(repo, Path::new("file")).unwrap(), before);
}

#[test]
fn rollback_recovery_refuses_newly_ignored_paths_and_changed_index_flags() {
    for mutation in ["ignored", "assume-unchanged"] {
        let dir = setup();
        let state = tempfile::tempdir().unwrap();
        let repo = dir.path();
        put(repo, "file", "base\n");
        commit_all(repo);
        let name = if mutation == "ignored" {
            "untracked"
        } else {
            "file"
        };
        put(repo, name, "working\n");
        execute_in(&plan(repo, &[name]), true, state.path()).unwrap();
        let reviewed = restore(repo, state.path());
        if mutation == "ignored" {
            put(repo, ".gitignore", "untracked\n");
        } else {
            g(repo, None, &["update-index", "--assume-unchanged", "file"]).unwrap();
        }
        assert!(reviewed.execute().is_err(), "{mutation}");
        assert!(
            RestorePlan::latest(0, repo, state.path()).is_err(),
            "{mutation}"
        );
        if mutation == "ignored" {
            assert!(!repo.join(name).exists());
        } else {
            assert_eq!(fs::read(repo.join(name)).unwrap(), b"base\n");
        }
    }
}

#[test]
fn rollback_recovery_recreates_the_saved_staged_blob_after_git_prunes_it() {
    let dir = setup();
    let state = tempfile::tempdir().unwrap();
    let repo = dir.path();
    put(repo, "file", "base\n");
    commit_all(repo);
    put(repo, "file", "uncommitted staged blob\n");
    g(repo, None, &["add", "file"]).unwrap();
    let staged = get(repo, &["rev-parse", ":file"]);
    put(repo, "file", "working bytes\n");
    execute_in(&plan(repo, &["file"]), true, state.path()).unwrap();
    g(repo, None, &["prune", "--expire", "now"]).unwrap();
    assert!(g(repo, None, &["cat-file", "-e", &staged]).is_err());
    restore(repo, state.path()).execute().unwrap();
    assert_eq!(get(repo, &["rev-parse", ":file"]), staged);
    assert_eq!(get(repo, &["show", ":file"]), "uncommitted staged blob");
    assert_eq!(fs::read(repo.join("file")).unwrap(), b"working bytes\n");
}
