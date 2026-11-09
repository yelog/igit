//! Read-only progress from the native helper's private download directory.
use anyhow::{Context, Result};
use std::{
    fs::{self, File},
    io::Read,
    path::Path,
    process::{Command, Output, Stdio},
    thread,
    time::Duration,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Progress {
    #[default]
    Preparing,
    Downloading {
        bytes: u64,
        total: Option<u64>,
    },
    VerifyingChecksum,
    Extracting,
    VerifyingInstallation,
    Activating,
    Finishing,
}

impl Progress {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Preparing => "Preparing update",
            Self::Downloading { .. } => "Downloading update",
            Self::VerifyingChecksum => "Verifying checksum",
            Self::Extracting => "Extracting release",
            Self::VerifyingInstallation => "Verifying installation",
            Self::Activating => "Activating update",
            Self::Finishing => "Finishing update",
        }
    }

    pub(crate) fn ratio(self) -> Option<f64> {
        match self {
            Self::Downloading {
                bytes,
                total: Some(total),
            } if total > 0 && bytes <= total => Some(bytes as f64 / total as f64),
            _ => None,
        }
    }

    fn read(directory: &Path) -> Option<Self> {
        Some(
            match fs::read_to_string(directory.join("stage")).ok()?.trim() {
                "downloading" => {
                    let archive = if cfg!(windows) {
                        "release.zip"
                    } else {
                        "release.tar.gz"
                    };
                    let bytes = fs::metadata(directory.join(archive)).map_or(0, |file| file.len());
                    let total = download_total(&directory.join("download.headers"));
                    Self::Downloading { bytes, total }
                }
                "verifying_checksum" => Self::VerifyingChecksum,
                "extracting" => Self::Extracting,
                "verifying_installation" => Self::VerifyingInstallation,
                "activating" => Self::Activating,
                "finishing" => Self::Finishing,
                _ => return None,
            },
        )
    }
}

fn download_total(path: &Path) -> Option<u64> {
    const HEADER_LIMIT: u64 = 64 * 1024;
    let mut raw = Vec::new();
    File::open(path)
        .ok()?
        .take(HEADER_LIMIT + 1)
        .read_to_end(&mut raw)
        .ok()?;
    if raw.len() as u64 > HEADER_LIMIT {
        return None;
    }
    response_length(&String::from_utf8_lossy(&raw))
}

fn response_length(headers: &str) -> Option<u64> {
    // curl can append proxy, redirect, and retry responses to the same file.
    // A partial response or a transfer encoding cannot supply a reliable total.
    let (mut success, mut complete, mut encoded) = (false, false, false);
    let mut length = None;
    for line in headers.lines() {
        if line.starts_with("HTTP/") {
            let mut parts = line.splitn(3, ' ');
            success = parts.nth(1) == Some("200")
                && !parts
                    .next()
                    .is_some_and(|reason| reason.eq_ignore_ascii_case("connection established"));
            complete = false;
            encoded = false;
            length = None;
        } else if line.is_empty() {
            complete = true;
        } else if !complete {
            if let Some((name, value)) = line.split_once(':') {
                if name.eq_ignore_ascii_case("transfer-encoding") {
                    encoded = true;
                } else if name.eq_ignore_ascii_case("content-length") {
                    length = Some(if length.is_some() {
                        None
                    } else {
                        value
                            .trim()
                            .parse::<u64>()
                            .ok()
                            .filter(|n| *n > 0 && *n <= 128 * 1024 * 1024)
                    });
                }
            }
        }
    }
    if success && complete && !encoded {
        length.flatten()
    } else {
        None
    }
}

pub(super) fn capture(
    command: &mut Command,
    directory: &Path,
    notify: &mut dyn FnMut(Progress),
) -> Result<Output> {
    let progress_dir = directory.join("progress");
    let stdout = directory.join("report.json");
    let stderr = directory.join("diagnostics");
    // Use files so the helper cannot block on a full pipe while we wait and
    // sample the download. The final report is still decoded as one JSON value.
    let mut child = command
        .arg(if cfg!(windows) {
            "-ProgressDir"
        } else {
            "--progress-dir"
        })
        .arg(&progress_dir)
        .stdin(Stdio::null())
        .stdout(File::create(&stdout)?)
        .stderr(File::create(&stderr)?)
        .spawn()
        .context("Start the update helper")?;
    let mut previous = Progress::Preparing;
    let status = loop {
        if let Some(status) = child.try_wait().context("Wait for the update helper")? {
            break status;
        }
        if let Some(progress) = Progress::read(&progress_dir).filter(|value| *value != previous) {
            notify(progress);
            previous = progress;
        }
        thread::sleep(Duration::from_millis(100));
    };
    Ok(Output {
        status,
        stdout: fs::read(stdout)?,
        stderr: fs::read(stderr)?,
    })
}

#[cfg(test)]
mod tests {
    use super::response_length;

    #[test]
    fn only_a_complete_final_download_response_supplies_a_total() {
        let redirect = "HTTP/1.1 302 Found\r\nContent-Length: 2\r\n\r\n";
        let proxy = "HTTP/1.1 200 Connection established\r\nContent-Length: 3\r\n\r\n";
        let final_response = "HTTP/2 200\r\ncontent-length: 8000\r\n\r\n";
        assert_eq!(
            response_length(&format!("{proxy}{redirect}{final_response}")),
            Some(8000)
        );
        for headers in [
            redirect,
            proxy,
            "HTTP/2 200\r\ncontent-length: 42\r\n",
            "HTTP/1.1 200 OK\r\nContent-Length: 42\r\nTransfer-Encoding: chunked\r\n\r\n",
            "HTTP/1.1 200 OK\r\nContent-Length: 42\r\nContent-Length: 43\r\n\r\n",
            "HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n",
        ] {
            assert_eq!(response_length(headers), None, "{headers:?}");
        }
    }
}
