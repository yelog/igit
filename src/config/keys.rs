//! Normal-mode command identities. Execution stays in the existing panel handlers.
use crate::ui::panes::Direction;
use anyhow::{bail, Result};
use crossterm::event::{KeyCode as K, KeyEvent, KeyModifiers as M};

pub type Sequence = Vec<KeyEvent>;
pub const LOG_GRAPH: u16 = 1;
pub const LOG_REFS: u16 = 2;
pub const LOG_FILES: u16 = 4;
pub const LOG_INFO: u16 = 8;
pub const LOG: u16 = 15;
pub const CHANGES: u16 = 16;
pub const MESSAGE: u16 = 32;
pub const PREVIEW: u16 = 64;
pub const COMMIT: u16 = 112;
pub const PUSH_REPOS: u16 = 128;
pub const PUSH_FILES: u16 = 256;
pub const PUSH_DETAILS: u16 = 512;
pub const PUSH: u16 = 896;
pub const MERGE_FILES: u16 = 1024;
pub const MERGE_DOC: u16 = 2048;
pub const MERGE: u16 = 3072;
pub const ALL: u16 = 4095;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommandAction {
    Keys(&'static str),
    SmartFocus(Direction),
    SmartResize(Direction),
}

impl CommandAction {
    pub fn keys(self) -> Option<&'static str> {
        match self {
            Self::Keys(keys) => Some(keys),
            Self::SmartFocus(_) | Self::SmartResize(_) => None,
        }
    }

    pub fn is_smart_pane(self) -> bool {
        matches!(self, Self::SmartFocus(_) | Self::SmartResize(_))
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::SmartFocus(Direction::Left) => "Focus left pane or Kitty window",
            Self::SmartFocus(Direction::Down) => "Focus pane or Kitty window below",
            Self::SmartFocus(Direction::Up) => "Focus pane or Kitty window above",
            Self::SmartFocus(Direction::Right) => "Focus right pane or Kitty window",
            Self::SmartResize(Direction::Left) => "Move divider left or resize Kitty window",
            Self::SmartResize(Direction::Down) => "Move divider down or resize Kitty window",
            Self::SmartResize(Direction::Up) => "Move divider up or resize Kitty window",
            Self::SmartResize(Direction::Right) => "Move divider right or resize Kitty window",
            Self::Keys(_) => "",
        }
    }
}

#[derive(Debug)]
pub struct Command {
    pub group: &'static str,
    pub name: &'static str,
    pub action: CommandAction,
    pub contexts: u16,
}

macro_rules! commands {
    (@action $keys:literal) => { CommandAction::Keys($keys) };
    (@action $direction:ident) => { CommandAction::SmartFocus(Direction::$direction) };
    (@action (resize $direction:ident)) => { CommandAction::SmartResize(Direction::$direction) };
    ($($group:literal { $($name:literal => $target:tt, $scope:expr;)* })*) => {
        pub static COMMANDS: &[Command] = &[$($(Command {
            group: $group, name: $name, action: commands!(@action $target), contexts: $scope,
        },)*)*];
    };
}

commands! {
    "global" {
        "focus-log" => "1", ALL;
        "focus-commit" => "2", ALL;
        "open-push" => "3", ALL;
        "open-merge" => "4", ALL;
        "roots" => "r", ALL;
        "actions" => ":", ALL;
        "refresh" => "F5", ALL;
        "help" => "?", ALL;
        "quit" => "q", ALL;
    }
    "panes" {
        "focus-next" => "Tab", ALL;
        "focus-previous" => "Shift-Tab", ALL;
        "focus-left" => "Ctrl-w h", ALL;
        "focus-down" => "Ctrl-w j", ALL;
        "focus-up" => "Ctrl-w k", ALL;
        "focus-right" => "Ctrl-w l", ALL;
        "smart-focus-pane-left" => Left, ALL;
        "smart-focus-pane-down" => Down, ALL;
        "smart-focus-pane-up" => Up, ALL;
        "smart-focus-pane-right" => Right, ALL;
        "smart-resize-pane-left" => (resize Left), ALL;
        "smart-resize-pane-down" => (resize Down), ALL;
        "smart-resize-pane-up" => (resize Up), ALL;
        "smart-resize-pane-right" => (resize Right), ALL;
        "cycle" => "Ctrl-w w", ALL;
        "increase-width" => "Ctrl-w >", ALL;
        "decrease-width" => "Ctrl-w <", ALL;
        "increase-height" => "Ctrl-w +", ALL;
        "decrease-height" => "Ctrl-w -", ALL;
        "reset-sizes" => "Ctrl-w =", ALL;
        "close-log-tab" => "Ctrl-w c", LOG;
    }
    "log" {
        "move-down" => "j", LOG;
        "move-up" => "k", LOG;
        "move-left" => "h", LOG;
        "move-right" => "l", LOG;
        "first-commit" => "gg", LOG_GRAPH;
        "first-item" => "g", LOG_REFS | LOG_FILES | LOG_INFO;
        "last-item" => "G", LOG;
        "page-down" => "PgDn", LOG;
        "page-up" => "PgUp", LOG;
        "half-page-down" => "Ctrl-d", LOG_GRAPH;
        "half-page-up" => "Ctrl-u", LOG_GRAPH;
        "screen-top" => "H", LOG_GRAPH;
        "screen-middle" => "M", LOG_GRAPH;
        "screen-bottom" => "L", LOG_GRAPH;
        "copy-id" => "yy", LOG_GRAPH;
        "copy-message" => "ym", LOG_GRAPH;
        "copy-author" => "ya", LOG_GRAPH;
        "open" => "Enter", LOG;
        "search" => "/", LOG;
        "branch-filter" => "b", LOG;
        "user-filter" => "a", LOG;
        "date-filter" => "d", LOG;
        "paths-filter" => "p", LOG;
        "clear-filters" => "x", LOG;
        "view-options" => "o", LOG;
        "toggle-refs" => "B", LOG;
        "toggle-inspector" => "I", LOG;
        "toggle-current-highlight" => "H", LOG_REFS | LOG_FILES | LOG_INFO;
        "toggle-reference-labels" => "L", LOG_REFS | LOG_FILES | LOG_INFO;
        "select-ref" => "Space", LOG_REFS;
        "previous-tab" => "[", LOG;
        "next-tab" => "]", LOG;
        "new-tab" => "Ctrl-t", LOG;
        "load-more" => "+", LOG;
        "collapse-all" => "z", LOG_FILES;
        "expand-all" => "Z", LOG_FILES;
    }
    "commit" {
        "move-down" => "j", COMMIT;
        "move-up" => "k", COMMIT;
        "collapse" => "h", CHANGES;
        "expand" => "l", CHANGES;
        "first-item" => "g", COMMIT;
        "last-item" => "G", COMMIT;
        "page-down" => "PgDn", COMMIT;
        "page-up" => "PgUp", COMMIT;
        "toggle" => "Space", CHANGES;
        "open" => "Enter", COMMIT;
        "move-to-group" => "m", COMMIT;
        "preview" => "v", COMMIT;
        "view-options" => "o", COMMIT;
        "toggle-directories" => "t", COMMIT;
        "toggle-status" => "s", COMMIT;
        "collapse-all" => "z", COMMIT;
        "expand-all" => "Z", COMMIT;
        "undo-selection" => "u", COMMIT;
        "new-group" => "n", COMMIT;
        "edit-message" => "i", COMMIT;
        "previous-repo" => "[", COMMIT;
        "next-repo" => "]", COMMIT;
        "amend" => "a", COMMIT;
        "ai-draft" => "A", COMMIT;
        "ai-settings" => "S", COMMIT;
        "review" => "c", COMMIT;
        "review-and-push" => "P", COMMIT;
    }
    "push" {
        "move-down" => "j", PUSH;
        "move-up" => "k", PUSH;
        "collapse" => "h", PUSH_REPOS | PUSH_FILES;
        "expand" => "l", PUSH_REPOS | PUSH_FILES;
        "open" => "Enter", PUSH_REPOS | PUSH_FILES;
        "toggle" => "Space", PUSH_REPOS;
        "edit-target" => "t", PUSH_REPOS;
        "view-options" => "o", PUSH;
        "review" => "p", PUSH;
        "retry" => "R", PUSH;
        "fetch" => "f", PUSH;
        "follow-tags" => "v", PUSH;
        "skip-hooks" => "x", PUSH;
        "details" => "d", PUSH;
        "collapse-all" => "[", PUSH_FILES;
        "expand-all" => "]", PUSH_FILES;
    }
    "merge" {
        "move-down" => "j", MERGE;
        "move-up" => "k", MERGE;
        "first-item" => "g", MERGE_FILES;
        "last-item" => "G", MERGE_FILES;
        "open" => "Enter", MERGE_FILES;
        "continue" => "C", MERGE_FILES;
        "previous-conflict" => "[c", MERGE_DOC;
        "next-conflict" => "]c", MERGE_DOC;
        "focus-left" => "h", MERGE_DOC;
        "focus-right" => "l", MERGE_DOC;
        "scroll-left" => "Left", MERGE_DOC;
        "scroll-right" => "Right", MERGE_DOC;
        "page-down" => "PgDn", MERGE_DOC;
        "page-up" => "PgUp", MERGE_DOC;
        "zoom" => "z", MERGE_DOC;
        "take-left" => "H", MERGE_DOC;
        "take-right" => "L", MERGE_DOC;
        "take-both" => "b", MERGE_DOC;
        "auto-resolve" => "a", MERGE_DOC;
        "edit" => "i", MERGE_DOC;
        "undo" => "u", MERGE_DOC;
        "save-draft" => "s", MERGE_DOC;
        "apply" => "A", MERGE_DOC;
        "back-to-files" => "f", MERGE_DOC;
    }
}

pub fn context(title: &str) -> u16 {
    match title {
        "Log / Commits" => LOG_GRAPH,
        "Log / Branch Tree" => LOG_REFS,
        "Log / File Tree" => LOG_FILES,
        "Log / Commit Details" => LOG_INFO,
        "Commit / Changes" => CHANGES,
        "Commit / Message" => MESSAGE,
        "Commit / Preview" => PREVIEW,
        "Push / Repos and Commits" => PUSH_REPOS,
        "Push / Changed Files" => PUSH_FILES,
        "Push / Diff and Details" => PUSH_DETAILS,
        "Merge / Conflicted Files" => MERGE_FILES,
        "Merge / Left Source" | "Merge / Result" | "Merge / Right Source" => MERGE_DOC,
        _ => 0,
    }
}

/// Terminals encode Shift+letters as uppercase characters; BackTab has two encodings.
pub fn normalize(mut key: KeyEvent) -> KeyEvent {
    if let K::Char(ch) = key.code {
        if key.modifiers.contains(M::SHIFT) {
            if ch.is_ascii_lowercase() {
                key.code = K::Char(ch.to_ascii_uppercase());
            }
            key.modifiers.remove(M::SHIFT);
        }
    }
    if key.code == K::Tab && key.modifiers.contains(M::SHIFT) {
        key.code = K::BackTab;
    }
    if key.code == K::BackTab {
        key.modifiers.remove(M::SHIFT);
    }
    KeyEvent::new(key.code, key.modifiers)
}

pub fn parse_sequence(value: &str) -> Result<Sequence> {
    let sequence = value
        .split_whitespace()
        .map(parse_key)
        .collect::<Result<Vec<_>>>()?;
    if sequence.is_empty() || sequence.len() > 4 {
        bail!("Key sequence must contain 1 to 4 keys: {value:?}");
    }
    Ok(sequence)
}

pub fn catalogue_sequence(value: &str) -> Sequence {
    let expanded = match value {
        "gg" => "g g",
        "yy" => "y y",
        "ym" => "y m",
        "ya" => "y a",
        "[c" => "[ c",
        "]c" => "] c",
        other => other,
    };
    parse_sequence(expanded).expect("valid built-in command")
}

fn parse_key(value: &str) -> Result<KeyEvent> {
    let mut rest = value;
    let mut modifiers = M::NONE;
    while rest.chars().count() > 1 {
        let Some(index) = rest.find(['-', '+']) else {
            break;
        };
        let modifier = match rest[..index].to_ascii_lowercase().as_str() {
            "ctrl" | "control" => M::CONTROL,
            "alt" | "option" => M::ALT,
            "shift" => M::SHIFT,
            "super" | "cmd" | "command" => M::SUPER,
            _ => bail!("Unknown key modifier in {value:?}"),
        };
        if modifiers.contains(modifier) {
            bail!("Repeated modifier in {value:?}");
        }
        modifiers |= modifier;
        rest = &rest[index + 1..];
    }
    let code = match rest.to_ascii_lowercase().as_str() {
        "space" => K::Char(' '),
        "enter" | "return" => K::Enter,
        "esc" | "escape" => K::Esc,
        "tab" => K::Tab,
        "backtab" => K::BackTab,
        "up" => K::Up,
        "down" => K::Down,
        "left" => K::Left,
        "right" => K::Right,
        "home" => K::Home,
        "end" => K::End,
        "pgup" | "pageup" => K::PageUp,
        "pgdn" | "pagedown" => K::PageDown,
        "backspace" => K::Backspace,
        "delete" | "del" => K::Delete,
        "insert" | "ins" => K::Insert,
        name if name.starts_with('f') && name.len() > 1 => K::F(
            name[1..]
                .parse::<u8>()
                .ok()
                .filter(|n| (1..=24).contains(n))
                .ok_or_else(|| anyhow::anyhow!("Invalid function key {value:?}"))?,
        ),
        _ if rest.chars().count() == 1 => K::Char(rest.chars().next().unwrap()),
        _ => bail!("Unknown key {value:?}; separate sequence steps with spaces (for example g g)"),
    };
    Ok(normalize(KeyEvent::new(code, modifiers)))
}

pub fn label(sequence: &[KeyEvent]) -> String {
    sequence
        .iter()
        .map(|key| {
            let mut text = String::new();
            for (flag, name) in [
                (M::CONTROL, "Ctrl-"),
                (M::ALT, "Alt-"),
                (M::SUPER, "Super-"),
                (M::SHIFT, "Shift-"),
            ] {
                if key.modifiers.contains(flag) {
                    text.push_str(name);
                }
            }
            text.push_str(&match key.code {
                K::Char(' ') => "Space".into(),
                K::Char(ch) => ch.to_string(),
                K::BackTab => "Shift-Tab".into(),
                K::F(n) => format!("F{n}"),
                K::PageUp => "PgUp".into(),
                K::PageDown => "PgDn".into(),
                other => format!("{other:?}"),
            });
            text
        })
        .collect::<Vec<_>>()
        .join(" ")
}
