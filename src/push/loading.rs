//! Background reads for outgoing commits and their previews.
use super::*;
use crate::core::loading::read_repositories;

impl PushPanel {
    pub(crate) fn reads_pending(&self) -> bool {
        self.load.pending() || self.preview_load.pending()
    }

    pub(super) fn read_status(&self) -> Option<&str> {
        if self.load.loading() {
            Some(if self.entries.is_empty() {
                "Loading outgoing commits…"
            } else {
                "Refreshing outgoing commits…"
            })
        } else {
            self.load_error.as_deref()
        }
    }

    pub(super) fn selected_identity(&self) -> Option<(usize, Option<String>)> {
        let (root, commit) = self.selected()?;
        let e = self.entries.iter().find(|e| e.idx == root)?;
        Some((
            root,
            commit.and_then(|i| e.logs.get(i)).map(|h| h.id.clone()),
        ))
    }

    pub(super) fn position_for_identity(
        &self,
        (root, commit): &(usize, Option<String>),
    ) -> Option<usize> {
        let entry = self.entries.iter().find(|e| e.idx == *root)?;
        let commit = commit
            .as_ref()
            .and_then(|id| entry.logs.iter().position(|h| &h.id == id));
        let rows = self.rows();
        rows.iter()
            .position(|row| *row == (*root, commit))
            .or_else(|| rows.iter().position(|row| *row == (*root, None)))
    }

    fn install_entries(&mut self, entries: Vec<Entry>, c: &Context) {
        let old = self.selected_identity();
        let previous = old
            .as_ref()
            .and_then(|(i, _)| self.entries.iter().find(|e| e.idx == *i))
            .cloned();
        let reseed = self.reseed.take();
        self.entries = entries;
        self.tracking_choices.retain(|idx, choice| {
            !c.scope.contains(idx)
                || self
                    .entries
                    .iter()
                    .any(|e| e.idx == *idx && e.source == choice.branch && e.t == choice.target)
        });
        if let Some(roots) = &reseed {
            self.checked = roots.iter().copied().collect();
            self.expanded = self.checked.clone();
            self.init = true;
        }
        if !self.init {
            if let Some(e) = self.entries.iter().find(|e| e.n > 0) {
                self.checked.insert(e.idx);
                self.expanded.insert(e.idx);
            }
            self.init = true;
        }
        self.entries.sort_by(|a, b| {
            c.repos[a.idx]
                .name
                .to_lowercase()
                .cmp(&c.repos[b.idx].name.to_lowercase())
                .then(a.idx.cmp(&b.idx))
        });
        let rows = self.rows();
        self.pos = old
            .as_ref()
            .filter(|_| reseed.is_none())
            .and_then(|identity| self.position_for_identity(identity))
            .or_else(|| {
                rows.iter()
                    .position(|(i, k)| k.is_none() && self.checked.contains(i))
            })
            .unwrap_or(0);
        let current = self.selected_identity();
        let selected = current
            .as_ref()
            .and_then(|(i, _)| self.entries.iter().find(|e| e.idx == *i));
        // Identical snapshots keep the current file, folds, scroll and text selection.
        if reseed.is_some() || old != current || previous.as_ref() != selected {
            self.inspect(c);
        }
    }

    pub(crate) fn poll_loading(&mut self, c: &Context) -> Result<bool> {
        let mut changed = false;
        if self
            .load_scope
            .as_ref()
            .is_some_and(|scope| !scope.matches(c))
        {
            self.refresh(c)?;
            changed = true;
        }
        if !self.busy() {
            if let Some(result) = self.load.take_result() {
                match result {
                    Ok(entries) => self.install_entries(entries, c),
                    Err(error) => {
                        self.load_error = Some(error.to_string());
                        return Err(error);
                    }
                }
                changed = true;
            }
            if self.load.needs_start() {
                let jobs: Vec<_> = c
                    .scope
                    .iter()
                    .map(|&i| (i, c.repos[i].clone(), self.targets.get(&i).cloned()))
                    .collect();
                if let Err(error) = self.load.start("igit-push-read", move |version| {
                    read_repositories(&jobs, &version, |(i, repo, target)| {
                        entry(*i, repo, target.as_ref())
                    })
                }) {
                    self.load_error = Some(format!("Unable to load outgoing commits: {error}"));
                    return Err(error);
                }
            }
        }
        if let Some(result) = self.preview_load.take_result() {
            self.preview_request = None;
            match result {
                Ok(preview::Loaded::Files { files, summary }) => {
                    self.preview = Preview {
                        tree: FileTree::new(&files, self.grouping),
                        files,
                        body: summary.clone(),
                        summary,
                        is_patch: false,
                    };
                    if self.show_details {
                        self.inspect_file(c);
                    }
                }
                Ok(preview::Loaded::Patch { body, is_patch }) => {
                    self.preview.body = body;
                    self.preview.is_patch = is_patch;
                }
                Err(error) => {
                    self.preview.body = error.to_string();
                    return Err(error);
                }
            }
            changed = true;
        }
        if self.preview_load.needs_start() {
            if let Some(request) = self.preview_request.clone() {
                if let Err(error) = self
                    .preview_load
                    .start("igit-push-read", move |_| preview::read(request))
                {
                    self.preview_request = None;
                    self.preview.body = format!("Unable to load preview: {error}");
                    return Err(error);
                }
            }
        }
        Ok(changed)
    }

    #[cfg(test)]
    pub(crate) fn finish_loading(&mut self, c: &Context) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
        while self.reads_pending() {
            self.poll_loading(c).unwrap();
            assert!(
                std::time::Instant::now() < deadline,
                "Push reads did not finish"
            );
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    }
}

#[cfg(test)]
mod tests;
