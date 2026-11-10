mod history;
mod view;

#[cfg(test)]
mod tests;

use super::{
    icons::{Icon, IconMode},
    idea,
};
use crossterm::event::{Event, MouseButton, MouseEventKind};
use ratatui::{layout::Rect, style::Color};
use std::{
    collections::VecDeque,
    fmt::{self, Write},
    path::Path,
    sync::OnceLock,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

pub(crate) use history::{History, HistoryEvent};

const HISTORY_CAPACITY: usize = 500;
const LIVE_CAPACITY: usize = 4;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Level {
    Info,
    Success,
    Warning,
    Error,
}

impl Level {
    fn ttl(self) -> Duration {
        Duration::from_secs(match self {
            Self::Success => 3,
            Self::Info => 4,
            Self::Warning => 6,
            Self::Error => 8,
        })
    }

    fn color(self) -> Color {
        match self {
            Self::Info => idea::BLUE,
            Self::Success => idea::GREEN,
            Self::Warning => idea::GOLD,
            Self::Error => idea::RED,
        }
    }

    fn icon(self, icons: IconMode) -> &'static str {
        match self {
            Self::Info => "i",
            Self::Success => icons.glyph(Icon::Success),
            Self::Warning => icons.glyph(Icon::Warning),
            Self::Error => icons.glyph(Icon::Error),
        }
    }
}

impl fmt::Display for Level {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Info => "info",
            Self::Success => "success",
            Self::Warning => "warning",
            Self::Error => "error",
        })
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Notification {
    pub(crate) id: u64,
    pub(crate) level: Level,
    pub(crate) source: String,
    pub(crate) body: String,
    created_at: u64,
    utc_offset: Option<i32>,
}

impl Notification {
    fn matches(&self, query: &str) -> bool {
        let query = query.to_lowercase();
        [
            self.source.as_str(),
            self.body.as_str(),
            &self.level.to_string(),
            &timestamp(self.created_at, self.utc_offset),
        ]
        .iter()
        .any(|text| text.to_lowercase().contains(&query))
    }
}

pub(crate) struct Live {
    pub(crate) id: u64,
    pub(crate) expires_at: Instant,
}

#[derive(Clone, Copy)]
struct LiveHit {
    id: u64,
    area: Rect,
    close: Rect,
}

pub(crate) struct Center {
    history: VecDeque<Notification>,
    live: Vec<Live>,
    next_id: u64,
    hits: Vec<LiveHit>,
    captured_button: Option<MouseButton>,
    utc_offset: Option<i32>,
}

impl Default for Center {
    fn default() -> Self {
        static OFFSET: OnceLock<Option<i32>> = OnceLock::new();
        Self {
            history: VecDeque::new(),
            live: Vec::new(),
            next_id: 0,
            hits: Vec::new(),
            captured_button: None,
            utc_offset: *OFFSET.get_or_init(local_offset),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LiveEvent {
    Pass,
    Handled,
    Open(u64),
}

impl Center {
    pub(crate) fn push(
        &mut self,
        level: Level,
        source: impl Into<String>,
        body: impl Into<String>,
    ) -> u64 {
        self.push_at(level, source.into(), body.into(), Instant::now())
    }

    fn push_at(&mut self, level: Level, source: String, body: String, now: Instant) -> u64 {
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        self.history.push_front(Notification {
            id,
            level,
            source: sanitize(&source).replace('\n', " "),
            body: sanitize(&body),
            created_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            utc_offset: self.utc_offset,
        });
        self.history.truncate(HISTORY_CAPACITY);
        self.live.insert(
            0,
            Live {
                id,
                expires_at: now + level.ttl(),
            },
        );
        self.live.truncate(LIVE_CAPACITY);
        id
    }

    pub(crate) fn expire(&mut self, now: Instant) -> bool {
        let before = self.live.len();
        self.live.retain(|live| live.expires_at > now);
        before != self.live.len()
    }

    pub(crate) fn dismiss_all_live(&mut self) -> bool {
        let changed = !self.live.is_empty();
        self.live.clear();
        changed
    }

    pub(crate) fn history(&self) -> impl DoubleEndedIterator<Item = &Notification> {
        self.history.iter()
    }

    #[cfg(test)]
    pub(crate) fn latest_body(&self) -> &str {
        self.history.front().map_or("", |entry| entry.body.as_str())
    }

    pub(crate) fn live(&self) -> &[Live] {
        &self.live
    }

    pub(crate) fn clear_hits(&mut self) {
        self.hits.clear();
    }

    pub(crate) fn contains(&self, x: u16, y: u16) -> bool {
        self.hits.iter().any(|hit| super::inside(hit.area, x, y))
    }

    pub(crate) fn live_event(&mut self, event: &Event, can_open: bool) -> LiveEvent {
        let Event::Mouse(mouse) = event else {
            if matches!(event, Event::FocusLost) {
                self.captured_button = None;
            }
            if matches!(event, Event::Resize(..)) {
                self.hits.clear();
            }
            return LiveEvent::Pass;
        };
        if let Some(button) = self.captured_button {
            match mouse.kind {
                MouseEventKind::Up(released) if released == button => {
                    self.captured_button = None;
                    return LiveEvent::Handled;
                }
                MouseEventKind::Drag(_) | MouseEventKind::Moved => return LiveEvent::Handled,
                _ => {}
            }
        }
        let Some(hit) = self
            .hits
            .iter()
            .find(|hit| super::inside(hit.area, mouse.column, mouse.row))
            .copied()
        else {
            return LiveEvent::Pass;
        };
        if let MouseEventKind::Down(button) = mouse.kind {
            self.captured_button = Some(button);
            if button == MouseButton::Left && self.live.iter().any(|live| live.id == hit.id) {
                if super::inside(hit.close, mouse.column, mouse.row) {
                    self.live.retain(|live| live.id != hit.id);
                } else if can_open && self.get(hit.id).is_some() {
                    return LiveEvent::Open(hit.id);
                }
            }
        }
        LiveEvent::Handled
    }

    fn get(&self, id: u64) -> Option<&Notification> {
        self.history.iter().find(|entry| entry.id == id)
    }
}

fn sanitize(value: &str) -> String {
    let mut safe = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '\n' => safe.push('\n'),
            '\t' => safe.push_str("    "),
            '\u{1b}' => safe.push_str("<ESC>"),
            '\r' => safe.push_str("<CR>"),
            character if character.is_control() => {
                let _ = write!(safe, "<0x{:02X}>", character as u32);
            }
            character => safe.push(character),
        }
    }
    safe
}

fn local_offset() -> Option<i32> {
    let output = crate::core::command(Path::new("."))
        .env("GIT_AUTHOR_NAME", "IGit")
        .env("GIT_AUTHOR_EMAIL", "clock@invalid")
        .env_remove("GIT_AUTHOR_DATE")
        .args(["var", "GIT_AUTHOR_IDENT"])
        .output()
        .ok()?;
    output.status.success().then_some(())?;
    offset_from_ident(std::str::from_utf8(&output.stdout).ok()?)
}

fn offset_from_ident(ident: &str) -> Option<i32> {
    let zone = ident.split_whitespace().last()?;
    if zone.len() != 5
        || !matches!(zone.as_bytes()[0], b'+' | b'-')
        || !zone.as_bytes()[1..].iter().all(u8::is_ascii_digit)
    {
        return None;
    }
    let hours: i32 = zone.get(1..3)?.parse().ok()?;
    let minutes: i32 = zone.get(3..5)?.parse().ok()?;
    if hours > 23 || minutes > 59 {
        return None;
    }
    let seconds = (hours * 60 + minutes) * 60;
    Some(if zone.starts_with('-') {
        -seconds
    } else {
        seconds
    })
}

fn local_seconds(seconds: u64, utc_offset: Option<i32>) -> i64 {
    (seconds as i64).saturating_add(i64::from(utc_offset.unwrap_or(0)))
}

fn clock(seconds: u64, utc_offset: Option<i32>) -> String {
    let seconds = local_seconds(seconds, utc_offset).rem_euclid(86_400);
    let value = format!(
        "{:02}:{:02}:{:02}",
        seconds / 3600,
        seconds / 60 % 60,
        seconds % 60
    );
    if utc_offset.is_some() {
        value
    } else {
        format!("{value} UTC")
    }
}

fn timestamp(seconds: u64, utc_offset: Option<i32>) -> String {
    let epoch_day = local_seconds(seconds, utc_offset).div_euclid(86_400) + 719_468;
    let day_in_cycle = epoch_day.rem_euclid(146_097);
    let year_in_cycle =
        (day_in_cycle - day_in_cycle / 1460 + day_in_cycle / 36_524 - day_in_cycle / 146_096) / 365;
    let day_in_year =
        day_in_cycle - (365 * year_in_cycle + year_in_cycle / 4 - year_in_cycle / 100);
    let month_in_year = (5 * day_in_year + 2) / 153;
    let day = day_in_year - (153 * month_in_year + 2) / 5 + 1;
    let month = if month_in_year < 10 {
        month_in_year + 3
    } else {
        month_in_year - 9
    };
    let year = epoch_day.div_euclid(146_097) * 400 + year_in_cycle + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02} {}",
        clock(seconds, utc_offset)
    )
}
