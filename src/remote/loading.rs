//! Optional background reads for configured remote lists.
use super::*;
use crate::core::loading::{read_repositories, Load, Scope};
use anyhow::Result;
use std::collections::BTreeMap;

#[derive(Default)]
pub(super) struct Loading {
    enabled: bool,
    scope: Option<Scope>,
    task: Load<Vec<Repository>>,
    pending: Option<Vec<Repository>>,
    error: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Selection {
    Repository(usize),
    Message(usize),
    Remote {
        root: usize,
        name: String,
    },
    Url {
        root: usize,
        name: String,
        push: bool,
        url: usize,
    },
}

impl RemotePanel {
    pub(crate) fn enable_background_loading(&mut self) {
        self.loading.enabled = true;
    }

    pub(crate) fn reads_pending(&self) -> bool {
        self.loading.task.pending() || self.loading.pending.is_some()
    }

    pub(crate) fn refresh_allowed(&self) -> bool {
        self.editor.is_none() && self.key_sequence().is_none() && !self.panes.dragging()
    }

    pub(super) fn refresh_loading(&mut self, c: &Context) -> Result<()> {
        if self.loading.enabled {
            self.request_load(c);
        } else {
            self.refresh_now(c);
        }
        Ok(())
    }

    fn request_load(&mut self, c: &Context) {
        self.clear_outdated_scope(c);
        self.queue_load(c);
    }

    fn queue_load(&mut self, c: &Context) {
        self.loading.pending = None;
        self.loading.error = None;
        self.review = None;
        if c.scope.is_empty() {
            self.loading.task.cancel();
            self.loading.scope = Some(Scope::new(c));
        } else {
            self.loading.scope.get_or_insert_with(|| Scope::new(c));
            self.loading.task.request();
        }
    }

    fn refresh_now(&mut self, c: &Context) {
        self.clear_outdated_scope(c);
        self.loading.task.cancel();
        self.loading.pending = None;
        self.loading.error = None;
        self.review = None;
        self.loading.scope = Some(Scope::new(c));
        self.install(model::load(c));
    }

    fn clear_outdated_scope(&mut self, c: &Context) -> bool {
        if self
            .loading
            .scope
            .as_ref()
            .is_some_and(|scope| scope.matches(c))
        {
            return false;
        }
        self.loading.task.cancel();
        self.loading.scope = None;
        self.loading.pending = None;
        self.loading.error = None;
        self.repos.clear();
        self.rows.clear();
        self.folded.clear();
        self.cursor = 0;
        self.goto_pending = false;
        self.top = 0;
        self.hits.clear();
        self.last_click = None;
        self.review = None;
        self.next_selection = None;
        self.panes.cancel_drag();
        true
    }

    pub(crate) fn poll_loading(&mut self, c: &Context) -> Result<bool> {
        if !self.loading.enabled {
            return Ok(false);
        }
        let mut changed = self.clear_outdated_scope(c);
        if changed {
            self.queue_load(c);
        }
        if let Some(result) = self.loading.task.take_result() {
            match result {
                Ok(repos) => {
                    self.loading.pending = Some(repos);
                    self.loading.error = None;
                }
                Err(error) => {
                    self.loading.error = Some(error.to_string());
                    return Err(error);
                }
            }
        }
        let pending = self
            .refresh_allowed()
            .then(|| self.loading.pending.take())
            .flatten();
        if let Some(repos) = pending {
            self.install(repos);
            self.loading.error = None;
            changed = true;
        }
        if self.loading.task.needs_start() {
            let jobs: Vec<_> = c
                .scope
                .iter()
                .map(|&root| (root, c.repos[root].path.clone()))
                .collect();
            if let Err(error) = self.loading.task.start("igit-remote-read", move |version| {
                read_repositories(&jobs, &version, |(root, path)| {
                    model::load_repository(*root, path)
                })
            }) {
                self.loading.error = Some(format!("Unable to load remotes: {error}"));
                return Err(error);
            }
        }
        Ok(changed)
    }

    fn install(&mut self, repos: Vec<Repository>) {
        let selection = self.selection_key();
        let destination = self.next_selection.take();
        let mut prior: BTreeMap<_, _> = std::mem::take(&mut self.repos)
            .into_iter()
            .map(|repo| (repo.root, repo))
            .collect();
        self.repos = repos
            .into_iter()
            .map(|repo| {
                if let Some(mut previous) =
                    repo.error.as_ref().and_then(|_| prior.remove(&repo.root))
                {
                    previous.error = repo.error;
                    return previous;
                }
                repo
            })
            .collect();
        self.rebuild();
        if let Some((root, name)) = destination {
            self.restore(root, name.as_deref());
        } else if let Some(row) = selection
            .as_ref()
            .and_then(|selection| self.row_for_selection(selection))
        {
            self.cursor = row;
        }
        self.review = None;
    }

    fn selection_key(&self) -> Option<Selection> {
        match *self.rows.get(self.cursor)? {
            Row::Repository(root) => Some(Selection::Repository(root)),
            Row::Message(root) => Some(Selection::Message(root)),
            Row::Remote { root, remote } => Some(Selection::Remote {
                root,
                name: self.repository(root)?.remotes.get(remote)?.name.clone(),
            }),
            Row::Url {
                root,
                remote,
                push,
                url,
            } => Some(Selection::Url {
                root,
                name: self.repository(root)?.remotes.get(remote)?.name.clone(),
                push,
                url,
            }),
        }
    }

    fn row_for_selection(&self, selection: &Selection) -> Option<usize> {
        match selection {
            Selection::Repository(root) => self
                .rows
                .iter()
                .position(|row| row == &Row::Repository(*root)),
            Selection::Message(root) => self
                .rows
                .iter()
                .position(|row| row == &Row::Message(*root))
                .or_else(|| {
                    self.rows
                        .iter()
                        .position(|row| row == &Row::Repository(*root))
                }),
            Selection::Remote { root, name } => self
                .remote_row(*root, name, None)
                .or_else(|| self.repository_row(*root)),
            Selection::Url {
                root,
                name,
                push,
                url,
            } => self
                .remote_row(*root, name, Some((*push, *url)))
                .or_else(|| self.remote_row(*root, name, None))
                .or_else(|| self.repository_row(*root)),
        }
    }

    fn repository_row(&self, root: usize) -> Option<usize> {
        self.rows
            .iter()
            .position(|row| row == &Row::Repository(root))
    }

    fn remote_row(&self, root: usize, name: &str, url: Option<(bool, usize)>) -> Option<usize> {
        let remote = self
            .repository(root)?
            .remotes
            .iter()
            .position(|remote| remote.name == name)?;
        let row = match url {
            Some((push, url)) => Row::Url {
                root,
                remote,
                push,
                url,
            },
            None => Row::Remote { root, remote },
        };
        self.rows.iter().position(|candidate| *candidate == row)
    }

    pub(super) fn write_block(
        &self,
        c: &Context,
        root: usize,
    ) -> Option<(NoticeLevel, &'static str)> {
        if self.loading.error.is_some() {
            Some((
                NoticeLevel::Error,
                "Remote refresh failed; refresh to retry",
            ))
        } else if self.reads_pending() {
            Some((
                NoticeLevel::Warning,
                "Remote list is refreshing; wait before changing it",
            ))
        } else if self
            .loading
            .scope
            .as_ref()
            .is_none_or(|scope| !scope.matches(c))
        {
            Some((
                NoticeLevel::Warning,
                "Roots changed; refresh Remote before changing remotes",
            ))
        } else if self
            .repository(root)
            .is_some_and(|repo| repo.error.is_some())
        {
            Some((
                NoticeLevel::Error,
                "Remote list unavailable; refresh to retry",
            ))
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::git;
    use crossterm::event::{KeyEvent, KeyModifiers};
    use std::{
        thread,
        time::{Duration, Instant},
    };

    fn key(panel: &mut RemotePanel, c: &Context, code: K) -> Action {
        panel.event(&Event::Key(KeyEvent::new(code, KeyModifiers::NONE)), c)
    }

    fn root(c: &Context, name: &str) -> usize {
        c.repos.iter().position(|repo| repo.name == name).unwrap()
    }

    fn finish_loading(panel: &mut RemotePanel, c: &Context) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while panel.reads_pending() {
            panel.poll_loading(c).unwrap();
            assert!(Instant::now() < deadline, "Remote reads did not finish");
            thread::sleep(Duration::from_millis(2));
        }
    }

    #[test]
    fn background_refresh_keeps_cached_rows_and_uses_latest_cursor_and_folds() {
        let (_temp, c, mut panel) = super::super::tests::fixture();
        let api = root(&c, "api");
        let web = root(&c, "web");
        git(
            &c.repos[api].path,
            &["config", "--add", "remote.origin.url", "../second.git"],
        )
        .unwrap();
        panel.refresh(&c).unwrap();
        let rows = panel.rows.clone();

        panel.enable_background_loading();
        panel.refresh(&c).unwrap();
        assert_eq!(panel.rows, rows);
        let send = panel.loading.task.hold();
        let remote = panel
            .rows
            .iter()
            .position(|row| matches!(row, Row::Remote { .. }))
            .unwrap();
        panel.cursor = remote;
        assert!(matches!(
            key(&mut panel, &c, K::Char('e')),
            Action::Notice(NoticeLevel::Warning, _)
        ));
        assert!(!panel.editing());
        panel.folded.insert(web);
        panel.rebuild();
        panel.cursor = panel
            .rows
            .iter()
            .position(|row| {
                matches!(
                    *row,
                    Row::Url {
                        root,
                        push: false,
                        url: 1,
                        ..
                    } if root == api
                )
            })
            .unwrap();
        let selected = panel.rows[panel.cursor];
        send.send(model::load(&c)).unwrap();

        assert!(panel.poll_loading(&c).unwrap());
        assert!(!panel.reads_pending());
        assert!(panel.folded.contains(&web));
        assert_eq!(panel.rows[panel.cursor], selected);
    }

    #[test]
    fn editor_and_key_sequence_block_background_refresh() {
        let (_temp, c, mut panel) = super::super::tests::fixture();
        let api = root(&c, "api");
        assert!(panel.refresh_allowed());
        key(&mut panel, &c, K::Char('g'));
        assert!(!panel.refresh_allowed());
        key(&mut panel, &c, K::Char('g'));
        assert!(panel.refresh_allowed());

        panel.restore(api, Some("origin"));
        assert!(matches!(key(&mut panel, &c, K::Enter), Action::Redraw));
        assert!(panel.editing());
        assert!(!panel.refresh_allowed());
        key(&mut panel, &c, K::Esc);
        assert!(!panel.editing());
        assert!(panel.refresh_allowed());

        assert!(matches!(
            key(&mut panel, &c, K::Char('d')),
            Action::Confirm { .. }
        ));
        panel.review = None;
        assert!(panel.refresh_allowed());
    }

    #[test]
    fn repository_failure_retains_remotes_blocks_writes_and_refresh_retries() {
        let (_temp, c, mut panel) = super::super::tests::fixture();
        let api = root(&c, "api");
        let before = panel.repository(api).unwrap().remotes.clone();

        panel.enable_background_loading();
        panel.refresh(&c).unwrap();
        let send = panel.loading.task.hold();
        let mut loaded = model::load(&c);
        let failed = loaded.iter_mut().find(|repo| repo.root == api).unwrap();
        failed.remotes.clear();
        failed.error = Some("simulated remote failure".into());
        send.send(loaded).unwrap();
        assert!(panel.poll_loading(&c).unwrap());

        let repo = panel.repository(api).unwrap();
        assert_eq!(repo.remotes, before);
        assert!(repo.error.is_some());
        assert!(panel
            .rows
            .iter()
            .any(|row| matches!(row, Row::Message(root) if *root == api)));
        panel.cursor = panel
            .rows
            .iter()
            .position(|row| matches!(row, Row::Remote { root, .. } if *root == api))
            .unwrap();
        assert!(matches!(
            key(&mut panel, &c, K::Char('e')),
            Action::Notice(NoticeLevel::Error, _)
        ));
        assert!(!panel.editing());
        assert!(matches!(
            key(&mut panel, &c, K::Char('d')),
            Action::Notice(NoticeLevel::Error, _)
        ));
        assert!(panel.review.is_none());

        panel.refresh(&c).unwrap();
        let retry = panel.loading.task.hold();
        retry.send(model::load(&c)).unwrap();
        assert!(panel.poll_loading(&c).unwrap());
        assert!(panel.repository(api).unwrap().error.is_none());
        assert!(matches!(key(&mut panel, &c, K::Char('e')), Action::Redraw));
        assert!(panel.editing());
        key(&mut panel, &c, K::Esc);
    }

    #[test]
    fn roots_change_clears_rows_and_discards_the_old_reader_error() {
        let (_temp, mut c, mut panel) = super::super::tests::fixture();
        panel.enable_background_loading();
        panel.refresh(&c).unwrap();
        let stale = panel.loading.task.hold();

        let removed = *c.scope.iter().next().unwrap();
        c.scope.remove(&removed);
        assert!(panel.poll_loading(&c).unwrap());
        assert!(panel.repos.is_empty() && panel.rows.is_empty());
        drop(stale);
        panel.poll_loading(&c).unwrap();
        assert!(panel.repos.is_empty() && panel.rows.is_empty());
        assert!(panel.loading.error.is_none());
        finish_loading(&mut panel, &c);

        assert!(panel.loading.error.is_none());
        assert!(panel.repos.iter().all(|repo| c.scope.contains(&repo.root)));
    }
}
