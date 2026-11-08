use super::*;
use std::fs;

fn fixture() -> (tempfile::TempDir, Request) {
    let dir = tempfile::tempdir().unwrap();
    core::git(dir.path(), &["init", "-q"]).unwrap();
    let repo = dir.path().canonicalize().unwrap();
    let path = repo.join("literal 文件 name.txt");
    fs::write(&path, "working copy\n").unwrap();
    let request = Request { repo, path };
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
