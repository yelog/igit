use super::{Operation, Review};
use crate::{
    branch::BranchPanel,
    ui::{
        idea::{repo_tint, GOLD, LOCAL_REF, REMOTE_REF},
        review::Dialog,
        selection::{TextLayout, Wrap},
        ACCENT, FG, MUTED,
    },
};
use ratatui::{
    layout::Rect,
    style::{Style, Stylize},
    text::{Line, Span},
    Frame,
};

impl BranchPanel {
    pub(crate) fn draw_review(
        &self,
        frame: &mut Frame,
        area: Rect,
        scroll: &mut u16,
        confirm_focused: bool,
        hits: &mut Vec<(Rect, usize)>,
    ) -> bool {
        let Some(review) = self.review.as_ref().filter(|review| {
            matches!(
                review.operation,
                Operation::Merge | Operation::Pull | Operation::Push
            )
        }) else {
            return false;
        };
        Dialog {
            title: review.operation.title(),
            icons: self.icons,
            letter_keys: matches!(review.operation, Operation::Pull | Operation::Push),
        }
        .draw(
            frame,
            area,
            |width| review.content(width),
            scroll,
            confirm_focused,
            hits,
        );
        true
    }
}

impl Review {
    fn content(&self, width: u16) -> TextLayout {
        let mut content = TextLayout::default();
        let muted = Style::new().fg(MUTED);
        let mut put = |line| {
            content.push(line, width, Wrap::Word);
        };
        let pulling = self.operation == Operation::Pull;
        put(Line::from(vec![
            Span::styled("From  ", muted),
            Span::styled(
                if pulling {
                    self.sync.as_ref().unwrap().target_label()
                } else {
                    self.source.name.clone()
                },
                Style::new()
                    .fg(if pulling || self.source.remote {
                        REMOTE_REF
                    } else {
                        LOCAL_REF
                    })
                    .bold(),
            ),
        ]));
        if !pulling {
            put(Line::styled(
                format!(
                    "      {}",
                    &self.source.oid[..self.source.oid.len().min(12)]
                ),
                muted,
            ));
        }
        if matches!(self.operation, Operation::Pull | Operation::Push) {
            put(Line::from(vec![
                Span::styled("To    ", muted),
                Span::styled(
                    if pulling {
                        self.source.name.clone()
                    } else {
                        self.sync.as_ref().unwrap().target_label()
                    },
                    Style::new()
                        .fg(if pulling { LOCAL_REF } else { REMOTE_REF })
                        .bold(),
                ),
            ]));
            if pulling {
                put(Line::styled(
                    format!(
                        "      {}",
                        &self.source.oid[..self.source.oid.len().min(12)]
                    ),
                    muted,
                ));
            }
            put(Line::default());
            put(Line::from(vec![
                Span::styled("Fast-forward only", Style::new().fg(GOLD).bold()),
                Span::styled(if pulling { "" } else { "  No force push" }, muted),
            ]));
            put(Line::styled(
                if pulling && self.source.current {
                    "Updates the current worktree; Git protects local changes."
                } else {
                    "Keeps your current checkout and local changes."
                },
                muted,
            ));
            put(Line::default());
            put(Line::from(vec![
                Span::styled("Repository  ", muted),
                Span::styled(
                    &self.repository,
                    Style::new().fg(repo_tint(self.root)).bold(),
                ),
            ]));
            put(Line::styled(self.path.display().to_string(), muted));
            if !self.source.current {
                put(Line::default());
                put(Line::from(vec![
                    Span::styled("Checkout  ", muted),
                    Span::styled(self.snapshot.head_label(), Style::new().fg(ACCENT)),
                ]));
                let current = self.snapshot.oid.as_deref().unwrap_or("(no commits)");
                put(Line::styled(
                    format!("          {}", &current[..current.len().min(12)]),
                    muted,
                ));
            }
            return content;
        }
        put(Line::from(vec![
            Span::styled("Into  ", muted),
            Span::styled(self.snapshot.head_label(), Style::new().fg(ACCENT).bold()),
            Span::styled(" (current)", muted),
        ]));
        let current = self.snapshot.oid.as_deref().unwrap_or("(no commits)");
        put(Line::styled(
            format!("      {}", &current[..current.len().min(12)]),
            muted,
        ));
        put(Line::default());
        put(Line::from(vec![
            Span::styled("Repository  ", muted),
            Span::styled(
                &self.repository,
                Style::new().fg(repo_tint(self.root)).bold(),
            ),
        ]));
        put(Line::styled(self.path.display().to_string(), muted));
        put(Line::default());
        put(Line::styled("What happens", Style::new().fg(FG).bold()));
        put(Line::from("Git may fast-forward or create a merge commit."));
        put(Line::styled(
            "Resolve any conflicts in the Conflicts tab.",
            muted,
        ));
        content
    }
}
