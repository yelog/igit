use super::*;
use crate::ui::help::{HelpContent, HelpSection as Section, PREVIEW_SCROLL_BINDINGS};

const HISTORY_SCROLL: Section = Section::new("Log / File History Preview", PREVIEW_SCROLL_BINDINGS);

impl LogPanel {
    pub(crate) fn keymap(&self) -> HelpContent {
        if let Some(comparison) = &self.workspace_comparison {
            return comparison.keymap();
        }
        if self.filter.file_history.is_some() && self.focus == Pane::Info && !self.editing() {
            return HelpContent::new(
                Section::new(
                    "Log / File History Diff",
                    &[
                        ("Tab", "Focus commit history"),
                        ("e", "Show File at Revision"),
                        ("D", "Compare with Local"),
                        ("[", "Focus previous revision"),
                        ("]", "Focus next revision"),
                        ("n", "Next change"),
                        ("p", "Previous change"),
                        ("j", "Scroll all revisions down"),
                        ("k", "Scroll all revisions up"),
                        ("h", "Scroll focused revision left"),
                        ("l", "Scroll focused revision right"),
                        ("f", "Fold or unfold unchanged regions"),
                        ("z", "Zoom focused revision / restore columns"),
                        ("Ctrl-a", "Select complete revision"),
                        ("y", "Copy selection or complete revision"),
                        ("Esc", "Clear selection; otherwise focus history"),
                    ],
                ),
                vec![HISTORY_SCROLL],
            );
        }
        let modal = if let Some(dialog) = &self.commit_dialog {
            Some(dialog.help())
        } else if self.diff.as_ref().and_then(|d| d.local.as_ref()).is_some() {
            Some(Section::new(
                "Log / Compare with Local",
                &[
                    ("L", "Apply selected revision change to local"),
                    ("A", "Review and apply all revision changes to local"),
                    ("Tab", "Focus next revision pane"),
                    ("Shift-Tab", "Focus previous revision pane"),
                    ("n", "Next change"),
                    ("p", "Previous change"),
                    ("/", "Find text in either pane"),
                    ("N", "Previous search match"),
                    ("j", "Scroll both revisions down"),
                    ("k", "Scroll both revisions up"),
                    ("h", "Scroll focused revision left"),
                    ("l", "Scroll focused revision right"),
                    ("f", "Fold or unfold unchanged regions"),
                    ("z", "Zoom focused revision / restore split panes"),
                    ("Ctrl-a", "Select the complete focused revision"),
                    ("y", "Copy selection or the complete focused revision"),
                    ("Esc", "Clear selection; otherwise close and refresh"),
                ],
            ))
        } else if self.diff.as_ref().is_some_and(|d| d.comparison.is_some()) {
            Some(Section::new(
                if self
                    .diff
                    .as_ref()
                    .and_then(|d| d.comparison.as_ref())
                    .is_some_and(|d| d.is_three_way())
                {
                    "Log / Merge Commit Diff (read-only)"
                } else {
                    "Log / Commit Diff (read-only)"
                },
                &[
                    ("Tab", "Focus next revision pane"),
                    ("Shift-Tab", "Focus previous revision pane"),
                    ("n", "Next change"),
                    ("p", "Previous change"),
                    ("]", "Next file comparison"),
                    ("[", "Previous file comparison"),
                    ("j", "Scroll all panes down"),
                    ("k", "Scroll all panes up"),
                    ("h", "Scroll focused revision left"),
                    ("l", "Scroll focused revision right"),
                    ("PgDn", "Page down"),
                    ("PgUp", "Page up"),
                    ("gg", "Go to start"),
                    ("G", "Go to end"),
                    ("f", "Fold or unfold unchanged regions"),
                    ("Enter", "Toggle focused unchanged region"),
                    ("z", "Zoom focused revision / restore split panes"),
                    ("Ctrl-a", "Select the complete focused revision"),
                    ("y", "Copy selection or the complete focused revision"),
                    ("Esc", "Clear selection; otherwise close diff"),
                ],
            ))
        } else if self.diff.is_some() {
            Some(Section::new(
                "Log / Diff",
                &[
                    ("Ctrl-a", "Select all diff text"),
                    ("j", "Scroll diff down"),
                    ("k", "Scroll diff up"),
                    ("h", "Scroll diff left"),
                    ("l", "Scroll diff right"),
                    ("PgDn", "Page down"),
                    ("PgUp", "Page up"),
                    ("gg", "Go to diff start"),
                    ("G", "Go to diff end"),
                    ("]", "Next file comparison"),
                    ("[", "Previous file comparison"),
                    ("Esc", "Clear selection; otherwise close diff"),
                ],
            ))
        } else if self.view_menu.is_some_and(|menu| menu.column.is_some()) {
            Some(Section::new(
                "Log / Columns",
                &[
                    ("j", "Select next column"),
                    ("k", "Select previous column"),
                    ("K", "Move column left"),
                    ("J", "Move column right"),
                    ("Shift-Up", "Move column left"),
                    ("Shift-Down", "Move column right"),
                    ("Space", "Toggle column visibility"),
                    ("Enter", "Toggle column visibility"),
                    ("h", "Back to view options"),
                    ("Esc", "Back to view options"),
                    ("o", "Close view options"),
                ],
            ))
        } else if self.view_menu.is_some() {
            Some(Section::new(
                "Log / View Options",
                &[
                    ("j", "Select next item"),
                    ("k", "Select previous item"),
                    ("l", "Open Columns options"),
                    ("Enter", "Toggle current option"),
                    ("h", "Back or close menu"),
                    ("o", "Close view options"),
                    ("Esc", "Back or close menu"),
                ],
            ))
        } else if let Some(menu) = &self.branch_menu {
            Some(if menu.text_entry() {
                Section::new(
                    "Log / Branch Search",
                    &[
                        ("Down", "Select next match"),
                        ("Up", "Select previous match"),
                        ("Enter", "Choose match or apply entered range"),
                        ("Tab", "Complete selected match"),
                        ("Ctrl-Enter", "Apply entered branch range"),
                        ("Backspace", "Delete last character"),
                        ("Ctrl-w", "Delete word before cursor"),
                        ("Ctrl-u", "Clear search"),
                        ("PgDn", "Page down"),
                        ("PgUp", "Page up"),
                        ("Esc", "Back to branch menu"),
                    ],
                )
            } else {
                Section::new(
                    "Log / Branch Filter",
                    &[
                        ("j", "Select next branch"),
                        ("k", "Select previous branch"),
                        ("h", "Back to parent menu"),
                        ("Right", "Expand branch folder"),
                        ("Enter", "Select branch or folder"),
                        ("Space", "Toggle branch favorite"),
                        ("/", "Search branches or enter range"),
                        ("Home", "Select first branch"),
                        ("End", "Select last branch"),
                        ("PgDn", "Page down"),
                        ("PgUp", "Page up"),
                        ("Esc", "Back or cancel filter"),
                    ],
                )
            })
        } else if self.editing_paths() {
            Some(match &self.path_dialog {
                Some(paths::Dialog::Text(_)) => Section::new(
                    "Log / Select Paths",
                    &[
                        ("Enter", "New line (one relative path per line)"),
                        ("Up", "Move cursor up"),
                        ("Down", "Move cursor down"),
                        ("Home", "Move to line start"),
                        ("End", "Move to line end"),
                        ("Ctrl-u", "Clear path draft"),
                        ("F2", "Apply all entered paths"),
                        ("Ctrl-s", "Apply all entered paths"),
                        ("Ctrl-Enter", "Apply all entered paths"),
                        ("Esc", "Cancel; keep original filter"),
                    ],
                ),
                Some(paths::Dialog::Tree(_)) => Section::new(
                    "Log / Select in Tree",
                    &[
                        ("j", "Move down one row"),
                        ("k", "Move up one row"),
                        ("l", "Expand folder or move to child"),
                        ("h", "Collapse folder or move to parent"),
                        ("Space", "Toggle repo, folder or file checkbox"),
                        ("yy", "Copy absolute path"),
                        ("yf", "Copy name"),
                        ("yn", "Copy name without extension"),
                        ("yr", "Copy relative path"),
                        ("PgDn", "Page down"),
                        ("PgUp", "Page up"),
                        ("Enter", "Apply checked paths"),
                        ("Esc", "Discard checkbox changes"),
                    ],
                ),
                None => Section::new(
                    "Log / Paths Filter",
                    &[
                        ("/", "Find repo or path"),
                        ("Down", "Select next item"),
                        ("Up", "Select previous item"),
                        ("Enter", "Open picker or apply paths / checked repos"),
                        ("Space", "Open picker or toggle repo checkbox"),
                        ("Ctrl-a", "Toggle all listed repo checkboxes"),
                        ("Ctrl-w", "Delete word before cursor"),
                        ("Ctrl-u", "Clear quick search"),
                        ("Esc", "Close Paths menu"),
                    ],
                ),
            })
        } else if let Some((field, _)) = self.editor {
            let title = match field {
                Field::Refs => "Log / Branch Search",
                Field::Text => "Log / Text Search",
                Field::Author => "Log / User Filter",
                Field::Date => "Log / Date Filter",
                Field::Path => "Log / Paths Filter",
                Field::Branch => "Log / Branch Filter",
            };
            let section = if field == Field::Author {
                Section::new(
                    title,
                    &[
                        ("Down", "Select next author"),
                        ("Up", "Select previous author"),
                        ("Tab", "Select next author"),
                        ("Shift-Tab", "Select previous author"),
                        ("PgDn", "Page through authors"),
                        ("PgUp", "Page back through authors"),
                        ("Home", "Select All users or first match"),
                        ("End", "Select last author"),
                        ("Enter", "Apply selected author or typed text"),
                        ("Backspace", "Delete last character"),
                        ("Ctrl-w", "Delete word before cursor"),
                        ("Ctrl-u", "Clear author search"),
                        ("Esc", "Cancel; keep original user filter"),
                    ],
                )
            } else if field == Field::Date && self.date_range.is_none() {
                Section::new(
                    title,
                    &[
                        ("j", "Select next date range"),
                        ("k", "Select previous date range"),
                        ("Enter", "Apply preset or enter dates"),
                        ("Esc", "Close date menu"),
                    ],
                )
            } else if field == Field::Date {
                Section::new(
                    title,
                    &[
                        ("Tab", "Switch start / end date"),
                        ("Enter", "Validate and apply date range"),
                        ("Backspace", "Delete last date character"),
                        ("Ctrl-u", "Clear current date"),
                        ("Esc", "Cancel; keep original dates"),
                    ],
                )
            } else {
                Section::new(
                    title,
                    &[
                        ("Down", "Select next match"),
                        ("Up", "Select previous match"),
                        ("Enter", "Apply search or filter"),
                        ("Backspace", "Delete last character"),
                        ("Ctrl-w", "Delete word before cursor"),
                        ("Ctrl-u", "Clear input"),
                        ("Esc", "Cancel; keep original filter"),
                    ],
                )
            };
            let mut content = HelpContent::new(section, vec![]);
            if field == Field::Text {
                content.sections.push(Section::new(
                    "Text Matching",
                    &[
                        ("Alt-r", "Toggle regular expressions"),
                        ("Alt-c", "Toggle case sensitivity"),
                    ],
                ));
            }
            return content;
        } else {
            None
        };
        if let Some(section) = modal {
            return HelpContent::new(section, vec![])
                .with_selection(self.selected_text().is_some(), true);
        }
        let current = match self.focus {
            Pane::Graph if self.filter.comparison.is_some() => Section::new(
                "Log / Branch Comparison",
                &[
                    ("j", "Select next commit"),
                    ("k", "Select previous commit"),
                    ("gg", "Select first commit"),
                    ("G", "Select last commit"),
                    ("H", "Select top visible commit"),
                    ("M", "Select middle visible commit"),
                    ("L", "Select bottom visible commit"),
                    ("Ctrl-f", "Page down"),
                    ("Ctrl-b", "Page up"),
                    ("Ctrl-d", "Half page down"),
                    ("Ctrl-u", "Half page up"),
                    ("yy", "Copy full commit ID"),
                    ("ym", "Copy full commit message"),
                    ("ya", "Copy author name and email"),
                    ("O", "Open commit in browser"),
                    ("Enter", "Open commit file diff"),
                    ("/", "Search commit text or hash"),
                ],
            ),
            Pane::Graph => Section::new(
                "Log / Commits",
                &[
                    ("j", "Select next commit"),
                    ("k", "Select previous commit"),
                    ("gg", "Select first commit"),
                    ("G", "Select last commit"),
                    ("H", "Select top visible commit"),
                    ("M", "Select middle visible commit"),
                    ("L", "Select bottom visible commit"),
                    ("Ctrl-f", "Page down"),
                    ("Ctrl-b", "Page up"),
                    ("Ctrl-d", "Half page down"),
                    ("Ctrl-u", "Half page up"),
                    ("yy", "Copy full commit ID"),
                    ("ym", "Copy full commit message"),
                    ("ya", "Copy author name and email"),
                    ("O", "Open commit in browser"),
                    ("a", "Open commit actions"),
                    ("D", "Compare with Local"),
                    ("v", "Toggle commit selection mode"),
                    ("Space", "Toggle commit checkbox in selection mode"),
                    ("C", "Review cherry-pick"),
                    ("d", "Drop commit"),
                    ("u", "Undo commit"),
                    ("R", "Revert commit"),
                    ("Esc", "Clear commit selection mode"),
                    ("F2", "Edit commit message"),
                    ("Enter", "Focus Diff pane"),
                    ("/", "Search commit text or hash"),
                ],
            ),
            Pane::Refs => Section::new(
                "Log / Branch Tree",
                &[
                    ("j", "Select next branch or tag"),
                    ("k", "Select previous branch or tag"),
                    ("h", "Collapse, move to parent or previous pane"),
                    ("l", "Expand, move to child or next pane"),
                    ("Enter", "Filter history by branch or tag"),
                    ("s", "Filter history by branch or tag"),
                    ("/", "Search branches or tags"),
                    ("gg", "Select first item"),
                    ("G", "Select last item"),
                    ("PgDn", "Page down"),
                    ("PgUp", "Page up"),
                ],
            ),
            Pane::Files => Section::new(
                "Log / File Tree",
                &[
                    ("j", "Select next changed file"),
                    ("k", "Select previous changed file"),
                    ("h", "Collapse, move to parent or previous pane"),
                    ("l", "Expand, move to child or next pane"),
                    ("Enter", "Open file diff or toggle folder"),
                    ("yy", "Copy absolute path"),
                    ("yf", "Copy name"),
                    ("yn", "Copy name without extension"),
                    ("yr", "Copy relative path"),
                    ("z", "Collapse all folders"),
                    ("Z", "Expand all folders"),
                    ("gg", "Select first item"),
                    ("G", "Select last item"),
                    ("PgDn", "Page down"),
                    ("PgUp", "Page up"),
                ],
            ),
            Pane::Info => Section::new(
                "Log / Commit Details",
                &[
                    ("Ctrl-a", "Select all commit details"),
                    ("/", "Search commit text or hash"),
                    ("Esc", "Clear text selection"),
                    ("j", "Scroll commit details down"),
                    ("k", "Scroll commit details up"),
                    ("gg", "Go to details start"),
                    ("G", "Go to details end"),
                    ("PgDn", "Page down"),
                    ("PgUp", "Page up"),
                    ("h", "Focus previous pane"),
                    ("l", "Focus next pane"),
                ],
            ),
        };
        let mut content = HelpContent::new(
            current,
            vec![Section::new(
                "Log / General",
                &[
                    ("Tab", "Focus next workspace pane"),
                    ("Shift-Tab", "Focus previous workspace pane"),
                    ("Space b", "Open Branch filter"),
                    ("Space u", "Open User filter"),
                    ("Space d", "Open Date filter"),
                    ("Space p", "Open Paths filter"),
                    ("x", "Clear filters in current Log tab"),
                    ("o", "Open Show / Highlight options"),
                    ("B", "Toggle Branch or Tag sidebar"),
                    ("I", "Toggle inspector sidebar"),
                    ("[", "Switch to previous Log tab"),
                    ("]", "Switch to next Log tab"),
                    ("Ctrl-t", "New Log tab"),
                    ("+", "Load more history"),
                ],
            )],
        );
        if self.filter.comparison.is_some() {
            content.sections[1] = Section::new(
                "Log / Comparison Controls",
                &[
                    ("Tab", "Focus next comparison pane"),
                    ("Shift-Tab", "Focus previous comparison pane"),
                    ("[", "Focus other comparison side"),
                    ("]", "Focus other comparison side"),
                    ("Space u", "Open User filter"),
                    ("Space d", "Open Date filter"),
                    ("Space p", "Open Paths filter"),
                    ("x", "Clear filters in this comparison side"),
                    ("o", "Open Show / Highlight options"),
                    ("I", "Toggle inspector sidebar"),
                    ("+", "Load more commits in this comparison side"),
                    ("F5", "Refresh both branches"),
                    ("Esc", "Close comparison and return to branches"),
                ],
            );
        }
        if self.filter.file_history.is_some() {
            content.sections.insert(
                1,
                Section::new(
                    "Log / File History",
                    &[("e", "Show File at Revision"), ("D", "Compare with Local")],
                ),
            );
            content.sections.insert(1, HISTORY_SCROLL);
        }
        content.with_selection(self.selected_text().is_some(), true)
    }
}
