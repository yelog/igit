use super::*;
use crate::log::snapshot::Snapshot;
use anyhow::anyhow;
use std::{
    sync::mpsc::{self, Receiver, TryRecvError},
    thread,
    time::Instant,
};

pub(super) const NAV_SETTLE: Duration = Duration::from_millis(60);

struct Job {
    generation: u64,
    identity: (usize, String),
    reply: Receiver<Result<Snapshot>>,
}

#[derive(Default)]
pub(super) struct Preview {
    generation: u64,
    due: Option<(Instant, (usize, String))>,
    running: Option<Job>,
    loaded: Option<(usize, String)>,
    error: Option<String>,
}

impl Preview {
    pub(super) fn message(&self) -> Option<&str> {
        if self.due.is_some()
            || self
                .running
                .as_ref()
                .is_some_and(|j| j.generation == self.generation)
        {
            Some("Loading saved files…")
        } else {
            self.error.as_deref()
        }
    }
}

impl StashPanel {
    pub(super) fn request_preview(&mut self, delay: Duration) {
        let identity = self.identity();
        if identity.is_some()
            && (self.preview.loaded == identity
                || self
                    .preview
                    .due
                    .as_ref()
                    .is_some_and(|(_, target)| Some(target) == identity.as_ref())
                || self.preview.running.as_ref().is_some_and(|job| {
                    job.generation == self.preview.generation
                        && Some(&job.identity) == identity.as_ref()
                }))
        {
            return;
        }
        self.preview.generation += 1;
        self.preview.due = identity.map(|identity| (Instant::now() + delay, identity));
        self.preview.loaded = None;
        self.preview.error = None;
        self.files.clear();
    }

    pub(crate) fn preview_wakeup(&self) -> Option<Duration> {
        if self.preview.running.is_some() {
            Some(Duration::from_millis(16))
        } else {
            self.preview
                .due
                .as_ref()
                .map(|(at, _)| at.saturating_duration_since(Instant::now()))
        }
    }

    #[cfg(test)]
    pub(super) fn preview_state(&self) -> (u64, Option<(usize, String)>) {
        (self.preview.generation, self.preview.loaded.clone())
    }

    pub(crate) fn poll_preview(&mut self, c: &Context) -> bool {
        let mut redraw = false;
        if let Some(job) = &self.preview.running {
            let result = match job.reply.try_recv() {
                Ok(result) => Some(result),
                Err(TryRecvError::Empty) => None,
                Err(TryRecvError::Disconnected) => {
                    Some(Err(anyhow!("Stash preview worker stopped")))
                }
            };
            if let Some(result) = result {
                if job.generation == self.preview.generation {
                    match result {
                        Ok(snapshot) => {
                            self.files.set(snapshot);
                            self.preview.loaded = Some(job.identity.clone());
                        }
                        Err(error) => {
                            self.preview.loaded = None;
                            self.preview.error = Some(format!("Could not load stash: {error:#}"));
                        }
                    }
                    redraw = true;
                }
                self.preview.running = None;
            }
        }
        let due = self
            .preview
            .running
            .is_none()
            .then(|| self.preview.due.take_if(|(at, _)| *at <= Instant::now()))
            .flatten();
        if let Some((_, (root, oid))) = due {
            let path = c.repos[root].path.clone();
            let identity = (root, oid.clone());
            let (send, reply) = mpsc::channel();
            thread::spawn(move || {
                let _ = send.send(Snapshot::stash(&path, root, &oid));
            });
            self.preview.running = Some(Job {
                generation: self.preview.generation,
                identity,
                reply,
            });
        }
        redraw
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stash_stale_preview_success_and_errors_are_discarded_after_scope_changes() {
        let (_temp, mut c, mut panel) = super::super::tests::fixture();
        let (root, oid) = panel.identity().unwrap();
        let old = Snapshot::stash(&c.repos[root].path, root, &oid).unwrap();
        let (send, reply) = mpsc::channel();
        panel.preview.running = Some(Job {
            generation: panel.preview.generation,
            identity: (root, oid.clone()),
            reply,
        });
        panel.preview.due = None;
        c.scope.clear();
        panel.refresh(&c).unwrap();
        send.send(Ok(old)).ok().unwrap();
        assert!(!panel.poll_preview(&c));
        assert!(panel.preview.message().is_none() && panel.preview_wakeup().is_none());
        assert!(panel.selection().is_none());

        let (send, reply) = mpsc::channel();
        panel.preview.running = Some(Job {
            generation: panel.preview.generation,
            identity: (root, oid),
            reply,
        });
        panel.refresh(&c).unwrap();
        send.send(Err(anyhow!("old repository error")))
            .ok()
            .unwrap();
        assert!(!panel.poll_preview(&c));
        assert!(panel.preview.message().is_none() && panel.preview_wakeup().is_none());
    }
}
