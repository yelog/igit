use super::*;
use crate::log::model::tests::{commit, file, g, repo};
use std::fs;

pub(in crate::log) fn fixture() -> (tempfile::TempDir, std::path::PathBuf, Rev) {
    let tmp = tempfile::tempdir().unwrap();
    let path = repo(tmp.path(), "app", "test@example.invalid");
    file(&path, "example.js", "base\n");
    file(&path, "README.md", "base readme\n");
    g(&path, &["branch", "incoming"]);
    file(&path, "example.js", "left\n");
    file(&path, "README.md", "local readme\n");
    let left = g(&path, &["rev-parse", "HEAD"]);
    g(&path, &["checkout", "-q", "incoming"]);
    let right = file(&path, "example.js", "right\n");
    g(&path, &["checkout", "-q", "main"]);
    let out = crate::core::command(&path)
        .args(["merge", "--no-commit", "incoming"])
        .output()
        .unwrap();
    assert!(!out.status.success());
    fs::write(path.join("example.js"), "resolved from both\n").unwrap();
    g(&path, &["add", "example.js"]);
    let oid = commit(&path, "Merge with a reviewed result");
    (
        tmp,
        path,
        Rev {
            oid,
            parents: vec![left, right],
            ..Rev::default()
        },
    )
}

#[test]
fn combined_and_parent_groups_keep_duplicate_paths_and_exact_comparisons() {
    let (_tmp, path, rev) = fixture();
    let (files, parents) = changed_files(&path, &rev).unwrap();
    assert_eq!(parents.len(), 2);
    assert_eq!(parents[0].files, 1..2);
    assert_eq!(parents[1].files, 2..4);
    assert_eq!(files[0].raw, b"example.js");
    assert_eq!(files[0].parent, None);
    assert_eq!(files[1].parent, Some(0));
    assert_eq!(files[2].raw, b"README.md");
    assert_eq!(files[2].parent, Some(1));
    let patch = super::super::patch(&path, &rev, &files[2])
        .unwrap()
        .join("\n");
    assert!(patch.contains("-base readme") && patch.contains("+local readme"));
    let patch = super::super::patch(&path, &rev, &files[0])
        .unwrap()
        .join("\n");
    assert!(patch.contains("diff --combined") && patch.contains("@@@"));
}

#[test]
fn historical_sources_ignore_and_preserve_dirty_index_and_worktree() {
    let (_tmp, path, rev) = fixture();
    let (files, _) = changed_files(&path, &rev).unwrap();
    fs::write(path.join("example.js"), "staged elsewhere\n").unwrap();
    g(&path, &["add", "example.js"]);
    fs::write(path.join("example.js"), "working elsewhere\n").unwrap();
    let index = fs::read(path.join(".git/index")).unwrap();
    let before = g(&path, &["status", "--porcelain=v1"]);
    let sources = sources(&path, &rev, &files[0]).unwrap();
    assert_eq!(
        sources.map(|s| s.text),
        ["left\n", "resolved from both\n", "right\n"]
    );
    assert_eq!(before, g(&path, &["status", "--porcelain=v1"]));
    assert_eq!(index, fs::read(path.join(".git/index")).unwrap());
    assert_eq!(
        fs::read_to_string(path.join("example.js")).unwrap(),
        "working elsewhere\n"
    );
    assert_eq!(g(&path, &["rev-parse", "HEAD"]), rev.oid);
}

#[test]
fn revision_blobs_are_literal_bounded_and_report_absence_and_binary_content() {
    let tmp = tempfile::tempdir().unwrap();
    let path = repo(tmp.path(), "app", "test@example.invalid");
    let oid = file(&path, "[x] 名.js", "const 名 = 'ok';\n");
    assert_eq!(
        source(&path, &oid, "[x] 名.js".as_bytes()).unwrap().text,
        "const 名 = 'ok';\n"
    );
    assert!(source(&path, &oid, b"absent")
        .unwrap()
        .description
        .contains("Absent"));
    fs::write(path.join("binary"), b"x\0y").unwrap();
    fs::write(path.join("large"), vec![b'x'; 512 * 1024 + 1]).unwrap();
    g(&path, &["add", "."]);
    let oid = commit(&path, "opaque content");
    assert!(source(&path, &oid, b"binary")
        .err()
        .unwrap()
        .to_string()
        .contains("Binary"));
    assert!(source(&path, &oid, b"large")
        .err()
        .unwrap()
        .to_string()
        .contains("512 KiB"));
}
