//! One repository's target and tracking intent are edited together, without Git writes.
use super::*;
use crate::ui::idea::{button_style, clipped};
use unicode_width::UnicodeWidthStr;

pub(super) struct Editor {
    pub idx: usize,
    pub text: String,
    pub set_upstream: bool,
}

impl PushPanel {
    pub(super) fn sets_upstream(&self, entry: &Entry) -> bool {
        self.tracking_choices
            .get(&entry.idx)
            .is_some_and(|choice| choice.branch == entry.source && choice.target == entry.t)
    }

    pub(super) fn target_event(&mut self, event: &Event, c: &Context) -> Option<Action> {
        let hit = if let Event::Mouse(m) = event {
            (m.kind == MouseEventKind::Down(MouseButton::Left))
                .then(|| self.hit_at(m.column, m.row))
                .flatten()
        } else {
            None
        };
        let editor = self.target_edit.as_mut()?;
        let mut save = matches!(hit, Some(Hit::SaveTarget));
        let mut cancel = matches!(hit, Some(Hit::CancelTarget));
        if matches!(hit, Some(Hit::Upstream)) {
            editor.set_upstream = !editor.set_upstream;
        }
        match event {
            Event::Paste(s) => editor.text.push_str(&s.replace(['\r', '\n'], "")),
            Event::Key(k) => match k.code {
                KeyCode::Esc => cancel = true,
                KeyCode::Enter => save = true,
                KeyCode::Tab | KeyCode::BackTab => editor.set_upstream = !editor.set_upstream,
                KeyCode::Backspace => {
                    editor.text.pop();
                }
                KeyCode::Char('u') if k.modifiers.contains(KeyModifiers::CONTROL) => {
                    editor.text.clear()
                }
                KeyCode::Char(ch)
                    if !k
                        .modifiers
                        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
                {
                    editor.text.push(ch)
                }
                _ => (),
            },
            _ => (),
        }
        if cancel {
            self.target_edit = None;
        } else if save {
            let idx = editor.idx;
            if !c.scope.contains(&idx) {
                return Some(Action::Notice(
                    NoticeLevel::Warning,
                    "Roots changed; cancel and review the target again".into(),
                ));
            }
            let Some((remote, dst)) = editor.text.split_once(':') else {
                return Some(Action::Notice(
                    NoticeLevel::Warning,
                    "Use a configured remote and valid remote:branch".into(),
                ));
            };
            if remote.starts_with('-')
                || remote == "."
                || !remote_list(&c.repos[idx].path).iter().any(|r| r == remote)
                || git(
                    &c.repos[idx].path,
                    &["check-ref-format", &format!("refs/heads/{dst}")],
                )
                .is_err()
            {
                return Some(Action::Notice(
                    NoticeLevel::Warning,
                    "Use a configured remote and valid remote:branch".into(),
                ));
            }
            let target = Target {
                remote: remote.into(),
                dst: dst.into(),
            };
            if editor.set_upstream {
                let Some(entry) = self.entries.iter().find(|e| e.idx == idx) else {
                    return Some(Action::Notice(
                        NoticeLevel::Warning,
                        "Repository changed; cancel and refresh".into(),
                    ));
                };
                self.tracking_choices.insert(
                    idx,
                    TrackingChoice {
                        branch: entry.source.clone(),
                        target: target.clone(),
                    },
                );
            } else {
                self.tracking_choices.remove(&idx);
            }
            self.targets.insert(idx, target);
            self.target_edit = None;
            self.inspect(c);
            let _ = self.refresh(c);
            return Some(Action::Notice(
                NoticeLevel::Success,
                "Push target and tracking choice updated; review before pushing".into(),
            ));
        }
        Some(Action::None)
    }

    pub(super) fn draw_target_editor(&mut self, f: &mut Frame, area: Rect, c: &Context) {
        let Some(editor) = &self.target_edit else {
            return;
        };
        let width = area.width.saturating_sub(2).min(74);
        let height = area.height.min(8);
        if width < 2 || height < 3 {
            return;
        }
        let popup = Rect::new(
            area.x + (area.width - width) / 2,
            area.y + (area.height - height) / 2,
            width,
            height,
        );
        f.render_widget(Clear, popup);
        // Keep covered pane controls from receiving clicks while editing.
        self.hits.push((popup, Hit::TargetSurface));
        let block = pane(
            format!("Target · {}", label(Path::new(&c.repos[editor.idx].name))),
            self.panes.active,
        );
        let inner = block.inner(popup);
        f.render_widget(block, popup);
        let input = Rect::new(inner.x, inner.y, inner.width, 1);
        let text = format!("{}{}", editor.text, self.icons.glyph(Icon::Cursor));
        let scroll = text
            .width()
            .saturating_sub(inner.width.into())
            .min(u16::MAX as usize) as u16;
        f.render_widget(
            Paragraph::new(text)
                .scroll((0, scroll))
                .style(Style::new().fg(ACCENT)),
            input,
        );
        self.hits.push((input, Hit::TargetField));
        let lines = [
            self.entries
                .iter()
                .find(|e| e.idx == editor.idx)
                .map(|e| format!("Current upstream: {}", e.config.description(&e.source)))
                .unwrap_or_default(),
            format!(
                "{} Set upstream after push  [Tab]",
                self.icons.check(editor.set_upstream)
            ),
            "Unchecked keeps current upstream.".into(),
        ];
        for (i, text) in lines.into_iter().enumerate() {
            let y = inner.y + i as u16 + 1;
            if y >= inner.bottom().saturating_sub(1) {
                break;
            }
            let row = Rect::new(inner.x, y, inner.width, 1);
            f.render_widget(
                Paragraph::new(clipped(&text, inner.width.into()))
                    .style(Style::new().fg(if i == 1 { FG } else { MUTED })),
                row,
            );
            if i == 1 {
                self.hits.push((row, Hit::Upstream));
            }
        }
        if inner.height >= 2 {
            let mut right = inner.right();
            for (label, hit, primary) in [
                (" Save Enter ", Hit::SaveTarget, true),
                (" Cancel Esc ", Hit::CancelTarget, false),
            ] {
                let width = label.width().min((right - inner.x).into()) as u16;
                let row = Rect::new(right - width, inner.bottom() - 1, width, 1);
                f.render_widget(Paragraph::new(label).style(button_style(primary)), row);
                if !row.is_empty() {
                    self.hits.push((row, hit));
                }
                right = row.x.saturating_sub(1).max(inner.x);
            }
        }
    }
}
