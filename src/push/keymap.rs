use super::*;
use crate::ui::help::{HelpContent, HelpSection as Section};

impl PushPanel {
    pub(crate) fn keymap(&self) -> HelpContent {
        if let Some(diff) = &self.diff {
            let section = match diff.content {
                Some(preview::FileDiff::Split(_)) => Section::new(
                    "Push / Diff (read-only)",
                    &[
                        ("Tab", "Focus next revision pane"),
                        ("Shift-Tab", "Focus previous revision pane"),
                        ("n", "Next change"),
                        ("p", "Previous change"),
                        ("]", "Next file comparison"),
                        ("[", "Previous file comparison"),
                        ("j", "Scroll both revisions down"),
                        ("k", "Scroll both revisions up"),
                        ("h", "Scroll focused revision left"),
                        ("l", "Scroll focused revision right"),
                        ("f", "Fold or unfold unchanged regions"),
                        ("z", "Zoom focused revision / restore split panes"),
                        ("Ctrl-a", "Select complete focused revision"),
                        ("y", "Copy selection or complete focused revision"),
                        ("Esc", "Clear selection; otherwise close diff"),
                    ],
                ),
                _ => Section::new(
                    "Push / Diff (read-only)",
                    &[
                        ("]", "Next file comparison"),
                        ("[", "Previous file comparison"),
                        ("j", "Scroll patch down"),
                        ("k", "Scroll patch up"),
                        ("Ctrl-a", "Select all patch text"),
                        ("Esc", "Clear selection; otherwise close diff"),
                    ],
                ),
            };
            return HelpContent::new(section, vec![])
                .with_selection(self.selected_text().is_some(), true);
        }
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
                    ("yy", "Copy absolute path"),
                    ("yf", "Copy name"),
                    ("yn", "Copy name without extension"),
                    ("yr", "Copy relative path"),
                    ("Enter", "Open file diff or toggle folder"),
                    ("[", "Collapse all folders"),
                    ("]", "Expand all folders"),
                    ("d", "Open current file diff or toggle details"),
                ],
            ),
            _ => Section::new(
                "Push / Diff and Details",
                &[
                    ("Ctrl-a", "Select all details text"),
                    ("j", "Scroll details down"),
                    ("k", "Scroll details up"),
                    ("Esc", "Clear selection; otherwise close details"),
                    ("d", "Close details"),
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
