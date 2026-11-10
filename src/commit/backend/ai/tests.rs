use super::*;
use crate::commit::backend::{g, oid, status, tests::*};
use std::time::Duration;

#[cfg(unix)]
mod conversions;
mod scope;

fn control() -> Control {
    Control::new(Duration::from_secs(60))
}

fn selected(repo: &Path, paths: &[&Path]) -> Vec<Change> {
    let changes = status(repo).unwrap();
    paths
        .iter()
        .map(|path| {
            changes
                .iter()
                .find(|change| change.path == *path)
                .unwrap_or_else(|| panic!("Missing selected path {}", path.display()))
                .clone()
        })
        .collect()
}

fn capture(repo: &Path, paths: &[&str]) -> Snapshot {
    let paths: Vec<_> = paths.iter().map(Path::new).collect();
    Snapshot::capture(repo, &selected(repo, &paths), &control()).unwrap()
}

fn unchanged(repo: &Path) -> (Option<String>, Option<Vec<u8>>, Vec<u8>) {
    (
        oid(repo).unwrap(),
        fs::read(repo.join(".git/index")).ok(),
        g(repo, None, &["ls-files", "--stage", "-z"]).unwrap(),
    )
}

#[test]
fn complete_patches_larger_than_160k_have_stats_and_leave_sources_unchanged() {
    let dir = setup();
    let repo = dir.path();
    put(
        repo,
        "selected",
        &format!("{}LAST_SELECTED_LINE\n", "a long line\n".repeat(20_000)),
    );
    put(repo, "unchecked", "PRIVATE_UNCHECKED_MARKER\n");
    let before = unchanged(repo);
    let snapshot = capture(repo, &["selected"]);
    assert_eq!(snapshot.files.len(), 1);
    let file = &snapshot.files[0];
    assert!(file.patch.len() > 160_000);
    assert!(file.patch.ends_with("+LAST_SELECTED_LINE\n"));
    assert!(!file.patch.contains("PRIVATE_UNCHECKED_MARKER"));
    assert_eq!(file.status, "A");
    assert_eq!((file.additions, file.deletions), (Some(20_001), Some(0)));
    snapshot.validate(&control()).unwrap();
    assert_eq!(unchanged(repo), before);
    assert!(fs::read_to_string(repo.join("selected"))
        .unwrap()
        .ends_with("LAST_SELECTED_LINE\n"));
}

#[test]
fn aggregate_patch_safety_cap_returns_an_error_without_truncating() {
    let dir = setup();
    let repo = dir.path();
    let bytes = vec![b'x'; 17 * 1024 * 1024];
    fs::write(repo.join("one"), &bytes).unwrap();
    fs::write(repo.join("two"), &bytes).unwrap();
    let before = unchanged(repo);
    let error = Snapshot::capture(
        repo,
        &selected(repo, &[Path::new("one"), Path::new("two")]),
        &control(),
    )
    .unwrap_err();
    assert!(error.to_string().contains("32 MiB"), "{error:#}");
    assert_eq!(unchanged(repo), before);
    assert_eq!(
        fs::metadata(repo.join("one")).unwrap().len(),
        bytes.len() as u64
    );
    assert_eq!(
        fs::metadata(repo.join("two")).unwrap().len(),
        bytes.len() as u64
    );
}

#[test]
fn context_and_inter_hunk_configuration_cannot_expand_ai_diffs() {
    let dir = setup();
    let repo = dir.path();
    let original = (0..100)
        .map(|line| format!("line {line:03}\n"))
        .collect::<String>();
    put(repo, "file", &original);
    commit_all(repo);
    put(
        repo,
        "file",
        &original
            .replace("line 010", "first changed")
            .replace("line 080", "second changed"),
    );
    let baseline = capture(repo, &["file"]).files.remove(0);
    g(repo, None, &["config", "diff.context", "10000"]).unwrap();
    g(repo, None, &["config", "diff.interHunkContext", "10000"]).unwrap();
    g(repo, None, &["config", "diff.noprefix", "true"]).unwrap();
    g(repo, None, &["config", "diff.mnemonicPrefix", "true"]).unwrap();
    let configured = capture(repo, &["file"]).files.remove(0);
    assert_eq!(baseline, configured);
    assert_eq!(configured.patch.matches("\n@@ ").count(), 2);
    assert!(!configured.patch.contains("line 050"));
    assert_eq!(
        (configured.additions, configured.deletions),
        (Some(2), Some(2))
    );
}

#[test]
fn frozen_chunk_reads_never_rebuild_from_later_working_content() {
    let dir = setup();
    let repo = dir.path();
    put(repo, "file", "base\n");
    commit_all(repo);
    put(repo, "file", "reviewed\n");
    let snapshot = capture(repo, &["file"]);
    let patch = snapshot.files[0].patch.clone();
    let tree = snapshot.frozen.tree.clone();
    assert!(snapshot.frozen.index.exists());
    put(repo, "file", "changed after capture\n");
    assert_eq!(
        snapshot
            .frozen
            .patch(Path::new("file"), PATCH_LIMIT, &control())
            .unwrap(),
        patch
    );
    assert_eq!(snapshot.frozen.tree, tree);
    assert!(snapshot.validate(&control()).is_err());
    assert_eq!(snapshot.files[0].patch, patch);
    assert_eq!(capture(repo, &["file"]).files[0].additions, Some(1));
}

#[test]
fn mixed_sources_ignore_unrelated_staging_and_working_edits_to_staged_sources() {
    let dir = setup();
    let repo = dir.path();
    for path in ["mixed", "working", "unchecked"] {
        put(repo, path, "base\n");
    }
    commit_all(repo);
    put(repo, "mixed", "REVIEWED_STAGED\n");
    put(repo, "unchecked", "PRIVATE_STAGED\n");
    g(repo, None, &["add", "mixed", "unchecked"]).unwrap();
    put(repo, "mixed", "PRIVATE_LATER_WORKING\n");
    put(repo, "working", "REVIEWED_WORKING\n");
    let before = unchanged(repo);
    let snapshot = capture(repo, &["mixed", "working"]);
    let patch = snapshot
        .files
        .iter()
        .map(|file| file.patch.as_str())
        .collect::<String>();
    assert!(patch.contains("REVIEWED_STAGED") && patch.contains("REVIEWED_WORKING"));
    assert!(!patch.contains("PRIVATE_"));
    assert_eq!(unchanged(repo), before);
    put(repo, "mixed", "another unstaged edit\n");
    put(repo, "unchecked", "another unrelated staged edit\n");
    g(repo, None, &["add", "unchecked"]).unwrap();
    snapshot.validate(&control()).unwrap();
    let second =
        Snapshot::capture(repo, &selected(repo, &[Path::new("mixed")]), &control()).unwrap();
    fs::remove_file(repo.join("mixed")).unwrap();
    fs::create_dir(repo.join("mixed")).unwrap();
    put(repo, "mixed/private", "unchecked replacement child\n");
    second.validate(&control()).unwrap();
    g(repo, None, &["rm", "--cached", "mixed"]).unwrap();
    assert!(second.validate(&control()).is_err());
    assert!(snapshot.validate(&control()).is_err());
}

#[test]
fn final_validation_rejects_working_index_head_and_branch_changes() {
    for mutation in ["working", "index", "head", "branch"] {
        let dir = setup();
        let repo = dir.path();
        put(repo, "file", "base\n");
        commit_all(repo);
        put(repo, "file", "reviewed\n");
        let snapshot = capture(repo, &["file"]);
        match mutation {
            "working" => put(repo, "file", "stale\n"),
            "index" => {
                g(repo, None, &["add", "file"]).unwrap();
            }
            "head" => {
                g(repo, None, &["commit", "--allow-empty", "-qm", "advance"]).unwrap();
            }
            "branch" => {
                g(repo, None, &["checkout", "-qb", "another"]).unwrap();
            }
            _ => unreachable!(),
        }
        assert!(snapshot.validate(&control()).is_err(), "{mutation}");
    }
}

#[test]
#[cfg(unix)]
fn unicode_metacharacters_tabs_and_newlines_keep_native_path_identity() {
    let dir = setup();
    let repo = dir.path();
    let paths = [
        PathBuf::from("src/[literal]*?/ｕｉ/文件.txt"),
        PathBuf::from("src/[literal]*?/line\twith\nnewline"),
        PathBuf::from("src/:(exclude)literal"),
        PathBuf::from("src/-option"),
        #[cfg(unix)]
        PathBuf::from(r"src/back\slash"),
        #[cfg(all(unix, not(target_os = "macos")))]
        PathBuf::from(raw(b"src/native\xffpath")),
    ];
    for path in &paths {
        fs::create_dir_all(repo.join(path).parent().unwrap()).unwrap();
        fs::write(repo.join(path), "base\n").unwrap();
    }
    commit_all(repo);
    for path in &paths {
        fs::write(repo.join(path), "STAGED_LITERAL_CONTENT\n").unwrap();
    }
    g(repo, None, &["add", "-A"]).unwrap();
    for path in &paths {
        fs::write(repo.join(path), "PRIVATE_LATER_WORKING\n").unwrap();
    }
    for path in &paths {
        let snapshot = Snapshot::capture(repo, &selected(repo, &[path]), &control()).unwrap();
        assert_eq!(snapshot.files[0].path, *path);
        assert_eq!(snapshot.files[0].status, "M");
        assert!(snapshot.files[0].patch.contains("+STAGED_LITERAL_CONTENT"));
        assert!(!snapshot.files[0].patch.contains("PRIVATE_LATER_WORKING"));
        snapshot.validate(&control()).unwrap();
    }
    g(repo, None, &["reset", "-q"]).unwrap();
    let refs: Vec<_> = paths.iter().map(PathBuf::as_path).collect();
    let snapshot = Snapshot::capture(repo, &selected(repo, &refs), &control()).unwrap();
    assert_eq!(
        snapshot
            .files
            .iter()
            .map(|file| &file.path)
            .collect::<Vec<_>>(),
        paths.iter().collect::<Vec<_>>()
    );
    assert!(snapshot
        .files
        .iter()
        .all(|file| file.patch.contains("+PRIVATE_LATER_WORKING")));
}

#[test]
fn unborn_binary_source_is_a_marker_with_binary_statistics() {
    let dir = setup();
    let repo = dir.path();
    fs::write(repo.join("binary"), b"\0PRIVATE_BINARY_BYTES\xff").unwrap();
    let before = unchanged(repo);
    let snapshot = capture(repo, &["binary"]);
    let file = &snapshot.files[0];
    assert_eq!(file.status, "A");
    assert_eq!((file.additions, file.deletions), (None, None));
    assert!(file.patch.contains("Binary files"));
    assert!(!file.patch.contains("PRIVATE_BINARY_BYTES"));
    assert!(!file.patch.contains("GIT binary patch"));
    snapshot.validate(&control()).unwrap();
    assert_eq!(unchanged(repo), before);
    g(repo, None, &["add", "binary"]).unwrap();
    let staged = capture(repo, &["binary"]);
    fs::write(repo.join("binary"), b"different working binary\0").unwrap();
    staged.validate(&control()).unwrap();
    g(
        repo,
        None,
        &["symbolic-ref", "HEAD", "refs/heads/another-unborn"],
    )
    .unwrap();
    assert!(staged.validate(&control()).is_err());
}

#[cfg(unix)]
#[test]
fn symlinks_and_executable_modes_keep_actual_git_kinds() {
    use std::os::unix::{fs::symlink, fs::PermissionsExt};
    let dir = setup();
    let repo = dir.path();
    g(repo, None, &["config", "core.filemode", "true"]).unwrap();
    put(repo, "file", "base\n");
    put(repo, "mode", "same contents\n");
    commit_all(repo);
    fs::remove_file(repo.join("file")).unwrap();
    symlink("target\nname", repo.join("file")).unwrap();
    fs::set_permissions(repo.join("mode"), fs::Permissions::from_mode(0o755)).unwrap();
    let snapshot = capture(repo, &["file", "mode"]);
    assert_eq!(snapshot.files[0].status, "T");
    assert!(snapshot.files[0].patch.contains("new file mode 120000"));
    assert!(snapshot.files[0].patch.contains("+target"));
    assert_eq!(snapshot.files[1].status, "M");
    assert_eq!(
        (snapshot.files[1].additions, snapshot.files[1].deletions),
        (Some(0), Some(0))
    );
    assert!(snapshot.files[1].patch.contains("new mode 100755"));
    snapshot.validate(&control()).unwrap();
    fs::remove_file(repo.join("file")).unwrap();
    symlink("another target", repo.join("file")).unwrap();
    assert!(snapshot.validate(&control()).is_err());
}

#[test]
fn cancelled_or_expired_control_applies_to_capture_and_final_validation() {
    let dir = setup();
    let repo = dir.path();
    put(repo, "file", "reviewed\n");
    let files = selected(repo, &[Path::new("file")]);
    let cancelled = control();
    cancelled.cancel();
    assert!(Snapshot::capture(repo, &files, &cancelled)
        .unwrap_err()
        .to_string()
        .contains("cancelled"));
    let snapshot = Snapshot::capture(repo, &files, &control()).unwrap();
    assert!(snapshot.validate(&cancelled).is_err());
    let expired = Control::new(Duration::ZERO);
    assert!(Snapshot::capture(repo, &files, &expired).is_err());
    assert!(snapshot.validate(&expired).is_err());
}
