use super::*;
use crate::commit::backend::tests::{commit_all, get, put, setup};

fn plan(repo: &Path, paths: &[&str]) -> Plan {
    let files = status(repo)
        .unwrap()
        .into_iter()
        .filter(|file| paths.iter().any(|path| file.path == Path::new(path)))
        .collect();
    review(repo, files).unwrap()
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
        assert!(review(repo, vec![file]).is_err());
    }
    assert_eq!(fs::read(repo.join(".git/index")).unwrap(), index);
}

#[cfg(unix)]
#[test]
fn add_to_git_batched_paths_preserve_newlines_bytes_symlinks_and_unselected_siblings() {
    use std::os::unix::{ffi::OsStringExt, fs::symlink};
    let dir = setup();
    let repo = dir.path();
    put(repo, "tracked", "base\n");
    commit_all(repo);
    let mut names = vec![
        PathBuf::from("line\nbreak[1].txt"),
        PathBuf::from("-leading-dash"),
    ];
    let non_utf8 = PathBuf::from(OsString::from_vec(b"non-utf8-\xff.txt".to_vec()));
    match fs::write(repo.join(&non_utf8), b"exact\0bytes\n") {
        Ok(()) => names.push(non_utf8),
        // APFS rejects invalid UTF-8 filenames; retain the other literal-path checks.
        Err(error) if matches!(error.raw_os_error(), Some(22 | 84 | 92)) => (),
        Err(error) => panic!("Cannot create filename fixture: {error}"),
    }
    for name in &names {
        fs::write(repo.join(name), b"exact\0bytes\n").unwrap();
    }
    symlink("tracked", repo.join("link")).unwrap();
    put(repo, "line\nbreak1.txt", "keep untracked\n");
    let files = status(repo)
        .unwrap()
        .into_iter()
        .filter(|file| names.contains(&file.path) || file.path == Path::new("link"))
        .collect();
    let p = review(repo, files).unwrap();
    execute(&p).unwrap();
    let changes = status(repo).unwrap();
    for name in &names {
        assert!(changes
            .iter()
            .any(|file| &file.path == name && file.xy == *b"A "));
        assert_eq!(fs::read(repo.join(name)).unwrap(), b"exact\0bytes\n");
    }
    assert_eq!(
        fs::read_link(repo.join("link")).unwrap(),
        PathBuf::from("tracked")
    );
    assert!(changes
        .iter()
        .any(|file| file.path == Path::new("link") && file.xy == *b"A "));
    assert!(changes
        .iter()
        .any(|file| file.path == Path::new("line\nbreak1.txt") && file.xy == *b"??"));
}

#[test]
#[ignore = "manual Add to Git performance measurement"]
fn add_to_git_profile_batch() {
    use std::time::Instant;
    for count in [6, 60] {
        let dir = setup();
        let repo = dir.path();
        put(repo, "tracked", "base\n");
        commit_all(repo);
        let paths: Vec<_> = (0..count).map(|i| format!("new-{i:03}.txt")).collect();
        let contents = "untracked content\n".repeat(256);
        for path in &paths {
            put(repo, path, &contents);
        }
        let started = Instant::now();
        let p = plan(repo, &paths.iter().map(String::as_str).collect::<Vec<_>>());
        let prepared = started.elapsed();
        let writing = Instant::now();
        execute(&p).unwrap();
        let written = writing.elapsed();
        let scanning = Instant::now();
        let changes = status(repo).unwrap();
        let scanned = scanning.elapsed();
        assert_eq!(changes.len(), count);
        assert!(changes.iter().all(|file| file.xy == *b"A "));
        for path in &paths {
            assert_eq!(fs::read_to_string(repo.join(path)).unwrap(), contents);
        }
        eprintln!(
            "ADD_PROFILE files={count} prepare_ms={} write_ms={} status_ms={} total_ms={}",
            prepared.as_millis(),
            written.as_millis(),
            scanned.as_millis(),
            (prepared + written + scanned).as_millis()
        );
    }
}
