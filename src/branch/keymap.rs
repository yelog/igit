use super::*;
use crate::ui::help::{HelpContent, HelpSection};

impl BranchPanel {
    pub(crate) fn keymap(&self) -> HelpContent {
        if self.input.is_some() {
            return HelpContent::new(
                HelpSection::new(
                    "Branch / Name Input",
                    &[
                        ("Backspace", "Delete last name character"),
                        ("Ctrl-u", "Clear branch name"),
                        ("Enter", "Submit branch name"),
                        ("Esc", "Cancel name input"),
                    ],
                ),
                vec![],
            );
        }
        if self.view_menu {
            return HelpContent::new(
                HelpSection::new(
                    "Branch / Group By",
                    &[
                        ("Space", "Toggle branch prefix grouping"),
                        ("Enter", "Toggle branch prefix grouping"),
                        ("Esc", "Close Group By menu"),
                    ],
                ),
                vec![],
            );
        }
        let section = if self.focused_pane() == PaneId::BranchRepos {
            HelpSection::new(
                "Branch / Repositories",
                &[
                    ("j", "Select next repository"),
                    ("k", "Select previous repository"),
                    ("Enter", "Focus selected repository's branches"),
                    ("l", "Focus branch tree"),
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
                    ("a", "Create local branch from focused branch"),
                    ("d", "Review safe deletion of local branch"),
                    ("Enter", "Switch branch or toggle group"),
                    ("m", "Merge focused branch into current branch"),
                    ("r", "Rename focused local branch"),
                    ("U", "Review fast-forward pull for focused local branch"),
                    ("P", "Review push for focused local branch"),
                ],
            )
        };
        let mut sections = vec![HelpSection::new(
            "Branch / List Navigation",
            &[
                ("gg", "Select first row"),
                ("G", "Select last row"),
                ("H", "Select first visible row"),
                ("M", "Select middle visible row"),
                ("L", "Select last visible row"),
                ("PgDn", "Move down one page"),
                ("PgUp", "Move up one page"),
                ("Ctrl-f", "Move down one page"),
                ("Ctrl-b", "Move up one page"),
                ("Ctrl-d", "Move down half a page"),
                ("Ctrl-u", "Move up half a page"),
                ("o", "Open Group By menu"),
                (":", "Open Actions (including Roots)"),
            ],
        )];
        if self.has_repository_pane() {
            sections.push(HelpSection::new(
                "Branch / Panes",
                &[
                    ("Tab", "Focus next pane"),
                    ("Shift-Tab", "Focus previous pane"),
                ],
            ));
        }
        HelpContent::new(section, sections)
    }
}
