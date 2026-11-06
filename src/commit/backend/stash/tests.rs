use super::*;
use crate::commit::backend::tests::{commit_all, put, setup};
use std::fs;

fn fixture() -> tempfile::TempDir {
    let dir = setup();
    let repo = dir.path();
    for name in ["mixed.txt", "unchecked.txt", "deleted.txt"] {
        put(repo, name, "base\n");
    }
    put(repo, ".gitignore", "ignored.txt\n");
    commit_all(repo);
    put(repo, "mixed.txt", "staged\n");
    put(repo, "unchecked.txt", "unchecked staged\n");
    g(repo, None, &["add", "mixed.txt", "unchecked.txt"]).unwrap();
    put(repo, "mixed.txt", "unstaged 中文\n");
    fs::remove_file(repo.join("deleted.txt")).unwrap();
    put(repo, "new file.txt", "new\n");
    put(repo, "ignored.txt", "ignored\n");
    dir
}

fn review(repo: &Path) -> Plan {
    plan(0, repo, "shared stash 中文").unwrap().unwrap()
}

fn read(repo: &Path, object: &str) -> String {
    txt(g(repo, None, &["show", object]).unwrap())
}

#[test]
fn stash_preserves_all_versions_and_untracked_files_and_round_trips() {
    let dir = fixture();
    let repo = dir.path();
    let head = oid(repo).unwrap();
    let index = g(repo, None, &["ls-files", "--stage", "-z"]).unwrap();
    let p = review(repo);
    assert_eq!(p.files().len(), 4);
    assert!(p.preview.contains("staged\n"));
    assert!(p.preview.contains("unstaged 中文"));
    let result = execute(&p).unwrap();
    assert!(result.warning.is_none(), "{:?}", result.warning);
    assert_eq!(oid(repo).unwrap(), head);
    assert!(status(repo).unwrap().is_empty());
    assert_eq!(read(repo, "stash@{0}:mixed.txt"), "unstaged 中文");
    assert_eq!(read(repo, "stash@{0}^2:mixed.txt"), "staged");
    assert_eq!(read(repo, "stash@{0}^2:unchecked.txt"), "unchecked staged");
    assert_eq!(read(repo, "stash@{0}^3:new file.txt"), "new");
    assert!(read(repo, "stash@{0}").contains("shared stash 中文"));
    assert_eq!(
        fs::read_to_string(repo.join("ignored.txt")).unwrap(),
        "ignored\n"
    );
    assert!(g(repo, None, &["cat-file", "-e", "stash@{0}:deleted.txt"]).is_err());
    g(repo, None, &["stash", "apply", "--index", &result.hash]).unwrap();
    assert_eq!(
        g(repo, None, &["ls-files", "--stage", "-z"]).unwrap(),
        index
    );
    assert_eq!(
        fs::read_to_string(repo.join("mixed.txt")).unwrap(),
        "unstaged 中文\n"
    );
    assert_eq!(
        fs::read_to_string(repo.join("new file.txt")).unwrap(),
        "new\n"
    );
    assert!(!repo.join("deleted.txt").exists());
}

#[test]
fn stash_review_is_read_only_and_empty_repositories_are_skipped() {
    let dir = fixture();
    let repo = dir.path();
    let index = fs::read(repo.join(".git/index")).unwrap();
    let before = Snapshot::read(repo).unwrap();
    let _p = review(repo);
    assert!(Snapshot::read(repo).unwrap() == before);
    assert_eq!(fs::read(repo.join(".git/index")).unwrap(), index);
    assert!(tip(repo).unwrap().is_none());
    execute(&review(repo)).unwrap();
    assert!(plan(0, repo, "next").unwrap().is_none());
    assert!(plan(0, repo, "  ").is_err());
}

#[test]
fn stash_saves_staged_deletion_and_its_untracked_replacement() {
    let dir = setup();
    let repo = dir.path();
    put(repo, "replacement.txt", "base\n");
    commit_all(repo);
    g(repo, None, &["rm", "-q", "replacement.txt"]).unwrap();
    put(repo, "replacement.txt", "untracked replacement\n");
    let p = review(repo);
    assert!(p.preview.contains("untracked replacement"));
    assert!(execute(&p).unwrap().warning.is_none());
    assert_eq!(
        read(repo, "stash@{0}^3:replacement.txt"),
        "untracked replacement"
    );
    assert!(g(
        repo,
        None,
        &["cat-file", "-e", "stash@{0}^2:replacement.txt"]
    )
    .is_err());
    assert!(status(repo).unwrap().is_empty());
}

#[test]
fn stash_rejects_changed_files_index_head_branch_and_stash_list() {
    for change in [
        "working",
        "untracked",
        "new",
        "index",
        "head",
        "branch",
        "stash",
    ] {
        let dir = fixture();
        let repo = dir.path();
        let p = review(repo);
        match change {
            "working" => put(repo, "mixed.txt", "new unstaged content\n"),
            "untracked" => put(repo, "new file.txt", "new untracked content\n"),
            "new" => put(repo, "another.txt", "arrived after review\n"),
            "index" => {
                g(repo, None, &["add", "mixed.txt"]).unwrap();
            }
            "head" => {
                g(repo, None, &["commit", "--allow-empty", "-qm", "external"]).unwrap();
            }
            "branch" => {
                g(repo, None, &["checkout", "-qb", "other"]).unwrap();
            }
            "stash" => {
                g(repo, None, &["stash", "push", "-u", "-m", "external"]).unwrap();
            }
            _ => unreachable!(),
        }
        let before = Snapshot::read(repo).unwrap();
        assert!(execute(&p).is_err(), "{change}");
        assert!(Snapshot::read(repo).unwrap() == before, "{change}");
    }
}

#[test]
fn stash_index_lock_failure_does_not_publish_or_remove_files() {
    let dir = fixture();
    let repo = dir.path();
    let p = review(repo);
    let before = Snapshot::read(repo).unwrap();
    fs::write(repo.join(".git/index.lock"), "other operation").unwrap();
    assert!(execute(&p).is_err());
    assert!(Snapshot::read(repo).unwrap() == before);
    assert!(repo.join("new file.txt").exists());
}

#[test]
fn stash_rejects_unborn_operations_conflicts_and_nested_repositories() {
    let dir = setup();
    let repo = dir.path();
    put(repo, "new.txt", "new\n");
    assert!(plan(0, repo, "stash")
        .err()
        .unwrap()
        .to_string()
        .contains("initial commit"));
    commit_all(repo);
    put(repo, "new.txt", "changed\n");
    fs::write(repo.join(".git/MERGE_HEAD"), oid(repo).unwrap().unwrap()).unwrap();
    assert!(plan(0, repo, "stash")
        .err()
        .unwrap()
        .to_string()
        .contains("in progress"));
    fs::remove_file(repo.join(".git/MERGE_HEAD")).unwrap();
    g(repo, None, &["checkout", "-qb", "other"]).unwrap();
    commit_all(repo);
    g(
        repo,
        None,
        &["checkout", "-q", "HEAD^", "-b", "conflicting"],
    )
    .unwrap();
    put(repo, "new.txt", "conflicting\n");
    commit_all(repo);
    assert!(g(repo, None, &["merge", "other"]).is_err());
    assert!(plan(0, repo, "stash").is_err());
    g(repo, None, &["merge", "--abort"]).unwrap();
    fs::create_dir(repo.join("nested")).unwrap();
    g(&repo.join("nested"), None, &["init", "-q"]).unwrap();
    assert!(plan(0, repo, "stash").is_err());
    assert!(tip(repo).unwrap().is_none());
}

#[test]
fn stash_rejects_changed_submodule_pointers() {
    let dir = setup();
    let repo = dir.path();
    put(repo, "file.txt", "base\n");
    commit_all(repo);
    let base = oid(repo).unwrap().unwrap();
    g(
        repo,
        None,
        &[
            "update-index",
            "--add",
            "--cacheinfo",
            "160000",
            &base,
            "module",
        ],
    )
    .unwrap();
    assert!(plan(0, repo, "stash")
        .err()
        .unwrap()
        .to_string()
        .contains("submodule pointer"));
    assert!(tip(repo).unwrap().is_none());
}

#[test]
fn stash_batch_supports_linked_worktrees_with_a_shared_reflog() {
    let dir = setup();
    let repo = dir.path();
    put(repo, "file.txt", "base\n");
    commit_all(repo);
    let container = tempfile::tempdir().unwrap();
    let linked = container.path().join("linked");
    g(
        repo,
        None,
        &[
            "worktree",
            "add",
            "--detach",
            linked.to_str().unwrap(),
            "HEAD",
        ],
    )
    .unwrap();
    put(repo, "file.txt", "main worktree\n");
    put(&linked, "file.txt", "linked worktree\n");
    let first = review(repo);
    let second = review(&linked);
    assert!(execute(&first).unwrap().warning.is_none());
    assert!(execute(&second).unwrap().warning.is_none());
    assert_eq!(read(repo, "stash@{0}:file.txt"), "linked worktree");
    assert_eq!(read(repo, "stash@{1}:file.txt"), "main worktree");
    assert!(status(repo).unwrap().is_empty());
    assert!(status(&linked).unwrap().is_empty());
}

#[test]
fn stash_does_not_recurse_into_a_hidden_dirty_submodule() {
    let origin = setup();
    put(origin.path(), "child.txt", "base\n");
    commit_all(origin.path());
    let dir = setup();
    let repo = dir.path();
    put(repo, "parent.txt", "base\n");
    commit_all(repo);
    g(
        repo,
        None,
        &[
            "-c",
            "protocol.file.allow=always",
            "submodule",
            "add",
            origin.path().to_str().unwrap(),
            "module",
        ],
    )
    .unwrap();
    commit_all(repo);
    g(repo, None, &["config", "submodule.recurse", "true"]).unwrap();
    let child = repo.join("module");
    put(&child, "child.txt", "staged child\n");
    g(&child, None, &["add", "child.txt"]).unwrap();
    put(&child, "child.txt", "working child\n");
    put(&child, "new.txt", "untracked child\n");
    let index = g(&child, None, &["ls-files", "--stage", "-z"]).unwrap();
    put(repo, "parent.txt", "working parent\n");
    let result = execute(&review(repo)).unwrap();
    assert!(result.warning.is_none());
    assert_eq!(
        g(&child, None, &["ls-files", "--stage", "-z"]).unwrap(),
        index
    );
    assert_eq!(
        fs::read_to_string(child.join("child.txt")).unwrap(),
        "working child\n"
    );
    assert_eq!(
        fs::read_to_string(child.join("new.txt")).unwrap(),
        "untracked child\n"
    );
    assert!(tip(&child).unwrap().is_none());
}

#[cfg(unix)]
#[test]
fn stash_handles_native_names_binary_files_symlinks_and_modes() {
    use std::os::unix::{
        ffi::OsStringExt,
        fs::{symlink, PermissionsExt},
    };
    let dir = setup();
    let repo = dir.path();
    put(repo, "base.txt", "base\n");
    commit_all(repo);
    let native = std::ffi::OsString::from_vec(b"non-utf8-\xff".to_vec());
    // APFS rejects invalid UTF-8 names; exercise native byte names where supported.
    let native = if fs::write(repo.join(&native), b"binary\0data").is_ok() {
        native
    } else {
        let name = std::ffi::OsString::from("binary 中文");
        fs::write(repo.join(&name), b"binary\0data").unwrap();
        name
    };
    let unusual = "--odd [x]\n中文.txt";
    put(repo, unusual, "unicode\n");
    fs::set_permissions(repo.join("base.txt"), fs::Permissions::from_mode(0o755)).unwrap();
    symlink("missing-target", repo.join("link")).unwrap();
    let result = execute(&review(repo)).unwrap();
    assert!(result.warning.is_none());
    g(repo, None, &["stash", "apply", "--index", &result.hash]).unwrap();
    assert_eq!(fs::read(repo.join(&native)).unwrap(), b"binary\0data");
    assert_eq!(fs::read_to_string(repo.join(unusual)).unwrap(), "unicode\n");
    assert_eq!(
        fs::read_link(repo.join("link")).unwrap(),
        Path::new("missing-target")
    );
    assert_ne!(
        fs::metadata(repo.join("base.txt"))
            .unwrap()
            .permissions()
            .mode()
            & 0o111,
        0
    );
}

#[cfg(unix)]
#[test]
fn stash_reports_published_object_when_cleanup_fails() {
    use std::os::unix::fs::PermissionsExt;
    let dir = fixture();
    let repo = dir.path();
    let hook = repo.join(".git/hooks/reference-transaction");
    fs::write(&hook, "#!/bin/sh\nif [ \"$1\" = committed ]; then\nwhile read old new ref; do\nif [ \"$ref\" = refs/stash ]; then : > .git/index.lock; fi\ndone\nfi\n").unwrap();
    fs::set_permissions(hook, fs::Permissions::from_mode(0o755)).unwrap();
    let result = execute(&review(repo)).unwrap();
    assert_eq!(tip(repo).unwrap().as_deref(), Some(result.hash.as_str()));
    assert!(result
        .warning
        .as_deref()
        .unwrap()
        .contains("cleanup failed"));
    assert_eq!(read(repo, "stash@{0}:mixed.txt"), "unstaged 中文");
    assert_eq!(read(repo, "stash@{0}^3:new file.txt"), "new");
}
