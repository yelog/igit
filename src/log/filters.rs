use super::*;
use anyhow::{bail, Context as _};
use crossterm::event::KeyEvent;
use regex_lite::{Regex, RegexBuilder};
use std::collections::BTreeSet;

pub(super) enum Needle {
    Regex(Regex),
    Literal(String, bool),
}
pub(super) fn matcher(q: &str, regex: bool, case_sensitive: bool) -> Result<Option<Needle>> {
    Ok(if q.is_empty() {
        None
    } else if regex {
        Some(Needle::Regex(
            RegexBuilder::new(q)
                .case_insensitive(!case_sensitive)
                .build()
                .context("Invalid regular expression")?,
        ))
    } else {
        Some(Needle::Literal(
            if case_sensitive {
                q.to_owned()
            } else {
                q.to_lowercase()
            },
            case_sensitive,
        ))
    })
}
impl Needle {
    pub(super) fn is_match(&self, text: &str) -> bool {
        match self {
            Self::Regex(r) => r.is_match(text),
            Self::Literal(q, true) => text.contains(q),
            Self::Literal(q, false) => text.to_lowercase().contains(q),
        }
    }
}
pub(super) fn short_ref(s: &str) -> &str {
    ["refs/heads/", "refs/remotes/", "refs/tags/"]
        .iter()
        .find_map(|p| s.strip_prefix(p))
        .unwrap_or(s)
}
impl Field {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Refs => "Branch or tag",
            Self::Text => "Text or hash",
            Self::Branch => "Branch",
            Self::Author => "User",
            Self::Date => "Date",
            Self::Path => "Paths",
        }
    }
    pub(super) fn inline(self) -> bool {
        matches!(self, Self::Refs | Self::Text)
    }
}
impl LogPanel {
    pub(super) fn matcher(&self, q: &str) -> Result<Option<Needle>> {
        matcher(q, self.regex, self.case_sensitive)
    }
    pub(super) fn toggle_match_mode(&mut self, h: Hit) -> Result<()> {
        let old = (self.regex, self.case_sensitive);
        match h {
            Hit::ToggleRegex => self.regex = !self.regex,
            _ => self.case_sensitive = !self.case_sensitive,
        }
        let keep = self.rev().map(|r| (r.root, r.oid.clone()));
        if let Err(e) = self.matcher(&self.filter.query) {
            (self.regex, self.case_sensitive) = old;
            return Err(e);
        }
        self.rebuild_visible(keep)
    }
    pub(super) fn value(&self, k: Field) -> String {
        match k {
            Field::Refs => self.ref_query.clone(),
            Field::Text => self.filter.query.clone(),
            Field::Branch => self
                .filter
                .branch
                .as_ref()
                .map(|b| b.text(true))
                .unwrap_or_default(),
            Field::Author => self.filter.author.clone(),
            Field::Date => self
                .filter
                .date
                .as_ref()
                .map(DateFilter::label)
                .unwrap_or_default(),
            Field::Path => String::new(), // Paths opens on its commands, with an empty quick search.
        }
    }
    pub(super) fn begin(&mut self, k: Field, c: &Context) -> Result<()> {
        if self.filter.comparison.is_some() && matches!(k, Field::Branch | Field::Refs) {
            return Ok(());
        }
        if k == Field::Path && self.filter.file_history.is_some() {
            return Ok(());
        }
        self.discard_editor();
        self.view_menu = None;
        self.last = None;
        if k == Field::Refs {
            self.show_refs = true;
            self.focus = Pane::Refs;
        } else {
            self.focus = Pane::Graph;
        }
        if k == Field::Branch {
            self.branch_menu = Some(branches::Menu::open(&self.hist, &self.filter, c)?);
        }
        self.editor = Some((k, self.value(k)));
        self.edit_changed(c);
        Ok(())
    }
    pub(super) fn discard_editor(&mut self) {
        self.path_options.clear();
        self.path_dialog = None;
        self.branch_menu = None;
        self.editor = None;
        self.date_range = None;
        self.choice = None;
        self.suggestions.clear();
    }
    pub(crate) fn dismiss_filter_dropdown(&mut self) {
        self.view_menu = None;
        if self.path_dialog.is_none() && matches!(self.editor, Some((field, _)) if !field.inline())
        {
            self.discard_editor();
        }
    }
    pub(super) fn edit_changed(&mut self, c: &Context) {
        self.choice = None;
        self.suggestions.clear();
        let Some((k, input)) = &self.editor else {
            return;
        };
        let needle = input.to_lowercase();
        let mut opts = Vec::new();
        match k {
            Field::Refs => {
                self.rebuild_refs(c);
                return;
            }
            Field::Text | Field::Branch => return,
            Field::Author => {
                opts.push(("me".into(), "me  · Git identity".into()));
                let authors = self
                    .hist
                    .revs
                    .iter()
                    .filter(|r| self.filter.includes_root(r.root))
                    .map(|r| &r.author)
                    .collect::<BTreeSet<_>>();
                opts.extend(authors.into_iter().map(|s| (s.clone(), s.clone())));
            }
            Field::Date => {
                self.suggestions = ["Select...", "Last 24 hours", "Last 7 days"]
                    .map(|s| (s.to_owned(), s.to_owned()))
                    .into();
                self.choice = Some(match self.filter.date {
                    Some(DateFilter::Last24Hours) => 1,
                    Some(DateFilter::Last7Days) => 2,
                    _ => 0,
                });
                return;
            }
            Field::Path => {
                self.path_options = c
                    .scope
                    .iter()
                    .copied()
                    .filter(|&id| c.repos[id].name.to_lowercase().contains(&needle))
                    .collect();
                self.choice = input.is_empty().then_some(0);
                return;
            }
        }
        if needle.is_empty() {
            self.suggestions.push((String::new(), "All users".into()));
        }
        self.suggestions
            .extend(opts.into_iter().filter(|(val, label)| {
                val.to_lowercase().contains(&needle) || label.to_lowercase().contains(&needle)
            }));
    }
    pub(super) fn accept_choice(&mut self, i: usize, c: &Context) -> Result<()> {
        if self.editing_paths() {
            return self.path_choice(i, c);
        }
        if matches!(self.editor, Some((Field::Date, _))) {
            match i {
                0 => {
                    self.date_range = Some(DateRangeEditor {
                        values: match &self.filter.date {
                            Some(DateFilter::Range { from, to }) => [from.clone(), to.clone()],
                            _ => Default::default(),
                        },
                        active: 0,
                    });
                    self.choice = None;
                    self.suggestions.clear();
                    return Ok(());
                }
                1 => self.filter.date = Some(DateFilter::Last24Hours),
                2 => self.filter.date = Some(DateFilter::Last7Days),
                _ => return Ok(()),
            }
            self.discard_editor();
            return self.reload(c, false);
        }
        if let (Some((_, value)), Some((selected, _))) =
            (self.editor.as_mut(), self.suggestions.get(i))
        {
            *value = selected.clone();
        }
        self.choice = None;
        self.apply(c)
    }
    pub(super) fn apply(&mut self, c: &Context) -> Result<()> {
        if self.path_dialog.is_some() {
            return self.apply_path_dialog(c);
        }
        if let Some(i) = self.choice {
            if self.editing_paths() && i >= paths::COMMANDS.len() {
                // The checkboxes apply immediately. Enter leaves the current selection intact.
                self.discard_editor();
                return Ok(());
            }
            return self.accept_choice(i, c);
        }
        let Some((k, v)) = self.editor.clone() else {
            return Ok(());
        };
        match k {
            Field::Refs => {
                self.ref_query = v;
                self.ref_search.invalidate();
            }
            Field::Text => {
                self.matcher(&v)?;
                self.filter.query = v;
            }
            Field::Branch => return Ok(()), // The hierarchical menu applies complete ref identities.
            Field::Author => self.filter.author = v,
            Field::Date => {
                let Some(range) = &self.date_range else {
                    return self.accept_choice(0, c);
                };
                let [from, to] = &range.values;
                if !valid_date(from) || !valid_date(to) {
                    bail!("Enter valid start and end dates as YYYY-MM-DD");
                }
                if from > to {
                    bail!("Start date must not be after end date");
                }
                self.filter.date = Some(DateFilter::Range {
                    from: from.clone(),
                    to: to.clone(),
                });
            }
            Field::Path => return self.apply_path_input(c, v),
        }
        self.discard_editor();
        match k {
            Field::Refs => {
                self.rebuild_refs(c);
                self.refs.reset_cursor();
                Ok(())
            }
            Field::Text => self.rebuild_visible(None),
            _ => self.reload(c, false),
        }
    }
    pub(super) fn clear_field(&mut self, k: Field, c: &Context) -> Result<()> {
        if k == Field::Path {
            if self.filter.file_history.is_some() {
                return Ok(());
            }
            return self.clear_paths(c);
        }
        if matches!(k, Field::Date | Field::Branch) {
            if k == Field::Date {
                self.filter.date = None;
            } else {
                self.filter.branch = None;
            }
            self.discard_editor();
            return self.reload(c, false);
        }
        self.editor = Some((k, String::new()));
        self.choice = None;
        self.apply(c)
    }
    pub(super) fn edit_key(&mut self, k: &KeyEvent, c: &Context) -> Result<()> {
        if self.branch_menu.is_some() {
            return self.branch_input(branches::Input::Key(k), c);
        }
        if self.editing_paths() {
            if self.path_dialog.is_some() {
                return self.path_dialog_key(k, c);
            }
            if k.code == K::Char('/')
                && self.choice.is_some()
                && self.editor.as_ref().is_some_and(|(_, q)| q.is_empty())
            {
                self.choice = None;
                return Ok(());
            }
            if k.code == K::Char('a') && k.modifiers.contains(Mods::CONTROL) {
                return self.toggle_listed_roots(c);
            }
            if k.code == K::Char(' ') && k.modifiers.is_empty() {
                if let Some(i) = self.choice {
                    return self.path_choice(i, c);
                }
            }
            if k.code == K::Down && self.choice.is_none() && !self.path_options.is_empty() {
                self.choice = Some(paths::COMMANDS.len());
                return Ok(());
            }
            if matches!(k.code, K::Home | K::End)
                || (self.choice.is_some() && matches!(k.code, K::Up | K::Down | K::Char('j' | 'k')))
            {
                let last = self.choice_count() - 1;
                let current = self.choice.unwrap_or(0);
                self.choice = Some(match k.code {
                    K::Home => 0,
                    K::End => last,
                    K::Up | K::Char('k') => current.checked_sub(1).unwrap_or(last),
                    _ => (current + 1).min(last),
                });
                return Ok(());
            }
        }
        let count = self.choice_count();
        if matches!(self.editor, Some((Field::Date, _))) {
            return self.date_key(k, c);
        }
        let Some((field, v)) = &mut self.editor else {
            return Ok(());
        };
        match k.code {
            K::Esc => {
                self.discard_editor();
                self.rebuild_refs(c);
            }
            K::Enter => self.apply(c)?,
            K::Down | K::Up if count > 0 => {
                let max = count - 1;
                self.choice = Some(match (self.choice, k.code) {
                    (None, K::Up) => max,
                    (None, _) => 0,
                    (Some(i), K::Down) => (i + 1).min(max),
                    (Some(i), _) => i.saturating_sub(1),
                });
            }
            K::Char('r' | 'c') if *field == Field::Text && k.modifiers.contains(Mods::ALT) => self
                .toggle_match_mode(if k.code == K::Char('r') {
                    Hit::ToggleRegex
                } else {
                    Hit::ToggleCase
                })?,
            K::Backspace => {
                v.pop();
                self.edit_changed(c);
            }
            K::Char('u') if k.modifiers.contains(Mods::CONTROL) => {
                v.clear();
                self.edit_changed(c);
            }
            K::Char('w') if k.modifiers == Mods::CONTROL => {
                v.truncate(crate::ui::selection::previous_word(v, v.len()));
                self.edit_changed(c);
            }
            K::Char(ch) if !k.modifiers.intersects(Mods::CONTROL | Mods::ALT) => {
                v.push(ch);
                self.edit_changed(c);
            }
            _ => {}
        }
        Ok(())
    }
    fn date_key(&mut self, k: &KeyEvent, c: &Context) -> Result<()> {
        match k.code {
            K::Esc => self.discard_editor(),
            K::Enter => self.apply(c)?,
            _ => {
                if let Some(range) = &mut self.date_range {
                    let v = &mut range.values[range.active];
                    match k.code {
                        K::Tab | K::BackTab | K::Down | K::Up => range.active = 1 - range.active,
                        K::Backspace => {
                            v.pop();
                        }
                        K::Char('u') if k.modifiers.contains(Mods::CONTROL) => v.clear(),
                        K::Char(ch) if !k.modifiers.intersects(Mods::CONTROL | Mods::ALT) => {
                            v.push(ch);
                        }
                        _ => {}
                    }
                } else {
                    match k.code {
                        K::Down | K::Char('j') => {
                            self.choice = Some((self.choice.unwrap_or(0) + 1).min(2));
                        }
                        K::Up | K::Char('k') => {
                            self.choice = Some(self.choice.unwrap_or(0).saturating_sub(1));
                        }
                        _ => {}
                    }
                }
            }
        }
        Ok(())
    }
    pub(super) fn snapshot(&self) -> ViewState {
        ViewState {
            filter: self.filter.clone(),
            ref_query: self.ref_query.clone(),
            ref_search: self.ref_search.clone(),
            file_search: self.file_search.clone(),
            regex: self.regex,
            case_sensitive: self.case_sensitive,
            selected: self.history_selection(),
        }
    }
    fn restore(&mut self, c: &Context) -> Result<()> {
        let s = self.views[self.view_index].clone();
        self.filter = s.filter;
        self.ref_query = s.ref_query;
        self.ref_search = s.ref_search;
        self.file_search = s.file_search;
        self.regex = s.regex;
        self.case_sensitive = s.case_sensitive;
        self.discard_editor();
        self.last = None;
        self.focus = Pane::Graph;
        self.reload_history(c, s.selected)
    }
    pub(super) fn new_tab(&mut self, c: &Context) -> Result<()> {
        if self.filter.comparison.is_some() {
            return Ok(());
        }
        self.views[self.view_index] = self.snapshot();
        self.views.push(ViewState::default());
        self.view_index = self.views.len() - 1;
        self.restore(c)
    }
    pub(crate) fn can_close_tab(&self) -> bool {
        self.views.len() > 1
    }
    pub(crate) fn close_tab(&mut self, c: &Context) -> Result<()> {
        if !self.can_close_tab() {
            return Ok(());
        }
        self.views.remove(self.view_index);
        self.view_index = self.view_index.min(self.views.len() - 1);
        self.restore(c)
    }
    pub(super) fn switch_tab(&mut self, i: usize, c: &Context) -> Result<()> {
        if i == self.view_index || i >= self.views.len() {
            return Ok(());
        }
        self.views[self.view_index] = self.snapshot();
        self.view_index = i;
        self.restore(c)
    }
}
fn valid_date(s: &str) -> bool {
    let b = s.as_bytes();
    if b.len() != 10
        || !b.iter().enumerate().all(|(i, &c)| {
            if i == 4 || i == 7 {
                c == b'-'
            } else {
                c.is_ascii_digit()
            }
        })
    {
        return false;
    }
    let y = s[..4].parse::<u32>().unwrap_or(0);
    let m = s[5..7].parse::<u8>().unwrap_or(0);
    let d = s[8..].parse::<u8>().unwrap_or(0);
    let days = match m {
        2 if y % 4 == 0 && (y % 100 != 0 || y % 400 == 0) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        _ => 0,
    };
    y > 0 && d > 0 && d <= days
}
