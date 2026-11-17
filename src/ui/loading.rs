use super::{icons::IconMode, idea::clipped, ACCENT, MUTED};
use ratatui::{
    style::Style,
    text::{Line, Span},
};
use std::time::Instant;
use unicode_width::UnicodeWidthStr;

#[derive(Default)]
pub(crate) struct Indicator {
    started: Option<Instant>,
    frame: usize,
}

impl Indicator {
    pub(crate) fn glyph(&self, icons: IconMode) -> char {
        icons.spinner(self.frame)
    }

    pub(crate) fn tick(&mut self, active: bool, now: Instant) -> bool {
        if !active {
            self.frame = 0;
            return self.started.take().is_some();
        }
        let started = *self.started.get_or_insert(now);
        let frame = (now.saturating_duration_since(started).as_millis() / 100 % 8) as usize;
        let changed = frame != self.frame;
        self.frame = frame;
        changed
    }

    pub(crate) fn line(&self, icons: IconMode, labels: &[&str], width: u16) -> Line<'static> {
        if width == 0 {
            return Line::default();
        }
        let mut spans = vec![Span::styled(
            self.glyph(icons).to_string(),
            Style::new().fg(ACCENT),
        )];
        if width > 2 {
            let available = usize::from(width - 2);
            let label = labels
                .iter()
                .find(|label| label.width() <= available)
                .or_else(|| labels.last())
                .copied()
                .unwrap_or_default();
            spans.push(Span::raw(" "));
            spans.push(Span::styled(
                clipped(label, available),
                Style::new().fg(MUTED),
            ));
        }
        Line::from(spans)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn animation_follows_elapsed_time_and_resets_when_inactive() {
        let mut indicator = Indicator::default();
        let start = Instant::now();
        assert!(!indicator.tick(false, start));
        assert!(!indicator.tick(true, start));
        assert!(!indicator.tick(true, start + Duration::from_millis(99)));
        assert!(indicator.tick(true, start + Duration::from_millis(100)));
        assert_eq!(indicator.frame, 1);
        assert!(!indicator.tick(true, start + Duration::from_millis(100)));
        assert!(indicator.tick(true, start + Duration::from_millis(799)));
        assert_eq!(indicator.frame, 7);
        assert!(indicator.tick(true, start + Duration::from_millis(800)));
        assert_eq!(indicator.frame, 0);
        assert!(indicator.tick(false, start + Duration::from_millis(801)));
        assert!(!indicator.tick(false, start + Duration::from_millis(802)));
        assert!(!indicator.tick(true, start + Duration::from_secs(5)));
        assert_eq!(indicator.frame, 0);
    }

    #[test]
    fn labels_fit_without_losing_the_mode_specific_spinner() {
        let indicator = Indicator::default();
        let labels = ["Refreshing outgoing commits…", "Refreshing…", "Refresh…"];
        for icons in [IconMode::Ascii, IconMode::Unicode, IconMode::NerdFont] {
            for width in 0..45 {
                let line = indicator.line(icons, &labels, width);
                assert!(line.width() <= usize::from(width));
                if width > 0 {
                    assert_eq!(line.spans[0].content, icons.spinner(0).to_string());
                    assert_eq!(line.spans[0].style.fg, Some(ACCENT));
                }
            }
            assert_eq!(
                indicator.line(icons, &labels, 40).to_string(),
                format!("{} {}", icons.spinner(0), labels[0])
            );
            assert_eq!(
                indicator.line(icons, &labels, 13).to_string(),
                format!("{} {}", icons.spinner(0), labels[1])
            );
            assert_eq!(
                indicator.line(icons, &labels, 10).to_string(),
                format!("{} {}", icons.spinner(0), labels[2])
            );
        }
    }
}
