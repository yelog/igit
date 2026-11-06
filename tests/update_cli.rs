use std::process::Command;

#[test]
fn update_help_and_unmanaged_checks_do_not_load_configuration_or_open_a_terminal() {
    let temporary = tempfile::tempdir().unwrap();
    std::fs::write(temporary.path().join("settings.toml"), "invalid = [").unwrap();
    for binary in [env!("CARGO_BIN_EXE_igit"), env!("CARGO_BIN_EXE_ig")] {
        let output = Command::new(binary)
            .args(["update", "--help"])
            .env("IGIT_CONFIG_HOME", temporary.path())
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
        let help = String::from_utf8(output.stdout).unwrap();
        assert!(help.contains("--check") && help.contains("--channel stable|beta"));
        let before = std::fs::read(binary).unwrap();
        for check in [true, false] {
            let mut command = Command::new(binary);
            command.args(["update", "--json"]);
            if check {
                command.arg("--check");
            }
            let output = command
                .env("IGIT_CONFIG_HOME", temporary.path())
                .output()
                .unwrap();
            assert_eq!(output.status.success(), check, "{output:?}");
            let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
            assert_eq!(report["status"], "manager_action_required");
            assert_eq!(report["current_version"], env!("CARGO_PKG_VERSION"));
        }
        assert_eq!(std::fs::read(binary).unwrap(), before);
    }
}

#[test]
fn update_rejects_unknown_flags_and_channel_values_before_network_or_writes() {
    for binary in [env!("CARGO_BIN_EXE_igit"), env!("CARGO_BIN_EXE_ig")] {
        for args in [
            vec!["update", "--channel"],
            vec!["update", "--channel", "nightly"],
            vec!["update", "--install-dir", "/tmp/foreign"],
        ] {
            let output = Command::new(binary).args(args).output().unwrap();
            assert!(!output.status.success());
            assert!(output.stdout.is_empty());
        }
    }
}
