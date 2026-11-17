use anyhow::{Context as _, Result};
use serde::{Deserialize, Serialize};
use std::{ffi::OsString, path::PathBuf};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Native {
    platform: String,
    bytes: Vec<u8>,
}

impl Native {
    pub(crate) fn new(path: &std::path::Path) -> Self {
        #[cfg(unix)]
        {
            Self {
                platform: "unix".into(),
                bytes: path.as_os_str().as_encoded_bytes().to_vec(),
            }
        }
        #[cfg(windows)]
        {
            use std::os::windows::ffi::OsStrExt;
            Self {
                platform: "windows".into(),
                bytes: path
                    .as_os_str()
                    .encode_wide()
                    .flat_map(u16::to_le_bytes)
                    .collect(),
            }
        }
    }

    pub(crate) fn path(&self) -> Result<PathBuf> {
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStringExt;
            anyhow::ensure!(self.platform == "unix", "State belongs to another platform");
            Ok(OsString::from_vec(self.bytes.clone()).into())
        }
        #[cfg(windows)]
        {
            use std::os::windows::ffi::OsStringExt;
            anyhow::ensure!(
                self.platform == "windows" && self.bytes.len().is_multiple_of(2),
                "Invalid Windows state path"
            );
            let wide: Vec<_> = self
                .bytes
                .chunks_exact(2)
                .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
                .collect();
            Ok(OsString::from_wide(&wide).into())
        }
    }
}

#[derive(Clone, Copy)]
enum Platform {
    Unix,
    Windows,
}

pub(super) fn directory() -> Result<PathBuf> {
    let platform = if cfg!(windows) {
        Platform::Windows
    } else {
        Platform::Unix
    };
    resolve(platform, |key| std::env::var_os(key))
}

fn resolve(platform: Platform, env: impl Fn(&str) -> Option<OsString>) -> Result<PathBuf> {
    let value = |key| {
        env(key)
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
    };
    if let Some(path) = value("IGIT_STATE_DIR") {
        return Ok(path);
    }
    if let Some(path) = value("XDG_STATE_HOME") {
        return Ok(path.join("igit"));
    }
    let home = || value("HOME").map(|path| path.join(".local/state/igit"));
    match platform {
        Platform::Unix => home(),
        Platform::Windows => value("LOCALAPPDATA")
            .or_else(|| value("APPDATA"))
            .map(|path| path.join("igit").join("state"))
            .or_else(|| {
                value("USERPROFILE").map(|path| {
                    path.join("AppData")
                        .join("Local")
                        .join("igit")
                        .join("state")
                })
            })
            // Retain support for Windows shells that supply only a Unix-style HOME.
            .or_else(home),
    }
    .context("Set IGIT_STATE_DIR to a private writable directory")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resolved(platform: Platform, variables: &[(&str, &str)]) -> Result<PathBuf> {
        resolve(platform, |key| {
            variables
                .iter()
                .find_map(|(name, value)| (*name == key).then(|| OsString::from(value)))
        })
    }

    #[test]
    fn explicit_overrides_take_precedence_on_both_platforms() {
        for platform in [Platform::Unix, Platform::Windows] {
            let variables = [
                ("IGIT_STATE_DIR", "private state"),
                ("XDG_STATE_HOME", "xdg"),
                ("LOCALAPPDATA", "local"),
                ("APPDATA", "roaming"),
                ("USERPROFILE", "profile"),
                ("HOME", "home"),
            ];
            assert_eq!(
                resolved(platform, &variables).unwrap(),
                PathBuf::from("private state")
            );
            assert_eq!(
                resolved(platform, &variables[1..]).unwrap(),
                PathBuf::from("xdg").join("igit")
            );
        }
    }

    #[test]
    fn empty_overrides_fall_through() {
        for platform in [Platform::Unix, Platform::Windows] {
            assert_eq!(
                resolved(
                    platform,
                    &[("IGIT_STATE_DIR", ""), ("XDG_STATE_HOME", "xdg")]
                )
                .unwrap(),
                PathBuf::from("xdg").join("igit")
            );
            assert_eq!(
                resolved(
                    platform,
                    &[
                        ("IGIT_STATE_DIR", ""),
                        ("XDG_STATE_HOME", ""),
                        ("HOME", "home")
                    ]
                )
                .unwrap(),
                PathBuf::from("home").join(".local/state/igit")
            );
        }
    }

    #[test]
    fn unix_home_path_is_unchanged_and_ignores_windows_variables() {
        let variables = [
            ("LOCALAPPDATA", "local"),
            ("APPDATA", "roaming"),
            ("USERPROFILE", "profile"),
            ("HOME", "home"),
        ];
        assert_eq!(
            resolved(Platform::Unix, &variables).unwrap(),
            PathBuf::from("home").join(".local/state/igit")
        );
        assert!(resolved(Platform::Unix, &variables[..3]).is_err());
    }

    #[test]
    fn windows_uses_per_user_directories_without_home() {
        let variables = [
            ("LOCALAPPDATA", "local"),
            ("APPDATA", "roaming"),
            ("USERPROFILE", "profile"),
        ];
        for (index, expected) in [
            PathBuf::from("local").join("igit").join("state"),
            PathBuf::from("roaming").join("igit").join("state"),
            PathBuf::from("profile")
                .join("AppData")
                .join("Local")
                .join("igit")
                .join("state"),
        ]
        .into_iter()
        .enumerate()
        {
            assert_eq!(
                resolved(Platform::Windows, &variables[index..]).unwrap(),
                expected
            );
        }
    }

    #[test]
    fn windows_skips_empty_native_directories() {
        for (local, roaming, expected) in [
            (
                "",
                "roaming",
                PathBuf::from("roaming").join("igit").join("state"),
            ),
            (
                "",
                "",
                PathBuf::from("profile")
                    .join("AppData")
                    .join("Local")
                    .join("igit")
                    .join("state"),
            ),
        ] {
            assert_eq!(
                resolved(
                    Platform::Windows,
                    &[
                        ("IGIT_STATE_DIR", ""),
                        ("XDG_STATE_HOME", ""),
                        ("LOCALAPPDATA", local),
                        ("APPDATA", roaming),
                        ("USERPROFILE", "profile"),
                        ("HOME", "home")
                    ]
                )
                .unwrap(),
                expected
            );
        }
    }

    #[test]
    fn missing_or_empty_environment_requires_an_explicit_directory() {
        for platform in [Platform::Unix, Platform::Windows] {
            for variables in [
                &[][..],
                &[
                    ("IGIT_STATE_DIR", ""),
                    ("XDG_STATE_HOME", ""),
                    ("LOCALAPPDATA", ""),
                    ("APPDATA", ""),
                    ("USERPROFILE", ""),
                    ("HOME", ""),
                ][..],
            ] {
                assert_eq!(
                    resolved(platform, variables).unwrap_err().to_string(),
                    "Set IGIT_STATE_DIR to a private writable directory"
                );
            }
        }
    }

    #[test]
    fn explicit_paths_keep_spaces_and_relative_components() {
        for path in [" ", "relative/../private state"] {
            for platform in [Platform::Unix, Platform::Windows] {
                assert_eq!(
                    resolved(platform, &[("IGIT_STATE_DIR", path)]).unwrap(),
                    PathBuf::from(path)
                );
            }
        }
    }

    #[cfg(unix)]
    #[test]
    fn native_path_bytes_are_preserved() {
        use std::os::unix::ffi::OsStringExt;

        let path = OsString::from_vec(b"private-\xff".to_vec());
        for platform in [Platform::Unix, Platform::Windows] {
            assert_eq!(
                resolve(platform, |key| (key == "IGIT_STATE_DIR")
                    .then(|| path.clone()))
                .unwrap(),
                PathBuf::from(&path)
            );
        }
    }
}
