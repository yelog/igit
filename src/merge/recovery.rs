//! Single-repository reviews for Git's destructive native recovery commands.
use super::engine::{
    checked, conflicts, git_path, native_path, operation, path_label, run, Operation,
};
use anyhow::{ensure, Context as _, Result};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Component, Path, PathBuf},
};

#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RecoveryAction {
    Abort,
    Skip,
}

impl RecoveryAction {
    pub fn flag(self) -> &'static str {
        match self {
            Self::Abort => "--abort",
            Self::Skip => "--skip",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Abort => "Abort",
            Self::Skip => "Skip",
        }
    }

    pub fn available(self, op: &Operation) -> bool {
        match self {
            Self::Abort => matches!(op.name, "merge" | "rebase" | "cherry-pick" | "revert"),
            Self::Skip => {
                matches!(op.name, "rebase" | "cherry-pick" | "revert") && !op.other.is_empty()
            }
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Working {
    Missing,
    File(Vec<u8>, u32),
    Symlink(PathBuf, u32),
    Submodule(u32),
}

fn mode(meta: &fs::Metadata) -> u32 {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        meta.permissions().mode()
    }
    #[cfg(not(unix))]
    {
        u32::from(meta.permissions().readonly())
    }
}

fn working(repo: &Path, path: &Path, submodule: bool) -> Result<Working> {
    let parts: Vec<_> = path.components().collect();
    ensure!(
        !parts.is_empty() && parts.iter().all(|p| matches!(p, Component::Normal(_))),
        "Unsafe working-tree path; use an external Git tool"
    );
    let mut parent = repo.to_path_buf();
    for part in &parts[..parts.len() - 1] {
        parent.push(part);
        match fs::symlink_metadata(&parent) {
            Ok(meta) => ensure!(
                meta.is_dir() && !meta.is_symlink() && !parent.join(".git").exists(),
                "Unsafe parent of {}; use an external Git tool",
                path_label(path)
            ),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
            Err(error) => return Err(error.into()),
        }
    }
    let full = repo.join(path);
    let meta = match fs::symlink_metadata(&full) {
        Ok(meta) => meta,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Working::Missing),
        Err(error) => return Err(error.into()),
    };
    if meta.is_symlink() {
        return Ok(Working::Symlink(fs::read_link(full)?, mode(&meta)));
    }
    if submodule && meta.is_dir() {
        // Recovery commands explicitly disable recursive submodule updates.
        return Ok(Working::Submodule(mode(&meta)));
    }
    ensure!(
        meta.is_file(),
        "{} is a directory or special file; use an external Git tool",
        path_label(path)
    );
    // Content hashes are filled in bounded native-argument batches below.
    Ok(Working::File(Vec::new(), mode(&meta)))
}

fn fingerprint_files(repo: &Path, states: &mut BTreeMap<PathBuf, Working>) -> Result<()> {
    let files: Vec<_> = states
        .iter()
        .filter_map(|(path, state)| matches!(state, Working::File(..)).then_some(path.clone()))
        .collect();
    let mut first = 0;
    while first < files.len() {
        let mut end = first;
        let mut bytes = 0;
        while end < files.len() && end - first < 128 {
            let size = files[end].as_os_str().as_encoded_bytes().len() + 1;
            if end > first && bytes + size > 8192 {
                break;
            }
            bytes += size;
            end += 1;
        }
        let output = crate::core::command(repo)
            .args(["hash-object", "--no-filters", "--"])
            .args(&files[first..end])
            .output()?;
        ensure!(
            output.status.success(),
            "Working files changed or could not be fingerprinted; review again"
        );
        let hashes: Vec<_> = output
            .stdout
            .split(|b| *b == b'\n')
            .filter(|h| !h.is_empty())
            .collect();
        ensure!(
            hashes.len() == end - first,
            "Incomplete working-file fingerprints; review again"
        );
        for (path, hash) in files[first..end].iter().zip(hashes) {
            if let Some(Working::File(value, _)) = states.get_mut(path) {
                *value = hash.to_vec();
            }
        }
        first = end;
    }
    Ok(())
}

#[derive(Debug, PartialEq, Eq)]
enum Metadata {
    Missing,
    File(Vec<u8>, u32),
    Directory(u32),
}

fn metadata(path: &Path, key: PathBuf, out: &mut BTreeMap<PathBuf, Metadata>) -> Result<()> {
    let meta = match fs::symlink_metadata(path) {
        Ok(meta) => meta,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            out.insert(key, Metadata::Missing);
            return Ok(());
        }
        Err(error) => return Err(error.into()),
    };
    ensure!(
        !meta.is_symlink(),
        "Operation metadata contains a symlink; use an external Git tool"
    );
    if meta.is_dir() {
        out.insert(key.clone(), Metadata::Directory(mode(&meta)));
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            metadata(&entry.path(), key.join(entry.file_name()), out)?;
        }
    } else {
        ensure!(
            meta.is_file(),
            "Unsupported operation metadata; use an external Git tool"
        );
        out.insert(key, Metadata::File(fs::read(path)?, mode(&meta)));
    }
    Ok(())
}

#[derive(PartialEq, Eq)]
struct State {
    repo: PathBuf,
    git_dir: PathBuf,
    #[cfg(unix)]
    directory_identity: [(u64, u64); 2],
    op: Operation,
    branch: Vec<u8>,
    refs: Vec<u8>,
    index: Vec<u8>,
    entries: Vec<u8>,
    metadata: BTreeMap<PathBuf, Metadata>,
    working: BTreeMap<PathBuf, Working>,
    config: Vec<u8>,
}

impl std::fmt::Debug for State {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Effective configuration can contain credentials. Keep snapshots out
        // of diagnostics, including assertion failures in repository tests.
        f.debug_struct("State")
            .field("repo", &self.repo)
            .field("op", &self.op)
            .finish_non_exhaustive()
    }
}

impl State {
    fn read(repo: &Path) -> Result<Self> {
        let repo = fs::canonicalize(repo)?;
        let git_dir = fs::canonicalize(git_path(&repo, ".")?)?;
        let op =
            operation(&repo)?.context("No native merge/rebase/cherry-pick/revert operation")?;
        let branch = run(&repo, &["symbolic-ref", "-q", "HEAD"])?;
        ensure!(
            branch.status.success() || branch.status.code() == Some(1),
            "Cannot read HEAD identity"
        );
        let index = fs::read(git_path(&repo, "index")?)?;
        let entries = checked(&repo, &["ls-files", "--stage", "-z"])?;
        let mut submodules = BTreeSet::new();
        let mut paths = BTreeSet::new();
        for record in entries.split(|b| *b == 0).filter(|r| !r.is_empty()) {
            let tab = record
                .iter()
                .position(|b| *b == b'\t')
                .context("Invalid index record")?;
            let path = native_path(&record[tab + 1..])?;
            if record.starts_with(b"160000 ") {
                submodules.insert(path.clone());
            }
            paths.insert(path);
        }
        // Include ignored and untracked files: a native reset can replace an
        // obstructing leaf even when ordinary `git status` does not list it.
        for path in checked(&repo, &["ls-files", "--others", "-z"])?
            .split(|b| *b == 0)
            .filter(|p| !p.is_empty())
        {
            paths.insert(native_path(path)?);
        }
        let mut working = paths
            .into_iter()
            .map(|path| {
                let state = working(&repo, &path, submodules.contains(&path))?;
                Ok((path, state))
            })
            .collect::<Result<_>>()?;
        fingerprint_files(&repo, &mut working)?;
        let mut operation_metadata = BTreeMap::new();
        for name in [
            "HEAD",
            "ORIG_HEAD",
            "MERGE_HEAD",
            "MERGE_MSG",
            "MERGE_MODE",
            "MERGE_AUTOSTASH",
            "AUTO_MERGE",
            "CHERRY_PICK_HEAD",
            "REVERT_HEAD",
            "REBASE_HEAD",
            "sequencer",
            "rebase-merge",
            "rebase-apply",
        ] {
            metadata(
                &git_path(&repo, name)?,
                name.into(),
                &mut operation_metadata,
            )?;
        }
        #[cfg(unix)]
        let directory_identity = {
            use std::os::unix::fs::MetadataExt;
            let repo_meta = fs::metadata(&repo)?;
            let git_meta = fs::metadata(&git_dir)?;
            [
                (repo_meta.dev(), repo_meta.ino()),
                (git_meta.dev(), git_meta.ino()),
            ]
        };
        let config = checked(&repo, &["config", "--null", "--list"])?;
        // Rebase detaches HEAD. Its original branch can move independently,
        // and --abort must not overwrite that later work under an old review.
        let refs = checked(
            &repo,
            &[
                "for-each-ref",
                "--format=%(refname)%00%(objectname)%00%(symref)",
            ],
        )?;
        Ok(Self {
            repo,
            git_dir,
            #[cfg(unix)]
            directory_identity,
            op,
            branch: branch.stdout,
            refs,
            index,
            entries,
            metadata: operation_metadata,
            working,
            config,
        })
    }
}

pub(super) struct RecoveryPlan {
    pub action: RecoveryAction,
    state: State,
}

impl RecoveryPlan {
    pub fn review(repo: &Path, action: RecoveryAction) -> Result<(Self, String)> {
        let state = State::read(repo)?;
        ensure!(
            action.available(&state.op),
            "{} is unavailable for this {} operation",
            action.label(),
            state.op.name
        );
        let mut body = format!(
            "Repository: {}\nOperation: git {} {}\nReviewed HEAD: {}\n\n{}\nGit may discard current resolutions, staged changes, and working-tree changes in this repository. Untracked paths obstructing restoration may also be replaced. Other repositories are outside this review.\n\n",
            path_label(&state.repo), state.op.name, action.flag(), state.op.head,
            match action {
                RecoveryAction::Abort => "Abort the entire operation and restore its original branch/HEAD using Git's native recovery.",
                RecoveryAction::Skip => "Discard the current replayed commit and continue the remaining sequence. Later commits can stop with new conflicts.",
            },
        );
        let changed = checked(
            repo,
            &[
                "diff",
                "--no-ext-diff",
                "--no-textconv",
                "--name-only",
                "-z",
                "HEAD",
                "--",
            ],
        )?;
        let changed = changed
            .split(|b| *b == 0)
            .filter(|p| !p.is_empty())
            .map(native_path)
            .collect::<Result<BTreeSet<_>>>()?;
        body.push_str("Current changed paths:\n");
        if changed.is_empty() {
            body.push_str("  (none)\n");
        }
        for path in changed {
            body.push_str(&format!("  {}\n", path_label(&path)));
        }
        ensure!(
            State::read(repo)? == state,
            "Repository changed while preparing recovery; review again"
        );
        Ok((Self { action, state }, body))
    }

    pub fn operation_name(&self) -> &'static str {
        self.state.op.name
    }

    pub fn execute(self, repo: &Path) -> Result<String> {
        ensure!(
            State::read(repo)? == self.state,
            "Repository, operation, HEAD, index, or working files changed; review again"
        );
        let output = crate::core::command(repo)
            .args([
                "-c",
                "submodule.recurse=false",
                self.state.op.name,
                self.action.flag(),
            ])
            .output()?;
        let details = [output.stdout, output.stderr]
            .into_iter()
            .map(|bytes| String::from_utf8_lossy(&bytes).trim().to_owned())
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("\n");
        let remaining = operation(repo)?;
        let count = conflicts(0, repo)?.len();
        let status = match remaining {
            Some(op) => format!("{} still pending; {count} unresolved paths. Review the current operation separately", op.name),
            None if count > 0 => format!("Operation ended; {count} unresolved paths remain. Resolve and review in Commit"),
            None => "Operation ended".into(),
        };
        let result = format!(
            "git {} {}: {}. {status}{}",
            self.state.op.name,
            self.action.flag(),
            if output.status.success() {
                "completed"
            } else {
                "stopped"
            },
            if details.is_empty() {
                String::new()
            } else {
                format!("\n{details}")
            }
        );
        // A failed native command may already have advanced the sequence or
        // restored an autostash. Always report its surviving native state.
        Ok(result)
    }
}
