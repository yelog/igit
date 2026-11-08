use super::*;
use anyhow::Context as _;
use std::{path::PathBuf, sync::mpsc};
use unicode_width::UnicodeWidthStr;

#[cfg(test)]
mod tests;

#[derive(Default)]
pub(super) struct State {
    pending: Option<Pending>,
}

struct Pending {
    receiver: mpsc::Receiver<Report>,
    started: Instant,
    total: usize,
    repositories: usize,
    phase: String,
    completed: Vec<Completed>,
}

struct Job {
    root: usize,
    path: PathBuf,
    name: String,
    files: Vec<Change>,
}

struct Completed {
    root: usize,
    path: PathBuf,
    name: String,
    result: Result<usize, String>,
    status: Result<Vec<Change>, String>,
}

enum Report {
    Phase(String),
    Completed(Completed),
    Done,
}

fn run(jobs: Vec<Job>, ignored: bool, sender: mpsc::Sender<Report>) {
    for job in jobs {
        let _ = sender.send(Report::Phase(format!("Checking {}", job.name)));
        let result = backend::add::review(&job.path, job.files)
            .and_then(|plan| {
                let _ = sender.send(Report::Phase(format!("Staging {}", job.name)));
                backend::add::execute(&plan)?;
                Ok(plan.files.len())
            })
            .map_err(|error| format!("{error:#}"));
        let _ = sender.send(Report::Phase(format!("Updating {}", job.name)));
        let status = backend::status(&job.path)
            .and_then(|mut changes| {
                if ignored {
                    changes.extend(backend::ignored(&job.path)?);
                }
                Ok(changes)
            })
            .map_err(|error| format!("{}: {error}", job.name));
        let _ = sender.send(Report::Completed(Completed {
            root: job.root,
            path: job.path,
            name: job.name,
            result,
            status,
        }));
    }
    let _ = sender.send(Report::Done);
}

impl CommitPanel {
    pub(crate) fn add_running(&self) -> bool {
        self.add.pending.is_some()
    }

    pub(crate) fn add_progress(&self, width: usize) -> String {
        let Some(pending) = &self.add.pending else {
            return String::new();
        };
        let frames: &[char] = if self.icons == IconMode::Ascii {
            &['|', '/', '-', '\\']
        } else {
            &['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧']
        };
        let spinner = frames[(pending.started.elapsed().as_millis() / 100) as usize % frames.len()];
        let label = format!(
            "{spinner} Adding {} file{}",
            pending.total,
            if pending.total == 1 { "" } else { "s" }
        );
        let candidates = [
            format!(
                "{label} · {} · {}/{} repositories done",
                pending.phase,
                pending.completed.len(),
                pending.repositories
            ),
            format!(
                "{label} · {}/{} repos done",
                pending.completed.len(),
                pending.repositories
            ),
            format!("{label}…"),
        ];
        crate::ui::idea::clipped(
            candidates
                .iter()
                .find(|text| text.width() <= width)
                .unwrap_or(&candidates[2]),
            width,
        )
    }

    pub(super) fn add_untracked(&mut self, c: &Context) -> Result<Action> {
        if self.add_running() {
            return Ok(Action::Notice(self.add_progress(usize::MAX)));
        }
        let row = self
            .active()
            .ok_or_else(|| anyhow::anyhow!("Select untracked files to add"))?;
        let mut files = BTreeMap::<usize, Vec<Change>>::new();
        for &i in &row.files {
            let file = &self.files[i];
            if c.scope.contains(&file.root) && file.change.xy == *b"??" && file.change.selectable()
            {
                files
                    .entry(file.root)
                    .or_default()
                    .push(file.change.clone());
            }
        }
        if files.is_empty() {
            bail!("No untracked files in the highlighted scope");
        }
        let total = files.values().map(Vec::len).sum();
        let repositories = files.len();
        let jobs = files
            .into_iter()
            .map(|(root, files)| {
                let repo = &c.repos[root];
                Job {
                    root,
                    path: repo.path.clone(),
                    name: repo.name.clone(),
                    files,
                }
            })
            .collect();
        let ignored = self.options.ignored;
        let (sender, receiver) = mpsc::channel();
        std::thread::Builder::new()
            .name("igit-add".into())
            .spawn(move || run(jobs, ignored, sender))
            .context("Cannot start Add to Git; retry the shortcut")?;
        self.add.pending = Some(Pending {
            receiver,
            started: Instant::now(),
            total,
            repositories,
            phase: "Starting".into(),
            completed: Vec::new(),
        });
        Ok(Action::Notice(self.add_progress(usize::MAX)))
    }

    pub(crate) fn poll_add(&mut self, c: &Context) -> Action {
        let Some(mut pending) = self.add.pending.take() else {
            return Action::None;
        };
        let interrupted = loop {
            match pending.receiver.try_recv() {
                Ok(Report::Phase(phase)) => pending.phase = phase,
                Ok(Report::Completed(result)) => pending.completed.push(result),
                Ok(Report::Done) => break false,
                Err(mpsc::TryRecvError::Disconnected) => break true,
                Err(mpsc::TryRecvError::Empty) => {
                    self.add.pending = Some(pending);
                    return Action::None;
                }
            }
        };
        let mut statuses = BTreeMap::new();
        let mut results = Vec::new();
        let mut added = 0;
        let mut failed = pending.repositories.saturating_sub(pending.completed.len());
        for completed in pending.completed {
            if c.repos
                .get(completed.root)
                .is_none_or(|repo| repo.path != completed.path)
            {
                failed += 1;
                results.push(format!(
                    "{}: workspace changed; refresh before retrying",
                    completed.name
                ));
                continue;
            }
            match completed.result {
                Ok(count) => {
                    added += count;
                    results.push(format!("ADDED · {} · {count} files", completed.name));
                }
                Err(error) => {
                    failed += 1;
                    results.push(format!("FAILED · {}: {error}", completed.name));
                }
            }
            if let Err(error) = &completed.status {
                results.push(format!("Refresh failed: {error}. Press F5 to refresh"));
            }
            statuses.insert(completed.root, completed.status);
        }
        self.apply_add_statuses(c, statuses);
        if interrupted {
            self.request_refresh();
            results.push("Add worker stopped. Inspect Git status before retrying; completed additions are kept".into());
        }
        Action::Notice(format!(
            "Add to Git: {added} added · {failed} failed\n{}",
            results.join("\n")
        ))
    }
}
