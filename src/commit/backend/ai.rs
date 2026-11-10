//! Freeze selected commit sources once, then read bounded AI patches from trees.
use super::{raw, Change};
use crate::commit::process::Control;
use anyhow::{bail, ensure, Context as _, Result};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Component, Path, PathBuf},
    process::{Command, Output},
};
use tempfile::TempDir;

const PATCH_LIMIT: usize = 32 * 1024 * 1024;
const METADATA_LIMIT: usize = 4 * 1024 * 1024;
const SMALL_LIMIT: usize = 64 * 1024;
const STALE: &str =
    "Selected sources, staging, branch, or Git conversions changed; refresh and generate again";

type Counts = (Option<usize>, Option<usize>);

const DIFF: &[&str] = &[
    "diff",
    "--no-ext-diff",
    "--no-textconv",
    "--no-color",
    "--no-renames",
    "--ignore-submodules=none",
    "--submodule=short",
    "--inter-hunk-context=0",
    "--no-relative",
    "--src-prefix=a/",
    "--dst-prefix=b/",
    "--output-indicator-new=+",
    "--output-indicator-old=-",
    "--output-indicator-context= ",
    "--diff-algorithm=myers",
    "--no-indent-heuristic",
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FilePatch {
    pub path: PathBuf,
    pub status: String,
    pub additions: Option<usize>,
    pub deletions: Option<usize>,
    pub patch: String,
}

#[derive(Debug)]
pub struct Snapshot {
    pub files: Vec<FilePatch>,
    selected: Vec<Change>,
    sources: Sources,
    frozen: Frozen,
}

#[derive(Debug)]
struct Frozen {
    repo: PathBuf,
    // Keep the private index and its referenced immutable objects alive for all
    // patch reads. Validation never rebuilds this index or either tree.
    _directory: TempDir,
    index: PathBuf,
    base: String,
    tree: String,
}

#[derive(Debug, PartialEq, Eq)]
struct Anchor {
    head: Option<String>,
    branch: Option<Vec<u8>>,
    index: PathBuf,
}

#[derive(Debug, PartialEq, Eq)]
struct Sources {
    anchor: Anchor,
    files: Vec<Source>,
    conversions: Option<Vec<u8>>,
}

#[derive(Debug, PartialEq, Eq)]
struct Source {
    index: Vec<u8>,
    staged: Option<u8>,
    working: Option<Working>,
    attributes: Option<Vec<u8>>,
}

#[derive(Debug, PartialEq, Eq)]
enum Working {
    Missing,
    Directory,
    File { hash: String, mode: u32 },
    Symlink(PathBuf),
    Gitlink(Option<String>),
}

fn command(repo: &Path, index: Option<&Path>) -> Command {
    let mut command = super::cmd(repo, index);
    command
        .args([
            "--no-optional-locks",
            "-c",
            "core.fsmonitor=false",
            "-c",
            "core.splitIndex=false",
            "-c",
            "core.quotePath=true",
        ])
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_NO_REPLACE_OBJECTS", "1")
        .env("GIT_NO_LAZY_FETCH", "1")
        .env_remove("GIT_GLOB_PATHSPECS")
        .env_remove("GIT_NOGLOB_PATHSPECS")
        .env_remove("GIT_ICASE_PATHSPECS");
    command
}

fn output(
    repo: &Path,
    index: Option<&Path>,
    args: &[&str],
    input: Option<&[u8]>,
    limit: usize,
    control: &Control,
) -> Result<Output> {
    let mut command = command(repo, index);
    command.args(args);
    control.run(command, input, limit)
}

fn checked(output: Output, operation: &'static str) -> Result<Vec<u8>> {
    // Git and clean-filter diagnostics can include credentials or source text.
    // Report only our operation description and the process exit status.
    ensure!(
        output.status.success(),
        "Git could not {operation} ({}); check Git configuration, filters, and repository permissions, then retry",
        output.status
    );
    Ok(output.stdout)
}

fn git(
    repo: &Path,
    index: Option<&Path>,
    args: &[&str],
    operation: &'static str,
    control: &Control,
) -> Result<Vec<u8>> {
    checked(
        output(repo, index, args, None, METADATA_LIMIT, control)?,
        operation,
    )
}

// Like backend::exact_path, but execution always goes through Control. Literal
// includes alone also match children, so tree diffs and add need an exclusion.
fn exact_command(repo: &Path, index: Option<&Path>, args: &[&str], path: &Path) -> Command {
    let mut include = std::ffi::OsString::from(":(top,literal)");
    include.push(path);
    let mut exclude = b":(top,exclude,glob)**/".to_vec();
    for &byte in path.as_os_str().as_encoded_bytes() {
        if matches!(byte, b'*' | b'?' | b'[' | b']' | b'\\') {
            exclude.push(b'\\');
        }
        exclude.push(byte);
    }
    exclude.extend_from_slice(b"/**/*");
    let mut command = command(repo, index);
    command
        .env_remove("GIT_LITERAL_PATHSPECS")
        .args(args)
        .arg("--")
        .arg(include)
        .arg(raw(&exclude));
    command
}

fn entry(repo: &Path, path: &Path, control: &Control) -> Result<Vec<u8>> {
    let mut command = command(repo, None);
    command.args(["ls-files", "--stage", "-z", "--"]).arg(path);
    let bytes = checked(
        control.run(command, None, METADATA_LIMIT)?,
        "read selected index entries",
    )?;
    let mut entry = Vec::new();
    for record in bytes.split(|&byte| byte == 0).filter(|r| !r.is_empty()) {
        let tab = record
            .iter()
            .position(|&byte| byte == b'\t')
            .context("Git returned an invalid index entry")?;
        if &record[tab + 1..] == path.as_os_str().as_encoded_bytes() {
            entry.extend_from_slice(record);
            entry.push(0);
        }
    }
    index_mode(&entry)?;
    Ok(entry)
}

fn index_mode(entry: &[u8]) -> Result<Option<&str>> {
    let mut records = entry.split(|&byte| byte == 0).filter(|r| !r.is_empty());
    let Some(record) = records.next() else {
        return Ok(None);
    };
    let tab = record
        .iter()
        .position(|&byte| byte == b'\t')
        .context("Git returned an invalid index entry")?;
    let mut fields = std::str::from_utf8(&record[..tab])?.split(' ');
    let mode = fields.next().context("Missing selected index mode")?;
    fields.next().context("Missing selected index object")?;
    ensure!(
        fields.next() == Some("0") && records.next().is_none(),
        "Unresolved selected file; resolve conflicts before generating a message"
    );
    Ok(Some(mode))
}

fn head(repo: &Path, control: &Control) -> Result<Option<String>> {
    let output = output(
        repo,
        None,
        &["rev-parse", "--verify", "-q", "HEAD"],
        None,
        SMALL_LIMIT,
        control,
    )?;
    if output.status.code() == Some(1) {
        return Ok(None);
    }
    Ok(Some(object_id(checked(output, "read HEAD")?)?))
}

fn object_id(bytes: Vec<u8>) -> Result<String> {
    let value = String::from_utf8(bytes)?;
    let value = value.trim_end_matches(['\r', '\n']);
    ensure!(
        matches!(value.len(), 40 | 64) && value.bytes().all(|byte| byte.is_ascii_hexdigit()),
        "Git returned an invalid object ID"
    );
    Ok(value.into())
}

fn anchor(repo: &Path, control: &Control) -> Result<Anchor> {
    let head = head(repo, control)?;
    let branch = output(
        repo,
        None,
        &["symbolic-ref", "-q", "HEAD"],
        None,
        SMALL_LIMIT,
        control,
    )?;
    let branch = if branch.status.code() == Some(1) {
        None
    } else {
        Some(checked(branch, "read the current branch")?)
    };
    let mut index = git(
        repo,
        None,
        &["rev-parse", "--path-format=absolute", "--git-path", "index"],
        "locate the index",
        control,
    )?;
    if index.last() == Some(&b'\n') {
        index.pop();
    }
    Ok(Anchor {
        head,
        branch,
        index: PathBuf::from(raw(&index)),
    })
}

fn name_status(bytes: &[u8]) -> Result<BTreeMap<PathBuf, u8>> {
    let mut result = BTreeMap::new();
    let mut fields = bytes.split(|&byte| byte == 0);
    while let Some(kind) = fields.next().filter(|kind| !kind.is_empty()) {
        let path = fields
            .next()
            .filter(|path| !path.is_empty())
            .context("Git returned a file change without a path")?;
        ensure!(
            kind.len() == 1 && matches!(kind[0], b'A' | b'M' | b'D' | b'T'),
            "Git returned an unsupported selected change kind"
        );
        ensure!(
            result.insert(PathBuf::from(raw(path)), kind[0]).is_none(),
            "Git returned duplicate selected paths"
        );
    }
    Ok(result)
}

fn staged(repo: &Path, base: &str, path: &Path, control: &Control) -> Result<Option<u8>> {
    let mut args = DIFF.to_vec();
    args.extend([
        "--cached",
        "--ita-invisible-in-index",
        "--name-status",
        "-z",
        base,
    ]);
    let bytes = checked(
        control.run(exact_command(repo, None, &args, path), None, METADATA_LIMIT)?,
        "read selected staged changes",
    )?;
    let mut changes = name_status(&bytes)?;
    let kind = changes.remove(path);
    ensure!(changes.is_empty(), "Git returned an unselected cached path");
    Ok(kind)
}

fn attributes(repo: &Path, index: &Path, path: &Path, control: &Control) -> Result<Vec<u8>> {
    let mut command = command(repo, Some(index));
    command.args([
        "check-attr",
        "-z",
        "--stdin",
        "text",
        "eol",
        "crlf",
        "working-tree-encoding",
        "filter",
        "ident",
        "diff",
    ]);
    let input = [path.as_os_str().as_encoded_bytes(), b"\0"].concat();
    checked(
        control.run(command, Some(&input), METADATA_LIMIT)?,
        "read selected attributes",
    )
}

fn conversions(repo: &Path, control: &Control) -> Result<Vec<u8>> {
    let output = output(
        repo,
        None,
        &[
            "config",
            "--null",
            "--get-regexp",
            r"^(core\.(autocrlf|eol|safecrlf|checkroundtripencoding|filemode|symlinks|ignorecase|attributesfile)|filter\.)",
        ],
        None,
        METADATA_LIMIT,
        control,
    )?;
    if output.status.code() == Some(1) {
        return Ok(Vec::new());
    }
    checked(output, "read Git conversion settings")
}

fn working(
    repo: &Path,
    file: &Change,
    index_mode: Option<&str>,
    control: &Control,
) -> Result<Working> {
    control.check()?;
    let mut parent = repo.to_path_buf();
    let parts: Vec<_> = file.path.components().collect();
    for part in &parts[..parts.len() - 1] {
        control.check()?;
        parent.push(part);
        match fs::symlink_metadata(&parent) {
            Ok(metadata) if metadata.is_dir() && !parent.join(".git").exists() => (),
            Ok(metadata) if metadata.is_file() => return Ok(Working::Missing),
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
                ) =>
            {
                return Ok(Working::Missing);
            }
            _ => bail!("Unsafe parent of selected path {}", file.label()),
        }
    }
    let path = repo.join(&file.path);
    let metadata = match fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error)
            if matches!(
                error.kind(),
                std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
            ) =>
        {
            return Ok(Working::Missing);
        }
        Err(error) => return Err(error).context("Cannot inspect the selected working file"),
    };
    if metadata.is_symlink() {
        return Ok(Working::Symlink(fs::read_link(path)?));
    }
    if metadata.is_dir() {
        if file.xy[1] == b'D' {
            // Fingerprint the replacement's kind, never its unchecked children.
            return Ok(Working::Directory);
        }
        if index_mode == Some("160000") {
            // An uninitialized submodule directory resolves Git commands in the
            // parent. Only a child working tree's own HEAD is a gitlink source.
            let prefix = git(
                &path,
                None,
                &["rev-parse", "--show-prefix"],
                "identify the selected submodule",
                control,
            )?;
            return if prefix == b"\n" {
                Ok(Working::Gitlink(head(&path, control)?))
            } else {
                Ok(Working::Directory)
            };
        }
        bail!(
            "{} is a nested repository or directory; commit in its own repository",
            file.label()
        );
    }
    ensure!(
        metadata.is_file(),
        "{} is a special file; select ordinary files or symlinks",
        file.label()
    );
    #[cfg(unix)]
    let mode = {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o7777
    };
    #[cfg(not(unix))]
    let mode = u32::from(metadata.permissions().readonly());
    let mut command = command(repo, None);
    command
        .args(["hash-object", "--no-filters", "--"])
        .arg(&file.path);
    let hash = object_id(checked(
        control.run(command, None, SMALL_LIMIT)?,
        "fingerprint a selected working file",
    )?)?;
    Ok(Working::File { hash, mode })
}

fn sources(frozen: &Frozen, files: &[Change], control: &Control) -> Result<Sources> {
    let repo = &frozen.repo;
    let anchor = anchor(repo, control)?;
    let has_working = files.iter().any(|file| !file.staged());
    let conversions = has_working
        .then(|| conversions(repo, control))
        .transpose()?;
    let mut selected = Vec::with_capacity(files.len());
    for file in files {
        control.check()?;
        let index = entry(repo, &file.path, control)?;
        let staged = staged(repo, &frozen.base, &file.path, control)?;
        ensure!(staged.is_some() == file.staged(), "{STALE}");
        let (working, attributes) = if file.staged() {
            // Do not stat, hash, status-scan, or run conversions for a staged
            // source: later working edits are deliberately outside its review.
            (None, None)
        } else {
            let working = working(repo, file, index_mode(&index)?, control)?;
            if file.xy[1] == b'D' {
                ensure!(
                    matches!(working, Working::Missing | Working::Directory),
                    "{STALE}"
                );
            }
            let attributes = attributes(repo, &frozen.index, &file.path, control)?;
            (Some(working), Some(attributes))
        };
        selected.push(Source {
            index,
            staged,
            working,
            attributes,
        });
    }
    ensure!(anchor == self::anchor(repo, control)?, "{STALE}");
    if let Some(before) = &conversions {
        ensure!(*before == self::conversions(repo, control)?, "{STALE}");
    }
    control.check()?;
    Ok(Sources {
        anchor,
        files: selected,
        conversions,
    })
}

fn remove(repo: &Path, index: &Path, path: &Path, control: &Control) -> Result<()> {
    let mut command = command(repo, Some(index));
    command
        .args(["update-index", "--force-remove", "--"])
        .arg(path);
    checked(
        control.run(command, None, SMALL_LIMIT)?,
        "prepare selected paths in the private index",
    )?;
    Ok(())
}

impl Frozen {
    fn changes(&self, control: &Control) -> Result<BTreeMap<PathBuf, u8>> {
        let mut args = DIFF.to_vec();
        args.extend(["--name-status", "-z", &self.base, &self.tree, "--"]);
        name_status(&git(
            &self.repo,
            Some(&self.index),
            &args,
            "read selected change kinds",
            control,
        )?)
    }

    fn stats(&self, control: &Control) -> Result<BTreeMap<PathBuf, Counts>> {
        let mut args = DIFF.to_vec();
        args.extend(["--numstat", "-z", &self.base, &self.tree, "--"]);
        let bytes = git(
            &self.repo,
            Some(&self.index),
            &args,
            "read selected patch statistics",
            control,
        )?;
        let mut result = BTreeMap::new();
        for record in bytes.split(|&byte| byte == 0).filter(|r| !r.is_empty()) {
            let mut fields = record.splitn(3, |&byte| byte == b'\t');
            let additions = fields.next().context("Missing selected additions")?;
            let deletions = fields.next().context("Missing selected deletions")?;
            let path = fields.next().context("Missing selected numstat path")?;
            let counts = if additions == b"-" && deletions == b"-" {
                (None, None)
            } else {
                (
                    Some(std::str::from_utf8(additions)?.parse()?),
                    Some(std::str::from_utf8(deletions)?.parse()?),
                )
            };
            ensure!(
                result.insert(PathBuf::from(raw(path)), counts).is_none(),
                "Git returned duplicate selected statistics"
            );
        }
        Ok(result)
    }

    fn patch(&self, path: &Path, limit: usize, control: &Control) -> Result<String> {
        let mut args = DIFF.to_vec();
        // --unified implies patch output, so it belongs only on patch reads,
        // never on the NUL-delimited name/status and numstat commands.
        args.extend(["--unified=1", self.base.as_str(), self.tree.as_str()]);
        let output = control
            .run(
                exact_command(&self.repo, Some(&self.index), &args, path),
                None,
                limit,
            )
            .map_err(|error| {
                anyhow::anyhow!(
                    "Could not read the complete selected patch (32 MiB aggregate safety cap): {error}"
                )
            })?;
        let bytes = checked(output, "read the complete selected patch")?;
        ensure!(
            !bytes.contains(&0),
            "Git returned binary bytes in a text patch; mark the selected path as binary before generating"
        );
        let patch = String::from_utf8_lossy(&bytes).into_owned();
        ensure!(
            patch.len() <= limit,
            "Selected patches exceed the 32 MiB aggregate safety cap; select fewer files"
        );
        ensure!(!patch.is_empty(), "No patch for the selected file");
        Ok(patch)
    }
}

impl Snapshot {
    pub fn capture(repo: &Path, files: &[Change], control: &Control) -> Result<Self> {
        control.check()?;
        ensure!(!files.is_empty(), "No files selected");
        let mut paths = BTreeSet::new();
        for file in files {
            control.check()?;
            ensure!(file.selectable(), "Unresolved or ignored selected file");
            ensure!(
                !file.directory()
                    && file.path.components().next().is_some()
                    && file
                        .path
                        .components()
                        .all(|part| matches!(part, Component::Normal(_)))
                    && !file.path.as_os_str().as_encoded_bytes().contains(&0),
                "Select repository-relative file paths"
            );
            ensure!(paths.insert(&file.path), "Duplicate selected file");
        }
        let initial = anchor(repo, control)?;
        let base = if let Some(head) = &initial.head {
            object_id(git(
                repo,
                None,
                &["rev-parse", "--verify", &format!("{head}^{{tree}}")],
                "resolve the base tree",
                control,
            )?)?
        } else {
            object_id(checked(
                output(repo, None, &["mktree"], Some(&[]), SMALL_LIMIT, control)?,
                "prepare the empty base tree",
            )?)?
        };
        let directory = tempfile::Builder::new().prefix("igit-ai-").tempdir_in(
            initial
                .index
                .parent()
                .context("Missing Git index directory")?,
        )?;
        let mut frozen = Frozen {
            repo: repo.into(),
            index: directory.path().join("selected"),
            _directory: directory,
            base,
            tree: String::new(),
        };
        git(
            repo,
            Some(&frozen.index),
            &["read-tree", &frozen.base],
            "initialize the private index",
            control,
        )?;

        // Import cached object IDs, never re-add staged worktree files. Do this
        // before conversion checks so missing .gitattributes uses the same
        // selected private-index fallback throughout working-file capture.
        let mut imported = BTreeMap::new();
        for file in files.iter().filter(|file| file.staged()) {
            let entry = entry(repo, &file.path, control)?;
            remove(repo, &frozen.index, &file.path, control)?;
            if !entry.is_empty() {
                checked(
                    output(
                        repo,
                        Some(&frozen.index),
                        &["update-index", "-z", "--index-info"],
                        Some(&entry),
                        SMALL_LIMIT,
                        control,
                    )?,
                    "import selected staged content",
                )?;
            }
            imported.insert(&file.path, entry);
        }
        // Set the selected deletion scope before sampling attributes. A missing
        // working .gitattributes falls back to this index, so leaving its HEAD
        // version here would sample different conversions from the later add.
        // Removing reviewed children first also makes replacements independent
        // of selection order.
        for file in files
            .iter()
            .filter(|file| !file.staged() && file.xy[1] == b'D')
        {
            remove(repo, &frozen.index, &file.path, control)?;
        }
        let before = sources(&frozen, files, control)?;
        ensure!(before.anchor == initial, "{STALE}");
        for (file, source) in files.iter().zip(&before.files) {
            if file.staged() {
                ensure!(imported.get(&file.path) == Some(&source.index), "{STALE}");
            }
        }
        for file in files
            .iter()
            .filter(|file| !file.staged() && file.xy[1] != b'D')
        {
            checked(
                control.run(
                    exact_command(repo, Some(&frozen.index), &["add", "-A"], &file.path),
                    None,
                    SMALL_LIMIT,
                )?,
                "capture selected working content",
            )?;
        }
        frozen.tree = object_id(git(
            repo,
            Some(&frozen.index),
            &["write-tree"],
            "freeze the selected tree",
            control,
        )?)?;
        let changes = frozen.changes(control)?;
        check_scope(files, &changes)?;
        let mut stats = frozen.stats(control)?;
        let mut prepared = Vec::with_capacity(files.len());
        let mut remaining = PATCH_LIMIT;
        for file in files {
            control.check()?;
            let patch = frozen.patch(&file.path, remaining, control)?;
            remaining -= patch.len();
            let (additions, deletions) = stats
                .remove(&file.path)
                .context("Git returned no statistics for the selected file")?;
            prepared.push(FilePatch {
                path: file.path.clone(),
                status: char::from(changes[&file.path]).to_string(),
                additions,
                deletions,
                patch,
            });
        }
        ensure!(stats.is_empty(), "Git returned unselected file statistics");
        ensure!(sources(&frozen, files, control)? == before, "{STALE}");
        control.check()?;
        Ok(Self {
            files: prepared,
            selected: files.to_vec(),
            sources: before,
            frozen,
        })
    }

    pub fn validate(&self, control: &Control) -> Result<()> {
        control.check()?;
        ensure!(
            sources(&self.frozen, &self.selected, control)? == self.sources,
            "{STALE}"
        );
        control.check()
    }
}

fn check_scope(files: &[Change], changes: &BTreeMap<PathBuf, u8>) -> Result<()> {
    let mut expected: BTreeMap<_, _> = files
        .iter()
        .map(|file| {
            (
                &file.path,
                if file.staged() {
                    file.xy[0]
                } else {
                    file.xy[1]
                } == b'D',
            )
        })
        .collect();
    for (path, &kind) in changes {
        let deleted = expected.remove(path).with_context(|| {
            format!(
                "Replacing a file with a directory or vice versa also affects unselected path {}. Select all affected paths and try again",
                crate::commit::display_path(path)
            )
        })?;
        ensure!(
            deleted == (kind == b'D'),
            "The selected versions cannot coexist in one commit: {}. Check file/directory replacements, refresh, and try again",
            crate::commit::display_path(path)
        );
    }
    if let Some((path, _)) = expected.first_key_value() {
        bail!(
            "The snapshot has no changes for selected path {}. Check your selection, refresh, and try again",
            crate::commit::display_path(path)
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests;
