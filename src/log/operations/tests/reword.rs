use super::*;

fn merged_history() -> (TempDir, Context, [String; 5]) {
    let (dir, _, [base, middle, main]) = history();
    let repo = dir.path();
    git(repo, &["switch", "-c", "side", &base]);
    commit(repo, "side", "Side commit");
    let side = git(repo, &["rev-parse", "HEAD"]);
    git(repo, &["switch", "main"]);
    git(repo, &["merge", "--no-ff", "-qm", "Merge side", "side"]);
    let merge = git(repo, &["rev-parse", "HEAD"]);
    let c = Context::discover(repo).unwrap();
    (dir, c, [middle, main, side, merge, base])
}

#[test]
fn editing_through_merges_preserves_trees_parent_order_and_dirty_state() {
    for selected in 0..5 {
        let (dir, c, ids) = merged_history();
        let repo = dir.path();
        commit(repo, "after-merge", "Later descendant");
        let head = git(repo, &["rev-parse", "HEAD"]);
        git(repo, &["branch", "keep-original", &head]);
        git(repo, &["tag", "before-edit", &ids[selected]]);
        dirty_index(repo);
        fs::write(repo.join("middle"), "Unrelated staged sibling").unwrap();
        git(repo, &["add", "middle"]);
        let index = fs::read(repo.join(".git/index")).unwrap();
        let files = git(repo, &["diff", "--binary"]);
        let value = "修改历史消息\n\nKeep the exact merge result.";
        execute(&c, &ids[selected], Kind::Edit, value);
        assert_eq!(git(repo, &["rev-parse", "keep-original"]), head);
        assert_eq!(git(repo, &["rev-parse", "before-edit"]), ids[selected]);
        assert_eq!(git(repo, &["rev-parse", "side"]), ids[2]);
        assert_eq!(
            git(repo, &["rev-parse", "HEAD^{tree}"]),
            git(repo, &["rev-parse", "keep-original^{tree}"])
        );
        assert_eq!(git(repo, &["rev-list", "--count", "HEAD"]), "6");
        let original = git(
            repo,
            &["rev-list", "--topo-order", "--reverse", "keep-original"],
        );
        let rewritten = git(repo, &["rev-list", "--topo-order", "--reverse", "HEAD"]);
        let mapping = original
            .lines()
            .zip(rewritten.lines())
            .collect::<std::collections::HashMap<_, _>>();
        for (old, new) in &mapping {
            assert_eq!(
                git(
                    repo,
                    &[
                        "show",
                        "-s",
                        "--format=%T%n%an%n%ae%n%aI%n%cn%n%ce%n%cI",
                        old
                    ]
                ),
                git(
                    repo,
                    &[
                        "show",
                        "-s",
                        "--format=%T%n%an%n%ae%n%aI%n%cn%n%ce%n%cI",
                        new
                    ]
                )
            );
            let parents = git(repo, &["show", "-s", "--format=%P", old]);
            let expected = parents
                .split_whitespace()
                .map(|parent| *mapping.get(parent).unwrap())
                .collect::<Vec<_>>()
                .join(" ");
            assert_eq!(git(repo, &["show", "-s", "--format=%P", new]), expected);
            assert_eq!(
                message(repo, new).unwrap(),
                if *old == ids[selected] {
                    value.to_owned()
                } else {
                    message(repo, old).unwrap()
                }
            );
        }
        assert_eq!(fs::read(repo.join(".git/index")).unwrap(), index);
        assert_eq!(git(repo, &["diff", "--binary"]), files);
        assert_eq!(git(repo, &["show", ":middle"]), "Unrelated staged sibling");
        assert_eq!(git(repo, &["rev-parse", "ORIG_HEAD"]), head);
        assert!(!repo.join(".git/rebase-merge").exists());
    }
}

#[test]
fn edit_refuses_commits_outside_the_checked_out_branch() {
    let (dir, c, [base, _, head]) = history();
    let repo = dir.path();
    git(repo, &["switch", "-c", "other", &base]);
    commit(repo, "other", "Other branch");
    let other = git(repo, &["rev-parse", "HEAD"]);
    git(repo, &["switch", "main"]);
    assert!(
        error(prepare(&c, &other, Kind::Edit, "Changed")).contains("outside the current branch")
    );
    assert_eq!(git(repo, &["rev-parse", "HEAD"]), head);
}

#[test]
fn failed_ref_update_keeps_history_and_exact_index_unchanged() {
    let (dir, c, [_, middle, head]) = history();
    let repo = dir.path();
    let (review, _) = prepare(&c, &middle, Kind::Edit, "New message").unwrap();
    let index = fs::read(repo.join(".git/index")).unwrap();
    fs::write(repo.join(".git/refs/heads/main.lock"), "External lock").unwrap();
    assert!(review.execute(&c).is_err());
    assert_eq!(git(repo, &["rev-parse", "HEAD"]), head);
    assert_eq!(fs::read(repo.join(".git/index")).unwrap(), index);
    assert!(git(repo, &["diff"]).is_empty());
}

#[test]
fn rewritten_headers_remove_invalid_signatures_and_preserve_other_bytes() {
    let raw = b"tree tree-id\nparent old-parent\nauthor A <a@example.invalid> 1 +0000\ncommitter C <c@example.invalid> 2 +0000\nencoding ISO-8859-1\ngpgsig -----BEGIN SIGNATURE-----\n signature\n -----END SIGNATURE-----\ngpgsig-sha256 other-signature\n continued\nmergetag object old-parent\n type commit\n tag release\n \n tag message\ncustom-header keep\n continuation\n\ncaf\xe9\n\n";
    let mapping = [("old-parent".to_owned(), "new-parent".to_owned())].into();
    let result = super::super::reword::replace(raw, &mapping, Some("新消息")).unwrap();
    let result = String::from_utf8(result).unwrap();
    assert!(result.contains("parent new-parent\n"));
    assert!(result.contains("custom-header keep\n continuation\n\n新消息\n"));
    assert!(!result.contains("gpgsig") && !result.contains("mergetag"));
    assert!(!result.contains("encoding"));
    let descendant = super::super::reword::replace(raw, &mapping, None).unwrap();
    assert!(descendant.ends_with(b"\n\ncaf\xe9\n\n"));
    assert!(descendant
        .windows(b"encoding ISO-8859-1".len())
        .any(|bytes| bytes == b"encoding ISO-8859-1"));
}
