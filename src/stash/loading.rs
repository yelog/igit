//! Optional background reads for repository-grouped stash lists.
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
struct Selection {
    root: usize,
    oid: String,
    occurrence: usize,
}

impl StashPanel {
    pub(crate) fn enable_background_loading(&mut self) {
        self.loading.enabled = true;
    }

    pub(crate) fn reads_pending(&self) -> bool {
        self.loading.task.pending() || self.loading.pending.is_some()
    }

    pub(crate) fn refresh_allowed(&self) -> bool {
        !self.editing()
            && !self.panes.dragging()
            && self.selected_text().is_none()
            && self.key_sequence().is_none()
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
        self.stash_search.invalidate();
        self.folded.clear();
        self.empty_expanded = false;
        self.cursor = 0;
        self.goto_pending = false;
        self.top = 0;
        self.hits.clear();
        self.review = None;
        self.panes.cancel_drag();
        self.request_preview(Duration::ZERO);
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
            if let Err(error) = self.loading.task.start("igit-stash-read", move |version| {
                read_repositories(&jobs, &version, |(root, path)| {
                    model::load_repository(*root, path)
                })
            }) {
                self.loading.error = Some(format!("Unable to load stashes: {error}"));
                return Err(error);
            }
        }
        Ok(changed)
    }

    fn install(&mut self, repos: Vec<Repository>) {
        let selection = self.selection_key();
        let previous = self.rows.get(self.cursor).copied();
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
        self.stash_search.invalidate();
        self.rebuild();
        self.cursor = selection
            .as_ref()
            .and_then(|selection| self.row_for_selection(selection))
            .or_else(|| previous.and_then(|row| self.restore_cursor(row)))
            .or_else(|| {
                self.rows
                    .iter()
                    .position(|row| matches!(row, Row::Stash { .. }))
            })
            .unwrap_or(0);
        self.review = None;
        self.request_preview(Duration::ZERO);
    }

    fn selection_key(&self) -> Option<Selection> {
        let Row::Stash { root, entry } = *self.rows.get(self.cursor)? else {
            return None;
        };
        let repo = self.repository(root)?;
        let oid = repo.entries.get(entry)?.oid.clone();
        let occurrence = repo.entries[..entry]
            .iter()
            .filter(|candidate| candidate.oid == oid)
            .count();
        Some(Selection {
            root,
            oid,
            occurrence,
        })
    }

    fn row_for_selection(&self, selection: &Selection) -> Option<usize> {
        let entry = self
            .repository(selection.root)?
            .entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| entry.oid == selection.oid)
            .nth(selection.occurrence)?
            .0;
        self.row_index(Row::Stash {
            root: selection.root,
            entry,
        })
    }

    pub(super) fn write_block(
        &self,
        c: &Context,
        root: usize,
    ) -> Option<(NoticeLevel, &'static str)> {
        if self.loading.error.is_some() {
            Some((NoticeLevel::Error, "Stash refresh failed; refresh to retry"))
        } else if self.reads_pending() {
            Some((
                NoticeLevel::Warning,
                "Stash list is refreshing; wait before changing it",
            ))
        } else if self
            .loading
            .scope
            .as_ref()
            .is_none_or(|scope| !scope.matches(c))
        {
            Some((
                NoticeLevel::Warning,
                "Roots changed; refresh Stash before changing stashes",
            ))
        } else if self
            .repository(root)
            .is_some_and(|repo| repo.error.is_some())
        {
            Some((
                NoticeLevel::Error,
                "Stash list unavailable; refresh to retry",
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

    fn key(panel: &mut StashPanel, c: &Context, code: K) -> Action {
        panel.event(&Event::Key(KeyEvent::new(code, KeyModifiers::NONE)), c)
    }

    fn finish_preview(panel: &mut StashPanel, c: &Context) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while panel.preview_wakeup().is_some() {
            panel.poll_preview(c);
            assert!(Instant::now() < deadline, "Stash preview did not finish");
            thread::sleep(Duration::from_millis(2));
        }
        assert!(
            panel.preview.message().is_none(),
            "{:?}",
            panel.preview.message()
        );
    }

    fn finish_loading(panel: &mut StashPanel, c: &Context) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while panel.reads_pending() {
            panel.poll_loading(c).unwrap();
            assert!(Instant::now() < deadline, "Stash reads did not finish");
            thread::sleep(Duration::from_millis(2));
        }
    }

    #[test]
    fn background_refresh_keeps_cached_rows_and_latest_fold_and_cursor() {
        let (_temp, c, mut panel) = super::super::tests::fixture();
        panel.enable_background_loading();
        let root = panel.selection().unwrap().0;
        let rows = panel.rows.clone();
        panel.refresh(&c).unwrap();
        assert_eq!(panel.rows, rows);
        assert!(panel.reads_pending());
        let send = panel.loading.task.hold();
        let stash = panel
            .rows
            .iter()
            .position(|row| matches!(row, Row::Stash { .. }))
            .unwrap();
        panel.select(stash);
        assert!(matches!(
            key(&mut panel, &c, K::Char('d')),
            Action::Notice(NoticeLevel::Warning, _)
        ));
        assert!(panel.review.is_none());

        panel.folded.insert(root);
        panel.rebuild();
        panel.cursor = panel.row_index(Row::Repository(root)).unwrap();
        send.send(model::load(&c)).unwrap();

        assert!(panel.poll_loading(&c).unwrap());
        assert!(!panel.reads_pending());
        assert!(panel.folded.contains(&root));
        assert_eq!(panel.rows[panel.cursor], Row::Repository(root));
    }

    #[test]
    fn background_results_wait_for_file_diff_and_key_sequences() {
        let (_temp, c, mut panel) = super::super::tests::fixture();
        assert!(panel.refresh_allowed());
        key(&mut panel, &c, K::Char('g'));
        assert!(!panel.refresh_allowed());
        key(&mut panel, &c, K::Char('g'));
        assert!(panel.refresh_allowed());

        let row = panel
            .rows
            .iter()
            .position(|row| matches!(row, Row::Stash { .. }))
            .unwrap();
        panel.select(row);
        finish_preview(&mut panel, &c);
        key(&mut panel, &c, K::Enter);
        key(&mut panel, &c, K::Enter);
        assert!(panel.editing());
        assert!(!panel.refresh_allowed());

        panel.enable_background_loading();
        panel.refresh(&c).unwrap();
        let send = panel.loading.task.hold();
        send.send(model::load(&c)).unwrap();
        assert!(!panel.poll_loading(&c).unwrap());
        assert!(panel.reads_pending());
        assert!(panel.editing());

        key(&mut panel, &c, K::Esc);
        assert!(!panel.editing());
        assert!(panel.poll_loading(&c).unwrap());
        assert!(!panel.reads_pending());
    }

    #[test]
    fn repository_failure_retains_entries_blocks_writes_and_refresh_retries() {
        let (_temp, c, mut panel) = super::super::tests::fixture();
        let row = panel
            .rows
            .iter()
            .position(|row| matches!(row, Row::Stash { .. }))
            .unwrap();
        panel.select(row);
        let root = panel.selection().unwrap().0;
        let before = panel.repository(root).unwrap().entries.clone();

        panel.enable_background_loading();
        panel.refresh(&c).unwrap();
        let send = panel.loading.task.hold();
        let mut loaded = model::load(&c);
        let failed = loaded.iter_mut().find(|repo| repo.root == root).unwrap();
        failed.entries.clear();
        failed.error = Some("simulated stash failure".into());
        let other = loaded
            .iter_mut()
            .find(|repo| repo.root != root && !repo.entries.is_empty())
            .unwrap();
        let other_root = other.root;
        other.entries.truncate(1);
        send.send(loaded).unwrap();

        assert!(panel.poll_loading(&c).unwrap());
        let repo = panel.repository(root).unwrap();
        assert_eq!(repo.entries, before);
        assert!(repo.error.is_some());
        assert_eq!(panel.repository(other_root).unwrap().entries.len(), 1);
        assert_eq!(
            panel
                .rows
                .iter()
                .filter(|row| matches!(**row, Row::Repository(candidate) if candidate == root))
                .count(),
            1
        );
        panel.cursor = panel
            .rows
            .iter()
            .position(|row| matches!(row, Row::Stash { root: candidate, .. } if *candidate == root))
            .unwrap();
        assert!(matches!(
            key(&mut panel, &c, K::Char('d')),
            Action::Notice(NoticeLevel::Error, _)
        ));
        assert!(panel.review.is_none());

        panel.refresh(&c).unwrap();
        let retry = panel.loading.task.hold();
        retry.send(model::load(&c)).unwrap();
        assert!(panel.poll_loading(&c).unwrap());
        assert!(panel.repository(root).unwrap().error.is_none());
        assert!(matches!(
            key(&mut panel, &c, K::Char('d')),
            Action::Confirm { .. }
        ));
    }

    #[test]
    fn roots_change_clears_rows_and_discards_the_stale_reader_result() {
        let (_temp, mut c, mut panel) = super::super::tests::fixture();
        panel.enable_background_loading();
        panel.refresh(&c).unwrap();
        let stale = panel.loading.task.hold();
        let stale_result = model::load(&c);

        let removed = *c.scope.iter().next().unwrap();
        c.scope.remove(&removed);
        assert!(panel.poll_loading(&c).unwrap());
        assert!(panel.repos.is_empty() && panel.rows.is_empty());
        stale.send(stale_result).unwrap();
        panel.poll_loading(&c).unwrap();
        assert!(panel.repos.is_empty() && panel.rows.is_empty());
        finish_loading(&mut panel, &c);

        assert!(panel.loading.error.is_none());
        assert!(panel.repos.iter().all(|repo| c.scope.contains(&repo.root)));
    }

    #[test]
    fn duplicate_oids_keep_reflog_position_and_reuse_the_successful_preview() {
        let (_temp, c, mut panel) = super::super::tests::fixture();
        let (root, oid) = panel.identity().unwrap();
        finish_preview(&mut panel, &c);
        std::fs::write(c.repos[root].path.join("file.txt"), "newer saved\n").unwrap();
        git(
            &c.repos[root].path,
            &["stash", "push", "-u", "-m", "newer saved"],
        )
        .unwrap();
        git(
            &c.repos[root].path,
            &["stash", "store", "-m", "duplicate saved", &oid],
        )
        .unwrap();
        panel.refresh(&c).unwrap();
        let entries = &panel.repository(root).unwrap().entries;
        let duplicates: Vec<_> = entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| entry.oid == oid)
            .map(|(entry, _)| entry)
            .collect();
        assert!(duplicates.len() >= 2);
        panel.select(
            panel
                .row_index(Row::Stash {
                    root,
                    entry: duplicates[1],
                })
                .unwrap(),
        );
        let selector = panel.selection().unwrap().1.selector.clone();
        finish_preview(&mut panel, &c);
        let preview = panel.preview_state();
        assert!(preview.1.is_some());

        panel.enable_background_loading();
        panel.refresh(&c).unwrap();
        let send = panel.loading.task.hold();
        send.send(model::load(&c)).unwrap();
        assert!(panel.poll_loading(&c).unwrap());

        assert_eq!(panel.selection_key().unwrap().occurrence, 1);
        assert_eq!(panel.selection().unwrap().1.selector, selector);
        assert_eq!(panel.preview_state(), preview);
    }
}
