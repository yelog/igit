use super::*;
use crate::ui::help::{HelpContent, HelpSection as Section};

impl MergePanel {
    pub(crate) fn keymap(&self) -> HelpContent {
        if self.view.overlay.is_some() {
            return HelpContent::new(
                Section::new(
                    "Merge / Editor Options",
                    &[
                        ("j", "Move down"),
                        ("k", "Move up"),
                        ("Enter", "Choose option"),
                        ("Esc", "Return to merge editor"),
                    ],
                ),
                vec![],
            );
        }
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
                        ("j", "Select next row"),
                        ("k", "Select previous row"),
                        ("gg", "Select first item"),
                        ("G", "Select last item"),
                        (
                            "Enter",
                            "Open file, fold or expand Unresolved, or review Continue",
                        ),
                        ("Y", "Preview the entire current side of the selected file"),
                        ("yy", "Copy absolute path"),
                        ("yf", "Copy name"),
                        ("yn", "Copy name without extension"),
                        ("yr", "Copy relative path"),
                        ("T", "Preview the entire incoming side of the selected file"),
                        (
                            "S",
                            "Prepare non-conflicting drafts in the selected repository",
                        ),
                        ("Space", "Fold or expand the Unresolved group"),
                        ("o", "Toggle full file paths"),
                        ("C", "Review Continue for the selected repository"),
                        ("B", "Review Abort for the selected repository operation"),
                        (
                            "N",
                            "Review Skip for a stopped rebase, cherry-pick, or revert",
                        ),
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
                    ("Shift-F7", "Previous pending change or conflict"),
                    ("F7", "Next pending change or conflict"),
                    ("h", "Focus left pane"),
                    ("l", "Focus right pane"),
                    ("Tab", "Focus next pane"),
                    ("Shift-Tab", "Focus previous pane"),
                    ("Left", "Scroll focused pane left"),
                    ("Right", "Scroll focused pane right"),
                    ("PgDn", "Page down; pause block tracking"),
                    ("PgUp", "Page up; pause block tracking"),
                    ("z", "Toggle full-width pane"),
                    ("o", "Fold or unfold unchanged lines"),
                    ("v", "Compare file versions"),
                    ("O", "Open merge editor settings"),
                ],
            ),
            vec![Section::new(
                "Merge / Resolve and Apply",
                &[
                    ("H", "Use left side for current block"),
                    ("L", "Use right side for current block"),
                    ("Y", "Accept entire left version"),
                    ("T", "Accept entire right version"),
                    ("Enter", "Accept focused source block or edit Result"),
                    ("x", "Ignore focused source block"),
                    ("d", "Show focused source details"),
                    ("b", "Keep both sides of current conflict"),
                    ("a", "Apply all non-conflicting blocks"),
                    (
                        "m",
                        "Resolve all non-conflicting and disjoint single-line edits",
                    ),
                    ("i", "Edit current block result"),
                    ("u", "Undo last resolution or edit"),
                    ("s", "Save session draft and close editor"),
                    ("A", "Review and apply file; write and stage"),
                    ("F", "Save session draft; return to conflict list"),
                    ("Esc", "Save session draft; return to conflict list"),
                ],
            )],
        )
    }
}
