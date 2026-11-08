//! Review and apply one text hunk through a private index.
use super::working::{read as working, Working};
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Unstaged,
    Staged,
}

impl Side {
    pub fn action(self) -> &'static str {
        match self {
            Self::Unstaged => "Stage",
            Self::Staged => "Unstage",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Unstaged => "Unstaged",
            Self::Staged => "Staged",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hunk {
    pub text: String,
}

#[derive(Clone, Debug)]
pub struct Snapshot {
    pub root: usize,
    pub repo: PathBuf,
    pub path: PathBuf,
    pub side: Side,
    pub patch: String,
    pub hunks: Vec<Hunk>,
    captured: Captured,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Captured {
    root: usize,
    repo: PathBuf,
    path: PathBuf,
    side: Side,
    state: State,
    patch: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct State {
    index: PathBuf,
    head: Option<String>,
    branch: String,
    entry: Vec<u8>,
    working: Option<(String, u32)>,
    attributes: Vec<u8>,
    conversions: Vec<u8>,
}

pub struct Plan {
    pub root: usize,
    pub repo: PathBuf,
    pub path: PathBuf,
    pub side: Side,
    /// The selected change, in its original direction. Unstage reverses it.
    pub patch: String,
    snapshot: Snapshot,
    reviewed: String,
    result: Vec<u8>,
}

const DIFF: &[&str] = &[
    "--no-optional-locks",
    "-c",
    "core.fsmonitor=false",
    "-c",
    "core.quotePath=true",
    "diff",
    "--no-ext-diff",
    "--no-textconv",
    "--no-color",
    "--no-renames",
    "--full-index",
    "--no-relative",
    "--src-prefix=a/",
    "--dst-prefix=b/",
    "--diff-algorithm=myers",
    "--no-indent-heuristic",
    "--unified=3",
    "--inter-hunk-context=0",
    "--output-indicator-new=+",
    "--output-indicator-old=-",
    "--output-indicator-context= ",
    "--ignore-submodules=none",
    "--submodule=short",
];

fn entry(repo: &Path, index: Option<&Path>, path: &Path) -> Result<Vec<u8>> {
    exact_path(repo, index, &["ls-files", "--stage", "-z"], path)
}

// Return the regular file's mode and blob, or absence. Never interpret a path
// through UTF-8: only the ASCII metadata preceding its tab is parsed.
fn regular(record: &[u8]) -> Result<Option<(&str, &str)>> {
    if record.is_empty() {
        return Ok(None);
    }
    let tab = record
        .iter()
        .position(|&b| b == b'\t')
        .context("Invalid Git entry")?;
    let metadata = std::str::from_utf8(&record[..tab])?;
    let mut fields = metadata.split(' ');
    let mode = fields.next().context("Missing Git mode")?;
    let blob = fields.next().context("Missing Git blob")?;
    let stage = fields.next().context("Missing Git stage")?;
    if !matches!(mode, "100644" | "100755") || stage != "0" {
        bail!("Hunk staging requires an ordinary tracked file; use whole-file operations for symlinks, submodules, or conflicts");
    }
    if record[tab + 1..]
        .split(|&b| b == 0)
        .filter(|p| !p.is_empty())
        .count()
        != 1
    {
        bail!("Multiple index entries; resolve the conflict before staging hunks");
    }
    Ok(Some((mode, blob)))
}

fn text(bytes: &[u8]) -> Result<()> {
    if bytes.contains(&0) || std::str::from_utf8(bytes).is_err() {
        bail!("Hunk staging requires UTF-8 text; use whole-file staging for binary or other encodings");
    }
    Ok(())
}

fn head_entry(repo: &Path, head: Option<&str>, path: &Path) -> Result<Vec<u8>> {
    let Some(head) = head else {
        return Ok(Vec::new());
    };
    let bytes = checked(run(
        repo,
        None,
        &[
            "ls-tree".as_ref(),
            "-z".as_ref(),
            head.as_ref(),
            "--".as_ref(),
            path.as_os_str(),
        ],
        None,
    )?)?;
    for record in bytes.split(|&b| b == 0).filter(|r| !r.is_empty()) {
        let tab = record
            .iter()
            .position(|&b| b == b'\t')
            .context("Invalid tree entry")?;
        if &record[tab + 1..] == path.as_os_str().as_encoded_bytes() {
            let metadata = std::str::from_utf8(&record[..tab])?;
            let mut fields = metadata.split(' ');
            let mode = fields.next().context("Missing tree mode")?;
            if fields.next() != Some("blob") {
                bail!("Hunk staging does not support submodules or file/directory replacements; use whole-file operations");
            }
            let blob = fields.next().context("Missing tree blob")?;
            let mut result = format!("{mode} {blob} 0\t").into_bytes();
            result.extend(path.as_os_str().as_encoded_bytes());
            result.push(0);
            return Ok(result);
        }
    }
    Ok(Vec::new())
}

fn attributes(repo: &Path, path: &Path) -> Result<Vec<u8>> {
    let input = [path.as_os_str().as_encoded_bytes(), b"\0"].concat();
    let bytes = checked(run(
        repo,
        None,
        &[
            "check-attr".as_ref(),
            "-z".as_ref(),
            "--stdin".as_ref(),
            "text".as_ref(),
            "eol".as_ref(),
            "working-tree-encoding".as_ref(),
            "filter".as_ref(),
            "ident".as_ref(),
            "crlf".as_ref(),
            "diff".as_ref(),
        ],
        Some(&input),
    )?)?;
    let fields: Vec<_> = bytes.split(|&b| b == 0).filter(|p| !p.is_empty()).collect();
    if fields.len() != 21 {
        bail!("Cannot verify Git attributes for hunk staging");
    }
    for triple in fields.chunks_exact(3) {
        let (attribute, value) = (triple[1], triple[2]);
        let lf_rule = matches!(
            (attribute, value),
            (b"text", b"set" | b"auto") | (b"eol", b"lf") | (b"diff", b"set")
        );
        if value != b"unspecified" && value != b"unset" && !lf_rule {
            bail!("Hunk staging does not support the {} attribute; use whole-file staging with Git's conversions", String::from_utf8_lossy(attribute));
        }
        if attribute == b"diff" && value == b"unset" {
            bail!("Git marks this path as binary; use whole-file staging");
        }
    }
    Ok(bytes)
}

fn conversions(repo: &Path) -> Result<Vec<u8>> {
    let output = run(repo, None, &[
        "config".as_ref(), "--null".as_ref(), "--get-regexp".as_ref(),
        r"^(core\.(autocrlf|eol|safecrlf|checkroundtripencoding|filemode|symlinks|ignorecase|attributesfile)|filter\.|diff\.|apply\.|extensions\.)".as_ref(),
    ], None)?;
    if output.status.code() == Some(1) {
        return Ok(Vec::new());
    }
    checked(output)
}

// LF rules are harmless when every relevant version is already LF text and
// checkout keeps LF. Reject configurations requiring any byte conversion.
fn line_endings(attributes: &[u8], config: &[u8], contents: &[Vec<u8>]) -> Result<()> {
    let fields: Vec<_> = attributes.split(|&b| b == 0).collect();
    let attr = |name: &[u8]| {
        fields
            .chunks_exact(3)
            .find(|triple| triple[1] == name)
            .map(|triple| triple[2])
            .unwrap_or(b"unspecified")
    };
    let configured = |name: &[u8]| {
        config
            .split(|&b| b == 0)
            .filter_map(|record| {
                let newline = record
                    .iter()
                    .position(|&b| b == b'\n')
                    .unwrap_or(record.len());
                (&record[..newline] == name)
                    .then_some(record.get(newline + 1..).unwrap_or_default())
            })
            .next_back()
            .map(|value| value.to_ascii_lowercase())
    };
    let autocrlf = configured(b"core.autocrlf").unwrap_or_else(|| b"false".to_vec());
    let autocrlf_off = matches!(autocrlf.as_slice(), b"false" | b"no" | b"off" | b"0");
    let autocrlf_input = autocrlf == b"input";
    let autocrlf_on = matches!(autocrlf.as_slice(), b"true" | b"yes" | b"on" | b"1" | b"");
    if !autocrlf_off && !autocrlf_input && !autocrlf_on {
        bail!("Cannot verify core.autocrlf; correct the Git configuration before staging hunks");
    }
    let text = attr(b"text");
    let eol_lf = attr(b"eol") == b"lf";
    let active = text != b"unset" && (matches!(text, b"set" | b"auto") || eol_lf || !autocrlf_off);
    if !active {
        return Ok(());
    }
    let eol = configured(b"core.eol").unwrap_or_else(|| b"native".to_vec());
    let checkout_lf = eol_lf
        || autocrlf_input
        || (autocrlf_off && (eol == b"lf" || (eol == b"native" && !cfg!(windows))));
    if !checkout_lf || contents.iter().any(|bytes| bytes.contains(&b'\r')) {
        bail!("This path requires line-ending conversion; use whole-file staging, or review LF-only content with eol=lf");
    }
    Ok(())
}

fn reject_rename(repo: &Path, head: Option<&str>, path: &Path) -> Result<()> {
    let Some(head) = head else { return Ok(()) };
    let bytes = g(
        repo,
        None,
        &[
            "--no-optional-locks",
            "diff",
            "--cached",
            "--name-status",
            "-z",
            "--find-renames",
            "--no-ext-diff",
            "--no-textconv",
            head,
            "--",
        ],
    )?;
    let mut fields = bytes.split(|&b| b == 0).filter(|p| !p.is_empty());
    while let Some(kind) = fields.next() {
        let first = fields.next().context("Missing changed path")?;
        if matches!(kind.first(), Some(b'R' | b'C')) {
            let second = fields.next().context("Missing renamed path")?;
            if [first, second].contains(&path.as_os_str().as_encoded_bytes()) {
                bail!("Hunk staging does not support renamed paths; stage or unstage the whole rename");
            }
        }
    }
    Ok(())
}

fn state(repo: &Path, path: &Path) -> Result<State> {
    let work = match working(repo, path)? {
        Working::File { hash, mode } => Some((hash, mode)),
        Working::Missing => None,
        Working::Symlink(_) => {
            bail!("Hunk staging does not support symlinks; use whole-file staging")
        }
    };
    let head = oid(repo)?;
    let index = git_path(repo, "index")?;
    let entry = entry(repo, None, path)?;
    let base = head_entry(repo, head.as_deref(), path)?;
    let indexed = regular(&entry)?;
    let committed = regular(&base)?;
    if indexed.is_none() && committed.is_none() {
        bail!("Hunk staging requires a tracked text file; use Add to Git for untracked files");
    }
    if indexed.zip(committed).is_some_and(|(a, b)| a.0 != b.0) {
        bail!("File mode changed; stage or unstage the whole file before staging hunks");
    }
    if indexed.is_some() {
        let debug = checked(run(
            repo,
            None,
            &[
                "--no-optional-locks".as_ref(),
                "-c".as_ref(),
                "core.fsmonitor=false".as_ref(),
                "ls-files".as_ref(),
                "--debug".as_ref(),
                "-z".as_ref(),
                "--".as_ref(),
                path.as_os_str(),
            ],
            None,
        )?)?;
        let metadata = debug
            .splitn(2, |&b| b == 0)
            .nth(1)
            .context("Missing index flags")?;
        let metadata = std::str::from_utf8(metadata)?;
        let flags = metadata
            .rsplit_once("flags: ")
            .context("Missing index flags")?
            .1
            .trim();
        let flags = u32::from_str_radix(flags, 16).context("Invalid index flags")?;
        // These are Git's internal split-index and fsmonitor cache markers.
        // They do not hide content; the raw working bytes are checked anyway.
        if flags & !(0x0800_0000 | 0x0020_0000) != 0 {
            bail!("Index flags are unsupported for hunk staging; clear intent-to-add, assume-unchanged, or skip-worktree and review again");
        }
    }
    // Reject conversions before asking Git for a working-tree diff: a clean
    // filter is an arbitrary command and must not run during this operation.
    let attributes = attributes(repo, path)?;
    let conversions = conversions(repo)?;
    let mut contents = Vec::new();
    let blobs: BTreeSet<_> = indexed
        .into_iter()
        .chain(committed)
        .map(|(_, blob)| blob)
        .collect();
    for blob in blobs {
        contents.push(g(repo, None, &["cat-file", "blob", blob])?);
    }
    if work.is_some() {
        contents.push(fs::read(repo.join(path))?);
    }
    for bytes in &contents {
        text(bytes)?;
    }
    line_endings(&attributes, &conversions, &contents)?;
    reject_rename(repo, head.as_deref(), path)?;
    Ok(State {
        index,
        head,
        branch: branch_ref(repo)?,
        entry,
        working: work,
        attributes,
        conversions,
    })
}

fn split(patch: &str) -> Result<(&str, Vec<Hunk>)> {
    if patch.is_empty() {
        return Ok(("", Vec::new()));
    }
    let mut starts = Vec::new();
    let mut offset = 0;
    for line in patch.split_inclusive('\n') {
        if line.starts_with("@@ ") {
            starts.push(offset);
        } else if offset > 0 && line.starts_with("diff --git ") {
            bail!("A hunk patch affects multiple files; use whole-file staging");
        }
        offset += line.len();
    }
    let first = *starts.first().context(
        "No supported text hunks; use whole-file staging for mode-only or binary changes",
    )?;
    let header = &patch[..first];
    if header
        .lines()
        .any(|line| line.starts_with("old mode ") || line.starts_with("new mode "))
    {
        bail!("File mode changed; use whole-file staging");
    }
    starts.push(patch.len());
    let hunks = starts
        .windows(2)
        .map(|pair| Hunk {
            text: patch[pair[0]..pair[1]].into(),
        })
        .collect();
    Ok((header, hunks))
}

pub fn read(root: usize, repo: &Path, path: &Path, side: Side) -> Result<Snapshot> {
    safe(repo)?;
    let before = state(repo, path)?;
    let mut args = DIFF.to_vec();
    if side == Side::Staged {
        args.push("--cached");
        if let Some(head) = before.head.as_deref() {
            args.push(head);
        }
    }
    let patch = String::from_utf8(exact_path(repo, None, &args, path)?)
        .context("Hunk staging requires a UTF-8 patch; use whole-file staging")?;
    let (_, hunks) = split(&patch)?;
    if state(repo, path)? != before {
        bail!("File, staging, branch, or Git conversions changed while reading; refresh and review again");
    }
    Ok(Snapshot {
        root,
        repo: repo.into(),
        path: path.into(),
        side,
        patch: patch.clone(),
        hunks,
        captured: Captured {
            root,
            repo: repo.into(),
            path: path.into(),
            side,
            state: before,
            patch,
        },
    })
}

impl Snapshot {
    fn validate(&self) -> Result<()> {
        let c = &self.captured;
        if self.root != c.root
            || self.repo != c.repo
            || self.path != c.path
            || self.side != c.side
            || self.patch != c.patch
            || self.hunks != split(&c.patch)?.1
            || read(self.root, &self.repo, &self.path, self.side)?.captured != *c
        {
            bail!("File, staging, branch, or Git conversions changed; refresh and review the hunk again");
        }
        Ok(())
    }
}

fn private_index(repo: &Path, index: &Path, next: &Path) -> Result<()> {
    if index.exists() {
        fs::copy(index, next)?;
        // Publish a self-contained index even when the original uses split-index.
        g(repo, Some(next), &["update-index", "--no-split-index"])?;
    } else {
        g(repo, Some(next), &["read-tree", "--empty"])?;
    }
    Ok(())
}

fn apply(repo: &Path, index: &Path, side: Side, patch: &str) -> Result<()> {
    let mut args = vec![
        "-c",
        "core.splitIndex=false",
        "apply",
        "--cached",
        "--whitespace=nowarn",
    ];
    if side == Side::Staged {
        args.push("--reverse");
    }
    checked(run(
        repo,
        Some(index),
        &args.iter().map(OsStr::new).collect::<Vec<_>>(),
        Some(patch.as_bytes()),
    )?)?;
    Ok(())
}

// Verify both trees and index records: intent-to-add entries do not appear in
// trees, and a file/directory collision must never evict an unrelated entry.
fn changed(
    repo: &Path,
    next: &Path,
    side: Side,
    path: &Path,
    patch: &str,
) -> Result<(String, Vec<u8>)> {
    let before_entries = g(repo, Some(next), &["ls-files", "--stage", "-v", "-z"])?;
    let before_entry = entry(repo, Some(next), path)?;
    let before = txt(g(repo, Some(next), &["write-tree"])?);
    apply(repo, next, side, patch)?;
    let after = txt(g(repo, Some(next), &["write-tree"])?);
    let after_entry = entry(repo, Some(next), path)?;
    let after_entries = g(repo, Some(next), &["ls-files", "--stage", "-v", "-z"])?;
    let paths = g(
        repo,
        None,
        &[
            "diff-tree",
            "-r",
            "--no-commit-id",
            "--no-renames",
            "--name-only",
            "-z",
            &before,
            &after,
            "--",
        ],
    )?;
    if paths != [path.as_os_str().as_encoded_bytes(), b"\0"].concat() {
        bail!("The hunk has no effect or affects another path; refresh and review again");
    }
    let unrelated = |record: &&[u8]| {
        record
            .iter()
            .position(|&b| b == b'\t')
            .is_some_and(|tab| &record[tab + 1..] != path.as_os_str().as_encoded_bytes())
    };
    if before_entries
        .split(|&b| b == 0)
        .filter(unrelated)
        .ne(after_entries.split(|&b| b == 0).filter(unrelated))
    {
        bail!("The hunk would change unrelated staging; use whole-file staging");
    }
    if regular(&before_entry)?
        .zip(regular(&after_entry)?)
        .is_some_and(|(a, b)| a.0 != b.0)
    {
        bail!("The hunk would change the file mode; use whole-file staging");
    }
    let (from, to) = if side == Side::Staged {
        (&after, &before)
    } else {
        (&before, &after)
    };
    let mut args = DIFF.to_vec();
    args.extend([from.as_str(), to.as_str()]);
    let patch = String::from_utf8(exact_path(repo, None, &args, path)?)?;
    split(&patch)?;
    Ok((patch, after_entry))
}

pub fn plan(snapshot: &Snapshot, hunk: usize) -> Result<Plan> {
    snapshot.validate()?;
    let selected = snapshot
        .hunks
        .get(hunk)
        .context("No hunk selected; choose a changed text hunk")?;
    let patch = format!("{}{}", split(&snapshot.patch)?.0, selected.text);
    let index = &snapshot.captured.state.index;
    let temp = tempfile::Builder::new()
        .prefix("igit-hunk-review-")
        .tempdir_in(index.parent().context("Missing Git directory")?)?;
    let next = temp.path().join("next");
    private_index(&snapshot.repo, index, &next)?;
    let (patch, result) = changed(&snapshot.repo, &next, snapshot.side, &snapshot.path, &patch)?;
    snapshot.validate()?;
    Ok(Plan {
        root: snapshot.root,
        repo: snapshot.repo.clone(),
        path: snapshot.path.clone(),
        side: snapshot.side,
        reviewed: patch.clone(),
        patch,
        snapshot: snapshot.clone(),
        result,
    })
}

pub fn execute(plan: &Plan) -> Result<()> {
    let repo = &plan.repo;
    let index = git_path(repo, "index")?;
    let lock = index.with_file_name("index.lock");
    let mut lockfile = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&lock)
        .context("Cannot lock the Git index; wait for other operations to finish")?;
    let mut guard = Guard(lock.clone(), false);
    if plan.root != plan.snapshot.root
        || plan.repo != plan.snapshot.repo
        || plan.path != plan.snapshot.path
        || plan.side != plan.snapshot.side
        || plan.patch != plan.reviewed
    {
        bail!("The hunk review changed; refresh and review again");
    }
    plan.snapshot.validate()?;
    let temp = tempfile::Builder::new()
        .prefix("igit-hunk-")
        .tempdir_in(index.parent().context("Missing Git directory")?)?;
    let next = temp.path().join("next");
    // Copy CURRENT staging only after the real index is locked and validated.
    private_index(repo, &index, &next)?;
    let (patch, result) = changed(repo, &next, plan.side, &plan.path, &plan.patch)?;
    if patch != plan.reviewed || result != plan.result {
        bail!("The hunk result changed; refresh and review again");
    }
    plan.snapshot.validate()?;
    if index.exists() {
        lockfile.set_permissions(fs::metadata(&index)?.permissions())?;
    }
    lockfile.write_all(&fs::read(&next)?)?;
    lockfile.sync_all()?;
    drop(lockfile);
    fs::rename(&lock, &index).context("Cannot publish the updated Git index")?;
    guard.1 = true;
    Ok(())
}

#[cfg(test)]
mod tests;
