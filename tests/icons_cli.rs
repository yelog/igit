use std::process::Command;

#[test]
fn both_entry_points_accept_icon_modes_without_changing_list_output() {
    let repo = tempfile::tempdir().unwrap();
    igit::core::git(repo.path(), &["init", "-q", "-b", "main"]).unwrap();
    let expected = Command::new(env!("CARGO_BIN_EXE_igit"))
        .arg("--list-roots")
        .arg(repo.path())
        .output()
        .unwrap();
    assert!(expected.status.success() && !expected.stdout.is_empty());
    for bin in [env!("CARGO_BIN_EXE_igit"), env!("CARGO_BIN_EXE_ig")] {
        for mode in ["nerd-font", "unicode", "ascii"] {
            let out = Command::new(bin)
                .args(["--list-roots", "--icons", mode])
                .arg(repo.path())
                .output()
                .unwrap();
            assert!(out.status.success());
            assert_eq!(out.stdout, expected.stdout);
        }
    }
}

#[test]
fn missing_and_unknown_modes_are_rejected_before_opening_the_terminal() {
    for args in [
        vec!["--icons"],
        vec!["--icons=emoji"],
        vec!["--icons", "auto"],
    ] {
        let out = Command::new(env!("CARGO_BIN_EXE_ig"))
            .args(args)
            .output()
            .unwrap();
        assert!(!out.status.success());
        let message = String::from_utf8(out.stderr).unwrap();
        assert!(
            message.contains("--icons") && message.contains("nerd-font, unicode or ascii"),
            "{message}"
        );
    }
}
