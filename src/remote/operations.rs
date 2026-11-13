//! Reviewed remote configuration changes for one repository in Roots.
use super::model::{self, ConfigEntry, ConfigSnapshot, Remote};
use crate::core::{command, git_path, revpath, Context, Repo};
use anyhow::{bail, ensure, Context as _, Result};
use std::{
    collections::BTreeSet,
    ffi::OsString,
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::Output,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Operation {
    Add(Remote),
    Edit { before: Remote, after: Remote },
    Remove(Remote),
}

impl Operation {
    pub(super) fn title(&self) -> &'static str {
        match self {
            Self::Add(_) => "Add remote",
            Self::Edit { before, after } if before.name != after.name => "Rename remote",
            Self::Edit { .. } => "Edit remote",
            Self::Remove(_) => "Remove remote",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Identity {
    worktree: PathBuf,
    git_dir: PathBuf,
    common_dir: PathBuf,
    config: PathBuf,
}

impl Identity {
    fn read(repo: &Repo) -> Result<Self> {
        ensure!(
            revpath(&repo.path, "--show-toplevel").as_deref() == Some(repo.path.as_path()),
            "Worktree unavailable; rescan the workspace"
        );
        Self::read_path(&repo.path)
    }

    fn read_path(path: &Path) -> Result<Self> {
        ensure!(
            revpath(path, "--show-toplevel").as_deref() == Some(path),
            "Worktree unavailable; rescan the workspace"
        );
        let git_dir = revpath(path, "--absolute-git-dir").context("Git directory unavailable")?;
        let common_dir = absolute_git_path(
            path,
            &["rev-parse", "--path-format=absolute", "--git-common-dir"],
        )
        .context("Git directory unavailable")?;
        let config = fs::canonicalize(common_dir.join("config"))
            .context("Git configuration is unavailable")?;
        ensure!(config.is_file(), "Git configuration is unavailable");
        Ok(Self {
            worktree: path.to_path_buf(),
            git_dir,
            common_dir,
            config,
        })
    }

    fn validate(&self) -> Result<()> {
        let current = Self::read_path(&self.worktree)?;
        ensure!(
            current.worktree == self.worktree
                && current.git_dir == self.git_dir
                && current.common_dir == self.common_dir
                && current.config == self.config,
            "Repository or Git directory changed; refresh and review again"
        );
        Ok(())
    }

    fn validate_selected(&self, repo: &Path) -> Result<()> {
        ensure!(
            self.worktree.as_path() == repo,
            "Repository changed; refresh and review again"
        );
        self.validate()
    }

    fn is_local_origin(&self, origin: &[u8], repo: &Path) -> bool {
        let Some(path) = origin.strip_prefix(b"file:") else {
            return false;
        };
        let Ok(path) = native_path(path) else {
            return false;
        };
        let path = if path.is_absolute() {
            path
        } else {
            repo.join(path)
        };
        fs::canonicalize(path)
            .ok()
            .is_some_and(|path| path == self.config)
    }
}

pub(super) struct Review {
    root: usize,
    repository: String,
    path: PathBuf,
    identity: Identity,
    config: ConfigSnapshot,
    refs: Vec<u8>,
    operation: Operation,
}

impl Review {
    pub(super) fn root(&self) -> usize {
        self.root
    }

    pub(super) fn prepare(c: &Context, root: usize, operation: Operation) -> Result<Self> {
        ensure!(c.scope.contains(&root), "Repository is outside Roots");
        let repo = c.repos.get(root).context("Repository is unavailable")?;
        let identity = Identity::read(repo)?;
        let config = ConfigSnapshot::read(&repo.path)?;
        validate_operation(&repo.path, &config, &identity, &operation)?;
        let refs = read_refs(&repo.path)?;
        Ok(Self {
            root,
            repository: repo.name.clone(),
            path: repo.path.clone(),
            identity,
            config,
            refs,
            operation,
        })
    }

    pub(super) fn title(&self) -> &'static str {
        self.operation.title()
    }

    pub(super) fn body(&self) -> String {
        let repository = model::label(&self.repository);
        let path = model::label(&self.path.to_string_lossy());
        let mut body = match &self.operation {
            Operation::Add(after) => format!(
                "Repository: {repository}\nPath: {path}\n\nAdd remote: {}\n\nFetch URLs:\n{}\nPush URLs:\n{}\n\nThis changes local Git configuration only. No network request will be made.",
                model::label(&after.name),
                endpoints(&after.urls, "(none configured)"),
                endpoints(&after.push_urls, "(inherit fetch URLs)")
            ),
            Operation::Edit { before, after } if before.name != after.name => {
                let mut body = format!(
                    "Repository: {repository}\nPath: {path}\n\nRename remote: {} to {}\n\nFetch URLs before:\n{}\nFetch URLs after:\n{}\nPush URLs before:\n{}\nPush URLs after:\n{}",
                    model::label(&before.name),
                    model::label(&after.name),
                    endpoints(&before.urls, "(none configured)"),
                    endpoints(&after.urls, "(none configured)"),
                    endpoints(&before.push_urls, "(inherit fetch URLs)"),
                    endpoints(&after.push_urls, "(inherit fetch URLs)")
                );
                if !before.push_urls.is_empty() && after.push_urls.is_empty() {
                    body.push_str(
                        "\n\nPush URLs are cleared; pushes will inherit the fetch URLs.",
                    );
                }
                body.push_str(
                    "\n\nGit also renames local remote-tracking refs and updates branch remote/upstream references. Other remote settings and custom fetch refspecs are preserved.",
                );
                body
            }
            Operation::Edit { before, after } => {
                let mut body = format!(
                    "Repository: {repository}\nPath: {path}\n\nEdit remote: {}\n\nFetch URLs before:\n{}\nFetch URLs after:\n{}\nPush URLs before:\n{}\nPush URLs after:\n{}",
                    model::label(&after.name),
                    endpoints(&before.urls, "(none configured)"),
                    endpoints(&after.urls, "(none configured)"),
                    endpoints(&before.push_urls, "(inherit fetch URLs)"),
                    endpoints(&after.push_urls, "(inherit fetch URLs)")
                );
                if !before.push_urls.is_empty() && after.push_urls.is_empty() {
                    body.push_str(
                        "\n\nPush URLs are cleared; pushes will inherit the fetch URLs.",
                    );
                }
                body.push_str(
                    "\n\nOnly the fetch and push endpoint lists change. Custom fetch refspecs, other remote settings, branches, and upstream configuration are preserved. No network request will be made.",
                );
                body
            }
            Operation::Remove(before) => format!(
                "Repository: {repository}\nPath: {path}\n\nRemove remote: {}\n\nCurrent fetch URLs:\n{}\nCurrent push URLs:\n{}\n\nThis removes the local remote configuration and local remote-tracking refs that Git maps to this remote. Branch remote and upstream references to it are removed. The server repository is not deleted. No network request will be made.",
                model::label(&before.name),
                endpoints(&before.urls, "(none configured)"),
                endpoints(&before.push_urls, "(inherit fetch URLs)")
            ),
        };
        body.push_str("\n\nRemote configuration is shared by linked worktrees of this repository.");
        body
    }

    pub(super) fn destination(&self) -> (usize, Option<String>) {
        let name = match &self.operation {
            Operation::Add(after) | Operation::Edit { after, .. } => Some(after.name.clone()),
            Operation::Remove(_) => None,
        };
        (self.root, name)
    }

    pub(super) fn execute(self, c: &Context) -> Result<String> {
        let repo = self.revalidate(c)?;
        match &self.operation {
            Operation::Add(after) => self.add(repo, after),
            Operation::Edit { before, after } => self.edit(repo, before, after),
            Operation::Remove(before) => self.remove(repo, before),
        }
    }

    fn revalidate<'a>(&self, c: &'a Context) -> Result<&'a Repo> {
        ensure!(
            c.scope.contains(&self.root),
            "Repository is outside Roots; refresh and review again"
        );
        let repo = c
            .repos
            .get(self.root)
            .filter(|repo| repo.path == self.path)
            .context("Repository changed; refresh and review again")?;
        self.identity.validate()?;
        ensure!(
            ConfigSnapshot::read(&repo.path)? == self.config,
            "Git configuration changed; refresh and review again"
        );
        ensure!(
            read_refs(&repo.path)? == self.refs,
            "Remote-tracking refs changed; refresh and review again"
        );
        validate_operation(&repo.path, &self.config, &self.identity, &self.operation)?;
        Ok(repo)
    }

    fn add(&self, repo: &Repo, after: &Remote) -> Result<String> {
        let output = command(&repo.path)
            .args(["remote", "add", "--", &after.name, &after.urls[0]])
            .output()
            .context("Cannot start Git")?;
        if !output.status.success() {
            match ConfigSnapshot::read(&repo.path) {
                Ok(config) if config.has_remote(&after.name) => {
                    bail!(
                        "Partial result: Git reported an error while adding remote {} in {}, but the remote now exists. Refresh Remote before retrying; do not add it again. {}",
                        model::label(&after.name),
                        model::label(&self.repository),
                        detail(&output)
                    )
                }
                Err(error) => {
                    bail!(
                        "Partial result: adding remote {} in {} failed and Git state could not be verified. Refresh Remote before retrying; do not add it again. {error:#}",
                        model::label(&after.name),
                        model::label(&self.repository)
                    )
                }
                Ok(_) => (),
            }
            bail!(
                "Add remote failed in {}: {}",
                model::label(&self.repository),
                detail(&output)
            );
        }
        let current = match ConfigSnapshot::read(&repo.path) {
            Ok(current) => current,
            Err(error) => {
                bail!(
                    "Partial result: remote {} may have been added in {}, but its configuration could not be verified. Refresh Remote before retrying; do not add it again. {error:#}",
                    model::label(&after.name),
                    model::label(&self.repository)
                )
            }
        };
        let added = match current.remote(&after.name) {
            Some(added) => added,
            None => {
                bail!(
                    "Partial result: Git reported success adding remote {} in {}, but it is not configured. Refresh Remote before retrying. ",
                    model::label(&after.name),
                    model::label(&self.repository)
                )
            }
        };
        ensure!(
            added.urls == vec![after.urls[0].clone()] && added.push_urls.is_empty(),
            "Partial result: remote {} was added in {}, but its endpoint configuration could not be verified. Refresh Remote before retrying; do not add it again.",
            model::label(&after.name),
            model::label(&self.repository)
        );
        let current_refs = match read_refs(&repo.path) {
            Ok(refs) => refs,
            Err(error) => {
                bail!(
                    "Partial result: remote {} was added in {}, but remote-tracking refs could not be verified. Refresh Remote before retrying; do not add it again. {error:#}",
                    model::label(&after.name),
                    model::label(&self.repository)
                )
            }
        };
        ensure!(
            current_refs == self.refs,
            "Partial result: remote {} was added in {}, but remote-tracking refs changed during the operation. Refresh Remote before retrying; do not add it again.",
            model::label(&after.name),
            model::label(&self.repository)
        );
        ensure!(
            unrelated(&current, &after.name) == unrelated(&self.config, &after.name),
            "Partial result: remote {} was added in {}, but unrelated Git configuration changed. Refresh Remote before retrying; do not add it again.",
            model::label(&after.name),
            model::label(&self.repository)
        );
        if after.urls.len() > 1 || !after.push_urls.is_empty() {
            if let Err(error) =
                self.update_endpoints(&repo.path, &self.identity, &current, &current_refs, after)
            {
                bail!(
                    "Partial result: remote {} was added in {}, but updating its endpoint lists failed. Refresh Remote before retrying; do not add it again. {error:#}",
                    model::label(&after.name),
                    model::label(&self.repository)
                );
            }
        }
        Ok(format!(
            "Added remote {} in {}",
            model::label(&after.name),
            model::label(&self.repository)
        ))
    }

    fn edit(&self, repo: &Repo, before: &Remote, after: &Remote) -> Result<String> {
        if before.name == after.name {
            if let Err(error) =
                self.update_endpoints(&repo.path, &self.identity, &self.config, &self.refs, after)
            {
                bail!(
                    "Edit remote failed or could not be verified in {}: {error:#}. Refresh Remote before retrying.",
                    model::label(&self.repository)
                );
            }
            return Ok(format!(
                "Updated remote {} in {}",
                model::label(&after.name),
                model::label(&self.repository)
            ));
        }

        let output = command(&repo.path)
            .args(["remote", "rename", "--", &before.name, &after.name])
            .output()
            .context("Cannot start Git")?;
        if !output.status.success() {
            let changed = ConfigSnapshot::read(&repo.path)
                .map(|config| config != self.config)
                .unwrap_or(true);
            if changed {
                bail!(
                    "Partial result: renaming remote {} to {} in {} may have changed Git state. Refresh Remote before retrying; do not repeat the rename. {}",
                    model::label(&before.name),
                    model::label(&after.name),
                    model::label(&self.repository),
                    detail(&output)
                );
            }
            bail!(
                "Rename remote failed in {}: {}",
                model::label(&self.repository),
                detail(&output)
            );
        }

        let current = match ConfigSnapshot::read(&repo.path) {
            Ok(current) => current,
            Err(error) => {
                bail!(
                    "Partial result: remote {} was renamed to {} in {}, but the new configuration could not be verified. Refresh Remote before retrying; do not repeat the rename. {error:#}",
                    model::label(&before.name),
                    model::label(&after.name),
                    model::label(&self.repository)
                )
            }
        };
        let current_refs = match read_refs(&repo.path) {
            Ok(refs) => refs,
            Err(error) => {
                bail!(
                    "Partial result: remote {} was renamed to {} in {}, but remote-tracking refs could not be verified. Refresh Remote before retrying; do not repeat the rename. {error:#}",
                    model::label(&before.name),
                    model::label(&after.name),
                    model::label(&self.repository)
                )
            }
        };
        let renamed = match current.remote(&after.name) {
            Some(renamed) if !current.has_remote(&before.name) => renamed,
            _ => {
                bail!(
                    "Partial result: renaming remote {} to {} in {} did not produce the expected configuration. Refresh Remote before retrying; do not repeat the rename.",
                    model::label(&before.name),
                    model::label(&after.name),
                    model::label(&self.repository)
                )
            }
        };
        ensure!(
            renamed.urls == before.urls && renamed.push_urls == before.push_urls,
            "Partial result: remote {} was renamed to {} in {}, but its endpoint lists changed unexpectedly. Refresh Remote before retrying; do not repeat the rename.",
            model::label(&before.name),
            model::label(&after.name),
            model::label(&self.repository)
        );
        if before.urls != after.urls || before.push_urls != after.push_urls {
            if let Err(error) =
                self.update_endpoints(&repo.path, &self.identity, &current, &current_refs, after)
            {
                bail!(
                    "Partial result: remote {} was renamed to {} in {}, but updating its endpoint lists failed. Refresh Remote before retrying; do not repeat the rename. {error:#}",
                    model::label(&before.name),
                    model::label(&after.name),
                    model::label(&self.repository)
                );
            }
        }
        Ok(format!(
            "Renamed remote {} to {} in {}",
            model::label(&before.name),
            model::label(&after.name),
            model::label(&self.repository)
        ))
    }

    fn remove(&self, repo: &Repo, before: &Remote) -> Result<String> {
        let output = command(&repo.path)
            .args(["remote", "remove", "--", &before.name])
            .output()
            .context("Cannot start Git")?;
        if !output.status.success() {
            let config_changed = ConfigSnapshot::read(&repo.path)
                .map(|config| config != self.config)
                .unwrap_or(true);
            let refs_changed = read_refs(&repo.path)
                .map(|refs| refs != self.refs)
                .unwrap_or(true);
            if config_changed || refs_changed {
                bail!(
                    "Partial result: removing remote {} in {} may have changed local configuration or refs. Refresh Remote before retrying; do not repeat the removal. {}",
                    model::label(&before.name),
                    model::label(&self.repository),
                    detail(&output)
                );
            }
            bail!(
                "Remove remote failed in {}: {}",
                model::label(&self.repository),
                detail(&output)
            );
        }
        Ok(format!(
            "Removed remote {} in {}",
            model::label(&before.name),
            model::label(&self.repository)
        ))
    }

    fn update_endpoints(
        &self,
        repo: &Path,
        identity: &Identity,
        expected: &ConfigSnapshot,
        expected_refs: &[u8],
        remote: &Remote,
    ) -> Result<()> {
        identity.validate_selected(repo)?;
        ensure!(
            ConfigSnapshot::read(repo)? == *expected,
            "Git configuration changed; refresh and review again"
        );
        ensure!(
            read_refs(repo)? == expected_refs,
            "Remote-tracking refs changed; refresh and review again"
        );

        let config_path = identity.config.clone();
        let mut lock_name = config_path.as_os_str().to_os_string();
        lock_name.push(".lock");
        let lock_path = PathBuf::from(lock_name);
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&lock_path)
            .context("Cannot lock Git configuration; refresh Remote and try again")?;
        let mut lock = ConfigLock {
            path: lock_path.clone(),
            published: false,
        };
        let original = fs::read(&config_path).context("Cannot read Git configuration")?;
        let permissions = fs::metadata(&config_path)?.permissions();
        file.write_all(&original)?;
        file.set_permissions(permissions)?;
        file.sync_all()?;
        drop(file);

        for (key, values) in [
            (
                format!("remote.{}.url", remote.name),
                remote.urls.as_slice(),
            ),
            (
                format!("remote.{}.pushurl", remote.name),
                remote.push_urls.as_slice(),
            ),
        ] {
            set_values(repo, &lock_path, &key, values, &expected.values(&key))?;
        }

        identity.validate()?;
        ensure!(
            ConfigSnapshot::read(repo)? == *expected,
            "Git configuration changed; refresh and review again"
        );
        ensure!(
            fs::read(&config_path)? == original,
            "Git configuration changed; refresh and review again"
        );
        fs::OpenOptions::new()
            .write(true)
            .open(&lock_path)?
            .sync_all()?;
        fs::rename(&lock_path, &config_path).context("Cannot publish Git configuration")?;
        lock.published = true;

        let current = ConfigSnapshot::read(repo)?;
        let actual = current
            .remote(&remote.name)
            .context("Remote configuration could not be verified; refresh Remote")?;
        ensure!(
            actual.urls == remote.urls && actual.push_urls == remote.push_urls,
            "Remote endpoints could not be verified; refresh Remote"
        );
        Ok(())
    }
}

fn validate_operation(
    repo: &Path,
    config: &ConfigSnapshot,
    identity: &Identity,
    operation: &Operation,
) -> Result<()> {
    match operation {
        Operation::Add(after) => {
            validate_name(repo, &after.name)?;
            validate_endpoints(after)?;
            ensure!(
                !config.has_remote(&after.name),
                "Remote already exists: {}",
                model::label(&after.name)
            );
        }
        Operation::Edit { before, after } => {
            validate_name(repo, &before.name)?;
            validate_name(repo, &after.name)?;
            validate_endpoints(after)?;
            let current = config
                .remote(&before.name)
                .context("Remote configuration changed; refresh and review again")?;
            ensure!(
                &current == before,
                "Remote configuration changed; refresh and review again"
            );
            ensure!(before != after, "No remote changes to review");
            if before.name != after.name {
                ensure!(
                    !config.has_remote(&after.name),
                    "Remote already exists: {}",
                    model::label(&after.name)
                );
            }
            ensure_local_remote(repo, config, identity, &before.name)?;
            if before.name != after.name {
                ensure_local_branch_side_effects(repo, config, identity, &before.name)?;
            }
        }
        Operation::Remove(before) => {
            validate_name(repo, &before.name)?;
            let current = config
                .remote(&before.name)
                .context("Remote configuration changed; refresh and review again")?;
            ensure!(
                &current == before,
                "Remote configuration changed; refresh and review again"
            );
            ensure_local_remote(repo, config, identity, &before.name)?;
            ensure_local_branch_side_effects(repo, config, identity, &before.name)?;
        }
    }
    Ok(())
}

fn validate_name(repo: &Path, name: &str) -> Result<()> {
    ensure!(!name.is_empty(), "Remote name cannot be empty");
    ensure!(
        !name.starts_with('-'),
        "Remote names cannot start with an option"
    );
    ensure!(
        !name.chars().any(char::is_control),
        "Remote names cannot contain control characters"
    );
    let reference = format!("refs/remotes/{name}/igit-check");
    let output = command(repo)
        .args(["check-ref-format", &reference])
        .output()
        .context("Cannot validate the remote name")?;
    ensure!(
        output.status.success(),
        "Enter a valid remote name: {}",
        model::label(name)
    );
    Ok(())
}

fn validate_endpoints(remote: &Remote) -> Result<()> {
    ensure!(
        !remote.urls.is_empty(),
        "At least one fetch URL is required for remote {}",
        model::label(&remote.name)
    );
    for url in &remote.urls {
        ensure!(
            !url.is_empty(),
            "Fetch URL lines cannot be empty for remote {}",
            model::label(&remote.name)
        );
        ensure!(
            !url.chars().any(char::is_control),
            "Fetch URLs cannot contain control characters for remote {}",
            model::label(&remote.name)
        );
    }
    for url in &remote.push_urls {
        ensure!(
            !url.is_empty(),
            "Push URL lines cannot be empty for remote {}. Clear the Push URLs field to inherit fetch URLs.",
            model::label(&remote.name)
        );
        ensure!(
            !url.chars().any(char::is_control),
            "Push URLs cannot contain control characters for remote {}",
            model::label(&remote.name)
        );
    }
    Ok(())
}

fn ensure_local_remote(
    repo: &Path,
    config: &ConfigSnapshot,
    identity: &Identity,
    name: &str,
) -> Result<()> {
    let entries = remote_entries(config, name);
    ensure!(
        !entries.is_empty(),
        "Remote configuration changed; refresh and review again"
    );
    for entry in entries {
        ensure!(
            identity.is_local_origin(&entry.origin, repo),
            "Remote {} uses inherited or external Git configuration ({}); edit it with Git before changing this remote here",
            model::label(name),
            origin_label(&entry.origin)
        );
    }
    Ok(())
}

fn ensure_local_branch_side_effects(
    repo: &Path,
    config: &ConfigSnapshot,
    identity: &Identity,
    name: &str,
) -> Result<()> {
    let mut branches = BTreeSet::new();
    for entry in &config.entries {
        if entry.key.eq_ignore_ascii_case("remote.pushdefault")
            && entry.value.as_deref() == Some(name.as_bytes())
        {
            ensure!(
                identity.is_local_origin(&entry.origin, repo),
                "Remote {} has inherited push configuration ({}); edit it with Git before changing this remote here",
                model::label(name),
                origin_label(&entry.origin)
            );
        }
        let Some((branch, variable)) = branch_parts(&entry.key) else {
            continue;
        };
        if (variable.eq_ignore_ascii_case("remote") || variable.eq_ignore_ascii_case("pushremote"))
            && entry.value.as_deref() == Some(name.as_bytes())
        {
            branches.insert(branch.to_owned());
        }
    }
    for branch in branches {
        for entry in &config.entries {
            let Some((candidate, _)) = branch_parts(&entry.key) else {
                continue;
            };
            if candidate == branch {
                ensure!(
                    identity.is_local_origin(&entry.origin, repo),
                    "Remote {} has inherited branch or upstream configuration ({}); edit it with Git before changing this remote here",
                    model::label(name),
                    origin_label(&entry.origin)
                );
            }
        }
    }
    Ok(())
}

fn branch_parts(key: &str) -> Option<(&str, &str)> {
    let rest = key.strip_prefix("branch.")?;
    let (branch, variable) = rest.rsplit_once('.')?;
    (!branch.is_empty()).then_some((branch, variable))
}

fn remote_entries<'a>(config: &'a ConfigSnapshot, name: &str) -> Vec<&'a ConfigEntry> {
    config
        .entries
        .iter()
        .filter(|entry| {
            model::remote_parts(&entry.key).is_some_and(|(candidate, _)| candidate == name)
        })
        .collect()
}

fn unrelated(config: &ConfigSnapshot, remote: &str) -> Vec<ConfigEntry> {
    config
        .entries
        .iter()
        .filter(|entry| model::remote_parts(&entry.key).is_none_or(|(name, _)| name != remote))
        .cloned()
        .collect()
}

fn origin_label(origin: &[u8]) -> String {
    model::label(&String::from_utf8_lossy(origin))
}

fn endpoints(urls: &[String], empty: &str) -> String {
    if urls.is_empty() {
        return format!("  {empty}");
    }
    urls.iter()
        .map(|url| {
            if url.is_empty() {
                "  (empty value)".into()
            } else {
                format!("  {}", model::label(url))
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn set_values(
    repo: &Path,
    config: &Path,
    key: &str,
    values: &[String],
    previous: &[&[u8]],
) -> Result<()> {
    if values.is_empty() {
        if previous.is_empty() {
            return Ok(());
        }
        let output = command(repo)
            .args(["config", "--file"])
            .arg(git_path(config))
            .args(["--unset-all", "--", key])
            .output()
            .context("Cannot start Git")?;
        if !output.status.success() && output.status.code() != Some(5) {
            bail!("{}", detail(&output));
        }
        return Ok(());
    }

    let output = command(repo)
        .args(["config", "--file"])
        .arg(git_path(config))
        .args(["--replace-all", "--", key, &values[0]])
        .output()
        .context("Cannot start Git")?;
    if !output.status.success() {
        bail!("{}", detail(&output));
    }
    for value in &values[1..] {
        let output = command(repo)
            .args(["config", "--file"])
            .arg(git_path(config))
            .args(["--add", "--", key, value])
            .output()
            .context("Cannot start Git")?;
        if !output.status.success() {
            bail!("{}", detail(&output));
        }
    }
    Ok(())
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

fn read_refs(repo: &Path) -> Result<Vec<u8>> {
    let output = command(repo)
        .args([
            "for-each-ref",
            "--format=%(refname)%00%(objectname)",
            "refs/remotes/",
        ])
        .output()
        .context("Cannot read remote-tracking refs")?;
    if !output.status.success() {
        bail!("{}", detail(&output));
    }
    Ok(output.stdout)
}

fn absolute_git_path(repo: &Path, args: &[&str]) -> Result<PathBuf> {
    let output = command(repo)
        .args(args)
        .output()
        .context("Cannot read the Git directory")?;
    if !output.status.success() {
        bail!("{}", detail(&output));
    }
    let mut end = output.stdout.len();
    while end > 0 && matches!(output.stdout[end - 1], b'\n' | b'\r') {
        end -= 1;
    }
    ensure!(end > 0, "Git returned an empty repository directory path");
    let path = native_path(&output.stdout[..end])?;
    fs::canonicalize(path).context("Git directory is unavailable")
}

fn detail(output: &Output) -> String {
    let text = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );
    if text.trim().is_empty() {
        format!("Git exited with {}", output.status)
    } else {
        model::label(text.trim())
    }
}

#[cfg(unix)]
fn native_path(bytes: &[u8]) -> Result<PathBuf> {
    use std::os::unix::ffi::OsStringExt;
    Ok(OsString::from_vec(bytes.to_vec()).into())
}

#[cfg(not(unix))]
fn native_path(bytes: &[u8]) -> Result<PathBuf> {
    String::from_utf8(bytes.to_vec())
        .map(PathBuf::from)
        .map_err(Into::into)
}

#[cfg(test)]
mod tests;
