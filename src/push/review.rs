use super::*;
use crate::ui::{
    idea::{repo_tint, GOLD, LOCAL_REF, REMOTE_REF},
    review::Dialog,
};
use ratatui::style::Stylize;

impl PushPanel {
    pub(crate) fn draw_review(
        &self,
        frame: &mut Frame,
        area: Rect,
        context: &Context,
        scroll: &mut u16,
        confirm_focused: bool,
        hits: &mut Vec<(Rect, usize)>,
    ) -> bool {
        if self.review.is_empty() {
            return false;
        }
        let count = self.review.len();
        Dialog {
            title: &format!(
                "Push {count} {}",
                if count == 1 {
                    "repository"
                } else {
                    "repositories"
                }
            ),
            icons: self.icons,
            letter_keys: false,
        }
        .draw(
            frame,
            area,
            |width| self.review_content(context, width),
            scroll,
            confirm_focused,
            hits,
        );
        true
    }

    fn review_content(&self, context: &Context, width: u16) -> TextLayout {
        let mut content = TextLayout::default();
        let muted = Style::new().fg(MUTED);
        let mut put = |line| {
            content.push(line, width, TextWrap::Word);
        };
        for (index, job) in self.review.iter().enumerate() {
            if index > 0 {
                put(Line::default());
            }
            put(Line::from(vec![
                Span::styled("From  ", muted),
                Span::styled(&job.branch, Style::new().fg(LOCAL_REF).bold()),
            ]));
            let mut commit = format!("      {}", &job.head[..job.head.len().min(12)]);
            if let Some(entry) = self
                .entries
                .iter()
                .find(|entry| entry.idx == job.idx && entry.head == job.head)
            {
                commit.push_str(&format!(
                    "  · {} commit{}",
                    entry.n,
                    if entry.n == 1 { "" } else { "s" }
                ));
            }
            put(Line::styled(commit, muted));
            put(Line::from(vec![
                Span::styled("To    ", muted),
                Span::styled(
                    format!("{}/{}", job.t.remote, job.t.dst),
                    Style::new().fg(REMOTE_REF).bold(),
                ),
            ]));
            let mut upstream = vec![
                Span::styled(
                    format!("Upstream  {} ", job.config.description(&job.branch)),
                    muted,
                ),
                Span::styled(self.icons.glyph(Icon::Right), muted),
                Span::styled(" ", muted),
            ];
            if job.set_upstream {
                upstream.push(Span::styled(
                    format!("{}:{}", job.t.remote, job.t.dst),
                    Style::new().fg(REMOTE_REF).bold(),
                ));
                upstream.push(Span::styled(" after push succeeds", muted));
            } else {
                upstream.push(Span::styled("keep unchanged", muted));
            }
            put(Line::from(upstream));
            put(Line::default());
            put(Line::from(vec![
                Span::styled("Repository  ", muted),
                Span::styled(
                    context
                        .repos
                        .get(job.idx)
                        .map(|repo| repo.name.clone())
                        .unwrap_or_else(|| label(&job.path)),
                    Style::new().fg(repo_tint(job.idx)).bold(),
                ),
            ]));
            put(Line::styled(job.path.display().to_string(), muted));
        }
        let strategy = self.review_strategy.unwrap_or(self.strategy);
        put(Line::default());
        put(Line::from(vec![
            Span::styled(
                format!("Update: {}", strategy.label()),
                Style::new().fg(GOLD).bold(),
            ),
            Span::styled("  No force push", muted),
        ]));
        put(Line::styled(
            "Fetch targets, update when needed, then push.",
            muted,
        ));
        put(Line::styled(
            if strategy == Strategy::Rebase {
                "Rebase rewrites local commits and preserves merge topology."
            } else {
                "Merge preserves local commits; may create a merge commit."
            },
            muted,
        ));
        put(Line::styled(
            "On conflicts: stop, resolve, continue, then review Push again.",
            muted,
        ));
        put(Line::default());
        put(Line::from(vec![
            Span::styled("Hooks  ", muted),
            Span::styled(
                if self.skip_hooks {
                    "Skipped"
                } else {
                    "Enabled"
                },
                Style::new().fg(if self.skip_hooks { GOLD } else { FG }),
            ),
            Span::styled("    Follow tags  ", muted),
            Span::styled(
                if self.follow_tags {
                    "Enabled"
                } else {
                    "Disabled"
                },
                Style::new().fg(if self.follow_tags { FG } else { MUTED }),
            ),
            Span::styled(" (annotated)", muted),
        ]));
        put(Line::styled(
            "Each repository runs separately; successes are retained.",
            muted,
        ));
        content
    }
}
