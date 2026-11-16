use super::*;

#[derive(Default)]
pub(super) struct State {
    loading: Option<startup::WorkspaceLoad>,
    ready: Option<Box<Shell>>,
    started: Option<Instant>,
    painted_second: u64,
}

impl State {
    pub(super) fn pending(&self) -> bool {
        self.loading.is_some() || self.ready.is_some()
    }

    pub(super) fn status(&self, merge_open: bool) -> String {
        let elapsed = self.started.map_or(0, |time| time.elapsed().as_secs());
        if self.ready.is_some() && merge_open {
            return "Ctrl-X cancel scan · Save/close the conflict document".into();
        }
        let stage = self
            .loading
            .as_ref()
            .map_or("Waiting to switch workspace", |loading| loading.stage);
        format!("Ctrl-X cancel scan · {elapsed}s · {stage}")
    }
}

pub(super) fn preserve_scope(previous: &Context, next: &mut Context) {
    if previous.workspace != next.workspace {
        return;
    }
    let included: BTreeSet<_> = previous
        .scope
        .iter()
        .filter_map(|&root| previous.repos.get(root))
        .map(|repo| &repo.path)
        .collect();
    next.scope = next
        .repos
        .iter()
        .enumerate()
        .filter_map(|(root, repo)| included.contains(&repo.path).then_some(root))
        .collect();
    next.state_dir = previous.state_dir.clone();
}

impl Shell {
    pub(super) fn start_workspace_scan(&mut self, path: PathBuf) -> Result<()> {
        self.auto_fetch.cancel();
        self.workspace_scan = State::default();
        let launch = cli::Launch {
            workspace: path,
            tab: Commit,
            file_history: None,
        };
        let loading = startup::WorkspaceLoad::start_preserving(
            launch,
            self.config.clone(),
            Some(self.ctx.clone()),
        )?;
        self.workspace_scan = State {
            loading: Some(loading),
            ready: None,
            started: Some(Instant::now()),
            painted_second: 0,
        };
        self.pop = None;
        self.notify(
            NoticeLevel::Info, "Workspace",
            "Scanning workspace in the background. Ctrl-X cancels; the current workspace stays open.".into(),
        );
        Ok(())
    }

    pub(super) fn poll_workspace_scan(&mut self) -> bool {
        let mut changed = false;
        if self.workspace_scan.pending() {
            let elapsed = self
                .workspace_scan
                .started
                .map_or(0, |time| time.elapsed().as_secs());
            if elapsed != self.workspace_scan.painted_second {
                self.workspace_scan.painted_second = elapsed;
                changed = true;
            }
        }
        if let Some(loading) = &mut self.workspace_scan.loading {
            let previous = (loading.stage, loading.roots);
            match loading.poll() {
                Ok(Some(next)) => {
                    self.workspace_scan.ready = Some(Box::new(next));
                    self.workspace_scan.loading = None;
                    changed = true;
                }
                Ok(None) => changed |= previous != (loading.stage, loading.roots),
                Err(error) => {
                    let elapsed = self
                        .workspace_scan
                        .started
                        .map_or(0, |time| time.elapsed().as_secs());
                    self.workspace_scan = State::default();
                    self.notify(NoticeLevel::Error, "Workspace",
                        format!("Workspace scan failed after {elapsed}s; current workspace retained: {error:#}"));
                    return true;
                }
            }
        }
        if self.workspace_scan.ready.is_none()
            || self.git_busy()
            || self.pop.is_some()
            || self.panel().editing()
            || self.merge.has_document()
            || self.commit.ai_running()
            || self.pending_edit.is_some()
            || self.pending_keys.is_some()
            || self.window_prefix.is_some()
            || self.path_copy_pending
        {
            return changed;
        }
        let mut next = *self.workspace_scan.ready.take().unwrap();
        let same = next.ctx.workspace == self.ctx.workspace;
        if same {
            next.auto_fetch = std::mem::take(&mut self.auto_fetch);
            preserve_scope(&self.ctx, &mut next.ctx);
            next.commit
                .inherit_workspace(&self.commit, &self.ctx, &next.ctx);
            let same_repositories = self.ctx.repos.iter().map(|repo| &repo.path).eq(next
                .ctx
                .repos
                .iter()
                .map(|repo| &repo.path));
            if same_repositories {
                next.log = std::mem::take(&mut self.log);
                next.log_load = std::mem::replace(&mut self.log_load, startup::LogLoad::Dormant);
                next.branch = std::mem::take(&mut self.branch);
                next.push = std::mem::take(&mut self.push);
                next.merge = std::mem::take(&mut self.merge);
                next.stash = std::mem::take(&mut self.stash);
                next.remote = std::mem::take(&mut self.remote);
                next.commit.panes = std::mem::take(&mut self.commit.panes);
            } else {
                for tab in Tab::DISPLAY_ORDER {
                    *next.layout_mut(tab) = std::mem::take(self.layout_mut(tab));
                }
            }
        }
        next.kitty = std::mem::take(&mut self.kitty);
        next.software_update = std::mem::take(&mut self.software_update);
        let notices = next
            .notifications
            .history()
            .rev()
            .cloned()
            .collect::<Vec<_>>();
        next.notifications = std::mem::take(&mut self.notifications);
        for notice in notices {
            next.notifications
                .push(notice.level, notice.source, notice.body);
        }
        if same {
            next.go(self.tab);
        }
        *self = next;
        self.refresh();
        self.notify(
            NoticeLevel::Success, "Workspace",
            if same {
                "Workspace rescanned; Roots and Commit selections retained. Include new repositories in Roots."
            } else {
                "Workspace opened"
            }.into(),
        );
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::git;

    fn wait(app: &mut Shell) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while app.workspace_scan.pending() {
            app.poll_workspace_scan();
            assert!(Instant::now() < deadline, "Workspace scan timed out");
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn same_workspace_rescan_retains_roots_and_excludes_new_repositories() {
        let directory = tempfile::tempdir().unwrap();
        for name in ["shown", "hidden"] {
            std::fs::create_dir(directory.path().join(name)).unwrap();
            git(&directory.path().join(name), &["init", "-q"]).unwrap();
        }
        let mut context = Context::discover(directory.path()).unwrap();
        context.state_dir = directory.path().join("state");
        context.scope = context
            .repos
            .iter()
            .enumerate()
            .filter_map(|(root, repo)| repo.path.ends_with("shown").then_some(root))
            .collect();
        let mut app = Shell::new(context, IconMode::Ascii);
        std::fs::create_dir(directory.path().join("new")).unwrap();
        git(&directory.path().join("new"), &["init", "-q"]).unwrap();
        app.start_workspace_scan(directory.path().into()).unwrap();
        assert_eq!(app.ctx.repos.len(), 2);
        wait(&mut app);
        assert_eq!(app.ctx.repos.len(), 3);
        assert_eq!(app.ctx.scope.len(), 1);
        assert!(app.ctx.repos[*app.ctx.scope.first().unwrap()]
            .path
            .ends_with("shown"));
        assert_eq!(app.icons, IconMode::Ascii);
    }

    #[test]
    fn failed_or_cancelled_scan_keeps_current_workspace_and_remains_usable() {
        let directory = tempfile::tempdir().unwrap();
        let mut context = Context::discover(directory.path()).unwrap();
        context.state_dir = directory.path().join("state");
        let mut app = Shell::new(context, IconMode::Ascii);
        let current = app.ctx.workspace.clone();
        app.start_workspace_scan(directory.path().join("missing"))
            .unwrap();
        wait(&mut app);
        assert_eq!(app.ctx.workspace, current);
        app.start_workspace_scan(directory.path().into()).unwrap();
        app.dispatch(&Event::Key(KeyEvent::new(
            KeyCode::Char('x'),
            KeyModifiers::CONTROL,
        )));
        assert!(!app.workspace_scan.pending());
        assert_eq!(app.ctx.workspace, current);
        app.request_quit();
        assert!(app.quit);
    }

    #[test]
    fn failed_repository_read_does_not_replace_the_current_workspace() {
        let directory = tempfile::tempdir().unwrap();
        let repo = directory.path().join("repo");
        std::fs::create_dir(&repo).unwrap();
        git(&repo, &["init", "-q", "-b", "main"]).unwrap();
        std::fs::write(repo.join("file"), "keep working content\n").unwrap();
        git(&repo, &["add", "file"]).unwrap();
        let mut context = Context::discover(directory.path()).unwrap();
        context.state_dir = directory.path().join("state");
        let mut app = Shell::new(context, IconMode::Ascii);
        app.pane_maximized = true;
        std::fs::write(repo.join(".git/index"), "broken index").unwrap();
        app.start_workspace_scan(directory.path().into()).unwrap();
        wait(&mut app);
        assert!(app.pane_maximized);
        assert!(app
            .notifications
            .history()
            .any(|notice| notice.level == NoticeLevel::Error
                && notice.body.contains("current workspace retained")));
        assert_eq!(
            std::fs::read(repo.join(".git/index")).unwrap(),
            b"broken index"
        );
        assert_eq!(
            std::fs::read(repo.join("file")).unwrap(),
            b"keep working content\n"
        );
    }

    #[test]
    fn rescan_waits_for_the_open_conflict_document_to_be_saved() {
        let directory = tempfile::tempdir().unwrap();
        let repo = directory.path().join("repo");
        std::fs::create_dir(&repo).unwrap();
        git(&repo, &["init", "-q", "-b", "main"]).unwrap();
        for (key, value) in [
            ("user.name", "Rescan test"),
            ("user.email", "rescan@example.invalid"),
            ("commit.gpgsign", "false"),
            ("core.hooksPath", "/dev/null"),
        ] {
            git(&repo, &["config", key, value]).unwrap();
        }
        std::fs::write(repo.join("file"), "base\n").unwrap();
        git(&repo, &["add", "file"]).unwrap();
        git(&repo, &["commit", "-qm", "base"]).unwrap();
        git(&repo, &["checkout", "-qb", "other"]).unwrap();
        std::fs::write(repo.join("file"), "other\n").unwrap();
        git(&repo, &["commit", "-qam", "other"]).unwrap();
        git(&repo, &["checkout", "-q", "main"]).unwrap();
        std::fs::write(repo.join("file"), "main\n").unwrap();
        git(&repo, &["commit", "-qam", "main"]).unwrap();
        assert!(git(&repo, &["merge", "--no-edit", "other"]).is_err());
        let working = std::fs::read(repo.join("file")).unwrap();
        let index = git(&repo, &["ls-files", "--stage"]).unwrap();
        let mut context = Context::discover(directory.path()).unwrap();
        context.state_dir = directory.path().join("state");
        let mut app = Shell::new(context, IconMode::Ascii);
        app.go(Conflicts);
        app.merge.event(
            &Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            &app.ctx,
        );
        assert!(app.merge.has_document());
        app.merge.event(
            &Event::Key(KeyEvent::new(KeyCode::Char('H'), KeyModifiers::NONE)),
            &app.ctx,
        );
        let added = directory.path().join("aaa");
        std::fs::create_dir(&added).unwrap();
        git(&added, &["init", "-q"]).unwrap();
        app.start_workspace_scan(directory.path().into()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        while app.workspace_scan.loading.is_some() {
            app.poll_workspace_scan();
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(app.workspace_scan.ready.is_some());
        assert!(app.merge.has_document());
        assert_eq!(app.ctx.repos.len(), 1);
        assert!(matches!(
            app.merge.event(
                &Event::Key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE)),
                &app.ctx
            ),
            Action::Notice(NoticeLevel::Success, _)
        ));
        wait(&mut app);
        assert_eq!(app.ctx.repos.len(), 2);
        assert_eq!(std::fs::read(repo.join("file")).unwrap(), working);
        assert_eq!(git(&repo, &["ls-files", "--stage"]).unwrap(), index);
    }
}
