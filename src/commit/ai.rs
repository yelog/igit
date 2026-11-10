use super::*;
use crate::config::AiConfig;
use anyhow::Context as _;
use std::{path::PathBuf, sync::mpsc};

mod input;
#[cfg(test)]
mod tests;
mod transport;

use super::process::Control;

const PATCH_LIMIT: usize = 32 * 1024 * 1024;

struct Repository {
    path: PathBuf,
    name: String,
    files: Vec<Change>,
}

struct Job {
    settings: AiConfig,
    repositories: Vec<Repository>,
}

#[derive(PartialEq, Eq)]
struct Selection {
    scope: crate::core::loading::Scope,
    // Working edits to a staged file do not change the selected index source.
    files: Vec<(usize, PathBuf, bool)>,
}

impl Selection {
    fn new(panel: &CommitPanel, c: &Context) -> Self {
        let mut files: Vec<_> = panel
            .selection(c)
            .into_iter()
            .map(|file| (file.root, file.change.path.clone(), file.change.staged()))
            .collect();
        files.sort();
        Self {
            scope: crate::core::loading::Scope::new(c),
            files,
        }
    }
}

struct Candidate {
    message: String,
    snapshots: Vec<backend::ai::Snapshot>,
}

enum Report {
    Progress(String),
    Generated(Result<Candidate, String>),
    Checked {
        revision: u64,
        completed: Instant,
        result: Result<Candidate, String>,
    },
}

pub(super) struct Pending {
    receiver: mpsc::Receiver<Report>,
    sender: mpsc::Sender<Report>,
    message: Option<String>,
    selection: Selection,
    control: Control,
    candidate: Option<Candidate>,
    finished: Option<Result<String, String>>,
    progress: String,
    frame: usize,
    revision: u64,
}

impl Drop for Pending {
    fn drop(&mut self) {
        self.control.cancel();
    }
}

impl Job {
    fn generate(self, control: &Control, sender: &mpsc::Sender<Report>) -> Result<Candidate> {
        let progress = |message| {
            let _ = sender.send(Report::Progress(message));
        };
        let mut snapshots = Vec::new();
        let mut files = Vec::new();
        let mut bytes = 0usize;
        for repository in self.repositories {
            control.check()?;
            progress(format!("Preparing changes · {}…", repository.name));
            let mut snapshot =
                backend::ai::Snapshot::capture(&repository.path, &repository.files, control)?;
            for file in std::mem::take(&mut snapshot.files) {
                bytes = bytes.saturating_add(file.patch.len());
                if bytes > PATCH_LIMIT {
                    bail!(
                        "Selected patches exceed the 32 MiB preparation limit; select fewer files"
                    );
                }
                files.push(input::File {
                    id: files.len(),
                    repository: repository.name.clone(),
                    path: file.path.to_string_lossy().into_owned(),
                    status: file.status,
                    additions: file.additions,
                    deletions: file.deletions,
                    patch: file.patch,
                });
            }
            snapshots.push(snapshot);
        }
        let client = transport::Client::new(&self.settings, control)?;
        let message = input::generate(
            &self.settings,
            &files,
            control,
            |system, user, output_tokens| client.send(system, user, output_tokens),
            progress,
        )?;
        control.check()?;
        Ok(Candidate { message, snapshots })
    }
}

fn parse_response(body: &[u8]) -> Result<String> {
    let value: serde_json::Value = serde_json::from_slice(body)
        .map_err(|_| anyhow::anyhow!("AI service returned invalid JSON"))?;
    if value.get("error").is_some_and(|error| !error.is_null()) {
        bail!("AI service returned an error; check the endpoint, model and credentials");
    }
    let choice = &value["choices"][0];
    if let Some(reason) = choice["finish_reason"].as_str() {
        if reason != "stop" {
            bail!("AI service did not return a complete message; adjust the prompt or model and retry");
        }
    }
    let message = choice["message"]["content"]
        .as_str()
        .filter(|message| !message.trim().is_empty())
        .ok_or_else(|| anyhow::anyhow!("AI service returned no valid message.content"))?;
    if message.contains('\0') {
        bail!("AI service returned an invalid commit message");
    }
    Ok(message.trim().to_owned())
}

impl CommitPanel {
    pub(crate) fn ai_running(&self) -> bool {
        self.ai_pending.is_some()
    }

    pub(super) fn ai_spinner(&self) -> char {
        let frames: &[char] = if self.icons == IconMode::Ascii {
            &['|', '/', '-', '\\']
        } else {
            &['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧']
        };
        frames[self.ai_pending.as_ref().map_or(0, |p| p.frame) % frames.len()]
    }

    pub(super) fn ai_progress(&self) -> &str {
        self.ai_pending
            .as_ref()
            .map_or("", |pending| pending.progress.as_str())
    }

    pub(super) fn invalidate_ai_check(&mut self) {
        if let Some(pending) = &mut self.ai_pending {
            pending.revision = pending.revision.wrapping_add(1);
        }
    }

    pub(super) fn show_ai_error(&mut self, error: impl ToString) -> Action {
        self.ai_error = error.to_string();
        Action::Notice(NoticeLevel::Error, self.ai_error.clone())
    }

    // This captures panel state only. All Git and filesystem work is on workers.
    fn prepare_ai(&self, c: &Context) -> Result<Job> {
        if self.ai_settings.endpoint.trim().is_empty() || self.ai_settings.model.trim().is_empty() {
            bail!("Set ai.endpoint and ai.model in settings.toml, then restart IGit. Press S to edit AI settings for this session");
        }
        self.ai_settings.validate()?;
        let mut selected = BTreeMap::<usize, Vec<Change>>::new();
        for file in self.selection(c) {
            selected
                .entry(file.root)
                .or_default()
                .push(file.change.clone());
        }
        if selected.is_empty() {
            bail!(Constraint("Select files first"));
        }
        Ok(Job {
            settings: self.ai_settings.clone(),
            repositories: selected
                .into_iter()
                .map(|(root, files)| Repository {
                    path: c.repos[root].path.clone(),
                    name: c.repos[root].name.clone(),
                    files,
                })
                .collect(),
        })
    }

    pub(super) fn start_ai(&mut self, c: &Context) -> Result<Action> {
        if self.ai_pending.is_some() {
            bail!(Constraint("AI request in progress. Please wait"));
        }
        let job = self.prepare_ai(c)?;
        let control = Control::new(Duration::from_secs(job.settings.total_timeout_seconds));
        let worker_control = control.clone();
        let (sender, receiver) = mpsc::channel();
        let worker_sender = sender.clone();
        std::thread::Builder::new()
            .name("igit-ai-prepare".into())
            .spawn(move || {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    job.generate(&worker_control, &worker_sender)
                }))
                .unwrap_or_else(|_| Err(anyhow::anyhow!("AI worker stopped unexpectedly")))
                .map_err(|error| error.to_string());
                let _ = worker_sender.send(Report::Generated(result));
            })
            .context("Cannot start the AI worker")?;
        self.ai_pending = Some(Pending {
            receiver,
            sender,
            message: self.saved.message.clone(),
            selection: Selection::new(self, c),
            control,
            candidate: None,
            finished: None,
            progress: "Preparing changes…".into(),
            frame: 0,
            revision: 0,
        });
        self.ai_error.clear();
        Ok(Action::Notice(
            NoticeLevel::Info,
            "Generating one commit message. Preparing changes in the background".into(),
        ))
    }

    pub(super) fn poll_ai(&mut self, c: &Context) -> Action {
        let editing = self.editing();
        let selection = self.ai_pending.as_ref().map(|_| Selection::new(self, c));
        let Some(pending) = self.ai_pending.as_mut() else {
            return Action::None;
        };
        pending.frame = pending.frame.wrapping_add(1);
        if selection.as_ref() != Some(&pending.selection) {
            pending.control.cancel();
            pending.finished = Some(Err(
                "The selected files or Roots changed during generation. The original message was kept. Generate again for the current selection".into(),
            ));
        } else if let Err(error) = pending.control.check() {
            if pending.finished.is_none() {
                pending.finished = Some(Err(error.to_string()));
            }
        }
        // Drain progress during editing, but defer adoption of generated messages.
        while let Ok(report) = pending.receiver.try_recv() {
            if pending.finished.is_some() {
                continue;
            }
            match report {
                Report::Progress(progress) => pending.progress = progress,
                Report::Generated(Ok(candidate)) => pending.candidate = Some(candidate),
                Report::Checked {
                    revision,
                    completed,
                    result: Ok(candidate),
                } if !editing
                    && revision == pending.revision
                    && completed.elapsed() <= Duration::from_secs(1) =>
                {
                    pending.finished = Some(Ok(candidate.message));
                }
                Report::Checked {
                    result: Ok(candidate),
                    ..
                } => {
                    // Recheck after any intervening input/refresh or delayed poll,
                    // including editors that opened and closed between ticks.
                    pending.candidate = Some(candidate);
                }
                Report::Generated(Err(error))
                | Report::Checked {
                    result: Err(error), ..
                } => {
                    pending.finished = Some(Err(error));
                }
            }
        }
        if editing && !matches!(pending.finished, Some(Err(_))) {
            return Action::None;
        }
        if let Some(candidate) = pending.candidate.take() {
            if pending.finished.is_none() {
                let sender = pending.sender.clone();
                let control = pending.control.clone();
                let revision = pending.revision;
                pending.progress = "Checking changes…".into();
                if let Err(error) = std::thread::Builder::new()
                    .name("igit-ai-check".into())
                    .spawn(move || {
                        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                            for snapshot in &candidate.snapshots {
                                snapshot.validate(&control)?;
                            }
                            control.check()?;
                            Ok(candidate)
                        }))
                        .unwrap_or_else(|_| {
                            Err(anyhow::anyhow!("AI source checker stopped unexpectedly"))
                        })
                        .map_err(|error: anyhow::Error| error.to_string());
                        let _ = sender.send(Report::Checked {
                            revision,
                            completed: Instant::now(),
                            result,
                        });
                    })
                {
                    pending.finished =
                        Some(Err(format!("Cannot check AI source changes: {error}")));
                }
            }
        }
        let Some(result) = pending.finished.take() else {
            return Action::None;
        };
        let original = self.ai_pending.take().unwrap().message.clone();
        match result {
            Ok(message) => {
                if self.saved.message != original {
                    return self.show_ai_error("The commit message changed during generation. Your newer message was kept. Generate again to replace it");
                }
                self.saved.message = Some(message);
                if let Err(error) = self.save(c) {
                    self.saved.message = original;
                    return self.show_ai_error(format!(
                        "Could not save AI commit message: {error}. The original message was kept"
                    ));
                }
                self.message_selection.clear();
                self.edit_view = (self.edit_view.0, 0, 0);
                self.set_focus(Focus::Message);
                Action::Notice(NoticeLevel::Success, "AI commit message generated".into())
            }
            Err(error) => self.show_ai_error(error),
        }
    }
}
