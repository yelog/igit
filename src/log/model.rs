mod branch_filter;
mod graph;
pub(super) mod merge_diff;
mod path_filter;
pub(super) use branch_filter::BranchFilter;
pub(super) use path_filter::PathFilter;
mod presentation;
pub(super) use graph::{graph_color, Graph, GraphWindow, NodeGlyphs};
#[cfg(test)]
mod tests;

use crate::core::{git, git_bytes, Context};
use anyhow::{anyhow, bail, Context as _, Result};
use std::{
    collections::{BTreeSet, BinaryHeap, HashMap, HashSet, VecDeque},
    io::Read,
    path::Path,
    process::Stdio,
};

pub(super) fn plain(v: &[u8]) -> String {
    let s = String::from_utf8_lossy(v);
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if c.is_control() {
            out.extend(c.escape_default());
        } else {
            out.push(c);
        }
    }
    out
}
#[derive(Clone, Default)]
pub(super) struct Filter {
    pub file_history: Option<super::file_history::Target>,
    pub query: String,
    pub author: String,
    pub date: Option<DateFilter>,
    pub paths: PathFilter,
    // None follows global Roots. An explicit empty set deliberately shows no commits.
    pub roots: Option<BTreeSet<usize>>,
    pub branch: Option<BranchFilter>,
}
impl Filter {
    pub fn single_branch(&self) -> Option<&str> {
        self.branch.as_ref().and_then(BranchFilter::single)
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum DateFilter {
    Last24Hours,
    Last7Days,
    Range { from: String, to: String },
}
impl DateFilter {
    pub fn label(&self) -> String {
        match self {
            Self::Last24Hours => "Last 24 hours".into(),
            Self::Last7Days => "Last 7 days".into(),
            Self::Range { from, to } if from == to => from.clone(),
            Self::Range { from, to } => format!("{from}..{to}"),
        }
    }

    fn args(&self, args: &mut Vec<String>) {
        let (since, until) = match self {
            Self::Last24Hours => ("24 hours ago".into(), "now".into()),
            Self::Last7Days => ("7 days ago".into(), "now".into()),
            Self::Range { from, to } => (format!("{from} 00:00:00"), format!("{to} 23:59:59")),
        };
        args.push(format!("--since-as-filter={since}"));
        args.push(format!("--until={until}"));
    }
}
#[derive(Clone, Default, PartialEq, Eq, Debug)]
pub(super) struct Branch {
    pub root: usize,
    pub name: String,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RefKind {
    Head,
    // The local branch HEAD points to, retaining its local identity and colour.
    Current,
    Local,
    Remote,
    Tag,
}
#[derive(Debug, Clone, Default)]
pub(super) struct Rev {
    pub root: usize,
    pub oid: String,
    pub parents: Vec<String>,
    pub ts: i64,
    pub date: String,
    pub authored_date: String,
    pub author: String,
    pub email: String,
    pub subject: String,
    pub refs: String,
    pub file: Option<HistoryPath>,
}
#[derive(Clone, Debug)]
pub(super) struct HistoryPath {
    pub raw: Vec<u8>,
    pub before: Option<Vec<u8>>,
}
#[derive(Default)]
pub(super) struct History {
    pub graph_layout: graph::PermanentLayout,
    pub revs: Vec<Rev>,
    pub branches: Vec<Branch>,
    pub current: HashMap<usize, String>,
    pub tracking: HashMap<(usize, String), (usize, usize)>,
    pub remote_heads: HashSet<(usize, String)>,
    remote_names: HashMap<usize, Vec<String>>,
    pub more: BTreeSet<usize>,
    pub errors: Vec<(usize, String)>,
    upstreams: HashMap<usize, HashMap<String, String>>,
    on_head: HashMap<usize, HashSet<String>>,
    dates: Option<[String; 2]>,
    identities: HashMap<usize, String>,
    signatures: HashMap<(usize, String), char>,
    cherry_picked: HashMap<usize, HashSet<String>>,
}
impl History {
    pub fn split_remote<'a>(&self, root: usize, short: &'a str) -> Option<(&'a str, &'a str)> {
        // Remote names may contain '/'; a branch below "team/mirror" is not a branch of "team".
        self.remote_names
            .get(&root)
            .into_iter()
            .flatten()
            .find_map(|r| {
                short
                    .strip_prefix(r)?
                    .strip_prefix('/')
                    .map(|branch| (&short[..r.len()], branch))
            })
            .or_else(|| short.split_once('/'))
    }
    pub fn remote_upstream(&self, root: usize, local_short: &str) -> Option<&str> {
        self.upstreams
            .get(&root)?
            .get(local_short)
            .map(String::as_str)
    }

    pub fn on_head(&self, r: &Rev) -> bool {
        self.on_head
            .get(&r.root)
            .is_some_and(|ids| ids.contains(&r.oid))
    }

    pub fn display_date(&self, r: &Rev) -> String {
        self.format_date(&r.date)
    }

    pub fn display_author_date(&self, r: &Rev) -> String {
        self.format_date(&r.authored_date)
    }

    fn format_date(&self, value: &str) -> String {
        if let (Some([today, yesterday]), Some((date, time))) =
            (&self.dates, value.split_once(", "))
        {
            if date == today {
                return format!("Today {time}");
            }
            if date == yesterday {
                return format!("Yesterday {time}");
            }
        }
        value.to_owned()
    }
}

// Obtain the current timestamp and local UTC offset from the same Git runtime that
// formats commit dates. The process-local identity lets this work before user setup.
fn local_dates(dir: &Path) -> Option<[String; 2]> {
    let o = crate::core::command(dir)
        .env("GIT_AUTHOR_NAME", "IGit")
        .env("GIT_AUTHOR_EMAIL", "clock@invalid")
        .env_remove("GIT_AUTHOR_DATE")
        .args(["var", "GIT_AUTHOR_IDENT"])
        .output()
        .ok()?;
    o.status.success().then_some(())?;
    dates_from_ident(std::str::from_utf8(&o.stdout).ok()?)
}

fn dates_from_ident(ident: &str) -> Option<[String; 2]> {
    let mut parts = ident.split_whitespace().rev();
    let zone = parts.next()?;
    let hhmm: i64 = zone.get(1..5)?.parse().ok()?;
    let minutes = hhmm / 100 * 60 + hhmm % 100;
    let offset = if zone.starts_with('-') {
        -minutes
    } else {
        minutes
    };
    let ts: i64 = parts.next()?.parse().ok()?;
    let today = (ts + 60 * offset).div_euclid(86_400);
    Some([civil_date(today), civil_date(today - 1)])
}

// March-based Gregorian calendar: move the leap day to the end of the year, then
// split days since the Unix epoch into 400-year cycles, years, months and days.
fn civil_date(epoch_day: i64) -> String {
    let z = epoch_day + 719_468;
    let day_in_cycle = z.rem_euclid(146_097);
    let year_in_cycle =
        (day_in_cycle - day_in_cycle / 1460 + day_in_cycle / 36_524 - day_in_cycle / 146_096) / 365;
    let day_in_year =
        day_in_cycle - (365 * year_in_cycle + year_in_cycle / 4 - year_in_cycle / 100);
    let month_in_year = (5 * day_in_year + 2) / 153;
    let day = day_in_year - (153 * month_in_year + 2) / 5 + 1;
    let month = if month_in_year < 10 {
        month_in_year + 3
    } else {
        month_in_year - 9
    };
    let year = z.div_euclid(146_097) * 400 + year_in_cycle + i64::from(month <= 2);
    format!("{year}/{month}/{day}")
}

// These are the refs the Log actually shows. --all would also include stash/index
// snapshots, bisect refs and private tool refs. HEAD also covers a detached checkout.
fn all_tips() -> Vec<String> {
    ["--branches", "--remotes", "--tags", "HEAD"]
        .map(str::to_owned)
        .into()
}

fn revisions(
    dir: &Path,
    root: usize,
    f: &Filter,
    tips: &[String],
    limit: usize,
) -> Result<Vec<Rev>> {
    let paths = if let Some(file) = &f.file_history {
        (file.root == root).then(|| vec![file.path.as_path()])
    } else {
        f.paths.for_root(root)
    };
    let Some(paths) = paths else {
        return Ok(Vec::new());
    };
    let mut a = vec![
        "--literal-pathspecs".to_owned(),
        "log".into(),
        "--date-order".into(),
        "--no-show-signature".into(),
        "--no-notes".into(),
        "--no-color".into(),
        "--decorate=full".into(),
        "--decorate-refs=HEAD".into(),
        "--decorate-refs=refs/heads/*".into(),
        "--decorate-refs=refs/remotes/*".into(),
        "--decorate-refs=refs/tags/*".into(),
        "--decorate-refs-exclude=refs/remotes/*/HEAD".into(),
        "--ignore-missing".into(),
        "--date=format-local:%Y/%m/%d, %H:%M".into(),
        "-z".into(),
        format!("--max-count={limit}"),
        "--format=%H%x00%P%x00%ct%x00%cd%x00%an%x00%s%x00%D%x00%ae%x00%ad".into(),
    ];
    if f.author.eq_ignore_ascii_case("me") {
        // Include the delimiters: an email with dots or '+' is a literal identity,
        // not an author regex or a substring of a teammate's address.
        let Ok(ident) = git(dir, &["var", "GIT_AUTHOR_IDENT"]) else {
            return Ok(vec![]);
        };
        let Some((email, _)) = ident.rsplit_once('<').and_then(|(_, s)| s.split_once('>')) else {
            return Ok(vec![]);
        };
        if email.is_empty() {
            return Ok(vec![]);
        }
        a.push("--fixed-strings".into());
        a.push(format!("--author=<{email}>"));
    } else if !f.author.is_empty() {
        a.push(format!("--author={}", f.author))
    }
    if let Some(date) = &f.date {
        date.args(&mut a);
    }
    if f.file_history.is_some() {
        a.extend(
            [
                "--follow",
                "--full-history",
                "--diff-merges=separate",
                "--name-status",
                "--find-renames",
                "--no-ext-diff",
                "--no-textconv",
            ]
            .map(str::to_owned),
        );
    }
    a.extend_from_slice(tips);
    a.push("--".into());
    let output = crate::core::command(dir)
        .args(a)
        .args(paths)
        .output()
        .context("git log")?;
    if !output.status.success() {
        bail!(
            "git log: {}",
            String::from_utf8_lossy(&output.stderr).trim_end()
        );
    }
    let data = output.stdout;
    let mut it = data.split(|b| *b == 0).peekable();
    let mut out = vec![];
    let mut seen = HashSet::new();
    while let Some(id) = it.next().filter(|s| !s.is_empty()) {
        let mut next = || {
            it.next()
                .ok_or_else(|| anyhow!("incomplete git log record"))
        };
        let mut rev = Rev {
            root,
            oid: plain(id),
            parents: plain(next()?)
                .split_whitespace()
                .map(str::to_owned)
                .collect(),
            ts: plain(next()?).parse().unwrap_or_default(),
            date: plain(next()?).replace("/0", "/"),
            author: plain(next()?),
            subject: plain(next()?),
            refs: plain(next()?),
            email: plain(next()?),
            authored_date: plain(next()?).replace("/0", "/"),
            file: None,
        };
        if f.file_history.is_some() {
            // -z keeps filenames literal. A status owns one path (two for a
            // rename); consuming those fields prevents hash-like filenames
            // from being confused with the next fixed-length commit record.
            while let Some(status) = it.peek().copied() {
                if (status.len() == 40 || status.len() == 64)
                    && status.iter().all(u8::is_ascii_hexdigit)
                {
                    break;
                }
                it.next();
                let status = status.strip_prefix(b"\n").unwrap_or(status);
                if status.is_empty() {
                    continue;
                }
                let first = it.next().context("missing history filename")?;
                let (raw, before) = if matches!(status.first(), Some(b'R' | b'C')) {
                    (
                        it.next().context("missing renamed history filename")?,
                        Some(first.to_vec()),
                    )
                } else {
                    (first, None)
                };
                rev.file = Some(HistoryPath {
                    raw: raw.to_vec(),
                    before,
                });
            }
        }
        // A merge has one name-status record per changed parent, but one row.
        if seen.insert(rev.oid.clone()) {
            out.push(rev);
        }
    }
    Ok(out)
}

impl Rev {
    pub fn decorations(&self) -> impl Iterator<Item = (RefKind, &str)> {
        self.refs.split(", ").filter_map(|r| {
            if r == "HEAD" {
                return Some((RefKind::Head, r));
            }
            for (prefix, kind) in [
                ("HEAD -> refs/heads/", RefKind::Current),
                ("refs/heads/", RefKind::Local),
                ("refs/remotes/", RefKind::Remote),
                ("tag: refs/tags/", RefKind::Tag),
            ] {
                if let Some(short) = r.strip_prefix(prefix) {
                    return Some((kind, short));
                }
            }
            None
        })
    }

    pub(super) fn is_head_tip(&self) -> bool {
        self.refs
            .split(", ")
            .any(|r| r == "HEAD" || r.starts_with("HEAD -> "))
    }
}

fn read_refs(dir: &Path, root: usize, h: &mut History) -> Result<()> {
    let data = git_bytes(
        dir,
        &[
            "for-each-ref",
            "--format=%(refname)%00%(HEAD)%00%(symref)%00%(upstream)%00%(upstream:track,nobracket)",
            "refs/heads",
            "refs/remotes",
            "refs/tags",
        ],
    )?;
    let mut nested_remote = false;
    for line in data.split(|&b| b == b'\n').filter(|v| !v.is_empty()) {
        let Ok(line) = std::str::from_utf8(line) else {
            continue;
        };
        let mut fields = line.split('\0');
        let (Some(name), Some(current), Some(target), Some(upstream), Some(track)) = (
            fields.next(),
            fields.next(),
            fields.next(),
            fields.next(),
            fields.next(),
        ) else {
            continue;
        };
        if name.starts_with("refs/remotes/") && !target.is_empty() {
            if name.ends_with("/HEAD") {
                h.remote_heads.insert((root, target.to_owned()));
            }
            continue;
        }
        if current == "*" {
            h.current.insert(root, name.to_owned());
        }
        nested_remote |= name
            .strip_prefix("refs/remotes/")
            .is_some_and(|s| s.matches('/').nth(1).is_some());
        if let (Some(local), Some(remote)) = (
            name.strip_prefix("refs/heads/"),
            upstream.strip_prefix("refs/remotes/"),
        ) {
            h.upstreams
                .entry(root)
                .or_default()
                .insert(local.to_owned(), remote.to_owned());
        }
        if !upstream.is_empty() && track != "gone" {
            let mut counts = (0, 0);
            for part in track.split(", ") {
                if let Some(n) = part.strip_prefix("ahead ") {
                    counts.0 = n.parse()?;
                } else if let Some(n) = part.strip_prefix("behind ") {
                    counts.1 = n.parse()?;
                }
            }
            h.tracking.insert((root, name.to_owned()), counts);
        }
        h.branches.push(Branch {
            root,
            name: name.to_owned(),
        });
    }
    if nested_remote {
        let mut names: Vec<_> = git(dir, &["remote"])?.lines().map(str::to_owned).collect();
        names.sort_by_key(|s| std::cmp::Reverse(s.len()));
        h.remote_names.insert(root, names);
    }
    Ok(())
}

fn reachable(
    dir: &Path,
    revs: &[Rev],
    f: &Filter,
    current: Option<&String>,
    tips: &[String],
) -> Result<HashSet<String>> {
    if revs.is_empty() {
        return Ok(HashSet::new());
    }
    if f.single_branch()
        .is_some_and(|b| b == "HEAD" || current.map(String::as_str) == Some(b))
    {
        return Ok(revs.iter().map(|r| r.oid.clone()).collect());
    }
    if f.branch.is_none() && f.date.is_none() && f.paths.is_empty() && f.author.is_empty() {
        // This is a topologically ordered prefix of *all* tips. Every child on a
        // path from HEAD to a visible parent is in this prefix, even with skewed clocks.
        let mut pending = HashSet::new();
        let mut reachable = HashSet::new();
        for r in revs {
            if r.is_head_tip() || pending.remove(r.oid.as_str()) {
                reachable.insert(r.oid.clone());
                pending.extend(r.parents.iter().map(String::as_str));
            }
        }
        return Ok(reachable);
    }
    // A filtered page may have omitted intermediate ancestors. Subtract HEAD in Git
    // once for the whole page. --since-as-filter preserves paths through clock skew.
    let min = revs.iter().map(|r| r.ts).min().unwrap();
    let max = revs.iter().map(|r| r.ts).max().unwrap();
    let mut a = vec![
        "rev-list".into(),
        "--ignore-missing".into(),
        format!("--since-as-filter=@{min}"),
        format!("--until=@{max}"),
    ];
    a.extend_from_slice(tips);
    a.extend(["^HEAD".into(), "--".into()]);
    let outside = git_bytes(dir, &a.iter().map(String::as_str).collect::<Vec<_>>())?;
    let outside: HashSet<_> = outside.split(|&c| c == b'\n').collect();
    Ok(revs
        .iter()
        .filter(|r| !outside.contains(r.oid.as_bytes()))
        .map(|r| r.oid.clone())
        .collect())
}

pub(super) fn load(c: &Context, f: &Filter, limit: usize) -> History {
    let mut file_context;
    let c = if let Some(file) = &f.file_history {
        file_context = c.clone();
        file_context.scope.retain(|root| *root == file.root);
        &file_context
    } else {
        c
    };
    let (graph_layout, errors) = graph::PermanentLayout::load(c);
    let mut h = History {
        graph_layout,
        errors,
        ..Default::default()
    };
    let mut streams = Vec::new();
    // Load a page for each global root; the tab can toggle these cached streams independently.
    if let Some(r) = c.scope.iter().find_map(|&i| c.repos.get(i)) {
        h.dates = local_dates(&r.path);
    }
    for &i in &c.scope {
        if f.file_history.as_ref().is_some_and(|file| file.root != i) {
            continue;
        }
        let Some(r) = c.repos.get(i) else { continue };
        let start = h.branches.len();
        if let Err(e) = read_refs(&r.path, i, &mut h) {
            h.errors.push((i, format!("{}: {e}", r.name)));
        }
        let tips = match &f.branch {
            Some(b) => match b.tips(&h.branches[start..]) {
                Some(tips) => tips,
                None => continue,
            },
            None => all_tips(),
        };
        match revisions(&r.path, i, f, &tips, limit.saturating_add(1)) {
            Ok(mut v) => {
                if v.len() > limit {
                    h.more.insert(i);
                }
                v.truncate(limit);
                match reachable(&r.path, &v, f, h.current.get(&i), &tips) {
                    Ok(ids) => {
                        h.on_head.insert(i, ids);
                    }
                    Err(e) => h.errors.push((i, format!("{}: {e}", r.name))),
                }
                streams.push(VecDeque::from(v));
            }
            Err(e) => h.errors.push((i, format!("{}: {e}", r.name))),
        }
    }
    // Merge only stream heads, so a bad committer clock cannot put a parent above its child.
    let mut heap = BinaryHeap::new();
    for (i, q) in streams.iter().enumerate() {
        if let Some(r) = q.front() {
            heap.push((r.ts, std::cmp::Reverse(i)))
        }
    }
    while let Some((_, std::cmp::Reverse(i))) = heap.pop() {
        let q = &mut streams[i];
        h.revs.push(q.pop_front().unwrap());
        if let Some(r) = q.front() {
            heap.push((r.ts, std::cmp::Reverse(i)))
        }
    }
    h
}
#[derive(Clone, Default)]
pub(super) struct File {
    pub status: String,
    pub raw: Vec<u8>,
    pub parent: Option<usize>,
}
pub(super) struct Parent {
    pub oid: String,
    pub subject: String,
    pub files: std::ops::Range<usize>,
}
#[derive(Default)]
pub(super) struct Details {
    pub files: Vec<File>,
    pub parents: Vec<Parent>,
    pub body: String,
    pub email: String,
    pub branches: Vec<(RefKind, String)>,
}

fn containing_branches(path: &Path, oid: &str) -> Result<Vec<(RefKind, String)>> {
    // Decorations are refs pointing *at* this commit. The inspector instead
    // needs the local and remote branches whose tips can *reach* it.
    let refs = git_bytes(
        path,
        &[
            "for-each-ref",
            "--contains",
            oid,
            "--format=%(refname:short)%00%(refname)%00%(symref)%00%(HEAD)",
            "refs/heads",
            "refs/remotes",
        ],
    )?;
    let mut branches = vec![];
    let mut head = false;
    for line in refs.split(|&b| b == b'\n').filter(|s| !s.is_empty()) {
        let mut fields = line.split(|&b| b == 0);
        if let (Some(name), Some(full_name), Some(target), Some(current)) =
            (fields.next(), fields.next(), fields.next(), fields.next())
        {
            if target.is_empty() {
                let kind = if current == b"*" {
                    RefKind::Current
                } else if full_name.starts_with(b"refs/heads/") {
                    RefKind::Local
                } else {
                    RefKind::Remote
                };
                branches.push((kind, plain(name)));
                head |= current == b"*";
            }
        }
    }
    // A detached HEAD has no branch for for-each-ref to report. This also
    // correctly omits an unborn HEAD while inspecting an existing tag/remote.
    if head
        || git_bytes(
            path,
            &[
                "rev-list",
                "--ignore-missing",
                "--max-count=1",
                oid,
                "^HEAD",
                "--",
            ],
        )?
        .is_empty()
    {
        branches.insert(0, (RefKind::Head, "HEAD".into()));
    }
    Ok(branches)
}

pub(super) fn details(path: &Path, r: &Rev) -> Result<Details> {
    let (files, parents) = merge_diff::changed_files(path, r)?;
    let msg = git_bytes(
        path,
        &[
            "show",
            "-s",
            "--no-show-signature",
            "--no-notes",
            "--format=%ae%x00%b",
            &r.oid,
            "--",
        ],
    )?;
    let mut fields = msg.splitn(2, |&b| b == 0);
    let email = fields.next().unwrap_or_default();
    let body = fields
        .next()
        .ok_or_else(|| anyhow!("incomplete git show record"))?;
    Ok(Details {
        files,
        parents,
        email: plain(email),
        branches: containing_branches(path, &r.oid)?,
        body: body
            .split(|b| *b == b'\n')
            .map(plain)
            .collect::<Vec<_>>()
            .join("\n")
            .trim_end_matches('\n')
            .into(),
    })
}
pub(super) fn path_from_bytes(bytes: &[u8]) -> Result<&Path> {
    #[cfg(unix)]
    {
        use std::{ffi::OsStr, os::unix::ffi::OsStrExt};
        Ok(Path::new(OsStr::from_bytes(bytes)))
    }
    #[cfg(not(unix))]
    {
        Ok(Path::new(std::str::from_utf8(bytes)?))
    }
}

pub(super) fn patch(path: &Path, r: &Rev, file: &File) -> Result<Vec<String>> {
    let parent = file.parent.unwrap_or(0);
    let combined = file.parent.is_none() && r.parents.len() > 1;
    let file = path_from_bytes(&file.raw)?;
    let mut cmd = crate::core::command(path);
    cmd.arg("--literal-pathspecs");
    if let Some(p) = r.parents.get(parent).filter(|_| !combined) {
        cmd.args([
            "diff",
            "--no-ext-diff",
            "--no-textconv",
            "--no-color",
            "--no-renames",
            p,
            &r.oid,
        ]);
    } else {
        cmd.args([
            "show",
            "--format=",
            "--root",
            "--no-ext-diff",
            "--no-textconv",
            "--no-color",
            "--no-renames",
            "--no-show-signature",
            &r.oid,
        ]);
    }
    if combined {
        cmd.arg("-c");
    }
    const LIMIT: usize = 512 * 1024;
    let mut child = cmd
        .arg("--")
        .arg(file)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .stdin(Stdio::null())
        .spawn()
        .context("git diff")?;
    let mut raw = vec![];
    let result = child
        .stdout
        .take()
        .unwrap()
        .take((LIMIT + 1) as u64)
        .read_to_end(&mut raw);
    let cut = raw.len() > LIMIT;
    if cut || result.is_err() {
        let _ = child.kill();
    }
    let status = child.wait()?;
    result?;
    if !cut && !status.success() {
        bail!("git diff failed ({status})")
    }
    raw.truncate(LIMIT);
    let mut lines: Vec<_> = raw.split(|b| *b == b'\n').map(plain).collect();
    if cut {
        lines.push("… Diff limited to 512 KiB · Use git show for the full patch".into())
    }
    Ok(lines)
}
