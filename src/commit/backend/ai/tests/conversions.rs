use super::*;

fn clean_filter(repo: &Path, script: &str) {
    g(repo, None, &["config", "filter.fixture.clean", script]).unwrap();
    g(repo, None, &["config", "filter.fixture.required", "true"]).unwrap();
}

#[test]
fn staged_capture_and_validation_never_run_a_clean_filter_or_textconv() {
    let dir = setup();
    let repo = dir.path();
    put(repo, ".gitattributes", "file filter=fixture diff=fixture\n");
    clean_filter(repo, "cat");
    put(repo, "file", "base\n");
    commit_all(repo);
    put(repo, "file", "REVIEWED_STAGED\n");
    g(repo, None, &["add", "file"]).unwrap();
    let files = selected(repo, &[Path::new("file")]);
    clean_filter(repo, "touch FILTER_RAN; exit 1");
    g(
        repo,
        None,
        &["config", "diff.fixture.textconv", "touch TEXTCONV_RAN"],
    )
    .unwrap();
    let snapshot = Snapshot::capture(repo, &files, &control()).unwrap();
    assert!(snapshot.files[0].patch.contains("+REVIEWED_STAGED"));
    assert!(!repo.join("FILTER_RAN").exists());
    assert!(!repo.join("TEXTCONV_RAN").exists());
    put(repo, "file", "PRIVATE_LATER_WORKING\n");
    clean_filter(repo, "touch ANOTHER_FILTER_RAN; exit 1");
    snapshot.validate(&control()).unwrap();
    assert!(!repo.join("ANOTHER_FILTER_RAN").exists());
    assert!(!repo.join("FILTER_RAN").exists());
    assert!(!repo.join("TEXTCONV_RAN").exists());
}

#[test]
fn failed_clean_filters_do_not_expose_diagnostics_or_selected_contents() {
    let dir = setup();
    let repo = dir.path();
    put(repo, ".gitattributes", "file filter=fixture\n");
    clean_filter(repo, "cat");
    put(repo, "file", "base\n");
    commit_all(repo);
    put(repo, "file", "PRIVATE_SELECTED_CONTENT\n");
    let files = selected(repo, &[Path::new("file")]);
    let before = unchanged(repo);
    clean_filter(
        repo,
        "touch FILTER_RAN; printf 'PRIVATE_STDOUT\\n'; printf 'PRIVATE_CREDENTIAL\\n' >&2; cat >&2; exit 1",
    );
    let error = Snapshot::capture(repo, &files, &control()).unwrap_err();
    assert!(repo.join("FILTER_RAN").exists());
    for message in [
        error.to_string(),
        format!("{error:#}"),
        format!("{error:?}"),
    ] {
        assert!(!message.contains("PRIVATE_"), "Git failure leaked output");
        assert!(message.contains("Git"), "{message}");
    }
    assert_eq!(unchanged(repo), before);
    assert_eq!(
        fs::read_to_string(repo.join("file")).unwrap(),
        "PRIVATE_SELECTED_CONTENT\n"
    );
}

#[test]
fn working_capture_uses_native_filters_but_final_validation_uses_raw_fingerprints() {
    let dir = setup();
    let repo = dir.path();
    put(repo, ".gitattributes", "file filter=fixture\n");
    clean_filter(repo, "tr '[:lower:]' '[:upper:]'");
    put(repo, "file", "base\n");
    commit_all(repo);
    put(repo, "file", "reviewed\n");
    let snapshot = capture(repo, &["file"]);
    assert!(snapshot.files[0].patch.contains("+REVIEWED"));
    assert!(!snapshot.files[0].patch.contains("+reviewed"));
    snapshot.validate(&control()).unwrap();
    // Both raw values clean to the same blob. The reviewed source still changed.
    put(repo, "file", "REVIEWED\n");
    assert!(snapshot.validate(&control()).is_err());
}

#[test]
fn selected_working_attribute_deletion_uses_a_consistent_private_index_fallback() {
    let dir = setup();
    let repo = dir.path();
    put(repo, ".gitattributes", "file filter=fixture\n");
    clean_filter(repo, "tr '[:lower:]' '[:upper:]'");
    put(repo, "file", "base\n");
    commit_all(repo);
    fs::remove_file(repo.join(".gitattributes")).unwrap();
    put(repo, "file", "reviewed\n");
    let before = unchanged(repo);
    for paths in [["file", ".gitattributes"], [".gitattributes", "file"]] {
        let snapshot = capture(repo, &paths);
        let file = snapshot
            .files
            .iter()
            .find(|file| file.path == Path::new("file"))
            .unwrap();
        assert!(file.patch.contains("-BASE\n"));
        assert!(file.patch.contains("+reviewed\n"));
        assert!(!file.patch.contains("+REVIEWED\n"));
        snapshot.validate(&control()).unwrap();
        assert_eq!(unchanged(repo), before);
        assert!(!repo.join(".gitattributes").exists());
    }
}

#[test]
fn final_validation_rejects_attribute_and_conversion_changes() {
    for mutation in ["attributes", "conversion"] {
        let dir = setup();
        let repo = dir.path();
        put(repo, "file", "base\n");
        commit_all(repo);
        put(repo, "file", "reviewed\n");
        let snapshot = capture(repo, &["file"]);
        if mutation == "attributes" {
            put(repo, ".gitattributes", "file text eol=lf\n");
        } else {
            g(repo, None, &["config", "core.autocrlf", "input"]).unwrap();
        }
        assert!(snapshot.validate(&control()).is_err(), "{mutation}");
    }
}

#[test]
fn source_or_attributes_changed_by_a_clean_filter_fail_capture() {
    for mutation in [
        "printf changed\\\\n > file; cat",
        "printf 'file -text\\n' > .gitattributes; cat",
        "git config core.autocrlf input; cat",
        "unset GIT_INDEX_FILE; git update-index --force-remove -- file; cat",
    ] {
        let dir = setup();
        let repo = dir.path();
        put(repo, ".gitattributes", "file filter=fixture\n");
        clean_filter(repo, "cat");
        put(repo, "file", "base\n");
        commit_all(repo);
        put(repo, "file", "reviewed\n");
        // Obtain the review before installing a filter with a deliberate side
        // effect. Only the controlled private-index add should execute it.
        let files = selected(repo, &[Path::new("file")]);
        clean_filter(repo, mutation);
        let error = Snapshot::capture(repo, &files, &control()).unwrap_err();
        assert!(
            error.to_string().contains("changed") || error.to_string().contains("no changes"),
            "{mutation}: {error:#}"
        );
    }
}
