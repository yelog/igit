use super::*;

#[test]
fn staged_deletion_keeps_unchecked_replacement_directory_out_of_the_patch() {
    for stage_child in [false, true] {
        let dir = setup();
        let repo = dir.path();
        put(repo, "node[1]", "reviewed old leaf\n");
        commit_all(repo);
        fs::remove_file(repo.join("node[1]")).unwrap();
        fs::create_dir(repo.join("node[1]")).unwrap();
        put(repo, "node[1]/private", "PRIVATE_CHILD\n");
        if stage_child {
            g(repo, None, &["add", "-A"]).unwrap();
        } else {
            g(repo, None, &["rm", "--cached", "--", "node[1]"]).unwrap();
        }
        let before = unchanged(repo);
        let snapshot = capture(repo, &["node[1]"]);
        assert_eq!(snapshot.files[0].status, "D");
        assert!(snapshot.files[0].patch.contains("-reviewed old leaf"));
        assert!(!snapshot.files[0].patch.contains("PRIVATE_CHILD"));
        put(repo, "node[1]/private", "another unchecked child edit\n");
        snapshot.validate(&control()).unwrap();
        assert_eq!(unchanged(repo), before);
    }
}

#[test]
fn working_deletion_does_not_read_unchecked_directory_children() {
    let dir = setup();
    let repo = dir.path();
    put(repo, "node", "old leaf\n");
    commit_all(repo);
    fs::remove_file(repo.join("node")).unwrap();
    fs::create_dir(repo.join("node")).unwrap();
    put(repo, "node/private", "PRIVATE_CHILD\n");
    let snapshot = capture(repo, &["node"]);
    assert_eq!(snapshot.files[0].status, "D");
    assert!(!snapshot.files[0].patch.contains("PRIVATE_CHILD"));
    put(repo, "node/private", "later private child\n");
    snapshot.validate(&control()).unwrap();
}

#[test]
fn directory_replacement_requires_all_affected_children_in_any_selection_order() {
    let dir = setup();
    let repo = dir.path();
    fs::create_dir(repo.join("node")).unwrap();
    put(repo, "node/a", "old a\n");
    put(repo, "node/b", "old b\n");
    commit_all(repo);
    fs::remove_dir_all(repo.join("node")).unwrap();
    put(repo, "node", "new leaf\n");
    let before = unchanged(repo);
    assert!(Snapshot::capture(repo, &selected(repo, &[Path::new("node")]), &control()).is_err());
    for paths in [["node", "node/a", "node/b"], ["node/b", "node/a", "node"]] {
        let snapshot = capture(repo, &paths);
        assert_eq!(
            snapshot
                .files
                .iter()
                .map(|file| file.status.as_str())
                .collect::<Vec<_>>(),
            paths.map(|path| if path == "node" { "A" } else { "D" })
        );
        snapshot.validate(&control()).unwrap();
    }
    assert_eq!(unchanged(repo), before);
}

#[test]
fn a_staged_leaf_and_a_working_child_cannot_silently_displace_each_other() {
    let dir = setup();
    let repo = dir.path();
    put(repo, "node", "old leaf\n");
    commit_all(repo);
    put(repo, "node", "selected staged edit\n");
    g(repo, None, &["add", "node"]).unwrap();
    fs::remove_file(repo.join("node")).unwrap();
    fs::create_dir(repo.join("node")).unwrap();
    put(repo, "node/child", "working child\n");
    let before = unchanged(repo);
    let error = Snapshot::capture(
        repo,
        &selected(repo, &[Path::new("node"), Path::new("node/child")]),
        &control(),
    )
    .unwrap_err();
    assert!(error.to_string().contains("cannot coexist"), "{error:#}");
    assert_eq!(unchanged(repo), before);
    assert_eq!(get(repo, &["show", ":node"]), "selected staged edit");
}

#[test]
fn stale_staging_is_rejected_before_freezing_and_nested_repositories_are_not_walked() {
    let dir = setup();
    let repo = dir.path();
    put(repo, "file", "base\n");
    commit_all(repo);
    put(repo, "file", "working\n");
    let files = selected(repo, &[Path::new("file")]);
    g(repo, None, &["add", "file"]).unwrap();
    assert!(Snapshot::capture(repo, &files, &control()).is_err());
    fs::create_dir(repo.join("nested")).unwrap();
    g(&repo.join("nested"), None, &["init", "-q"]).unwrap();
    put(repo, "nested/private", "PRIVATE_NESTED_CONTENT\n");
    let nested = Change {
        path: "nested".into(),
        xy: *b"??",
        conflict: false,
    };
    assert!(Snapshot::capture(repo, &[nested], &control()).is_err());
}

#[test]
fn gitlink_patch_freezes_the_child_revision_and_ignores_child_dirt() {
    let dir = setup();
    let repo = dir.path();
    let child = setup();
    put(child.path(), "file", "first\n");
    commit_all(child.path());
    g(
        repo,
        None,
        &[
            "-c",
            "protocol.file.allow=always",
            "submodule",
            "add",
            "-q",
            child.path().to_str().unwrap(),
            "sdk",
        ],
    )
    .unwrap();
    commit_all(repo);
    let sdk = repo.join("sdk");
    for args in [
        ["config", "user.name", "Fixture"],
        ["config", "user.email", "fixture@invalid.test"],
        ["config", "commit.gpgsign", "false"],
    ] {
        g(&sdk, None, &args).unwrap();
    }
    put(&sdk, "file", "second\n");
    commit_all(&sdk);
    let snapshot = capture(repo, &["sdk"]);
    assert_eq!(snapshot.files[0].status, "M");
    assert!(snapshot.files[0].patch.contains("+Subproject commit "));
    assert!(!snapshot.files[0].patch.contains("+second"));
    put(&sdk, "file", "PRIVATE_DIRT\n");
    snapshot.validate(&control()).unwrap();
    g(repo, None, &["add", "sdk"]).unwrap();
    let staged = capture(repo, &["sdk"]);
    commit_all(&sdk);
    staged.validate(&control()).unwrap();
    assert!(snapshot.validate(&control()).is_err());
}
