use super::{
    backend,
    rows::{monochrome, CONFLICT_SELECTION},
    slice, word, CommitPanel, Focus, Hit, Kind, MessageAction, Row,
};
use crate::{
    core::Context,
    ui::icons::{Icon, IconMode},
    ui::idea::{
        button_style, clipped as fit, diff_line, repo_tint, BLUE, DIM, EDGE, GOLD, GREEN, INK,
        PANEL, RED, SELECT, TAG,
    },
    ui::panes::{pane_frame, Axis, PaneId, SplitId},
    ui::selection::{TextLayout, TextSelection, Wrap},
    ui::Panel,
};
use ratatui::{
    layout::{Constraint, Layout, Margin, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};
use unicode_width::UnicodeWidthStr;

const SPLIT_AT: u16 = 92;
const DELETED: Color = Color::Rgb(116, 117, 131);

fn fg(color: Color) -> Style {
    Style::default().fg(color)
}
fn bold(color: Color) -> Style {
    fg(color).add_modifier(Modifier::BOLD)
}
fn count(n: usize, noun: &str) -> String {
    format!("{n} {noun}{}", if n == 1 { "" } else { "s" })
}
fn file_line(icons: IconMode, change: &backend::Change, width: usize) -> Line<'static> {
    // Use the version being committed for the tint.
    let color = if change.ignored() {
        DIM
    } else if change.conflict {
        RED
    } else {
        match if change.staged() {
            change.xy[0]
        } else {
            change.xy[1]
        } {
            b'A' => GREEN,
            b'D' => DELETED,
            b'?' => RED,
            _ => BLUE,
        }
    };
    let name = change.label();
    let (glyph, tint) = if change.directory() {
        (icons.folder(false), DIM)
    } else {
        icons.file(&change.path)
    };
    let mut line = Line::default();
    if name.width() + glyph.width() < width {
        line.push_span(Span::styled(glyph, fg(tint)));
        line.push_span(" ");
    }
    line.push_span(Span::styled(fit(&name, width - line.width()), fg(color)));
    line
}
impl CommitPanel {
    pub(super) fn hit_button(
        &mut self,
        f: &mut Frame,
        bar: Rect,
        x: &mut u16,
        txt: &str,
        hit: Hit,
    ) {
        let w = txt.width() + 2;
        if w > usize::from(bar.right().saturating_sub(*x)) || bar.is_empty() {
            return;
        }
        let area = Rect::new(*x, bar.y, w as u16, 1);
        let disabled = self.add_running()
            && matches!(hit, Hit::MessageAction(_) | Hit::StageHunks | Hit::Refresh);
        let style = if disabled {
            button_style(false).fg(Color::DarkGray)
        } else {
            match hit {
                Hit::MessageAction(action) => {
                    let primary = if self.pane_focused(PaneId::Message) && !self.message_editing() {
                        self.message_action
                    } else {
                        MessageAction::Commit
                    };
                    button_style(action == primary)
                }
                Hit::EditSave => button_style(true),
                Hit::EditCancel => button_style(false),
                Hit::TogglePreview if self.preview_open => fg(BLUE).bg(TAG),
                Hit::ViewOptions if self.view_menu.is_some() => fg(BLUE).bg(TAG),
                Hit::Directories if self.options.directories => fg(BLUE).bg(TAG),
                Hit::Status if self.options.status => fg(BLUE).bg(TAG),
                Hit::UndoCheck if self.last_check.is_empty() => fg(Color::DarkGray),
                _ => fg(DIM),
            }
        };
        let shortcut = match hit {
            Hit::MessageAction(action)
                if !disabled
                    && self.pane_focused(PaneId::Message)
                    && !self.editing()
                    && !self.execution_running()
                    && (action != MessageAction::Generate || !self.ai_running()) =>
            {
                Some(action.shortcut()).filter(|key| self.message_mnemonics.contains(key))
            }
            _ => None,
        };
        let label = if let Some(index) =
            shortcut.and_then(|key| txt.find(|ch: char| ch.to_ascii_lowercase() == key))
        {
            Line::from(vec![
                Span::raw(format!(" {}", &txt[..index])),
                Span::styled(
                    txt[index..index + 1].to_string(),
                    fg(GOLD).add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
                ),
                Span::raw(format!("{} ", &txt[index + 1..])),
            ])
        } else {
            Line::raw(format!(" {txt} "))
        };
        f.render_widget(Paragraph::new(label).style(style), area);
        if !disabled {
            self.hits.push((area, hit));
        }
        *x = area.right().saturating_add(1);
    }

    pub(super) fn draw_workbench(&mut self, f: &mut Frame, area: Rect, c: &Context) {
        let [changes, message, preview] = self.workbench_areas(area);
        if !changes.is_empty() || !message.is_empty() {
            self.draw_sidebar(f, changes, message, c);
        }
        if !preview.is_empty() {
            let inner = pane_frame(f, preview, self.pane_focused(PaneId::Preview));
            self.draw_preview(f, inner, c, self.stacked);
        }
    }

    pub(super) fn workbench_areas(&mut self, area: Rect) -> [Rect; 3] {
        self.panes.begin();
        self.hits.clear();
        self.tree = Rect::default();
        self.diff = Rect::default();
        self.edit_view.0 = Rect::default();
        self.stacked = area.width < SPLIT_AT;
        let [sidebar, preview] = if self.panes.maximized {
            if self.focus == Focus::Diff {
                [Rect::default(), area]
            } else {
                [area, Rect::default()]
            }
        } else {
            if !self.preview_open || (self.stacked && self.focus != Focus::Diff) {
                [area, Rect::default()]
            } else if self.stacked {
                [Rect::default(), area]
            } else {
                self.panes.divide(
                    SplitId::CommitPreview,
                    area,
                    Axis::Width,
                    (u32::from(area.width) * 54 / 100) as u16,
                    [34, 24],
                )
            }
        };
        let [changes, message] = if self.panes.maximized {
            if self.focus == Focus::Message {
                [Rect::default(), sidebar]
            } else {
                [sidebar, Rect::default()]
            }
        } else if sidebar.is_empty() {
            [Rect::default(); 2]
        } else {
            let message_height = match sidebar.height {
                0..=6 => 2,
                7..=13 => 3,
                _ => 5,
            };
            self.panes.divide(
                SplitId::CommitMessage,
                sidebar,
                Axis::Height,
                sidebar.height.saturating_sub(message_height),
                [2, 2],
            )
        };
        self.panes.add(PaneId::Changes, changes);
        if self.preview_open {
            self.panes.add(PaneId::Preview, preview);
        }
        self.panes.add(PaneId::Message, message);
        if message.is_empty() {
            self.message_selection.reset();
        }
        [changes, message, preview]
    }

    fn draw_sidebar(&mut self, f: &mut Frame, changes: Rect, message: Rect, c: &Context) {
        let mut list = pane_frame(f, changes, self.pane_focused(PaneId::Changes));
        let composer = pane_frame(f, message, self.pane_focused(PaneId::Message));
        let last_row = slice(changes, changes.height.saturating_sub(1));
        let rule = if changes.is_empty() || message.is_empty() {
            Rect::default()
        } else if list.bottom() < changes.bottom() {
            // Only the real facing borders can resize; a compact composer's
            // first row contains the editable shared message.
            if composer.y == message.y {
                self.panes.handle(SplitId::CommitMessage, last_row);
            }
            last_row
        } else if composer.y > message.y {
            // Short panes only have side borders. Use the adjacent top border instead.
            let edge = slice(message, 0);
            self.panes.handle(SplitId::CommitMessage, edge);
            edge
        } else if changes.height > 2 {
            list.height -= 1;
            self.panes.handle(SplitId::CommitMessage, last_row);
            word(
                f,
                last_row,
                &"─".repeat(last_row.width.into()),
                fg(if self.pane_focused(PaneId::Changes) {
                    BLUE
                } else {
                    EDGE
                }),
            );
            last_row
        } else {
            // At the minimum terminal size the toolbar and the only Changes row
            // need both lines; keep the drag handle on the frame, outside the row.
            let grip = Rect {
                width: last_row.width.min(1),
                ..last_row
            };
            self.panes.handle(SplitId::CommitMessage, grip);
            if !grip.is_empty() {
                f.buffer_mut()[(grip.x, grip.y)].set_symbol("└");
            }
            Rect::default()
        }
        .inner(Margin::new(1, 0));
        let [bar, tree] =
            Layout::vertical([Constraint::Length(1), Constraint::Fill(1)]).areas(list);
        self.tree = tree;
        let tools_end = self.draw_tools(f, bar);
        if !tree.is_empty() {
            self.offset = self
                .offset
                .min(self.cursor)
                .min(self.view.len().saturating_sub(tree.height as usize))
                .max((self.cursor + 1).saturating_sub(tree.height as usize));
        }
        for (i, row) in self
            .view
            .iter()
            .enumerate()
            .skip(self.offset)
            .take(tree.height as usize)
        {
            let r = slice(tree, (i - self.offset) as u16);
            let model = self.changes_tree();
            let style = if i == self.cursor {
                fg(INK).bg(if model.has_merge_conflict(row) {
                    CONFLICT_SELECTION
                } else {
                    SELECT
                })
            } else {
                fg(INK)
            };
            f.render_widget(
                Paragraph::new(model.row_line(
                    row,
                    c,
                    r.width,
                    monochrome(),
                    self.changes_search.field(&row.id, 0),
                ))
                .style(style),
                r,
            );
            if row.kind == Kind::Conflicts {
                let resolve = model.resolve_control(row, r);
                if !resolve.is_empty() {
                    self.hits.push((resolve, Hit::ResolveConflicts));
                }
            }
            self.hits.push((r, Hit::Row(i)));
        }

        let wrap_actions = composer.width < 52 && composer.height >= 3;
        let [message, actions] = Layout::vertical([
            Constraint::Fill(1),
            Constraint::Length(if wrap_actions { 2 } else { 1 }),
        ])
        .areas(composer);
        let draft = message;
        let msg = self.saved.message.as_ref().filter(|s| !s.trim().is_empty());
        if self.message_editing() {
            self.draw_input(f, draft, false);
        } else {
            self.edit_view = (draft, 0, 0);
            self.message_selection.render(
                f,
                draft,
                TextLayout::plain(msg.map_or("", String::as_str), draft.width, Wrap::Clip),
                0,
                0,
            );
            if msg.is_none() {
                word(f, slice(draft, 0), "Commit message…", fg(DIM));
            }
        }
        self.hits.push((draft, Hit::Msg));

        let compact_actions = composer.width < 52 && !wrap_actions;
        let short_actions = compact_actions && composer.width < 42;
        let mut action_row = slice(actions, 0);
        let mut x = action_row.x;
        self.hit_button(
            f,
            action_row,
            &mut x,
            "Commit",
            Hit::MessageAction(MessageAction::Commit),
        );
        self.hit_button(
            f,
            action_row,
            &mut x,
            if short_actions {
                "Cmt and Push"
            } else if compact_actions {
                "Commit and Push…"
            } else {
                "Commit and Push..."
            },
            Hit::MessageAction(MessageAction::CommitAndPush),
        );
        if wrap_actions {
            action_row = slice(actions, 1);
            x = action_row.x;
        }
        let ai = if self.ai_running() {
            if short_actions {
                "AI…"
            } else if self.ai_progress().width() + 4
                > usize::from(action_row.right().saturating_sub(x))
            {
                "Generating…"
            } else {
                self.ai_progress()
            }
        } else if short_actions {
            "Gen"
        } else if compact_actions {
            "Generate"
        } else {
            "Generate message"
        };
        let glyph = if self.ai_running() {
            self.ai_spinner().to_string()
        } else {
            self.icons.glyph(Icon::AutoResolve).to_string()
        };
        let ai = format!("{glyph} {ai}");
        self.hit_button(
            f,
            action_row,
            &mut x,
            &ai,
            Hit::MessageAction(MessageAction::Generate),
        );
        if self.add_running() {
            if !rule.is_empty() {
                let progress = self.add_progress(usize::from(rule.width));
                word(
                    f,
                    rule,
                    &format!("{progress:width$}", width = usize::from(rule.width)),
                    fg(BLUE).bg(PANEL),
                );
            }
            return;
        }
        let selected = self.selection(c);
        let root_count = selected
            .iter()
            .map(|v| v.root)
            .collect::<std::collections::BTreeSet<_>>()
            .len();
        let excluded = self
            .files
            .iter()
            .filter(|v| {
                v.change.selectable()
                    && !c.scope.contains(&v.root)
                    && self.checks.get(&v.key) == Some(&true)
            })
            .count();
        let mut status = format!(
            "{} selected · {}",
            count(selected.len(), "file"),
            count(root_count, "root")
        );
        if excluded != 0 {
            status.push_str(&format!(" · {excluded} excluded"));
        }
        if self.checked_repos(c).iter().any(|r| self.amend.contains(r)) {
            status.push_str(" · Amend");
        }
        if self.execution_running() {
            status = self.execution.progress.clone();
        }
        // Leave the scope visible even when the compact action row is occupied.
        let w = status.width() as u16;
        let r = if action_row.right().saturating_sub(x) > w {
            Rect::new(action_row.right() - w, action_row.y, w, 1)
        } else if bar.right().saturating_sub(tools_end) > w {
            Rect::new(bar.right() - w, bar.y, w, bar.height)
        } else {
            let w = w.min(rule.width);
            Rect::new(rule.right() - w, rule.y, w, rule.height)
        };
        word(f, r, &fit(&status, usize::from(r.width)), fg(DIM));
        if let Some(hint) = self.hover {
            let free = bar.right().saturating_sub(tools_end);
            let r = if usize::from(free) >= hint.width() {
                Rect::new(tools_end, bar.y, free, 1)
            } else {
                rule
            };
            word(f, r, &fit(hint, usize::from(r.width)), fg(INK).bg(PANEL));
        }
    }

    fn draw_tools(&mut self, f: &mut Frame, bar: Rect) -> u16 {
        let mut x = bar.x;
        if bar.is_empty() {
            return x;
        }
        for (icon, action) in [
            (Icon::Refresh, Hit::Refresh),
            (Icon::Undo, Hit::UndoCheck),
            (Icon::Move, Hit::Move),
            (Icon::Add, Hit::New),
            (Icon::Preview, Hit::ViewOptions),
            (Icon::Diff, Hit::TogglePreview),
            (Icon::ExpandAll, Hit::Expand(true)),
            (Icon::CollapseAll, Hit::Expand(false)),
        ] {
            let label = self.icons.glyph(icon);
            if matches!(action, Hit::ViewOptions | Hit::Expand(true)) {
                let divider = self.icons.glyph(Icon::Divider);
                if divider.width() + label.width() + 2 > usize::from(bar.right().saturating_sub(x))
                {
                    break;
                }
                let w = divider.width() as u16;
                word(f, Rect::new(x, bar.y, w, 1), divider, fg(EDGE));
                x += w;
            }
            self.hit_button(f, bar, &mut x, label, action);
        }
        self.hit_button(f, bar, &mut x, "Hunks", Hit::StageHunks);
        x
    }

    fn draw_preview(&mut self, f: &mut Frame, area: Rect, c: &Context, compact: bool) {
        let [header, body] =
            Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).areas(area);
        self.diff = body;
        self.ensure_preview(c);
        let file = self.active().and_then(Row::file);
        let title = if file.is_some() {
            "Diff preview"
        } else {
            "Overview"
        };
        word(
            f,
            header,
            title,
            bold(if self.pane_focused(PaneId::Preview) {
                BLUE
            } else {
                INK
            }),
        );
        let close = if compact {
            format!("{} Tree", self.icons.glyph(Icon::Back))
        } else {
            self.icons.glyph(Icon::Close).to_owned()
        };
        let mut x = header
            .right()
            .saturating_sub(close.width() as u16 + 2)
            .max(header.x);
        let right = x.saturating_sub(1);
        self.hit_button(f, header, &mut x, &close, Hit::Show(Focus::Tree));
        let Some(row) = self.view.get(self.cursor) else {
            return;
        };
        if let Some(i) = file {
            let v = &self.files[i];
            let source = if v.change.ignored() {
                "Ignored · read-only".to_owned()
            } else if v.change.conflict {
                "Conflict".to_owned()
            } else if v.change.xy == *b"??" {
                "Untracked".to_owned()
            } else if v.change.staged() {
                format!("HEAD {} INDEX", self.icons.glyph(Icon::Right))
            } else {
                "Working tree".to_owned()
            };
            let tag = format!(" {} · {source} ", String::from_utf8_lossy(&v.change.xy));
            let w = tag.width() as u16;
            if right > header.x + title.width() as u16 + w {
                word(
                    f,
                    Rect::new(right - w, header.y, w, 1),
                    &tag,
                    fg(if v.change.conflict { RED } else { DIM }).bg(TAG),
                );
            }
            let repo = &c.repos[v.root];
            let (file_icon, tint) = if v.change.directory() {
                (self.icons.folder(false), DIM)
            } else {
                self.icons.file(&v.change.path)
            };
            let details = vec![Line::from(vec![
                Span::styled(
                    format!("{} {}  /  ", self.icons.repo(repo.submodule), repo.name),
                    fg(repo_tint(v.root)),
                ),
                Span::styled(format!("{file_icon} "), fg(tint)),
                Span::styled(v.change.label(), fg(INK)),
            ])];
            if let Some(comparison) = self.diff_comparison.as_mut() {
                let mut layout = preview_layout(body.width, details);
                let height = (layout.rows.len() as u16).min(body.height.saturating_sub(4));
                let [details, comparison_area] =
                    Layout::vertical([Constraint::Length(height), Constraint::Min(0)]).areas(body);
                // File identity stays visible above both source revisions.
                for (offset, row) in layout.rows.drain(..usize::from(height)).enumerate() {
                    f.render_widget(
                        Paragraph::new(row.line),
                        Rect::new(details.x, details.y + offset as u16, details.width, 1),
                    );
                }
                comparison.draw_inline(f, comparison_area, self.icons);
            } else {
                self.draw_patch(f, body, details);
            }
        } else {
            let (checked, _) = self.check_counts(row);
            let conflicts = row
                .files
                .iter()
                .filter(|&&i| self.files[i].change.conflict)
                .count();
            let ignored = row
                .files
                .iter()
                .filter(|&&i| self.files[i].change.ignored())
                .count();
            let mut meta = vec![
                Span::styled(format!("{}  ", row.label), bold(INK)),
                Span::styled(
                    format!("{} · {checked} selected", count(row.files.len(), "file")),
                    fg(DIM),
                ),
            ];
            if conflicts > 0 {
                meta.push(Span::styled(
                    format!(" · {}", count(conflicts, "conflict")),
                    fg(RED),
                ));
            }
            if ignored > 0 {
                meta.push(Span::styled(
                    format!(" · {ignored} ignored · read-only"),
                    fg(DIM),
                ));
            }
            if let Kind::Repo(r) = row.kind {
                meta.push(Span::styled(
                    format!(" · on {}", c.repos[r].branch),
                    fg(DIM),
                ));
            }
            let mut details = vec![Line::from(meta)];
            match &row.kind {
                Kind::Repo(r) => details.push(Line::styled(
                    format!("Worktree  {}", super::display_path(&c.repos[*r].path)),
                    fg(DIM),
                )),
                Kind::Directory(path) if !self.options.repository_grouping() => {
                    details.push(Line::styled(
                        format!(
                            "Workspace  {}",
                            super::display_path(&super::tree::directory_root(c).join(path))
                        ),
                        fg(DIM),
                    ));
                }
                Kind::Directory(path) => {
                    let root = self.files[row.files[0]].root;
                    details.push(Line::from(vec![
                        Span::styled(
                            format!(
                                "{} {}  /  ",
                                self.icons.repo(c.repos[root].submodule),
                                c.repos[root].name
                            ),
                            fg(repo_tint(root)),
                        ),
                        Span::styled(super::display_path(path), fg(INK)),
                    ]));
                }
                _ => {}
            }
            if row.files.is_empty() {
                preview_body(
                    f,
                    body,
                    &mut self.diff_scroll,
                    &mut self.diff_selection,
                    details,
                    std::iter::once(Line::styled("No changes in this changelist.", fg(DIM))),
                );
                return;
            }
            let groups = row
                .files
                .chunk_by(|a, b| self.files[*a].root == self.files[*b].root);
            let lines = groups
                .flat_map(|files| {
                    let root = self.files[files[0]].root;
                    std::iter::once((root, None, files.len()))
                        .chain(files.iter().map(move |&i| (root, Some(i), 0)))
                })
                .map(|(root, file, n)| {
                    if let Some(i) = file {
                        let v = &self.files[i];
                        let mut line = file_line(
                            self.icons,
                            &v.change,
                            usize::from(body.width.saturating_sub(2)),
                        );
                        line.spans.insert(0, Span::raw("  "));
                        line
                    } else {
                        Line::from(vec![
                            Span::styled(
                                format!("{} ", self.icons.repo(c.repos[root].submodule)),
                                fg(repo_tint(root)),
                            ),
                            Span::styled(c.repos[root].name.clone(), bold(INK)),
                            Span::styled(format!("  {}", count(n, "file")), fg(DIM)),
                        ])
                    }
                });
            preview_body(
                f,
                body,
                &mut self.diff_scroll,
                &mut self.diff_selection,
                details,
                lines,
            );
        }
    }

    fn draw_patch(&mut self, f: &mut Frame, area: Rect, details: Vec<Line<'_>>) {
        if area.is_empty() {
            return;
        }
        let width = usize::from(area.width);
        let mut layout = preview_layout(area.width, details);
        let first = layout.rows.len();
        let lines = self.diff_text.split('\n');
        let height = usize::from(area.height);
        if let Some(hit) = self.diff_match.take() {
            if let Some(text) = lines.clone().nth(hit.row) {
                let row = first + hit.row;
                self.diff_scroll = self
                    .diff_scroll
                    .min(row)
                    .max((row + 1).saturating_sub(height));
                let cells = hit.cells(text);
                if self.diff_left > cells.start {
                    self.diff_left = cells.start.saturating_sub(width / 4);
                }
                self.diff_left = self
                    .diff_left
                    .max(cells.end.saturating_sub(width).min(cells.start));
            }
        }
        self.diff_scroll = self
            .diff_scroll
            .min((first + lines.clone().count()).saturating_sub(height));
        self.diff_line_at_top = self.diff_scroll.checked_sub(first);
        let visible = self.diff_scroll..self.diff_scroll + height;
        let mut widest = width;
        for (i, text) in lines.enumerate() {
            let line = diff_line(text);
            if self.diff_left != 0 {
                widest = widest.max(text.width());
            }
            if self.diff_search.prompt().visible() && visible.contains(&(first + i)) {
                let spans = self.diff_search.field(&i, 0).spans(text, line.style);
                layout.push(Line::from(spans), area.width, Wrap::Clip);
            } else {
                layout.push(line, area.width, Wrap::Clip);
            }
        }
        self.diff_left = self.diff_left.min(widest.saturating_sub(width));
        self.diff_selection
            .render(f, area, layout, self.diff_scroll, self.diff_left);
    }
}

fn preview_layout(width: u16, details: Vec<Line<'_>>) -> TextLayout {
    let mut layout = TextLayout::default();
    for line in details {
        layout.push(line, width, Wrap::Character);
    }
    layout.decoration(Line::styled("─".repeat(width.into()), fg(EDGE)));
    layout
}

// Keep long paths readable with the same scroll controls as the patch, even on a short terminal.
fn preview_body<'a>(
    f: &mut Frame,
    area: Rect,
    scroll: &mut usize,
    selection: &mut TextSelection,
    details: Vec<Line<'a>>,
    content: impl Iterator<Item = Line<'a>>,
) {
    if area.width == 0 {
        return;
    }
    let mut layout = preview_layout(area.width, details);
    for line in content {
        layout.push(line, area.width, Wrap::Clip);
    }
    let max = layout.rows.len().saturating_sub(usize::from(area.height));
    *scroll = (*scroll).min(max);
    selection.render(f, area, layout, *scroll, 0);
}
