use anyhow::{anyhow, bail, Context as _, Result};
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::{OsStr, OsString},
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
};
use tempfile::TempDir;

pub(super) mod add;
pub(super) mod discard;
pub(super) mod staging;
pub(super) mod stash;
mod working;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Change {
    pub path: PathBuf,
    pub xy: [u8; 2],
    pub conflict: bool,
}
impl Change {
    pub fn staged(&self) -> bool {
        self.xy[0] != b' ' && self.xy[0] != b'?' && !self.ignored()
    }
    pub fn ignored(&self) -> bool {
        self.xy == *b"!!"
    }
    pub fn selectable(&self) -> bool {
        !self.conflict && !self.ignored()
    }
    pub fn directory(&self) -> bool {
        self.path.as_os_str().as_encoded_bytes().ends_with(b"/")
    }
    pub fn label(&self) -> String {
        super::display_path(&self.path)
    }
}
#[cfg(unix)]
fn raw(b: &[u8]) -> OsString {
    use std::os::unix::ffi::OsStringExt;
    OsString::from_vec(b.to_vec())
}
#[cfg(not(unix))]
fn raw(b: &[u8]) -> OsString {
    OsString::from(String::from_utf8_lossy(b).into_owned())
}

fn cmd(repo: &Path, idx: Option<&Path>) -> Command {
    let mut c = crate::core::command(repo);
    c.env("GIT_LITERAL_PATHSPECS", "1")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env_remove("GIT_INDEX_FILE");
    if let Some(p) = idx {
        c.env("GIT_INDEX_FILE", p);
    }
    c
}
fn run(repo: &Path, idx: Option<&Path>, args: &[&OsStr], input: Option<&[u8]>) -> Result<Output> {
    let mut c = cmd(repo, idx);
    c.args(args)
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = c.spawn()?;
    if let Some(b) = input {
        child.stdin.take().unwrap().write_all(b)?;
    }
    Ok(child.wait_with_output()?)
}
fn checked(o: Output) -> Result<Vec<u8>> {
    if !o.status.success() {
        let e = String::from_utf8_lossy(&o.stderr);
        let s = String::from_utf8_lossy(&o.stdout);
        bail!(
            "Git: {}",
            if e.trim().is_empty() {
                s.trim()
            } else {
                e.trim()
            }
        )
    }
    Ok(o.stdout)
}
fn g(repo: &Path, idx: Option<&Path>, args: &[&str]) -> Result<Vec<u8>> {
    checked(run(
        repo,
        idx,
        &args.iter().map(OsStr::new).collect::<Vec<_>>(),
        None,
    )?)
}
fn txt(v: Vec<u8>) -> String {
    String::from_utf8_lossy(&v)
        .trim_end_matches(['\r', '\n'])
        .to_owned()
}
fn oid(repo: &Path) -> Result<Option<String>> {
    let out = run(
        repo,
        None,
        &[
            "rev-parse".as_ref(),
            "--verify".as_ref(),
            "-q".as_ref(),
            "HEAD".as_ref(),
        ],
        None,
    )?;
    if out.status.success() {
        Ok(Some(txt(out.stdout)))
    } else if out.status.code() == Some(1) {
        Ok(None)
    } else {
        Err(anyhow!(String::from_utf8_lossy(&out.stderr).to_string()))
    }
}
pub fn status(repo: &Path) -> Result<Vec<Change>> {
    let b = g(
        repo,
        None,
        &[
            "--no-optional-locks",
            "status",
            "--porcelain=v1",
            "-z",
            "--untracked-files=all",
            "--no-renames",
            // The child root shows its own dirty files. Only actual gitlink
            // revisions can contribute a patch to the superproject.
            "--ignore-submodules=dirty",
        ],
    )?;
    let mut changes: Vec<Change> = b
        .split(|c| *c == 0)
        .filter(|x| x.len() >= 4)
        .map(|r| Change {
            path: PathBuf::from(raw(&r[3..])),
            xy: [r[0], r[1]],
            conflict: matches!(
                &r[..2],
                b"DD" | b"AU" | b"UD" | b"UA" | b"DU" | b"AA" | b"UU"
            ),
        })
        .collect();
    // After a staged deletion followed by an untracked replacement, porcelain
    // reports the same path twice. Offer the reviewed deletion first. Once
    // committed, the replacement appears as an unchecked untracked file.
    let staged = changes
        .iter()
        .filter(|x| x.staged())
        .map(|x| x.path.clone())
        .collect::<std::collections::BTreeSet<_>>();
    changes.retain(|x| x.xy != *b"??" || !staged.contains(&x.path));
    Ok(changes)
}
pub fn ignored(repo: &Path) -> Result<Vec<Change>> {
    // Keep ignored build/dependency directories as single entries instead of
    // walking their contents. Git supplies the trailing slash and native bytes.
    let paths = g(
        repo,
        None,
        &[
            "--no-optional-locks",
            "ls-files",
            "--others",
            "--ignored",
            "--exclude-standard",
            "--directory",
            "-z",
        ],
    )?;
    Ok(paths
        .split(|b| *b == 0)
        .filter(|p| !p.is_empty())
        .map(|p| Change {
            path: PathBuf::from(raw(p)),
            xy: *b"!!",
            conflict: false,
        })
        .collect())
}
fn git_path(repo: &Path, n: &str) -> Result<PathBuf> {
    let mut b = g(
        repo,
        None,
        &["rev-parse", "--path-format=absolute", "--git-path", n],
    )?;
    if b.last() == Some(&b'\n') {
        b.pop();
    }
    Ok(PathBuf::from(raw(&b)))
}
fn safe(repo: &Path) -> Result<()> {
    for n in [
        "MERGE_HEAD",
        "CHERRY_PICK_HEAD",
        "REVERT_HEAD",
        "rebase-merge",
        "rebase-apply",
        "sequencer",
    ] {
        if git_path(repo, n)?.exists() {
            bail!("{n} in progress; finish or abort the current Git operation first")
        }
    }
    if !g(repo, None, &["ls-files", "-u", "-z"])?.is_empty() {
        bail!("Unresolved conflicts. Open the Conflicts tab first")
    }
    Ok(())
}
fn change_idx(repo: &Path, path: &Path, args: &[&str], file: &Path) -> Result<Vec<u8>> {
    let mut aa: Vec<_> = args.iter().map(OsStr::new).collect();
    aa.push(OsStr::new("--"));
    aa.push(file.as_os_str());
    checked(run(repo, Some(path), &aa, None)?)
}
// A literal Git pathspec also matches descendants. Intersect it with an
// escaped path glob requiring a nonempty child. The leading **/ prevents Git
// from treating the exclusion as an explicit path inside a submodule; the
// literal include still limits everything to the selected path and children.
fn exact_path(repo: &Path, idx: Option<&Path>, args: &[&str], file: &Path) -> Result<Vec<u8>> {
    let mut include = OsString::from(":(top,literal)");
    include.push(file);
    let index_read = args.first() == Some(&"ls-files");
    let mut exclude = b":(top,exclude,glob)**/".to_vec();
    for &byte in file.as_os_str().as_encoded_bytes() {
        if matches!(byte, b'*' | b'?' | b'[' | b']' | b'\\') {
            exclude.push(b'\\');
        }
        exclude.push(byte);
    }
    exclude.extend_from_slice(b"/**/*");
    let mut command = cmd(repo, idx);
    command
        .env_remove("GIT_LITERAL_PATHSPECS")
        .env_remove("GIT_GLOB_PATHSPECS")
        .env_remove("GIT_NOGLOB_PATHSPECS")
        .env_remove("GIT_ICASE_PATHSPECS")
        .args(args)
        .arg("--")
        .arg(include);
    if !index_read {
        command.arg(raw(&exclude));
    }
    let output = checked(command.output()?)?;
    if !index_read {
        return Ok(output);
    }
    // Filter NUL-delimited index records by native path, including all stages.
    // This also handles a selected deletion whose path is now a directory.
    let staged = args.contains(&"--stage");
    Ok(output
        .split(|&b| b == 0)
        .filter(|record| {
            let path = if staged {
                record
                    .iter()
                    .position(|&b| b == b'\t')
                    .map(|tab| &record[tab + 1..])
            } else {
                Some(*record)
            };
            path == Some(file.as_os_str().as_encoded_bytes())
        })
        .flat_map(|record| record.iter().copied().chain(std::iter::once(0)))
        .collect())
}
fn base_tree(repo: &Path, head: Option<&str>) -> Result<String> {
    if let Some(h) = head {
        Ok(txt(g(
            repo,
            None,
            &["rev-parse", &format!("{h}^{{tree}}")],
        )?))
    } else {
        Ok(txt(checked(run(
            repo,
            None,
            &["mktree".as_ref()],
            Some(&[]),
        )?)?))
    }
}
fn tree_patch(repo: &Path, base: &str, tree: &str) -> Result<Vec<u8>> {
    g(
        repo,
        None,
        &[
            "diff",
            "--no-ext-diff",
            "--no-textconv",
            "--no-color",
            "--no-renames",
            "--ignore-submodules=none",
            "--submodule=short",
            base,
            tree,
            "--",
        ],
    )
}
/// Build the exact snapshot displayed in review. Use real Git's filters, modes,
/// ignore rules and staged blob OIDs. Never stage the user's live worktree here.
fn snapshot(repo: &Path, files: &[Change], base: &str, di: &TempDir) -> Result<(PathBuf, String)> {
    let at = di.path().join("selected");
    g(repo, Some(&at), &["read-tree", base])?;
    for f in files {
        if f.conflict {
            bail!("Unresolved: {}", f.label())
        }
        if f.staged() {
            // The cached version wins, even for MM/MD.
            let entries = exact_path(repo, None, &["ls-files", "--stage", "-z"], &f.path)?;
            change_idx(repo, &at, &["update-index", "--force-remove"], &f.path)?;
            if !entries.is_empty() {
                checked(run(
                    repo,
                    Some(&at),
                    &[
                        "update-index".as_ref(),
                        "-z".as_ref(),
                        "--index-info".as_ref(),
                    ],
                    Some(&entries),
                )?)?;
            }
        } else {
            if f.xy == *b"??"
                && repo
                    .join(&f.path)
                    .symlink_metadata()
                    .is_ok_and(|m| m.is_dir())
            {
                bail!(
                    "{} is a nested repository or directory; commit in its own repository",
                    f.label()
                )
            }
            exact_path(repo, Some(&at), &["add", "-A"], &f.path)?;
        }
    }
    let tree = txt(g(repo, Some(&at), &["write-tree"])?);
    check_tree_scope(repo, base, &tree, files)?;
    Ok((at, tree))
}
// Adding a leaf can still evict ancestors/descendants. Require every changed
// leaf to be selected, and every selected version to survive (including a
// staged edit which a selected working child could turn into a deletion).
fn check_tree_scope(repo: &Path, base: &str, tree: &str, files: &[Change]) -> Result<()> {
    let mut expected: BTreeMap<_, _> = files
        .iter()
        .map(|f| {
            (
                f.path.as_os_str().as_encoded_bytes(),
                if f.staged() { f.xy[0] } else { f.xy[1] } == b'D',
            )
        })
        .collect();
    let changed = g(
        repo,
        None,
        &[
            "diff-tree",
            "-r",
            "-z",
            "--no-commit-id",
            "--name-status",
            "--no-renames",
            "--ignore-submodules=none",
            base,
            tree,
            "--",
        ],
    )?;
    let mut fields = changed.split(|b| *b == 0);
    while let Some(kind) = fields.next().filter(|s| !s.is_empty()) {
        let path = fields
            .next()
            .context("Git returned a file change without a path")?;
        let Some(deleted) = expected.remove(path) else {
            bail!(
                "Replacing a file with a directory or vice versa also affects unselected path {}. Select all affected paths and review again",
                super::display_path(Path::new(&raw(path)))
            )
        };
        if deleted != (kind == b"D") {
            bail!(
                "The selected versions cannot coexist in one commit: {}. Check file/directory replacements, refresh, and review again",
                super::display_path(Path::new(&raw(path)))
            )
        }
    }
    if let Some((path, _)) = expected.first_key_value() {
        bail!(
            "The snapshot has no changes for selected path {}. Check your selection, refresh, and review again",
            super::display_path(Path::new(&raw(path)))
        )
    }
    Ok(())
}
#[derive(Clone, Debug)]
pub struct Plan {
    pub root: usize,
    pub repo: PathBuf,
    pub files: Vec<Change>,
    pub msg: String,
    pub head: Option<String>,
    pub tree: String,
    pub preview: String,
    pub branch: String,
    pub amend: bool,
    stages: Vec<Vec<u8>>,
}
fn stages(repo: &Path, files: &[Change]) -> Result<Vec<Vec<u8>>> {
    files
        .iter()
        .map(|f| exact_path(repo, None, &["ls-files", "--stage", "-z"], &f.path))
        .collect()
}
fn check_sources(repo: &Path, files: &[Change]) -> Result<()> {
    let live: BTreeMap<_, _> = status(repo)?
        .into_iter()
        .map(|x| (x.path.clone(), x))
        .collect();
    if files.iter().any(|f| {
        !f.selectable()
            || live
                .get(&f.path)
                .is_none_or(|x| x.conflict || x.staged() != f.staged())
    }) {
        bail!("Files or staging state changed. Press F5 and review again")
    }
    Ok(())
}
pub fn plan(
    root: usize,
    repo: &Path,
    files: Vec<Change>,
    msg: String,
    amend: bool,
) -> Result<Plan> {
    if msg.trim().is_empty() {
        bail!("Commit message is required")
    }
    if files.is_empty() {
        bail!("No files selected")
    }
    safe(repo)?;
    check_sources(repo, &files)?;
    let head = oid(repo)?;
    if amend && head.is_none() {
        bail!("No commit to amend")
    }
    if amend
        && !g(
            repo,
            None,
            &[
                "for-each-ref",
                "--contains",
                "HEAD",
                "--format=%(refname)",
                "refs/remotes",
            ],
        )?
        .is_empty()
    {
        bail!("Cannot amend: HEAD is already reachable from a remote-tracking ref")
    }
    let idx = git_path(repo, "index")?;
    let d = tempfile::Builder::new()
        .prefix("igit-review-")
        .tempdir_in(idx.parent().context("missing git directory")?)?;
    let old = base_tree(repo, head.as_deref())?;
    let (_, tree) = snapshot(repo, &files, &old, &d)?;
    if tree == old && !amend {
        bail!("No committable changes selected. If a submodule only has uncommitted files, commit them in the submodule first")
    }
    // Derive review text from the exact two trees, not from a possibly stale
    // render-time worktree diff. Keep the row summary visible above patches.
    let bytes = tree_patch(repo, &old, &tree)?;
    let patch = String::from_utf8_lossy(&bytes);
    let mut preview = String::new();
    for (i, l) in patch.lines().enumerate() {
        if i >= 500 || preview.len() > 48000 {
            preview.push_str("… Preview truncated; see the file list above for remaining paths.");
            break;
        }
        preview.extend(l.chars().take(1800));
        if l.chars().nth(1800).is_some() {
            preview.push('…')
        }
        preview.push('\n');
    }
    let stages = stages(repo, &files)?;
    Ok(Plan {
        root,
        repo: repo.into(),
        files,
        msg,
        head,
        tree,
        amend,
        branch: branch_ref(repo)?,
        preview,
        stages,
    })
}
fn branch_ref(repo: &Path) -> Result<String> {
    let v = run(
        repo,
        None,
        &["symbolic-ref".as_ref(), "-q".as_ref(), "HEAD".as_ref()],
        None,
    )?;
    if v.status.success() {
        Ok(txt(v.stdout))
    } else if v.status.code() == Some(1) {
        Ok("HEAD".into())
    } else {
        Err(anyhow!("Cannot verify the current branch"))
    }
}
struct Guard(PathBuf, bool);
impl Drop for Guard {
    fn drop(&mut self) {
        if !self.1 {
            let _ = fs::remove_file(&self.0);
        }
    }
}
pub struct Outcome {
    pub hash: String,
    pub warning: Option<String>,
}
// An unselected, staged file may only exist in the real index, not in HEAD.
// Git can implicitly drop that entry to make room for a conflicting file or
// directory. Check the index copy before publishing; include intent-to-add
// entries, which don't necessarily appear in a tree.
fn reset_index_paths<'a>(
    repo: &Path,
    index: &Path,
    tree: &str,
    paths: impl IntoIterator<Item = &'a Path>,
) -> Result<()> {
    let entries = || g(repo, Some(index), &["ls-files", "--stage", "-z"]);
    let before = entries()?;
    let mut allowed = BTreeSet::new();
    for path in paths {
        exact_path(repo, Some(index), &["reset", "-q", tree], path)?;
        allowed.insert(path.as_os_str().as_encoded_bytes());
    }
    let after = entries()?;
    let unchecked = |record: &&[u8]| {
        record
            .iter()
            .position(|b| *b == b'\t')
            .is_some_and(|tab| !allowed.contains(&record[tab + 1..]))
    };
    if before
        .split(|b| *b == 0)
        .filter(unchecked)
        .ne(after.split(|b| *b == 0).filter(unchecked))
    {
        bail!("Replacing a file with a directory or vice versa would overwrite unselected staged entries. Check the real index and review again")
    }
    Ok(())
}
pub fn execute(p: &Plan) -> Result<Outcome> {
    let repo = &p.repo;
    safe(repo)?;
    let idx = git_path(repo, "index")?;
    let lock = idx.with_file_name("index.lock");
    let mut lockfile = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&lock)
        .context("Cannot lock the Git index; wait for other operations to finish")?;
    let mut guard = Guard(lock.clone(), false);
    if stages(repo, &p.files)? != p.stages {
        bail!("Staged content changed. Review again")
    }
    if oid(repo)? != p.head || branch_ref(repo)? != p.branch {
        bail!("Branch or HEAD changed. Review again")
    }
    let d = tempfile::Builder::new()
        .prefix("igit-commit-")
        .tempdir_in(idx.parent().unwrap())?;
    if p.amend
        && !g(
            repo,
            None,
            &[
                "for-each-ref",
                "--contains",
                "HEAD",
                "--format=%(refname)",
                "refs/remotes",
            ],
        )?
        .is_empty()
    {
        bail!("HEAD is now reachable from a remote-tracking ref. Review again")
    }
    let before = base_tree(repo, p.head.as_deref())?;
    let staged_before = g(
        repo,
        None,
        &[
            "diff",
            "--cached",
            "--name-only",
            "--no-renames",
            "--ignore-submodules=none",
            "-z",
            &before,
        ],
    )?
    .split(|x| *x == 0)
    .filter(|x| !x.is_empty())
    .map(|x| PathBuf::from(raw(x)))
    .collect::<std::collections::BTreeSet<_>>();
    let (at, tree) = snapshot(repo, &p.files, &before, &d)?;
    if tree != p.tree {
        bail!("Selected content changed. Refresh and review again")
    }
    // Start the next real index from the *current* live index, which is locked.
    // Replace selected entries only; every other staged path stays untouched.
    let next = d.path().join("next");
    if idx.exists() {
        fs::copy(&idx, &next)?;
    } else {
        g(repo, Some(&next), &["read-tree", "--empty"])?;
    }
    reset_index_paths(repo, &next, &tree, p.files.iter().map(|f| f.path.as_path()))?;
    // Prepare the real index replacement before committing; only a rename remains
    // after Git updates HEAD. An interrupted publish leaves Git's recovery lock.
    if idx.exists() {
        lockfile.set_permissions(fs::metadata(&idx)?.permissions())?;
    }
    lockfile.write_all(&fs::read(&next)?)?;
    lockfile.sync_all()?;
    drop(lockfile);
    let mut aa = vec![OsStr::new("commit")];
    if p.amend {
        aa.push(OsStr::new("--amend"))
    }
    aa.extend([OsStr::new("--file=-"), OsStr::new("--cleanup=verbatim")]);
    checked(run(repo, Some(&at), &aa, Some(p.msg.as_bytes()))?)?;
    guard.1 = true;
    let hash = txt(g(repo, None, &["rev-parse", "--short", "HEAD"])
        .context("Commit created, but HEAD could not be read. Keep index.lock; do not retry")?);
    let finalized = (|| -> Result<Option<String>> {
        let actual = txt(g(repo, None, &["rev-parse", "HEAD^{tree}"])?);
        if actual == tree {
            return Ok(None);
        }
        // A formatter or other commit hook may update its private index. Align
        // selected files and originally-clean paths to the actual commit; keep
        // unrelated, previously staged blobs exactly as the user left them.
        let changed = g(
            repo,
            None,
            &[
                "diff",
                "--name-only",
                "--no-renames",
                "--ignore-submodules=none",
                "-z",
                &tree,
                &actual,
            ],
        )?;
        let mut names = Vec::new();
        let mut reset = Vec::new();
        for bytes in changed.split(|b| *b == 0).filter(|x| !x.is_empty()) {
            let path = PathBuf::from(raw(bytes));
            names.push(path.to_string_lossy().replace(['\n', '\r'], " "));
            if !staged_before.contains(&path) || p.files.iter().any(|x| x.path == path) {
                reset.push(path);
            }
        }
        reset_index_paths(repo, &next, &actual, reset.iter().map(|p| p.as_path()))?;
        let mut replacement = tempfile::NamedTempFile::new_in(idx.parent().unwrap())?;
        replacement
            .as_file()
            .set_permissions(fs::metadata(&lock)?.permissions())?;
        replacement.write_all(&fs::read(&next)?)?;
        replacement.as_file().sync_all()?;
        replacement.persist(&lock)?;
        Ok(Some(format!(
            "A hook changed the committed content: {}. Review before pushing",
            names.join(", ")
        )))
    })()
    .with_context(|| {
        format!("Commit {hash} created, but the real index could not be reconciled. Keep index.lock; do not commit again")
    })?;
    fs::rename(&lock, &idx).context(
        "Commit created, but publishing the index failed. Keep index.lock; do not retry the commit",
    )?;
    Ok(Outcome {
        hash,
        warning: finalized,
    })
}
pub fn diff(repo: &Path, f: &Change) -> Result<String> {
    if f.ignored() {
        return Ok("Ignored by Git · read-only\nExcluded from commits and AI patches.".into());
    }
    if f.conflict {
        return Ok("Unmerged path · open the Conflicts tab to resolve it".into());
    }
    if f.xy == *b"??" {
        let path = repo.join(&f.path);
        if path.is_file() && !path.is_symlink() {
            use std::io::Read;
            const LIMIT: usize = 96 * 1024;
            let mut content = Vec::new();
            fs::File::open(&path)?
                .take((LIMIT + 1) as u64)
                .read_to_end(&mut content)?;
            if content.contains(&0) {
                return Ok("Binary new file".into());
            }
            let truncated = content.len() > LIMIT;
            if truncated {
                let mut end = LIMIT;
                while end > 0 && content[end] & 0xc0 == 0x80 {
                    end -= 1;
                }
                content.truncate(end);
            }
            let mut preview = format!(
                "+++ {} (untracked)\n{}",
                f.label(),
                String::from_utf8_lossy(&content)
                    .lines()
                    .map(|x| format!("+{x}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            );
            if truncated {
                preview.push_str("\n\n… Preview truncated at 96 KiB. Open the file to inspect the rest; committing includes the full selected file.");
            }
            return Ok(preview);
        }
        return Ok(format!("New path: {}", f.label()));
    }
    let mut args = vec![
        "--no-optional-locks",
        "diff",
        "--no-ext-diff",
        "--no-textconv",
        "--no-color",
        "--no-renames",
        "--ignore-submodules=none",
        "--submodule=short",
    ];
    if f.staged() {
        args.push("--cached")
    }
    Ok(txt(exact_path(repo, None, &args, &f.path)?))
}
pub fn patch(repo: &Path, files: &[Change]) -> Result<String> {
    check_sources(repo, files)?;
    let h = oid(repo)?;
    let idx = git_path(repo, "index")?;
    let di = tempfile::Builder::new()
        .prefix("igit-patch-")
        .tempdir_in(idx.parent().unwrap())?;
    let base = base_tree(repo, h.as_deref())?;
    let (_, tree) = snapshot(repo, files, &base, &di)?;
    let b = tree_patch(repo, &base, &tree)?;
    if b.is_empty() {
        bail!("No patch for the selected files")
    }
    if b.len() > 160_000 {
        bail!("Selected patch exceeds 160 KB. Select fewer files before generating a message")
    }
    Ok(String::from_utf8_lossy(&b).into_owned())
}
#[cfg(test)]
mod tests {
    use super::*;
    pub(super) fn setup() -> TempDir {
        let d = TempDir::new().unwrap();
        let r = d.path();
        for args in [
            &["init", "-q"][..],
            &["config", "user.name", "Fixture"],
            &["config", "user.email", "fixture@invalid.test"],
            &["config", "commit.gpgsign", "false"],
            &["config", "core.hooksPath", ".git/hooks"],
        ] {
            g(r, None, args).unwrap();
        }
        d
    }
    pub(super) fn put(r: &Path, n: &str, s: &str) {
        fs::write(r.join(n), s).unwrap()
    }
    pub(super) fn commit_all(r: &Path) {
        g(r, None, &["add", "-A"]).unwrap();
        g(r, None, &["commit", "-qm", "base"]).unwrap();
    }
    fn p(r: &Path, paths: &[&str]) -> Plan {
        let selected = status(r)
            .unwrap()
            .into_iter()
            .filter(|f| paths.iter().any(|s| Path::new(s) == f.path))
            .collect();
        plan(0, r, selected, "selected".into(), false).unwrap()
    }
    pub(super) fn get(r: &Path, args: &[&str]) -> String {
        txt(g(r, None, args).unwrap())
    }

    #[test]
    fn untracked_preview_announces_truncation_and_keeps_unicode_whole() {
        let dir = setup();
        let repo = dir.path();
        const LIMIT: usize = 96 * 1024;
        for length in [LIMIT - 1, LIMIT, LIMIT + 1] {
            let bytes = vec![b'a'; length];
            fs::write(repo.join("new.txt"), &bytes).unwrap();
            let file = status(repo)
                .unwrap()
                .into_iter()
                .find(|change| change.path == Path::new("new.txt"))
                .unwrap();
            let preview = diff(repo, &file).unwrap();
            assert_eq!(preview.contains("Preview truncated"), length > LIMIT);
            assert_eq!(fs::read(repo.join("new.txt")).unwrap(), bytes);
        }
        let text = format!("{}中文 tail", "a".repeat(LIMIT - 1));
        fs::write(repo.join("new.txt"), &text).unwrap();
        let file = status(repo)
            .unwrap()
            .into_iter()
            .find(|change| change.path == Path::new("new.txt"))
            .unwrap();
        let preview = diff(repo, &file).unwrap();
        assert!(preview.contains("Preview truncated at 96 KiB"));
        assert!(!preview.contains('\u{fffd}'));
        assert!(!preview.contains("tail"));
        assert!(get(repo, &["ls-files", "--stage"]).is_empty());
        assert_eq!(fs::read_to_string(repo.join("new.txt")).unwrap(), text);

        fs::write(repo.join("new.txt"), b"binary\0content").unwrap();
        assert_eq!(diff(repo, &file).unwrap(), "Binary new file");
    }

    #[test]
    fn ignored_paths_are_collapsed_read_only_and_excluded_from_plans() {
        let d = setup();
        let r = d.path();
        put(r, ".gitignore", "cache/\n*.log\n");
        put(r, ".git/info/exclude", "local.tmp\n");
        fs::create_dir(r.join("cache")).unwrap();
        put(r, "cache/private.txt", "ignored content marker");
        put(r, "app.log", "ignored content marker");
        put(r, "local.tmp", "local only");
        let ignored = ignored(r).unwrap();
        assert_eq!(
            ignored
                .iter()
                .map(|f| f.path.as_path())
                .collect::<BTreeSet<_>>(),
            ["app.log", "cache/", "local.tmp"].map(Path::new).into()
        );
        for change in &ignored {
            assert!(change.ignored() && !change.staged() && !change.selectable());
            assert!(!diff(r, change).unwrap().contains("ignored content marker"));
        }
        assert!(ignored
            .iter()
            .find(|f| f.path == Path::new("cache"))
            .unwrap()
            .directory());
        assert!(plan(0, r, ignored, "ignored".into(), false).is_err());
        assert_eq!(status(r).unwrap().len(), 1); // Only .gitignore is untracked.
        assert!(g(r, None, &["ls-files", "--stage"]).unwrap().is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn nested_literal_paths_keep_the_selected_staged_blob() {
        let d = setup();
        let r = d.path();
        let paths = [
            PathBuf::from("src/[literal]*?/ｕｉ/selected.rs"),
            PathBuf::from(r"src/[literal]*?/back\slash.rs"),
            #[cfg(not(target_os = "macos"))]
            PathBuf::from(raw(b"src/[literal]*?/bad\xff.rs")),
        ];
        for path in &paths {
            fs::create_dir_all(r.join(path).parent().unwrap()).unwrap();
            fs::write(r.join(path), "base\n").unwrap();
        }
        commit_all(r);
        for path in &paths {
            fs::write(r.join(path), "selected staged\n").unwrap();
        }
        g(r, None, &["add", "-A"]).unwrap();
        for path in &paths {
            fs::write(r.join(path), "selected working\n").unwrap();
        }
        for path in &paths {
            let selected = status(r)
                .unwrap()
                .into_iter()
                .find(|f| &f.path == path)
                .unwrap();
            assert_eq!(selected.xy, *b"MM");
            let plan = plan(0, r, vec![selected], "literal path".into(), false).unwrap();
            assert!(plan.preview.contains("+selected staged"));
            assert!(!plan.preview.contains("selected working"));
            assert_eq!(
                exact_path(r, None, &["ls-files", "-z"], path).unwrap(),
                [path.as_os_str().as_encoded_bytes(), b"\0"].concat()
            );
        }
    }

    #[test]
    fn mixed_and_unrelated_index_survive() {
        let d = setup();
        let r = d.path();
        for n in ["a", "b", "c"] {
            put(r, n, "base\n")
        }
        commit_all(r);
        put(r, "a", "selected staged\n");
        put(r, "b", "keep staged\n");
        g(r, None, &["add", "a", "b"]).unwrap();
        put(r, "a", "later work\n");
        put(r, "c", "selected working\n");
        let plan = p(r, &["a", "c"]);
        execute(&plan).unwrap();
        assert_eq!(get(r, &["show", "HEAD:a"]), "selected staged");
        assert_eq!(get(r, &["show", "HEAD:b"]), "base");
        assert_eq!(get(r, &["show", "HEAD:c"]), "selected working");
        assert_eq!(get(r, &["show", ":b"]), "keep staged");
        assert_eq!(get(r, &["diff", "--cached", "--name-only"]), "b");
        assert_eq!(get(r, &["diff", "--name-only"]), "a");
    }
    #[test]
    fn untracked_first_commit_and_stale_review() {
        let d = setup();
        let r = d.path();
        let name = if cfg!(windows) { "a[x]" } else { "a*[x]" };
        put(r, name, "hello\n");
        let before = p(r, &[name]);
        put(r, name, "new\n");
        assert!(execute(&before).is_err());
        assert!(oid(r).unwrap().is_none());
        let after = p(r, &[name]);
        execute(&after).unwrap();
        assert_eq!(get(r, &["show", &format!("HEAD:{name}")]), "new");
        assert!(status(r).unwrap().is_empty());
    }
    fn change(r: &Path, path: &str) -> Change {
        status(r)
            .unwrap()
            .into_iter()
            .find(|f| f.path == Path::new(path))
            .unwrap()
    }
    #[test]
    fn path_scope_file_deletion_does_not_take_an_unchecked_directory_child() {
        for staged_child in [false, true] {
            let d = setup();
            let r = d.path();
            // Pathspec metacharacters are part of this file's real name.
            let node = "node[1]";
            let child = "node[1]/private";
            put(r, node, "old leaf\n");
            commit_all(r);
            fs::remove_file(r.join(node)).unwrap();
            fs::create_dir(r.join(node)).unwrap();
            put(r, child, "private staged version\n");
            if staged_child {
                g(r, None, &["add", "-A"]).unwrap();
            }
            put(r, child, "private working version\n");
            let selected = vec![change(r, node)];
            let head = oid(r).unwrap();
            let index = fs::read(r.join(".git/index")).unwrap();
            let preview = diff(r, &selected[0]).unwrap();
            let ai = patch(r, &selected).unwrap();
            let plan = plan(0, r, selected, "delete old leaf".into(), false).unwrap();
            for (label, text) in [
                ("file preview", preview),
                ("AI patch", ai),
                ("commit review", plan.preview.clone()),
            ] {
                assert!(text.contains("-old leaf"), "{label}: {text}");
                assert!(
                    !text.contains("private"),
                    "{label} exposed the unchecked child: {text}"
                );
            }
            assert_eq!(oid(r).unwrap(), head);
            assert_eq!(fs::read(r.join(".git/index")).unwrap(), index);
            execute(&plan).unwrap();
            assert_eq!(
                get(
                    r,
                    &[
                        "diff-tree",
                        "--no-commit-id",
                        "--name-status",
                        "-r",
                        "--no-renames",
                        "HEAD"
                    ]
                ),
                format!("D\t{node}")
            );
            assert_eq!(get(r, &["ls-tree", "-r", "--name-only", "HEAD"]), "");
            assert_eq!(
                fs::read_to_string(r.join(child)).unwrap(),
                "private working version\n"
            );
            if staged_child {
                assert_eq!(
                    get(r, &["show", ":node[1]/private"]),
                    "private staged version"
                );
                assert_eq!(get(r, &["diff", "--cached", "--name-only"]), child);
            } else {
                assert!(get(r, &["ls-files"]).is_empty());
                assert_eq!(change(r, child).xy, *b"??");
            }
        }
    }
    #[test]
    fn path_scope_directory_to_file_requires_its_tracked_children() {
        let d = setup();
        let r = d.path();
        fs::create_dir(r.join("node")).unwrap();
        put(r, "node/a", "old a\n");
        put(r, "node/b", "old b\n");
        commit_all(r);
        fs::remove_dir_all(r.join("node")).unwrap();
        put(r, "node", "replacement\n");
        let head = oid(r).unwrap();
        let index = fs::read(r.join(".git/index")).unwrap();
        let only_new = vec![change(r, "node")];
        assert!(
            patch(r, &only_new).is_err(),
            "AI must not include the unchecked deletions"
        );
        assert!(plan(0, r, only_new, "partial".into(), false).is_err());
        assert_eq!(oid(r).unwrap(), head);
        assert_eq!(fs::read(r.join(".git/index")).unwrap(), index);
        assert_eq!(fs::read_to_string(r.join("node")).unwrap(), "replacement\n");
        let all = p(r, &["node", "node/a", "node/b"]);
        execute(&all).unwrap();
        assert_eq!(get(r, &["ls-tree", "-r", "--name-only", "HEAD"]), "node");
        assert_eq!(get(r, &["show", "HEAD:node"]), "replacement");
        assert!(status(r).unwrap().is_empty());
    }
    #[test]
    fn path_scope_cannot_select_a_staged_file_and_a_working_child_together() {
        let d = setup();
        let r = d.path();
        put(r, "node", "old\n");
        commit_all(r);
        put(r, "node", "reviewed staged edit\n");
        g(r, None, &["add", "node"]).unwrap();
        fs::remove_file(r.join("node")).unwrap();
        fs::create_dir(r.join("node")).unwrap();
        put(r, "node/new", "working child\n");
        let both = vec![change(r, "node"), change(r, "node/new")];
        assert_eq!(both[0].xy, *b"MD");
        let head = oid(r).unwrap();
        let index = fs::read(r.join(".git/index")).unwrap();
        // The actual diff can name BOTH paths yet delete the selected staged edit.
        assert!(patch(r, &both).is_err());
        assert!(plan(0, r, both, "impossible selection".into(), false).is_err());
        assert_eq!(oid(r).unwrap(), head);
        assert_eq!(fs::read(r.join(".git/index")).unwrap(), index);
        assert_eq!(get(r, &["show", ":node"]), "reviewed staged edit");
        assert_eq!(
            fs::read_to_string(r.join("node/new")).unwrap(),
            "working child\n"
        );
    }
    #[test]
    fn path_scope_working_file_cannot_displace_an_unchecked_staged_child() {
        let d = setup();
        let r = d.path();
        put(r, "base", "base\n");
        commit_all(r);
        fs::create_dir(r.join("node")).unwrap();
        put(r, "node/child", "private staged child\n");
        g(r, None, &["add", "node/child"]).unwrap();
        fs::remove_dir_all(r.join("node")).unwrap();
        put(r, "node", "new working file\n");
        let head = oid(r).unwrap();
        let index = fs::read(r.join(".git/index")).unwrap();
        let result = plan(0, r, vec![change(r, "node")], "keep child".into(), false)
            .and_then(|p| execute(&p));
        assert!(
            result.is_err(),
            "publishing the new file must not discard the staged child"
        );
        assert_eq!(oid(r).unwrap(), head);
        assert_eq!(fs::read(r.join(".git/index")).unwrap(), index);
        assert_eq!(get(r, &["show", ":node/child"]), "private staged child");
        assert_eq!(
            fs::read_to_string(r.join("node")).unwrap(),
            "new working file\n"
        );
    }
    #[cfg(unix)]
    #[test]
    fn rejected_hook_preserves_head_index_worktree() {
        use std::os::unix::fs::PermissionsExt;
        let d = setup();
        let r = d.path();
        put(r, "one", "one");
        commit_all(r);
        put(r, "one", "two");
        put(r, "unrelated", "x");
        g(r, None, &["add", "unrelated"]).unwrap();
        let idx = fs::read(r.join(".git/index")).unwrap();
        let head = oid(r).unwrap();
        put(r, ".git/hooks/pre-commit", "#!/bin/sh\nexit 2\n");
        fs::set_permissions(
            r.join(".git/hooks/pre-commit"),
            fs::Permissions::from_mode(0o700),
        )
        .unwrap();
        let pp = p(r, &["one"]);
        assert!(execute(&pp).is_err());
        assert_eq!(oid(r).unwrap(), head);
        assert_eq!(fs::read(r.join(".git/index")).unwrap(), idx);
        assert_eq!(fs::read_to_string(r.join("one")).unwrap(), "two");
        assert!(!r.join(".git/index.lock").exists())
    }
}
#[cfg(all(test, unix))]
mod hook_change_tests {
    use super::*;
    #[test]
    fn report_hook_mutations_without_spurious_reverse_stage() {
        use std::os::unix::fs::PermissionsExt;
        let d = tempfile::tempdir().unwrap();
        let r = d.path();
        for args in [
            &["init", "-q"][..],
            &["config", "user.name", "Fixture"],
            &["config", "user.email", "fixture@invalid.test"],
            &["config", "core.hooksPath", ".git/hooks"],
            &["config", "commit.gpgsign", "false"],
        ] {
            g(r, None, args).unwrap();
        }
        for n in ["task", "meta", "unrelated"] {
            fs::write(r.join(n), "original").unwrap()
        }
        g(r, None, &["add", "-A"]).unwrap();
        g(r, None, &["commit", "-qm", "base"]).unwrap();
        fs::write(r.join("unrelated"), "keep this staged").unwrap();
        g(r, None, &["add", "unrelated"]).unwrap();
        fs::write(r.join("task"), "edit").unwrap();
        let hook = r.join(".git/hooks/pre-commit");
        fs::write(
            &hook,
            "#!/bin/sh\nprintf formatted > task\nprintf generated > meta\ngit add -- task meta\n",
        )
        .unwrap();
        fs::set_permissions(hook, fs::Permissions::from_mode(0o700)).unwrap();
        let proposal = plan(
            0,
            r,
            status(r)
                .unwrap()
                .into_iter()
                .filter(|x| x.path == Path::new("task"))
                .collect(),
            "review".into(),
            false,
        )
        .unwrap();
        let result = execute(&proposal).unwrap();
        assert!(result.warning.unwrap().contains("meta"));
        assert_eq!(
            txt(g(r, None, &["show", "HEAD:task"]).unwrap()),
            "formatted"
        );
        assert_eq!(txt(g(r, None, &["show", ":meta"]).unwrap()), "generated");
        assert_eq!(
            txt(g(r, None, &["diff", "--cached", "--name-only"]).unwrap()),
            "unrelated"
        );
    }
}
#[cfg(test)]
mod submodule_tests {
    use super::*;
    fn init(p: &Path) {
        fs::create_dir(p).unwrap();
        for args in [
            &["init", "-q"][..],
            &["config", "user.name", "Fixture"],
            &["config", "user.email", "fixture@invalid.test"],
            &["config", "core.hooksPath", ".git/hooks"],
            &["config", "commit.gpgsign", "false"],
        ] {
            g(p, None, args).unwrap();
        }
    }
    #[test]
    fn child_dirt_is_not_a_parent_patch_but_gitlink_updates_are() {
        let temp = tempfile::tempdir().unwrap();
        let src = temp.path().join("source");
        let par = temp.path().join("parent");
        init(&src);
        init(&par);
        fs::write(src.join("file"), "base").unwrap();
        g(&src, None, &["add", "file"]).unwrap();
        g(&src, None, &["commit", "-qm", "base"]).unwrap();
        g(
            &par,
            None,
            &[
                "-c",
                "protocol.file.allow=always",
                "submodule",
                "add",
                "-q",
                src.to_str().unwrap(),
                "sdk",
            ],
        )
        .unwrap();
        g(&par, None, &["commit", "-qm", "add sdk"]).unwrap();
        let child = par.join("sdk");
        fs::write(child.join("file"), "new").unwrap();
        fs::write(child.join("untracked"), "extra").unwrap();
        assert!(status(&par).unwrap().is_empty());
        assert_eq!(status(&child).unwrap().len(), 2);
        g(
            &child,
            None,
            &[
                "-c",
                "commit.gpgsign=false",
                "-c",
                "core.hooksPath=/dev/null",
                "-c",
                "user.name=Fixture",
                "-c",
                "user.email=test@invalid.test",
                "commit",
                "-qam",
                "child work",
            ],
        )
        .unwrap();
        let ptr = status(&par).unwrap();
        assert_eq!(ptr.len(), 1);
        assert_eq!(ptr[0].path, Path::new("sdk"));
        assert!(!ptr[0].staged());
        let p = plan(0, &par, ptr, "new child revision".into(), false).unwrap();
        assert!(!p.tree.is_empty());
        g(&par, None, &["add", "sdk"]).unwrap();
        let staged = status(&par).unwrap();
        assert_eq!(staged.len(), 1);
        assert!(staged[0].staged());
        let next = plan(0, &par, staged, "new child revision".into(), false).unwrap();
        assert_eq!(next.tree, p.tree);
        execute(&next).unwrap();
        assert!(status(&par).unwrap().is_empty());
        assert_eq!(status(&child).unwrap().len(), 1);
        // Removing a gitlink must also keep the initialized child worktree.
        g(&par, None, &["rm", "--cached", "--", "sdk"]).unwrap();
        let selected = status(&par)
            .unwrap()
            .into_iter()
            .filter(|f| f.path == Path::new("sdk"))
            .collect();
        let removal = plan(0, &par, selected, "remove gitlink".into(), false).unwrap();
        execute(&removal).unwrap();
        assert!(g(&par, None, &["ls-tree", "HEAD", "--", "sdk"])
            .unwrap()
            .is_empty());
        assert_eq!(fs::read_to_string(child.join("file")).unwrap(), "new");
        assert_eq!(
            fs::read_to_string(child.join("untracked")).unwrap(),
            "extra"
        );
    }
}
