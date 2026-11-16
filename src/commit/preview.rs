//! Commit candidates share file history's read-only revision comparison.
use super::*;
use crate::core::loading::Load;
use crate::merge::{RevisionDiff, RevisionEvent};
use crate::ui::search::Prompt;
use std::path::PathBuf;

#[cfg(test)]
mod tests;

const NAV_SETTLE: Duration = Duration::from_millis(60);

#[derive(Default)]
pub(super) struct State {
    background: bool,
    target: Option<Target>,
    due: Option<Instant>,
    task: Load<Option<Prepared>>,
}

#[derive(Clone, PartialEq, Eq)]
struct Target {
    key: String,
    root: usize,
    repo: PathBuf,
    change: Change,
}

pub(super) struct Prepared {
    loaded: Loaded,
    comparison: Option<Box<RevisionDiff>>,
}

impl Prepared {
    pub(super) fn new(name: String, loaded: Loaded) -> Self {
        let comparison = loaded.pair.as_ref().map(|pair| {
            Box::new(RevisionDiff::two_way(
                name,
                pair.revisions.clone(),
                pair.sources.each_ref().map(|source| source.text.clone()),
                pair.sources
                    .each_ref()
                    .map(|source| source.description.clone()),
            ))
        });
        Self { loaded, comparison }
    }
}

impl State {
    fn cancel(&mut self) {
        self.task.cancel();
        self.target = None;
        self.due = None;
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Loaded {
    pub patch: String,
    pub pair: Option<backend::preview::Comparison>,
}

impl Loaded {
    pub(super) fn read(repo: &Path, change: &Change) -> Self {
        if change.ignored() || change.conflict {
            return Self {
                patch: backend::diff(repo, change).unwrap_or_else(|e| e.to_string()),
                pair: None,
            };
        }
        match backend::preview::load(repo, change) {
            Ok(pair) => Self {
                patch: String::new(),
                pair: Some(pair),
            },
            Err(error) => Self {
                patch: format!(
                    "{error}\n\n{}",
                    backend::diff(repo, change).unwrap_or_else(|e| e.to_string())
                ),
                pair: None,
            },
        }
    }
}

impl CommitPanel {
    pub(crate) fn enable_background_previews(&mut self) {
        self.preview.background = true;
    }

    pub(crate) fn cancel_preview_read(&mut self) {
        if self.preview.target.is_some() {
            self.preview.cancel();
        }
        let _ = self.preview.task.take_result();
    }

    pub(crate) fn preview_wakeup(&self) -> Option<Duration> {
        if self.preview.target.is_some() && self.preview.task.pending() {
            Some(
                self.preview
                    .due
                    .filter(|_| self.preview.task.needs_start())
                    .map_or(Duration::from_millis(16), |due| {
                        due.saturating_duration_since(Instant::now())
                    }),
            )
        } else {
            None
        }
    }

    fn preview_target(&self, c: &Context) -> Option<Target> {
        let row = self.active()?;
        let file = &self.files[row.file()?];
        c.scope.contains(&file.root).then(|| Target {
            key: row.id.clone(),
            root: file.root,
            repo: c.repos[file.root].path.clone(),
            change: file.change.clone(),
        })
    }

    pub(super) fn ensure_preview(&mut self, c: &Context) {
        if !self.preview_open {
            self.cancel_preview_read();
            return;
        }
        let key = self.active().map_or("", |row| row.id.as_str());
        if key == self.diff_key {
            self.cancel_preview_read();
            return;
        }
        let key = key.to_owned();
        if let Some(target) = self.preview_target(c) {
            if self.preview.background {
                if self.preview.target.as_ref() != Some(&target) {
                    self.invalidate_preview();
                    self.preview.task.request();
                    self.preview.target = Some(target);
                    self.preview.due = Some(Instant::now() + NAV_SETTLE);
                }
                return;
            }
            let loaded = Loaded::read(&target.repo, &target.change);
            self.set_prepared_preview(key, Prepared::new(target.change.label(), loaded));
        } else {
            self.set_prepared_preview(
                key,
                Prepared::new(
                    String::new(),
                    Loaded {
                        patch: String::new(),
                        pair: None,
                    },
                ),
            );
        }
    }

    pub(crate) fn poll_preview(&mut self, c: &Context) -> Result<bool> {
        if !self.preview.background || self.diff.is_empty() {
            self.cancel_preview_read();
            let _ = self.preview.task.take_result();
            return Ok(false);
        }
        self.ensure_preview(c);
        if let Some(result) = self.preview.task.take_result() {
            match result {
                Ok(Some(prepared)) => {
                    if let Some(target) = self.preview.target.take() {
                        return Ok(self.set_prepared_preview(target.key, prepared));
                    }
                }
                Err(error) => {
                    self.fail_preview(&error);
                    return Err(error);
                }
                Ok(None) => {}
            }
        }
        if self.preview.due.is_some_and(|due| due <= Instant::now())
            && self.preview.task.needs_start()
        {
            let target = self.preview.target.as_ref().unwrap().clone();
            let started = self.preview.task.start("commit-preview", move |version| {
                let loaded = Loaded::read(&target.repo, &target.change);
                (!version.cancelled()).then(|| Prepared::new(target.change.label(), loaded))
            });
            self.preview.due = None;
            if let Err(error) = started {
                self.fail_preview(&error);
                return Err(error);
            }
        }
        Ok(false)
    }

    fn fail_preview(&mut self, error: &anyhow::Error) {
        if let Some(target) = self.preview.target.take() {
            self.set_prepared_preview(
                target.key,
                Prepared::new(
                    target.change.label(),
                    Loaded {
                        patch: format!("Could not load diff: {error:#}"),
                        pair: None,
                    },
                ),
            );
        }
    }

    pub(super) fn preview_root(&self) -> Option<usize> {
        self.preview
            .target
            .as_ref()
            .map(|target| target.root)
            .or(self.diff_root)
    }

    pub(super) fn set_prepared_preview(&mut self, key: String, prepared: Prepared) -> bool {
        let Prepared {
            loaded,
            mut comparison,
        } = prepared;
        self.preview.cancel();
        let root = self
            .active()
            .and_then(Row::file)
            .map(|i| self.files[i].root);
        if self.diff_key == key
            && self.diff_text == loaded.patch
            && self.diff_sources == loaded.pair
        {
            self.diff_root = root;
            return false;
        }
        if let Some(view) = &self.diff_comparison {
            self.diff_search.inherit_query(view.search_prompt());
        }
        self.diff_selection.reset();
        self.invalidate_diff_search();
        if self.diff_key != key {
            self.diff_scroll = 0;
            self.diff_left = 0;
        }
        self.diff_key = key;
        self.diff_root = root;
        self.diff_text = loaded.patch;
        if let Some(view) = &mut comparison {
            if let Some(previous) = &self.diff_comparison {
                view.inherit_search(previous);
            } else {
                view.inherit_query(self.diff_search.prompt());
            }
        }
        self.diff_comparison = comparison;
        self.diff_sources = loaded.pair;
        true
    }

    pub(super) fn comparison_visible(&self) -> bool {
        self.file_preview_visible()
            && self.active().is_some_and(|row| row.id == self.diff_key)
            && self.diff_comparison.is_some()
    }

    fn comparison_pending(&self) -> bool {
        self.file_preview_visible() && self.active().is_some_and(|row| row.id != self.diff_key)
    }

    pub(crate) fn file_comparison_focused(&self) -> bool {
        self.focus == Focus::Diff
            && (self.comparison_visible() || self.comparison_pending())
            && !self.editing()
    }

    pub(super) fn comparison_search(&self) -> Option<&Prompt> {
        self.comparison_visible()
            .then(|| self.diff_comparison.as_ref().unwrap().search_prompt())
    }

    pub(super) fn clear_preview_selection(&mut self) {
        self.diff_selection.reset();
        if let Some(view) = &mut self.diff_comparison {
            view.clear_selection();
        }
    }

    pub(super) fn invalidate_preview(&mut self) {
        self.preview.cancel();
        if let Some(view) = &self.diff_comparison {
            self.diff_search.inherit_query(view.search_prompt());
        }
        self.diff_key.clear();
        self.diff_root = None;
        self.diff_sources = None;
        self.diff_comparison = None;
        self.clear_preview_selection();
        self.invalidate_diff_search();
    }

    /// Revision controls belong to the preview's current input context.
    pub(crate) fn inline_diff_key(&self, event: &Event) -> bool {
        !self.editing() && self.comparison_key(event)
    }

    pub(super) fn comparison_key(&self, event: &Event) -> bool {
        self.focus == Focus::Diff
            && (self.comparison_visible() || self.comparison_pending())
            && matches!(event, Event::Key(k) if
                ((k.modifiers - Mod::SHIFT).is_empty()
                    && matches!(k.code,
                        K::Char('n' | 'p' | 'f' | 'z' | 'j' | 'k' | 'h' | 'l' | 'y' | '[' | ']' | ' ')
                        | K::Enter | K::Esc | K::Up | K::Down | K::Left | K::Right
                        | K::PageUp | K::PageDown | K::Home | K::End))
                || (k.modifiers == Mod::CONTROL
                    && (matches!(k.code, K::Char('a' | 'f' | 'b' | 'd' | 'u') | K::Insert)
                        || k.code == K::Char('c') && self.selected_text().is_some())))
    }

    pub(super) fn comparison_event(&mut self, event: &Event, c: &Context) -> Option<Action> {
        if !self.comparison_visible() {
            return self.comparison_key(event).then_some(Action::None);
        }
        if self.dragged.is_some()
            && matches!(event, Event::Mouse(m) if matches!(m.kind, M::Drag(_) | M::Up(_)))
        {
            return None;
        }
        if matches!(event, Event::FocusLost) {
            self.diff_comparison.as_mut().unwrap().event(event);
            return None;
        }
        let scroll = preview_scroll_key(event);
        let key = self.comparison_key(event);
        let view = self.diff_comparison.as_mut().unwrap();
        let mouse = if let Event::Mouse(m) = event {
            inside(self.diff, m.column, m.row) || view.mouse_active()
        } else {
            false
        };
        if !mouse && !key && scroll.is_none() {
            return None;
        }
        self.dragged = None;
        self.last_click = None;
        if mouse {
            self.focus = Focus::Diff;
        }
        let mut forwarded = event.clone();
        if let Event::Key(k) = &mut forwarded {
            k.code = scroll.unwrap_or(match k.code {
                K::Char('[') => K::BackTab,
                K::Char(']') => K::Tab,
                code => code,
            });
        }
        Some(match view.event(&forwarded) {
            RevisionEvent::Copy(text) => Action::Copy {
                text,
                label: "file revision",
            },
            RevisionEvent::Close => self.launch(Hit::Show(Focus::Tree), c),
            RevisionEvent::None
            | RevisionEvent::File(_)
            | RevisionEvent::Apply(_)
            | RevisionEvent::ApplyAll => Action::None,
        })
    }
}
