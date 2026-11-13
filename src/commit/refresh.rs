use super::*;
use std::{path::PathBuf, sync::mpsc, thread};
mod watch;

#[cfg(test)]
mod tests;

const FALLBACK: Duration = Duration::from_secs(30);
type Status = Result<Vec<Change>, String>;

#[derive(Default)]
pub(super) struct Refresh {
    generation: u64,
    pending: Option<Pending>,
    deferred: BTreeSet<usize>,
    next: Option<Instant>,
    error: String,
    errors: BTreeMap<usize, String>,
    watch: Option<watch::Watch>,
    requested: bool,
    expedite: bool,
}

struct Pending {
    generation: u64,
    roots: BTreeSet<usize>,
    receiver: mpsc::Receiver<Snapshot>,
}

struct Snapshot {
    statuses: BTreeMap<usize, Status>,
    branches: BTreeMap<usize, String>,
    preview: Option<(String, Change, preview::Loaded)>,
}

fn statuses(c: &Context, ignored: bool, roots: &BTreeSet<usize>) -> BTreeMap<usize, Status> {
    roots
        .iter()
        .map(|&root| {
            let repo = &c.repos[root];
            let status = backend::status(&repo.path)
                .and_then(|mut changes| {
                    if ignored {
                        changes.extend(backend::ignored(&repo.path)?);
                    }
                    Ok(changes)
                })
                .map_err(|e| format!("{}: {e}", repo.name));
            (root, status)
        })
        .collect()
}

impl Snapshot {
    fn read(
        mut c: Context,
        ignored: bool,
        target: Option<(String, usize, PathBuf)>,
        roots: BTreeSet<usize>,
    ) -> Self {
        let statuses = statuses(&c, ignored, &roots);
        let preview = target.and_then(|(key, root, path)| {
            let change = statuses
                .get(&root)?
                .as_ref()
                .ok()?
                .iter()
                .find(|f| f.path == path)?;
            let loaded = preview::Loaded::read(&c.repos[root].path, change);
            Some((key, change.clone(), loaded))
        });
        let branches = roots
            .into_iter()
            .filter(|root| statuses[root].is_ok())
            .map(|root| {
                let repo = &mut c.repos[root];
                repo.refresh_branch();
                (root, repo.branch.clone())
            })
            .collect();
        Self {
            statuses,
            branches,
            preview,
        }
    }
}

impl CommitPanel {
    /// Request a fresh scan without starting a second worker.
    pub(crate) fn request_refresh(&mut self) {
        self.invalidate_ai_check();
        self.refresh.generation = self.refresh.generation.wrapping_add(1);
        self.refresh.deferred.clear();
        self.refresh.requested = true;
    }

    /// Tab/focus changes release queued events, without rescanning unchanged roots.
    pub(crate) fn resume_refresh(&mut self) {
        self.refresh.expedite = true;
    }

    pub(crate) fn refresh_pending(&self) -> bool {
        self.refresh.requested || self.refresh.pending.is_some()
    }

    pub(super) fn refresh_files(&mut self, c: &Context) -> Result<()> {
        self.request_refresh();
        // Startup calls this on its loader thread, before the first status read:
        // edits during that read are already being watched and remain queued.
        if self
            .refresh
            .watch
            .as_ref()
            .is_none_or(|watch| !watch.matches(c))
        {
            self.refresh.watch = Some(watch::Watch::new(c));
        }
        let roots = (0..c.repos.len()).collect();
        self.refresh
            .watch
            .as_ref()
            .unwrap()
            .take(&roots, Instant::now(), true);
        self.apply_statuses(c, statuses(c, self.options.ignored, &roots));
        self.refresh.next = Some(Instant::now() + FALLBACK);
        self.refresh.requested = false;
        self.last_check.clear();
        self.last_click = None;
        self.dragged = None;
        self.invalidate_preview();
        self.changes_search.invalidate();
        self.refresh.report_error()
    }

    pub(super) fn apply_add_statuses(
        &mut self,
        c: &Context,
        mut statuses: BTreeMap<usize, Status>,
    ) {
        let refreshed = statuses
            .iter()
            .filter_map(|(&root, status)| status.is_ok().then_some(root))
            .collect::<BTreeSet<_>>();
        let scanned = statuses.keys().copied().collect::<BTreeSet<_>>();
        // An older read must not put just-added paths back into Unversioned Files.
        self.refresh.generation = self.refresh.generation.wrapping_add(1);
        if let Some(pending) = self.refresh.pending.take() {
            self.refresh
                .deferred
                .extend(pending.roots.difference(&refreshed).copied());
        }
        if !self.options.ignored {
            for changes in statuses
                .values_mut()
                .filter_map(|status| status.as_mut().ok())
            {
                changes.retain(|change| !change.ignored());
            }
        }
        self.apply_statuses(c, statuses);
        if self.diff_root.is_some_and(|root| scanned.contains(&root)) {
            self.invalidate_preview();
        }
    }

    fn apply_statuses(&mut self, c: &Context, mut statuses: BTreeMap<usize, Status>) -> bool {
        self.invalidate_ai_check();
        let scope_changed = self.rendered_scope != c.scope;
        let old_target = self
            .active()
            .map(|r| (r.id.clone(), r.file().map(|i| self.files[i].clone())));
        let mut files = Vec::new();
        for root in 0..c.repos.len() {
            match statuses.remove(&root) {
                Some(Ok(changes)) => {
                    self.refresh.errors.remove(&root);
                    for change in changes {
                        let key = format!("{}:{}", rid(c, root), keypart(&change.path));
                        let group = if change.ignored() {
                            IGNORED.into()
                        } else if change.xy == *b"??" {
                            NEW.into()
                        } else {
                            self.saved
                                .membership
                                .get(&key)
                                .cloned()
                                .unwrap_or_else(|| DEF.into())
                        };
                        self.checks
                            .entry(key.clone())
                            .or_insert(group == DEF && change.selectable());
                        files.push(File {
                            root,
                            key,
                            change,
                            group,
                        });
                    }
                }
                Some(Err(error)) => {
                    // A transient Git failure must not empty a repository's tree.
                    self.refresh.errors.insert(root, error);
                    files.extend(self.files.iter().filter(|f| f.root == root).cloned());
                }
                None => files.extend(self.files.iter().filter(|f| f.root == root).cloned()),
            }
        }
        let changed = files != self.files;
        if changed {
            self.changes_search.invalidate();
        }
        self.files = files;
        if changed || scope_changed {
            self.sync(c);
        }
        let target = self
            .active()
            .map(|r| (r.id.clone(), r.file().map(|i| self.files[i].clone())));
        if target != old_target {
            self.invalidate_preview();
        }
        changed || scope_changed
    }

    pub(crate) fn poll_refresh(&mut self, c: &mut Context) -> Result<bool> {
        if !self.loaded
            || self.execution_running()
            || self.add_running()
            || c.repos.is_empty()
            || (self.editing() && !self.message_editing())
            || self.dragged.is_some()
            || self.panes.dragging()
            || self.diff_selection.mouse_active()
            || self
                .diff_comparison
                .as_ref()
                .is_some_and(|view| view.mouse_active())
            || self.message_selection.mouse_active()
        {
            return Ok(false);
        }
        let mut changed = false;
        if let Some(pending) = &self.refresh.pending {
            match pending.receiver.try_recv() {
                Ok(snapshot) => {
                    let current = pending.generation == self.refresh.generation;
                    self.refresh.pending = None;
                    if current {
                        let Snapshot {
                            statuses,
                            branches,
                            preview,
                        } = snapshot;
                        let scanned = statuses.keys().copied().collect::<BTreeSet<_>>();
                        // Slow scans also get an idle interval. A complete scope
                        // scan already reconciles any notifications that were missed.
                        if c.scope.iter().all(|root| statuses.contains_key(root)) {
                            self.refresh.next = Some(Instant::now() + FALLBACK);
                        }
                        for (root, branch) in branches {
                            let repo = &mut c.repos[root];
                            changed |= repo.branch != branch;
                            repo.branch = branch;
                        }
                        changed |= self.apply_statuses(c, statuses);
                        let mut preview_applied = false;
                        if let Some((key, change, loaded)) = preview {
                            if self.preview_open
                                && self.active().is_some_and(|r| {
                                    r.id == key
                                        && r.file().is_some_and(|i| self.files[i].change == change)
                                })
                            {
                                changed |= self.set_preview(key, loaded);
                                preview_applied = true;
                            }
                        }
                        if !preview_applied
                            && self.diff_root.is_some_and(|root| scanned.contains(&root))
                        {
                            self.invalidate_preview();
                            changed = true;
                        }
                    }
                }
                Err(mpsc::TryRecvError::Empty) => return Ok(false),
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.refresh.pending = None;
                    bail!("Changes refresh worker stopped");
                }
            }
        }
        if self.refresh.pending.is_none() {
            let now = Instant::now();
            let fallback = self.refresh.next.is_none_or(|next| now >= next);
            let full = self.refresh.requested;
            let scope = if full {
                (0..c.repos.len()).collect()
            } else {
                c.scope.clone()
            };
            let mut roots = self
                .refresh
                .watch
                .as_ref()
                .map_or_else(BTreeSet::new, |watch| {
                    watch.take(&scope, now, full || fallback || self.refresh.expedite)
                });
            if full || fallback {
                roots.extend(scope.iter().copied());
                self.refresh.next = Some(now + FALLBACK);
            }
            roots.extend(self.refresh.take_deferred(&scope, full));
            self.refresh.requested = false;
            self.refresh.expedite = false;
            if roots.is_empty() {
                self.refresh.report_error()?;
                return Ok(changed);
            }
            self.invalidate_ai_check();
            let target = self
                .preview_open
                .then(|| self.active())
                .flatten()
                .and_then(|r| {
                    let f = &self.files[r.file()?];
                    Some((r.id.clone(), f.root, f.change.path.clone()))
                });
            let ctx = c.clone();
            let ignored = self.options.ignored;
            let scan_roots = roots.clone();
            let (send, receive) = mpsc::channel();
            thread::spawn(move || {
                let _ = send.send(Snapshot::read(ctx, ignored, target, roots));
            });
            self.refresh.pending = Some(Pending {
                generation: self.refresh.generation,
                roots: scan_roots,
                receiver: receive,
            });
        }
        self.refresh.report_error()?;
        Ok(changed)
    }
}

impl Refresh {
    fn take_deferred(&mut self, scope: &BTreeSet<usize>, all: bool) -> BTreeSet<usize> {
        if all {
            return std::mem::take(&mut self.deferred);
        }
        let due = self
            .deferred
            .intersection(scope)
            .copied()
            .collect::<BTreeSet<_>>();
        self.deferred.retain(|root| !due.contains(root));
        due
    }

    fn report_error(&mut self) -> Result<()> {
        let mut errors = self.errors.values().cloned().collect::<Vec<_>>();
        if let Some(watch) = &self.watch {
            let error = watch.error();
            if !error.is_empty() {
                errors.push(error);
            }
        }
        let error = errors.join(" | ");
        let report = !error.is_empty() && error != self.error;
        self.error = error;
        if report {
            bail!("{}", self.error);
        }
        Ok(())
    }
}
