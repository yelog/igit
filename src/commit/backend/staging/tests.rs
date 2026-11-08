use super::*;
use crate::commit::backend::{
    self,
    tests::{commit_all, get, put, setup},
};

fn lines() -> String {
    (0..60).map(|i| format!("line {i:02}\n")).collect()
}

fn seeded() -> TempDir {
    let dir = setup();
    let repo = dir.path();
    g(repo, None, &["config", "core.autocrlf", "false"]).unwrap();
    put(repo, "file", &lines());
    put(repo, "sibling", "base sibling\n");
    commit_all(repo);
    dir
}

fn snapshot(repo: &Path, side: Side) -> Snapshot {
    read(7, repo, Path::new("file"), side).unwrap()
}

fn bytes(repo: &Path, spec: &str) -> Vec<u8> {
    g(repo, None, &["show", spec]).unwrap()
}

fn index(repo: &Path) -> Vec<u8> {
    fs::read(git_path(repo, "index").unwrap()).unwrap()
}

fn hash(repo: &Path, contents: &[u8]) -> String {
    txt(checked(
        run(
            repo,
            None,
            &["hash-object".as_ref(), "--stdin".as_ref()],
            Some(contents),
        )
        .unwrap(),
    )
    .unwrap())
}

#[test]
fn stage_one_hunk_preserves_existing_and_later_unrelated_staging() {
    let dir = seeded();
    let repo = dir.path();
    let staged = lines().replace("line 02\n", "previously staged\n");
    put(repo, "file", &staged);
    put(repo, "sibling", "staged sibling\n");
    g(repo, None, &["add", "file", "sibling"]).unwrap();
    let working = staged
        .replace("line 22\n", "chosen hunk\n")
        .replace("line 50\n", "keep working\n");
    put(repo, "file", &working);
    let head = oid(repo).unwrap();
    let before = index(repo);
    let snapshot = snapshot(repo, Side::Unstaged);
    assert_eq!(snapshot.root, 7);
    assert_eq!(snapshot.hunks.len(), 2);
    let plan = plan(&snapshot, 0).unwrap();
    assert!(plan.patch.contains("+chosen hunk"));
    assert!(!plan.patch.contains("keep working"));
    let expected = staged.replace("line 22\n", "chosen hunk\n");
    // A partial patch's destination hash must describe the partial result,
    // rather than the original full working-tree diff's destination blob.
    assert!(plan
        .patch
        .contains(&format!("..{} 100644", hash(repo, expected.as_bytes()))));
    assert_eq!(index(repo), before);
    assert_eq!(fs::read(repo.join("file")).unwrap(), working.as_bytes());
    put(repo, "sibling", "new staging after review\n");
    put(repo, "later", "added after review\n");
    g(repo, None, &["add", "sibling", "later"]).unwrap();
    let unrelated = entry(repo, None, Path::new("sibling")).unwrap();
    let later = entry(repo, None, Path::new("later")).unwrap();
    execute(&plan).unwrap();
    assert_eq!(bytes(repo, ":file"), expected.as_bytes());
    assert_eq!(entry(repo, None, Path::new("sibling")).unwrap(), unrelated);
    assert_eq!(entry(repo, None, Path::new("later")).unwrap(), later);
    assert_eq!(fs::read(repo.join("file")).unwrap(), working.as_bytes());
    assert_eq!(oid(repo).unwrap(), head);
    assert!(!git_path(repo, "index.lock").unwrap().exists());
}

#[test]
fn either_of_two_separated_hunks_can_be_staged_and_review_is_read_only() {
    for hunk in 0..2 {
        let dir = seeded();
        let repo = dir.path();
        let working = lines()
            .replace("line 03\n", "first\n")
            .replace("line 49\n", "second\n");
        put(repo, "file", &working);
        let before = index(repo);
        let head = oid(repo).unwrap();
        let snapshot = snapshot(repo, Side::Unstaged);
        assert_eq!(snapshot.hunks.len(), 2);
        assert_eq!(
            snapshot
                .hunks
                .iter()
                .map(|h| h.text.as_str())
                .collect::<String>(),
            snapshot.patch[split(&snapshot.patch).unwrap().0.len()..]
        );
        let cancelled = plan(&snapshot, hunk).unwrap();
        drop(cancelled);
        assert_eq!(index(repo), before);
        assert_eq!(oid(repo).unwrap(), head);
        assert_eq!(fs::read(repo.join("file")).unwrap(), working.as_bytes());
        let selected = plan(&snapshot, hunk).unwrap();
        execute(&selected).unwrap();
        let expected = if hunk == 0 {
            lines().replace("line 03\n", "first\n")
        } else {
            lines().replace("line 49\n", "second\n")
        };
        assert_eq!(bytes(repo, ":file"), expected.as_bytes());
        assert_eq!(
            read(7, repo, Path::new("file"), Side::Unstaged)
                .unwrap()
                .hunks
                .len(),
            1
        );
        let after = index(repo);
        assert!(execute(&selected).is_err(), "a review must not be replayed");
        assert_eq!(index(repo), after);
    }
}

#[test]
fn staged_hunk_then_selected_commit_keeps_remaining_work_and_sibling_staging() {
    let dir = seeded();
    let repo = dir.path();
    let working = lines()
        .replace("line 03\n", "commit this\n")
        .replace("line 49\n", "leave working\n");
    put(repo, "file", &working);
    put(repo, "sibling", "unselected staged\n");
    g(repo, None, &["add", "sibling"]).unwrap();
    execute(&plan(&snapshot(repo, Side::Unstaged), 0).unwrap()).unwrap();
    let selected = status(repo)
        .unwrap()
        .into_iter()
        .filter(|change| change.path == Path::new("file"))
        .collect();
    let commit = backend::plan(7, repo, selected, "selected hunk".into(), false).unwrap();
    backend::execute(&commit).unwrap();
    assert_eq!(
        bytes(repo, "HEAD:file"),
        lines().replace("line 03\n", "commit this\n").as_bytes()
    );
    assert_eq!(bytes(repo, "HEAD:sibling"), b"base sibling\n");
    assert_eq!(bytes(repo, ":sibling"), b"unselected staged\n");
    assert_eq!(fs::read(repo.join("file")).unwrap(), working.as_bytes());
    assert!(get(repo, &["diff", "--", "file"]).contains("+leave working"));
}

#[test]
fn unstage_only_chosen_hunk_with_offsets_from_an_earlier_insertion() {
    for hunk in 0..2 {
        let dir = seeded();
        let repo = dir.path();
        let first = lines().replace("line 03\n", "insert one\ninsert two\ninsert three\n");
        let staged = first.replace("line 49\n", "later staged change\n");
        put(repo, "file", &staged);
        put(repo, "sibling", "keep staged\n");
        g(repo, None, &["add", "file", "sibling"]).unwrap();
        let working = staged.replace("line 25\n", "working only\n");
        put(repo, "file", &working);
        let snapshot = snapshot(repo, Side::Staged);
        assert_eq!(snapshot.hunks.len(), 2);
        let head = oid(repo).unwrap();
        let selected = plan(&snapshot, hunk).unwrap();
        assert_eq!(selected.side.action(), "Unstage");
        execute(&selected).unwrap();
        let expected = if hunk == 0 {
            lines().replace("line 49\n", "later staged change\n")
        } else {
            first
        };
        assert_eq!(bytes(repo, ":file"), expected.as_bytes());
        assert_eq!(bytes(repo, ":sibling"), b"keep staged\n");
        assert_eq!(fs::read(repo.join("file")).unwrap(), working.as_bytes());
        assert_eq!(oid(repo).unwrap(), head);
    }
}

fn mutate(repo: &Path, mutation: &str) {
    match mutation {
        "working content" => put(
            repo,
            "file",
            &lines()
                .replace("line 03\n", "reviewed\n")
                .replace("line 40\n", "external\n"),
        ),
        "index content" => {
            g(repo, None, &["add", "file"]).unwrap();
        }
        "branch" => {
            g(repo, None, &["branch", "other"]).unwrap();
            g(repo, None, &["symbolic-ref", "HEAD", "refs/heads/other"]).unwrap();
        }
        "HEAD" => {
            g(repo, None, &["commit", "--allow-empty", "-qm", "external"]).unwrap();
        }
        "conversion" => {
            g(repo, None, &["config", "core.autocrlf", "true"]).unwrap();
        }
        "config" => {
            g(repo, None, &["config", "diff.context", "9"]).unwrap();
        }
        "attributes" => put(repo, ".gitattributes", "file text\n"),
        "index flags" => {
            g(repo, None, &["update-index", "--assume-unchanged", "file"]).unwrap();
        }
        _ => unreachable!(),
    }
}

#[test]
fn stale_content_index_head_branch_attributes_and_config_never_refresh_a_review() {
    for mutation in [
        "working content",
        "index content",
        "branch",
        "HEAD",
        "conversion",
        "config",
        "attributes",
        "index flags",
    ] {
        for during_review in [false, true] {
            let dir = seeded();
            let repo = dir.path();
            put(repo, "file", &lines().replace("line 03\n", "reviewed\n"));
            let snapshot = snapshot(repo, Side::Unstaged);
            let prepared = during_review.then(|| plan(&snapshot, 0).unwrap());
            mutate(repo, mutation);
            let before = index(repo);
            let head = oid(repo).unwrap();
            let work = fs::read(repo.join("file")).unwrap();
            let result = if let Some(prepared) = prepared {
                execute(&prepared)
            } else {
                plan(&snapshot, 0).map(|_| ())
            };
            assert!(result.is_err(), "{mutation}, prepared={during_review}");
            assert_eq!(index(repo), before, "{mutation}");
            assert_eq!(oid(repo).unwrap(), head, "{mutation}");
            assert_eq!(fs::read(repo.join("file")).unwrap(), work, "{mutation}");
            assert!(!git_path(repo, "index.lock").unwrap().exists());
        }
    }
}

#[test]
fn staged_review_also_checks_raw_working_content_and_selected_index_entry() {
    for mutation in ["working", "index"] {
        let dir = seeded();
        let repo = dir.path();
        put(repo, "file", &lines().replace("line 03\n", "staged\n"));
        g(repo, None, &["add", "file"]).unwrap();
        let selected = plan(&snapshot(repo, Side::Staged), 0).unwrap();
        put(repo, "file", &lines().replace("line 03\n", "external\n"));
        if mutation == "index" {
            g(repo, None, &["add", "file"]).unwrap();
        }
        let before = index(repo);
        assert!(execute(&selected).is_err());
        assert_eq!(index(repo), before);
    }
}

#[test]
fn clean_sides_have_no_hunks_and_invalid_or_modified_reviews_cannot_execute() {
    let dir = seeded();
    let repo = dir.path();
    for side in [Side::Staged, Side::Unstaged] {
        let clean = snapshot(repo, side);
        assert!(clean.patch.is_empty() && clean.hunks.is_empty());
        assert!(plan(&clean, 0).is_err());
    }
    put(repo, "file", &lines().replace("line 03\n", "working\n"));
    assert!(snapshot(repo, Side::Staged).hunks.is_empty());
    let mut source = snapshot(repo, Side::Unstaged);
    assert!(plan(&source, 8).is_err());
    let before = index(repo);
    let mut selected = plan(&source, 0).unwrap();
    selected.patch.push('\n');
    assert!(execute(&selected).is_err());
    source.hunks[0].text.clear();
    assert!(plan(&source, 0).is_err());
    assert_eq!(index(repo), before);
}

#[test]
fn an_existing_index_lock_is_preserved() {
    let dir = seeded();
    let repo = dir.path();
    put(repo, "file", "reviewed\n");
    let selected = plan(&snapshot(repo, Side::Unstaged), 0).unwrap();
    let before = index(repo);
    let lock = git_path(repo, "index.lock").unwrap();
    fs::write(&lock, b"another operation").unwrap();
    assert!(execute(&selected).is_err());
    assert_eq!(fs::read(lock).unwrap(), b"another operation");
    assert_eq!(index(repo), before);
}

#[test]
fn no_newline_markers_and_empty_text_are_exact_in_both_directions() {
    for (base, edited) in [
        ("old without newline", "new without newline"),
        ("old\n", "new"),
        ("old", "new\n"),
        ("", "new"),
        ("old", ""),
    ] {
        let dir = setup();
        let repo = dir.path();
        g(repo, None, &["config", "core.autocrlf", "false"]).unwrap();
        put(repo, "file", base);
        commit_all(repo);
        put(repo, "file", edited);
        let source = snapshot(repo, Side::Unstaged);
        assert_eq!(source.hunks.len(), 1);
        if !base.is_empty() || !edited.is_empty() {
            assert!(source.patch.contains("\\ No newline at end of file"));
        }
        execute(&plan(&source, 0).unwrap()).unwrap();
        assert_eq!(bytes(repo, ":file"), edited.as_bytes());
        execute(&plan(&snapshot(repo, Side::Staged), 0).unwrap()).unwrap();
        assert_eq!(bytes(repo, ":file"), base.as_bytes());
        assert_eq!(fs::read(repo.join("file")).unwrap(), edited.as_bytes());
    }
}

#[test]
fn initial_commit_addition_and_tracked_deletion_leave_working_files_untouched() {
    let dir = setup();
    let repo = dir.path();
    g(repo, None, &["config", "core.autocrlf", "false"]).unwrap();
    put(repo, "file", "initial content");
    g(repo, None, &["add", "file"]).unwrap();
    let before = index(repo);
    let source = snapshot(repo, Side::Staged);
    let selected = plan(&source, 0).unwrap();
    assert_eq!(index(repo), before);
    execute(&selected).unwrap();
    assert!(entry(repo, None, Path::new("file")).unwrap().is_empty());
    assert_eq!(fs::read(repo.join("file")).unwrap(), b"initial content");
    assert!(oid(repo).unwrap().is_none());
    commit_all(repo);
    fs::remove_file(repo.join("file")).unwrap();
    execute(&plan(&snapshot(repo, Side::Unstaged), 0).unwrap()).unwrap();
    assert!(entry(repo, None, Path::new("file")).unwrap().is_empty());
    execute(&plan(&snapshot(repo, Side::Staged), 0).unwrap()).unwrap();
    assert_eq!(bytes(repo, ":file"), b"initial content");
    assert!(!repo.join("file").exists());
}

#[cfg(unix)]
#[test]
fn literal_unicode_wildcard_quoted_and_native_paths_affect_only_the_selected_leaf() {
    let paths = [
        PathBuf::from("src/[literal]*?/ｕｉ/文件.txt"),
        PathBuf::from("src/[literal]*?/quote\"\tline\nname"),
        PathBuf::from(r"src/[literal]*?/back\slash"),
        PathBuf::from(":(glob)*"),
        #[cfg(not(target_os = "macos"))]
        PathBuf::from(raw(b"src/[literal]*?/bad\xff")),
    ];
    let dir = seeded();
    let repo = dir.path();
    for path in &paths {
        fs::create_dir_all(repo.join(path).parent().unwrap()).unwrap();
        fs::write(repo.join(path), lines()).unwrap();
    }
    commit_all(repo);
    for path in &paths {
        let working = lines()
            .replace("line 03\n", "chosen\n")
            .replace("line 49\n", "remaining\n");
        fs::write(repo.join(path), &working).unwrap();
        let source = read(7, repo, path, Side::Unstaged).unwrap();
        assert_eq!(source.hunks.len(), 2, "{path:?}");
        execute(&plan(&source, 0).unwrap()).unwrap();
        let selected = regular(&entry(repo, None, path).unwrap())
            .unwrap()
            .unwrap()
            .1
            .to_owned();
        assert_eq!(
            g(repo, None, &["cat-file", "blob", &selected]).unwrap(),
            lines().replace("line 03\n", "chosen\n").as_bytes()
        );
        execute(&plan(&read(7, repo, path, Side::Staged).unwrap(), 0).unwrap()).unwrap();
        assert_eq!(fs::read(repo.join(path)).unwrap(), working.as_bytes());
    }
    assert!(g(repo, None, &["diff", "--cached"]).unwrap().is_empty());
}

#[test]
fn binary_untracked_renamed_submodule_and_intent_to_add_paths_are_refused() {
    let dir = seeded();
    let repo = dir.path();
    put(repo, "new", "untracked\n");
    assert!(read(7, repo, Path::new("new"), Side::Unstaged).is_err());
    g(repo, None, &["add", "-N", "new"]).unwrap();
    assert!(read(7, repo, Path::new("new"), Side::Unstaged).is_err());
    put(repo, "file", "binary\0working\n");
    assert!(read(7, repo, Path::new("file"), Side::Unstaged).is_err());
    g(repo, None, &["add", "file"]).unwrap();
    put(repo, "file", "text working\n");
    assert!(read(7, repo, Path::new("file"), Side::Staged).is_err());
    let dir = seeded();
    let repo = dir.path();
    g(repo, None, &["mv", "file", "renamed"]).unwrap();
    for path in ["file", "renamed"] {
        assert!(read(7, repo, Path::new(path), Side::Staged).is_err());
    }
    let head = get(repo, &["rev-parse", "HEAD"]);
    g(
        repo,
        None,
        &[
            "update-index",
            "--add",
            "--cacheinfo",
            &format!("160000,{head},sdk"),
        ],
    )
    .unwrap();
    assert!(read(7, repo, Path::new("sdk"), Side::Staged).is_err());
}

#[test]
fn conversion_attributes_and_clean_filters_are_refused_before_filter_execution() {
    for attr in [
        "text eol=crlf",
        "eol=crlf",
        "working-tree-encoding=UTF-16",
        "filter=unsafe",
        "ident",
        "-diff",
        "diff=custom",
    ] {
        let dir = seeded();
        let repo = dir.path();
        put(repo, ".gitattributes", &format!("file {attr}\n"));
        g(
            repo,
            None,
            &["config", "filter.unsafe.clean", "touch filter-ran; cat"],
        )
        .unwrap();
        g(
            repo,
            None,
            &["config", "diff.custom.command", "touch filter-ran"],
        )
        .unwrap();
        put(repo, "file", "changed\n");
        let before = index(repo);
        assert!(
            read(7, repo, Path::new("file"), Side::Unstaged).is_err(),
            "{attr}"
        );
        assert!(!repo.join("filter-ran").exists(), "{attr}");
        assert_eq!(index(repo), before);
    }
}

#[test]
fn byte_preserving_lf_attributes_and_input_autocrlf_support_both_directions() {
    for (rule, autocrlf) in [
        ("text eol=lf", "false"),
        ("text=auto eol=lf", "true"),
        ("eol=lf", "true"),
        ("text", "input"),
        ("", "input"),
        ("-text", "true"),
    ] {
        let dir = seeded();
        let repo = dir.path();
        put(repo, ".gitattributes", &format!("file {rule}\n"));
        g(repo, None, &["config", "core.autocrlf", autocrlf]).unwrap();
        let working = lines()
            .replace("line 03\n", "chosen\n")
            .replace("line 49\n", "remaining\n");
        put(repo, "file", &working);
        let before = index(repo);
        let selected = plan(&snapshot(repo, Side::Unstaged), 0).unwrap();
        assert_eq!(index(repo), before);
        execute(&selected).unwrap();
        assert_eq!(
            bytes(repo, ":file"),
            lines().replace("line 03\n", "chosen\n").as_bytes()
        );
        execute(&plan(&snapshot(repo, Side::Staged), 0).unwrap()).unwrap();
        assert_eq!(bytes(repo, ":file"), lines().as_bytes());
        assert_eq!(fs::read(repo.join("file")).unwrap(), working.as_bytes());
    }
}

#[test]
fn lf_rules_refuse_crlf_content_or_crlf_checkout_conversion() {
    for (rule, autocrlf, crlf_content) in [
        ("text eol=lf", "false", true),
        ("", "input", true),
        ("", "true", false),
        ("text", "false", false),
    ] {
        let dir = seeded();
        let repo = dir.path();
        put(repo, ".gitattributes", &format!("file {rule}\n"));
        g(repo, None, &["config", "core.autocrlf", autocrlf]).unwrap();
        g(repo, None, &["config", "core.eol", "crlf"]).unwrap();
        let working = if crlf_content {
            "changed\r\n"
        } else {
            "changed\n"
        };
        put(repo, "file", working);
        let before = index(repo);
        assert!(
            read(7, repo, Path::new("file"), Side::Unstaged).is_err(),
            "{rule}, {autocrlf}"
        );
        assert_eq!(index(repo), before);
        assert_eq!(fs::read(repo.join("file")).unwrap(), working.as_bytes());
    }
}

#[cfg(unix)]
#[test]
fn symlink_parents_leaf_symlinks_and_mode_changes_are_refused() {
    use std::os::unix::fs::{symlink, PermissionsExt};
    let dir = seeded();
    let repo = dir.path();
    symlink("file", repo.join("link")).unwrap();
    commit_all(repo);
    assert!(read(7, repo, Path::new("link"), Side::Staged).is_err());
    put(repo, "file", "changed\n");
    let selected = plan(&snapshot(repo, Side::Unstaged), 0).unwrap();
    let before = index(repo);
    fs::set_permissions(repo.join("file"), fs::Permissions::from_mode(0o755)).unwrap();
    assert!(execute(&selected).is_err());
    assert!(read(7, repo, Path::new("file"), Side::Unstaged).is_err());
    assert_eq!(index(repo), before);
    fs::remove_file(repo.join("file")).unwrap();
    symlink("sibling", repo.join("file")).unwrap();
    assert!(read(7, repo, Path::new("file"), Side::Unstaged).is_err());
    fs::create_dir(repo.join("folder")).unwrap();
    put(repo, "folder/child", "base\n");
    commit_all(repo);
    put(repo, "folder/child", "reviewed\n");
    let selected = plan(
        &read(7, repo, Path::new("folder/child"), Side::Unstaged).unwrap(),
        0,
    )
    .unwrap();
    fs::rename(repo.join("folder"), repo.join("old-folder")).unwrap();
    symlink("old-folder", repo.join("folder")).unwrap();
    assert!(execute(&selected).is_err());
    assert_eq!(
        fs::read(repo.join("old-folder/child")).unwrap(),
        b"reviewed\n"
    );
}

#[test]
fn split_indexes_and_linked_worktrees_keep_their_native_index_identity() {
    let dir = seeded();
    let repo = dir.path();
    g(repo, None, &["config", "core.splitIndex", "true"]).unwrap();
    g(repo, None, &["update-index", "--split-index"]).unwrap();
    put(repo, "file", "split index change\n");
    execute(&plan(&snapshot(repo, Side::Unstaged), 0).unwrap()).unwrap();
    assert_eq!(bytes(repo, ":file"), b"split index change\n");
    let linked = repo.join("linked");
    checked(
        run(
            repo,
            None,
            &[
                "worktree".as_ref(),
                "add".as_ref(),
                "-qb".as_ref(),
                "linked".as_ref(),
                linked.as_os_str(),
                "HEAD".as_ref(),
            ],
            None,
        )
        .unwrap(),
    )
    .unwrap();
    put(&linked, "file", "linked worktree change\n");
    execute(&plan(&snapshot(&linked, Side::Unstaged), 0).unwrap()).unwrap();
    assert_eq!(bytes(&linked, ":file"), b"linked worktree change\n");
    assert_eq!(bytes(repo, ":file"), b"split index change\n");
}
