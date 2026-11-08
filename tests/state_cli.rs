use std::{ffi::OsStr, path::Path, process::Command};

fn isolated_cli(binary: &str, directory: &Path) -> Command {
    let mut command = Command::new(binary);
    command.env_clear();
    for name in ["PATH", "SystemRoot", "WINDIR", "TEMP", "TMP", "TMPDIR"] {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
    command
        .current_dir(directory)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", directory.join("settings.toml"))
        .args(["--list-roots", "--config"])
        .arg(directory.join("settings.toml"))
        .arg(directory.join("repo"));
    command
}

#[test]
fn both_entry_points_discover_roots_without_home() {
    let temporary = tempfile::tempdir().unwrap();
    let directory = temporary.path();
    let repo = directory.join("repo");
    std::fs::create_dir(&repo).unwrap();
    std::fs::write(directory.join("settings.toml"), "").unwrap();
    igit::core::git(&repo, &["init", "-q", "-b", "main"]).unwrap();

    let state = directory.join("private state");
    let mut cases = vec![
        vec![("IGIT_STATE_DIR", state.as_os_str())],
        vec![
            ("IGIT_STATE_DIR", OsStr::new("")),
            ("XDG_STATE_HOME", state.as_os_str()),
        ],
    ];
    if cfg!(windows) {
        for key in ["LOCALAPPDATA", "APPDATA", "USERPROFILE"] {
            cases.push(vec![
                ("IGIT_STATE_DIR", OsStr::new("")),
                ("XDG_STATE_HOME", OsStr::new("")),
                (key, state.as_os_str()),
            ]);
        }
    }
    for binary in [env!("CARGO_BIN_EXE_igit"), env!("CARGO_BIN_EXE_ig")] {
        for variables in &cases {
            let output = isolated_cli(binary, directory)
                .envs(variables.iter().copied())
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{binary} with {variables:?}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            let roots = String::from_utf8(output.stdout).unwrap();
            assert_eq!(
                roots.trim_end(),
                format!(
                    "repo\tmain\tworktree\t{}",
                    std::fs::canonicalize(&repo).unwrap().display()
                )
            );
        }
    }
}

#[test]
fn empty_state_variables_do_not_resolve_to_the_working_directory() {
    let temporary = tempfile::tempdir().unwrap();
    let directory = temporary.path();
    std::fs::create_dir(directory.join("repo")).unwrap();
    std::fs::write(directory.join("settings.toml"), "").unwrap();

    let output = isolated_cli(env!("CARGO_BIN_EXE_igit"), directory)
        .envs([
            ("IGIT_STATE_DIR", ""),
            ("XDG_STATE_HOME", ""),
            ("LOCALAPPDATA", ""),
            ("APPDATA", ""),
            ("USERPROFILE", ""),
            ("HOME", ""),
        ])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("Set IGIT_STATE_DIR to a private writable directory"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stdout.is_empty());
}
