use super::{display_path, rows::monochrome, slice, word, CommitPanel};
use crate::ui::{
    idea::{DIM, EDGE, INK, TAG},
    panes::pane_frame,
};
use ratatui::{layout::Rect, style::Style, Frame};
use std::path::Path;

fn skeleton(f: &mut Frame, area: Rect, indent: u16, width: u16) {
    let indent = indent.min(area.width);
    let bar = Rect::new(
        area.x + indent,
        area.y,
        width.min(area.width - indent),
        area.height.min(1),
    );
    let fill = if monochrome() { "-" } else { " " };
    word(
        f,
        bar,
        &fill.repeat(usize::from(bar.width)),
        Style::new().fg(DIM).bg(EDGE),
    );
}

impl CommitPanel {
    pub(crate) fn draw_loading(
        &mut self,
        f: &mut Frame,
        area: Rect,
        path: &Path,
        status: &str,
        detail: &str,
    ) {
        let [changes, message, preview] = self.workbench_areas(area);
        let list = pane_frame(f, changes, true);
        let composer = pane_frame(f, message, false);
        let preview = pane_frame(f, preview, false);
        word(f, slice(list, 0), "Changes", Style::new().fg(INK));
        word(f, slice(list, 1), status, Style::new().fg(DIM));
        if list.height > 4 {
            word(f, slice(list, 3), &display_path(path), Style::new().fg(DIM));
            word(f, slice(list, 4), detail, Style::new().fg(DIM));
        }
        for (offset, indent, width) in [
            (6, 0, 22),
            (8, 2, 30),
            (10, 4, 18),
            (12, 4, 25),
            (14, 2, 24),
            (16, 4, 16),
        ] {
            skeleton(f, slice(list, offset), indent, width);
        }
        word(
            f,
            slice(composer, 0),
            "Commit message…",
            Style::new().fg(DIM),
        );
        let wrap = composer.width < 52 && composer.height >= 3;
        let actions = slice(
            composer,
            composer.height.saturating_sub(if wrap { 2 } else { 1 }),
        );
        let label = if composer.width < 42 && !wrap {
            " Commit   Commit+Push   AI "
        } else if composer.width < 52 && !wrap {
            " Commit   Commit & Push…   Generate "
        } else if wrap {
            " Commit   Commit and Push... "
        } else {
            " Commit   Commit and Push...   Generate message "
        };
        word(f, actions, label, Style::new().fg(DIM).bg(TAG));
        if wrap {
            word(
                f,
                slice(composer, composer.height.saturating_sub(1)),
                " Generate message ",
                Style::new().fg(DIM).bg(TAG),
            );
        }
        word(f, slice(preview, 0), "Diff preview", Style::new().fg(INK));
        word(
            f,
            slice(preview, 2),
            "Waiting for changes",
            Style::new().fg(DIM),
        );
        for (offset, width) in [(5, 28), (7, 36), (9, 23), (11, 31)] {
            skeleton(f, slice(preview, offset), 0, width);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::{icons::IconMode, Panel};
    use ratatui::{backend::TestBackend, Terminal};

    #[test]
    fn loading_workbench_never_registers_file_or_write_actions() {
        for icons in [IconMode::Ascii, IconMode::Unicode, IconMode::NerdFont] {
            let mut panel = CommitPanel {
                icons,
                ..CommitPanel::default()
            };
            for (width, height) in [(120, 34), (92, 22), (80, 22), (44, 10)] {
                let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
                terminal
                    .draw(|frame| {
                        panel.draw_loading(
                            frame,
                            frame.area(),
                            Path::new("/workspace/模块"),
                            "Loading changes...",
                            "2 roots found",
                        );
                    })
                    .unwrap();
                assert!(panel.hits.is_empty());
                assert!(!panel.editing());
            }
        }
    }
}
