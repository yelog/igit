use std::process::Command;

#[test]
fn both_commands_report_the_cargo_version_without_loading_configuration() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("settings.toml"), "invalid = [").unwrap();
    let header = format!(
        "IGit {} — terminal Git workbench",
        env!("CARGO_PKG_VERSION")
    );
    for binary in [env!("CARGO_BIN_EXE_igit"), env!("CARGO_BIN_EXE_ig")] {
        for flag in ["--version", "-V", "--help", "-h"] {
            let output = Command::new(binary)
                .arg(flag)
                .env("IGIT_CONFIG_HOME", dir.path())
                .output()
                .unwrap();
            assert!(output.status.success(), "{output:?}");
            let stdout = String::from_utf8(output.stdout).unwrap();
            assert_eq!(stdout.lines().next(), Some(header.as_str()));
            assert!(output.stderr.is_empty());
        }
    }
}
