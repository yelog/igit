//! Application updates have no connection to repository Pull/Fetch.
mod cache;
#[cfg(test)]
mod tests;
mod view;

use crate::{
    config::UpdateConfig,
    installation::{self, Inspection, Progress, Status},
    ui::NoticeLevel,
};
use std::{
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver, Sender, TryRecvError},
    time::{Duration, Instant, SystemTime},
};

const FADE_TICK: Duration = Duration::from_millis(16);
const FADE_STEPS: u8 = 10;
const SWEEP_TICK: Duration = Duration::from_millis(100);

#[derive(Default)]
enum State {
    #[default]
    Idle,
    Checking,
    Report(Inspection),
    Installing(Inspection, Progress),
    Ready(Inspection),
    Failed(String),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Action {
    Check,
    Install,
    Instructions,
    Restart,
    Close,
}

enum JobEvent {
    Progress(Progress),
    Finished(Result<Inspection, String>),
}

#[derive(Default)]
pub(super) struct SoftwareUpdate {
    state: State,
    job: Option<Receiver<JobEvent>>,
    automatic: bool,
    scheduled: bool,
    startup_at: Option<Instant>,
    cache_path: Option<PathBuf>,
    primary: bool,
    instructions: bool,
    scroll: u16,
    opened_at: Option<Instant>,
    started_at: Option<Instant>,
    fade: u8,
    sweep: usize,
    notice: Option<(NoticeLevel, String)>,
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
            self.report_notice(&report, true);
            self.state = State::Report(report);
            return true;
        } else {
            self.startup_at = Some(Instant::now() + Duration::from_millis(1500));
        }
        false
    }

    pub(super) fn open(&mut self) {
        self.primary = false;
        self.instructions = false;
        self.scroll = 0;
        self.automatic = false;
        self.opened_at = Some(Instant::now());
        self.fade = 0;
        if matches!(self.state, State::Idle | State::Failed(_)) {
            self.check(false);
        }
    }

    fn check(&mut self, automatic: bool) {
        if !self.can_check_again() {
            return;
        }
        self.startup_at = None;
        self.automatic = automatic;
        self.spawn(State::Checking, |_| installation::inspect());
    }

    fn install(&mut self) {
        let State::Report(report) = &self.state else {
            return;
        };
        if report.status != Status::UpdateAvailable {
            return;
        }
        let report = report.clone();
        self.spawn(
            State::Installing(report.clone(), Progress::Preparing),
            move |sender| {
                installation::install(&report, |progress| {
                    let _ = sender.send(JobEvent::Progress(progress));
                })
            },
        );
    }

    fn spawn(
        &mut self,
        state: State,
        run: impl FnOnce(&Sender<JobEvent>) -> anyhow::Result<Inspection> + Send + 'static,
    ) {
        if self.job.is_some() {
            return;
        }
        let (sender, receiver) = mpsc::channel();
        match std::thread::Builder::new()
            .name("igit-software-update".into())
            .spawn(move || {
                let result = run(&sender).map_err(|error| format!("{error:#}"));
                let _ = sender.send(JobEvent::Finished(result));
            }) {
            Ok(_) => {
                self.state = state;
                self.job = Some(receiver);
                self.started_at = Some(Instant::now());
                self.sweep = 0;
            }
            Err(error) => {
                self.state = if self.automatic && matches!(state, State::Checking) {
                    State::Idle
                } else {
                    let message = format!("Cannot start update worker: {error}");
                    self.notice = Some((NoticeLevel::Error, message.clone()));
                    State::Failed(message)
                }
            }
        }
        self.primary = false;
        self.instructions = false;
        self.scroll = 0;
    }

    pub(super) fn poll(&mut self, visible: bool) -> bool {
        if self.startup_at.is_some_and(|due| Instant::now() >= due) {
            self.startup_at = None;
            if matches!(self.state, State::Idle) {
                self.check(true);
                return true;
            }
        }
        let Some(job) = &self.job else { return false };
        let mut changed = false;
        let result = loop {
            match job.try_recv() {
                Ok(JobEvent::Progress(progress)) => {
                    if let State::Installing(_, current) = &mut self.state {
                        changed |= visible && *current != progress;
                        *current = progress;
                    }
                }
                Ok(JobEvent::Finished(result)) => break result,
                Err(TryRecvError::Empty) => return changed,
                Err(TryRecvError::Disconnected) => break Err(
                    "Update worker stopped unexpectedly. Check the installation before retrying."
                        .into(),
                ),
            }
        };
        self.job = None;
        self.started_at = None;
        let installing = self.installing();
        self.state = match result {
            Ok(report) if installing => {
                if let Some(path) = &self.cache_path {
                    let _ = std::fs::remove_file(path);
                }
                self.notice = Some((
                    NoticeLevel::Success,
                    format!(
                        "IGit {} installed. Open Update Center to restart.",
                        report.target_version
                    ),
                ));
                State::Ready(report)
            }
            Ok(report) => {
                if let Some(path) = &self.cache_path {
                    let _ = cache::write(path, &report, SystemTime::now());
                }
                self.report_notice(&report, self.automatic);
                State::Report(report)
            }
            Err(_) if self.automatic && !installing => State::Idle,
            Err(message) => {
                self.notice = Some((NoticeLevel::Error, message.clone()));
                State::Failed(message)
            }
        };
        self.primary = false;
        self.instructions = false;
        self.scroll = 0;
        true
    }

    fn report_notice(&mut self, report: &Inspection, automatic: bool) {
        self.notice = match report.status {
            Status::UpdateAvailable | Status::ManagerActionRequired => Some((
                NoticeLevel::Info,
                format!(
                    "IGit {} is available. Open Update Center for details.",
                    report.target_version
                ),
            )),
            Status::UpToDate if !automatic => Some((
                NoticeLevel::Success,
                format!("IGit {} is up to date.", report.current_version),
            )),
            _ => None,
        };
    }

    pub(super) fn take_notice(&mut self) -> Option<(NoticeLevel, String)> {
        self.notice.take()
    }

    pub(super) fn installing(&self) -> bool {
        matches!(self.state, State::Installing(..))
    }

    fn can_check_again(&self) -> bool {
        match &self.state {
            State::Idle | State::Failed(_) => true,
            State::Report(report) => report.status == Status::UpToDate,
            _ => false,
        }
    }

    fn actions(&self) -> &'static [Action] {
        match &self.state {
            State::Idle | State::Failed(_) => &[Action::Check, Action::Close],
            State::Ready(_) => &[Action::Restart, Action::Close],
            State::Report(report) if report.status == Status::UpdateAvailable => {
                &[Action::Install, Action::Close]
            }
            State::Report(report)
                if report.status == Status::ManagerActionRequired && !self.instructions =>
            {
                &[Action::Instructions, Action::Close]
            }
            _ => &[Action::Close],
        }
    }

    fn focused_button(&self) -> usize {
        if self.primary {
            0
        } else {
            self.actions().len() - 1
        }
    }

    fn sweeping(&self) -> bool {
        matches!(self.state, State::Installing(_, progress) if progress.ratio().is_none())
    }

    pub(super) fn animate(&mut self, now: Instant) -> bool {
        let fade = self.opened_at.map_or(FADE_STEPS, |opened| {
            (now.saturating_duration_since(opened).as_millis() / FADE_TICK.as_millis())
                .min(FADE_STEPS.into()) as u8
        });
        let sweep = if self.sweeping() {
            self.started_at.or(self.opened_at).map_or(0, |started| {
                (now.saturating_duration_since(started).as_millis() / SWEEP_TICK.as_millis())
                    as usize
            })
        } else {
            0
        };
        let changed = (self.fade, self.sweep) != (fade, sweep);
        (self.fade, self.sweep) = (fade, sweep);
        changed
    }

    pub(super) fn animation_timeout(&self) -> Option<Duration> {
        if self.opened_at.is_some() && self.fade < FADE_STEPS {
            Some(FADE_TICK)
        } else if self.sweeping() {
            Some(SWEEP_TICK)
        } else {
            None
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
