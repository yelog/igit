//! Background conflict-list refreshes and opened-document source tracking.
use super::{
    list::{read_repository, RepoSnapshot},
    MergePanel,
};
use crate::core::{
    loading::{read_repositories, Load, Scope, Version},
    Context, Repo,
};
use anyhow::{anyhow, Result};

struct Loaded {
    snapshots: Vec<RepoSnapshot>,
}

impl Loaded {
    fn read(jobs: Vec<(usize, Repo)>, version: Version) -> Option<Self> {
        let snapshots =
            read_repositories(&jobs, &version, |(root, repo)| read_repository(*root, repo));
        (!version.cancelled()).then_some(Self { snapshots })
    }
}

#[derive(Default)]
pub(super) struct Loading {
    enabled: bool,
    scope: Option<Scope>,
    task: Load<Option<Loaded>>,
}

impl Loading {
    pub(super) fn enabled(&self) -> bool {
        self.enabled
    }
}

impl MergePanel {
    /// Enable after the initial synchronous refresh. Direct Panel callers retain
    /// synchronous refresh semantics by default.
    pub(crate) fn enable_background_loading(&mut self) {
        self.loading.enabled = true;
    }

    pub(crate) fn reads_pending(&self) -> bool {
        self.loading.enabled && self.loading.task.pending()
    }

    pub(crate) fn refresh_allowed(&self) -> bool {
        self.edit.is_none()
            && self.view.overlay.is_none()
            && self.key_sequence().is_none()
            && !self.view.dragging()
            && !self.panes.dragging()
    }

    pub(super) fn note_loaded_scope(&mut self, c: &Context) {
        self.loading.scope = Some(Scope::new(c));
    }

    fn clear_outdated_scope(&mut self, c: &Context) -> bool {
        let changed = self
            .loading
            .scope
            .as_ref()
            .is_some_and(|scope| !scope.matches(c));
        if !changed {
            self.loading.scope.get_or_insert_with(|| Scope::new(c));
            return false;
        }
        self.loading.task.cancel();
        self.loading.scope = Some(Scope::new(c));
        self.list.clear();
        if self.doc.is_some() {
            self.doc_stale = true;
        }
        true
    }

    pub(super) fn queue_refresh(&mut self, c: &Context) {
        self.clear_outdated_scope(c);
        self.loading.task.request();
    }

    pub(crate) fn poll_loading(&mut self, c: &Context) -> Result<bool> {
        if !self.loading.enabled {
            return Ok(false);
        }
        let mut changed = self.clear_outdated_scope(c);
        if changed {
            self.loading.task.request();
        }
        if self.loading.task.needs_start() {
            let jobs = c
                .scope
                .iter()
                .map(|&root| (root, c.repos[root].clone()))
                .collect();
            self.loading.task.start("igit-merge-read", move |version| {
                Loaded::read(jobs, version)
            })?;
        }
        if !self.refresh_allowed() {
            return Ok(changed);
        }
        if let Some(result) = self.loading.task.take_result() {
            match result {
                Ok(Some(loaded)) => {
                    let errors = self.list.install(loaded.snapshots);
                    self.update_document_staleness(c);
                    changed = true;
                    if !errors.is_empty() {
                        return Err(anyhow!("{}", errors.join("; ")));
                    }
                }
                Ok(None) => {}
                Err(error) => {
                    return Err(error);
                }
            }
        }
        Ok(changed)
    }

    pub(super) fn update_document_staleness(&mut self, c: &Context) {
        let Some(doc) = self.doc.as_ref() else {
            self.doc_stale = false;
            return;
        };
        if !c.scope.contains(&doc.file.root) {
            self.doc_stale = true;
            return;
        }
        if self.list.repo_failed(doc.file.root) {
            return;
        }
        let stale = !self.list.source_matches(&doc.file, &doc.op);
        self.doc_stale |= stale;
    }

    #[cfg(test)]
    pub(crate) fn finish_loading(&mut self, c: &Context) -> Option<String> {
        use std::time::{Duration, Instant};
        let deadline = Instant::now() + Duration::from_secs(20);
        let mut error = None;
        loop {
            match self.poll_loading(c) {
                Ok(_) => {}
                Err(failure) => error = Some(format!("{failure:#}")),
            }
            if !self.reads_pending() {
                return error;
            }
            assert!(Instant::now() < deadline, "Merge reads did not finish");
            std::thread::sleep(Duration::from_millis(1));
        }
    }
}
