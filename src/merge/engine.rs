use anyhow::{anyhow, ensure, Context as _, Result};
use similar::{DiffTag, TextDiff};
use std::{
    borrow::Cow,
    collections::BTreeMap,
    fs,
    io::Write,
    path::{Component, Path, PathBuf},
    process::Output,
};
use tempfile::NamedTempFile;

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub(super) struct Stages(pub [Option<(String, String)>; 3]);
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Conflict {
    pub root: usize,
    pub path: String,
    pub stages: Stages,
}

impl Conflict {
    pub(super) fn stage_error(&self) -> Option<&'static str> {
        let [Some((base, _)), Some((current, _)), Some((incoming, _))] = &self.stages.0 else {
            return Some("Added/deleted/renamed conflict: resolve this path explicitly outside the text editor; nothing was modified");
        };
        if base != "100644" && base != "100755" {
            return Some(
                "Binary/symlink/submodule conflict; inspect each side and resolve externally",
            );
        }
        if base != current || base != incoming {
            return Some("File mode changed; resolve mode and contents externally");
        }
        None
    }
}

fn run(repo: &Path, args: &[&str]) -> Result<Output> {
    crate::core::command(repo)
        .arg("--literal-pathspecs")
        .args(args)
        .output()
        .context("run git")
}
fn checked(repo: &Path, args: &[&str]) -> Result<Vec<u8>> {
    let o = run(repo, args)?;
    ensure!(
        o.status.success(),
        "{}",
        String::from_utf8_lossy(&o.stderr).trim()
    );
    Ok(o.stdout)
}
pub(super) fn conflicts(id: usize, repo: &Path) -> Result<Vec<Conflict>> {
    let raw = checked(repo, &["ls-files", "-u", "-z"])?;
    let mut result = BTreeMap::<String, Stages>::new();
    for rec in raw.split(|v| *v == 0).filter(|a| !a.is_empty()) {
        let pos = rec
            .iter()
            .position(|x| *x == b'\t')
            .context("invalid index record")?;
        let name = std::str::from_utf8(&rec[pos + 1..])
            .context("non-UTF-8 conflict path; resolve with git mergetool")?;
        let fields: Vec<_> = std::str::from_utf8(&rec[..pos])?
            .split_ascii_whitespace()
            .collect();
        ensure!(fields.len() == 3, "invalid index metadata");
        let slot = fields[2].parse::<usize>()?;
        ensure!((1..=3).contains(&slot), "unexpected index stage");
        result.entry(name.into()).or_default().0[slot - 1] =
            Some((fields[0].into(), fields[1].into()));
    }
    Ok(result
        .into_iter()
        .map(|(path, stages)| Conflict {
            root: id,
            path,
            stages,
        })
        .collect())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Kind {
    Context,
    Safe,
    Conflict,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum FileSide {
    Current,
    Incoming,
}

impl FileSide {
    pub const ALL: [Self; 2] = [Self::Current, Self::Incoming];

    pub fn pane(self) -> usize {
        match self {
            Self::Current => 0,
            Self::Incoming => 2,
        }
    }

    pub fn slot(self) -> usize {
        self.pane() / 2 + 1
    }

    pub fn text(self, chunk: &Chunk) -> &str {
        match self {
            Self::Current => &chunk.left,
            Self::Incoming => &chunk.right,
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub(super) struct Resolution {
    pending: [bool; 2],
    accepted: bool,
}

#[derive(Debug, Clone)]
pub(super) struct Chunk {
    pub base: String,
    pub left: String,
    pub right: String,
    pub result: String,
    pub kind: Kind,
    pub resolution: Resolution,
}
fn lines(s: &str) -> Vec<&str> {
    s.split_inclusive('\n').collect()
}
#[derive(Clone, Debug)]
struct Hunk {
    start: usize,
    end: usize,
    new: String,
    side: usize,
}
fn hunks(base: &str, other: &str, side: usize) -> Vec<Hunk> {
    let nl = lines(other);
    TextDiff::configure()
        .timeout(std::time::Duration::from_millis(500))
        .diff_lines(base, other)
        .ops()
        .iter()
        .filter(|o| o.tag() != DiffTag::Equal)
        .map(|o| Hunk {
            start: o.old_range().start,
            end: o.old_range().end,
            new: nl[o.new_range()].concat(),
            side,
        })
        .collect()
}
// Insertions on a boundary are deliberately grouped with the neighboring edit.
// It is preferable to ask the user than to guess the order of inserted text.
fn overlaps(a: &Hunk, b: &Hunk) -> bool {
    if a.start == a.end || b.start == b.end {
        a.start <= b.end && b.start <= a.end
    } else {
        a.start < b.end && b.start < a.end
    }
}
fn apply_slice(src: &[&str], start: usize, end: usize, grp: &[Hunk], side: usize) -> String {
    let mut out = String::new();
    let mut at = start;
    for h in grp.iter().filter(|h| h.side == side) {
        out.push_str(&src[at..h.start].concat());
        out.push_str(&h.new);
        at = h.end
    }
    out.push_str(&src[at..end].concat());
    out
}
fn context(s: String) -> Chunk {
    Chunk {
        base: s.clone(),
        left: s.clone(),
        right: s.clone(),
        result: s,
        kind: Kind::Context,
        resolution: Resolution::default(),
    }
}
pub(super) fn diff3(base: &str, a: &str, b: &str) -> Vec<Chunk> {
    let src = lines(base);
    let mut hs = hunks(base, a, 0);
    hs.extend(hunks(base, b, 1));
    hs.sort_by_key(|h| (h.start, h.end, h.side));
    let mut groups: Vec<Vec<Hunk>> = vec![];
    for h in hs {
        if let Some(last) = groups.last_mut() {
            if last.iter().rev().any(|other| overlaps(other, &h)) {
                last.push(h);
                continue;
            }
        }
        groups.push(vec![h])
    }
    let mut chunks = vec![];
    let mut at = 0;
    for group in groups {
        let start = group[0].start;
        let end = group.iter().map(|h| h.end).max().unwrap();
        if at < start {
            chunks.push(context(src[at..start].concat()))
        }
        let base = src[start..end].concat();
        let left = apply_slice(&src, start, end, &group, 0);
        let right = apply_slice(&src, start, end, &group, 1);
        let kind = if left == right || left == base || right == base {
            Kind::Safe
        } else {
            Kind::Conflict
        };
        let pending = [left != base, right != base];
        chunks.push(Chunk {
            result: base.clone(),
            base,
            left,
            right,
            kind,
            resolution: Resolution {
                pending,
                accepted: false,
            },
        });
        at = end
    }
    if at < src.len() || chunks.is_empty() {
        chunks.push(context(src[at..].concat()))
    }
    chunks
}
impl Chunk {
    pub fn pending(&self, side: FileSide) -> bool {
        self.resolution.pending[side.slot() - 1]
    }

    pub fn done(&self) -> bool {
        !self.resolution.pending.iter().any(|&pending| pending)
    }

    pub fn resolve(&mut self, result: String) {
        self.result = result;
        self.resolution = Resolution::default();
    }

    pub fn use_side(&mut self, side: FileSide) {
        self.resolve(side.text(self).to_owned());
    }

    /// A conflict keeps the other side open. Accepting it later appends its
    /// lines; accepting after an ignore replaces the base instead.
    pub fn decide(&mut self, side: FileSide, accept: bool) {
        if !self.pending(side) {
            return;
        }
        if self.kind == Kind::Safe {
            if accept {
                self.use_side(side);
            } else {
                self.resolution = Resolution::default();
            }
            return;
        }
        if accept {
            let next = side.text(self).to_owned();
            if self.resolution.accepted {
                append_lines(&mut self.result, &next);
            } else {
                self.result = next;
            }
            self.resolution.accepted = true;
        }
        self.resolution.pending[side.slot() - 1] = false;
    }

    pub fn safe(&mut self, source: Option<FileSide>) -> bool {
        if self.kind != Kind::Safe {
            return false;
        }
        let side = source.or_else(|| FileSide::ALL.into_iter().find(|&s| self.pending(s)));
        if let Some(side) = side.filter(|&side| self.pending(side)) {
            self.use_side(side);
            true
        } else {
            false
        }
    }

    pub fn both(&mut self) {
        let mut result = self.left.clone();
        append_lines(&mut result, &self.right);
        self.resolve(result);
    }

    /// A preview shared by availability, batch resolution, and session drafts.
    /// Safe blocks borrow their source; only the small character merge allocates.
    /// Existing accept/ignore decisions and manual results are never replaced.
    pub fn automatic_result(&self) -> Option<Cow<'_, str>> {
        if self.kind == Kind::Safe {
            FileSide::ALL
                .into_iter()
                .find(|&side| self.pending(side))
                .map(|side| Cow::Borrowed(side.text(self)))
        } else {
            self.simple_result().map(Cow::Owned)
        }
    }

    /// Only combine disjoint character edits on the same single line.
    /// Changes touching the same position, including insertion boundaries,
    /// remain for manual review.
    pub fn simple_result(&self) -> Option<String> {
        if self.kind != Kind::Conflict
            || !FileSide::ALL.into_iter().all(|side| self.pending(side))
            || [&self.base, &self.left, &self.right]
                .iter()
                .any(|s| s.len() > 4096 || s.split_inclusive('\n').count() != 1)
        {
            return None;
        }
        let base: Vec<_> = self.base.chars().collect();
        let left: Vec<_> = self.left.chars().collect();
        let right: Vec<_> = self.right.chars().collect();
        let edit = |new: &[char]| {
            let start = base.iter().zip(new).take_while(|(a, b)| a == b).count();
            let tail = base[start..]
                .iter()
                .rev()
                .zip(new[start..].iter().rev())
                .take_while(|(a, b)| a == b)
                .count();
            (start..base.len() - tail, start..new.len() - tail)
        };
        let (l, ln) = edit(&left);
        let (r, rn) = edit(&right);
        let ((a, first), (b, last)) = if l.start < r.start {
            ((l, &left[ln]), (r, &right[rn]))
        } else {
            ((r, &right[rn]), (l, &left[ln]))
        };
        if a.end > b.start || (a.end == b.start && (a.is_empty() || b.is_empty())) {
            return None;
        }
        Some(
            base[..a.start]
                .iter()
                .chain(first)
                .chain(&base[a.end..b.start])
                .chain(last)
                .chain(&base[b.end..])
                .collect(),
        )
    }
}

fn append_lines(result: &mut String, next: &str) {
    if !result.is_empty() && !result.ends_with('\n') && !next.is_empty() {
        result.push('\n');
    }
    result.push_str(next);
}

fn git_path(repo: &Path, name: &str) -> Result<PathBuf> {
    let s = checked(repo, &["rev-parse", "--git-path", name])?;
    Ok(repo.join(std::str::from_utf8(&s)?.trim_end_matches(['\r', '\n'])))
}
fn rev(repo: &Path, name: &str) -> String {
    run(repo, &["rev-parse", "--verify", name])
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
        .unwrap_or_default()
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Operation {
    pub name: &'static str,
    pub head: String,
    pub other: String,
}
pub(super) fn operation(repo: &Path) -> Result<Option<Operation>> {
    let dir = git_path(repo, ".")?;
    let (name, other) = if dir.join("rebase-merge").is_dir() || dir.join("rebase-apply").is_dir() {
        ("rebase", "REBASE_HEAD")
    } else if dir.join("MERGE_HEAD").is_file() {
        ("merge", "MERGE_HEAD")
    } else if dir.join("CHERRY_PICK_HEAD").is_file() {
        ("cherry-pick", "CHERRY_PICK_HEAD")
    } else if dir.join("REVERT_HEAD").is_file() {
        ("revert", "REVERT_HEAD")
    } else {
        return Ok(None);
    };
    Ok(Some(Operation {
        name,
        head: rev(repo, "HEAD"),
        other: rev(repo, other),
    }))
}

/// A branch name is only a display hint when it points to this exact commit.
pub(super) fn unique_branch(repo: &Path, oid: &str) -> Option<String> {
    if oid.is_empty() {
        return None;
    }
    let output = crate::core::git(
        repo,
        &[
            "for-each-ref",
            &format!("--points-at={oid}"),
            "--format=%(refname)%00%(symref)",
            "refs/heads",
            "refs/remotes",
        ],
    )
    .ok()?;
    let mut names = output.lines().filter_map(|line| {
        let (name, symref) = line.split_once('\0')?;
        if !symref.is_empty() {
            return None;
        }
        name.strip_prefix("refs/heads/")
            .or_else(|| name.strip_prefix("refs/remotes/"))
    });
    let only = names.next()?;
    names.next().is_none().then(|| only.to_owned())
}

pub(super) fn continue_plan(repo: &Path) -> Result<(Operation, String, String)> {
    let op = operation(repo)?.context("No merge/rebase/sequencer to continue")?;
    ensure!(
        checked(repo, &["ls-files", "-u", "-z"])?.is_empty(),
        "There are unresolved files in this repository"
    );
    let index_tree = String::from_utf8(checked(repo, &["write-tree"])?)?
        .trim()
        .to_owned();
    let summary = String::from_utf8_lossy(&checked(
        repo,
        &[
            "diff",
            "--cached",
            "--no-ext-diff",
            "--no-textconv",
            "--stat",
        ],
    )?)
    .into_owned();
    Ok((op, index_tree, summary))
}
pub(super) fn continue_op(repo: &Path, op: &Operation, tree: &str) -> Result<String> {
    let (now, index, _) = continue_plan(repo)?;
    ensure!(
        &now == op && tree == index,
        "Operation or staged files changed; review again"
    );
    let o = crate::core::command(repo)
        .args([op.name, "--continue"])
        .output()?;
    ensure!(
        o.status.success(),
        "git {} --continue: {}",
        op.name,
        String::from_utf8_lossy(&o.stderr).trim()
    );
    Ok(format!("{} continued", op.name))
}
fn validate_path(repo: &Path, path: &str) -> Result<PathBuf> {
    let p = Path::new(path);
    ensure!(
        !p.is_absolute() && p.components().all(|c| matches!(c, Component::Normal(_))),
        "Unsafe path component"
    );
    let mut cur = repo.to_owned();
    for part in p.components() {
        cur.push(part);
        let md = fs::symlink_metadata(&cur)?;
        ensure!(
            !md.file_type().is_symlink(),
            "Symlink conflict or ancestor; use an external mergetool"
        )
    }
    ensure!(cur.is_file(), "Not a regular working tree file");
    Ok(cur)
}
const LIMIT: usize = 2 * 1024 * 1024;
fn regular(repo: &Path, path: &str) -> Result<(PathBuf, Vec<u8>)> {
    let file = validate_path(repo, path)?;
    ensure!(
        fs::metadata(&file)?.len() <= LIMIT as u64,
        "File is larger than 2 MiB; use an external mergetool"
    );
    let data = fs::read(&file)?;
    ensure!(
        !data.contains(&0),
        "Binary conflict: resolve with an external mergetool"
    );
    Ok((file, data))
}
fn clean_marker_labels(s: &str) -> String {
    s.split_inclusive('\n')
        .map(|l| {
            for m in ["<<<<<<<", "|||||||", ">>>>>>>"] {
                if l.starts_with(&format!("{m} ")) {
                    return format!("{}{}", m, if l.ends_with('\n') { "\n" } else { "" });
                }
            }
            l.to_owned()
        })
        .collect()
}
fn generated(
    repo: &Path,
    w: &[u8],
    path: &str,
    base: &str,
    left: &str,
    right: &str,
) -> Result<bool> {
    if let Ok(bytes) = checked(repo, &["cat-file", "blob", &format!("AUTO_MERGE:{path}")]) {
        if bytes == w {
            return Ok(true);
        }
    }
    let td = tempfile::tempdir()?;
    let new = |n: &str, s: &str| -> Result<PathBuf> {
        let p = td.path().join(n);
        fs::write(&p, s)?;
        Ok(p)
    };
    let a = new("current", left)?;
    let b = new("base", base)?;
    let c = new("incoming", right)?;
    let want = clean_marker_labels(std::str::from_utf8(w)?);
    for fmt in [None, Some("--diff3"), Some("--zdiff3")] {
        let mut cmd = crate::core::command(repo);
        cmd.args(["merge-file", "-p"]);
        if let Some(arg) = fmt {
            cmd.arg(arg);
        }
        let out = cmd.arg(&a).arg(&b).arg(&c).output()?;
        if let Some(n) = out.status.code() {
            if n < 128 && clean_marker_labels(&String::from_utf8_lossy(&out.stdout)) == want {
                return Ok(true);
            }
        }
    }
    Ok(false)
}
fn attributes(repo: &Path, path: &str) -> Result<Vec<u8>> {
    checked(
        repo,
        &[
            "check-attr",
            "-z",
            "merge",
            "filter",
            "working-tree-encoding",
            "ident",
            "eol",
            "text",
            "--",
            path,
        ],
    )
}
fn conversion_state(repo: &Path, path: &str) -> Result<Vec<u8>> {
    let mut attrs = attributes(repo, path)?;
    for key in [
        "core.autocrlf",
        "core.eol",
        "core.safecrlf",
        "core.filemode",
    ] {
        attrs.push(0);
        let o = run(repo, &["config", "--get", key])?;
        ensure!(
            o.status.success() || o.status.code() == Some(1),
            "Cannot read Git configuration"
        );
        attrs.extend(o.stdout);
    }
    Ok(attrs)
}
pub(super) struct Document {
    pub file: Conflict,
    pub chunks: Vec<Chunk>,
    pub op: Option<Operation>,
    repo: PathBuf,
    original: Vec<u8>,
    conversion_state: Vec<u8>,
    permissions: fs::Permissions,
    head: String,
    pub left_label: String,
    pub right_label: String,
}
impl Document {
    pub fn open(repo: &Path, file: Conflict) -> Result<Self> {
        if let Some(error) = file.stage_error() {
            anyhow::bail!("{error}");
        }
        let st = &file.stages.0;
        let attrs = attributes(repo, &file.path)?;
        let conversion_state = conversion_state(repo, &file.path)?;
        for a in attrs.split(|&x| x == 0).collect::<Vec<_>>().chunks_exact(3) {
            let key = std::str::from_utf8(a[1])?;
            let v = std::str::from_utf8(a[2])?;
            ensure!(
                v == "unspecified"
                    || (key == "merge" && (v == "text" || v == "set"))
                    || (key != "merge" && v == "unset")
                    || (key == "text" && matches!(v, "auto" | "set"))
                    // Standard EOL rules can be byte-preserving (notably eol=lf).
                    // The per-stage checkout comparison below remains authoritative.
                    || (key == "eol" && matches!(v, "lf" | "crlf")),
                "Path uses {}={}; use its configured Git merge/filter workflow",
                key,
                v
            )
        }
        let (working_path, w) = regular(repo, &file.path)?;
        let permissions = fs::metadata(working_path)?.permissions();
        let mut versions = Vec::new();
        for v in st.iter().flatten() {
            let size: usize = std::str::from_utf8(&checked(repo, &["cat-file", "-s", &v.1])?)?
                .trim()
                .parse()?;
            ensure!(size <= LIMIT, "Large blob; use an external mergetool");
            let bytes = checked(repo, &["cat-file", "blob", &v.1])?;
            ensure!(
                !bytes.contains(&0),
                "Binary conflict; use an external mergetool"
            );
            // Compare actual checkout bytes: core.autocrlf and attribute-based
            // conversions must not be silently undone when building the preview.
            let converted = checked(
                repo,
                &[
                    "cat-file",
                    "--filters",
                    &format!("--path={}", file.path),
                    &v.1,
                ],
            )?;
            ensure!(bytes==converted,"Git checkout filters or line endings transform this file. Resolve externally; working-tree bytes have been preserved");
            let s =
                String::from_utf8(bytes).context("Non-UTF-8 source: use an external mergetool")?;
            ensure!(
                s.lines().count() < 25000,
                "File exceeds interactive merge line limit"
            );
            versions.push(s);
        }
        let base = &versions[0];
        // The editor already starts at the base. Reopening those exact bytes
        // keeps the working file intact and leaves all choices pending.
        ensure!(
            w.as_slice() == base.as_bytes()
                || generated(repo, &w, &file.path, base, &versions[1], &versions[2])?,
            "Working file differs from Git's original conflict output; existing edits were preserved. Finish editing externally and git add this path"
        );
        let op = operation(repo)?;
        let head = rev(repo, "HEAD");
        let reb = op.as_ref().is_some_and(|x| x.name == "rebase");
        let branch = run(repo, &["symbolic-ref", "-q", "--short", "HEAD"])?.stdout;
        let bname = String::from_utf8_lossy(&branch).trim().to_owned();
        let left_label = if reb {
            format!("Rebased HEAD {}", head.get(..7).unwrap_or(&head))
        } else if bname.is_empty() {
            format!("HEAD {}", head.get(..7).unwrap_or(&head))
        } else {
            bname
        };
        let right_label = if let Some(o) = &op {
            let short = o.other.get(..7).unwrap_or(&o.other);
            if reb {
                format!("Replay {short}")
            } else {
                let branch = if o.name == "merge" && !o.other.is_empty() {
                    if rev(repo, "@{upstream}") == o.other {
                        run(
                            repo,
                            &[
                                "rev-parse",
                                "--abbrev-ref",
                                "--symbolic-full-name",
                                "@{upstream}",
                            ],
                        )
                        .ok()
                        .filter(|o| o.status.success())
                        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
                    } else {
                        unique_branch(repo, &o.other)
                    }
                } else {
                    None
                };
                branch.unwrap_or_else(|| format!("Incoming {short}"))
            }
        } else {
            format!("Stage 3 {}", &st[2].as_ref().unwrap().1[..7])
        };
        let chunks = diff3(base, &versions[1], &versions[2]);
        Ok(Self {
            file,
            chunks,
            op,
            head,
            left_label,
            right_label,
            repo: repo.to_owned(),
            original: w,
            conversion_state,
            permissions,
        })
    }
    pub fn pending(&self) -> usize {
        self.chunks.iter().filter(|x| !x.done()).count()
    }

    pub fn source_name(&self, side: FileSide) -> &str {
        match side {
            FileSide::Current => &self.left_label,
            FileSide::Incoming => &self.right_label,
        }
    }

    pub fn source_details(&self, side: FileSide) -> Vec<String> {
        let (_, blob) = self.file.stages.0[side.slot()].as_ref().unwrap();
        let rev = match side {
            FileSide::Current => Some(self.head.as_str()),
            FileSide::Incoming => self.op.as_ref().map(|o| o.other.as_str()),
        };
        let mut lines = vec![
            self.file.path.clone(),
            format!("Stage {} blob {blob}", side.slot()),
        ];
        if let Some(rev) = rev.filter(|id| !id.is_empty()) {
            if let Ok(bytes) = checked(
                &self.repo,
                &[
                    "show",
                    "-s",
                    "--no-notes",
                    "--format=%H%n%an <%ae>%n%aI%n%s",
                    rev,
                ],
            ) {
                lines.push(String::new());
                lines.extend(String::from_utf8_lossy(&bytes).lines().map(str::to_owned));
            }
        }
        lines
    }
    pub fn apply(&self) -> Result<()> {
        ensure!(
            self.pending() == 0,
            "Resolve every conflict and non-conflicting change first"
        );
        ensure!(
            rev(&self.repo, "HEAD") == self.head && operation(&self.repo)? == self.op,
            "Operation or HEAD changed; reopen the conflict"
        );
        ensure!(
            self.conversion_state == conversion_state(&self.repo, &self.file.path)?,
            "Git attributes/config changed; reopen before applying"
        );
        let current = conflicts(self.file.root, &self.repo)?;
        ensure!(
            current.iter().any(|c| c == &self.file),
            "Index conflict stages changed; reload before applying"
        );
        let (p, bytes) = regular(&self.repo, &self.file.path)?;
        ensure!(
            bytes == self.original,
            "Working file changed since it was opened; no changes were written"
        );
        let text = self
            .chunks
            .iter()
            .map(|x| x.result.as_str())
            .collect::<String>();
        ensure!(
            !text.lines().any(|l| l.starts_with("<<<<<<< ")
                || l.starts_with(">>>>>>> ")
                || l.starts_with("||||||| ")),
            "Result still contains conflict markers"
        );
        let md = fs::metadata(&p)?;
        ensure!(
            md.permissions() == self.permissions,
            "Working file permissions changed; reopen before applying"
        );
        let mut tmp = NamedTempFile::new_in(p.parent().context("missing parent")?)?;
        tmp.write_all(text.as_bytes())?;
        tmp.as_file().set_permissions(md.permissions().clone())?;
        tmp.as_file().sync_all()?;
        ensure!(
            regular(&self.repo, &self.file.path)?.1 == self.original,
            "File changed before application; no changes were written"
        );
        tmp.persist(&p).map_err(|e| e.error)?;
        if let Err(e) = checked(&self.repo, &["add", "--", &self.file.path]) {
            if fs::read(&p).ok().as_deref() == Some(text.as_bytes()) {
                let mut back = NamedTempFile::new_in(p.parent().unwrap())?;
                back.write_all(&bytes)?;
                back.as_file().set_permissions(md.permissions())?;
                back.persist(&p).map_err(|x| x.error)?;
            }
            return Err(anyhow!("Unable to stage file: {e}"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    mod automatic;
    mod conversion;

    use super::*;
    fn g(p: &Path, args: &[&str]) -> String {
        String::from_utf8(checked(p, args).unwrap()).unwrap()
    }
    fn fixture() -> tempfile::TempDir {
        fixture_with(
            "a\nb\nc\nshared\ne\nf\ng\n",
            "left safe\nb\nc\nlocal\ne\nf\ng\n",
            "a\nb\nc\nincoming\ne\nf\nright safe\n",
        )
    }

    fn fixture_with(base: &str, left: &str, right: &str) -> tempfile::TempDir {
        let td = tempfile::tempdir().unwrap();
        let p = td.path();
        g(p, &["init", "-q", "-b", "main"]);
        g(p, &["config", "user.name", "Test"]);
        g(p, &["config", "user.email", "test@example.invalid"]);
        g(p, &["config", "commit.gpgsign", "false"]);
        g(p, &["config", "core.hooksPath", "/dev/null"]);
        let name = "config with spaces.txt";
        fs::write(p.join(name), base).unwrap();
        g(p, &["add", "--", name]);
        g(p, &["commit", "-qm", "base"]);
        g(p, &["checkout", "-qb", "incoming"]);
        fs::write(p.join(name), right).unwrap();
        g(p, &["commit", "-qam", "incoming"]);
        g(p, &["checkout", "-q", "main"]);
        fs::write(p.join(name), left).unwrap();
        g(p, &["commit", "-qam", "local"]);
        let o = run(p, &["merge", "--no-edit", "incoming"]).unwrap();
        assert_eq!(o.status.code(), Some(1));
        td
    }
    #[test]
    fn real_merge_safe_conflict_apply_continue() {
        let td = fixture();
        let p = td.path();
        let c = conflicts(11, p).unwrap().pop().unwrap();
        let mut d = Document::open(p, c).unwrap();
        assert_eq!(
            d.chunks.iter().filter(|x| x.kind == Kind::Conflict).count(),
            1
        );
        assert_eq!(d.chunks.iter().filter(|x| x.kind == Kind::Safe).count(), 2);
        for x in &mut d.chunks {
            x.safe(None);
        }
        assert_eq!(d.pending(), 1);
        assert!(d.apply().is_err());
        for x in &mut d.chunks {
            if !x.done() {
                x.use_side(FileSide::Incoming);
            }
        }
        d.apply().unwrap();
        assert!(conflicts(11, p).unwrap().is_empty());
        assert!(operation(p).unwrap().is_some());
        assert_eq!(
            fs::read_to_string(p.join("config with spaces.txt")).unwrap(),
            "left safe\nb\nc\nincoming\ne\nf\nright safe\n"
        );
        let (op, tree, _) = continue_plan(p).unwrap();
        continue_op(p, &op, &tree).unwrap();
        assert!(operation(p).unwrap().is_none());
        assert_eq!(
            g(p, &["rev-list", "--parents", "-1", "HEAD"])
                .split_whitespace()
                .count(),
            3
        );
    }
    #[test]
    fn preserves_external_edits_before_and_after_open() {
        let td = fixture();
        let p = td.path();
        let c = conflicts(0, p).unwrap().pop().unwrap();
        let mut d = Document::open(p, c.clone()).unwrap();
        for x in &mut d.chunks {
            x.use_side(FileSide::Current);
        }
        fs::write(p.join(&c.path), "my external edits\n").unwrap();
        assert!(d.apply().is_err());
        assert!(Document::open(p, c.clone()).is_err());
        assert_eq!(
            fs::read_to_string(p.join(c.path)).unwrap(),
            "my external edits\n"
        );
        assert!(!conflicts(0, p).unwrap().is_empty())
    }

    #[test]
    fn enter_opens_a_conflict_with_the_base_saved_in_the_working_tree() {
        use crate::{
            core::Context,
            merge::MergePanel,
            ui::{Action, Panel},
        };
        use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};

        let td = fixture();
        let repo = td.path();
        let path = repo.join("config with spaces.txt");
        let base = checked(repo, &["show", ":1:config with spaces.txt"]).unwrap();
        assert_ne!(fs::read(&path).unwrap(), base);
        fs::write(&path, &base).unwrap();
        let head = g(repo, &["rev-parse", "HEAD"]);
        let index = g(repo, &["ls-files", "--stage"]);

        let ctx = Context::discover(repo).unwrap();
        let mut panel = MergePanel::default();
        panel.refresh(&ctx).unwrap();
        let key = |code| Event::Key(KeyEvent::new(code, KeyModifiers::NONE));
        if let Action::Notice(error) = panel.event(&key(KeyCode::Enter), &ctx) {
            panic!("Enter refused to open the saved base: {error}");
        }
        let doc = panel.doc.as_ref().expect("Enter opens the merge editor");
        let result: String = doc
            .chunks
            .iter()
            .map(|chunk| chunk.result.as_str())
            .collect();
        assert_eq!(result.as_bytes(), base);
        assert!(doc
            .chunks
            .iter()
            .any(|c| c.kind == Kind::Conflict && !c.done()));
        assert!(matches!(
            panel.event(&key(KeyCode::Char('A')), &ctx),
            Action::Notice(_)
        ));
        assert_eq!(fs::read(path).unwrap(), base);
        assert_eq!(g(repo, &["ls-files", "--stage"]), index);
        assert_eq!(g(repo, &["rev-parse", "HEAD"]), head);
    }

    #[test]
    fn safe_edits_preserve_eof_and_independent_adjacent_changes() {
        let mut cs = diff3("a\nb\nc", "A\nb\nc", "a\nB\nc");
        for c in &mut cs {
            c.safe(None);
        }
        assert!(cs.iter().all(Chunk::done));
        assert_eq!(
            cs.iter().map(|c| c.result.as_str()).collect::<String>(),
            "A\nB\nc"
        );
        let cs = diff3("a\n", "left\na\n", "right\na\n");
        assert_eq!(cs.iter().filter(|x| x.kind == Kind::Conflict).count(), 1);
    }

    #[test]
    fn one_side_can_be_ignored_appended_or_resolved_with_disjoint_edits() {
        let chunks = diff3("base", "左", "右");
        let mut chunk = chunks[0].clone();
        chunk.decide(FileSide::Incoming, true);
        assert_eq!(chunk.result, "右");
        assert!(chunk.pending(FileSide::Current));
        chunk.decide(FileSide::Current, true);
        assert!(chunk.done());
        assert_eq!(chunk.result, "右\n左");

        let mut chunk = chunks[0].clone();
        chunk.decide(FileSide::Current, false);
        assert_eq!(chunk.result, "base");
        assert!(!chunk.done());
        chunk.decide(FileSide::Incoming, true);
        assert!(chunk.done());
        assert_eq!(chunk.result, "右");

        let same = diff3("base\n", "match\n", "match\n");
        let mut chunk = same[0].clone();
        chunk.decide(FileSide::Current, true);
        assert!(chunk.done());
        assert_eq!(chunk.result, "match\n");

        let disjoint = diff3("开始 = 0 / 结束\n", "左 = 0 / 结束\n", "开始 = 0 / 右\n");
        assert_eq!(
            disjoint[0].simple_result().as_deref(),
            Some("左 = 0 / 右\n")
        );
        let overlapping = diff3("abc\n", "aXc\n", "aYc\n");
        assert_eq!(overlapping[0].simple_result(), None);
    }

    #[test]
    fn rebase_labels_and_conversion_safety() {
        let t = fixture();
        let p = t.path();
        g(p, &["merge", "--abort"]);
        assert!(!run(p, &["rebase", "incoming"]).unwrap().status.success());
        let c = conflicts(0, p).unwrap().pop().unwrap();
        let d = Document::open(p, c.clone()).unwrap();
        assert!(d.left_label.contains("Rebased"));
        assert!(d.right_label.contains("Replay"));
        let conflict = d.chunks.iter().find(|x| x.kind == Kind::Conflict).unwrap();
        assert_eq!(conflict.left, "incoming\n");
        assert_eq!(conflict.right, "local\n");
        g(p, &["config", "core.autocrlf", "true"]);
        let original = fs::read(p.join(&c.path)).unwrap();
        assert!(Document::open(p, c.clone()).is_err());
        assert_eq!(fs::read(p.join(&c.path)).unwrap(), original);
    }

    #[test]
    fn open_and_merge_gutters_match_their_rendered_cells_in_each_mode() {
        use crate::{
            core::Context,
            merge::{Hit, MergePanel},
            ui::{icons::IconMode, Action, Panel},
        };
        use crossterm::event::{
            Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
        };
        use ratatui::{backend::TestBackend, layout::Rect, Terminal};
        let td = fixture();
        let ctx = Context::discover(td.path()).unwrap();
        let path = td.path().join("config with spaces.txt");
        let original = fs::read(&path).unwrap();
        let click = |p: &mut MergePanel, x, y| {
            p.event(
                &Event::Mouse(MouseEvent {
                    kind: MouseEventKind::Down(MouseButton::Left),
                    column: x,
                    row: y,
                    modifiers: KeyModifiers::NONE,
                }),
                &ctx,
            )
        };
        let key = |ch| Event::Key(KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE));
        let text = |term: &Terminal<TestBackend>, r: Rect| -> String {
            (r.x..r.right())
                .map(|x| term.backend().buffer()[(x, r.y)].symbol())
                .collect()
        };
        for icons in [IconMode::Ascii, IconMode::Unicode, IconMode::NerdFont] {
            let mut p = MergePanel {
                icons,
                ..Default::default()
            };
            p.refresh(&ctx).unwrap();
            let mut term = Terminal::new(TestBackend::new(44, 18)).unwrap();
            term.draw(|f| p.draw(f, f.area(), &ctx)).unwrap();
            let open = p
                .hits
                .iter()
                .find(|(_, h)| matches!(h, Hit::Open))
                .unwrap()
                .0;
            assert_eq!(text(&term, open).trim(), "Resolve Manually");
            click(&mut p, open.right() - 1, open.y);
            p.event(&key('a'), &ctx);
            assert_eq!(p.doc.as_ref().unwrap().pending(), 1);
            for width in [44, 120] {
                let mut term = Terminal::new(TestBackend::new(width, 18)).unwrap();
                for side in FileSide::ALL {
                    p.side = side.pane();
                    term.draw(|f| p.draw(f, f.area(), &ctx)).unwrap();
                    let gutter = p
                        .hits
                        .iter()
                        .find(|(_, h)| matches!(h, Hit::Decide(i,s,true) if *i == p.idx && *s == side))
                        .unwrap()
                        .0;
                    let take = match (icons, side) {
                        (IconMode::Ascii, FileSide::Current) => ">>",
                        (IconMode::Ascii, FileSide::Incoming) => "<<",
                        (_, FileSide::Current) => "»",
                        (_, FileSide::Incoming) => "«",
                    };
                    assert_eq!(text(&term, gutter), take);
                    let beside = if side == FileSide::Current {
                        gutter.x - 1
                    } else {
                        gutter.right()
                    };
                    click(&mut p, beside, gutter.y);
                    assert_eq!(p.doc.as_ref().unwrap().chunks[p.idx].result, "shared\n");
                    for x in gutter.x..gutter.right() {
                        click(&mut p, x, gutter.y);
                        let chunk = &p.doc.as_ref().unwrap().chunks[p.idx];
                        assert!(!chunk.done());
                        assert!(!chunk.pending(side));
                        assert_eq!(chunk.result, side.text(chunk));
                        assert!(matches!(p.event(&key('A'), &ctx), Action::Notice(_)));
                        p.event(&key('u'), &ctx);
                    }
                }
            }
        }
        assert_eq!(fs::read(path).unwrap(), original);
        assert_eq!(conflicts(0, td.path()).unwrap().len(), 1);
    }

    #[test]
    fn folded_editor_aligns_insertions_and_reopens_partial_decisions() {
        use crate::{
            core::Context,
            merge::{Hit, MergePanel},
            ui::{Action, Panel},
        };
        use crossterm::event::{
            Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
        };
        use ratatui::{backend::TestBackend, layout::Rect, Terminal};

        let mut base = (1..=100)
            .map(|n| format!("context {n:03}\n"))
            .collect::<Vec<_>>();
        base[19] = "function reviewOrders() {\n".into();
        let mut left = base.clone();
        let mut right = base.clone();
        left[1] = "identical edit\n".into();
        right[1] = left[1].clone();
        left[23] = "left transport\n".into();
        right[23] = "right transport\n".into();
        left[49] = "left safe\nextra line\n".into();
        right[74] = "right safe\n".into();
        let td = fixture_with(&base.concat(), &left.concat(), &right.concat());
        let ctx = Context::discover(td.path()).unwrap();
        let path = td.path().join("config with spaces.txt");
        let original = fs::read(&path).unwrap();
        let index = g(td.path(), &["ls-files", "--stage"]);
        let mut panel = MergePanel::default();
        panel.refresh(&ctx).unwrap();
        let mut term = Terminal::new(TestBackend::new(152, 54)).unwrap();
        let draw = |p: &mut MergePanel, term: &mut Terminal<TestBackend>| {
            term.draw(|f| p.draw(f, f.area(), &ctx)).unwrap();
        };
        let click = |p: &mut MergePanel, r: Rect| {
            p.event(
                &Event::Mouse(MouseEvent {
                    kind: MouseEventKind::Down(MouseButton::Left),
                    column: r.x,
                    row: r.y,
                    modifiers: KeyModifiers::NONE,
                }),
                &ctx,
            )
        };
        let press = |p: &mut MergePanel, code| {
            p.event(&Event::Key(KeyEvent::new(code, KeyModifiers::NONE)), &ctx)
        };
        let control = |p: &MergePanel, hit: fn(&Hit) -> bool| {
            p.hits.iter().find(|(_, target)| hit(target)).unwrap().0
        };
        press(&mut panel, KeyCode::Enter);
        draw(&mut panel, &mut term);
        let row = |y, cols: std::ops::Range<u16>| {
            cols.map(|x| term.backend().buffer()[(x, y)].symbol())
                .collect::<String>()
        };
        assert!(row(1, 0..152).contains("3 changes, 1 conflict"));
        assert!((4..53).any(|y| row(y, 0..48).contains(" reviewOrders() ∿")));
        let aligned = (4..53)
            .find(|&y| row(y, 0..152).matches("context 051").count() == 3)
            .unwrap();
        assert!(row(aligned, 0..48).contains("52"));
        assert!(row(aligned, 48..104).contains("51"));
        assert!(row(aligned, 104..152).contains("51"));

        let folded = panel.rows_total;
        let fold = control(&panel, |h| matches!(h, Hit::Fold(_)));
        click(&mut panel, fold);
        draw(&mut panel, &mut term);
        assert!(panel.rows_total > folded);
        click(&mut panel, fold);
        draw(&mut panel, &mut term);
        assert_eq!(panel.rows_total, folded);

        let auto_left = control(&panel, |h| matches!(h, Hit::Auto(Some(FileSide::Current))));
        click(&mut panel, auto_left);
        draw(&mut panel, &mut term);
        assert_eq!(panel.doc.as_ref().unwrap().pending(), 2);
        let accept = control(&panel, |h| {
            matches!(h, Hit::Decide(_, FileSide::Current, true))
        });
        click(&mut panel, accept);
        assert_eq!(
            panel.doc.as_ref().unwrap().chunks[panel.idx].result,
            "left transport\n"
        );
        assert_eq!(panel.doc.as_ref().unwrap().pending(), 2);
        let save = control(&panel, |h| matches!(h, Hit::Save));
        assert!(matches!(click(&mut panel, save), Action::Notice(_)));
        assert!(panel.at_root());
        assert_eq!(fs::read(&path).unwrap(), original);
        assert_eq!(g(td.path(), &["ls-files", "--stage"]), index);

        press(&mut panel, KeyCode::Enter);
        draw(&mut panel, &mut term);
        let ignore = control(&panel, |h| {
            matches!(h, Hit::Decide(_, FileSide::Incoming, false))
        });
        click(&mut panel, ignore);
        let doc = panel.doc.as_ref().unwrap();
        assert_eq!(doc.chunks[panel.idx].result, "left transport\n");
        assert_eq!(doc.pending(), 1);
        press(&mut panel, KeyCode::Char('a'));
        assert!(matches!(
            press(&mut panel, KeyCode::Char('A')),
            Action::Confirm { .. }
        ));
        assert_eq!(fs::read(&path).unwrap(), original);
        assert_eq!(g(td.path(), &["ls-files", "--stage"]), index);
    }

    #[test]
    fn pane_navigation_merge_drag_and_zoom_preserve_the_current_conflict() {
        use crate::{
            core::Context,
            merge::MergePanel,
            ui::{
                panes::{Axis, PaneId},
                Panel,
            },
        };
        use crossterm::event::{
            Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
        };
        use ratatui::{backend::TestBackend, Terminal};

        let td = fixture();
        let ctx = Context::discover(td.path()).unwrap();
        let mut p = MergePanel::default();
        let draw = |p: &mut MergePanel, w| {
            let mut term = Terminal::new(TestBackend::new(w, 24)).unwrap();
            term.draw(|f| p.draw(f, f.area(), &ctx)).unwrap();
            term.backend().buffer().clone()
        };
        let key = |p: &mut MergePanel, code| {
            p.event(&Event::Key(KeyEvent::new(code, KeyModifiers::NONE)), &ctx);
        };
        let mouse = |p: &mut MergePanel, kind, x, y| {
            p.event(
                &Event::Mouse(MouseEvent {
                    kind,
                    column: x,
                    row: y,
                    modifiers: KeyModifiers::NONE,
                }),
                &ctx,
            );
        };
        p.refresh(&ctx).unwrap();
        draw(&mut p, 120);
        assert_eq!(p.focused_pane(), PaneId::Conflicts);
        assert!(!p
            .panes
            .can_resize(p.panes.rect(PaneId::Conflicts).unwrap(), Axis::Width));
        key(&mut p, KeyCode::Enter);
        let before = (p.idx, p.doc.as_ref().unwrap().pending());
        let frame = draw(&mut p, 120);
        let result = p.panes.rect(PaneId::Result).unwrap();
        let incoming = p.panes.rect(PaneId::Incoming).unwrap();
        assert_eq!(frame[(result.x, result.y + 2)].symbol(), "│");
        assert_eq!(frame[(incoming.x, incoming.y + 2)].symbol(), "│");
        let y = result.bottom() - 2;
        // Empty space should select a side without choosing a chunk or taking its text.
        mouse(
            &mut p,
            MouseEventKind::Down(MouseButton::Left),
            incoming.x + 2,
            y,
        );
        assert_eq!(p.focused_pane(), PaneId::Incoming);
        let x = incoming.x;
        mouse(&mut p, MouseEventKind::Down(MouseButton::Left), x, y);
        mouse(&mut p, MouseEventKind::Drag(MouseButton::Left), x - 8, y);
        mouse(&mut p, MouseEventKind::Up(MouseButton::Left), x - 8, y);
        draw(&mut p, 120);
        assert_eq!(
            p.panes.rect(PaneId::Result).unwrap().width,
            result.width - 8
        );
        assert_eq!(
            p.panes.rect(PaneId::Incoming).unwrap().width,
            incoming.width + 8
        );
        assert_eq!(p.focused_pane(), PaneId::Incoming);

        for width in [120, 60] {
            if width == 120 {
                key(&mut p, KeyCode::Char('z'));
            }
            draw(&mut p, width);
            let visible = p.panes.rect(p.focused_pane()).unwrap();
            assert_eq!(visible.width, width);
            assert!(!p.panes.can_resize(visible, Axis::Width));
            key(&mut p, KeyCode::Tab);
            draw(&mut p, width);
            assert_eq!(p.panes.rect(p.focused_pane()).unwrap().width, width);
            if width == 120 {
                key(&mut p, KeyCode::Char('z'));
            }
        }
        assert_eq!(p.focused_pane(), PaneId::Result);
        key(&mut p, KeyCode::Char('i'));
        draw(&mut p, 120);
        let x = p.panes.rect(PaneId::Current).unwrap().right() - 1;
        mouse(&mut p, MouseEventKind::Down(MouseButton::Left), x, y);
        mouse(&mut p, MouseEventKind::Up(MouseButton::Left), x - 4, y);
        key(&mut p, KeyCode::Tab);
        assert!(p.edit.as_ref().unwrap().chars.contains(&'\t'));
        key(&mut p, KeyCode::Esc);
        draw(&mut p, 120);
        assert_eq!(
            p.panes.rect(PaneId::Incoming).unwrap().width,
            incoming.width + 8
        );
        assert_eq!((p.idx, p.doc.as_ref().unwrap().pending()), before);
    }

    #[test]
    fn maximized_merge_keeps_side_order_actions_and_existing_zoom_preference() {
        use crate::{
            core::Context,
            merge::{Hit, MergePanel},
            ui::{
                panes::{Axis, PaneId},
                Panel,
            },
        };
        use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
        use ratatui::{backend::TestBackend, layout::Rect, Terminal};

        let td = fixture();
        let ctx = Context::discover(td.path()).unwrap();
        let original = fs::read(td.path().join("config with spaces.txt")).unwrap();
        let mut p = MergePanel::default();
        p.refresh(&ctx).unwrap();
        p.event(
            &Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            &ctx,
        );
        let before = (p.idx, p.doc.as_ref().unwrap().pending());
        let mut term = Terminal::new(TestBackend::new(120, 24)).unwrap();
        let draw = |p: &mut MergePanel, term: &mut Terminal<TestBackend>| {
            term.draw(|f| p.draw(f, f.area(), &ctx)).unwrap();
        };
        draw(&mut p, &mut term);
        let current = p.panes.rect(PaneId::Current).unwrap();
        assert!(p.panes.resize(current, Axis::Width, 7));
        draw(&mut p, &mut term);
        let restored = p.panes.nodes().to_vec();

        p.panes.maximized = true;
        for (side, id) in MergePanel::SIDES.into_iter().enumerate() {
            p.focus_pane(id);
            draw(&mut p, &mut term);
            assert_eq!(
                p.panes.nodes(),
                &MergePanel::SIDES.map(|other| (
                    other,
                    if id == other {
                        Rect::new(0, 2, 120, 21)
                    } else {
                        Rect::default()
                    }
                ))
            );
            let visible = p.panes.rect(id).unwrap();
            assert!(!p.panes.can_resize(visible, Axis::Width));
            for (area, hit) in &p.hits {
                if let Hit::Select(_, target) = hit {
                    assert_eq!(*target, side);
                    assert_eq!(area.intersection(visible), *area);
                }
            }
            for matches in [
                (|hit: &Hit| matches!(hit, Hit::List)) as fn(&Hit) -> bool,
                |hit| matches!(hit, Hit::UseFile(FileSide::Current)),
                |hit| matches!(hit, Hit::Save),
            ] {
                assert!(p.hits.iter().any(|(_, hit)| matches(hit)));
            }
            assert!(!p.zoom);
            assert_eq!((p.idx, p.doc.as_ref().unwrap().pending()), before);
        }
        p.panes.maximized = false;
        draw(&mut p, &mut term);
        assert_eq!(p.panes.nodes(), restored);

        p.zoom = true;
        p.panes.maximized = true;
        draw(&mut p, &mut term);
        p.panes.maximized = false;
        draw(&mut p, &mut term);
        assert!(p.zoom);
        assert_eq!(
            p.panes
                .nodes()
                .iter()
                .filter(|(_, area)| !area.is_empty())
                .count(),
            1
        );
        p.panes.maximized = true;
        for (width, height) in [(1, 5), (44, 4), (0, 0)] {
            term.draw(|f| p.draw(f, Rect::new(0, 0, width, height), &ctx))
                .unwrap();
            assert_eq!(p.panes.nodes().len(), 3);
        }
        assert_eq!(p.rows_height, 0);
        assert_eq!(p.rows_total, 0);
        assert!(p.hits.is_empty());
        assert_eq!(
            fs::read(td.path().join("config with spaces.txt")).unwrap(),
            original
        );
        assert_eq!(conflicts(0, td.path()).unwrap().len(), 1);
    }

    #[test]
    fn multi_root_tui_keyboard_and_mouse_apply_to_one_root() {
        use crate::{
            core::{Context, Repo},
            merge::{Hit, MergePanel},
            ui::{Action, Panel},
        };
        use crossterm::event::{
            Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
        };
        use ratatui::{backend::TestBackend, Terminal};
        let a = fixture();
        let b = fixture();
        let roots = [a.path(), b.path()];
        let ctx = Context {
            repos: roots
                .into_iter()
                .enumerate()
                .map(|(i, p)| Repo {
                    path: p.into(),
                    name: format!("repo-{i}"),
                    branch: "main".into(),
                    parent: None,
                    submodule: false,
                })
                .collect(),
            scope: [0, 1].into_iter().collect(),
            state_dir: a.path().into(),
            workspace: a.path().into(),
        };
        let mut panel = MergePanel::default();
        panel.refresh(&ctx).unwrap();
        assert_eq!(panel.list.files_in(0).count(), 1);
        assert_eq!(panel.list.files_in(1).count(), 1);
        let key = |code| Event::Key(KeyEvent::new(code, KeyModifiers::NONE));
        panel.event(&key(KeyCode::Char('j')), &ctx);
        panel.event(&key(KeyCode::Enter), &ctx);
        assert_eq!(panel.doc.as_ref().unwrap().file.root, 1);
        panel.event(&key(KeyCode::Char('a')), &ctx);
        let mut term = Terminal::new(TestBackend::new(145, 42)).unwrap();
        term.draw(|f| panel.draw(f, f.area(), &ctx)).unwrap();
        let (hit, _) = panel
            .hits
            .iter()
            .find(|(_, h)| matches!(h,Hit::Decide(i,FileSide::Incoming,true) if *i==panel.idx))
            .unwrap();
        let mouse = Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: hit.x,
            row: hit.y,
            modifiers: KeyModifiers::NONE,
        });
        panel.event(&mouse, &ctx);
        assert_eq!(panel.doc.as_ref().unwrap().pending(), 1);
        panel.event(&key(KeyCode::Char('h')), &ctx);
        panel.event(&key(KeyCode::Char('h')), &ctx);
        panel.event(&key(KeyCode::Char('x')), &ctx);
        assert_eq!(panel.doc.as_ref().unwrap().pending(), 0);
        panel.event(&key(KeyCode::Char('u')), &ctx);
        panel.event(&key(KeyCode::Char('u')), &ctx);
        assert_eq!(panel.doc.as_ref().unwrap().pending(), 1);
        panel.event(&key(KeyCode::Char('i')), &ctx);
        term.draw(|f| panel.draw(f, f.area(), &ctx)).unwrap();
        assert!(panel.editing());
        panel.event(&key(KeyCode::Char('!')), &ctx);
        panel.event(
            &Event::Key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL)),
            &ctx,
        );
        assert!(!panel.editing());
        assert!(matches!(
            panel.event(&key(KeyCode::Char('A')), &ctx),
            Action::Confirm { .. }
        ));
        assert!(matches!(
            panel.confirm("merge:apply", &ctx),
            Action::Refresh(_)
        ));
        assert!(!conflicts(0, a.path()).unwrap().is_empty());
        assert!(conflicts(1, b.path()).unwrap().is_empty());
        assert!(fs::read_to_string(b.path().join("config with spaces.txt"))
            .unwrap()
            .contains("shared!"));
        assert!(operation(b.path()).unwrap().is_some());
    }
}
