//! Snapshot the Commit panel's selection and run the batch in the background.
use super::backend::Plan;
use super::*;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc::{self, Receiver, TryRecvError},
    Arc,
};

#[derive(Default)]
pub(super) struct Execution {
    pub enabled: bool,
    blocked: bool,
    pending: Option<Pending>,
    pub progress: String,
}

struct Pending {
    receiver: Receiver<Report>,
    stop: Arc<AtomicBool>,
    completed: Vec<Completed>,
    push: bool,
    message: Option<String>,
}

struct Completed {
    plan: Plan,
    outcome: Result<backend::Outcome, String>,
}

enum Report {
    Started(String),
    Completed(Box<Completed>),
    Done,
}

fn run(c: &Context, plans: Vec<Plan>, stop: &AtomicBool, mut report: impl FnMut(Report)) {
    let mut failed = BTreeSet::<usize>::new();
    let total = plans.len();
    for (index, plan) in plans.into_iter().enumerate() {
        let root = plan.root;
        let dependency = failed.iter().any(|&child| {
            c.repos[child].parent == Some(root)
                && plan
                    .files
                    .iter()
                    .any(|file| plan.repo.join(&file.path) == c.repos[child].path)
        });
        let outcome = if stop.load(Ordering::Acquire) {
            Err("Not started: batch stopped after the current repository".into())
        } else if !c.scope.contains(&root) {
            Err("Outside the selected Roots; not committed".into())
        } else if dependency {
            Err("Skipped because a child repository failed".into())
        } else {
            report(Report::Started(format!(
                "Committing {} · {}/{} · Ctrl-X stops remaining repositories",
                c.repos[root].name,
                index + 1,
                total
            )));
            backend::execute(&plan).map_err(|error| format!("{error:#}"))
        };
        if outcome.is_err() {
            failed.insert(root);
        }
        report(Report::Completed(Box::new(Completed { plan, outcome })));
    }
    report(Report::Done);
}

impl CommitPanel {
    #[cfg(test)]
    pub(crate) fn hold_execution(&mut self) -> impl FnOnce() {
        let (sender, receiver) = mpsc::channel();
        self.execution.pending = Some(Pending {
            receiver,
            stop: Arc::new(AtomicBool::new(false)),
            completed: Vec::new(),
            push: false,
            message: self.saved.message.clone(),
        });
        move || {
            let _ = sender.send(Report::Done);
        }
    }

    pub(crate) fn enable_background_execution(&mut self) {
        self.execution.enabled = true;
    }

    pub(crate) fn background_execution_enabled(&self) -> bool {
        self.execution.enabled
    }

    pub(crate) fn execution_running(&self) -> bool {
        self.execution.pending.is_some()
    }

    pub(crate) fn block_submissions(&mut self, blocked: bool) {
        self.execution.blocked = blocked;
    }

    pub(crate) fn stop_execution(&mut self) -> Action {
        if let Some(pending) = &self.execution.pending {
            pending.stop.store(true, Ordering::Release);
            self.execution.progress =
                "Stopping after the current repository; completed commits are kept".into();
            Action::Notice(self.execution.progress.clone())
        } else {
            Action::None
        }
    }

    pub(super) fn submit(&mut self, c: &Context, push: bool) -> Result<Action> {
        if self.add_running() {
            bail!("Adding files to Git; wait before committing");
        }
        if self.execution.blocked {
            bail!("Wait for the remote operation before committing");
        }
        if self.execution_running() {
            bail!("Wait for the current commit batch to finish");
        }
        let plans = self.prepare_submission(c)?;
        Ok(self.execute_submission(c, plans, push))
    }

    pub(super) fn prepare_submission(&mut self, c: &Context) -> Result<Vec<Plan>> {
        let mut files_by_root = BTreeMap::<_, Vec<_>>::new();
        for file in self.selection(c) {
            files_by_root
                .entry(file.root)
                .or_default()
                .push(file.change.clone());
        }
        if files_by_root.is_empty() {
            bail!("Select files first");
        }
        let msg = self.saved.message.clone().unwrap_or_default();
        if msg.trim().is_empty() {
            self.edit_message(c);
            bail!("Enter a commit message for the selected files");
        }
        let mut plans = Vec::with_capacity(files_by_root.len());
        for (root, files) in files_by_root {
            let repo = &c.repos[root];
            let plan = backend::plan(
                root,
                &repo.path,
                files,
                msg.clone(),
                self.amend.contains(&root),
            )
            .map_err(|error| anyhow::anyhow!("{}: {error}", repo.name))?;
            plans.push(plan);
        }
        plans.sort_by_key(|p| std::cmp::Reverse(p.repo.components().count()));
        Ok(plans)
    }

    pub(super) fn execute_submission(
        &mut self,
        c: &Context,
        plans: Vec<Plan>,
        push: bool,
    ) -> Action {
        let message = self.saved.message.clone();
        if !self.execution.enabled {
            let mut completed = Vec::new();
            run(c, plans, &AtomicBool::new(false), |report| {
                if let Report::Completed(result) = report {
                    completed.push(*result);
                }
            });
            return self.finish_submission(c, completed, push, message, None);
        }
        let ctx = c.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = stop.clone();
        let (sender, receiver) = mpsc::channel();
        let spawned = std::thread::Builder::new()
            .name("igit-commit".into())
            .spawn(move || {
                run(&ctx, plans, &worker_stop, |report| {
                    let _ = sender.send(report);
                });
            });
        if let Err(error) = spawned {
            return err(format!("Cannot start commit worker: {error}"));
        }
        self.execution.progress = "Starting commits…".into();
        self.execution.pending = Some(Pending {
            receiver,
            stop,
            completed: Vec::new(),
            push,
            message,
        });
        Action::Notice(self.execution.progress.clone())
    }

    pub(crate) fn poll_execution(&mut self, c: &Context) -> Action {
        let Some(mut pending) = self.execution.pending.take() else {
            return Action::None;
        };
        let mut progress = None;
        loop {
            match pending.receiver.try_recv() {
                Ok(Report::Started(label)) => progress = Some(label),
                Ok(Report::Completed(result)) => pending.completed.push(*result),
                Ok(Report::Done) => {
                    self.execution.progress.clear();
                    return self.finish_submission(
                        c,
                        pending.completed,
                        pending.push,
                        pending.message,
                        None,
                    );
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.execution.progress.clear();
                    return self.finish_submission(
                        c,
                        pending.completed,
                        false,
                        pending.message,
                        Some("Commit worker stopped before reporting completion. Inspect HEAD and the index before retrying."),
                    );
                }
            }
        }
        self.execution.pending = Some(pending);
        if let Some(label) = progress {
            self.execution.progress = label.clone();
            Action::Notice(label)
        } else {
            Action::None
        }
    }

    fn finish_submission(
        &mut self,
        c: &Context,
        completed: Vec<Completed>,
        mut push: bool,
        original_message: Option<String>,
        interrupted: Option<&str>,
    ) -> Action {
        let mut success = Vec::new();
        let mut out = Vec::new();
        let mut failed = interrupted.is_some();
        for Completed { plan, outcome } in completed {
            let name = &c.repos[plan.root].name;
            match outcome {
                Ok(result) => {
                    out.push(format!(
                        "{name} {} {}",
                        self.icons.glyph(Icon::Success),
                        result.hash
                    ));
                    if let Some(note) = result.warning {
                        push = false;
                        out.push(format!("{} {note}", self.icons.glyph(Icon::Warning)));
                    }
                    success.push(plan.root);
                    self.amend.remove(&plan.root);
                    for file in plan.files {
                        self.checks.insert(
                            format!("{}:{}", rid(c, plan.root), keypart(&file.path)),
                            false,
                        );
                    }
                }
                Err(error) => {
                    failed = true;
                    out.push(format!("{name} {} {error}", self.icons.glyph(Icon::Error)));
                }
            }
        }
        if let Some(reason) = interrupted {
            out.push(reason.into());
        }
        if !success.is_empty()
            && !failed
            && self.saved.message == original_message
            && !self
                .files
                .iter()
                .any(|file| file.change.selectable() && self.checks.get(&file.key) == Some(&true))
        {
            self.saved.message = Some(String::new());
        }
        if let Err(error) = self.save(c) {
            out.push(format!("Failed to save local state: {error}"));
        }
        Action::Committed {
            push: push && !success.is_empty(),
            repos: success,
            message: out.join(" | "),
        }
    }
}

#[cfg(test)]
mod tests;
