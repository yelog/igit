use super::*;
use crate::ui::help::{HelpContent, HelpSection};

impl BranchPanel {
    pub(crate) fn keymap(&self) -> HelpContent {
        let section = if self.input.is_some() {
            HelpSection::new(
                "Branch / Name Input",
                &[
                    ("Backspace", "Delete last name character"),
                    ("Ctrl-u", "Clear branch name"),
                    ("Enter", "Validate name and review action"),
                    ("Esc", "Cancel name input"),
                ],
            )
        } else {
            HelpSection::new(
                "Branch / Branch Tree",
                &[
                    ("j", "Select next branch or group"),
                    ("k", "Select previous branch or group"),
                    ("h", "Collapse group or move to parent"),
                    ("l", "Expand group or move to child"),
                    ("Space", "Toggle group"),
                    ("g", "Select first row"),
                    ("G", "Select last row"),
                    ("PgDn", "Move down one page"),
                    ("PgUp", "Move up one page"),
                    ("a", "Create local branch from focused branch"),
                    ("d", "Review safe deletion of local branch"),
                    ("Enter", "Switch branch or toggle group"),
                    ("m", "Merge focused branch into current branch"),
                    ("r", "Rename focused local branch"),
                    (":", "Open Actions (including Roots)"),
                ],
            )
        };
        HelpContent::new(section, vec![])
    }
}
