//! Compare an immutable revision with the working file and write reviewed
//! aligned blocks back without touching the index.
use super::*;
use crate::{core::revision, merge::RevisionDiff};
use anyhow::{ensure, Context as _, Result};
use std::{
    fs,
    io::Write,
    path::{Component, PathBuf},
};
use tempfile::NamedTempFile;

const LIMIT: usize = revision::LIMIT;

#[derive(Clone, Debug)]
enum Snapshot {
    Missing,
    File {
        bytes: Vec<u8>,
        permissions: fs::Permissions,
    },
}

impl Snapshot {
    fn read(repo: &Path, path: &Path) -> Result<Self> {
        let file = safe_file(repo, path, true)?;
        let metadata = match fs::symlink_metadata(&file) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Self::Missing),
            Err(error) => return Err(error).with_context(|| format!("read {}", file.display())),
        };
        ensure!(
            !metadata.file_type().is_symlink(),
            "Compare with Local does not write through symlinks"
        );
        ensure!(
            metadata.is_file(),
            "Compare with Local requires a regular working file"
        );
        ensure!(
            metadata.len() <= LIMIT as u64,
            "Working file is larger than 512 KiB"
        );
        let bytes = fs::read(&file).with_context(|| format!("read {}", file.display()))?;
        Ok(Self::File {
            bytes,
            permissions: metadata.permissions(),
        })
    }

    fn text(&self) -> Result<String> {
        let Self::File { bytes, .. } = self else {
            return Ok(String::new());
        };
        ensure!(
            !bytes.contains(&0),
            "Working file is binary or non-UTF-8; use an external diff tool"
        );
        String::from_utf8(bytes.clone())
            .context("Working file is binary or non-UTF-8; use an external diff tool")
    }

    fn note(&self) -> String {
        let Self::File { bytes, .. } = self else {
            return "Deleted in working tree".into();
        };
        let text = String::from_utf8_lossy(bytes);
        let ending = if text.is_empty() {
            "Empty file"
        } else if !text.ends_with('\n') {
            "No newline at end of file"
        } else if text.contains("\r\n") {
            "CRLF"
        } else {
            "LF"
        };
        format!("Working tree · {ending}")
    }

    fn same(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Missing, Self::Missing) => true,
            (
                Self::File {
                    bytes: left,
                    permissions: left_permissions,
                },
                Self::File {
                    bytes: right,
                    permissions: right_permissions,
                },
            ) => left == right && mode(left_permissions) == mode(right_permissions),
            _ => false,
        }
    }
}

fn mode(permissions: &fs::Permissions) -> u32 {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        permissions.mode()
    }
    #[cfg(not(unix))]
    {
        u32::from(permissions.readonly())
    }
}

fn set_new_permissions(file: &fs::File) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(fs::Permissions::from_mode(0o644))?;
    }
    #[cfg(not(unix))]
    let _ = file;
    Ok(())
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum LineEndings {
    None,
    Lf,
    Crlf,
    Mixed,
}

fn line_endings(text: &str) -> LineEndings {
    let crlf = text.matches("\r\n").count();
    let lf = text.matches('\n').count() - crlf;
    match (crlf > 0, lf > 0) {
        (false, false) => LineEndings::None,
        (true, false) => LineEndings::Crlf,
        (false, true) => LineEndings::Lf,
        (true, true) => LineEndings::Mixed,
    }
}

/// Keep the revision's content while following the working file's line
/// endings, so a CRLF checkout does not turn every line into a change.
fn match_line_endings(mut text: String, reference: &str) -> Result<String> {
    let source = line_endings(&text);
    let target = line_endings(reference);
    ensure!(
        source != LineEndings::Mixed && target != LineEndings::Mixed,
        "Mixed line endings are not supported by Compare with Local; use an external diff tool"
    );
    match (source, target) {
        (LineEndings::Lf, LineEndings::Crlf) => {
            text = text.replace("\r\n", "\n").replace('\n', "\r\n");
        }
        (LineEndings::Crlf, LineEndings::Lf) => text = text.replace("\r\n", "\n"),
        _ => {}
    }
    Ok(text)
}

fn safe_file(repo: &Path, path: &Path, allow_missing_parent: bool) -> Result<PathBuf> {
    let parts: Vec<_> = path.components().collect();
    ensure!(
        !parts.is_empty()
            && parts
                .iter()
                .all(|part| matches!(part, Component::Normal(_))),
        "Invalid working-tree path"
    );
    ensure!(
        !path.starts_with(".git"),
        "Choose a file outside Git metadata"
    );
    let mut file = repo.to_path_buf();
    for (index, part) in parts.iter().enumerate() {
        file.push(part);
        let last = index + 1 == parts.len();
        match fs::symlink_metadata(&file) {
            Ok(metadata) => {
                ensure!(
                    !metadata.file_type().is_symlink(),
                    "Compare with Local does not write through symlinks"
                );
                if !last {
                    ensure!(metadata.is_dir(), "Working file parent is not a directory");
                    ensure!(
                        fs::symlink_metadata(file.join(".git")).is_err(),
                        "Compare with Local does not enter nested repositories or submodules"
                    );
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                if last {
                    break;
                }
                ensure!(
                    allow_missing_parent,
                    "Working file directory no longer exists"
                );
                break;
            }
            Err(error) => {
                return Err(error).with_context(|| format!("inspect {}", file.display()));
            }
        }
    }
    Ok(file)
}

fn conversion_state(repo: &Path, path: &Path) -> Result<Vec<u8>> {
    let output = crate::core::command(repo)
        .args([
            "check-attr",
            "-z",
            "filter",
            "working-tree-encoding",
            "ident",
            "text",
            "eol",
            "--",
        ])
        .arg(path)
        .output()
        .context("read Git attributes")?;
    ensure!(output.status.success(), "Cannot read Git attributes");
    let fields: Vec<_> = output.stdout.split(|&byte| byte == 0).collect();
    for field in fields.chunks_exact(3) {
        if field.len() < 3 {
            continue;
        }
        let key = std::str::from_utf8(field[1]).context("Git attribute name is not UTF-8")?;
        let value = std::str::from_utf8(field[2]).context("Git attribute value is not UTF-8")?;
        ensure!(
            value == "unspecified"
                || value == "unset"
                || (key == "text" && matches!(value, "auto" | "set"))
                || (key == "eol" && matches!(value, "lf" | "crlf")),
            "Path uses {key}={value}; use its configured Git filter workflow"
        );
    }
    let mut state = output.stdout;
    for key in [
        "core.autocrlf",
        "core.eol",
        "core.safecrlf",
        "core.filemode",
    ] {
        state.push(0);
        let output = crate::core::command(repo)
            .args(["config", "--get", key])
            .output()
            .with_context(|| format!("read Git config {key}"))?;
        ensure!(
            output.status.success() || output.status.code() == Some(1),
            "Cannot read Git configuration"
        );
        state.extend(output.stdout);
    }
    Ok(state)
}

#[derive(Clone)]
pub(crate) struct LocalCompare {
    root: usize,
    repo: PathBuf,
    repo_name: String,
    path: PathBuf,
    revision_id: String,
    revision_text: String,
    revision_note: String,
    revision_present: bool,
    original: Snapshot,
    conversion_state: Vec<u8>,
    changed: bool,
}

impl LocalCompare {
    pub(super) fn prepare(c: &Context, rev: &model::Rev, target: &Target) -> Result<Self> {
        ensure!(
            rev.root == target.root && c.scope.contains(&target.root),
            "The file history repository is outside the current scope"
        );
        let repo = c
            .repos
            .get(target.root)
            .context("File history repository is unavailable")?;
        let revision_path = rev
            .file
            .as_ref()
            .map(|file| model::path_from_bytes(&file.raw))
            .transpose()?
            .map_or_else(|| target.path.clone(), Path::to_path_buf);
        let revision = revision::regular_file(&repo.path, &rev.oid, &revision_path)?;
        let (revision_text, revision_note, revision_present) = revision.map_or_else(
            || (String::new(), "Absent in this revision".into(), false),
            |source| (source.text, source.description, true),
        );
        let conversion_state = conversion_state(&repo.path, &target.path)?;
        let original = Snapshot::read(&repo.path, &target.path)?;
        original.text()?;
        Ok(Self {
            root: target.root,
            repo: repo.path.clone(),
            repo_name: repo.name.clone(),
            path: target.path.clone(),
            revision_id: rev.oid.clone(),
            revision_text,
            revision_note,
            revision_present,
            original,
            conversion_state,
            changed: false,
        })
    }

    pub(super) fn diff(&self) -> Result<DiffView> {
        let local_text = self.original.text()?;
        let revision_text = match_line_endings(self.revision_text.clone(), &local_text)?;
        let name = format!(
            "{} · {}",
            self.repo_name,
            model::plain(self.path.as_os_str().as_encoded_bytes())
        );
        let comparison = RevisionDiff::local(
            name.clone(),
            self.revision_id.clone(),
            revision_text,
            local_text,
            self.revision_note.clone(),
            self.original.note(),
            self.revision_present,
        );
        Ok(DiffView {
            name,
            comparison: Some(comparison),
            local: Some(self.clone()),
            ..Default::default()
        })
    }

    pub(super) fn refresh(&self) -> Result<Self> {
        let original = Snapshot::read(&self.repo, &self.path)?;
        original.text()?;
        Ok(Self {
            original,
            ..self.clone()
        })
    }

    pub(super) fn mark_changed(&mut self) {
        self.changed = true;
    }

    pub(crate) fn changed(&self) -> bool {
        self.changed
    }

    pub(super) fn revision_present(&self) -> bool {
        self.revision_present
    }

    pub(super) fn ensure_current(&self, c: &Context) -> Result<()> {
        ensure!(
            c.scope.contains(&self.root),
            "The file history repository is outside the current scope"
        );
        let repo = c
            .repos
            .get(self.root)
            .context("File history repository is unavailable")?;
        ensure!(
            repo.path == self.repo,
            "The repository changed since Compare with Local opened"
        );
        safe_file(&self.repo, &self.path, false)?;
        Ok(())
    }

    pub(super) fn ensure_matches(
        &self,
        c: &Context,
        rev: &model::Rev,
        target: &Target,
    ) -> Result<()> {
        self.ensure_current(c)?;
        ensure!(
            rev.root == self.root && rev.oid == self.revision_id,
            "The selected revision changed; reopen Compare with Local"
        );
        ensure!(
            target.root == self.root && target.path == self.path,
            "The file history target changed; reopen Compare with Local"
        );
        Ok(())
    }

    pub(super) fn write(&self, text: &str) -> Result<()> {
        ensure!(
            self.conversion_state == conversion_state(&self.repo, &self.path)?,
            "Git attributes/config changed; reopen Compare with Local"
        );
        let file = safe_file(&self.repo, &self.path, false)?;
        let current = Snapshot::read(&self.repo, &self.path)?;
        ensure!(
            self.original.same(&current),
            "Working file changed since Compare with Local opened; reopen the comparison"
        );
        let parent = file
            .parent()
            .context("Working file has no parent directory")?;
        ensure!(parent.is_dir(), "Working file directory no longer exists");
        let mut temporary = NamedTempFile::new_in(parent)?;
        temporary.write_all(text.as_bytes())?;
        match &self.original {
            Snapshot::Missing => {
                set_new_permissions(temporary.as_file())?;
                temporary.as_file().sync_all()?;
                safe_file(&self.repo, &self.path, false)?;
                ensure!(
                    self.original.same(&Snapshot::read(&self.repo, &self.path)?),
                    "Working file changed before it could be created"
                );
                ensure!(
                    self.conversion_state == conversion_state(&self.repo, &self.path)?,
                    "Git attributes/config changed before the file could be created"
                );
                temporary
                    .persist_noclobber(&file)
                    .map_err(|error| error.error)?;
            }
            Snapshot::File { permissions, .. } => {
                temporary.as_file().set_permissions(permissions.clone())?;
                temporary.as_file().sync_all()?;
                safe_file(&self.repo, &self.path, false)?;
                ensure!(
                    self.original.same(&Snapshot::read(&self.repo, &self.path)?),
                    "Working file changed before it could be written"
                );
                ensure!(
                    self.conversion_state == conversion_state(&self.repo, &self.path)?,
                    "Git attributes/config changed before the file could be written"
                );
                temporary.persist(&file).map_err(|error| error.error)?;
            }
        }
        Ok(())
    }
}
