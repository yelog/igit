use super::*;
use crate::{
    config::FetchConfig,
    core::{
        fetch::{self as backend, Key, Observation, Outcome, Target},
        loading::{read_repositories, Load},
        process::{with_read_control, ReadControl},
    },
};
use std::{
    collections::{BTreeMap, VecDeque},
    path::Path,
    sync::mpsc::{self, Receiver, TryRecvError},
};

const STARTUP_DELAY: Duration = Duration::from_secs(2);
const STAGGER: Duration = Duration::from_millis(250);
const CONCURRENCY: usize = 2;

#[derive(Clone, Debug, PartialEq, Eq)]
struct Root {
    path: PathBuf,
    branch: String,
    push: Vec<(String, String)>,
}

#[derive(Clone, Default, PartialEq, Eq)]
struct Inputs {
    workspace: PathBuf,
    roots: Vec<Root>,
}

struct Discovery {
    inputs: Inputs,
    demanded: bool,
    roots: Vec<(PathBuf, Result<Vec<Target>, String>)>,
}

struct Item {
    target: Target,
    roots: BTreeSet<PathBuf>,
    success: Option<Instant>,
    completed: Option<Instant>,
    retry: Option<Instant>,
    failures: u32,
    authentication: bool,
    error: Option<String>,
    demanded: bool,
}

impl Item {
    fn new(target: Target) -> Self {
        Self {
            target,
            roots: BTreeSet::new(),
            success: None,
            completed: None,
            retry: None,
            failures: 0,
            authentication: false,
            error: None,
            demanded: true,
        }
    }

    fn due(&self, now: Instant, config: &FetchConfig) -> bool {
        if self.authentication || self.retry.is_some_and(|retry| retry > now) {
            return false;
        }
        let period = if self.demanded {
            config.freshness_seconds
        } else {
            config.interval_seconds
        };
        self.success
            .is_none_or(|success| now.saturating_duration_since(success).as_secs() >= period)
    }

    fn succeeded(&mut self, now: Instant) {
        self.success = Some(now);
        self.completed = Some(now);
        self.retry = None;
        self.failures = 0;
        self.authentication = false;
        self.error = None;
        self.demanded = false;
    }

    fn failed(&mut self, message: String, now: Instant) {
        self.completed = Some(now);
        self.failures = self.failures.saturating_add(1);
        self.authentication = backend::authentication_failed(&message);
        self.retry =
            Some(now + Duration::from_secs((60_u64 << (self.failures.min(6) - 1)).min(1800)));
        self.error = Some(message);
        self.demanded = false;
    }
}

struct Running {
    key: Key,
    target: Target,
    control: ReadControl,
    receiver: Receiver<Result<Outcome, String>>,
}

pub(super) enum Deferred {
    Dispatch(Event),
    Panel(Tab, Event),
    Popup(Event),
    Confirm(Tab, String),
}

#[derive(Default)]
pub(super) struct State {
    active: bool,
    focused: bool,
    startup: Option<Instant>,
    next_discovery: Option<Instant>,
    next_launch: Option<Instant>,
    requested: bool,
    demanded: bool,
    inputs: Inputs,
    discovery: Load<Discovery>,
    items: BTreeMap<Key, Item>,
    eligible: BTreeSet<Key>,
    errors: BTreeMap<PathBuf, String>,
    running: Vec<Running>,
    observations: Vec<(Observation, Instant)>,
    deferred: VecDeque<Deferred>,
    replaying: bool,
    footer: String,
}

impl Drop for State {
    fn drop(&mut self) {
        self.cancel();
    }
}

impl State {
    pub(super) fn activate(&mut self, now: Instant) {
        if !self.active {
            self.active = true;
            self.focused = true;
            self.startup = Some(now + STARTUP_DELAY);
            self.request();
        }
    }

    pub(super) fn request(&mut self) {
        self.requested = true;
        self.demanded = true;
    }

    pub(super) fn focus(&mut self, focused: bool) {
        self.focused = focused;
        if focused {
            self.request();
        } else {
            self.cancel();
        }
    }

    pub(super) fn cancel(&mut self) {
        for running in &self.running {
            running.control.cancel();
        }
    }

    pub(super) fn running(&self) -> bool {
        !self.running.is_empty()
    }

    pub(super) fn footer(&self) -> &str {
        &self.footer
    }

    pub(super) fn defer_confirmation(&mut self, owner: Tab, token: String) {
        self.deferred.push_back(Deferred::Confirm(owner, token));
    }

    fn install(&mut self, discovered: Discovery) {
        if discovered.inputs != self.inputs {
            return;
        }
        self.eligible.clear();
        self.errors.clear();
        for item in self.items.values_mut() {
            item.roots.clear();
        }
        for (path, result) in discovered.roots {
            match result {
                Ok(targets) => {
                    for target in targets {
                        let key = target.key.clone();
                        let item = self
                            .items
                            .entry(key.clone())
                            .or_insert_with(|| Item::new(target.clone()));
                        item.target = target;
                        item.roots.insert(path.clone());
                        self.eligible.insert(key);
                    }
                }
                Err(message) => {
                    self.errors.insert(path, message);
                }
            }
        }
        self.items.retain(|key, _| self.eligible.contains(key));
        for running in &self.running {
            if !self.eligible.contains(&running.key) {
                running.control.cancel();
            }
        }
        for (observation, when) in std::mem::take(&mut self.observations) {
            self.apply_observation(&observation, when);
        }
        for item in self.items.values_mut() {
            item.demanded |= discovered.demanded;
        }
    }

    pub(super) fn observe(&mut self, observation: Observation, now: Instant) {
        self.apply_observation(&observation, now);
        self.observations.retain(|(previous, _)| {
            previous.path != observation.path
                || previous.remote != observation.remote
                || previous.endpoint != observation.endpoint
                || previous.branch != observation.branch
        });
        self.observations.push((observation, now));
        self.request();
    }

    fn apply_observation(&mut self, observation: &Observation, now: Instant) {
        for item in self.items.values_mut() {
            if item.roots.contains(&observation.path)
                && item.target.key.remote == observation.remote
                && item.target.endpoint == observation.endpoint
                && item.completed.is_none_or(|completed| completed < now)
            {
                if item.authentication {
                    item.authentication = false;
                    item.retry = None;
                    item.demanded = true;
                    item.completed = Some(now);
                }
                if item.target.key.branch == observation.branch
                    && item.success.is_none_or(|success| success < now)
                {
                    item.succeeded(now);
                }
            }
        }
    }

    fn collect(&mut self, now: Instant) -> Vec<(Target, Result<Outcome, String>)> {
        let mut completed = Vec::new();
        let mut pending = Vec::new();
        for running in self.running.drain(..) {
            let mut result = match running.receiver.try_recv() {
                Ok(result) => result,
                Err(TryRecvError::Empty) => {
                    pending.push(running);
                    continue;
                }
                Err(TryRecvError::Disconnected) => {
                    Err("Fetch worker stopped; cached data retained".into())
                }
            };
            if let Some(item) = self.items.get_mut(&running.key) {
                if running.control.cancelled() {
                    item.retry = Some(now + STARTUP_DELAY);
                    item.demanded = true;
                    result = Err("Background fetch stopped; cached data retained".into());
                } else {
                    match &result {
                        Ok(_) => item.succeeded(now),
                        Err(message) => item.failed(message.clone(), now),
                    }
                }
            }
            if self
                .inputs
                .roots
                .iter()
                .any(|root| root.path == running.target.path)
                && self.eligible.contains(&running.key)
            {
                completed.push((running.target, result));
            }
        }
        self.running = pending;
        completed
    }

    fn sync(&mut self, inputs: Inputs) {
        if self.inputs == inputs {
            return;
        }
        if self.inputs.workspace != inputs.workspace {
            self.items.clear();
            self.observations.clear();
        }
        self.cancel();
        self.discovery.cancel();
        self.eligible.clear();
        self.errors.clear();
        self.observations.retain(|(observation, _)| {
            inputs
                .roots
                .iter()
                .any(|root| root.path == observation.path)
        });
        self.inputs = inputs;
        self.request();
    }

    fn discover(&mut self, config: &FetchConfig, now: Instant) {
        self.discovery.request();
        let inputs = self.inputs.clone();
        let demanded = std::mem::take(&mut self.demanded);
        let interval = Duration::from_secs(config.interval_seconds);
        let config = config.clone();
        let result = self
            .discovery
            .start("igit-fetch-discovery", move |version| {
                let roots = read_repositories(&inputs.roots, &version, |root| {
                    (
                        root.path.clone(),
                        backend::discover(
                            &root.path,
                            config.all_remotes,
                            &config.excluded_remotes,
                            &root.push,
                        )
                        .map_err(|error| format!("{error:#}")),
                    )
                });
                Discovery {
                    inputs,
                    demanded,
                    roots,
                }
            });
        if let Err(error) = result {
            for root in &self.inputs.roots {
                self.errors.insert(root.path.clone(), error.to_string());
            }
        }
        self.requested = false;
        self.next_discovery = Some(now + interval);
    }

    fn launch(&mut self, config: &FetchConfig, now: Instant) {
        for item in self.items.values_mut() {
            if item.demanded
                && item.success.is_some_and(|success| {
                    now.saturating_duration_since(success).as_secs() < config.freshness_seconds
                })
            {
                item.demanded = false;
            }
        }
        if self.running.len() >= CONCURRENCY || self.next_launch.is_some_and(|next| next > now) {
            return;
        }
        let key = self
            .eligible
            .iter()
            .filter(|key| {
                !self
                    .running
                    .iter()
                    .any(|running| running.key.common_dir == key.common_dir)
            })
            .filter(|key| {
                self.items
                    .get(*key)
                    .is_some_and(|item| item.due(now, config))
            })
            .min_by_key(|key| {
                let item = &self.items[*key];
                (key.branch.is_none(), item.success, (*key).clone())
            })
            .cloned();
        let Some(key) = key else { return };
        let target = self.items[&key].target.clone();
        let control = ReadControl::new(Duration::from_secs(config.timeout_seconds));
        let worker_target = target.clone();
        let worker_control = control.clone();
        let (send, receiver) = mpsc::channel();
        match std::thread::Builder::new()
            .name("igit-auto-fetch".into())
            .spawn(move || {
                let result = with_read_control(&worker_control, || {
                    backend::execute(&worker_target, &worker_control)
                })
                .map_err(|error| format!("{error:#}"));
                let _ = send.send(result);
            }) {
            Ok(_) => {
                self.running.push(Running {
                    key,
                    target,
                    control,
                    receiver,
                });
            }
            Err(error) => self
                .items
                .get_mut(&key)
                .unwrap()
                .failed(error.to_string(), now),
        }
        self.next_launch = Some(now + STAGGER);
    }

    fn status(&self, key: &Key, now: Instant) -> String {
        if self.running.iter().any(|running| &running.key == key) {
            return "Fetching…".into();
        }
        let item = &self.items[key];
        if item.authentication {
            return "Fetch paused · Authentication required".into();
        }
        if item.error.is_some() {
            return "Fetch failed · Cached data".into();
        }
        item.success.map_or_else(
            || "Not fetched".into(),
            |success| {
                format!(
                    "Fetched {} ago",
                    age(now.saturating_duration_since(success))
                )
            },
        )
    }

    fn root_status(&self, path: &Path, push: bool, now: Instant) -> String {
        if let Some(message) = self.errors.get(path) {
            return format!("Fetch unavailable · {message}");
        }
        let matching: Vec<_> = self
            .eligible
            .iter()
            .filter(|key| self.items[*key].roots.contains(path))
            .filter(|key| !push || key.branch.is_some())
            .collect();
        if matching.is_empty() {
            return "Not fetched".into();
        }
        matching
            .into_iter()
            .map(|key| format!("{}: {}", key.remote, self.status(key, now)))
            .collect::<Vec<_>>()
            .join(" · ")
    }

    fn summary(&self, config: &FetchConfig, now: Instant) -> String {
        if !self.active {
            return String::new();
        }
        if !config.enabled {
            return "Auto-fetch off".into();
        }
        if self.running() {
            return if self.deferred.is_empty() {
                format!("Fetching {}…", self.running.len())
            } else {
                "Stopping background fetch…".into()
            };
        }
        let failed = self
            .eligible
            .iter()
            .filter(|key| self.items[*key].error.is_some())
            .count()
            + self.errors.len();
        if failed > 0 {
            return format!("Fetch: {failed} failed · Cached data");
        }
        if self.eligible.is_empty() {
            return if self.discovery.pending() || self.requested {
                "Not fetched".into()
            } else {
                "Auto-fetch: no eligible remotes".into()
            };
        }
        if self
            .eligible
            .iter()
            .any(|key| self.items[key].success.is_none())
        {
            return "Not fetched".into();
        }
        let oldest = self
            .eligible
            .iter()
            .filter_map(|key| self.items[key].success)
            .min()
            .unwrap();
        format!("Fetched {} ago", age(now.saturating_duration_since(oldest)))
    }
}

fn age(duration: Duration) -> String {
    match duration.as_secs() {
        seconds if seconds < 60 => format!("{seconds}s"),
        seconds if seconds < 3600 => format!("{}m", seconds / 60),
        seconds => format!("{}h", seconds / 3600),
    }
}

fn browsing(event: &Event) -> bool {
    match event {
        Event::Resize(..) | Event::FocusGained | Event::FocusLost => true,
        Event::Mouse(mouse) => matches!(
            mouse.kind,
            MouseEventKind::Moved
                | MouseEventKind::ScrollUp
                | MouseEventKind::ScrollDown
                | MouseEventKind::ScrollLeft
                | MouseEventKind::ScrollRight
                | MouseEventKind::Up(_)
                | MouseEventKind::Drag(_)
        ),
        Event::Key(key) => {
            key.kind == KeyEventKind::Release
                || key.modifiers.is_empty()
                    && matches!(
                        key.code,
                        KeyCode::Up
                            | KeyCode::Down
                            | KeyCode::Left
                            | KeyCode::Right
                            | KeyCode::PageUp
                            | KeyCode::PageDown
                            | KeyCode::Home
                            | KeyCode::End
                            | KeyCode::Tab
                            | KeyCode::Char('j' | 'k' | 'h' | 'l' | 'J' | 'K' | 'g' | 'G' | ' ')
                    )
        }
        Event::Paste(_) => false,
    }
}

impl Shell {
    pub(super) fn workspace_update_running(&self) -> bool {
        let mut popup = self.pop.as_ref();
        while let Some(Popup::Help { origin, .. }) = popup {
            popup = origin.as_deref();
        }
        matches!(popup, Some(Popup::Update(dialog)) if dialog.running())
    }

    pub(super) fn defer_fetch_dispatch(&mut self, event: &Event) -> bool {
        if !self.auto_fetch.deferred.is_empty()
            && self.pop.is_none()
            && !self.panel().editing()
            && self.key_sequence().is_none()
            && matches!(event, Event::Key(key)
                if key.kind != KeyEventKind::Release
                && (key.code == KeyCode::Char('q') && key.modifiers.is_empty()
                    || key.code == KeyCode::Char('c')
                        && key.modifiers == KeyModifiers::CONTROL
                        && self.panel().selected_text().is_none()))
        {
            self.auto_fetch.cancel();
            self.request_quit();
            return true;
        }
        if !self.auto_fetch.replaying
            && !self.auto_fetch.deferred.is_empty()
            && matches!(event, Event::Key(_) | Event::Mouse(_) | Event::Paste(_))
        {
            self.auto_fetch
                .deferred
                .push_back(Deferred::Dispatch(event.clone()));
            return true;
        }
        false
    }

    pub(super) fn defer_fetch_panel(&mut self, event: &Event) -> bool {
        if !self.auto_fetch.running() || browsing(event) && !self.panel().editing() {
            return false;
        }
        self.auto_fetch.cancel();
        self.auto_fetch
            .deferred
            .push_back(Deferred::Panel(self.tab, event.clone()));
        true
    }

    pub(super) fn defer_fetch_popup(&mut self, event: &Event) -> bool {
        if !self.auto_fetch.running() {
            return false;
        }
        self.auto_fetch.cancel();
        self.auto_fetch
            .deferred
            .push_back(Deferred::Popup(event.clone()));
        true
    }

    pub(super) fn poll_auto_fetch(&mut self) -> bool {
        let now = Instant::now();
        for observation in self.push.take_fetch_observations() {
            self.auto_fetch.observe(observation, now);
        }
        let push_targets = self.push.fetch_targets(&self.ctx);
        let inputs = Inputs {
            workspace: self.ctx.workspace.clone(),
            roots: self
                .ctx
                .scope
                .iter()
                .map(|root| {
                    let repo = &self.ctx.repos[*root];
                    Root {
                        path: repo.path.clone(),
                        branch: repo.branch.clone(),
                        push: push_targets.get(&repo.path).cloned().unwrap_or_default(),
                    }
                })
                .collect(),
        };
        self.auto_fetch.sync(inputs);
        let mut changed = false;
        for (target, outcome) in self.auto_fetch.collect(now) {
            if let Ok(outcome) = outcome {
                if target.key.branch.is_some() {
                    let paths = self.auto_fetch.items[&target.key].roots.clone();
                    for path in paths {
                        self.push.remember_fetch(Observation {
                            path,
                            remote: target.key.remote.clone(),
                            endpoint: target.endpoint.clone(),
                            branch: target.key.branch.clone(),
                            tip: outcome.tip.clone(),
                        });
                    }
                    self.refresh_state.request(Push);
                }
                if outcome.changed {
                    self.refresh_state.request(Log);
                    self.refresh_state.request(Branch);
                    self.refresh_state.request(Push);
                }
            } else {
                for tab in [Log, Branch, Push] {
                    self.refresh_state.request(tab);
                }
            }
            changed = true;
        }
        if let Some(result) = self.auto_fetch.discovery.take_result() {
            match result {
                Ok(discovered) => self.auto_fetch.install(discovered),
                Err(error) => {
                    for root in &self.auto_fetch.inputs.roots {
                        self.auto_fetch
                            .errors
                            .insert(root.path.clone(), error.to_string());
                    }
                }
            }
            changed = true;
        }
        if !self.auto_fetch.running() {
            if let Some(input) = self.auto_fetch.deferred.pop_front() {
                self.auto_fetch.replaying = true;
                match input {
                    Deferred::Dispatch(event) => self.dispatch(&event),
                    Deferred::Panel(tab, event) if tab == self.tab => self.panel_event(&event),
                    Deferred::Popup(event) => self.pop_event(&event),
                    Deferred::Confirm(tab, token) => self.confirm(tab, &token),
                    Deferred::Panel(_, _) => (),
                }
                self.auto_fetch.replaying = false;
                return true;
            }
        }
        let allowed = self.auto_fetch.active
            && self.config.fetch.enabled
            && self.auto_fetch.focused
            && self.auto_fetch.deferred.is_empty()
            && !self.quit
            && self.pop.is_none()
            && !self.git_busy()
            && !self.workspace_scan.pending()
            && !self.panel().editing()
            && self.panel().selected_text().is_none()
            && !self.layout(self.tab).dragging()
            && if self.tab == Commit {
                self.commit.fetch_allowed()
            } else {
                self.refresh_allowed(self.tab)
            }
            && !self.merge.has_document()
            && self.pending_edit.is_none()
            && self.pending_keys.is_none()
            && self.key_sequence().is_none()
            && self
                .auto_fetch
                .startup
                .is_none_or(|deadline| now >= deadline);
        if !allowed {
            self.auto_fetch.cancel();
        } else {
            if !self.auto_fetch.discovery.pending()
                && (self.auto_fetch.requested
                    || self
                        .auto_fetch
                        .next_discovery
                        .is_none_or(|next| now >= next))
            {
                self.auto_fetch.discover(&self.config.fetch, now);
                changed = true;
            }
            if !self.auto_fetch.discovery.pending() && !self.auto_fetch.requested {
                let previous = self.auto_fetch.running.len();
                self.auto_fetch.launch(&self.config.fetch, now);
                changed |= previous != self.auto_fetch.running.len();
            }
        }
        self.auto_fetch.observations.retain(|(_, when)| {
            now.saturating_duration_since(*when).as_secs() <= self.config.fetch.interval_seconds
        });
        let status = self.auto_fetch.summary(&self.config.fetch, now);
        if self.auto_fetch.footer != status {
            self.auto_fetch.footer = status;
            changed = true;
        }
        let statuses = self
            .ctx
            .scope
            .iter()
            .filter(|_| self.auto_fetch.active)
            .map(|root| {
                (
                    *root,
                    if self.config.fetch.enabled {
                        self.auto_fetch
                            .root_status(&self.ctx.repos[*root].path, true, now)
                    } else {
                        "Auto-fetch off".into()
                    },
                )
            })
            .collect();
        changed |= self.push.set_fetch_status(statuses);
        changed
    }
}

#[cfg(test)]
mod tests;
