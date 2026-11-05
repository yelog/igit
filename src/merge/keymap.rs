use super::*;
use crate::ui::help::{HelpContent, HelpSection as Section};

impl MergePanel {
    pub(crate) fn keymap(&self) -> HelpContent {
        if self.edit.is_some() {
            return HelpContent::new(
                Section::new(
                    "Merge / Edit Result",
                    &[
                        ("Left", "Move cursor left"),
                        ("Right", "Move cursor right"),
                        ("Up", "Move cursor up"),
                        ("Down", "Move cursor down"),
                        ("Home", "Move to line start"),
                        ("End", "Move to line end"),
                        ("Enter", "Insert new line"),
                        ("Tab", "Insert tab"),
                        ("Backspace", "Delete character before cursor"),
                        ("Delete", "Delete character after cursor"),
                        ("Ctrl-s", "Accept edited block"),
                        ("Ctrl-Enter", "Accept edited block"),
                        ("Esc", "Cancel block edit"),
                    ],
                ),
                vec![],
            );
        }
        if self.doc.is_none() {
            return HelpContent::new(
                Section::new(
                    "Merge / Conflicted Files",
                    &[
                        ("j", "Select next conflicted file or repo operation"),
                        ("k", "Select previous conflicted file or repo operation"),
                        ("g", "Select first item"),
                        ("G", "Select last item"),
                        ("Enter", "Open conflict or review pending operation"),
                        ("C", "Review and continue repo merge / rebase"),
                    ],
                ),
                vec![],
            );
        }
        let title = match self.side {
            0 => "Merge / Left Source",
            1 => "Merge / Result",
            _ => "Merge / Right Source",
        };
        HelpContent::new(
            Section::new(
                title,
                &[
                    ("j", "Select next change block"),
                    ("k", "Select previous change block"),
                    ("[c", "Previous unresolved conflict"),
                    ("]c", "Next unresolved conflict"),
                    ("h", "Focus left pane"),
                    ("l", "Focus right pane"),
                    ("Tab", "Focus next pane"),
                    ("Shift-Tab", "Focus previous pane"),
                    ("Left", "Scroll all three panes left"),
                    ("Right", "Scroll all three panes right"),
                    ("PgDn", "Page down; pause block tracking"),
                    ("PgUp", "Page up; pause block tracking"),
                    ("z", "Toggle full-width pane"),
                ],
            ),
            vec![Section::new(
                "Merge / Resolve and Apply",
                &[
                    ("H", "Use left side for current block"),
                    ("L", "Use right side for current block"),
                    ("b", "Keep both sides of current conflict"),
                    ("a", "Apply all non-conflicting blocks"),
                    ("i", "Edit current block result"),
                    ("u", "Undo last resolution or edit"),
                    ("s", "Save file draft for this session"),
                    ("A", "Review and apply file; write and stage"),
                    ("Esc", "Save session draft; return to conflict list"),
                ],
            )],
        )
    }
}
