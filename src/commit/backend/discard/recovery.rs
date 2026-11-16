use super::super::{checked, exact_path, run};
use super::*;
use anyhow::ensure;
use serde::{Deserialize, Serialize};
use std::{
    ffi::OsString,
    path::Component,
    time::{SystemTime, UNIX_EPOCH},
};

#[cfg(test)]
mod tests;

const HISTORY: usize = 8;
const CONTENT_LIMIT: usize = 32 * 1024 * 1024;
const RECORD_LIMIT: u64 = 256 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Native(Vec<u8>);

impl Native {
    fn new(path: &Path) -> Self {
        #[cfg(unix)]
        {
            Self(path.as_os_str().as_encoded_bytes().to_vec())
        }
        #[cfg(windows)]
        {
            use std::os::windows::ffi::OsStrExt;
            Self(
                path.as_os_str()
                    .encode_wide()
                    .flat_map(u16::to_le_bytes)
                    .collect(),
            )
        }
    }

    fn path(&self) -> Result<PathBuf> {
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStringExt;
            Ok(OsString::from_vec(self.0.clone()).into())
        }
        #[cfg(windows)]
        {
            use std::os::windows::ffi::OsStringExt;
            ensure!(
                self.0.len().is_multiple_of(2),
                "Invalid native recovery path"
            );
            let wide: Vec<_> = self
                .0
                .chunks_exact(2)
                .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
                .collect();
            Ok(OsString::from_wide(&wide).into())
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
enum Content {
    Missing,
    File { bytes: Vec<u8>, mode: u32 },
    Symlink(Native),
}

impl Content {
    fn read(repo: &Path, path: &Path) -> Result<Self> {
        let before = working(repo, path)?;
        let content = match &before {
            Working::Missing => Self::Missing,
            Working::Symlink(target) => Self::Symlink(Native::new(target)),
            Working::File { hash, mode } => {
                ensure!(
                    fs::metadata(repo.join(path))?.len() <= CONTENT_LIMIT as u64,
                    "{} exceeds the 32 MiB recovery limit; use an external tool",
                    path.display()
                );
                let bytes = fs::read(repo.join(path))?;
                let actual = checked(run(
                    repo,
                    None,
                    &[
                        "hash-object".as_ref(),
                        "--no-filters".as_ref(),
                        "--stdin".as_ref(),
                    ],
                    Some(&bytes),
                )?)?;
                ensure!(
                    std::str::from_utf8(&actual)?.trim() == hash.as_str(),
                    "{} changed while its recovery copy was read",
                    path.display()
                );
                Self::File { bytes, mode: *mode }
            }
        };
        ensure!(
            before == working(repo, path)?,
            "{} changed while its recovery copy was read",
            path.display()
        );
        Ok(content)
    }

    fn size(&self) -> usize {
        match self {
            Self::File { bytes, .. } => bytes.len(),
            Self::Symlink(target) => target.0.len(),
            Self::Missing => 0,
        }
    }

    fn description(&self) -> String {
        match self {
            Self::Missing => "missing".into(),
            Self::File { bytes, mode } => format!("file, {} bytes, mode {mode:o}", bytes.len()),
            Self::Symlink(target) => format!(
                "symlink -> {}",
                target
                    .path()
                    .map_or_else(|_| "?".into(), |p| p.display().to_string())
            ),
        }
    }

    fn write(&self, repo: &Path, path: &Path) -> Result<()> {
        working(repo, path)?;
        let full = repo.join(path);
        let parent = full.parent().context("Recovery path has no parent")?;
        match self {
            Self::Missing => {
                if fs::symlink_metadata(&full).is_ok() {
                    fs::remove_file(&full)?;
                    sync_dir(parent)?;
                }
            }
            Self::File { bytes, mode } => {
                fs::create_dir_all(parent)?;
                working(repo, path)?;
                let mut temp = tempfile::NamedTempFile::new_in(parent)?;
                temp.write_all(bytes)?;
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    temp.as_file()
                        .set_permissions(fs::Permissions::from_mode(*mode))?;
                }
                #[cfg(not(unix))]
                {
                    let mut permissions = temp.as_file().metadata()?.permissions();
                    permissions.set_readonly(*mode != 0);
                    temp.as_file().set_permissions(permissions)?;
                }
                temp.as_file().sync_all()?;
                temp.persist(&full).map_err(|error| error.error)?;
                sync_dir(parent)?;
            }
            Self::Symlink(target) => {
                fs::create_dir_all(parent)?;
                working(repo, path)?;
                let temp = tempfile::NamedTempFile::new_in(parent)?.into_temp_path();
                fs::remove_file(&temp)?;
                #[cfg(unix)]
                std::os::unix::fs::symlink(target.path()?, &temp)?;
                #[cfg(windows)]
                std::os::windows::fs::symlink_file(target.path()?, &temp)?;
                fs::rename(&temp, &full)?;
                sync_dir(parent)?;
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Snapshot {
    content: Content,
    index: Vec<u8>,
    flags: u32,
}

impl Snapshot {
    fn read(repo: &Path, path: &Path) -> Result<Self> {
        Ok(Self {
            content: Content::read(repo, path)?,
            index: stage(repo, None, path)?,
            flags: flags(repo, None, path)?,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Blob {
    oid: String,
    bytes: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct SavedPath {
    path: Native,
    before: Snapshot,
    after: Option<Snapshot>,
    blobs: Vec<Blob>,
    restored: bool,
    restoring: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Record {
    version: u32,
    repo: Native,
    git_dir: Native,
    head: Option<String>,
    branch: String,
    finished: bool,
    paths: Vec<SavedPath>,
}

pub(super) struct Journal {
    path: PathBuf,
    repo: PathBuf,
    record: Record,
}

fn sync_dir(path: &Path) -> Result<()> {
    #[cfg(unix)]
    fs::File::open(path)?.sync_all()?;
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

fn private_dir(path: &Path) -> Result<()> {
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(path)?;
    ensure!(
        !fs::symlink_metadata(path)?.is_symlink(),
        "Recovery state directory must not be a symlink"
    );
    Ok(())
}

fn save(path: &Path, record: &Record) -> Result<()> {
    let parent = path.parent().context("Recovery state has no parent")?;
    let mut temp = tempfile::NamedTempFile::new_in(parent)?;
    serde_json::to_writer(temp.as_file_mut(), record)?;
    ensure!(
        temp.as_file().metadata()?.len() <= RECORD_LIMIT,
        "Recovery record exceeds 256 MiB; the previous backup was retained"
    );
    temp.as_file().sync_all()?;
    temp.persist(path).map_err(|error| error.error)?;
    sync_dir(parent)
}

fn read(path: &Path) -> Result<Record> {
    let file = fs::File::open(path)?;
    ensure!(
        file.metadata()?.len() <= RECORD_LIMIT,
        "Recovery record is too large"
    );
    let record: Record = serde_json::from_reader(file)?;
    ensure!(record.version == 1, "Unsupported rollback recovery version");
    Ok(record)
}

fn directory(state: &Path, repo: &Path) -> PathBuf {
    let hash = Native::new(repo)
        .0
        .iter()
        .fold(0xcbf29ce484222325u64, |value, byte| {
            (value ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
        });
    state.join("rollbacks").join(format!("{hash:016x}"))
}

fn records(directory: &Path) -> Result<Vec<PathBuf>> {
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
        Err(error) => return Err(error.into()),
    };
    let mut paths = vec![];
    for entry in entries {
        let entry = entry?;
        if entry.file_type()?.is_file() && entry.path().extension().is_some_and(|ext| ext == "json")
        {
            paths.push(entry.path());
        }
    }
    paths.sort();
    Ok(paths)
}

fn stage(repo: &Path, index: Option<&Path>, path: &Path) -> Result<Vec<u8>> {
    exact_path(repo, index, &["ls-files", "--stage", "-z"], path)
}

fn flags(repo: &Path, index: Option<&Path>, path: &Path) -> Result<u32> {
    let output = exact_path(
        repo,
        index,
        &[
            "--no-optional-locks",
            "-c",
            "core.fsmonitor=false",
            "ls-files",
            "--debug",
            "-z",
        ],
        path,
    )?;
    if output.is_empty() {
        return Ok(0);
    }
    let metadata = output
        .splitn(2, |byte| *byte == 0)
        .nth(1)
        .context("Missing recovery index flags")?;
    let metadata = std::str::from_utf8(metadata)?;
    let flags = metadata
        .rsplit_once("flags: ")
        .context("Missing recovery index flags")?
        .1
        .trim();
    Ok(u32::from_str_radix(flags, 16)? & !(0x0800_0000 | 0x0020_0000))
}

fn validate_entry(path: &Path, entry: &[u8]) -> Result<()> {
    if entry.is_empty() {
        return Ok(());
    }
    let entry = entry
        .strip_suffix(&[0])
        .context("Invalid saved index record")?;
    let tab = entry
        .iter()
        .position(|byte| *byte == b'\t')
        .context("Invalid saved index record")?;
    let fields: Vec<_> = std::str::from_utf8(&entry[..tab])?
        .split_ascii_whitespace()
        .collect();
    ensure!(
        fields.len() == 3
            && ["100644", "100755", "120000"].contains(&fields[0])
            && fields[2] == "0"
            && [40, 64].contains(&fields[1].len())
            && fields[1].bytes().all(|byte| byte.is_ascii_hexdigit())
            && &entry[tab + 1..] == path.as_os_str().as_encoded_bytes(),
        "Unsupported or mismatched saved index entry"
    );
    Ok(())
}

fn valid_path(path: &Path) -> Result<()> {
    ensure!(
        !path.as_os_str().is_empty()
            && path.components().all(|part| {
                matches!(part, Component::Normal(name) if !name.as_encoded_bytes().eq_ignore_ascii_case(b".git"))
            }),
        "Unsafe path in rollback recovery"
    );
    Ok(())
}

fn blobs(repo: &Path, entry: &[u8]) -> Result<Vec<Blob>> {
    let mut result = vec![];
    for record in entry
        .split(|byte| *byte == 0)
        .filter(|record| !record.is_empty())
    {
        let header = record.split(|byte| *byte == b'\t').next().unwrap();
        let fields: Vec<_> = std::str::from_utf8(header)?
            .split_ascii_whitespace()
            .collect();
        ensure!(
            fields.len() == 3 && fields[2] == "0",
            "Unsupported recovery index entry"
        );
        let bytes = g(repo, None, &["cat-file", "blob", fields[1]])?;
        ensure!(
            bytes.len() <= CONTENT_LIMIT,
            "Staged content exceeds the 32 MiB recovery limit"
        );
        result.push(Blob {
            oid: fields[1].into(),
            bytes,
        });
    }
    Ok(result)
}

impl Journal {
    pub(super) fn prepare(plan: &Plan, state: &Path) -> Result<Self> {
        let repo = fs::canonicalize(&plan.repo)?;
        let directory = directory(state, &repo);
        private_dir(&directory)?;
        ensure!(
            !fs::canonicalize(&directory)?.starts_with(&repo),
            "Set IGIT_STATE_DIR outside the repository before rolling back"
        );
        let mut history = records(&directory)?;
        while history.len() >= HISTORY {
            let oldest = history.remove(0);
            let old = read(&oldest)?;
            ensure!(
                old.finished && !old.paths.iter().any(|path| path.restoring),
                "Recovery history contains an interrupted operation at {}; preserve or resolve it before another rollback",
                oldest.display()
            );
            fs::remove_file(oldest)?;
            sync_dir(&directory)?;
        }
        let mut paths = vec![];
        let mut total = 0;
        for (file, expected) in plan.files.iter().zip(&plan.working) {
            valid_path(&file.path)?;
            ensure!(
                &working(&repo, &file.path)? == expected,
                "{} changed before its backup",
                file.label()
            );
            let before = Snapshot::read(&repo, &file.path)?;
            ensure!(
                before.flags == 0,
                "{} has intent-to-add, assume-unchanged or skip-worktree flags; clear them before rollback",
                file.label()
            );
            let blobs = blobs(&repo, &before.index)?;
            total +=
                before.content.size() + blobs.iter().map(|blob| blob.bytes.len()).sum::<usize>();
            ensure!(
                total <= CONTENT_LIMIT,
                "Rollback backup exceeds 32 MiB; select fewer files"
            );
            paths.push(SavedPath {
                path: Native::new(&file.path),
                before,
                after: None,
                blobs,
                restored: false,
                restoring: false,
            });
        }
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let placeholder = tempfile::Builder::new()
            .prefix(&format!("{stamp:032}-"))
            .suffix(".json")
            .tempfile_in(&directory)?;
        let path = placeholder.path().to_owned();
        let record = Record {
            version: 1,
            repo: Native::new(&repo),
            git_dir: Native::new(&fs::canonicalize(git_path(&repo, ".")?)?),
            head: plan.head.clone(),
            branch: plan.branch.clone(),
            finished: false,
            paths,
        };
        save(&path, &record)?;
        let (_, path) = placeholder.keep()?;
        Ok(Self { path, repo, record })
    }

    pub(super) fn record_working(&mut self, path: &Path) -> Result<()> {
        let after = Snapshot::read(&self.repo, path)?;
        self.record
            .paths
            .iter_mut()
            .find(|saved| saved.path == Native::new(path))
            .context("Rollback path is missing from its backup")?
            .after = Some(after);
        save(&self.path, &self.record).with_context(|| {
            format!(
                "Working file changed; recovery backup is at {}",
                self.path.display()
            )
        })
    }

    pub(super) fn record_index(&mut self, index: &Path, completed: &[&Path]) -> Result<()> {
        for path in completed {
            let saved = self
                .record
                .paths
                .iter_mut()
                .find(|saved| saved.path == Native::new(path))
                .context("Rollback path is missing from its backup")?;
            let after = saved.after.as_mut().context(
                "Working file changed but its post-state could not be saved; preserve the recovery backup",
            )?;
            after.index = stage(&self.repo, Some(index), path)?;
            after.flags = flags(&self.repo, Some(index), path)?;
        }
        save(&self.path, &self.record).with_context(|| {
            format!(
                "Cannot record the rollback index post-state; backup retained at {}",
                self.path.display()
            )
        })
    }

    pub(super) fn finish(&mut self) -> Result<()> {
        self.record.finished = true;
        save(&self.path, &self.record)
    }
}

pub struct RestorePlan {
    pub root: usize,
    pub repo: PathBuf,
    path: PathBuf,
    record: Record,
    expected: Vec<(usize, Snapshot)>,
}

impl RestorePlan {
    pub fn latest(root: usize, repo: &Path, state: &Path) -> Result<Option<Self>> {
        let repo = fs::canonicalize(repo)?;
        for path in records(&directory(state, &repo))?.into_iter().rev() {
            let record =
                read(&path).with_context(|| format!("Cannot read backup {}", path.display()))?;
            if record
                .paths
                .iter()
                .all(|path| path.restored || path.after.is_none())
                && record.finished
            {
                continue;
            }
            ensure!(
                record.repo == Native::new(&repo),
                "Rollback backup belongs to another repository"
            );
            let mut expected = vec![];
            let mut unique = BTreeSet::new();
            for (index, saved) in record
                .paths
                .iter()
                .enumerate()
                .filter(|(_, path)| !path.restored)
            {
                let file = saved.path.path()?;
                valid_path(&file)?;
                ensure!(
                    unique.insert(file.clone()),
                    "Duplicate path in rollback backup"
                );
                validate_entry(&file, &saved.before.index)?;
                ensure!(
                    !saved.restoring,
                    "An interrupted restore needs inspection: {}",
                    path.display()
                );
                let Some(after) = &saved.after else {
                    ensure!(
                        record.finished,
                        "Rollback was interrupted before recording its post-state: {}",
                        path.display()
                    );
                    continue;
                };
                validate_entry(&file, &after.index)?;
                let current = Snapshot::read(&repo, &file)?;
                ensure!(
                    current.content == after.content
                        && (current.index == after.index && current.flags == after.flags
                            || !record.finished
                                && current.index == saved.before.index
                                && current.flags == saved.before.flags),
                    "{} changed after rollback; recovery refused. Backup retained at {}",
                    file.display(),
                    path.display()
                );
                expected.push((index, current));
            }
            if expected.is_empty() {
                continue;
            }
            let plan = Self {
                root,
                repo,
                path,
                record,
                expected,
            };
            plan.validate()?;
            return Ok(Some(plan));
        }
        Ok(None)
    }

    fn validate(&self) -> Result<()> {
        safe(&self.repo)?;
        ensure!(
            self.record.repo == Native::new(&fs::canonicalize(&self.repo)?)
                && self.record.git_dir
                    == Native::new(&fs::canonicalize(git_path(&self.repo, ".")?)?),
            "Repository identity changed; recovery refused"
        );
        ensure!(
            oid(&self.repo)? == self.record.head && branch_ref(&self.repo)? == self.record.branch,
            "Branch or HEAD changed after rollback; recovery refused"
        );
        ensure!(
            read(&self.path)? == self.record,
            "Rollback backup changed; review recovery again"
        );
        let current_entries = entries(&self.repo, None)?;
        for (index, expected) in &self.expected {
            let path = self.record.paths[*index].path.path()?;
            if expected.index.is_empty() {
                let mut command = super::super::cmd(&self.repo, None);
                command
                    .env_remove("GIT_LITERAL_PATHSPECS")
                    .env_remove("GIT_GLOB_PATHSPECS")
                    .env_remove("GIT_NOGLOB_PATHSPECS")
                    .env_remove("GIT_ICASE_PATHSPECS")
                    .args(["check-ignore", "-q", "--"])
                    .arg(&path);
                let ignored = crate::core::read_output(&self.repo, "check-ignore", &mut command)?;
                ensure!(
                    ignored.status.code() == Some(1),
                    "{} is ignored or its ignore state cannot be verified; recovery refused",
                    path.display()
                );
            }
            validate_layout(
                &Change {
                    path: path.clone(),
                    xy: *b" M",
                    conflict: false,
                },
                &current_entries,
            )?;
            ensure!(
                Snapshot::read(&self.repo, &path)? == *expected,
                "{} changed after the recovery review; nothing was restored",
                path.display()
            );
        }
        Ok(())
    }

    pub fn description(&self) -> Result<String> {
        let mut text = format!(
            "Restore {} path(s) in {}\nBackup: {}\nOnly these working files and index entries will be restored.\n\n",
            self.expected.len(), self.repo.display(), self.path.display()
        );
        for (index, after) in &self.expected {
            let saved = &self.record.paths[*index];
            text.push_str(&format!(
                "{}\n  Current: {}\n  Restore: {}\n  Saved index: {}\n",
                crate::commit::display_path(&saved.path.path()?),
                after.content.description(),
                saved.before.content.description(),
                if saved.before.index.is_empty() {
                    "untracked / absent".into()
                } else {
                    String::from_utf8_lossy(&saved.before.index)
                        .split('\t')
                        .next()
                        .unwrap_or("")
                        .to_owned()
                }
            ));
            if let Content::File { bytes, .. } = &saved.before.content {
                if let Ok(before) = std::str::from_utf8(bytes) {
                    let current = match &after.content {
                        Content::File { bytes, .. } => std::str::from_utf8(bytes).unwrap_or(""),
                        _ => "",
                    };
                    let patch = similar::TextDiff::from_lines(current, before)
                        .unified_diff()
                        .header("after rollback", "saved working copy")
                        .to_string();
                    for line in patch.lines().take(120) {
                        text.push_str(&line.chars().take(500).collect::<String>());
                        text.push('\n');
                    }
                    if patch.lines().count() > 120 {
                        text.push_str("… Preview truncated\n");
                    }
                } else {
                    text.push_str("  Binary working content is saved verbatim.\n");
                }
            }
            text.push('\n');
        }
        Ok(text)
    }

    pub fn execute(self) -> Result<usize> {
        let path = self.path.clone();
        self.restore_files()
            .with_context(|| format!("Recovery backup retained at {}", path.display()))
    }

    fn restore_files(mut self) -> Result<usize> {
        let index = git_path(&self.repo, "index")?;
        let lock = index.with_file_name("index.lock");
        let mut lockfile = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&lock)
            .context("Cannot lock the Git index for recovery")?;
        let mut guard = Guard(lock.clone(), false);
        self.validate()?;
        let temp = tempfile::tempdir()?;
        let next = temp.path().join("index");
        copy_index(&self.repo, &index, &next)?;
        let format = g(&self.repo, None, &["rev-parse", "--show-object-format"])?;
        let zero = "0".repeat(if format.starts_with(b"sha256") {
            64
        } else {
            40
        });
        for (number, _) in &self.expected {
            let saved = &self.record.paths[*number];
            let path = saved.path.path()?;
            for blob in &saved.blobs {
                let output = checked(run(
                    &self.repo,
                    None,
                    &["hash-object".as_ref(), "-w".as_ref(), "--stdin".as_ref()],
                    Some(&blob.bytes),
                )?)?;
                ensure!(
                    std::str::from_utf8(&output)?.trim() == blob.oid,
                    "Saved index blob is invalid"
                );
            }
            let mut input = format!("0 {zero}\t").into_bytes();
            input.extend(path.as_os_str().as_encoded_bytes());
            input.push(0);
            input.extend(&saved.before.index);
            checked(run(
                &self.repo,
                Some(&next),
                &[
                    "update-index".as_ref(),
                    "-z".as_ref(),
                    "--index-info".as_ref(),
                ],
                Some(&input),
            )?)?;
        }
        self.validate()?;
        let mut completed = vec![];
        let mut failure = None;
        for (number, expected) in &self.expected {
            let saved = &self.record.paths[*number];
            let path = saved.path.path()?;
            match Snapshot::read(&self.repo, &path) {
                Ok(current) if current == *expected => (),
                Ok(_) => {
                    failure = Some(anyhow::anyhow!(
                        "{} changed during recovery",
                        path.display()
                    ));
                    break;
                }
                Err(error) => {
                    failure = Some(error);
                    break;
                }
            };
            self.record.paths[*number].restoring = true;
            save(&self.path, &self.record)?;
            match Snapshot::read(&self.repo, &path) {
                Ok(current) if current == *expected => (),
                current => {
                    self.record.paths[*number].restoring = false;
                    save(&self.path, &self.record)?;
                    failure = Some(match current {
                        Ok(_) => {
                            anyhow::anyhow!("{} changed while preparing recovery", path.display())
                        }
                        Err(error) => error,
                    });
                    break;
                }
            }
            if let Err(error) = self.record.paths[*number]
                .before
                .content
                .write(&self.repo, &path)
            {
                failure = Some(error);
                break;
            }
            completed.push(*number);
        }
        if completed.is_empty() {
            return Err(failure.unwrap_or_else(|| anyhow::anyhow!("No paths restored")));
        }
        if failure.is_some() {
            copy_index(&self.repo, &index, &next)?;
            for number in &completed {
                let saved = &self.record.paths[*number];
                let path = saved.path.path()?;
                let mut input = format!("0 {zero}\t").into_bytes();
                input.extend(path.as_os_str().as_encoded_bytes());
                input.push(0);
                input.extend(&saved.before.index);
                checked(run(
                    &self.repo,
                    Some(&next),
                    &[
                        "update-index".as_ref(),
                        "-z".as_ref(),
                        "--index-info".as_ref(),
                    ],
                    Some(&input),
                )?)?;
            }
        }
        if index.exists() {
            lockfile.set_permissions(fs::metadata(&index)?.permissions())?;
        }
        lockfile.write_all(&fs::read(&next)?)?;
        lockfile.sync_all()?;
        drop(lockfile);
        guard.1 = true;
        fs::rename(&lock, &index).context("Working files restored but index publication failed; preserve index.lock and the backup")?;
        for number in &completed {
            self.record.paths[*number].restored = true;
            self.record.paths[*number].restoring = false;
        }
        if self.record.paths.iter().all(|path| path.restored) {
            self.record.finished = true;
        }
        save(&self.path, &self.record).context("Files and index restored but recovery history update failed; inspect the retained backup")?;
        if let Some(error) = failure {
            bail!(
                "Restored {} of {} paths: {error:#}. Backup retained at {}",
                completed.len(),
                self.expected.len(),
                self.path.display()
            );
        }
        Ok(completed.len())
    }
}
