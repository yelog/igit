use super::*;
use crate::ui::help::{HelpContent, HelpSection as Section, PREVIEW_SCROLL_BINDINGS};

impl CommitPanel {
    fn with_file_preview_help(&self, mut content: HelpContent) -> HelpContent {
        if self.file_preview_visible() {
            content.sections.insert(
                1,
                Section::new("Commit / File Preview", PREVIEW_SCROLL_BINDINGS),
            );
        }
        if self.file_preview_visible() && self.focus == Focus::Diff {
            content.sections.insert(
                1,
                Section::new(
                    if self.comparison_visible() {
                        "Commit / Revisions"
                    } else {
                        "Commit / Patch"
                    },
                    if self.comparison_visible() {
                        &[
                            ("[", "Focus previous revision"),
                            ("]", "Focus next revision"),
                            ("n", "Next difference or search match"),
                            ("p", "Previous difference"),
                            ("f", "Fold or unfold unchanged lines"),
                            ("z", "Zoom or split focused revision"),
                            ("h", "Scroll revision left"),
                            ("l", "Scroll revision right"),
                            ("y", "Copy selection or complete revision"),
                        ]
                    } else {
                        &[("h", "Scroll patch left"), ("l", "Scroll patch right")]
                    },
                ),
            );
        }
        content
    }

    pub(crate) fn keymap(&self) -> HelpContent {
        if self.staging.is_some() {
            return self.staging_help();
        }
        if self.execution_running() {
            return self.with_file_preview_help(HelpContent::new(
                Section::new(
                    "Commit / Running",
                    &[
                        ("j", "Browse next change"),
                        ("k", "Browse previous change"),
                        ("v", "Toggle preview"),
                        (
                            "Ctrl-x",
                            "Stop remaining repositories after the current commit",
                        ),
                    ],
                ),
                vec![],
            ));
        }
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
                    ],
                ),
            };
            let extra = if matches!(edit, Edit::Message) {
                vec![
                    Section::new(
                        "Commit / Message Navigation",
                        &[
                            ("Esc", "Save message and return to actions"),
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
            } else if matches!(edit, Edit::AISettings) {
                vec![Section::new(
                    "Commit / AI Settings",
                    &[("Esc", "Cancel settings")],
                )]
            } else {
                vec![]
            };
            return HelpContent::new(section, extra)
                .with_selection(self.selected_text().is_some(), false);
        }
        if self.add_running() {
            let current = match self.focus {
                Focus::Tree => Section::new(
                    "Commit / Changes",
                    &[
                        ("j", "Browse next change"),
                        ("k", "Browse previous change"),
                        ("h", "Collapse container or move to parent"),
                        ("l", "Expand container or move to child"),
                        ("Space", "Toggle commit checkbox"),
                        ("Enter", "Open Diff or toggle container"),
                        ("yy", "Copy absolute path"),
                        ("yf", "Copy name"),
                        ("yn", "Copy name without extension"),
                        ("yr", "Copy relative path"),
                        ("v", "Toggle preview"),
                        ("c", "Focus and edit commit message"),
                        ("i", "Edit shared commit message"),
                    ],
                ),
                Focus::Diff => Section::new(
                    "Commit / Preview",
                    &[
                        ("j", "Scroll preview down"),
                        ("k", "Scroll preview up"),
                        ("Esc", "Return to changes"),
                        ("v", "Toggle preview"),
                        ("c", "Focus and edit commit message"),
                    ],
                ),
                Focus::Message => {
                    Section::new("Commit / Message", &[("i", "Edit shared commit message")])
                }
            };
            return self.with_file_preview_help(HelpContent::new(current, vec![]));
        }
        let current = match self.focus {
            Focus::Tree => Section::new(
                "Commit / Changes",
                &[
                    ("j", "Preview next change; keep checkboxes"),
                    ("k", "Select previous change"),
                    ("Space", "Toggle file, repo or changelist checkbox"),
                    ("Enter", "Open Diff, Resolve conflicts, or toggle container"),
                    ("yy", "Copy absolute path"),
                    ("yf", "Copy name"),
                    ("yn", "Copy name without extension"),
                    ("yr", "Copy relative path"),
                    ("c", "Focus and edit commit message"),
                    ("e", "Edit highlighted working file"),
                    (
                        "Ctrl-g",
                        "Inspect staged / unstaged hunks and review Stage / Unstage",
                    ),
                    ("h", "Collapse container or move to parent"),
                    ("l", "Expand container or move to child"),
                    ("g", "Select first change"),
                    ("G", "Select last change"),
                    ("PgDn", "Page down"),
                    ("PgUp", "Page up"),
                    ("m", "Move current scope to another changelist"),
                    ("a", "Add untracked files in the highlighted scope to Git"),
                    (
                        "Insert",
                        "Add untracked files in the highlighted scope to Git",
                    ),
                    ("d", "Review rollback for the highlighted scope"),
                ],
            ),
            Focus::Diff => Section::new(
                "Commit / Preview",
                &[
                    (
                        "Ctrl-g",
                        "Inspect staged / unstaged hunks and review Stage / Unstage",
                    ),
                    (
                        "Ctrl-a",
                        "Select all text in the focused revision or summary",
                    ),
                    ("Esc", "Clear selection; otherwise return to changes"),
                    ("j", "Scroll diff or scope summary down"),
                    ("k", "Scroll diff or scope summary up"),
                    ("g", "Go to preview start"),
                    ("G", "Go to preview end"),
                    ("PgDn", "Page down"),
                    ("PgUp", "Page up"),
                    ("Enter", "Toggle unchanged context or return from summary"),
                    ("c", "Focus and edit commit message"),
                    ("v", "Close preview"),
                ],
            ),
            Focus::Message => Section::new(
                "Commit / Message",
                &[
                    ("Left", "Select previous message action"),
                    ("Right", "Select next message action"),
                    ("Enter", "Run selected message action"),
                    ("i", "Edit shared commit message"),
                    ("[", "Previous repository in Changes"),
                    ("]", "Next repository in Changes"),
                    ("M", "Toggle Amend for highlighted repository"),
                    ("A", "Generate and fill shared commit message"),
                    ("S", "Edit AI endpoint, model and prompt"),
                ],
            ),
        };
        let content = HelpContent::new(
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
                    ("Ctrl-n", "New changelist"),
                    ("i", "Edit shared commit message"),
                    ("[", "Previous repository in Changes"),
                    ("]", "Next repository in Changes"),
                    ("M", "Toggle Amend for highlighted repository"),
                    ("A", "Generate and fill shared commit message"),
                    ("S", "Open AI settings"),
                    ("P", "Commit, then open Push preview"),
                ],
            )],
        );
        self.with_file_preview_help(content)
            .with_selection(self.selected_text().is_some(), true)
    }
}
