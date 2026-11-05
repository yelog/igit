use std::{
    fs,
    process::{Command, Output},
};

fn run(bin: &str, home: &std::path::Path, args: &[&str]) -> Output {
    Command::new(bin)
        .env("IGIT_CONFIG_HOME", home)
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn configuration_cli_defaults_validation_and_recovery_work_without_terminal_or_git() {
    let dir = tempfile::tempdir().unwrap();
    for bin in [env!("CARGO_BIN_EXE_igit"), env!("CARGO_BIN_EXE_ig")] {
        let output = run(bin, dir.path(), &["--print-default-config"]);
        assert!(output.status.success());
        assert_eq!(
            String::from_utf8(output.stdout).unwrap(),
            include_str!("../config/default.toml")
        );
        let output = run(
            bin,
            dir.path(),
            &["--check-config", "/nonexistent/igit-repo"],
        );
        assert!(output.status.success(), "{:?}", output);
        assert!(String::from_utf8_lossy(&output.stdout).contains("built-in defaults"));
        assert!(!dir.path().join("settings.toml").exists());
        fs::write(
            dir.path().join("settings.toml"),
            "[keybindings.commit]\nreview=['i']",
        )
        .unwrap();
        let output = run(
            bin,
            dir.path(),
            &["--check-config", "/nonexistent/igit-repo"],
        );
        assert!(!output.status.success());
        let error = String::from_utf8_lossy(&output.stderr);
        assert!(
            error.contains("settings.toml")
                && error.contains("Conflicting")
                && error.contains("commit.review"),
            "{error}"
        );
        assert!(!output.stdout.contains(&27));
        for flag in ["--help", "--version", "--print-default-config"] {
            assert!(run(bin, dir.path(), &[flag]).status.success());
        }
        fs::remove_file(dir.path().join("settings.toml")).unwrap();
    }
}

#[test]
fn configuration_cli_explicit_path_is_required_and_overrides_the_environment() {
    let dir = tempfile::tempdir().unwrap();
    let custom = dir.path().join("custom settings.toml");
    fs::write(dir.path().join("settings.toml"), "invalid = [").unwrap();
    let bin = env!("CARGO_BIN_EXE_igit");
    let path = custom.to_str().unwrap();
    assert!(!run(bin, dir.path(), &["--config", path, "--check-config"])
        .status
        .success());
    fs::write(&custom, "[ui]\nicons='unicode'\n[terminal]\nmouse=false").unwrap();
    let output = run(
        bin,
        dir.path(),
        &[
            "--config",
            path,
            "--icons=ascii",
            "--mouse=on",
            "--check-config",
        ],
    );
    assert!(output.status.success(), "{:?}", output);
    assert!(String::from_utf8_lossy(&output.stdout).contains(path));
    assert!(run(
        bin,
        dir.path(),
        &[&format!("--config={path}"), "--check-config"]
    )
    .status
    .success());
    for args in [
        vec!["--config"],
        vec!["--config="],
        vec!["--mouse"],
        vec!["--mouse=maybe"],
        vec!["--icons=bad"],
    ] {
        assert!(!run(bin, dir.path(), &args).status.success(), "{args:?}");
    }
}

// APFS rejects invalid UTF-8 filenames; Linux filesystems can exercise this path.
#[cfg(target_os = "linux")]
#[test]
fn configuration_cli_preserves_non_utf8_explicit_paths() {
    use std::{ffi::OsString, os::unix::ffi::OsStringExt};
    let dir = tempfile::tempdir().unwrap();
    let path = dir
        .path()
        .join(OsString::from_vec(b"settings-\xff.toml".to_vec()));
    fs::write(&path, "[ui]\nicons='ascii'").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_igit"))
        .arg("--config")
        .arg(path)
        .arg("--check-config")
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
}
