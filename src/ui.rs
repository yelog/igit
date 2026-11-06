pub(crate) mod clipboard;
pub(crate) mod help;
pub(crate) mod icons;
pub(crate) mod idea;
pub(crate) mod key_sequence;
pub(crate) mod panes;
pub(crate) mod selection;
use crate::core::Context;
use anyhow::Result;
use crossterm::event::Event;
use ratatui::{
    layout::Rect,
    style::{Color, Style},
    widgets::{Block, Borders},
    Frame,
};

pub enum Action {
    None,
    Notice(String),
    Copy {
        text: String,
        label: &'static str,
    },
    Refresh(String),
    OpenConflicts,
    CloseDialog,
    ReviewRollback,
    Confirm {
        title: String,
        body: String,
        token: String,
    },
    Committed {
        repos: Vec<usize>,
        push: bool,
        message: String,
    },
}
pub trait Panel {
    fn refresh(&mut self, c: &Context) -> Result<()>;
    fn draw(&mut self, f: &mut Frame, area: Rect, c: &Context);
    fn event(&mut self, e: &Event, c: &Context) -> Action;
    fn tick(&mut self, _c: &Context) -> Action {
        Action::None
    }
    fn editing(&self) -> bool {
        false
    }
    fn selected_text(&self) -> Option<&str> {
        None
    }
    fn help(&self) -> &'static str;
    fn confirm(&mut self, _token: &str, _c: &Context) -> Action {
        Action::None
    }
}
pub fn inside(r: Rect, x: u16, y: u16) -> bool {
    x >= r.x && x < r.right() && y >= r.y && y < r.bottom()
}
pub const BG: Color = idea::PANEL;
pub const FG: Color = idea::INK;
pub const MUTED: Color = idea::DIM;
pub const ACCENT: Color = idea::BLUE;
pub const LINE: Color = idea::EDGE;
pub const SEL: Color = idea::SELECT;
pub fn pane(title: impl Into<String>, focus: bool) -> Block<'static> {
    Block::default()
        .style(Style::new().fg(FG).bg(BG))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(if focus { ACCENT } else { LINE }))
        .title_style(Style::default().fg(if focus { ACCENT } else { FG }))
        .title(format!(" {} ", title.into()))
}
