use super::*;
use crate::ui::help::{HelpContent, HelpSection as Section};

impl PushPanel {
    pub(crate) fn keymap(&self) -> HelpContent {
        if self.group_menu.is_some() {
            return HelpContent::new(
                Section::new(
                    "Push / File Grouping",
                    &[
                        ("j", "Select next grouping option"),
                        ("k", "Select previous grouping option"),
                        ("g", "Select first option: Directory"),
                        ("G", "Select last option: Module"),
                        ("Space", "Toggle grouping option"),
                        ("Enter", "Toggle grouping option"),
                        ("Esc", "Close Group By menu"),
                    ],
                ),
                vec![],
            );
        }
        if self.target_edit.is_some() {
            return HelpContent::new(
                Section::new(
                    "Push / Edit Target",
                    &[
                        ("Backspace", "Delete last target character"),
                        ("Ctrl-u", "Clear target"),
                        ("Tab", "Toggle setting source branch upstream after push"),
                        (
                            "Shift-Tab",
                            "Toggle setting source branch upstream after push",
                        ),
                        ("Enter", "Save remote:branch and tracking choice for review"),
                        ("Esc", "Cancel; keep original target and tracking choice"),
                    ],
                ),
                vec![],
            );
        }
        let current = match self.focus {
            0 => Section::new(
                "Push / Repos and Commits",
                &[
                    ("j", "Select next repo or commit"),
                    ("k", "Select previous repo or commit"),
                    ("Space", "Toggle current repo checkbox"),
                    ("h", "Collapse repo commits"),
                    ("l", "Expand repo commits"),
                    ("Enter", "Toggle repo or open commit files"),
                    ("t", "Edit current repo target and upstream choice"),
                ],
            ),
            1 => Section::new(
                "Push / Changed Files",
                &[
                    ("j", "Select next changed file"),
                    ("k", "Select previous changed file"),
                    ("h", "Collapse folder or move to parent"),
                    ("l", "Expand folder or move to child"),
                    ("Enter", "Open file diff or toggle folder"),
                    ("[", "Collapse all folders"),
                    ("]", "Expand all folders"),
                    ("d", "Toggle current file diff"),
                ],
            ),
            _ => Section::new(
                "Push / Diff and Details",
                &[
                    ("Ctrl-a", "Select all details text"),
                    ("j", "Scroll diff or details down"),
                    ("k", "Scroll diff or details up"),
                    ("Esc", "Clear selection; otherwise close details"),
                    ("d", "Close diff / details"),
                ],
            ),
        };
        HelpContent::new(
            current,
            vec![Section::new(
                "Push / General",
                &[
                    ("Tab", "Focus next pane"),
                    ("Shift-Tab", "Focus previous pane"),
                    ("o", "Open Group By menu"),
                    ("Space", "Toggle current repo checkbox"),
                    ("t", "Edit current repo target and upstream choice"),
                    ("P", "Review push for visible checked repos"),
                    ("R", "Review and retry failed visible repos only"),
                    ("F", "Fetch Push targets in Roots"),
                    ("v", "Toggle follow tags"),
                    ("x", "Toggle skip push hooks"),
                    ("d", "Toggle diff / details"),
                ],
            )],
        )
        .with_selection(self.selected_text().is_some(), true)
    }
}
