mod configuration;
mod transport;

use super::{
    command,
    process::{self, ReadControl},
    read_output,
};
use anyhow::{bail, ensure, Result};
use configuration::{Configuration, Repository};
use std::{
    collections::BTreeSet,
    fmt,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub(crate) struct Key {
    pub common_dir: PathBuf,
    pub remote: String,
    pub fingerprint: u64,
    pub branch: Option<String>,
}

#[derive(Clone)]
pub(crate) struct Target {
    pub key: Key,
    pub path: PathBuf,
    pub endpoint: String,
    git_dir: PathBuf,
    configuration: Configuration,
    refspecs: Vec<String>,
    transport: transport::Transport,
    automatic: bool,
}

impl fmt::Debug for Target {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Target")
            .field("key", &self.key)
            .field("path", &self.path)
            .field("endpoint", &"<redacted>")
            .finish_non_exhaustive()
    }
}

#[derive(Debug)]
pub(crate) struct Discovery {
    pub targets: Vec<Target>,
    pub warnings: Vec<String>,
}

#[derive(Debug)]
pub(crate) struct Outcome {
    pub changed: bool,
    pub tip: Option<String>,
}

#[derive(Clone)]
pub(crate) struct Observation {
    pub path: PathBuf,
    pub remote: String,
    pub endpoint: String,
    pub branch: Option<String>,
    pub tip: Option<String>,
}

pub(crate) fn push_target(path: &Path, remote: &str, branch: &str) -> Result<Target> {
    let repository = Repository::read(path)?;
    ensure!(
        repository.remotes()?.iter().any(|name| name == remote),
        "Cannot capture an unconfigured push remote"
    );
    let mut target = repository.target(remote, Some(branch))?;
    target.automatic = false;
    Ok(target)
}

pub(crate) fn discover(
    path: &Path,
    all_remotes: bool,
    excluded_remotes: &[String],
    push_targets: &[(String, String)],
) -> Result<Vec<Target>> {
    let report = discover_with_warnings(path, all_remotes, excluded_remotes, push_targets)?;
    if report.targets.is_empty() && !report.warnings.is_empty() {
        bail!("{}", report.warnings.join("; "));
    }
    Ok(report.targets)
}

pub(crate) fn discover_with_warnings(
    path: &Path,
    all_remotes: bool,
    excluded_remotes: &[String],
    push_targets: &[(String, String)],
) -> Result<Discovery> {
    process::check_read_control()?;
    let repository = Repository::read(path)?;
    let remotes = repository.remotes()?;
    let mut report = Discovery {
        targets: Vec::new(),
        warnings: Vec::new(),
    };
    let ordinary = if all_remotes {
        remotes.clone()
    } else {
        repository.default_remote(&remotes)?.into_iter().collect()
    };
    let requests: BTreeSet<_> = ordinary
        .into_iter()
        .map(|remote| (remote, None))
        .chain(
            push_targets
                .iter()
                .map(|(remote, branch)| (remote.clone(), Some(branch.clone()))),
        )
        .collect();
    for (remote, branch) in requests {
        process::check_read_control()?;
        if !remotes.contains(&remote) || excluded_remotes.contains(&remote) {
            continue;
        }
        let target = (|| {
            ensure!(
                !remote.starts_with('-') && remote != "." && !remote.chars().any(char::is_control),
                "Automatic fetch skipped an unsupported remote name"
            );
            if repository.configuration.skip(&remote)? {
                return Ok(None);
            }
            repository.target(&remote, branch.as_deref()).map(Some)
        })();
        match target {
            Ok(Some(target)) => report.targets.push(target),
            Ok(None) => (),
            Err(error) => {
                process::check_read_control()?;
                report.warnings.push(error.to_string());
            }
        }
    }
    Ok(report)
}

pub(crate) fn execute(target: &Target, control: &ReadControl) -> Result<Outcome> {
    let control = control.scoped();
    process::with_read_control(&control, || execute_controlled(target, &control))
}

fn execute_controlled(target: &Target, control: &ReadControl) -> Result<Outcome> {
    control.check()?;
    revalidate(target)?;
    let hooks =
        tempfile::tempdir().map_err(|_| anyhow::anyhow!("Cannot isolate automatic fetch hooks"))?;
    if let Some(branch) = &target.key.branch {
        let reference = format!("refs/heads/{branch}");
        let mut query = transport::command(target, hooks.path());
        query.args(["ls-remote", "--refs", "--", &target.endpoint, &reference]);
        let output = transport::run(target, control, "push target lookup", &mut query)?;
        let text = std::str::from_utf8(&output)
            .map_err(|_| anyhow::anyhow!("Invalid automatic fetch branch response"))?;
        let mut tip = None;
        for line in text.lines() {
            let (oid, name) = line
                .split_once('\t')
                .ok_or_else(|| anyhow::anyhow!("Invalid automatic fetch branch response"))?;
            ensure!(
                name == reference && valid_oid(oid) && tip.is_none(),
                "Ambiguous automatic fetch branch response"
            );
            tip = Some(oid.to_owned());
        }
        revalidate(target)?;
        if let Some(oid) = &tip {
            let mut fetch = transport::fetch_command(target, hooks.path());
            fetch.args(["--", &target.endpoint, oid]);
            transport::run(target, control, "push target fetch", &mut fetch)?;
            revalidate(target)?;
        }
        return Ok(Outcome {
            changed: false,
            tip,
        });
    }
    configuration::protect_refs(target)?;
    let before = tracking_refs(&target.path)?;
    revalidate(target)?;
    let mut fetch = transport::fetch_command(target, hooks.path());
    fetch.args(["--", &target.endpoint]);
    fetch.args(&target.refspecs);
    transport::run(target, control, "automatic fetch", &mut fetch)?;
    revalidate(target)?;
    let after = tracking_refs(&target.path)?;
    Ok(Outcome {
        changed: before != after,
        tip: None,
    })
}

fn revalidate(target: &Target) -> Result<()> {
    let repository = Repository::read(&target.path)?;
    ensure!(
        repository.path == target.path
            && repository.git_dir == target.git_dir
            && repository.common_dir == target.key.common_dir
            && repository.configuration == target.configuration,
        "Automatic fetch configuration or Git directory changed; discover targets again"
    );
    ensure!(
        !target.automatic || !repository.configuration.skip(&target.key.remote)?,
        "Automatic fetch remote is now excluded; discover targets again"
    );
    let current = repository.target(&target.key.remote, target.key.branch.as_deref())?;
    ensure!(
        current.key == target.key
            && current.endpoint == target.endpoint
            && current.refspecs == target.refspecs,
        "Automatic fetch endpoint or refspec changed; discover targets again"
    );
    Ok(())
}

fn tracking_refs(path: &Path) -> Result<Vec<u8>> {
    let output = read_output(
        path,
        "automatic fetch tracking refs",
        command(path).args([
            "for-each-ref",
            "--sort=refname",
            "--format=%(refname)%00%(objectname)%00%(symref)",
            "refs/remotes/",
        ]),
    )?;
    ensure!(
        output.status.success(),
        "Cannot read automatic fetch tracking refs"
    );
    Ok(output.stdout)
}

fn valid_oid(value: &str) -> bool {
    matches!(value.len(), 40 | 64) && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

pub(crate) fn authentication_failed(message: &str) -> bool {
    let message = message.to_ascii_lowercase();
    [
        "authentication failed",
        "authentication required",
        "authorization failed",
        "could not read username",
        "could not read password",
        "terminal prompts disabled",
        "permission denied (publickey",
        "permission denied (password",
        "no supported authentication methods",
        "http 401",
        "http 403",
        "error: 401",
        "error: 403",
        "returned error: 401",
        "returned error: 403",
        "invalid credentials",
        "access denied",
    ]
    .iter()
    .any(|pattern| message.contains(pattern))
}

#[cfg(test)]
mod tests;
