//! Parse startup arguments without changing the invocation directory or Git state.
use super::{IconMode, Tab, HELP};
use anyhow::{bail, Result};
use std::{
    ffi::OsString,
    path::{Path, PathBuf},
};

#[derive(Debug)]
pub(super) enum Command {
    Help,
    Version,
    Open(Options),
}

#[derive(Debug)]
pub(super) struct Options {
    pub(super) list: bool,
    pub(super) icons: Option<IconMode>,
    pub(super) mouse: Option<bool>,
    pub(super) config_path: Option<PathBuf>,
    pub(super) check_config: bool,
    pub(super) print_config: bool,
    tab: Tab,
    path: Option<PathBuf>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Launch {
    pub(super) workspace: PathBuf,
    pub(super) tab: Tab,
    pub(super) file_history: Option<PathBuf>,
}

pub(super) fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Command> {
    let mut options = Options {
        list: false,
        icons: None,
        mouse: None,
        config_path: None,
        check_config: false,
        print_config: false,
        tab: Tab::Commit,
        path: None,
    };
    let mut args = args.into_iter();
    let mut literal = false;
    let mut panel_seen = false;
    while let Some(argument) = args.next() {
        if !literal {
            if argument == "--" {
                literal = true;
                continue;
            }
            if argument == "--help" || argument == "-h" {
                return Ok(Command::Help);
            }
            if argument == "--version" || argument == "-V" {
                return Ok(Command::Version);
            }
            if argument == "--list-roots" {
                options.list = true;
                continue;
            }
            if argument == "--icons" {
                let value = args.next().ok_or_else(|| {
                    anyhow::anyhow!("Missing --icons mode: expected nerd-font, unicode or ascii")
                })?;
                options.icons = Some(parse_icons(&value.to_string_lossy())?);
                continue;
            }
            if let Some(value) = argument.to_str().and_then(|s| s.strip_prefix("--icons=")) {
                options.icons = Some(parse_icons(value)?);
                continue;
            }
            if argument == "--config" {
                options.config_path = Some(PathBuf::from(
                    args.next()
                        .filter(|value| !value.is_empty())
                        .ok_or_else(|| anyhow::anyhow!("Missing --config path"))?,
                ));
                continue;
            }
            if let Some(value) = argument.to_str().and_then(|s| s.strip_prefix("--config=")) {
                if value.is_empty() {
                    bail!("Missing --config path");
                }
                options.config_path = Some(PathBuf::from(value));
                continue;
            }
            if argument == "--mouse" {
                let value = args
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("Missing --mouse mode: expected on or off"))?;
                options.mouse = Some(parse_mouse(&value.to_string_lossy())?);
                continue;
            }
            if let Some(value) = argument.to_str().and_then(|s| s.strip_prefix("--mouse=")) {
                options.mouse = Some(parse_mouse(value)?);
                continue;
            }
            if argument == "--check-config" {
                options.check_config = true;
                continue;
            }
            if argument == "--print-default-config" {
                options.print_config = true;
                continue;
            }
            if argument.to_string_lossy().starts_with('-') {
                bail!("Unknown option: {}\n{HELP}", argument.to_string_lossy());
            }
            if options.path.is_none() && !panel_seen {
                if let Some(tab) = parse_tab(&argument) {
                    options.tab = tab;
                    panel_seen = true;
                    continue;
                }
            }
        }
        if argument.is_empty() {
            bail!("Path must not be empty\n{HELP}");
        }
        if options.path.is_some() {
            bail!(
                "Unexpected argument: {}\n{HELP}",
                argument.to_string_lossy()
            );
        }
        options.path = Some(PathBuf::from(argument));
    }
    Ok(Command::Open(options))
}

impl Options {
    pub(super) fn launch(&self, cwd: &Path) -> Result<Launch> {
        let path = self.path.as_ref().map_or_else(
            || cwd.to_path_buf(),
            |path| {
                if path.is_absolute() {
                    path.clone()
                } else {
                    cwd.join(path)
                }
            },
        );
        if self.tab == Tab::Log && self.path.is_some() && !path.is_dir() {
            // A tracked file and even its containing directories may have been deleted.
            // Discovery needs an existing directory; Log resolves the owning worktree.
            let workspace = path
                .parent()
                .and_then(|parent| parent.ancestors().find(|ancestor| ancestor.is_dir()))
                .ok_or_else(|| {
                    anyhow::anyhow!("No existing parent directory for file: {}", path.display())
                })?
                .to_path_buf();
            Ok(Launch {
                workspace,
                tab: self.tab,
                file_history: Some(path),
            })
        } else {
            Ok(Launch {
                workspace: path,
                tab: self.tab,
                file_history: None,
            })
        }
    }
}

fn parse_tab(argument: &std::ffi::OsStr) -> Option<Tab> {
    match argument.to_str()? {
        "commit" => Some(Tab::Commit),
        "log" => Some(Tab::Log),
        "push" => Some(Tab::Push),
        "branch" => Some(Tab::Branch),
        "stash" => Some(Tab::Stash),
        "remote" => Some(Tab::Remote),
        _ => None,
    }
}

fn parse_icons(value: &str) -> Result<IconMode> {
    value
        .parse()
        .map_err(|reason: &str| anyhow::anyhow!("Invalid --icons value '{value}': {reason}"))
}

fn parse_mouse(value: &str) -> Result<bool> {
    match value {
        "on" => Ok(true),
        "off" => Ok(false),
        _ => bail!("Invalid --mouse value '{value}': expected on or off"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn options(arguments: &[&str]) -> Options {
        let Command::Open(options) = parse(arguments.iter().map(OsString::from)).unwrap() else {
            panic!("Expected startup options");
        };
        options
    }

    #[test]
    fn default_directory_and_every_direct_panel_resolve_from_the_invocation_directory() {
        let cwd = tempfile::tempdir().unwrap();
        let default = options(&[]).launch(cwd.path()).unwrap();
        assert_eq!(default.workspace, cwd.path());
        assert_eq!(default.tab, Tab::Commit);
        assert!(default.file_history.is_none());
        let directory = options(&["workspace"]).launch(cwd.path()).unwrap();
        assert_eq!(directory.workspace, cwd.path().join("workspace"));
        assert_eq!(directory.tab, Tab::Commit);
        for (name, tab) in [
            ("commit", Tab::Commit),
            ("log", Tab::Log),
            ("push", Tab::Push),
            ("branch", Tab::Branch),
            ("stash", Tab::Stash),
            ("remote", Tab::Remote),
        ] {
            let launch = options(&[name]).launch(cwd.path()).unwrap();
            assert_eq!(launch.tab, tab);
            assert_eq!(launch.workspace, cwd.path());
            assert!(launch.file_history.is_none());
        }
    }

    #[test]
    fn options_work_before_and_after_panel_and_path_arguments() {
        let parsed = options(&[
            "--icons",
            "ascii",
            "log",
            "module/文件 name.rs",
            "--config=settings.toml",
            "--mouse",
            "off",
            "--list-roots",
            "--check-config",
            "--print-default-config",
        ]);
        assert_eq!(parsed.tab, Tab::Log);
        assert_eq!(parsed.path, Some("module/文件 name.rs".into()));
        assert_eq!(parsed.icons, Some(IconMode::Ascii));
        assert_eq!(parsed.mouse, Some(false));
        assert_eq!(parsed.config_path, Some("settings.toml".into()));
        assert!(parsed.list && parsed.check_config && parsed.print_config);
        let parsed = options(&[
            "--config",
            "custom settings.toml",
            "--icons=unicode",
            "--mouse=on",
        ]);
        assert_eq!(parsed.config_path, Some("custom settings.toml".into()));
        assert_eq!(parsed.icons, Some(IconMode::Unicode));
        assert_eq!(parsed.mouse, Some(true));
    }

    #[test]
    fn separator_keeps_option_and_command_names_literal() {
        for name in [
            "commit", "log", "push", "branch", "stash", "update", "--help", "--",
        ] {
            let parsed = options(&["--", name]);
            assert_eq!(parsed.tab, Tab::Commit);
            assert_eq!(parsed.path, Some(name.into()));
        }
        let parsed = options(&["log", "--", "--version"]);
        assert_eq!(parsed.tab, Tab::Log);
        assert_eq!(parsed.path, Some("--version".into()));
        assert_eq!(options(&["./log"]).path, Some("./log".into()));
        assert_eq!(options(&["log", "branch"]).path, Some("branch".into()));
    }

    #[test]
    fn file_history_uses_file_parent_and_keeps_deleted_paths() {
        let cwd = tempfile::tempdir().unwrap();
        let nested = cwd.path().join("模块 with spaces");
        fs::create_dir(&nested).unwrap();
        let file = nested.join("source.rs");
        fs::write(&file, "source").unwrap();
        let launch = options(&["log", "模块 with spaces/source.rs"])
            .launch(cwd.path())
            .unwrap();
        assert_eq!(launch.workspace, nested);
        assert_eq!(launch.file_history, Some(file.clone()));
        fs::remove_file(&file).unwrap();
        fs::remove_dir(&nested).unwrap();
        let deleted = options(&["log", "模块 with spaces/source.rs"])
            .launch(cwd.path())
            .unwrap();
        assert_eq!(deleted.workspace, cwd.path());
        assert_eq!(deleted.file_history, Some(file));
    }

    #[test]
    fn log_directories_and_absolute_file_paths_have_distinct_startup_targets() {
        let cwd = tempfile::tempdir().unwrap();
        let other = tempfile::tempdir().unwrap();
        let directory = options(&["log", other.path().to_str().unwrap()])
            .launch(cwd.path())
            .unwrap();
        assert_eq!(directory.workspace, other.path());
        assert!(directory.file_history.is_none());
        let file = other.path().join("--source.rs");
        let history = options(&["log", "--", file.to_str().unwrap()])
            .launch(cwd.path())
            .unwrap();
        assert_eq!(history.workspace, other.path());
        assert_eq!(history.file_history, Some(file));
    }

    #[test]
    fn malformed_arguments_are_rejected_and_information_flags_do_not_need_paths() {
        for arguments in [
            vec!["--unknown"],
            vec!["--icons"],
            vec!["--icons=wrong"],
            vec!["--mouse"],
            vec!["--mouse=wrong"],
            vec!["--config"],
            vec!["--config="],
            vec!["--config", ""],
            vec![""],
            vec!["workspace", "log"],
            vec!["log", "first", "second"],
            vec!["--", "log", "file"],
        ] {
            assert!(
                parse(arguments.iter().map(OsString::from)).is_err(),
                "{arguments:?}"
            );
        }
        for flag in ["--help", "-h"] {
            assert!(matches!(parse([flag.into()]).unwrap(), Command::Help));
        }
        for flag in ["--version", "-V"] {
            assert!(matches!(parse([flag.into()]).unwrap(), Command::Version));
        }
    }

    #[cfg(unix)]
    #[test]
    fn parsing_preserves_non_utf8_path_arguments() {
        use std::os::unix::ffi::OsStringExt;
        let path = OsString::from_vec(b"source-\xff.rs".to_vec());
        let Command::Open(parsed) = parse(["log".into(), "--".into(), path.clone()]).unwrap()
        else {
            panic!("Expected startup options");
        };
        assert_eq!(parsed.path, Some(PathBuf::from(path)));
    }
}
