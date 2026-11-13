//! Compare an immutable revision with the working file and write reviewed
//! aligned blocks back without touching the index.
use super::*;
use crate::{core::revision, merge::RevisionDiff};
use anyhow::{ensure, Context as _, Result};
use std::{
    fs,
    io::{Read, Write},
    path::{Component, PathBuf},
};
use tempfile::NamedTempFile;

const LIMIT: usize = revision::LIMIT;

#[derive(Clone, PartialEq, Eq)]
struct RepositoryIdentity {
    canonical: PathBuf,
    created: Option<std::time::SystemTime>,
    #[cfg(unix)]
    file: [u64; 2],
}

impl RepositoryIdentity {
    fn read(repo: &Path) -> Result<Self> {
        let metadata = fs::symlink_metadata(repo).context("Cannot inspect repository root")?;
        ensure!(
            !metadata.file_type().is_symlink(),
            "Compare with Local does not write through symlink repository roots"
        );
        ensure!(
            metadata.is_dir(),
            "Comparison repository root is unavailable"
        );
        #[cfg(unix)]
        let file = {
            use std::os::unix::fs::MetadataExt;
            [metadata.dev(), metadata.ino()]
        };
        Ok(Self {
            canonical: repo.canonicalize()?,
            created: metadata.created().ok(),
            #[cfg(unix)]
            file,
        })
    }
}

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
        let mut bytes = Vec::new();
        fs::File::open(&file)?
            .take((LIMIT + 1) as u64)
            .read_to_end(&mut bytes)
            .with_context(|| format!("read {}", file.display()))?;
        ensure!(bytes.len() <= LIMIT, "Working file is larger than 512 KiB");
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

    fn workspace_note(&self) -> String {
        let Self::File { permissions, .. } = self else {
            return "Absent from working tree".into();
        };
        #[cfg(unix)]
        let executable = mode(permissions) & 0o100 != 0;
        #[cfg(not(unix))]
        let executable = {
            let _ = permissions;
            false
        };
        let mode = if executable { "100755" } else { "100644" };
        let note = self.note();
        let ending = note.strip_prefix("Working tree · ").unwrap_or(&note);
        format!("{mode} · {ending}")
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
    RepositoryIdentity::read(repo)?;
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
            "--literal-pathspecs",
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
    repository_identity: RepositoryIdentity,
    path: PathBuf,
    revision_id: String,
    revision_text: String,
    revision_note: String,
    revision_present: bool,
    original: Snapshot,
    conversion_state: Vec<u8>,
    changed: bool,
    working_tree: bool,
}

impl LocalCompare {
    pub(in crate::log) fn prepare_head(c: &Context, file: &Path) -> Result<Self> {
        let target = Target::resolve(c, file)?;
        ensure!(
            c.scope.contains(&target.root),
            "The file repository is outside the current scope"
        );
        let repo = &c.repos[target.root];
        let head = crate::core::git(&repo.path, &["rev-parse", "--verify", "HEAD^{commit}"])
            .context("File Diff requires an existing HEAD commit")?;
        Self::prepare_file(target.root, repo, &head, &target.path, &target.path)
    }

    pub(in crate::log) fn ensure_head(&self) -> Result<()> {
        self.ensure_repository()?;
        ensure!(
            crate::core::git(&self.repo, &["rev-parse", "--verify", "HEAD^{commit}"])?
                == self.revision_id,
            "HEAD changed since File Diff opened; reopen the comparison"
        );
        ensure!(
            self.conversion_state == conversion_state(&self.repo, &self.path)?,
            "Git attributes/config changed; reopen the comparison"
        );
        safe_file(&self.repo, &self.path, false)?;
        Ok(())
    }

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
        Self::prepare_file(target.root, repo, &rev.oid, &revision_path, &target.path)
    }

    fn prepare_file(
        root: usize,
        repo: &crate::core::Repo,
        oid: &str,
        revision_path: &Path,
        path: &Path,
    ) -> Result<Self> {
        let repository_identity = RepositoryIdentity::read(&repo.path)?;
        let revision = revision::regular_file(&repo.path, oid, revision_path)?;
        let (revision_text, revision_note, revision_present) = revision.map_or_else(
            || (String::new(), "Absent in this revision".into(), false),
            |source| (source.text, source.description, true),
        );
        let conversion_state = conversion_state(&repo.path, path)?;
        let original = Snapshot::read(&repo.path, path)?;
        original.text()?;
        let local = Self {
            root,
            repo: repo.path.clone(),
            repo_name: repo.name.clone(),
            repository_identity,
            path: path.to_path_buf(),
            revision_id: oid.to_owned(),
            revision_text,
            revision_note,
            revision_present,
            original,
            conversion_state,
            changed: false,
            working_tree: false,
        };
        local.ensure_repository()?;
        Ok(local)
    }

    pub(in crate::log) fn prepare_working_tree(
        root: usize,
        repo: &crate::core::Repo,
        oid: &str,
        path: &Path,
        sources: &[revision::Source; 2],
    ) -> Result<Self> {
        let mut local = Self::prepare_file(root, repo, oid, path, path)?;
        ensure!(
            local.revision_text == sources[0].text && local.original.text()? == sources[1].text,
            "File changed while loading the comparison; press F5 to refresh"
        );
        safe_file(&repo.path, path, false)?;
        local.working_tree = true;
        Ok(local)
    }

    pub(in crate::log) fn diff(&self) -> Result<DiffView> {
        let local_text = self.original.text()?;
        let comparison = self.comparison(&local_text)?;
        let name = if self.working_tree {
            model::plain(self.path.as_os_str().as_encoded_bytes())
        } else {
            format!(
                "{} · {}",
                self.repo_name,
                model::plain(self.path.as_os_str().as_encoded_bytes())
            )
        };
        Ok(DiffView {
            name,
            comparison: Some(comparison),
            local: Some(self.clone()),
            ..Default::default()
        })
    }

    pub(in crate::log) fn comparison(&self, local_text: &str) -> Result<RevisionDiff> {
        let revision_text = match_line_endings(self.revision_text.clone(), local_text)?;
        let path = model::plain(self.path.as_os_str().as_encoded_bytes());
        let name = if self.working_tree {
            path
        } else {
            format!("{} · {path}", self.repo_name)
        };
        let comparison = if self.working_tree {
            RevisionDiff::working_tree(
                name.clone(),
                self.revision_id.clone(),
                [revision_text, local_text.to_owned()],
                [self.revision_note.clone(), self.original.workspace_note()],
                true,
            )
        } else {
            RevisionDiff::local(
                name.clone(),
                self.revision_id.clone(),
                revision_text,
                local_text.to_owned(),
                self.revision_note.clone(),
                self.original.note(),
                self.revision_present,
            )
        };
        Ok(comparison)
    }

    pub(in crate::log) fn head_comparison(
        &self,
        buffer_text: Option<&str>,
    ) -> Result<RevisionDiff> {
        let mut comparison = match buffer_text {
            Some(text) => self.comparison(text)?,
            None => self.comparison(&self.original.text()?)?,
        };
        comparison.label_head(buffer_text.is_some());
        Ok(comparison)
    }

    pub(in crate::log) fn text(&self) -> Result<String> {
        self.original.text()
    }

    pub(in crate::log) fn refresh(&self) -> Result<Self> {
        self.ensure_repository()?;
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

    fn ensure_repository(&self) -> Result<()> {
        ensure!(
            self.repository_identity == RepositoryIdentity::read(&self.repo)?,
            "Repository root changed since Compare with Local opened; reopen the comparison"
        );
        Ok(())
    }

    pub(super) fn ensure_current(&self, c: &Context) -> Result<()> {
        ensure!(
            c.scope.contains(&self.root),
            "The comparison repository is outside the current scope"
        );
        let repo = c
            .repos
            .get(self.root)
            .context("Comparison repository is unavailable")?;
        ensure!(
            repo.path == self.repo,
            "The repository changed since Compare with Local opened"
        );
        self.ensure_repository()?;
        safe_file(&self.repo, &self.path, false)?;
        Ok(())
    }

    pub(in crate::log) fn ensure_matches(
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

    pub(in crate::log) fn write(&self, text: &str) -> Result<()> {
        self.ensure_repository()?;
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
                self.ensure_repository()?;
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
                self.ensure_repository()?;
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
