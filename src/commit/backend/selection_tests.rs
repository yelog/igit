use super::{
    tests::{commit_all, get, put, setup},
    *,
};
use crate::commit::process::Control;
use std::time::Duration;

fn clean_filter(repo: &Path) {
    g(
        repo,
        None,
        &[
            "config",
            "filter.fixture.clean",
            "tr '[:lower:]' '[:upper:]'",
        ],
    )
    .unwrap();
    g(repo, None, &["config", "filter.fixture.required", "true"]).unwrap();
}

fn selected(repo: &Path, paths: &[&str]) -> Vec<Change> {
    let changes = status(repo).unwrap();
    paths
        .iter()
        .map(|path| {
            changes
                .iter()
                .find(|change| change.path == Path::new(path))
                .unwrap_or_else(|| panic!("Missing selected path {path}"))
                .clone()
        })
        .collect()
}

fn unchanged(repo: &Path) -> (Option<String>, Vec<u8>, Vec<u8>) {
    (
        oid(repo).unwrap(),
        fs::read(repo.join(".git/index")).unwrap(),
        g(repo, None, &["ls-files", "--stage", "-z"]).unwrap(),
    )
}

fn assert_ai_matches_plan(repo: &Path, files: &[Change], plan: &Plan) -> ai::Snapshot {
    let control = Control::new(Duration::from_secs(60));
    let snapshot = ai::Snapshot::capture(repo, files, &control).unwrap();
    let patch = snapshot
        .files
        .iter()
        .map(|file| file.patch.as_str())
        .collect::<String>();
    let directory = TempDir::new().unwrap();
    let index = directory.path().join("evidence");
    let base = base_tree(repo, plan.head.as_deref()).unwrap();
    g(repo, Some(&index), &["read-tree", &base]).unwrap();
    checked(
        run(
            repo,
            Some(&index),
            &[
                "apply".as_ref(),
                "--cached".as_ref(),
                "--whitespace=nowarn".as_ref(),
            ],
            Some(patch.as_bytes()),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(
        txt(g(repo, Some(&index), &["write-tree"]).unwrap()),
        plan.tree,
        "The complete AI patch must produce the exact reviewed commit tree"
    );
    snapshot.validate(&control).unwrap();
    snapshot
}

#[test]
fn selected_working_attribute_deletion_matches_ai_and_commit_in_both_orders() {
    let directory = setup();
    let repo = directory.path();
    clean_filter(repo);
    put(repo, ".gitattributes", "file filter=fixture\n");
    put(repo, "file", "base\n");
    commit_all(repo);
    fs::remove_file(repo.join(".gitattributes")).unwrap();
    put(repo, "file", "reviewed\n");
    let before = unchanged(repo);
    let mut plans = Vec::new();
    for paths in [["file", ".gitattributes"], [".gitattributes", "file"]] {
        let files = selected(repo, &paths);
        assert!(files.iter().all(|file| !file.staged()));
        let plan = plan(0, repo, files.clone(), "remove conversion".into(), false).unwrap();
        assert_eq!(
            get(repo, &["show", &format!("{}:file", plan.tree)]),
            "reviewed"
        );
        assert!(
            g(repo, None, &["ls-tree", &plan.tree, "--", ".gitattributes"])
                .unwrap()
                .is_empty()
        );
        let snapshot = assert_ai_matches_plan(repo, &files, &plan);
        let file = snapshot
            .files
            .iter()
            .find(|file| file.path == Path::new("file"))
            .unwrap();
        assert!(file.patch.contains("-BASE\n"), "{}", file.patch);
        assert!(file.patch.contains("+reviewed\n"), "{}", file.patch);
        assert!(!file.patch.contains("+REVIEWED\n"), "{}", file.patch);
        assert_eq!(unchanged(repo), before);
        assert!(!repo.join(".gitattributes").exists());
        assert_eq!(fs::read_to_string(repo.join("file")).unwrap(), "reviewed\n");
        plans.push(plan);
    }
    assert_eq!(plans[0].tree, plans[1].tree);
    execute(&plans[0]).unwrap();
    assert_eq!(get(repo, &["rev-parse", "HEAD^{tree}"]), plans[0].tree);
    assert_eq!(get(repo, &["show", "HEAD:file"]), "reviewed");
    assert!(!repo.join(".gitattributes").exists());
    assert!(status(repo).unwrap().is_empty());
}

#[test]
fn selected_staged_attributes_precede_working_files_in_both_orders() {
    let directory = setup();
    let repo = directory.path();
    clean_filter(repo);
    put(repo, ".gitattributes", "file filter=fixture\n");
    put(repo, "file", "base\n");
    commit_all(repo);
    put(repo, ".gitattributes", "file -filter\n");
    g(repo, None, &["add", ".gitattributes"]).unwrap();
    // Missing working attributes make Git fall back to the private index.
    // The reviewed staged version must win over the attributes in HEAD.
    fs::remove_file(repo.join(".gitattributes")).unwrap();
    put(repo, "file", "reviewed\n");
    let before = unchanged(repo);
    let mut plans = Vec::new();
    for paths in [["file", ".gitattributes"], [".gitattributes", "file"]] {
        let files = selected(repo, &paths);
        assert_eq!(
            files
                .iter()
                .find(|file| file.path == Path::new(".gitattributes"))
                .unwrap()
                .xy,
            *b"MD"
        );
        let plan = plan(0, repo, files.clone(), "change conversion".into(), false).unwrap();
        assert_eq!(
            get(repo, &["show", &format!("{}:file", plan.tree)]),
            "reviewed"
        );
        assert_eq!(
            get(repo, &["show", &format!("{}:.gitattributes", plan.tree)]),
            "file -filter"
        );
        let snapshot = assert_ai_matches_plan(repo, &files, &plan);
        let attributes = snapshot
            .files
            .iter()
            .find(|file| file.path == Path::new(".gitattributes"))
            .unwrap();
        assert_eq!(attributes.status, "M");
        assert!(attributes.patch.contains("+file -filter\n"));
        assert_eq!(unchanged(repo), before);
        assert!(!repo.join(".gitattributes").exists());
        assert_eq!(fs::read_to_string(repo.join("file")).unwrap(), "reviewed\n");
        plans.push(plan);
    }
    assert_eq!(plans[0].tree, plans[1].tree);
    execute(&plans[0]).unwrap();
    assert_eq!(get(repo, &["rev-parse", "HEAD^{tree}"]), plans[0].tree);
    assert_eq!(get(repo, &["show", "HEAD:file"]), "reviewed");
    assert_eq!(get(repo, &["show", "HEAD:.gitattributes"]), "file -filter");
    assert!(!repo.join(".gitattributes").exists());
    let remaining = status(repo).unwrap();
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].path, Path::new(".gitattributes"));
    assert_eq!(remaining[0].xy, *b" D");
}

#[test]
fn attribute_preparation_preserves_mixed_staged_versions_and_unchecked_staging() {
    let directory = setup();
    let repo = directory.path();
    clean_filter(repo);
    put(repo, ".gitattributes", "file filter=fixture\n");
    for path in ["file", "mixed", "unchecked"] {
        put(repo, path, "base\n");
    }
    commit_all(repo);
    put(repo, "mixed", "reviewed staged\n");
    put(repo, "unchecked", "PRIVATE_STAGED\n");
    g(repo, None, &["add", "mixed", "unchecked"]).unwrap();
    put(repo, "mixed", "later working\n");
    put(repo, "unchecked", "PRIVATE_WORKING\n");
    fs::remove_file(repo.join(".gitattributes")).unwrap();
    put(repo, "file", "reviewed\n");
    let before = unchanged(repo);
    let mut plans = Vec::new();
    for paths in [
        ["file", "mixed", ".gitattributes"],
        [".gitattributes", "mixed", "file"],
    ] {
        let files = selected(repo, &paths);
        assert_eq!(
            files
                .iter()
                .find(|file| file.path == Path::new("mixed"))
                .unwrap()
                .xy,
            *b"MM"
        );
        let plan = plan(0, repo, files.clone(), "reviewed selection".into(), false).unwrap();
        assert_eq!(
            get(repo, &["show", &format!("{}:file", plan.tree)]),
            "reviewed"
        );
        assert_eq!(
            get(repo, &["show", &format!("{}:mixed", plan.tree)]),
            "reviewed staged"
        );
        assert_eq!(
            get(repo, &["show", &format!("{}:unchecked", plan.tree)]),
            "base"
        );
        let snapshot = assert_ai_matches_plan(repo, &files, &plan);
        assert!(snapshot.files.iter().all(|file| {
            file.path != Path::new("unchecked")
                && !file.patch.contains("PRIVATE_")
                && !file.patch.contains("later working")
        }));
        assert_eq!(unchanged(repo), before);
        assert_eq!(
            fs::read_to_string(repo.join("mixed")).unwrap(),
            "later working\n"
        );
        assert_eq!(
            fs::read_to_string(repo.join("unchecked")).unwrap(),
            "PRIVATE_WORKING\n"
        );
        plans.push(plan);
    }
    assert_eq!(plans[0].tree, plans[1].tree);
    execute(&plans[0]).unwrap();
    assert_eq!(get(repo, &["rev-parse", "HEAD^{tree}"]), plans[0].tree);
    assert_eq!(get(repo, &["show", "HEAD:file"]), "reviewed");
    assert_eq!(get(repo, &["show", "HEAD:mixed"]), "reviewed staged");
    assert_eq!(get(repo, &["show", "HEAD:unchecked"]), "base");
    assert_eq!(get(repo, &["show", ":unchecked"]), "PRIVATE_STAGED");
    assert_eq!(get(repo, &["diff", "--cached", "--name-only"]), "unchecked");
    assert_eq!(get(repo, &["diff", "--name-only"]), "mixed\nunchecked");
    assert_eq!(
        fs::read_to_string(repo.join("mixed")).unwrap(),
        "later working\n"
    );
    assert_eq!(
        fs::read_to_string(repo.join("unchecked")).unwrap(),
        "PRIVATE_WORKING\n"
    );
}

#[test]
fn recreated_working_deletion_is_rejected_without_changing_sources() {
    let directory = setup();
    let repo = directory.path();
    put(repo, "file", "base\n");
    commit_all(repo);
    fs::remove_file(repo.join("file")).unwrap();
    let files = selected(repo, &["file"]);
    assert_eq!(files[0].xy, *b" D");
    let plan = plan(0, repo, files, "delete reviewed file".into(), false).unwrap();
    put(repo, "file", "recreated after review\n");
    let before = unchanged(repo);
    let error = execute(&plan).err().expect("Reject a stale deletion");
    assert!(error.to_string().contains("changed"), "{error:#}");
    assert_eq!(unchanged(repo), before);
    assert_eq!(get(repo, &["show", "HEAD:file"]), "base");
    assert_eq!(
        fs::read_to_string(repo.join("file")).unwrap(),
        "recreated after review\n"
    );
    assert!(!repo.join(".git/index.lock").exists());
}
