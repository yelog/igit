use super::*;
use crate::ui::key_sequence::KeySequence;
use crossterm::event::KeyEvent;

impl PushPanel {
    pub(super) fn selected_commit(&self) -> Option<(usize, &Out)> {
        if self.focus != 0 || self.editing() {
            return None;
        }
        let (repo, commit) = self.selected()?;
        let entry = self.entries.iter().find(|entry| entry.idx == repo)?;
        Some((repo, entry.logs.get(commit?)?))
    }

    pub(crate) fn key_sequence(&self) -> Option<KeySequence> {
        (self.copy_pending && self.selected_commit().is_some()).then_some(KeySequence::CommitCopy)
    }

    pub(crate) fn cancel_key_sequence(&mut self) {
        self.copy_pending = false;
    }

    pub(super) fn commit_copy_key(&mut self, key: &KeyEvent, context: &Context) -> Option<Action> {
        let pending = std::mem::take(&mut self.copy_pending);
        if !key.modifiers.is_empty() && key.modifiers != KeyModifiers::SHIFT {
            return None;
        }
        let (repo, commit) = self.selected_commit()?;
        if pending {
            if let KeyCode::Char(suffix @ ('y' | 'm' | 'a')) = key.code {
                return Some(
                    crate::ui::commit_copy::copy(
                        &context.repos.get(repo)?.path,
                        &commit.id,
                        None,
                        suffix,
                    )
                    .unwrap_or_else(|error| {
                        Action::Notice(NoticeLevel::Error, format!("Push: {error}"))
                    }),
                );
            }
        }
        if key.code == KeyCode::Char('y') {
            self.copy_pending = true;
            return Some(Action::None);
        }
        None
    }
}
