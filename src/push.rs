mod commit_copy;
#[cfg(test)]
mod copy_tests;
mod diff;
mod display;
mod files;
mod keymap;
mod loading;
mod preview;
mod search;
mod submodules;
mod target;
mod update;
mod upstream;
mod view;
#[cfg(test)]
mod view_tests;
use crate::{
    core::{
        command, git, git_bytes,
        loading::{Load, Scope},
        redact_remote_error,
        strategy::Strategy,
        update::published_graph,
        Context, Repo,
    },
    ui::{
        icons::{Icon, IconMode},
        idea::{checkbox, GREEN, RED},
        panes::{Axis, PaneId, PaneLayout, SplitId},
        selection::{TextLayout, TextSelection, Wrap as TextWrap},
        *,
    },
};
use anyhow::{bail, Context as _, Result};
use crossterm::event::{Event, KeyCode, KeyModifiers, MouseButton, MouseEventKind};
use files::{Change, FileTree, Group, Grouping, Kind};
use ratatui::{
    layout::{Constraint as C, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Clear, Paragraph},
    Frame,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver},
};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Target {
    remote: String,
    dst: String,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Out {
    id: String,
    msg: String,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Entry {
    idx: usize,
    source: String,
    head: String,
    t: Target,
    tracking: Option<String>,
    config: upstream::Snapshot,
    n: usize,
    behind: Option<usize>,
    logs: Vec<Out>,
    error: String,
}
#[derive(Clone, Debug)]
struct Job {
    idx: usize,
    path: PathBuf,
    branch: String,
    head: String,
    t: Target,
    tracking: Option<String>,
    url: String,
    config: upstream::Snapshot,
    set_upstream: bool,
    focused_ref: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum PushOutcome {
    Complete,
    Updated(Strategy),
    UpToDate,
    UpstreamNotSet(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct TrackingChoice {
    branch: String,
    target: Target,
}
#[derive(Clone, Default)]
struct Preview {
    files: Vec<Change>,
    tree: FileTree,
    summary: String,
    body: String,
}
enum Report {
    Row(usize, Result<PushOutcome, String>),
    Phase(usize, &'static str),
    Conflicts(usize),
    Done,
}
#[derive(Clone, Copy)]
enum Hit {
    Row(usize),
    Check(usize),
    Fold(usize),
    Target(usize),
    File(usize),
    FileFold(usize),
    FoldAll(bool),
    GroupBy,
    MenuSurface,
    Group(Group),
    Details,
    EditTarget,
    Cancel,
    Send,
    Retry,
    Hooks,
    Tags,
    Fetch,
    Strategy,
    Conflicts,
    TargetSurface,
    TargetField,
    Upstream,
    SaveTarget,
    CancelTarget,
}
#[derive(Default)]
pub struct PushPanel {
    pub(crate) shortcut_labels: BTreeMap<&'static str, String>,
    pub(crate) icons: IconMode,
    pub(crate) panes: PaneLayout,
    entries: Vec<Entry>,
    load: Load<Vec<Entry>>,
    load_scope: Option<Scope>,
    load_error: Option<String>,
    preview_load: Load<preview::Loaded>,
    preview_request: Option<preview::Request>,
    checked: BTreeSet<usize>,
    expanded: BTreeSet<usize>,
    targets: BTreeMap<usize, Target>,
    tracking_choices: BTreeMap<usize, TrackingChoice>,
    init: bool,
    pub(crate) reseed: Option<Vec<usize>>,
    pos: usize,
    offset: usize,
    focus: usize,
    copy_pending: bool,
    outgoing_search: crate::ui::search::Search<(usize, Option<usize>)>,
    file_search: crate::ui::search::Search,
    preview: Preview,
    diff: Option<diff::Diff>,
    grouping: Grouping,
    group_menu: Option<Group>,
    show_details: bool,
    detail_off: u16,
    detail_selection: TextSelection,
    target_edit: Option<target::Editor>,
    hits: Vec<(Rect, Hit)>,
    results: BTreeMap<usize, Result<PushOutcome, String>>,
    inflight: Option<Receiver<Report>>,
    busy: String,
    review: Vec<Job>,
    pub(crate) strategy: Strategy,
    review_strategy: Option<Strategy>,
    conflicted: BTreeSet<usize>,
    follow_tags: bool,
    skip_hooks: bool,
    last_fetch: bool,
    pub(crate) progress_seq: u64,
    writes_blocked: bool,
}
fn remote_list(p: &Path) -> Vec<String> {
    git(p, &["remote"])
        .unwrap_or_default()
        .lines()
        .map(str::to_owned)
        .collect()
}
#[cfg(test)]
fn get_t(r: &Repo, branch: &str) -> Target {
    upstream::Snapshot::read(&r.path, branch)
        .unwrap_or_default()
        .target(r, branch)
}
fn remote_head(p: &Path, t: &Target) -> Option<String> {
    git(
        p,
        &[
            "rev-parse",
            "--verify",
            &format!("refs/remotes/{}/{}", t.remote, t.dst),
        ],
    )
    .ok()
}
fn entry(i: usize, r: &Repo, t: Option<&Target>) -> Entry {
    let mut x = Entry {
        idx: i,
        ..Default::default()
    };
    let run = || -> Result<Entry> {
        let source = git(&r.path, &["symbolic-ref", "--short", "-q", "HEAD"])
            .map_err(|_| anyhow::anyhow!("detached HEAD: switch to a branch"))?;
        let head = git(&r.path, &["rev-parse", "--verify", "HEAD"])
            .map_err(|_| anyhow::anyhow!("no commits yet"))?;
        let config = upstream::Snapshot::read(&r.path, &source)?;
        let t = t.cloned().unwrap_or_else(|| config.target(r, &source));
        if t.remote.is_empty() {
            bail!("no remote configured")
        }
        let tracking = remote_head(&r.path, &t);
        let range = tracking
            .as_ref()
            .map(|h| format!("{h}..{head}"))
            .unwrap_or_else(|| head.clone());
        let (n, behind) = if let Some(tip) = &tracking {
            let output = published_graph(&r.path)
                .args([
                    "rev-list",
                    "--left-right",
                    "--count",
                    &format!("{head}...{tip}"),
                    "--",
                ])
                .output()?;
            if !output.status.success() {
                bail!("Cannot compare the source and target history")
            }
            let text = String::from_utf8(output.stdout)?;
            let mut counts = text.split_whitespace();
            (
                counts
                    .next()
                    .context("Outgoing count unavailable")?
                    .parse()?,
                Some(
                    counts
                        .next()
                        .context("Incoming count unavailable")?
                        .parse()?,
                ),
            )
        } else {
            (
                git(&r.path, &["rev-list", "--count", &range])?.parse()?,
                None,
            )
        };
        let logs = if n == 0 {
            Vec::new()
        } else {
            git(
                &r.path,
                &["log", "--max-count=60", "--format=%H%x09%s", &range, "--"],
            )?
            .lines()
            .filter_map(|x| x.split_once('\t'))
            .map(|(id, msg)| Out {
                id: id.into(),
                msg: msg.into(),
            })
            .collect()
        };
        Ok(Entry {
            idx: i,
            source,
            head,
            t,
            tracking,
            config,
            n,
            behind,
            logs,
            error: String::new(),
        })
    };
    match run() {
        Ok(v) => v,
        Err(e) => {
            x.error = e.to_string();
            x
        }
    }
}
fn url(p: &Path, r: &str) -> Result<String> {
    let s = git(p, &["remote", "get-url", "--push", "--all", r])?;
    if s.lines().count() != 1 {
        bail!("multiple push URLs; use Git to inspect this remote")
    }
    Ok(s)
}

fn advertised_tip(path: &Path, endpoint: &str, reference: &str) -> Result<Option<String>> {
    let output = git(path, &["ls-remote", "--refs", "--", endpoint, reference])?;
    let mut tip = None;
    for line in output.lines() {
        if let Some((oid, name)) = line.split_once('\t') {
            if name == reference {
                if tip.is_some()
                    || !matches!(oid.len(), 40 | 64)
                    || !oid.bytes().all(|byte| byte.is_ascii_hexdigit())
                {
                    bail!("Cannot verify the exact push target");
                }
                tip = Some(oid.to_owned());
            }
        }
    }
    Ok(tip)
}

fn job(repo: &Repo, x: &Entry) -> Result<Job> {
    if x.t.remote.starts_with('-')
        || x.t.remote == "."
        || !remote_list(&repo.path).contains(&x.t.remote)
    {
        bail!("pick a configured remote")
    }
    git(
        &repo.path,
        &["check-ref-format", &format!("refs/heads/{}", x.t.dst)],
    )?;
    if git(
        &repo.path,
        &["config", "--bool", &format!("remote.{}.mirror", x.t.remote)],
    )
    .as_deref()
    .ok()
        == Some("true")
    {
        bail!("mirror remote requires a separate manual review")
    }
    let j = Job {
        idx: x.idx,
        path: repo.path.clone(),
        branch: x.source.clone(),
        head: x.head.clone(),
        url: url(&repo.path, &x.t.remote)?,
        t: x.t.clone(),
        tracking: x.tracking.clone(),
        config: x.config.clone(),
        set_upstream: false,
        focused_ref: None,
    };
    j.revalidate()?;
    Ok(j)
}

impl Job {
    fn revalidate(&self) -> Result<()> {
        let source_matches = if let Some(reference) = &self.focused_ref {
            git(&self.path, &["rev-parse", "--verify", reference])? == self.head
        } else {
            git(&self.path, &["symbolic-ref", "--short", "-q", "HEAD"])? == self.branch
                && git(&self.path, &["rev-parse", "HEAD"])? == self.head
        };
        if !source_matches
            || url(&self.path, &self.t.remote)? != self.url
            || upstream::Snapshot::read(&self.path, &self.branch)? != self.config
        {
            bail!("branch, HEAD, remote or upstream config changed; refresh and review");
        }
        if git(&self.path, &["ls-remote", "--get-url", "--", &self.url])? != self.url {
            bail!("Push URL resolves to a different endpoint; inspect URL rewrite rules before retrying");
        }
        if git(
            &self.path,
            &[
                "config",
                "--bool",
                &format!("remote.{}.mirror", self.t.remote),
            ],
        )
        .as_deref()
        .ok()
            == Some("true")
        {
            bail!("mirror remote requires a separate manual review");
        }
        Ok(())
    }
}

pub(crate) struct FocusedPush(Job);

impl FocusedPush {
    pub(crate) fn prepare(
        repo: &Repo,
        root: usize,
        reference: &str,
        head: &str,
        remote: &str,
        destination: &str,
        tracking_ref: &str,
    ) -> Result<Self> {
        let branch = reference
            .strip_prefix("refs/heads/")
            .context("Select a local branch")?;
        let job = Job {
            idx: root,
            path: repo.path.clone(),
            branch: branch.into(),
            head: head.into(),
            t: Target {
                remote: remote.into(),
                dst: destination.into(),
            },
            tracking: git(&repo.path, &["rev-parse", "--verify", tracking_ref]).ok(),
            url: url(&repo.path, remote)?,
            config: upstream::Snapshot::read(&repo.path, branch)?,
            set_upstream: false,
            focused_ref: Some(reference.into()),
        };
        job.revalidate()?;
        Ok(Self(job))
    }

    pub(crate) fn execute(&self) -> Result<()> {
        push_one(&self.0, false, false)?;
        Ok(())
    }
}

fn push_one(j: &Job, tags: bool, nohooks: bool) -> Result<PushOutcome> {
    let p = &j.path;
    j.revalidate()?;
    if j.set_upstream {
        j.config.writable_path(p, &j.branch)?;
    }
    let dst = format!("refs/heads/{}", j.t.dst);
    // Query the *push* endpoint, which may differ from the fetch endpoint.
    let actual = advertised_tip(p, &j.url, &dst)
        .map_err(|e| anyhow::anyhow!(redact_remote_error(&e.to_string(), &j.url)))?;
    if actual.as_deref() != j.tracking.as_deref() {
        bail!("remote tip differs from preview; Fetch, review target and retry (check push URL if different)")
    }
    if let Some(ref old) = j.tracking {
        match published_graph(p)
            .args(["merge-base", "--is-ancestor", old, &j.head])
            .output()?
            .status
            .code()
        {
            Some(0) => (),
            Some(1) => bail!("non-fast-forward; integrate upstream before retrying"),
            _ => bail!("cannot verify fast-forward against the stored history; fetch and review"),
        }
    }
    submodules::verify_for_remote(p, &j.head, j.tracking.as_deref(), Some(&j.url))?;
    j.revalidate()?;
    let spec = format!("{}:{}", j.head, dst);
    // A lease performs a compare-and-swap on the reviewed target. The ancestry check above still forbids rewritten history.
    let lease = format!(
        "--force-with-lease={}:{}",
        dst,
        j.tracking.as_deref().unwrap_or("")
    );
    let mut args = vec![
        "-c",
        "push.autoSetupRemote=false",
        "push",
        "--porcelain",
        "--recurse-submodules=check",
        if tags {
            "--follow-tags"
        } else {
            "--no-follow-tags"
        },
    ];
    if nohooks {
        args.push("--no-verify")
    }
    args.extend([lease.as_str(), "--", &j.t.remote, &spec]);
    git(p, &args).map_err(|e| anyhow::anyhow!(redact_remote_error(&e.to_string(), &j.url)))?;
    if j.set_upstream {
        // Git's --set-upstream does not configure a source branch for an OID
        // refspec. Keep the immutable push and publish tracking separately.
        if let Err(error) = upstream::set(j) {
            return Ok(PushOutcome::UpstreamNotSet(format!(
                "Pushed {} @ {:.10} to {}:{}; upstream not set: {}. Configure upstream with Git; Retry will not repeat this push.",
                j.branch, j.head, j.t.remote, j.t.dst, redact_remote_error(&error.to_string(), &j.url)
            )));
        }
    }
    Ok(PushOutcome::Complete)
}
fn label(p: &Path) -> String {
    p.to_string_lossy()
        .chars()
        .map(|c| if c.is_control() { '�' } else { c })
        .collect()
}

fn failure_summary(error: &str) -> String {
    const LIMIT: usize = 220;

    let lines: Vec<_> = error
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    let status = lines.first().copied().unwrap_or("Push failed");
    let remote_lines = || {
        lines
            .iter()
            .filter_map(|line| line.strip_prefix("remote:").map(str::trim))
            .filter(|line| !line.is_empty())
    };
    let detail = remote_lines()
        .find_map(|line| {
            ["GL-HOOK-ERR:", "error:", "fatal:"]
                .iter()
                .find_map(|prefix| line.strip_prefix(prefix).map(str::trim))
                .filter(|line| !line.is_empty())
        })
        .or_else(|| remote_lines().next());
    let prefix = [
        "Updated successfully; push failed:",
        "Push failed:",
        "Fetch failed; nothing pushed:",
    ]
    .iter()
    .find(|prefix| status.starts_with(**prefix))
    .copied()
    .unwrap_or("");
    let mut summary = match detail {
        Some(detail) if !prefix.is_empty() => format!("{prefix} {detail}"),
        Some(detail) => detail.to_owned(),
        None => status.to_owned(),
    };
    if summary.chars().count() > LIMIT {
        summary = summary.chars().take(LIMIT - 1).collect::<String>();
        summary.push('…');
    }
    summary
}

impl PushPanel {
    const PANES: [PaneId; 3] = [PaneId::Outgoing, PaneId::PushFiles, PaneId::PushDetails];

    pub(crate) fn set_strategy(&mut self, strategy: Strategy) {
        if !self.busy() && self.strategy != strategy {
            self.strategy = strategy;
            self.review.clear();
            self.review_strategy = None;
        }
    }

    fn choose_strategy(&self) -> Action {
        if self.busy() {
            Action::Notice(
                NoticeLevel::Info,
                "Wait for update and push to finish".into(),
            )
        } else {
            Action::ChooseUpdateStrategy
        }
    }

    pub(crate) fn path_copy_target(&self, c: &Context) -> Option<crate::ui::path_copy::Target> {
        if self.focus != 1 || self.editing() {
            return None;
        }
        let repo = &c.repos.get(self.selected()?.0)?.path;
        let (path, directory) = match &self.preview.tree.selected()?.kind {
            Kind::File(i) => (self.preview.files.get(*i)?.path.as_path(), false),
            Kind::Directory { path, .. } => (path.as_path(), true),
            Kind::Repository | Kind::Module => (Path::new(""), true),
        };
        Some(crate::ui::path_copy::Target::new(repo, path, directory))
    }

    pub(crate) fn focused_pane(&self) -> PaneId {
        Self::PANES[self.focus]
    }

    pub(crate) fn focus_pane(&mut self, id: PaneId) {
        if !self.editing() && (id != PaneId::PushDetails || self.show_details) {
            if let Some(i) = Self::PANES.iter().position(|&p| p == id) {
                self.cancel_key_sequence();
                self.focus = i;
            }
        }
    }

    fn active_pane(&self, id: PaneId) -> bool {
        self.panes.active
            && (!self.editing() || self.focused_search().is_some_and(|prompt| prompt.editing()))
            && self.focused_pane() == id
    }

    pub(crate) fn busy(&self) -> bool {
        self.inflight.is_some()
    }
    pub(crate) fn block_writes(&mut self, blocked: bool) {
        self.writes_blocked = blocked;
    }
    fn rows(&self) -> Vec<(usize, Option<usize>)> {
        let mut rows = vec![];
        for e in &self.entries {
            rows.push((e.idx, None));
            if self.expanded.contains(&e.idx) {
                rows.extend((0..e.logs.len()).map(|k| (e.idx, Some(k))))
            }
        }
        rows
    }
    fn selected(&self) -> Option<(usize, Option<usize>)> {
        self.rows().get(self.pos).copied()
    }
    fn update_details(&mut self) {
        if self.preview_load.loading()
            && self
                .preview_request
                .as_ref()
                .is_some_and(|request| request.file.is_none())
        {
            return;
        }
        self.detail_selection.reset();
        self.detail_off = 0;
        self.preview.body = match self.preview.tree.selected() {
            Some(files::Node {
                kind: Kind::Directory { path, .. },
                count,
                ..
            }) => {
                format!(
                    "{}\n\n{count} changed files\n\n{}",
                    label(path),
                    self.preview.summary
                )
            }
            _ => self.preview.summary.clone(),
        };
    }
    fn activate_file(&mut self, row: usize, fold: bool, c: &Context) {
        if row >= self.preview.tree.visible.len() {
            return;
        }
        self.preview.tree.cursor = row;
        if fold {
            self.preview.tree.toggle();
        } else if self.preview.tree.file_index().is_some() {
            self.open_file(c);
            return;
        }
        if self.show_details {
            self.update_details();
        }
    }
    fn toggle_details(&mut self, c: &Context) {
        if !self.show_details && self.focus == 1 && self.preview.tree.file_index().is_some() {
            self.open_file(c);
            return;
        }
        self.show_details = !self.show_details;
        if self.show_details {
            self.update_details();
        } else if self.focus == 2 {
            self.focus = 1;
        }
    }
    fn toggle(&mut self, i: usize) {
        if self.entries.iter().any(|e| e.idx == i && e.n > 0) && !self.checked.remove(&i) {
            self.checked.insert(i);
        }
    }
    fn target(&mut self, i: usize) {
        if self.read_status().is_some() || self.busy() {
            return;
        }
        if let Some(e) = self
            .entries
            .iter()
            .find(|e| e.idx == i && e.error.is_empty())
        {
            self.target_edit = Some(target::Editor {
                idx: i,
                text: format!("{}:{}", e.t.remote, e.t.dst),
                set_upstream: self.sets_upstream(e),
            });
            self.review.clear();
        }
    }
    fn review(&mut self, c: &Context, retry: bool) -> Action {
        if self.busy() {
            return Action::Notice(NoticeLevel::Info, self.busy.clone());
        }
        if let Some(message) = self.read_status() {
            return Action::Notice(
                if self.load.loading() {
                    NoticeLevel::Warning
                } else {
                    NoticeLevel::Error
                },
                message.into(),
            );
        }
        if self
            .load_scope
            .as_ref()
            .is_some_and(|scope| !scope.matches(c))
        {
            return Action::Notice(
                NoticeLevel::Warning,
                "Roots changed; refresh before reviewing a push".into(),
            );
        }
        if retry && self.last_fetch {
            return Action::Notice(
                NoticeLevel::Warning,
                "Retry Fetch after fixing the remote".into(),
            );
        }
        let mut js = vec![];
        let mut lines = vec![];
        self.review.clear();
        for e in &self.entries {
            if !c.scope.contains(&e.idx)
                || e.n == 0
                || !e.error.is_empty()
                || if retry {
                    !matches!(self.results.get(&e.idx), Some(Err(_)))
                } else {
                    !self.checked.contains(&e.idx)
                }
            {
                continue;
            }
            match job(&c.repos[e.idx], e) {
                Ok(mut j) => {
                    j.set_upstream = self.sets_upstream(e);
                    if j.set_upstream {
                        if let Err(error) = j.config.writable_path(&j.path, &j.branch) {
                            return Action::Notice(NoticeLevel::Error, error.to_string());
                        }
                    }
                    lines.push(format!(
                        "{}: {} @ {:.8} {} {}:{}  ({} commits)",
                        c.repos[e.idx].name,
                        e.source,
                        e.head,
                        self.icons.glyph(Icon::Right),
                        e.t.remote,
                        e.t.dst,
                        e.n
                    ));
                    lines.push(format!(
                        "  Upstream: {} {} {}",
                        e.config.description(&e.source),
                        self.icons.glyph(Icon::Right),
                        if j.set_upstream {
                            format!("{}:{} after push succeeds", e.t.remote, e.t.dst)
                        } else {
                            "keep unchanged".into()
                        }
                    ));
                    js.push(j)
                }
                Err(err) => return Action::Notice(NoticeLevel::Error, err.to_string()),
            }
        }
        if js.is_empty() {
            return Action::Notice(
                NoticeLevel::Warning,
                "No outgoing repository selected".into(),
            );
        }
        js.sort_by_key(|j| std::cmp::Reverse(j.path.components().count()));
        let count = js.len();
        self.review = js;
        self.review_strategy = Some(self.strategy);
        let strategy_detail = if self.strategy == Strategy::Rebase {
            "Rebase rewrites local commits and preserves merge topology."
        } else {
            "Merge preserves local commits and may create a merge commit."
        };
        Action::Confirm{title:format!("Push {count} repositories?"),body:format!("{}\n\nUpdate strategy: {}\nFetch each push target, update local commits when needed, then push.\n{}\nStop a repository on conflicts; resolve, continue and push again.\n\nHooks: {}   Follow annotated tags: {}\nEach repository is updated and pushed separately; no force push.",lines.join("\n"),self.strategy.label(),strategy_detail,!self.skip_hooks,self.follow_tags),token:"push".into()}
    }
    fn start(&mut self, c: &Context, fetch: bool) -> Action {
        if self.writes_blocked {
            return Action::Notice(
                NoticeLevel::Warning,
                "Wait for the current Git write before starting Push or Fetch".into(),
            );
        }
        if self.busy() {
            return Action::Notice(NoticeLevel::Info, self.busy.clone());
        }
        if let Some(message) = self.read_status() {
            return Action::Notice(
                if self.load.loading() {
                    NoticeLevel::Warning
                } else {
                    NoticeLevel::Error
                },
                message.into(),
            );
        }
        if self
            .load_scope
            .as_ref()
            .is_some_and(|scope| !scope.matches(c))
        {
            return Action::Notice(
                NoticeLevel::Warning,
                "Roots changed; refresh before starting a remote operation".into(),
            );
        }
        if !fetch {
            if self.review.is_empty() {
                return Action::Notice(
                    NoticeLevel::Warning,
                    "No reviewed push; select repositories and review again".into(),
                );
            }
            if self.review_strategy != Some(self.strategy) {
                self.review.clear();
                return Action::Notice(
                    NoticeLevel::Warning,
                    "Update strategy changed; review again".into(),
                );
            }
            if self.review.iter().any(|j| {
                !c.scope.contains(&j.idx)
                    || c.repos.get(j.idx).is_none_or(|repo| repo.path != j.path)
                    || !self.entries.iter().any(|e| {
                        e.idx == j.idx
                            && e.source == j.branch
                            && e.head == j.head
                            && e.t == j.t
                            && e.tracking == j.tracking
                            && e.config == j.config
                            && self.sets_upstream(e) == j.set_upstream
                            && self
                                .targets
                                .get(&e.idx)
                                .cloned()
                                .unwrap_or_else(|| e.config.target(&c.repos[e.idx], &e.source))
                                == j.t
                    })
            }) {
                self.review.clear();
                return Action::Notice(
                    NoticeLevel::Warning,
                    "Push target or tracking choice changed; review again".into(),
                );
            }
        }
        let (tx, rx) = mpsc::channel();
        let js = std::mem::take(&mut self.review);
        let (tags, hooks) = (self.follow_tags, self.skip_hooks);
        let strategy = self.strategy;
        let items: Vec<_> = self
            .entries
            .iter()
            .filter(|e| c.scope.contains(&e.idx))
            .map(|e| (e.idx, c.repos[e.idx].path.clone(), e.t.remote.clone()))
            .collect();
        let parents: Vec<_> = c.repos.iter().map(|r| r.parent).collect();
        self.last_fetch = fetch;
        self.busy = if fetch {
            "Fetching…"
        } else {
            "Updating and pushing…"
        }
        .into();
        self.inflight = Some(rx);
        if fetch {
            self.results.clear()
        } else {
            for j in &js {
                self.results.remove(&j.idx);
                self.conflicted.remove(&j.idx);
            }
        }
        std::thread::spawn(move || {
            if fetch {
                for (i, p, r) in items {
                    if r.is_empty() {
                        continue;
                    }
                    let res = git(&p, &["fetch", "--no-recurse-submodules", "--", &r])
                        .map(|_| PushOutcome::Complete)
                        .map_err(|e| {
                            let u = url(&p, &r).unwrap_or_else(|_| "<remote>".into());
                            redact_remote_error(&e.to_string(), &u)
                        });
                    let _ = tx.send(Report::Row(i, res));
                }
            } else {
                let mut blocked = BTreeSet::new();
                for j in js {
                    let res = if blocked.contains(&j.idx) {
                        Err("not executed: dependent submodule failed".into())
                    } else {
                        let result = update::execute(&j, strategy, tags, hooks, |phase| {
                            let _ = tx.send(Report::Phase(j.idx, phase));
                        })
                        .map_err(|e| e.to_string());
                        if result.is_err() && crate::core::update::conflicts(&j.path) {
                            let _ = tx.send(Report::Conflicts(j.idx));
                        }
                        result
                    };
                    if res.is_err() {
                        let mut anc = parents[j.idx];
                        while let Some(i) = anc {
                            blocked.insert(i);
                            anc = parents[i]
                        }
                    }
                    let _ = tx.send(Report::Row(j.idx, res));
                }
            }
            let _ = tx.send(Report::Done);
        });
        Action::Notice(NoticeLevel::Info, self.busy.clone())
    }
    pub(crate) fn poll(&mut self, c: &Context) -> Option<(NoticeLevel, String)> {
        let mut finished = false;
        let rx = self.inflight.as_ref()?;
        while let Ok(m) = rx.try_recv() {
            match m {
                Report::Row(i, r) => {
                    if r.is_ok() && !self.last_fetch {
                        self.tracking_choices.remove(&i);
                    }
                    self.results.insert(i, r);
                    self.progress_seq = self.progress_seq.wrapping_add(1);
                }
                Report::Done => finished = true,
                Report::Phase(i, phase) => {
                    self.busy = format!("{phase} · {}", label(Path::new(&c.repos[i].name)));
                    self.progress_seq = self.progress_seq.wrapping_add(1);
                }
                Report::Conflicts(i) => {
                    self.conflicted.insert(i);
                }
            }
        }
        if !finished {
            return None;
        }
        self.inflight = None;
        let n = self.results.values().filter(|r| r.is_err()).count();
        let upstream_failures = self
            .results
            .values()
            .filter(|r| matches!(r, Ok(PushOutcome::UpstreamNotSet(_))))
            .count();
        let level = if self.results.is_empty() {
            NoticeLevel::Info
        } else if n == self.results.len() {
            NoticeLevel::Error
        } else if n > 0 || upstream_failures > 0 {
            NoticeLevel::Warning
        } else {
            NoticeLevel::Success
        };
        let mut msg = if upstream_failures > 0 {
            if n == 0 {
                format!("Pushed · {upstream_failures} upstream setup failed · d details")
            } else {
                format!(
                    "{n} failed · {upstream_failures} pushed, upstream setup failed · d details"
                )
            }
        } else if n == 0 {
            if self.last_fetch {
                "Fetching complete".into()
            } else if self
                .results
                .values()
                .any(|result| matches!(result, Ok(PushOutcome::Updated(_))))
            {
                format!(
                    "Updated with {} and pushed successfully",
                    self.strategy.label()
                )
            } else if self
                .results
                .values()
                .all(|result| matches!(result, Ok(PushOutcome::UpToDate)))
            {
                "Up to date".into()
            } else {
                "Pushed successfully".into()
            }
        } else if !self.conflicted.is_empty() {
            format!("{n} failed / not executed · Resolve conflicts, continue, then push again")
        } else if n == 1 {
            self.results
                .values()
                .find_map(|result| result.as_ref().err())
                .map(|error| format!("1 failed · {}", failure_summary(error)))
                .unwrap_or_else(|| "1 failed · select row for details".into())
        } else {
            format!("{n} failed / not executed · select row for details")
        };
        for (root, result) in &self.results {
            if let Err(error) = result {
                let name = c
                    .repos
                    .get(*root)
                    .map(|repo| label(Path::new(&repo.name)))
                    .unwrap_or_else(|| "Repository".into());
                msg.push_str(&format!("\n\n{name}:\n{error}"));
            }
        }
        let _ = self.refresh(c);
        self.inspect(c);
        Some((level, msg))
    }
}
impl Panel for PushPanel {
    fn cancel_mouse(&mut self) {
        self.detail_selection.cancel_mouse();
        if let Some(diff) = &mut self.diff {
            diff.cancel_mouse();
        }
    }

    fn help(&self) -> &'static str {
        if self.diff.is_some() {
            "F1 Help   Esc Back"
        } else if self.target_edit.is_some() {
            "Type remote:branch · Tab tracking · Enter save · Esc cancel"
        } else if self.group_menu.is_some() {
            "Group By · j/k select · Space/Enter toggle · Esc close"
        } else {
            "j/k move · h/l fold · o group by · Enter diff · d details · [/] fold all · Space check root · t target · P push · F fetch · Tab/F6 pane"
        }
    }
    fn editing(&self) -> bool {
        self.diff.is_some()
            || self.target_edit.is_some()
            || self.group_menu.is_some()
            || self.focused_search().is_some_and(|prompt| prompt.editing())
    }
    fn selected_text(&self) -> Option<&str> {
        if let Some(diff) = &self.diff {
            diff.selected_text()
        } else if !self.editing() && self.show_details && self.focus == 2 {
            self.detail_selection.selected_text()
        } else {
            None
        }
    }
    fn refresh(&mut self, c: &Context) -> Result<()> {
        self.cancel_key_sequence();
        let old = self.selected_identity();
        let scope_changed = self
            .load_scope
            .as_ref()
            .is_some_and(|scope| !scope.matches(c));
        self.load_scope = Some(Scope::new(c));
        self.load_error = None;
        self.load.request();
        self.review.clear();
        self.entries.retain(|e| c.scope.contains(&e.idx));
        if scope_changed || old != self.selected_identity() {
            self.outgoing_search.invalidate();
            self.pos = old
                .as_ref()
                .and_then(|identity| self.position_for_identity(identity))
                .unwrap_or(0);
            self.inspect(c);
        }
        Ok(())
    }
    fn draw(&mut self, f: &mut Frame, a: Rect, c: &Context) {
        self.draw_panel(f, a, c);
    }
    fn event(&mut self, e: &Event, c: &Context) -> Action {
        if matches!(e, Event::Key(k) if k.kind == crossterm::event::KeyEventKind::Release) {
            return Action::None;
        }
        if self.editing()
            || matches!(e, Event::Paste(_) | Event::Resize(_, _) | Event::FocusLost)
            || matches!(e, Event::Mouse(mouse) if mouse.kind != MouseEventKind::Moved)
        {
            self.cancel_key_sequence();
        }
        if self.diff.is_some() {
            return self.diff_event(e, c);
        }
        if matches!(e, Event::FocusLost) {
            self.detail_selection.clear();
        }
        if self.group_menu_event(e) {
            return Action::None;
        }
        if let Some(action) = self.target_event(e, c) {
            return action;
        }
        if let Event::Key(key) = e {
            if let Some(action) = self.commit_copy_key(key, c) {
                return action;
            }
        }
        if self.search_event(e, c) {
            self.panes.cancel_drag();
            return Action::None;
        }
        if self.show_details && self.focus == 2 {
            if let Some(action) = self.detail_selection.key(e, true) {
                return action;
            }
        }
        if let Event::Mouse(m) = e {
            if self.panes.mouse(m) {
                return Action::None;
            }
            if let Some(action) = self
                .show_details
                .then(|| self.detail_selection.mouse(m))
                .flatten()
            {
                self.focus = 2;
                return action;
            }
        }
        let mut key = None;
        let mut hit = None;
        let mut motion_pane = self.focused_pane();
        match e {
            Event::Key(k) => key = Some(k.code),
            Event::Mouse(m) => {
                let (x, y) = (m.column, m.row);
                match m.kind {
                    MouseEventKind::Down(MouseButton::Left) => {
                        if let Some(id) = self.panes.at(x, y) {
                            self.focus_pane(id);
                        }
                        hit = self.hit_at(x, y);
                    }
                    MouseEventKind::ScrollDown | MouseEventKind::ScrollUp => {
                        if let Some(id) = self.panes.at(x, y) {
                            motion_pane = id;
                            key = Some(if m.kind == MouseEventKind::ScrollDown {
                                KeyCode::Down
                            } else {
                                KeyCode::Up
                            });
                        }
                    }
                    _ => {}
                }
            }
            _ => (),
        }
        if let Some(h) = hit {
            match h {
                Hit::Row(v) => {
                    self.pos = v;
                    self.inspect(c)
                }
                Hit::File(v) => {
                    self.focus = 1;
                    self.activate_file(v, false, c)
                }
                Hit::FileFold(v) => {
                    self.focus = 1;
                    self.activate_file(v, true, c)
                }
                Hit::FoldAll(close) => {
                    self.focus = 1;
                    self.preview.tree.fold_all(close);
                    if self.show_details {
                        self.update_details();
                    }
                }
                Hit::Details => self.toggle_details(c),
                Hit::EditTarget => {
                    if let Some((i, _)) = self.selected() {
                        self.target(i);
                    }
                }
                Hit::Cancel => return Action::CloseDialog,
                Hit::GroupBy => self.group_menu = Some(Group::Directory),
                Hit::MenuSurface | Hit::Group(_) => (),
                Hit::TargetSurface
                | Hit::TargetField
                | Hit::Upstream
                | Hit::SaveTarget
                | Hit::CancelTarget => (),
                Hit::Check(i) => self.toggle(i),
                Hit::Fold(i) => {
                    if !self.expanded.remove(&i) {
                        self.expanded.insert(i);
                    }
                    self.pos = self
                        .rows()
                        .iter()
                        .position(|x| *x == (i, None))
                        .unwrap_or(0);
                    self.inspect(c)
                }
                Hit::Send => return self.review(c, false),
                Hit::Retry => return self.review(c, true),
                Hit::Fetch => return self.start(c, true),
                Hit::Strategy => return self.choose_strategy(),
                Hit::Conflicts => return Action::OpenConflicts,
                Hit::Tags => self.follow_tags = !self.follow_tags,
                Hit::Hooks => self.skip_hooks = !self.skip_hooks,
                Hit::Target(i) => self.target(i),
            }
        }
        if let Some(k) = key {
            match k {
                KeyCode::Tab | KeyCode::F(6) | KeyCode::BackTab => {
                    let back = matches!(e,Event::Key(k) if k.modifiers.contains(KeyModifiers::SHIFT))
                        || k == KeyCode::BackTab;
                    let panes = if self.show_details { 3 } else { 2 };
                    self.focus = (self.focus + if back { panes - 1 } else { 1 }) % panes
                }
                KeyCode::Char('j') | KeyCode::Char('k') | KeyCode::Up | KeyCode::Down => {
                    let down = matches!(k, KeyCode::Down | KeyCode::Char('j'));
                    let mv = |v: usize, len: usize| {
                        if down {
                            (v + 1).min(len.saturating_sub(1))
                        } else {
                            v.saturating_sub(1)
                        }
                    };
                    match motion_pane {
                        PaneId::Outgoing => {
                            self.pos = mv(self.pos, self.rows().len());
                            self.inspect(c)
                        }
                        PaneId::PushFiles => {
                            self.preview.tree.cursor =
                                mv(self.preview.tree.cursor, self.preview.tree.visible.len());
                            if self.show_details {
                                self.update_details();
                            }
                        }
                        _ => {
                            self.detail_off = if down {
                                self.detail_off.saturating_add(1)
                            } else {
                                self.detail_off.saturating_sub(1)
                            }
                        }
                    }
                }
                KeyCode::Char('P') => return self.review(c, false),
                KeyCode::Char('R') => return self.review(c, true),
                KeyCode::Char('F') => return self.start(c, true),
                KeyCode::Char('s') => return self.choose_strategy(),
                KeyCode::Char('C') if self.conflicted.iter().any(|i| c.scope.contains(i)) => {
                    return Action::OpenConflicts
                }
                KeyCode::Char('v') => self.follow_tags = !self.follow_tags,
                KeyCode::Char('x') => self.skip_hooks = !self.skip_hooks,
                KeyCode::Char('d') => self.toggle_details(c),
                KeyCode::Char('o') => self.group_menu = Some(Group::Directory),
                KeyCode::Esc if self.show_details => {
                    self.show_details = false;
                    self.focus = 1;
                }
                KeyCode::Char('[' | ']') if self.focus == 1 => {
                    self.preview.tree.fold_all(k == KeyCode::Char('['));
                    if self.show_details {
                        self.update_details();
                    }
                }
                KeyCode::Char('h' | 'l') | KeyCode::Left | KeyCode::Right if self.focus == 1 => {
                    self.preview
                        .tree
                        .horizontal(matches!(k, KeyCode::Char('h') | KeyCode::Left));
                    if self.show_details {
                        self.update_details();
                    }
                }
                _ => {
                    if let Some((i, s)) = self.selected() {
                        match k {
                            KeyCode::Char(' ') => self.toggle(i),
                            KeyCode::Char('t') => self.target(i),
                            KeyCode::Char('h')
                            | KeyCode::Char('l')
                            | KeyCode::Left
                            | KeyCode::Right
                            | KeyCode::Enter
                                if self.focus == 0 =>
                            {
                                if s.is_some() && k == KeyCode::Enter {
                                    self.focus = 1
                                } else {
                                    if matches!(k, KeyCode::Char('h') | KeyCode::Left)
                                        || (k == KeyCode::Enter && self.expanded.contains(&i))
                                    {
                                        self.expanded.remove(&i);
                                    } else {
                                        self.expanded.insert(i);
                                    }
                                    self.pos = self
                                        .rows()
                                        .iter()
                                        .position(|r| *r == (i, None))
                                        .unwrap_or(0);
                                    self.inspect(c)
                                }
                            }
                            KeyCode::Enter if self.focus == 1 => {
                                if self.preview.tree.file_index().is_some() {
                                    self.open_file(c);
                                } else {
                                    self.preview.tree.toggle();
                                    if self.show_details {
                                        self.update_details();
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
        Action::None
    }
    fn confirm(&mut self, token: &str, c: &Context) -> Action {
        if token == "push" {
            self.start(c, false)
        } else {
            Action::None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use unicode_width::UnicodeWidthStr;
    fn g(p: &Path, a: &[&str]) -> String {
        git(p, a).unwrap()
    }
    pub(super) fn make(parent: &Path, n: &str) -> PathBuf {
        let repo = parent.join(n);
        let bare = parent.join(format!("{n}.git"));
        fs::create_dir(&repo).unwrap();
        fs::create_dir(&bare).unwrap();
        g(&bare, &["init", "--bare", "-q", "-b", "main"]);
        g(&repo, &["init", "-q", "-b", "main"]);
        for (k, v) in [
            ("user.name", "IGit test"),
            ("user.email", "test@invalid"),
            ("commit.gpgSign", "false"),
            ("core.hooksPath", "/dev/null"),
        ] {
            g(&repo, &["config", k, v]);
        }
        fs::write(repo.join("a"), "base").unwrap();
        g(&repo, &["add", "a"]);
        g(&repo, &["commit", "-qm", "base"]);
        g(&repo, &["remote", "add", "origin", bare.to_str().unwrap()]);
        g(&repo, &["push", "-q", "-u", "origin", "main"]);
        fs::write(repo.join("a"), "next").unwrap();
        g(&repo, &["commit", "-qam", "next"]);
        repo
    }

    #[test]
    fn rendered_push_controls_keep_browsing_and_review_separate_in_each_mode() {
        use crossterm::event::MouseEvent;
        use ratatui::{backend::TestBackend, Terminal};
        let td = tempfile::tempdir().unwrap();
        make(td.path(), "app");
        make(td.path(), "tools");
        let c = Context::discover(td.path()).unwrap();
        let click = |p: &mut PushPanel, x, y| {
            p.event(
                &Event::Mouse(MouseEvent {
                    kind: MouseEventKind::Down(MouseButton::Left),
                    column: x,
                    row: y,
                    modifiers: KeyModifiers::NONE,
                }),
                &c,
            )
        };
        let hit = |p: &PushPanel, matches: fn(&Hit) -> bool| {
            p.hits.iter().find(|(_, h)| matches(h)).unwrap().0
        };
        for icons in [IconMode::Ascii, IconMode::Unicode, IconMode::NerdFont] {
            let mut p = PushPanel {
                icons,
                ..Default::default()
            };
            p.refresh(&c).unwrap();
            p.finish_loading(&c);
            let mut term = Terminal::new(TestBackend::new(44, 16)).unwrap();
            let draw = |p: &mut PushPanel, term: &mut Terminal<TestBackend>| {
                term.draw(|f| p.draw(f, f.area(), &c)).unwrap();
            };
            let text = |term: &Terminal<TestBackend>, r: Rect| -> String {
                (r.x..r.right())
                    .map(|x| term.backend().buffer()[(x, r.y)].symbol())
                    .collect()
            };
            draw(&mut p, &mut term);
            let other = hit(&p, |h| matches!(h, Hit::Row(2)));
            let prefix = format!("{} ", icons.disclosure(false));
            let start = other.x + prefix.width() as u16;
            for offset in 0..icons.check(false).width() as u16 {
                for checked in [false, true] {
                    assert!(text(&term, other)
                        .starts_with(&format!("{prefix}{} ", icons.check(checked))));
                    click(&mut p, start + offset, other.y);
                    assert_eq!(p.checked.contains(&1), !checked);
                    assert_eq!(p.selected(), Some((0, None)));
                    draw(&mut p, &mut term);
                }
            }
            click(
                &mut p,
                start + icons.check(false).width() as u16 + 1,
                other.y,
            );
            assert_eq!(p.selected(), Some((1, None)));
            assert_eq!(p.checked, BTreeSet::from([0]));
            click(&mut p, other.x, other.y);
            assert!(p.expanded.contains(&1));
            assert_eq!(p.checked, BTreeSet::from([0]));

            p.results.insert(0, Ok(PushOutcome::Complete));
            p.results.insert(1, Err("offline".into()));
            draw(&mut p, &mut term);
            assert!(text(&term, other).contains(&format!("tools {}", icons.glyph(Icon::Error))));
            let tags = hit(&p, |h| matches!(h, Hit::Tags));
            assert_eq!(
                text(&term, tags),
                format!("{} Follow tags", icons.check(false))
            );
            click(&mut p, tags.right() - 1, tags.y);
            assert!(p.follow_tags);
            let send = hit(&p, |h| matches!(h, Hit::Send));
            assert!(
                matches!(click(&mut p, send.right() - 1, send.y), Action::Confirm {ref body, ..}
                if body.contains("app:") && !body.contains("tools:") && body.contains(icons.glyph(Icon::Right)))
            );
            let retry = hit(&p, |h| matches!(h, Hit::Retry));
            assert!(
                matches!(click(&mut p, retry.x, retry.y), Action::Confirm {ref body, ..}
                if body.contains("tools:") && !body.contains("app:"))
            );
        }
    }

    #[test]
    fn push_repository_sorting_keeps_the_checked_root_selected_and_honours_reseed() {
        let td = tempfile::tempdir().unwrap();
        make(td.path(), "app");
        make(td.path(), "tools");
        let mut c = Context::discover(td.path()).unwrap();
        c.repos[0].name = "z-app".into();
        c.repos[1].name = "a-tools".into();
        let mut p = PushPanel::default();
        p.refresh(&c).unwrap();
        p.finish_loading(&c);
        assert_eq!(p.entries.iter().map(|e| e.idx).collect::<Vec<_>>(), [1, 0]);
        assert_eq!(p.checked, BTreeSet::from([0]));
        assert_eq!(p.selected(), Some((0, None)));
        p.reseed = Some(vec![1]);
        p.refresh(&c).unwrap();
        p.finish_loading(&c);
        assert_eq!(p.checked, BTreeSet::from([1]));
        assert_eq!(p.selected(), Some((1, None)));
    }

    #[test]
    fn push_preview_reads_real_statuses_and_opens_exact_files_without_writes() {
        let td = tempfile::tempdir().unwrap();
        let repo = make(td.path(), "app");
        for path in ["src/a", "src/b", "dev"] {
            fs::create_dir_all(repo.join(path)).unwrap();
        }
        fs::write(repo.join("src/a/index.vue"), "first base\n").unwrap();
        fs::write(repo.join("src/b/index.vue"), "second base\n").unwrap();
        fs::write(repo.join("dev/old.conf"), "legacy config\n").unwrap();
        fs::write(repo.join("old.ts"), "export const renamed = true;\n").unwrap();
        g(&repo, &["add", "--all"]);
        g(&repo, &["commit", "-qm", "baseline tree"]);
        g(&repo, &["push", "-q", "origin", "main"]);
        fs::write(repo.join("src/a/index.vue"), "first updated\n").unwrap();
        fs::write(repo.join("src/b/index.vue"), "second updated\n").unwrap();
        fs::write(repo.join("dev/compose.yaml"), "services: {}\n").unwrap();
        fs::remove_file(repo.join("dev/old.conf")).unwrap();
        g(&repo, &["mv", "old.ts", "src/renamed.ts"]);
        g(&repo, &["add", "--all"]);
        g(&repo, &["commit", "-qm", "feat: preview all change kinds"]);
        let reviewed_commit = g(&repo, &["rev-parse", "HEAD"]);
        fs::write(repo.join("src/a/index.vue"), "first final\n").unwrap();
        g(&repo, &["commit", "-qam", "follow-up on the first file"]);

        // The read-only preview must use committed content, even with dirty index/worktree.
        fs::write(repo.join("a"), "already staged\n").unwrap();
        fs::write(repo.join("src/a/index.vue"), "staged revision\n").unwrap();
        g(&repo, &["add", "a", "src/a/index.vue"]);
        fs::write(repo.join("src/a/index.vue"), "unstaged after staging\n").unwrap();
        let snapshot = || {
            [
                g(&repo, &["rev-parse", "HEAD"]),
                g(&repo, &["status", "--porcelain=v1", "-z"]),
                g(&repo, &["ls-files", "--stage", "-z"]),
                g(&repo, &["diff", "--binary"]),
                g(&repo, &["diff", "--cached", "--binary"]),
            ]
        };
        let before = snapshot();
        let c = Context::discover(td.path()).unwrap();
        let mut p = PushPanel::default();
        p.refresh(&c).unwrap();
        p.finish_loading(&c);
        let statuses: BTreeMap<_, _> = p
            .preview
            .files
            .iter()
            .map(|f| (label(&f.path), f.status))
            .collect();
        assert_eq!(
            statuses,
            BTreeMap::from([
                ("dev/compose.yaml".into(), b'A'),
                ("dev/old.conf".into(), b'D'),
                ("src/a/index.vue".into(), b'M'),
                ("src/b/index.vue".into(), b'M'),
                ("src/renamed.ts".into(), b'R'),
            ])
        );
        let key = |p: &mut PushPanel, code| {
            p.event(
                &Event::Key(crossterm::event::KeyEvent::new(code, KeyModifiers::NONE)),
                &c,
            )
        };
        let select = |p: &mut PushPanel, path: &str| {
            if p.diff.is_some() {
                key(p, KeyCode::Esc);
            }
            let row = p
                .preview
                .tree
                .visible
                .iter()
                .position(|&n| match p.preview.tree.nodes[n].kind {
                    Kind::File(i) => p.preview.files[i].path == Path::new(path),
                    _ => false,
                })
                .unwrap();
            p.activate_file(row, false, &c);
            p.finish_loading(&c);
        };
        for (path, old, new) in [
            ("src/a/index.vue", "first base\n", "first final\n"),
            ("src/b/index.vue", "second base\n", "second updated\n"),
            ("dev/old.conf", "legacy config\n", ""),
            ("dev/compose.yaml", "", "services: {}\n"),
            (
                "src/renamed.ts",
                "export const renamed = true;\n",
                "export const renamed = true;\n",
            ),
        ] {
            select(&mut p, path);
            assert_eq!(compared_revisions(&mut p, &c), [old, new], "{path}");
        }

        select(&mut p, "src/b/index.vue");
        key(&mut p, KeyCode::Esc);
        assert!(!p.editing());
        key(&mut p, KeyCode::Char('o'));
        for group in Group::ALL {
            if p.grouping.enabled(group) {
                key(&mut p, KeyCode::Char(' '));
            }
            key(&mut p, KeyCode::Down);
        }
        key(&mut p, KeyCode::Esc);
        assert_eq!(p.preview.tree.visible.len(), 5);
        assert_eq!(
            p.preview.files[p.preview.tree.file_index().unwrap()].path,
            Path::new("src/b/index.vue")
        );

        p.pos = 1 + p.entries[0]
            .logs
            .iter()
            .position(|h| h.id == reviewed_commit)
            .unwrap();
        p.inspect(&c);
        p.finish_loading(&c);
        assert_eq!(p.preview.files.len(), 5);
        assert_eq!(p.preview.tree.visible.len(), 5);
        select(&mut p, "src/a/index.vue");
        assert_eq!(
            compared_revisions(&mut p, &c),
            ["first base\n", "first updated\n"]
        );
        assert_eq!(p.checked, BTreeSet::from([0]));

        p.pos = 0;
        p.targets.insert(
            0,
            Target {
                remote: "origin".into(),
                dst: "new-target".into(),
            },
        );
        p.refresh(&c).unwrap();
        p.finish_loading(&c);
        assert!(p.preview.files.iter().all(|file| {
            file.status
                == if file.path == Path::new("dev/old.conf") {
                    b'D'
                } else {
                    b'A'
                }
        }));
        assert_eq!(p.preview.tree.visible.len(), p.preview.files.len());
        select(&mut p, "src/a/index.vue");
        assert_eq!(compared_revisions(&mut p, &c), ["", "first final\n"]);
        let root = g(&repo, &["rev-list", "--max-parents=0", "HEAD"]);
        p.pos = 1 + p.entries[0].logs.iter().position(|h| h.id == root).unwrap();
        p.inspect(&c);
        p.finish_loading(&c);
        assert_eq!(p.preview.files.len(), 1);
        assert_eq!(p.preview.files[0].status, b'A');
        select(&mut p, "a");
        assert_eq!(compared_revisions(&mut p, &c), ["", "base"]);
        assert_eq!(snapshot(), before);
    }

    #[test]
    fn repository_preview_excludes_remote_only_changes_after_divergence() {
        let td = tempfile::tempdir().unwrap();
        let repo = make(td.path(), "app");
        let head = g(&repo, &["rev-parse", "HEAD"]);
        g(&repo, &["switch", "-qc", "remote-work", "origin/main"]);
        fs::write(repo.join("remote-only"), "remote content\n").unwrap();
        g(&repo, &["add", "remote-only"]);
        g(&repo, &["commit", "-qm", "remote addition"]);
        fs::write(repo.join("a"), "remote version\n").unwrap();
        g(&repo, &["commit", "-qam", "remote edit"]);
        g(&repo, &["update-ref", "refs/remotes/origin/main", "HEAD"]);
        g(&repo, &["switch", "-q", "main"]);
        fs::write(repo.join("a"), "staged\n").unwrap();
        g(&repo, &["add", "a"]);
        fs::write(repo.join("a"), "unstaged\n").unwrap();
        let snapshot = || {
            [
                g(&repo, &["show-ref"]),
                g(&repo, &["status", "--porcelain=v1", "-z"]),
                g(&repo, &["ls-files", "--stage", "-z"]),
                g(&repo, &["diff", "--binary"]),
                g(&repo, &["diff", "--cached", "--binary"]),
            ]
        };
        let before = snapshot();
        let c = Context::discover(td.path()).unwrap();
        let mut panel = PushPanel::default();
        panel.refresh(&c).unwrap();
        panel.finish_loading(&c);
        assert_eq!(panel.entries[0].n, 1);
        assert_eq!(panel.entries[0].logs[0].id, head);
        let tracking = panel.entries[0].tracking.clone();
        for pos in [0, 1, 0] {
            panel.pos = pos;
            panel.inspect(&c);
            panel.finish_loading(&c);
            assert_eq!(panel.preview.files.len(), 1);
            assert_eq!(panel.preview.files[0].path, Path::new("a"));
            let row = panel
                .preview
                .tree
                .visible
                .iter()
                .position(|&node| matches!(panel.preview.tree.nodes[node].kind, Kind::File(_)))
                .unwrap();
            panel.activate_file(row, false, &c);
            panel.finish_loading(&c);
            assert_eq!(compared_revisions(&mut panel, &c), ["base", "next"]);
            assert_eq!(panel.checked, BTreeSet::from([0]));
            assert_eq!(panel.entries[0].tracking, tracking);
        }
        assert_eq!(snapshot(), before);
    }

    #[test]
    fn repository_preview_unions_repeated_and_reverted_paths() {
        let td = tempfile::tempdir().unwrap();
        let repo = make(td.path(), "app");
        fs::write(repo.join("a"), "base").unwrap();
        fs::write(repo.join("temporary"), "intermediate content\n").unwrap();
        g(&repo, &["add", "--all"]);
        g(&repo, &["commit", "-qm", "revert a and add temporary"]);
        fs::remove_file(repo.join("temporary")).unwrap();
        fs::write(repo.join("final"), "final content\n").unwrap();
        g(&repo, &["add", "--all"]);
        g(&repo, &["commit", "-qm", "remove temporary and add final"]);
        let c = Context::discover(td.path()).unwrap();
        let mut panel = PushPanel::default();
        panel.refresh(&c).unwrap();
        panel.finish_loading(&c);
        assert_eq!(panel.entries[0].n, 3);
        let statuses: BTreeMap<_, _> = panel
            .preview
            .files
            .iter()
            .map(|file| (label(&file.path), file.status))
            .collect();
        assert_eq!(
            statuses,
            BTreeMap::from([
                ("a".into(), b'M'),
                ("final".into(), b'A'),
                ("temporary".into(), b'D'),
            ])
        );
        for (path, old, new) in [
            ("a", "next", "base"),
            ("temporary", "intermediate content\n", ""),
            ("final", "", "final content\n"),
        ] {
            let row = panel
                .preview
                .tree
                .visible
                .iter()
                .position(|&node| match panel.preview.tree.nodes[node].kind {
                    Kind::File(index) => panel.preview.files[index].path == Path::new(path),
                    _ => false,
                })
                .unwrap();
            panel.activate_file(row, false, &c);
            panel.finish_loading(&c);
            assert_eq!(compared_revisions(&mut panel, &c), [old, new], "{path}");
        }
    }

    #[test]
    fn repository_preview_includes_changes_older_than_the_visible_commit_limit() {
        let td = tempfile::tempdir().unwrap();
        let repo = make(td.path(), "app");
        let tree = g(&repo, &["rev-parse", "HEAD^{tree}"]);
        let mut head = g(&repo, &["rev-parse", "HEAD"]);
        for _ in 0..61 {
            head = g(
                &repo,
                &["commit-tree", &tree, "-p", &head, "-m", "empty follow-up"],
            );
        }
        g(&repo, &["update-ref", "refs/heads/main", &head]);
        fs::write(repo.join("newest"), "newest content\n").unwrap();
        g(&repo, &["add", "newest"]);
        g(&repo, &["commit", "-qm", "newest change"]);
        let c = Context::discover(td.path()).unwrap();
        let mut panel = PushPanel::default();
        panel.refresh(&c).unwrap();
        panel.finish_loading(&c);
        assert_eq!(panel.entries[0].n, 63);
        assert_eq!(panel.entries[0].logs.len(), 60);
        assert_eq!(
            panel
                .preview
                .files
                .iter()
                .map(|file| label(&file.path))
                .collect::<Vec<_>>(),
            ["a", "newest"]
        );
    }

    #[test]
    fn repository_preview_keeps_rename_history_and_cumulative_sources() {
        let td = tempfile::tempdir().unwrap();
        let repo = make(td.path(), "app");
        let base = "line one\nline two\nline three\n";
        let next = "line one\nline two\nline three\nline four\n";
        fs::write(repo.join("a"), base).unwrap();
        g(&repo, &["commit", "-qam", "rename baseline"]);
        g(&repo, &["push", "-q", "origin", "main"]);
        fs::write(repo.join("a"), next).unwrap();
        g(&repo, &["commit", "-qam", "edit before renaming"]);
        g(&repo, &["mv", "a", "middle"]);
        g(&repo, &["commit", "-qm", "first rename"]);
        g(&repo, &["mv", "middle", "last"]);
        g(&repo, &["commit", "-qm", "second rename"]);
        let c = Context::discover(td.path()).unwrap();
        let mut panel = PushPanel::default();
        panel.refresh(&c).unwrap();
        panel.finish_loading(&c);
        assert_eq!(
            panel
                .preview
                .files
                .iter()
                .map(|file| label(&file.path))
                .collect::<Vec<_>>(),
            ["a", "last", "middle"]
        );
        let last = panel
            .preview
            .files
            .iter()
            .find(|file| file.path == Path::new("last"))
            .unwrap();
        assert!(last.commit.is_none());
        assert_eq!(last.previous.as_deref(), Some(Path::new("a")));
        for (path, old, new) in [
            ("a", base, next),
            ("middle", next, next),
            ("last", base, next),
        ] {
            let row = panel
                .preview
                .tree
                .visible
                .iter()
                .position(|&node| match panel.preview.tree.nodes[node].kind {
                    Kind::File(index) => panel.preview.files[index].path == Path::new(path),
                    _ => false,
                })
                .unwrap();
            panel.activate_file(row, false, &c);
            panel.finish_loading(&c);
            assert_eq!(compared_revisions(&mut panel, &c), [old, new], "{path}");
        }
    }

    fn compared_revisions(p: &mut PushPanel, c: &Context) -> [String; 2] {
        assert!(matches!(
            p.diff.as_ref().and_then(|d| d.content.as_ref()),
            Some(preview::FileDiff::Split(_))
        ));
        let event = |code| Event::Key(crossterm::event::KeyEvent::new(code, KeyModifiers::NONE));
        let copy = |p: &mut PushPanel| match p.event(&event(KeyCode::Char('y')), c) {
            Action::Copy { text, .. } => text,
            _ => panic!("expected revision contents"),
        };
        let after = copy(p);
        p.event(&event(KeyCode::Tab), c);
        let before = copy(p);
        p.event(&event(KeyCode::Tab), c);
        [before, after]
    }

    #[test]
    fn merge_commit_preview_compares_against_its_first_parent() {
        let td = tempfile::tempdir().unwrap();
        let repo = make(td.path(), "app");
        g(&repo, &["switch", "-qc", "topic"]);
        fs::write(repo.join("side.ts"), "side branch content\n").unwrap();
        g(&repo, &["add", "side.ts"]);
        g(&repo, &["commit", "-qm", "side change"]);
        g(&repo, &["switch", "-q", "main"]);
        fs::write(repo.join("main.ts"), "main branch content\n").unwrap();
        g(&repo, &["add", "main.ts"]);
        g(&repo, &["commit", "-qm", "main change"]);
        g(&repo, &["merge", "--no-ff", "-qm", "merge topic", "topic"]);
        let c = Context::discover(td.path()).unwrap();
        let mut p = PushPanel::default();
        p.refresh(&c).unwrap();
        p.finish_loading(&c);
        assert_eq!(
            p.preview
                .files
                .iter()
                .map(|file| label(&file.path))
                .collect::<Vec<_>>(),
            ["a", "main.ts", "side.ts"]
        );
        p.pos = 1;
        p.inspect(&c);
        p.finish_loading(&c);
        assert_eq!(p.preview.files.len(), 1);
        assert_eq!(p.preview.files[0].path, PathBuf::from("side.ts"));
        p.preview.tree.cursor = p
            .preview
            .tree
            .visible
            .iter()
            .position(|&n| matches!(p.preview.tree.nodes[n].kind, Kind::File(0)))
            .unwrap();
        p.open_file(&c);
        p.finish_loading(&c);
        assert_eq!(
            compared_revisions(&mut p, &c),
            ["", "side branch content\n"]
        );
    }

    #[test]
    fn bare_push_exact_tip_and_stale_plans() {
        let td = tempfile::tempdir().unwrap();
        let p = make(td.path(), "app");
        let c = Context::discover(td.path()).unwrap();
        let x = entry(0, &c.repos[0], None);
        assert_eq!(x.n, 1);
        let j = job(&c.repos[0], &x).unwrap();
        push_one(&j, false, false).unwrap();
        assert_eq!(
            g(
                &td.path().join("app.git"),
                &["rev-parse", "refs/heads/main"]
            ),
            j.head
        );
        assert!(push_one(&j, false, false).is_err());
        g(&p, &["commit", "--allow-empty", "-qm", "later"]);
        assert!(push_one(&j, false, false).is_err());
    }

    fn assert_synthetic_parent_cannot_publish(install_parent: impl FnOnce(&Path, &str, &str)) {
        let td = tempfile::tempdir().unwrap();
        let p = make(td.path(), "app");
        let bare = td.path().join("app.git");
        let remote = g(&bare, &["rev-parse", "refs/heads/main"]);
        let head = g(
            &p,
            &["commit-tree", "HEAD^{tree}", "-m", "independent root"],
        );
        g(&p, &["update-ref", "refs/heads/main", &head]);
        let ancestry = ["merge-base", "--is-ancestor", &remote, &head];
        assert!(git(&p, &ancestry).is_err());
        install_parent(&p, &head, &remote);
        assert!(
            git(&p, &ancestry).is_ok(),
            "ordinary Git sees a fast-forward"
        );

        let c = Context::discover(&p).unwrap();
        let r = &c.repos[0];
        let j = job(r, &entry(0, r, None)).unwrap();
        let result = push_one(&j, false, false);
        assert_eq!(
            g(&bare, &["rev-parse", "refs/heads/main"]),
            remote,
            "a synthetic parent must never authorize a remote rewrite: {result:?}"
        );
        assert!(result.unwrap_err().to_string().contains("non-fast-forward"));
    }

    #[test]
    fn replace_ref_cannot_turn_a_non_fast_forward_into_a_push() {
        assert_synthetic_parent_cannot_publish(|repo, tip, parent| {
            g(repo, &["replace", "--graft", tip, parent]);
        });
    }

    #[test]
    fn legacy_graft_cannot_turn_a_non_fast_forward_into_a_push() {
        assert_synthetic_parent_cannot_publish(|repo, tip, parent| {
            let info = repo.join(".git/info");
            fs::create_dir_all(&info).unwrap();
            fs::write(info.join("grafts"), format!("{tip} {parent}\n")).unwrap();
        });
    }

    #[test]
    fn completed_push_notifications_use_structured_outcomes() {
        let directory = tempfile::tempdir().unwrap();
        make(directory.path(), "app");
        make(directory.path(), "tools");
        let context = Context::discover(directory.path()).unwrap();
        for (outcomes, expected) in [
            (
                vec![Ok(PushOutcome::Complete), Ok(PushOutcome::Complete)],
                NoticeLevel::Success,
            ),
            (
                vec![Ok(PushOutcome::Complete), Err("remote rejected".into())],
                NoticeLevel::Warning,
            ),
            (
                vec![Err("remote rejected".into()), Err("not executed".into())],
                NoticeLevel::Error,
            ),
            (
                vec![Ok(PushOutcome::UpstreamNotSet("config locked".into()))],
                NoticeLevel::Warning,
            ),
        ] {
            let (sender, receiver) = mpsc::channel();
            let mut panel = PushPanel {
                inflight: Some(receiver),
                busy: "Pushing…".into(),
                results: outcomes.into_iter().enumerate().collect(),
                ..Default::default()
            };
            sender.send(Report::Done).unwrap();
            let (level, _) = panel.poll(&context).unwrap();
            assert_eq!(level, expected);
            assert!(panel.poll(&context).is_none());
            panel.finish_loading(&context);
        }
    }

    #[test]
    fn rejected_push_notification_includes_the_server_policy_reason() {
        let directory = tempfile::tempdir().unwrap();
        make(directory.path(), "app");
        let context = Context::discover(directory.path()).unwrap();
        let (sender, receiver) = mpsc::channel();
        let mut panel = PushPanel {
            inflight: Some(receiver),
            busy: "Pushing…".into(),
            results: [(
                0,
                Err("Push failed: remote: [01-commit-message-check] executed\nremote: GL-HOOK-ERR: Commit blocked: missing issueKey.\nTo example.invalid"
                    .into()),
            )]
            .into_iter()
            .collect(),
            ..Default::default()
        };
        sender.send(Report::Done).unwrap();
        let (_, message) = panel.poll(&context).unwrap();
        assert_eq!(
            message.lines().next(),
            Some("1 failed · Push failed: Commit blocked: missing issueKey.")
        );
        assert!(message.contains("[01-commit-message-check]"), "{message}");
        assert!(message.contains("To example.invalid"), "{message}");
        panel.finish_loading(&context);
    }

    #[test]
    fn push_failure_summary_prioritizes_errors_over_multiple_hook_banners() {
        let error = "Updated successfully; push failed: remote: [first-check] executed\nremote: [second-check] executed\nremote:\nremote: error: Commit blocked: missing issueKey.\nerror: failed to push some refs";
        assert_eq!(
            failure_summary(error),
            "Updated successfully; push failed: Commit blocked: missing issueKey."
        );
        assert_eq!(failure_summary("Permission denied"), "Permission denied");
        let long = format!("Push failed:\nremote: GL-HOOK-ERR: {}", "界".repeat(250));
        let summary = failure_summary(&long);
        assert_eq!(summary.chars().count(), 220);
        assert!(summary.ends_with('…'));
    }

    #[test]
    fn two_remotes_partial_failure_and_retry_scope() {
        use std::time::{Duration, Instant};
        let t = tempfile::tempdir().unwrap();
        make(t.path(), "a");
        make(t.path(), "b");
        let mut c = Context::discover(t.path()).unwrap();
        assert_eq!(c.repos.len(), 2);
        let mut p = PushPanel {
            reseed: Some(vec![0, 1]),
            ..Default::default()
        };
        p.refresh(&c).unwrap();
        p.finish_loading(&c);
        assert!(matches!(p.review(&c, false), Action::Confirm { .. }));
        g(
            &t.path().join("b.git"),
            &[
                "update-ref",
                "refs/heads/main",
                &g(&c.repos[1].path, &["rev-parse", "HEAD~1"]),
            ],
        );
        let reviewed = p.review[1].head.clone();
        g(
            &c.repos[1].path,
            &["commit", "--allow-empty", "-qm", "changed during preview"],
        );
        p.start(&c, false);
        let wait = |p: &mut PushPanel, c: &Context| {
            let now = Instant::now();
            while p.poll(c).is_none() {
                assert!(now.elapsed() < Duration::from_secs(20));
                std::thread::sleep(Duration::from_millis(10));
            }
        };
        wait(&mut p, &c);
        assert!(p.results[&0].is_ok());
        assert!(p.results[&1].is_err());
        assert_ne!(g(&t.path().join("b.git"), &["rev-parse", "main"]), reviewed);
        c.scope.remove(&1);
        p.refresh(&c).unwrap();
        p.finish_loading(&c);
        assert!(matches!(
            p.review(&c, true),
            Action::Notice(NoticeLevel::Warning, _)
        ));
        c.scope.insert(1);
        p.refresh(&c).unwrap();
        p.finish_loading(&c);
        assert!(matches!(p.review(&c, true), Action::Confirm { .. }));
        assert_eq!(p.review.len(), 1);
        assert_eq!(p.review[0].idx, 1);
        p.start(&c, false);
        wait(&mut p, &c);
        assert_eq!(
            g(&t.path().join("b.git"), &["rev-parse", "main"]),
            g(&c.repos[1].path, &["rev-parse", "HEAD"])
        );
    }
}
