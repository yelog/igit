//! Commit candidates share file history's read-only revision comparison.
use super::*;
use crate::merge::{RevisionDiff, RevisionEvent};
use crate::ui::search::Prompt;

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
    pub(super) fn ensure_preview(&mut self, c: &Context) {
        if !self.preview_open {
            return;
        }
        let key = self.active().map_or("", |row| row.id.as_str());
        if key == self.diff_key {
            return;
        }
        let key = key.to_owned();
        let loaded = self
            .active()
            .and_then(Row::file)
            .map(|i| {
                let file = &self.files[i];
                Loaded::read(&c.repos[file.root].path, &file.change)
            })
            .unwrap_or(Loaded {
                patch: String::new(),
                pair: None,
            });
        self.set_preview(key, loaded);
    }

    pub(super) fn set_preview(&mut self, key: String, loaded: Loaded) -> bool {
        if self.diff_key == key
            && self.diff_text == loaded.patch
            && self.diff_sources == loaded.pair
        {
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
        self.diff_text = loaded.patch;
        self.diff_comparison = loaded.pair.as_ref().map(|pair| {
            let name = self
                .active()
                .and_then(Row::file)
                .map(|i| self.files[i].change.label())
                .unwrap_or_default();
            let mut view = RevisionDiff::two_way(
                name,
                pair.revisions.clone(),
                pair.sources.each_ref().map(|s| s.text.clone()),
                pair.sources.each_ref().map(|s| s.description.clone()),
            );
            if let Some(previous) = &self.diff_comparison {
                view.inherit_search(previous);
            } else {
                view.inherit_query(self.diff_search.prompt());
            }
            Box::new(view)
        });
        self.diff_sources = loaded.pair;
        true
    }

    pub(super) fn comparison_visible(&self) -> bool {
        self.file_preview_visible()
            && self.active().is_some_and(|row| row.id == self.diff_key)
            && self.diff_comparison.is_some()
    }

    pub(crate) fn file_comparison_focused(&self) -> bool {
        self.focus == Focus::Diff && self.comparison_visible() && !self.editing()
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
        if let Some(view) = &self.diff_comparison {
            self.diff_search.inherit_query(view.search_prompt());
        }
        self.diff_key.clear();
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
            && self.comparison_visible()
            && matches!(event, Event::Key(k) if
                ((k.modifiers - Mod::SHIFT).is_empty()
                    && matches!(k.code,
                        K::Char('n' | 'p' | 'f' | 'z' | 'j' | 'k' | 'h' | 'l' | 'g' | 'G' | 'y' | '[' | ']' | ' ')
                        | K::Enter | K::Esc | K::Up | K::Down | K::Left | K::Right
                        | K::PageUp | K::PageDown | K::Home | K::End))
                || (k.modifiers == Mod::CONTROL
                    && (matches!(k.code, K::Char('a' | 'f' | 'b' | 'd' | 'u') | K::Insert)
                        || k.code == K::Char('c') && self.selected_text().is_some())))
    }

    pub(super) fn comparison_event(&mut self, event: &Event, c: &Context) -> Option<Action> {
        if !self.comparison_visible() {
            return None;
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
            RevisionEvent::None | RevisionEvent::File(_) => Action::None,
        })
    }
}
