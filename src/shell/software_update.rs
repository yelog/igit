//! Application updates have no connection to repository Pull/Fetch.
mod cache;
#[cfg(test)]
mod tests;
mod view;

use crate::{
    config::UpdateConfig,
    installation::{self, Inspection, Status},
};
use std::{
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver, TryRecvError},
    time::{Instant, SystemTime},
};

#[derive(Default)]
enum State {
    #[default]
    Idle,
    Checking,
    Report(Inspection),
    Installing(Inspection),
    Ready(Inspection),
    Failed(String),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Action {
    Check,
    Install,
    Restart,
    Close,
}

#[derive(Default)]
pub(super) struct SoftwareUpdate {
    state: State,
    job: Option<Receiver<Result<Inspection, String>>>,
    automatic: bool,
    scheduled: bool,
    startup_at: Option<Instant>,
    cache_path: Option<PathBuf>,
    primary: bool,
    scroll: u16,
}

impl SoftwareUpdate {
    pub(super) fn schedule(&mut self, state_dir: &Path, config: &UpdateConfig) -> bool {
        if self.scheduled {
            return false;
        }
        self.scheduled = true;
        self.cache_path = Some(
            state_dir
                .parent()
                .unwrap_or(state_dir)
                .join("update-check.json"),
        );
        if !config.check_on_startup || !matches!(self.state, State::Idle) {
            return false;
        }
        if let Some(report) = self
            .cache_path
            .as_ref()
            .and_then(|path| cache::read(path, SystemTime::now(), config.interval()))
        {
            self.state = State::Report(report);
            return true;
        } else {
            self.startup_at = Some(Instant::now() + std::time::Duration::from_millis(1500));
        }
        false
    }

    pub(super) fn open(&mut self) {
        self.primary = false;
        self.scroll = 0;
        self.automatic = false;
        if matches!(self.state, State::Idle | State::Failed(_)) {
            self.check(false);
        }
    }

    fn check(&mut self, automatic: bool) {
        if matches!(
            self.state,
            State::Checking | State::Installing(_) | State::Ready(_)
        ) {
            return;
        }
        self.startup_at = None;
        self.automatic = automatic;
        self.spawn(State::Checking, installation::inspect);
    }

    fn install(&mut self) {
        let State::Report(report) = &self.state else {
            return;
        };
        if report.status != Status::UpdateAvailable {
            return;
        }
        let report = report.clone();
        self.spawn(State::Installing(report.clone()), move || {
            installation::install(&report)
        });
    }

    fn spawn(
        &mut self,
        state: State,
        run: impl FnOnce() -> anyhow::Result<Inspection> + Send + 'static,
    ) {
        if self.job.is_some() {
            return;
        }
        let (sender, receiver) = mpsc::channel();
        match std::thread::Builder::new()
            .name("igit-software-update".into())
            .spawn(move || {
                let _ = sender.send(run().map_err(|error| format!("{error:#}")));
            }) {
            Ok(_) => {
                self.state = state;
                self.job = Some(receiver);
            }
            Err(error) => {
                self.state = if self.automatic && matches!(state, State::Checking) {
                    State::Idle
                } else {
                    State::Failed(format!("Cannot start update worker: {error}"))
                }
            }
        }
        self.primary = false;
        self.scroll = 0;
    }

    pub(super) fn poll(&mut self) -> bool {
        if self.startup_at.is_some_and(|due| Instant::now() >= due) {
            self.startup_at = None;
            if matches!(self.state, State::Idle) {
                self.check(true);
                return true;
            }
        }
        let Some(job) = &self.job else { return false };
        let result = match job.try_recv() {
            Ok(result) => result,
            Err(TryRecvError::Empty) => return false,
            Err(TryRecvError::Disconnected) => Err(
                "Update worker stopped unexpectedly. Check the installation before retrying."
                    .into(),
            ),
        };
        self.job = None;
        let installing = self.installing();
        self.state = match result {
            Ok(report) if installing => {
                if let Some(path) = &self.cache_path {
                    let _ = std::fs::remove_file(path);
                }
                State::Ready(report)
            }
            Ok(report) => {
                if let Some(path) = &self.cache_path {
                    let _ = cache::write(path, &report, SystemTime::now());
                }
                State::Report(report)
            }
            Err(_) if self.automatic && !installing => State::Idle,
            Err(message) => State::Failed(message),
        };
        self.primary = false;
        self.scroll = 0;
        true
    }

    pub(super) fn installing(&self) -> bool {
        matches!(self.state, State::Installing(_))
    }

    fn actions(&self) -> Vec<Action> {
        match &self.state {
            State::Checking | State::Installing(_) => vec![Action::Close],
            State::Ready(_) => vec![Action::Restart, Action::Close],
            State::Report(report) if report.status == Status::UpdateAvailable => {
                vec![Action::Install, Action::Close]
            }
            State::Report(report) if report.status == Status::ManagerActionRequired => {
                vec![Action::Close]
            }
            _ => vec![Action::Check, Action::Close],
        }
    }
}

impl super::Shell {
    pub(super) fn open_software_update(&mut self) {
        self.cancel_key_sequences();
        self.cancel_drags();
        self.software_update.open();
        self.pop = Some(super::Popup::SoftwareUpdate);
    }
}
