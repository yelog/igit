//! History, optional presentation reads, and graph preparation share one latest-request worker.
use super::*;
use crate::core::loading::{Load, Scope};
use std::collections::BTreeSet;

type Selected = Option<(usize, String)>;

#[derive(Debug)]
pub(super) struct HistoryUnavailable;

impl std::fmt::Display for HistoryUnavailable {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("History is not current; wait for loading or refresh to retry")
    }
}

impl std::error::Error for HistoryUnavailable {}

struct Request {
    tab: usize,
    comparison: Option<compare::Range>,
    file: Option<file_history::Target>,
    author: String,
    date: Option<DateFilter>,
    paths: model::PathFilter,
    branch: Option<String>,
    limit: usize,
    signatures: bool,
    cherry: bool,
}

impl Request {
    fn new(p: &LogPanel) -> Self {
        Self {
            tab: p.view_index,
            comparison: p.filter.comparison.clone(),
            file: p.filter.file_history.clone(),
            author: p.filter.author.clone(),
            date: p.filter.date.clone(),
            paths: p.filter.paths.clone(),
            branch: p.filter.branch.as_ref().map(|b| b.text(false)),
            limit: p.limit,
            signatures: p.options.shows(display::Column::Signature),
            cherry: p.options.not_cherry_picked,
        }
    }

    fn matches(&self, p: &LogPanel) -> bool {
        self.tab == p.view_index
            && self.comparison == p.filter.comparison
            && self.file == p.filter.file_history
            && self.author == p.filter.author
            && self.date == p.filter.date
            && self.paths == p.filter.paths
            && self.branch == p.filter.branch.as_ref().map(|b| b.text(false))
            && self.limit == p.limit
            && self.signatures == p.options.shows(display::Column::Signature)
            && self.cherry == p.options.not_cherry_picked
    }
}

// These filters project a loaded page and never need another Git history query.
#[derive(Clone, PartialEq, Eq)]
pub(super) struct Projection {
    query: String,
    roots: Option<BTreeSet<usize>>,
    regex: bool,
    case_sensitive: bool,
}

impl Projection {
    pub(super) fn new(p: &LogPanel) -> Self {
        Self {
            query: p.filter.query.clone(),
            roots: p.filter.roots.clone(),
            regex: p.regex,
            case_sensitive: p.case_sensitive,
        }
    }

    pub(super) fn build(&self, hist: &History) -> Result<(Vec<usize>, Graph)> {
        let matcher = filters::matcher(&self.query, self.regex, self.case_sensitive)?;
        let visible: Vec<_> = hist
            .revs
            .iter()
            .enumerate()
            .filter(|(_, r)| {
                self.roots
                    .as_ref()
                    .is_none_or(|roots| roots.contains(&r.root))
                    && matcher.as_ref().is_none_or(|m| {
                        [&r.oid, &r.subject, &r.refs]
                            .into_iter()
                            .any(|v| m.is_match(v))
                    })
            })
            .map(|(i, _)| i)
            .collect();
        let graph = Graph::using_layout(&hist.revs, &visible, &hist.graph_layout);
        Ok((visible, graph))
    }
}

struct Loaded {
    hist: History,
    projection: Projection,
    visible: Vec<usize>,
    graph: Graph,
}

impl Loaded {
    fn build(hist: History, projection: Projection) -> Result<Self> {
        let (visible, graph) = projection.build(&hist)?;
        Ok(Self {
            hist,
            projection,
            visible,
            graph,
        })
    }
}

struct Read {
    context: Context,
    filter: Filter,
    limit: usize,
    signatures: bool,
    cherry: bool,
    projection: Projection,
    project: Option<History>,
}

impl Read {
    fn run(self, cancelled: impl Fn() -> bool) -> Option<Result<Loaded>> {
        let hist = match self.project {
            Some(hist) => hist,
            None => {
                let mut hist =
                    model::load_with_cancel(&self.context, &self.filter, self.limit, &cancelled);
                if cancelled() {
                    return None;
                }
                hist.read_presentation(&self.context, &self.filter, self.signatures, self.cherry);
                hist
            }
        };
        if cancelled() {
            return None;
        }
        Some(Loaded::build(hist, self.projection))
    }
}

#[derive(Default)]
pub(super) struct Loading {
    enabled: bool,
    scope: Option<Scope>,
    request: Option<Request>,
    task: Load<Option<Result<Loaded>>>,
    // A page whose text/Roots projection changed during a read only needs graph work.
    project: Option<History>,
    pub(super) selected: Selected,
    stale: bool,
    loading_more: bool,
    error: Option<String>,
}

impl Loading {
    pub(super) fn stale(&self) -> bool {
        self.enabled && self.stale
    }
}

impl LogPanel {
    /// Enable only after the initial synchronous refresh has run on the startup worker.
    /// Direct Panel callers retain synchronous refresh semantics by default.
    pub(crate) fn enable_background_loading(&mut self) {
        self.history_load.enabled = true;
    }

    pub(crate) fn history_pending(&self) -> bool {
        self.history_load.enabled && self.history_load.task.pending()
    }

    pub(super) fn suspend_history(&mut self) {
        self.history_load.task.cancel();
        self.history_load.project = None;
        self.history_load.stale = true;
        self.history_load.loading_more = false;
        self.history_load.error = None;
    }

    pub(super) fn drain_suspended_history(&mut self) {
        let _ = self.history_load.task.take_result();
    }

    pub(super) fn history_selection(&self) -> Selected {
        if self.history_load.stale() {
            self.history_load.selected.clone()
        } else {
            self.rev().map(|r| (r.root, r.oid.clone()))
        }
    }

    pub(super) fn history_status(&self) -> Option<&str> {
        if self.history_load.stale() {
            if let Some(error) = self.history_load.error.as_deref() {
                Some(error)
            } else if self.hist.revs.is_empty() {
                Some("Loading history…")
            } else if self.history_load.loading_more {
                Some("Loading older commits… · cached commits")
            } else {
                Some("Refreshing history… · cached commits")
            }
        } else {
            None
        }
    }

    pub(super) fn require_current_history(&self, c: &Context) -> Result<()> {
        anyhow::ensure!(
            !self.history_load.enabled
                || !self.history_load.stale
                    && self
                        .history_load
                        .scope
                        .as_ref()
                        .is_some_and(|s| s.matches(c))
                    && self
                        .history_load
                        .request
                        .as_ref()
                        .is_some_and(|r| r.matches(self)),
            HistoryUnavailable
        );
        Ok(())
    }

    pub(super) fn reload_history(&mut self, c: &Context, selected: Selected) -> Result<()> {
        self.reload_history_with_state(c, selected, false)
    }

    fn reload_history_with_state(
        &mut self,
        c: &Context,
        selected: Selected,
        loading_more: bool,
    ) -> Result<()> {
        if self.history_load.enabled {
            self.queue_history(c, selected, loading_more);
            return Ok(());
        }
        self.hist = model::load(c, &self.filter, self.limit);
        self.read_presentation(c);
        self.history_load.scope = Some(Scope::new(c));
        self.history_load.request = Some(Request::new(self));
        self.history_load.loading_more = false;
        self.rebuild_refs(c);
        self.rebuild_visible(selected)
    }

    fn queue_history(&mut self, c: &Context, mut selected: Selected, loading_more: bool) {
        if self
            .history_load
            .scope
            .as_ref()
            .is_some_and(|s| !s.matches(c))
        {
            // Root indices belong to one discovery/scope. Never render them against new paths.
            self.hist = History::default();
            self.graph = Graph::default();
            self.visible.clear();
            self.refs = refs::RefTree::default();
            self.cursor = 0;
            self.graph_top = 0;
            self.graph_origin = 0;
            selected = None;
        }
        self.history_load.scope = Some(Scope::new(c));
        self.history_load.request = Some(Request::new(self));
        self.history_load.selected = selected;
        self.history_load.stale = true;
        self.history_load.loading_more = loading_more;
        self.history_load.error = None;
        self.history_load.project = None;
        self.history_load.task.request();
        self.commit_dialog = None;
        self.commit_review = None;
        self.cancel_key_sequence();
        self.last = None;
        self.hits.clear();
        self.refresh_history_preview();
    }

    fn has_more_commits(&self) -> bool {
        self.hist
            .more
            .iter()
            .any(|&root| self.filter.includes_root(root))
    }

    pub(super) fn load_more(&mut self, c: &Context) -> Result<()> {
        if !self.has_more_commits() || self.history_load.stale() || self.history_load.task.pending()
        {
            return Ok(());
        }
        let selected = self.history_selection();
        self.limit = self.limit.saturating_mul(2);
        self.reload_history_with_state(c, selected, true)
    }

    /// Load the next history window when the commit cursor approaches the end of
    /// the current page. Git history has no cheap stable cursor, so grow the
    /// window geometrically and keep all Git work on the existing worker.
    pub(crate) fn poll_more(&mut self, c: &Context) -> Result<bool> {
        if self.graph_height == 0
            || self.visible.is_empty()
            || self.history_load.stale()
            || self.history_load.task.pending()
            || !self.refresh_allowed()
            || !self.has_more_commits()
        {
            return Ok(false);
        }
        let last = self.visible.len() - 1;
        let near_end = last.saturating_sub(self.cursor) <= LOAD_MORE_DISTANCE;
        // Avoid auto-loading a short filtered projection until the user actually
        // navigates to its last row.
        let can_scroll_to_end = self.cursor == last || self.visible.len() > self.graph_height;
        if !near_end || !can_scroll_to_end {
            return Ok(false);
        }
        self.load_more(c)?;
        Ok(true)
    }

    pub(super) fn check_history_context(&mut self, c: &Context) -> bool {
        if self.history_load.enabled
            && (self
                .history_load
                .scope
                .as_ref()
                .is_some_and(|s| !s.matches(c))
                || self
                    .history_load
                    .request
                    .as_ref()
                    .is_some_and(|r| !r.matches(self)))
        {
            self.queue_history(c, self.history_selection(), false);
            true
        } else {
            false
        }
    }

    pub(super) fn refresh_history_preview(&mut self) {
        if self.history_load.stale() {
            // Invalidate the old preview without scheduling Git for a cached row from another
            // tab/filter. Navigation stays available until current history is installed.
            let visible = std::mem::take(&mut self.visible);
            self.request_preview(Duration::ZERO);
            self.visible = visible;
        } else {
            self.request_preview(Duration::ZERO);
        }
    }

    pub(crate) fn poll_history(&mut self, c: &Context) -> Result<bool> {
        if !self.history_load.enabled {
            return Ok(false);
        }
        let changed = self.take_history(c)?;
        if self.history_load.task.needs_start() {
            let read = self.history_read(c);
            if let Err(error) = self
                .history_load
                .task
                .start("igit-log-read", move |version| {
                    read.run(|| version.cancelled())
                })
            {
                self.history_load.error = Some(format!("Unable to load history: {error}"));
                return Err(error);
            }
        }
        Ok(changed)
    }

    fn take_history(&mut self, c: &Context) -> Result<bool> {
        let mut changed = self.check_history_context(c);
        if let Some(result) = self.history_load.task.take_result() {
            match result.and_then(|result| result.transpose()) {
                Ok(Some(loaded)) => {
                    if loaded.projection != Projection::new(self) {
                        self.history_load.project = Some(loaded.hist);
                        self.history_load.task.request();
                    } else {
                        self.hist = loaded.hist;
                        self.visible = loaded.visible;
                        self.graph = loaded.graph;
                        self.history_load.stale = false;
                        self.history_load.loading_more = false;
                        self.history_load.error = None;
                        let selected = self.history_load.selected.take();
                        self.restore_history_cursor(selected);
                        self.rebuild_refs(c);
                        self.refresh_history_preview();
                        self.hits.clear();
                        self.last = None;
                        changed = true;
                    }
                }
                Ok(None) => (),
                Err(error) => {
                    self.history_load.error = Some(format!("Could not load history: {error:#}"));
                    return Err(error);
                }
            }
        }
        Ok(changed)
    }

    fn history_read(&mut self, c: &Context) -> Read {
        Read {
            context: c.clone(),
            filter: self.filter.clone(),
            limit: self.limit,
            signatures: self.options.shows(display::Column::Signature),
            cherry: self.options.not_cherry_picked,
            projection: Projection::new(self),
            project: self.history_load.project.take(),
        }
    }
}

#[cfg(test)]
mod tests;
