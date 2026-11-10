use super::*;
use std::fs;

fn fixture() -> (tempfile::TempDir, Request) {
    let dir = tempfile::tempdir().unwrap();
    core::git(dir.path(), &["init", "-q"]).unwrap();
    let repo = dir.path().canonicalize().unwrap();
    let path = repo.join("literal 文件 name.txt");
    fs::write(&path, "working copy\n").unwrap();
    let request = Request {
        repo,
        path,
        revision: None,
    };
    (dir, request)
}

#[test]
fn editor_selection_restores_git_precedence_instead_of_noninteractive_true() {
    let (_dir, request) = fixture();
    core::git(
        &request.repo,
        &["config", "core.editor", "configured-editor --wait"],
    )
    .unwrap();
    assert_eq!(
        configured_editor(&request.repo, None).unwrap(),
        "configured-editor --wait"
    );
    assert_eq!(
        configured_editor(&request.repo, Some(OsStr::new("override-editor --flag"))).unwrap(),
        "override-editor --flag"
    );
}

#[test]
fn host_request_escapes_control_characters_and_keeps_literal_unicode_paths() {
    let (_dir, mut request) = fixture();
    if cfg!(unix) {
        request.path = request.repo.join("file\n\x1b\x07 文件.txt");
        fs::write(&request.path, "text\n").unwrap();
    }
    let Prepared::Neovim(sequence) = request.prepare(Some("session:7")).unwrap() else {
        panic!("Expected a host request");
    };
    assert_eq!(sequence.bytes().filter(|&b| b == 0x1b).count(), 1);
    assert_eq!(sequence.bytes().filter(|&b| b == 7).count(), 1);
    let payload = sequence
        .strip_prefix("\x1b]51;igit-edit;")
        .unwrap()
        .strip_suffix('\x07')
        .unwrap();
    let value: serde_json::Value = serde_json::from_str(payload).unwrap();
    assert_eq!(value["path"], request.path.to_str().unwrap());
    assert_eq!(value["version"], 1);
    assert_eq!(value["session"], "session:7");
    let mut sent = vec![];
    send_request(&mut sent, &sequence).unwrap();
    assert_eq!(sent, sequence.as_bytes());
}

#[test]
fn unavailable_deleted_and_container_targets_never_start_an_editor() {
    let (_dir, mut request) = fixture();
    fs::remove_file(&request.path).unwrap();
    assert!(request.prepare(Some("session")).is_err());
    assert!(!request.path.exists());
    request.path = request.repo.clone();
    assert!(request.prepare(Some("session")).is_err());
    request.path = request.repo.join("../outside.txt");
    assert!(request.prepare(Some("session")).is_err());
}

#[cfg(unix)]
#[test]
fn external_editor_gets_flags_cwd_and_one_literal_path_and_reports_failures() {
    use std::os::unix::fs::PermissionsExt;
    let (_dir, mut request) = fixture();
    request.path = request
        .repo
        .join("literal 文件 ' \" ; $(touch escaped).txt");
    fs::write(&request.path, "working copy\n").unwrap();
    let script = request.repo.join("fake editor");
    fs::write(&script, "#!/bin/sh\nprintf '%s\\0' \"$PWD\" \"$EDITOR_CHECK\" \"$@\" > record\nprintf 'edited\\n' > \"$2\"\n").unwrap();
    fs::set_permissions(&script, fs::Permissions::from_mode(0o700)).unwrap();
    // Exercise a configured shell expression, not just an executable name.
    let expression = format!("EDITOR_CHECK=ok '{}' --wait", script.display());
    // tempfile names do not contain quotes; the selected file intentionally does.
    execute(&mut request.command(expression.into())).unwrap();
    let record = fs::read(request.repo.join("record")).unwrap();
    let fields: Vec<_> = record
        .split(|&b| b == 0)
        .filter(|s| !s.is_empty())
        .collect();
    assert_eq!(
        fields,
        [
            request.repo.as_os_str().as_encoded_bytes(),
            b"ok",
            b"--wait",
            request.path.as_os_str().as_encoded_bytes()
        ]
    );
    assert_eq!(fs::read_to_string(&request.path).unwrap(), "edited\n");
    assert!(!request.repo.join("escaped").exists());
    assert!(execute(&mut request.command("exit 17; :".into())).is_err());
    assert!(execute(&mut request.command("/does/not/exist".into())).is_err());
}

fn revision_fixture() -> (tempfile::TempDir, Request) {
    let (dir, mut request) = fixture();
    for (name, value) in [
        ("user.name", "Revision tester"),
        ("user.email", "revision@example.invalid"),
        ("commit.gpgsign", "false"),
        ("core.hooksPath", "/dev/null"),
        ("core.editor", "true"),
        ("core.autocrlf", "false"),
    ] {
        core::git(&request.repo, &["config", name, value]).unwrap();
    }
    fs::write(&request.path, "const value = '中文';\r\n").unwrap();
    core::git(&request.repo, &["add", "."]).unwrap();
    core::git(&request.repo, &["commit", "-qm", "revision"]).unwrap();
    request.revision = Some(core::git(&request.repo, &["rev-parse", "HEAD"]).unwrap());
    fs::write(&request.path, "staged\n").unwrap();
    core::git(&request.repo, &["add", "."]).unwrap();
    fs::write(&request.path, "working\n").unwrap();
    request.path = request.path.strip_prefix(&request.repo).unwrap().to_owned();
    (dir, request)
}

#[test]
fn revision_request_contains_immutable_identity_without_working_copy_content() {
    let (_dir, request) = revision_fixture();
    let index = fs::read(request.repo.join(".git/index")).unwrap();
    let Prepared::Neovim(sequence) = request.prepare(Some("session:revision")).unwrap() else {
        panic!("Expected a revision host request");
    };
    let payload = sequence
        .strip_prefix("\x1b]51;igit-show-revision;")
        .unwrap()
        .strip_suffix('\x07')
        .unwrap();
    let value: serde_json::Value = serde_json::from_str(payload).unwrap();
    assert_eq!(value["version"], 1);
    assert_eq!(value["session"], "session:revision");
    assert_eq!(value["repo"], request.repo.to_str().unwrap());
    assert_eq!(value["path"], request.path.to_str().unwrap());
    assert_eq!(value["revision"], request.revision.as_deref().unwrap());
    assert_eq!(index, fs::read(request.repo.join(".git/index")).unwrap());
    assert_eq!(
        fs::read_to_string(request.repo.join(&request.path)).unwrap(),
        "working\n"
    );
    fs::remove_file(request.repo.join(&request.path)).unwrap();
    assert!(matches!(
        request.prepare(Some("session")),
        Ok(Prepared::Neovim(_))
    ));
}

#[test]
fn revision_external_snapshot_has_idea_title_and_exact_blob_and_is_cleaned_up() {
    let (_dir, request) = revision_fixture();
    let Prepared::External {
        mut command,
        snapshot,
    } = request.prepare(None).unwrap()
    else {
        panic!("Expected an external snapshot");
    };
    let snapshot = snapshot.unwrap();
    let path = PathBuf::from(command.get_args().last().unwrap());
    assert!(path.starts_with(snapshot.path()));
    assert_eq!(
        path.file_name().unwrap().to_str().unwrap(),
        format!(
            "literal 文件 name.txt ({})",
            &request.revision.as_ref().unwrap()[..8]
        )
    );
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        "const value = '中文';\r\n"
    );
    assert!(fs::metadata(&path).unwrap().permissions().readonly());
    execute(&mut command).unwrap();
    drop(snapshot);
    assert!(!path.exists());
    assert_eq!(
        fs::read_to_string(request.repo.join(&request.path)).unwrap(),
        "working\n"
    );
}

#[test]
fn revision_open_rejects_absent_binary_large_or_invalid_targets_but_accepts_empty_files() {
    let (_dir, mut request) = revision_fixture();
    let original_path = request.path.clone();
    let revision = request.revision.clone();
    request.path = "missing.txt".into();
    assert!(request
        .prepare(Some("session"))
        .err()
        .unwrap()
        .to_string()
        .contains("absent"));
    request.path = "../outside.txt".into();
    assert!(request.prepare(Some("session")).is_err());
    request.path = original_path;
    request.revision = Some("HEAD".into());
    assert!(request.prepare(Some("session")).is_err());
    request.revision = revision;
    for content in [vec![], vec![0, 1], vec![b'x'; core::revision::LIMIT + 1]] {
        fs::write(request.repo.join(&request.path), &content).unwrap();
        core::git(&request.repo, &["add", "."]).unwrap();
        core::git(&request.repo, &["commit", "-qm", "new content"]).unwrap();
        request.revision = Some(core::git(&request.repo, &["rev-parse", "HEAD"]).unwrap());
        assert_eq!(request.prepare(Some("session")).is_ok(), content.is_empty());
    }
}
