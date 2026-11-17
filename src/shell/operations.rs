use super::*;
use crate::core::process::{with_write_control, WriteControl};
use std::sync::{
    mpsc::{self, Receiver, TryRecvError},
    Arc, Mutex,
};

enum OwnedPanel {
    Rollback(Option<Box<Rollback>>),
    Commit(Box<CommitPanel>),
    Branch(Box<BranchPanel>),
    Log(Box<LogPanel>),
    Merge(Box<MergePanel>),
    Stash(Box<StashPanel>),
    Remote(Box<RemotePanel>),
}

impl OwnedPanel {
    fn confirm(&mut self, token: &str, context: &Context) -> Action {
        match self {
            Self::Rollback(dialog) => dialog.take().expect("Owned rollback review").apply(context),
            Self::Commit(panel) => panel.confirm(token, context),
            Self::Branch(panel) => panel.confirm(token, context),
            Self::Log(panel) => panel.confirm(token, context),
            Self::Merge(panel) => panel.confirm(token, context),
            Self::Stash(panel) => panel.confirm(token, context),
            Self::Remote(panel) => panel.confirm(token, context),
        }
    }

    fn restore(self, shell: &mut Shell) {
        match self {
            Self::Rollback(Some(dialog)) => shell.pop = Some(Popup::Rollback(dialog)),
            Self::Rollback(None) => (),
            Self::Commit(panel) => shell.commit = *panel,
            Self::Branch(panel) => shell.branch = *panel,
            Self::Log(panel) => shell.log = *panel,
            Self::Merge(panel) => shell.merge = *panel,
            Self::Stash(panel) => shell.stash = *panel,
            Self::Remote(panel) => shell.remote = *panel,
        }
    }
}

pub(super) struct Running {
    receiver: Receiver<(OwnedPanel, Action)>,
    control: WriteControl,
    owner: Tab,
    started: Instant,
    second: u64,
    review_interrupt: bool,
    scope: &'static str,
}

pub(super) fn scope_label(owner: Tab, token: &str) -> &'static str {
    match owner {
        Commit if token.contains("stash") => {
            "All changes in Roots · checkboxes do not limit this action"
        }
        Commit => "Reviewed files or lines · other selections stay unchanged",
        Push => "Checked repositories in Roots · reviewed targets",
        Log | Branch | Stash | Remote => "Reviewed repository only",
        Conflicts => "Reviewed file or repository operation only",
    }
}

impl Shell {
    pub(super) fn start_confirmed_operation(&mut self, owner: Tab, token: &str) -> bool {
        if !self.refresh_state.background {
            return false;
        }
        let panel = match owner {
            Commit => OwnedPanel::Commit(Box::new(std::mem::take(&mut self.commit))),
            Branch => OwnedPanel::Branch(Box::new(std::mem::take(&mut self.branch))),
            Log => OwnedPanel::Log(Box::new(std::mem::take(&mut self.log))),
            Conflicts => OwnedPanel::Merge(Box::new(std::mem::take(&mut self.merge))),
            Stash => OwnedPanel::Stash(Box::new(std::mem::take(&mut self.stash))),
            Remote => OwnedPanel::Remote(Box::new(std::mem::take(&mut self.remote))),
            _ => return false,
        };
        self.start_owned_operation(owner, token, panel);
        true
    }

    pub(super) fn start_rollback(&mut self, dialog: Box<Rollback>) {
        self.start_owned_operation(Commit, "rollback", OwnedPanel::Rollback(Some(dialog)));
    }

    fn start_owned_operation(&mut self, owner: Tab, token: &str, panel: OwnedPanel) {
        let owned = Arc::new(Mutex::new(Some(panel)));
        let worker_panel = owned.clone();
        let context = self.ctx.clone();
        let scope = scope_label(owner, token);
        let token = token.to_owned();
        let (sender, receiver) = mpsc::channel();
        let control = WriteControl::default();
        let worker_control = control.clone();
        let result = std::thread::Builder::new().name("igit-reviewed-write".into()).spawn(move || {
            let mut panel = worker_panel.lock().unwrap_or_else(|error| error.into_inner()).take().unwrap();
            let action = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                with_write_control(&worker_control, || {
                    crate::core::activity::with_directory(&context.state_dir, || panel.confirm(&token, &context))
                })
            })).unwrap_or_else(|_| Action::Refresh(
                NoticeLevel::Error,
                "Operation stopped unexpectedly. Inspect Git state and Activity before retrying.".into(),
            ));
            let _ = sender.send((panel, action));
        });
        match result {
            Ok(_) => {
                self.operation = Some(Running {
                    receiver,
                    control,
                    owner,
                    started: Instant::now(),
                    second: 0,
                    review_interrupt: false,
                    scope,
                });
                self.pop = Some(Popup::Operation);
            }
            Err(error) => {
                if let Some(panel) = owned
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .take()
                {
                    panel.restore(self);
                }
                self.notify(
                    NoticeLevel::Error,
                    owner.name(),
                    format!("Cannot start operation: {error}"),
                );
            }
        }
    }

    pub(super) fn poll_operation(&mut self) -> bool {
        let Some(operation) = &mut self.operation else {
            return false;
        };
        let result = match operation.receiver.try_recv() {
            Ok(reply) => Some(Ok(reply)),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => Some(Err(())),
        };
        if let Some(result) = result {
            let owner = operation.owner;
            self.operation = None;
            if matches!(self.pop, Some(Popup::Operation)) {
                self.pop = None;
            } else if let Some(Popup::Help { origin, .. }) = self.pop.as_mut() {
                if matches!(origin.as_deref(), Some(Popup::Operation)) {
                    *origin = None;
                }
            }
            match result {
                Ok((panel, action)) => {
                    panel.restore(self);
                    self.action_from(owner.name(), action);
                }
                Err(()) => self.notify(
                    NoticeLevel::Error,
                    owner.name(),
                    "Operation result is unknown. Inspect Git state and Activity before retrying."
                        .into(),
                ),
            }
            self.refresh();
            return true;
        }
        let second = operation.started.elapsed().as_secs();
        let changed = second != operation.second;
        operation.second = second;
        changed
    }

    pub(super) fn operation_event(&mut self, event: &Event) {
        let Some(operation) = &mut self.operation else {
            return;
        };
        let Event::Key(key) = event else { return };
        if key.kind == KeyEventKind::Release {
            return;
        }
        if operation.review_interrupt {
            match key.code {
                KeyCode::Char('y') if key.modifiers.is_empty() => {
                    operation.control.interrupt();
                    operation.review_interrupt = false;
                }
                KeyCode::Esc | KeyCode::Char('n') => operation.review_interrupt = false,
                _ => (),
            }
        } else if key.code == KeyCode::Char('x') && key.modifiers == KeyModifiers::CONTROL {
            operation.review_interrupt = true;
        }
    }

    pub(super) fn draw_operation(&self, frame: &mut Frame, area: Rect) {
        let Some(operation) = &self.operation else {
            return;
        };
        let width = area.width.saturating_sub(2).min(78);
        let height = area.height.saturating_sub(2).min(11);
        let popup = Rect::new(
            area.x + (area.width - width) / 2,
            area.y + (area.height - height) / 2,
            width,
            height,
        );
        frame.render_widget(Clear, popup);
        let block = pane(
            format!(
                "{} operation · {}s",
                operation.owner.name(),
                operation.started.elapsed().as_secs()
            ),
            true,
        );
        let body = block.inner(popup);
        frame.render_widget(block, popup);
        let text = if operation.review_interrupt {
            "Interrupt the active Git process?\nGit may already have changed HEAD, the index, or working files.\nThe result can be unknown. Inspect Activity and Conflicts before retrying.\n\nn / Esc Keep running · y Interrupt".into()
        } else {
            let phase = operation.control.phase();
            format!("{}\n\nScope: {}\n\nWorking in the background. Slow hooks may take time.\nCtrl-X Review interruption", if phase.is_empty() { "Revalidating reviewed state…" } else { &phase },
                operation.scope)
        };
        frame.render_widget(Paragraph::new(text).wrap(Wrap { trim: false }), body);
    }

    pub(super) fn draw_operation_backdrop(&self, frame: &mut Frame) {
        let area = frame.area();
        if let Some(backdrop) = self
            .backdrop
            .as_ref()
            .filter(|backdrop| backdrop.area == area)
        {
            *frame.buffer_mut() = backdrop.clone();
        } else {
            frame.render_widget(Block::default().style(Style::new().bg(BG)), area);
        }
        frame.buffer_mut().set_style(
            area,
            Style::new().add_modifier(ratatui::style::Modifier::DIM),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::git;
    use crate::shell::test_support::{fixture, screen, snapshot};

    fn send(app: &mut Shell, sequence: &str) {
        for key in crate::config::keys::parse_sequence(sequence).unwrap() {
            app.dispatch(&Event::Key(key));
        }
    }

    #[test]
    fn confirmed_worker_restores_panel_and_help_and_revalidates_roots() {
        for stale in [false, true] {
            let (_directory, mut app) = fixture();
            let repo = app.ctx.repos[0].path.clone();
            let before = snapshot(&repo);
            send(&mut app, "6 a Ctrl-u");
            app.dispatch(&Event::Paste("origin".into()));
            send(&mut app, "Tab Ctrl-u");
            app.dispatch(&Event::Paste("../reviewed.git".into()));
            send(&mut app, "Ctrl-s");
            assert!(matches!(
                app.pop,
                Some(Popup::Confirm { owner: Remote, .. })
            ));
            if stale {
                app.ctx.scope.clear();
            }
            app.enable_background_loading();
            send(&mut app, "y");
            assert!(app.operation.is_some());
            assert!(app.git_busy());
            app.request_quit();
            assert!(!app.quit);
            send(&mut app, "F1");
            let deadline = Instant::now() + Duration::from_secs(10);
            while app.operation.is_some() {
                app.poll_operation();
                assert!(Instant::now() < deadline, "Confirmed worker did not finish");
                std::thread::sleep(Duration::from_millis(5));
            }
            assert!(matches!(app.pop, Some(Popup::Help { origin: None, .. })));
            send(&mut app, "Esc");
            assert!(app.pop.is_none());
            if stale {
                assert!(git(&repo, &["remote"]).unwrap().is_empty());
            } else {
                assert_eq!(
                    git(&repo, &["remote", "get-url", "origin"]).unwrap(),
                    "../reviewed.git"
                );
                assert!(!app.remote.editing());
                let records = crate::core::activity::load(&app.ctx.state_dir)
                    .unwrap()
                    .records;
                assert!(
                    records
                        .iter()
                        .any(|record| record.operation == "remote add"
                            && record.status == "Succeeded")
                );
            }
            assert_eq!(snapshot(&repo), before);
        }
    }

    #[test]
    fn operation_dialog_keeps_the_previous_panel_frame_visible() {
        let (_directory, mut app) = fixture();
        send(&mut app, "4");
        let before = screen(&mut app, 120, 36);
        assert!(before.contains("IGit"), "{before}");
        assert!(before.contains("Branch"), "{before}");

        let _taken = std::mem::take(&mut app.branch);
        let (_sender, receiver) = mpsc::channel();
        app.operation = Some(Running {
            receiver,
            control: WriteControl::default(),
            owner: Branch,
            started: Instant::now(),
            second: 0,
            review_interrupt: false,
            scope: "Reviewed repository only",
        });
        app.pop = Some(Popup::Operation);

        let during = screen(&mut app, 120, 36);
        assert!(during.contains("IGit"), "{during}");
        assert!(during.contains("Branch"), "{during}");
        assert!(during.contains("Branch operation"), "{during}");
        assert!(during.contains("Revalidating reviewed state…"), "{during}");
    }
}
