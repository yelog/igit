//! A navigation burst needs one inspector, for the row where it ends.
use super::{file_history, inspector::Inspector, model, Details, DiffView, LogPanel};
use crate::core::Context;
use anyhow::{anyhow, Result};
use std::{
    sync::mpsc::{self, Receiver, TryRecvError},
    thread,
    time::{Duration, Instant},
};

pub(super) const NAV_SETTLE: Duration = Duration::from_millis(60);

type Loaded = Result<(Details, Inspector, Option<DiffView>)>;

struct Job {
    generation: u64,
    reply: Receiver<Loaded>,
}

#[derive(Default)]
pub(super) struct Preview {
    // Changes even when refreshing the same (root, SHA): refs may have moved.
    generation: u64,
    due: Option<Instant>,
    running: Option<Job>,
    error: Option<String>,
}

impl Preview {
    pub(super) fn loading(&self) -> bool {
        self.due.is_some()
            || self
                .running
                .as_ref()
                .is_some_and(|j| j.generation == self.generation)
    }

    pub(super) fn message(&self) -> Option<&str> {
        if self.loading() {
            Some("Loading commit details…")
        } else {
            self.error.as_deref()
        }
    }

    pub(super) fn expedite(&mut self) {
        if self.due.is_some() {
            self.due = Some(Instant::now());
        }
    }

    fn take_result(&mut self) -> Option<Loaded> {
        let job = self.running.as_ref()?;
        let result = match job.reply.try_recv() {
            Ok(result) => result,
            Err(TryRecvError::Empty) => return None,
            Err(TryRecvError::Disconnected) => Err(anyhow!("Log preview worker stopped")),
        };
        let current = job.generation == self.generation;
        self.running = None;
        current.then_some(result)
    }
}

impl LogPanel {
    pub(super) fn request_preview(&mut self, delay: Duration) {
        self.info_selection.reset();
        self.info_click = None;
        self.preview.generation += 1;
        self.preview.due = self.rev().map(|_| Instant::now() + delay);
        self.preview.error = None;
        self.details = Details::default();
        self.inspector = Inspector::default();
        self.diff = None;
        self.history_diff = None;
        self.history_mouse = false;
    }

    pub(crate) fn preview_wakeup(&self) -> Option<Duration> {
        if self.preview.running.is_some() {
            Some(Duration::from_millis(16))
        } else {
            self.preview
                .due
                .map(|at| at.saturating_duration_since(Instant::now()))
        }
    }

    pub(crate) fn poll_preview(&mut self, c: &Context) -> Result<bool> {
        if let Some(result) = self.preview.take_result() {
            let open_diff = self.diff.take_if(|d| d.waiting_for_files).is_some();
            match result {
                Ok((details, tree, history_diff)) => {
                    self.details = details;
                    self.inspector = tree;
                    self.history_diff = history_diff;
                    if open_diff {
                        self.open_diff(c)?;
                    }
                }
                Err(e) => {
                    self.preview.error = Some(format!("Could not load commit details: {e:#}"));
                    return Err(e);
                }
            }
            return Ok(true);
        }
        // A slow query can occupy one worker, but moving never queues work behind it.
        // When it finishes, start only the newest selection, from its own repository.
        if self.preview.running.is_none() && self.preview.due.is_some_and(|at| at <= Instant::now())
        {
            self.preview.due = None;
            if let Some(rev) = self.rev().cloned() {
                let repo = c.repos[rev.root].clone();
                let history_file = self.filter.file_history.clone();
                let (send, reply) = mpsc::channel();
                thread::spawn(move || {
                    let loaded = (|| {
                        if let Some(target) = history_file {
                            let file = model::File {
                                raw: rev.file.as_ref().map_or_else(
                                    || target.path.as_os_str().as_encoded_bytes().to_vec(),
                                    |file| file.raw.clone(),
                                ),
                                ..Default::default()
                            };
                            let diff = file_history::comparison(&repo, &rev, &file, 0)?;
                            let details = Details {
                                files: vec![file],
                                ..Default::default()
                            };
                            Ok((details, Inspector::default(), Some(diff)))
                        } else {
                            let details = model::details(&repo.path, &rev)?;
                            let tree = Inspector::new(&details);
                            Ok((details, tree, None))
                        }
                    })();
                    let _ = send.send(loaded);
                });
                self.preview.running = Some(Job {
                    generation: self.preview.generation,
                    reply,
                });
            }
        }
        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        log::tests::{commit, finish_preview, git, init},
        ui::Panel,
    };

    // Control completion separately from selection, without relying on Git or timer races.
    fn hold_reply(p: &mut LogPanel) -> mpsc::Sender<Loaded> {
        let (send, reply) = mpsc::channel();
        p.preview.due = None;
        p.preview.running = Some(Job {
            generation: p.preview.generation,
            reply,
        });
        send
    }

    #[test]
    fn stale_success_on_refresh_and_stale_error_on_empty_scope_are_discarded() {
        let d = init();
        commit(d.path(), "file", "unchanged object");
        let mut c = Context::discover(d.path()).unwrap();
        let mut p = LogPanel::default();
        p.refresh(&c).unwrap();
        let old = hold_reply(&mut p);
        git(d.path(), &["branch", "new-ref"]);
        p.refresh(&c).unwrap(); // Same repository and SHA, different refs.
        assert!(!p.poll_preview(&c).unwrap());
        old.send(Ok((
            Details {
                branches: vec![(model::RefKind::Local, "obsolete".into())],
                ..Default::default()
            },
            Inspector::default(),
            None,
        )))
        .unwrap();
        assert!(!p.poll_preview(&c).unwrap());
        assert!(p.details.branches.is_empty());
        finish_preview(&mut p, &c);
        assert!(p.details.branches.iter().any(|(_, b)| b == "new-ref"));
        assert!(!p.details.branches.iter().any(|(_, b)| b == "obsolete"));

        p.refresh(&c).unwrap();
        let old = hold_reply(&mut p);
        c.scope.clear();
        p.refresh(&c).unwrap();
        old.send(Err(anyhow!("previous repository is gone")))
            .unwrap();
        assert!(!p.poll_preview(&c).unwrap());
        assert!(p.rev().is_none() && p.preview.message().is_none() && p.preview_wakeup().is_none());
    }
}
