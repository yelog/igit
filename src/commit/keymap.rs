use super::*;
use crate::ui::help::{HelpContent, HelpSection as Section};

impl CommitPanel {
    pub(crate) fn keymap(&self) -> HelpContent {
        if self.view_menu.is_some() {
            return HelpContent::new(
                Section::new(
                    "Commit / View Options",
                    &[
                        ("j", "Select next view option"),
                        ("k", "Select previous view option"),
                        ("Tab", "Select next view option"),
                        ("Shift-Tab", "Select previous view option"),
                        ("g", "Select first view option"),
                        ("G", "Select last view option"),
                        ("Enter", "Toggle view option"),
                        ("Space", "Toggle view; keep menu and checked files"),
                        ("Esc", "Close view options"),
                        ("o", "Close view options"),
                    ],
                ),
                vec![],
            );
        }
        if let Some(edit) = &self.edit {
            let section = match edit {
                Edit::Move => Section::new(
                    "Commit / Move to Changelist",
                    &[
                        ("j", "Select next changelist"),
                        ("k", "Select previous changelist"),
                        ("Enter", "Move current scope to selected changelist"),
                        ("Esc", "Cancel move"),
                    ],
                ),
                Edit::AIDraft => Section::new(
                    "Commit / AI Draft Review",
                    &[
                        ("j", "Scroll draft down"),
                        ("k", "Scroll draft up"),
                        ("Enter", "Use draft to replace shared message"),
                        ("Esc", "Discard draft; keep original message"),
                    ],
                ),
                Edit::Stash => Section::new(
                    "Commit / Create Stash",
                    &[
                        ("Enter", "Review all Roots changes with one shared message"),
                        ("Ctrl-s", "Review stash"),
                        ("F2", "Review stash"),
                        ("Esc", "Cancel without creating a stash"),
                    ],
                ),
                Edit::Group => Section::new(
                    "Commit / New Changelist",
                    &[
                        ("Left", "Move cursor left"),
                        ("Right", "Move cursor right"),
                        ("Home", "Move to line start"),
                        ("End", "Move to line end"),
                        ("Backspace", "Delete character before cursor"),
                        ("Delete", "Delete character after cursor"),
                        ("Enter", "Save new changelist"),
                        ("Esc", "Cancel new changelist"),
                    ],
                ),
                Edit::Message | Edit::AISettings => Section::new(
                    if matches!(edit, Edit::Message) {
                        "Commit / Edit Message"
                    } else {
                        "Commit / AI Settings"
                    },
                    &[
                        ("Left", "Move cursor left"),
                        ("Right", "Move cursor right"),
                        ("Up", "Move cursor up"),
                        ("Down", "Move cursor down"),
                        ("Home", "Move to line start"),
                        ("End", "Move to line end"),
                        ("Enter", "Insert new line"),
                        ("Backspace", "Delete character before cursor"),
                        ("Delete", "Delete character after cursor"),
                        ("Ctrl-s", "Save input"),
                        ("F2", "Save input"),
                        ("Esc", "Clear selection; otherwise cancel editing"),
                    ],
                ),
            };
            let extra = if matches!(edit, Edit::Message) {
                vec![
                    Section::new(
                        "Commit / Message Navigation",
                        &[
                            ("Ctrl-w", "Delete word before cursor"),
                            ("Ctrl-u", "Delete to line start"),
                            ("Tab", "Save message and focus next pane"),
                            ("Shift-Tab", "Save message and focus previous pane"),
                            ("F6", "Save message and focus next pane"),
                        ],
                    ),
                    Section::new(
                        "Commit / Text Selection",
                        &[
                            ("Ctrl-a", "Select entire message"),
                            ("Shift-Left", "Extend selection left"),
                            ("Shift-Right", "Extend selection right"),
                            ("Shift-Up", "Extend selection up"),
                            ("Shift-Down", "Extend selection down"),
                            ("Shift-Home", "Select to line start"),
                            ("Shift-End", "Select to line end"),
                        ],
                    ),
                ]
            } else {
                vec![]
            };
            return HelpContent::new(section, extra)
                .with_selection(self.selected_text().is_some(), false);
        }
        let current = match self.focus {
            Focus::Tree => Section::new(
                "Commit / Changes",
                &[
                    ("j", "Preview next change; keep checkboxes"),
                    ("k", "Select previous change"),
                    ("Space", "Toggle file, repo or changelist checkbox"),
                    ("Enter", "Open Diff, Resolve conflicts, or toggle container"),
                    ("h", "Collapse container or move to parent"),
                    ("l", "Expand container or move to child"),
                    ("g", "Select first change"),
                    ("G", "Select last change"),
                    ("PgDn", "Page down"),
                    ("PgUp", "Page up"),
                    ("m", "Move current scope to another changelist"),
                    ("d", "Review rollback for the highlighted scope"),
                ],
            ),
            Focus::Diff => Section::new(
                "Commit / Preview",
                &[
                    ("Ctrl-a", "Select all preview text"),
                    ("Esc", "Clear selection; otherwise return to changes"),
                    ("j", "Scroll diff or scope summary down"),
                    ("k", "Scroll diff or scope summary up"),
                    ("g", "Go to preview start"),
                    ("G", "Go to preview end"),
                    ("PgDn", "Page down"),
                    ("PgUp", "Page up"),
                    ("Enter", "Return to current change"),
                    ("v", "Close preview"),
                ],
            ),
            Focus::Message => Section::new(
                "Commit / Message",
                &[
                    ("Enter", "Edit shared commit message"),
                    ("[", "Previous repository in Changes"),
                    ("]", "Next repository in Changes"),
                    ("a", "Toggle Amend for highlighted repository"),
                    ("A", "Review patches, then generate AI draft"),
                    ("S", "Edit AI endpoint, model and prompt"),
                ],
            ),
        };
        HelpContent::new(
            current,
            vec![Section::new(
                "Commit / General",
                &[
                    ("Tab", "Focus next pane"),
                    ("Shift-Tab", "Focus previous pane"),
                    ("v", "Toggle current row preview"),
                    ("o", "Open View Options"),
                    ("t", "Toggle flat list / directory grouping"),
                    ("s", "Stash all Roots changes, including untracked files"),
                    ("T", "Toggle two-column Git status"),
                    ("z", "Collapse all containers"),
                    ("Z", "Expand all containers"),
                    ("u", "Undo last checkbox change"),
                    ("n", "New changelist"),
                    ("i", "Edit shared commit message"),
                    ("[", "Previous repository in Changes"),
                    ("]", "Next repository in Changes"),
                    ("a", "Toggle Amend for highlighted repository"),
                    ("A", "Review and generate AI draft"),
                    ("S", "Open AI settings"),
                    ("c", "Review commit of visible checked files"),
                    ("P", "Review commit, then open Push preview"),
                    ("Esc", "Return to changes"),
                ],
            )],
        )
        .with_selection(self.selected_text().is_some(), true)
    }
}
