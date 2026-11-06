//! Read-only work stays off the terminal thread, with one latest request per reader.
use super::*;
use std::{
    sync::{
        atomic::{AtomicU64, AtomicUsize, Ordering},
        mpsc::TryRecvError,
        Arc,
    },
    thread,
};

pub(super) struct Load<T> {
    generation: Arc<AtomicU64>,
    requested: bool,
    running: Option<(u64, Receiver<T>)>,
}

impl<T> Default for Load<T> {
    fn default() -> Self {
        Self {
            generation: Arc::default(),
            requested: false,
            running: None,
        }
    }
}

pub(super) struct Version {
    generation: Arc<AtomicU64>,
    value: u64,
}

impl Version {
    fn cancelled(&self) -> bool {
        self.generation.load(Ordering::Relaxed) != self.value
    }
}

impl<T> Drop for Load<T> {
    fn drop(&mut self) {
        self.generation.fetch_add(1, Ordering::Relaxed);
    }
}

impl<T: Send + 'static> Load<T> {
    pub(super) fn request(&mut self) {
        self.cancel();
        self.requested = true;
    }

    pub(super) fn cancel(&mut self) {
        self.generation.fetch_add(1, Ordering::Relaxed);
        self.requested = false;
    }

    pub(super) fn pending(&self) -> bool {
        self.requested || self.running.is_some()
    }

    pub(super) fn loading(&self) -> bool {
        self.requested
            || self
                .running
                .as_ref()
                .is_some_and(|(v, _)| *v == self.generation.load(Ordering::Relaxed))
    }

    fn needs_start(&self) -> bool {
        self.requested && self.running.is_none()
    }

    pub(super) fn start(&mut self, read: impl FnOnce(Version) -> T + Send + 'static) -> Result<()> {
        if !self.needs_start() {
            return Ok(());
        }
        self.requested = false;
        let value = self.generation.load(Ordering::Relaxed);
        let version = Version {
            generation: self.generation.clone(),
            value,
        };
        let (send, receive) = mpsc::channel();
        thread::Builder::new()
            .name("igit-push-read".into())
            .spawn(move || {
                let _ = send.send(read(version));
            })?;
        self.running = Some((value, receive));
        Ok(())
    }

    pub(super) fn take_result(&mut self) -> Option<Result<T>> {
        let (generation, receiver) = self.running.as_ref()?;
        let result = match receiver.try_recv() {
            Ok(value) => Ok(value),
            Err(TryRecvError::Empty) => return None,
            Err(TryRecvError::Disconnected) => {
                Err(anyhow::anyhow!("Push reader stopped; refresh to retry"))
            }
        };
        let current = *generation == self.generation.load(Ordering::Relaxed);
        self.running = None;
        current.then_some(result)
    }
}

#[derive(PartialEq, Eq)]
pub(super) struct Scope {
    workspace: PathBuf,
    roots: Vec<(usize, PathBuf)>,
}

impl Scope {
    pub(super) fn new(c: &Context) -> Self {
        Self {
            workspace: c.workspace.clone(),
            roots: c
                .scope
                .iter()
                .map(|&i| (i, c.repos[i].path.clone()))
                .collect(),
        }
    }

    pub(super) fn matches(&self, c: &Context) -> bool {
        self.workspace == c.workspace
            && self.roots.len() == c.scope.len()
            && self
                .roots
                .iter()
                .zip(&c.scope)
                .all(|((i, path), root)| i == root && path == &c.repos[*root].path)
    }
}

fn entries(jobs: Vec<(usize, Repo, Option<Target>)>, version: Version) -> Vec<Entry> {
    // Bound disk/process pressure even for workspaces with hundreds of roots.
    const READERS: usize = 4;
    let next = AtomicUsize::new(0);
    thread::scope(|scope| {
        let workers: Vec<_> = (0..READERS.min(jobs.len()))
            .map(|_| {
                scope.spawn(|| {
                    let mut entries = Vec::new();
                    while !version.cancelled() {
                        let Some((i, repo, target)) =
                            jobs.get(next.fetch_add(1, Ordering::Relaxed))
                        else {
                            break;
                        };
                        entries.push(entry(*i, repo, target.as_ref()));
                    }
                    entries
                })
            })
            .collect();
        let mut entries: Vec<_> = workers
            .into_iter()
            .flat_map(|w| w.join().unwrap())
            .collect();
        entries.sort_by_key(|e| e.idx);
        entries
    })
}

impl PushPanel {
    pub(crate) fn reads_pending(&self) -> bool {
        self.load.pending() || self.preview_load.pending()
    }

    pub(super) fn read_status(&self) -> Option<&str> {
        if self.load.loading() {
            Some(if self.entries.is_empty() {
                "Loading outgoing commits…"
            } else {
                "Refreshing outgoing commits…"
            })
        } else {
            self.load_error.as_deref()
        }
    }

    pub(super) fn selected_identity(&self) -> Option<(usize, Option<String>)> {
        let (root, commit) = self.selected()?;
        let e = self.entries.iter().find(|e| e.idx == root)?;
        Some((
            root,
            commit.and_then(|i| e.logs.get(i)).map(|h| h.id.clone()),
        ))
    }

    pub(super) fn position_for_identity(
        &self,
        (root, commit): &(usize, Option<String>),
    ) -> Option<usize> {
        let entry = self.entries.iter().find(|e| e.idx == *root)?;
        let commit = commit
            .as_ref()
            .and_then(|id| entry.logs.iter().position(|h| &h.id == id));
        let rows = self.rows();
        rows.iter()
            .position(|row| *row == (*root, commit))
            .or_else(|| rows.iter().position(|row| *row == (*root, None)))
    }

    fn install_entries(&mut self, entries: Vec<Entry>, c: &Context) {
        let old = self.selected_identity();
        let previous = old
            .as_ref()
            .and_then(|(i, _)| self.entries.iter().find(|e| e.idx == *i))
            .cloned();
        let reseed = self.reseed.take();
        self.entries = entries;
        if let Some(roots) = &reseed {
            self.checked = roots.iter().copied().collect();
            self.expanded = self.checked.clone();
            self.init = true;
        }
        if !self.init {
            if let Some(e) = self.entries.iter().find(|e| e.n > 0) {
                self.checked.insert(e.idx);
                self.expanded.insert(e.idx);
            }
            self.init = true;
        }
        self.entries.sort_by(|a, b| {
            c.repos[a.idx]
                .name
                .to_lowercase()
                .cmp(&c.repos[b.idx].name.to_lowercase())
                .then(a.idx.cmp(&b.idx))
        });
        let rows = self.rows();
        self.pos = old
            .as_ref()
            .filter(|_| reseed.is_none())
            .and_then(|identity| self.position_for_identity(identity))
            .or_else(|| {
                rows.iter()
                    .position(|(i, k)| k.is_none() && self.checked.contains(i))
            })
            .unwrap_or(0);
        let current = self.selected_identity();
        let selected = current
            .as_ref()
            .and_then(|(i, _)| self.entries.iter().find(|e| e.idx == *i));
        // Identical snapshots keep the current file, folds, scroll and text selection.
        if reseed.is_some() || old != current || previous.as_ref() != selected {
            self.inspect(c);
        }
    }

    pub(crate) fn poll_loading(&mut self, c: &Context) -> Result<bool> {
        let mut changed = false;
        if self
            .load_scope
            .as_ref()
            .is_some_and(|scope| !scope.matches(c))
        {
            self.refresh(c)?;
            changed = true;
        }
        if !self.busy() {
            if let Some(result) = self.load.take_result() {
                match result {
                    Ok(entries) => self.install_entries(entries, c),
                    Err(error) => {
                        self.load_error = Some(error.to_string());
                        return Err(error);
                    }
                }
                changed = true;
            }
            if self.load.needs_start() {
                let jobs = c
                    .scope
                    .iter()
                    .map(|&i| (i, c.repos[i].clone(), self.targets.get(&i).cloned()))
                    .collect();
                if let Err(error) = self.load.start(move |version| entries(jobs, version)) {
                    self.load_error = Some(format!("Unable to load outgoing commits: {error}"));
                    return Err(error);
                }
            }
        }
        if let Some(result) = self.preview_load.take_result() {
            self.preview_request = None;
            match result {
                Ok(preview::Loaded::Files { files, summary }) => {
                    self.preview = Preview {
                        tree: FileTree::new(&files, self.grouping),
                        files,
                        body: summary.clone(),
                        summary,
                        is_patch: false,
                    };
                    if self.show_details {
                        self.inspect_file(c);
                    }
                }
                Ok(preview::Loaded::Patch { body, is_patch }) => {
                    self.preview.body = body;
                    self.preview.is_patch = is_patch;
                }
                Err(error) => {
                    self.preview.body = error.to_string();
                    return Err(error);
                }
            }
            changed = true;
        }
        if self.preview_load.needs_start() {
            if let Some(request) = self.preview_request.clone() {
                if let Err(error) = self.preview_load.start(move |_| preview::read(request)) {
                    self.preview_request = None;
                    self.preview.body = format!("Unable to load preview: {error}");
                    return Err(error);
                }
            }
        }
        Ok(changed)
    }

    #[cfg(test)]
    pub(crate) fn finish_loading(&mut self, c: &Context) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
        while self.reads_pending() {
            self.poll_loading(c).unwrap();
            assert!(
                std::time::Instant::now() < deadline,
                "Push reads did not finish"
            );
            thread::sleep(std::time::Duration::from_millis(1));
        }
    }
}

#[cfg(test)]
mod tests;
