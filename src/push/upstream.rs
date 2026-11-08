//! Track effective branch configuration and publish explicit upstream changes after a push.
use super::*;
use anyhow::Context as _;
use std::{fs, io::Write};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Snapshot {
    values: Vec<Value>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Value {
    key: String,
    origin: Vec<u8>,
    bytes: Option<Vec<u8>>,
}

impl Snapshot {
    pub fn read(path: &Path, branch: &str) -> Result<Self> {
        let output = command(path)
            .args([
                "config",
                "--null",
                "--show-origin",
                "--get-regexp",
                r"^(branch\..*\.(remote|pushremote|merge)|remote\.pushdefault|extensions\.worktreeconfig)$",
            ])
            .output()?;
        if !output.status.success() && output.status.code() != Some(1) {
            bail!(
                "Cannot read branch configuration: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
        let keys = [
            format!("branch.{branch}.remote"),
            format!("branch.{branch}.merge"),
            format!("branch.{branch}.pushremote"),
            "remote.pushdefault".into(),
            "extensions.worktreeconfig".into(),
        ];
        let mut fields = output.stdout.split(|b| *b == 0);
        let mut values = Vec::new();
        while let (Some(origin), Some(item)) = (fields.next(), fields.next()) {
            let (key, bytes) = match item.iter().position(|b| *b == b'\n') {
                Some(i) => (&item[..i], Some(item[i + 1..].to_vec())),
                None => (item, None),
            };
            if let Some(key) = keys.iter().find(|k| k.as_bytes() == key) {
                values.push(Value {
                    key: key.clone(),
                    origin: origin.into(),
                    bytes,
                });
            }
        }
        Ok(Self { values })
    }

    fn last(&self, key: &str) -> String {
        self.values
            .iter()
            .rev()
            .find(|v| v.key == key)
            .map(|v| String::from_utf8_lossy(v.bytes.as_deref().unwrap_or_default()).into_owned())
            .unwrap_or_default()
    }

    pub fn target(&self, repo: &Repo, branch: &str) -> Target {
        let upstream = self.last(&format!("branch.{branch}.remote"));
        let mut remote = self.last(&format!("branch.{branch}.pushremote"));
        if remote.is_empty() {
            remote = self.last("remote.pushdefault");
        }
        if remote.is_empty() {
            remote = upstream.clone();
        }
        if remote.is_empty() {
            let remotes = remote_list(&repo.path);
            remote = if remotes.iter().any(|r| r == "origin") {
                "origin".into()
            } else {
                remotes.first().cloned().unwrap_or_default()
            };
        }
        let dst = if upstream == remote {
            self.last(&format!("branch.{branch}.merge"))
                .strip_prefix("refs/heads/")
                .unwrap_or(branch)
                .to_owned()
        } else {
            branch.into()
        };
        Target { remote, dst }
    }

    pub fn description(&self, branch: &str) -> String {
        let remote = self.last(&format!("branch.{branch}.remote"));
        let merges: Vec<_> = self
            .values
            .iter()
            .filter(|v| v.key == format!("branch.{branch}.merge"))
            .map(|v| String::from_utf8_lossy(v.bytes.as_deref().unwrap_or_default()).into_owned())
            .collect();
        let text = if remote.is_empty() && merges.is_empty() {
            "none".into()
        } else if !remote.is_empty() && merges.len() == 1 {
            format!(
                "{}:{}",
                remote,
                merges[0].strip_prefix("refs/heads/").unwrap_or(&merges[0])
            )
        } else {
            format!("remote={remote}; merge={}", merges.join(", "))
        };
        text.chars()
            .map(|c| if c.is_control() { '�' } else { c })
            .collect()
    }

    pub fn writable_path(&self, path: &Path, branch: &str) -> Result<PathBuf> {
        let worktree = git(path, &["config", "--bool", "extensions.worktreeConfig"])
            .as_deref()
            .ok()
            == Some("true");
        let mut args = vec!["rev-parse", "--path-format=absolute"];
        args.push(if worktree {
            "--absolute-git-dir"
        } else {
            "--git-common-dir"
        });
        let mut bytes = git_bytes(path, &args)?;
        if bytes.last() == Some(&b'\n') {
            bytes.pop();
        }
        let directory = native_path(bytes)?;
        let storage = directory.join(if worktree {
            "config.worktree"
        } else {
            "config"
        });
        let storage = if storage.exists() {
            fs::canonicalize(storage)?
        } else {
            storage
        };
        // Merge is multivalued, and a later include can override remote. Never
        // rewrite included/global files as a side effect of Push or claim that
        // adding local keys replaced an upstream declared outside this file.
        for value in self.values.iter().filter(|v| {
            v.key == format!("branch.{branch}.merge") || v.key == format!("branch.{branch}.remote")
        }) {
            let origin = value.origin.strip_prefix(b"file:")
                .ok_or_else(|| anyhow::anyhow!("Upstream config is inherited; configure it with Git before choosing Set upstream"))?;
            let origin = native_path(origin.into())?;
            let origin = if origin.is_absolute() {
                origin
            } else {
                path.join(origin)
            };
            if fs::canonicalize(origin)? != storage {
                bail!("Upstream config is inherited; configure it with Git before choosing Set upstream");
            }
        }
        Ok(storage)
    }
}

#[cfg(unix)]
fn native_path(bytes: Vec<u8>) -> Result<PathBuf> {
    use std::os::unix::ffi::OsStringExt;
    Ok(std::ffi::OsString::from_vec(bytes).into())
}

#[cfg(not(unix))]
fn native_path(bytes: Vec<u8>) -> Result<PathBuf> {
    Ok(String::from_utf8(bytes)?.into())
}

struct ConfigLock {
    path: PathBuf,
    published: bool,
}

impl Drop for ConfigLock {
    fn drop(&mut self) {
        if !self.published {
            let _ = fs::remove_file(&self.path);
        }
    }
}

fn config_bytes(path: &Path) -> Result<Option<Vec<u8>>> {
    match fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

pub(super) fn set(job: &Job) -> Result<()> {
    job.revalidate()?;
    let config = job.config.writable_path(&job.path, &job.branch)?;
    let mut name = config.as_os_str().to_os_string();
    name.push(".lock");
    let lock_path = PathBuf::from(name);
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&lock_path)
        .context(
            "Cannot lock Git config; configure the upstream with Git after releasing the lock",
        )?;
    let mut lock = ConfigLock {
        path: lock_path,
        published: false,
    };
    let original = config_bytes(&config)?;
    if let Some(bytes) = &original {
        file.write_all(bytes)?;
    }
    if let Ok(metadata) = fs::metadata(&config) {
        file.set_permissions(metadata.permissions())?;
    }
    file.sync_all()?;
    drop(file);
    // Edit only our unpublished lock file. Both keys reach the real config in
    // one rename; errors cannot leave half of an upstream configuration behind.
    for (key, value) in [
        (
            format!("branch.{}.remote", job.branch),
            job.t.remote.clone(),
        ),
        (
            format!("branch.{}.merge", job.branch),
            format!("refs/heads/{}", job.t.dst),
        ),
    ] {
        let output = command(&job.path)
            .args(["config", "--file"])
            .arg(&lock.path)
            .args(["--replace-all", &key, &value])
            .output()?;
        if !output.status.success() {
            bail!(
                "Cannot update upstream config: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
    }
    job.revalidate()?;
    if config_bytes(&config)? != original {
        bail!("Git config changed after push; configure upstream with Git");
    }
    fs::OpenOptions::new()
        .write(true)
        .open(&lock.path)?
        .sync_all()?;
    fs::rename(&lock.path, &config).context("Cannot publish upstream config")?;
    lock.published = true;
    Ok(())
}

#[cfg(test)]
mod tests;
