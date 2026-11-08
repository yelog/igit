use super::*;
use crate::commit::backend::tests::{commit_all, get, put, setup};

fn plan(repo: &Path, paths: &[&str]) -> Plan {
    let files = status(repo)
        .unwrap()
        .into_iter()
        .filter(|file| paths.iter().any(|path| file.path == Path::new(path)))
        .collect();
    review(0, repo, files).unwrap()
}

#[test]
fn add_to_git_preserves_worktree_and_late_unrelated_staging_with_git_filters() {
    let dir = setup();
    let repo = dir.path();
    put(repo, ".gitattributes", "*.txt text eol=lf\n");
    put(repo, "mixed", "base\n");
    put(repo, "other", "base\n");
    commit_all(repo);
    put(repo, "mixed", "staged\n");
    g(repo, None, &["add", "mixed"]).unwrap();
    put(repo, "mixed", "working\n");
    put(repo, "new 中文[1].txt", "normalize\r\n");
    put(repo, "new 中文1.txt", "keep untracked\n");
    fs::write(repo.join("binary"), [0, 1, 255]).unwrap();
    let index = fs::read(repo.join(".git/index")).unwrap();
    let head = oid(repo).unwrap();
    let p = plan(repo, &["new 中文[1].txt", "binary"]);
    assert_eq!(fs::read(repo.join(".git/index")).unwrap(), index);
    put(repo, "other", "staged after review\n");
    g(repo, None, &["add", "other"]).unwrap();
    execute(&p).unwrap();
    assert_eq!(
        g(repo, None, &["show", ":new 中文[1].txt"]).unwrap(),
        b"normalize\n"
    );
    assert_eq!(
        fs::read(repo.join("new 中文[1].txt")).unwrap(),
        b"normalize\r\n"
    );
    assert_eq!(g(repo, None, &["show", ":binary"]).unwrap(), [0, 1, 255]);
    assert_eq!(get(repo, &["show", ":mixed"]), "staged");
    assert_eq!(get(repo, &["show", ":other"]), "staged after review");
    assert_eq!(fs::read(repo.join("mixed")).unwrap(), b"working\n");
    assert!(status(repo)
        .unwrap()
        .iter()
        .any(|f| f.path == Path::new("new 中文1.txt") && f.xy == *b"??"));
    assert_eq!(oid(repo).unwrap(), head);
    assert!(
        execute(&p).is_err(),
        "a consumed review cannot stage the same path again"
    );
}

#[test]
fn add_to_git_stale_content_staging_ignore_rules_and_filters_preserve_index() {
    for mutation in ["content", "staging", "ignored", "filter", "branch"] {
        let dir = setup();
        let repo = dir.path();
        g(repo, None, &["config", "core.autocrlf", "false"]).unwrap();
        put(repo, "base", "base\n");
        commit_all(repo);
        put(repo, "first.txt", "first\r\n");
        put(repo, "second", "second\n");
        let p = plan(repo, &["first.txt", "second"]);
        match mutation {
            "content" => put(repo, "second", "changed\n"),
            "staging" => {
                g(repo, None, &["add", "second"]).unwrap();
            }
            "ignored" => put(repo, ".gitignore", "second\n"),
            "filter" => put(repo, ".gitattributes", "*.txt text eol=lf\n"),
            "branch" => {
                g(repo, None, &["switch", "-qc", "changed-branch"]).unwrap();
            }
            _ => unreachable!(),
        }
        let index = fs::read(repo.join(".git/index")).unwrap();
        assert!(execute(&p).is_err(), "{mutation}");
        assert_eq!(
            fs::read(repo.join(".git/index")).unwrap(),
            index,
            "{mutation}"
        );
        assert!(!repo.join(".git/index.lock").exists());
        assert_eq!(fs::read(repo.join("first.txt")).unwrap(), b"first\r\n");
    }
}

#[test]
fn add_to_git_unborn_head_and_existing_index_lock() {
    let dir = setup();
    let repo = dir.path();
    put(repo, "new", "initial\n");
    let p = plan(repo, &["new"]);
    assert!(!repo.join(".git/index").exists());
    put(repo, ".git/index.lock", "another process");
    assert!(execute(&p).is_err());
    assert_eq!(
        fs::read(repo.join(".git/index.lock")).unwrap(),
        b"another process"
    );
    assert!(!repo.join(".git/index").exists());
    fs::remove_file(repo.join(".git/index.lock")).unwrap();
    execute(&p).unwrap();
    assert_eq!(get(repo, &["show", ":new"]), "initial");
    assert!(oid(repo).unwrap().is_none());
}

#[test]
fn add_to_git_file_directory_replacement_cannot_evict_unselected_staging() {
    let dir = setup();
    let repo = dir.path();
    put(repo, "base", "base\n");
    commit_all(repo);
    put(repo, "node", "staged ancestor\n");
    g(repo, None, &["add", "node"]).unwrap();
    fs::remove_file(repo.join("node")).unwrap();
    fs::create_dir(repo.join("node")).unwrap();
    put(repo, "node/child", "untracked child\n");
    let index = fs::read(repo.join(".git/index")).unwrap();
    let p = plan(repo, &["node/child"]);
    assert!(execute(&p).is_err());
    assert_eq!(fs::read(repo.join(".git/index")).unwrap(), index);
    assert_eq!(get(repo, &["show", ":node"]), "staged ancestor");
    assert_eq!(
        fs::read(repo.join("node/child")).unwrap(),
        b"untracked child\n"
    );
}

#[test]
fn add_to_git_rejects_nested_repositories_and_tracked_inputs() {
    let dir = setup();
    let repo = dir.path();
    put(repo, "tracked", "base\n");
    commit_all(repo);
    put(repo, "tracked", "changed\n");
    fs::create_dir(repo.join("nested")).unwrap();
    g(&repo.join("nested"), None, &["init", "-q"]).unwrap();
    let index = fs::read(repo.join(".git/index")).unwrap();
    for file in status(repo).unwrap() {
        assert!(review(0, repo, vec![file]).is_err());
    }
    assert_eq!(fs::read(repo.join(".git/index")).unwrap(), index);
}
