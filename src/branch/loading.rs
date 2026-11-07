//! Branch snapshots and the initial row projection are built away from the terminal.
use super::*;
use crate::core::{
    loading::{read_repositories, Load, Scope, Version},
    Repo,
};

struct Loaded {
    repos: Vec<Repository>,
    tree: Tree,
}

impl Loaded {
    fn read(
        jobs: Vec<(usize, Repo)>,
        root: Option<usize>,
        tree: Tree,
        version: Version,
    ) -> Option<Self> {
        let repos = read_repositories(&jobs, &version, |(root, repo)| read_repo(*root, repo));
        if version.cancelled() {
            return None;
        }
        Some(Self::build(repos, root, tree))
    }

    fn build(repos: Vec<Repository>, root: Option<usize>, mut tree: Tree) -> Self {
        tree.rebuild(repos.iter().find(|repo| Some(repo.root) == root));
        Self { repos, tree }
    }
}

fn read_repo(root: usize, repo: &Repo) -> Repository {
    let (state, default_branch) = match Snapshot::read_with_default(repo) {
        Ok((snapshot, default_branch)) => (Ok(snapshot), default_branch),
        Err(error) => (Err(format!("{error:#}")), None),
    };
    Repository {
        root,
        name: repo.name.clone(),
        path: repo.path.clone(),
        submodule: repo.submodule,
        upstream_widths: state.as_ref().ok().and_then(rows::measure_upstream),
        default_branch,
        state,
    }
}

#[derive(Default)]
pub(super) struct Loading {
    scope: Option<Scope>,
    task: Load<Option<Loaded>>,
    error: Option<String>,
}

impl BranchPanel {
    pub(crate) fn enter(&mut self, c: &Context) {
        if !self.loading.task.loading()
            || self
                .loading
                .scope
                .as_ref()
                .is_some_and(|scope| !scope.matches(c))
        {
            self.request_load(c);
        }
    }

    pub(crate) fn invalidate_reads(&mut self, c: &Context) {
        if !self.clear_outdated_scope(c) {
            self.loading.task.cancel();
        }
    }

    fn clear_outdated_scope(&mut self, c: &Context) -> bool {
        if self
            .loading
            .scope
            .as_ref()
            .is_none_or(|scope| scope.matches(c))
        {
            return false;
        }
        self.loading.scope = None;
        self.loading.task.cancel();
        self.loading.error = None;
        self.repos.clear();
        self.tree.clear_rows();
        self.repo_viewport = Viewport::default();
        self.viewport = Viewport::default();
        if c.scope.len() <= 1 {
            self.focus = Focus::Branches;
        }
        self.panes.begin();
        self.panes.cancel_drag();
        self.hits.clear();
        true
    }

    pub(super) fn request_load(&mut self, c: &Context) {
        self.clear_outdated_scope(c);
        self.cancel_key_sequence();
        self.repo_root = self
            .repo_root
            .filter(|root| c.scope.contains(root))
            .or_else(|| c.scope.first().copied());
        if !c.scope.is_empty() {
            self.loading.scope.get_or_insert_with(|| Scope::new(c));
            self.loading.task.request();
            self.loading.error = None;
        }
    }

    pub(crate) fn reads_pending(&self) -> bool {
        self.loading.task.pending()
    }

    pub(super) fn read_status(&self) -> Option<&str> {
        if self.loading.task.loading() {
            Some(if self.repos.is_empty() {
                "Loading branches…"
            } else {
                "Refreshing branches…"
            })
        } else {
            self.loading.error.as_deref()
        }
    }

    pub(crate) fn poll_loading(&mut self, c: &Context) -> Result<bool> {
        let mut changed = self.clear_outdated_scope(c);
        if changed {
            self.request_load(c);
        }
        if let Some(result) = self.loading.task.take_result() {
            match result {
                Ok(Some(loaded)) => {
                    self.repos = loaded.repos;
                    let repo = self.repository_index().map(|index| &self.repos[index]);
                    self.tree.install(loaded.tree, repo);
                    self.keep_branch_visible();
                    self.hits.clear();
                    changed = true;
                }
                Ok(None) => (),
                Err(error) => {
                    self.loading.error = Some(error.to_string());
                    return Err(error);
                }
            }
        }
        if self.loading.task.needs_start() {
            let jobs = c
                .scope
                .iter()
                .map(|&root| (root, c.repos[root].clone()))
                .collect();
            let tree = self.tree.for_refresh();
            let root = self.repo_root;
            if let Err(error) = self.loading.task.start("igit-branch-read", move |version| {
                Loaded::read(jobs, root, tree, version)
            }) {
                self.loading.error = Some("Unable to load branches; press F5 to retry".into());
                return Err(error);
            }
        }
        Ok(changed)
    }

    #[cfg(test)]
    pub(crate) fn finish_loading(&mut self, c: &Context) {
        use std::time::{Duration, Instant};
        let deadline = Instant::now() + Duration::from_secs(20);
        while self.reads_pending() {
            self.poll_loading(c).unwrap();
            assert!(Instant::now() < deadline, "Branch reads did not finish");
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    #[cfg(test)]
    pub(crate) fn hold_loading(&mut self, c: &Context) -> impl FnOnce() + 'static {
        let repos = c
            .scope
            .iter()
            .map(|&root| read_repo(root, &c.repos[root]))
            .collect();
        let loaded = Loaded::build(repos, self.repo_root, self.tree.for_refresh());
        let send = self.loading.task.hold();
        move || assert!(send.send(Some(loaded)).is_ok(), "Branch read was abandoned")
    }
}
