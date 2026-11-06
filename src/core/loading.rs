//! Keep repository reads off the terminal thread and retain only the latest request.
use super::Context;
use anyhow::Result;
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, AtomicUsize, Ordering},
        mpsc::{self, Receiver, TryRecvError},
        Arc,
    },
    thread,
};

pub(crate) struct Load<T> {
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

pub(crate) struct Version {
    generation: Arc<AtomicU64>,
    value: u64,
}

impl Version {
    pub(crate) fn cancelled(&self) -> bool {
        self.generation.load(Ordering::Relaxed) != self.value
    }
}

impl<T> Drop for Load<T> {
    fn drop(&mut self) {
        self.generation.fetch_add(1, Ordering::Relaxed);
    }
}

impl<T: Send + 'static> Load<T> {
    pub(crate) fn request(&mut self) {
        self.cancel();
        self.requested = true;
    }

    pub(crate) fn cancel(&mut self) {
        self.generation.fetch_add(1, Ordering::Relaxed);
        self.requested = false;
    }

    pub(crate) fn pending(&self) -> bool {
        self.requested || self.running.is_some()
    }

    pub(crate) fn loading(&self) -> bool {
        self.requested
            || self
                .running
                .as_ref()
                .is_some_and(|(v, _)| *v == self.generation.load(Ordering::Relaxed))
    }

    pub(crate) fn needs_start(&self) -> bool {
        self.requested && self.running.is_none()
    }

    pub(crate) fn start(
        &mut self,
        name: &str,
        read: impl FnOnce(Version) -> T + Send + 'static,
    ) -> Result<()> {
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
        thread::Builder::new().name(name.into()).spawn(move || {
            let _ = send.send(read(version));
        })?;
        self.running = Some((value, receive));
        Ok(())
    }

    pub(crate) fn take_result(&mut self) -> Option<Result<T>> {
        let (generation, receiver) = self.running.as_ref()?;
        let result = match receiver.try_recv() {
            Ok(value) => Ok(value),
            Err(TryRecvError::Empty) => return None,
            Err(TryRecvError::Disconnected) => Err(anyhow::anyhow!(
                "Background reader stopped; refresh to retry"
            )),
        };
        let current = *generation == self.generation.load(Ordering::Relaxed);
        self.running = None;
        current.then_some(result)
    }

    #[cfg(test)]
    pub(crate) fn hold(&mut self) -> mpsc::Sender<T> {
        assert!(self.needs_start());
        self.requested = false;
        let (send, receive) = mpsc::channel();
        self.running = Some((self.generation.load(Ordering::Relaxed), receive));
        send
    }
}

#[derive(PartialEq, Eq)]
pub(crate) struct Scope {
    workspace: PathBuf,
    roots: Vec<(usize, PathBuf)>,
}

impl Scope {
    pub(crate) fn new(c: &Context) -> Self {
        Self {
            workspace: c.workspace.clone(),
            roots: c
                .scope
                .iter()
                .map(|&i| (i, c.repos[i].path.clone()))
                .collect(),
        }
    }

    pub(crate) fn matches(&self, c: &Context) -> bool {
        self.workspace == c.workspace
            && self.roots.len() == c.scope.len()
            && self
                .roots
                .iter()
                .zip(&c.scope)
                .all(|((i, path), root)| i == root && path == &c.repos[*root].path)
    }
}

pub(crate) fn read_repositories<Job: Sync, Output: Send>(
    jobs: &[Job],
    version: &Version,
    read: impl Fn(&Job) -> Output + Sync,
) -> Vec<Output> {
    // Bound disk and process pressure even when Roots contains hundreds of repositories.
    const READERS: usize = 4;
    let next = AtomicUsize::new(0);
    thread::scope(|scope| {
        let workers: Vec<_> = (0..READERS.min(jobs.len()))
            .map(|_| {
                scope.spawn(|| {
                    let mut results = Vec::new();
                    while !version.cancelled() {
                        let i = next.fetch_add(1, Ordering::Relaxed);
                        let Some(job) = jobs.get(i) else { break };
                        results.push((i, read(job)));
                    }
                    results
                })
            })
            .collect();
        let mut results: Vec<_> = workers
            .into_iter()
            .flat_map(|worker| worker.join().unwrap())
            .collect();
        results.sort_unstable_by_key(|(i, _)| *i);
        results.into_iter().map(|(_, value)| value).collect()
    })
}

#[cfg(test)]
mod tests;
