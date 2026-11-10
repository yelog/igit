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
pub const BRANCH: u16 = 4096;
pub const STASH_LIST: u16 = 8192;
pub const STASH_FILES: u16 = 16384;
pub const STASH: u16 = STASH_LIST | STASH_FILES;
pub const ALL: u16 = 32767;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommandAction {
    Keys(&'static str),
    SmartFocus(Direction),
    SmartResize(Direction),
    SmartMaximize,
}

impl CommandAction {
    pub fn keys(self) -> Option<&'static str> {
        match self {
            Self::Keys(keys) => Some(keys),
            Self::SmartFocus(_) | Self::SmartResize(_) | Self::SmartMaximize => None,
        }
    }

    pub fn is_smart_pane(self) -> bool {
        matches!(
            self,
            Self::SmartFocus(_) | Self::SmartResize(_) | Self::SmartMaximize
        )
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
            Self::SmartMaximize => "Maximize / restore pane and Kitty layout",
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
    (@action (maximize)) => { CommandAction::SmartMaximize };
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
        "focus-commit" => "1", ALL;
        "focus-log" => "2", ALL;
        "open-push" => "3", ALL;
        "focus-branch" => "4", ALL;
        "focus-stash" => "5", ALL;
        "open-merge" => "6", ALL;
        "pull" => "p", ALL;
        "fetch" => "f", ALL;
        "update" => "F9", ALL;
        "roots" => "r", ALL & !BRANCH;
        "actions" => ":", ALL;
        "refresh" => "F5", ALL;
        "help" => "?", ALL;
        "quit" => "q", ALL;
    }
    "stash" {
        "find" => "/", STASH;
        "next-match" => "n", STASH;
        "previous-match" => "N", STASH;
        "apply" => "a", STASH;
        "pop" => "P", STASH;
        "drop" => "d", STASH;
        "move-down" => "j", STASH;
        "move-up" => "k", STASH;
        "collapse" => "h", STASH;
        "expand" => "l", STASH;
        "toggle" => "Space", STASH;
        "open" => "Enter", STASH;
        "first-item" => "g", STASH;
        "last-item" => "G", STASH;
        "page-down" => "PgDn", STASH;
        "page-up" => "PgUp", STASH;
        "collapse-all" => "z", STASH_FILES;
        "expand-all" => "Z", STASH_FILES;
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
        "smart-toggle-pane-maximized" => (maximize), ALL;
        "toggle-pane-maximized" => "Ctrl-w f", ALL & !LOG;
        "cycle" => "Ctrl-w w", ALL;
        "increase-width" => "Ctrl-w >", ALL;
        "decrease-width" => "Ctrl-w <", ALL;
        "increase-height" => "Ctrl-w +", ALL;
        "decrease-height" => "Ctrl-w -", ALL;
        "reset-sizes" => "Ctrl-w =", ALL;
        "close-log-tab" => "Ctrl-w c", LOG;
    }
    "branch" {
        "find" => "/", BRANCH;
        "next-match" => "n", BRANCH;
        "previous-match" => "N", BRANCH;
        "move-down" => "j", BRANCH;
        "move-up" => "k", BRANCH;
        "collapse" => "h", BRANCH;
        "expand" => "l", BRANCH;
        "toggle" => "Space", BRANCH;
        "first-item" => "gg", BRANCH;
        "last-item" => "G", BRANCH;
        "page-down" => "PgDn", BRANCH;
        "page-up" => "PgUp", BRANCH;
        "half-page-down" => "Ctrl-d", BRANCH;
        "half-page-up" => "Ctrl-u", BRANCH;
        "screen-top" => "H", BRANCH;
        "screen-middle" => "M", BRANCH;
        "screen-bottom" => "L", BRANCH;
        "view-options" => "o", BRANCH;
        "create" => "a", BRANCH;
        "delete" => "d", BRANCH;
        "checkout" => "Enter", BRANCH;
        "merge" => "m", BRANCH;
        "rename" => "r", BRANCH;
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
        "commit-actions" => "a", LOG_GRAPH;
        "edit-message" => "F2", LOG_GRAPH;
        "open" => "Enter", LOG;
        "search" => "/", LOG_GRAPH | LOG_INFO;
        "find" => "/", LOG_REFS | LOG_FILES;
        "next-match" => "n", LOG_REFS | LOG_FILES;
        "previous-match" => "N", LOG_REFS | LOG_FILES;
        "search-refs" => "S", LOG_REFS;
        "branch-filter" => "Space b", LOG;
        "user-filter" => "Space u", LOG;
        "date-filter" => "Space d", LOG;
        "paths-filter" => "Space p", LOG;
        "clear-filters" => "x", LOG;
        "view-options" => "o", LOG;
        "toggle-refs" => "B", LOG;
        "toggle-inspector" => "I", LOG;
        "toggle-current-highlight" => "H", LOG_REFS | LOG_FILES | LOG_INFO;
        "toggle-reference-labels" => "L", LOG_REFS | LOG_FILES | LOG_INFO;
        "select-ref" => "s", LOG_REFS;
        "previous-tab" => "[", LOG;
        "next-tab" => "]", LOG;
        "new-tab" => "Ctrl-t", LOG;
        "load-more" => "+", LOG;
        "collapse-all" => "z", LOG_FILES;
        "expand-all" => "Z", LOG_FILES;
    }
    "commit" {
        "find" => "/", CHANGES | PREVIEW;
        "next-match" => "n", CHANGES | PREVIEW;
        "previous-match" => "N", CHANGES | PREVIEW;
        "scroll-preview-left" => "h", PREVIEW;
        "scroll-preview-right" => "l", PREVIEW;
        "move-down" => "j", COMMIT;
        "move-up" => "k", COMMIT;
        "collapse" => "h", CHANGES;
        "expand" => "l", CHANGES;
        "first-item" => "g", COMMIT;
        "last-item" => "G", COMMIT;
        "page-down" => "PgDn", COMMIT;
        "page-up" => "PgUp", COMMIT;
        "toggle" => "Space", CHANGES;
        "discard" => "d", CHANGES;
        "add-to-git" => "a", CHANGES;
        "edit-file" => "e", CHANGES;
        "stage-hunks" => "Ctrl-g", CHANGES | PREVIEW;
        "open" => "Enter", COMMIT;
        "move-to-group" => "m", COMMIT;
        "preview" => "v", COMMIT;
        "view-options" => "o", COMMIT;
        "toggle-directories" => "t", COMMIT;
        "toggle-status" => "T", COMMIT;
        "stash" => "s", COMMIT;
        "collapse-all" => "z", COMMIT;
        "expand-all" => "Z", COMMIT;
        "undo-selection" => "u", COMMIT;
        "new-group" => "Ctrl-n", COMMIT;
        "edit-message" => "i", COMMIT;
        "previous-repo" => "[", COMMIT;
        "next-repo" => "]", COMMIT;
        "amend" => "M", COMMIT;
        "ai-draft" => "A", COMMIT;
        "ai-settings" => "S", COMMIT;
        "review" => "c", CHANGES | PREVIEW;
        "review-and-push" => "P", COMMIT;
    }
    "push" {
        "find" => "/", PUSH_REPOS | PUSH_FILES;
        "next-match" => "n", PUSH_REPOS | PUSH_FILES;
        "previous-match" => "N", PUSH_REPOS | PUSH_FILES;
        "move-down" => "j", PUSH;
        "move-up" => "k", PUSH;
        "collapse" => "h", PUSH_REPOS | PUSH_FILES;
        "expand" => "l", PUSH_REPOS | PUSH_FILES;
        "open" => "Enter", PUSH_REPOS | PUSH_FILES;
        "toggle" => "Space", PUSH_REPOS;
        "edit-target" => "t", PUSH_REPOS;
        "view-options" => "o", PUSH;
        "review" => "P", PUSH;
        "retry" => "R", PUSH;
        "fetch" => "F", PUSH;
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
        "abort" => "B", MERGE_FILES;
        "skip" => "N", MERGE_FILES;
        "accept-current" => "Y", MERGE;
        "accept-incoming" => "T", MERGE;
        "prepare-simple" => "S", MERGE_FILES;
        "toggle-unresolved" => "Space", MERGE_FILES;
        "toggle-paths" => "o", MERGE_FILES;
        "previous-conflict" => "[c", MERGE_DOC;
        "next-conflict" => "]c", MERGE_DOC;
        "previous-change" => "Shift-F7", MERGE_DOC;
        "next-change" => "F7", MERGE_DOC;
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
        "resolve-simple" => "m", MERGE_DOC;
        "fold-context" => "o", MERGE_DOC;
        "compare" => "v", MERGE_DOC;
        "source-details" => "d", MERGE_DOC;
        "editor-settings" => "O", MERGE_DOC;
        "accept-side" => "Enter", MERGE_DOC;
        "ignore-side" => "x", MERGE_DOC;
        "edit" => "i", MERGE_DOC;
        "undo" => "u", MERGE_DOC;
        "save-draft" => "s", MERGE_DOC;
        "apply" => "A", MERGE_DOC;
        "back-to-files" => "F", MERGE_DOC;
    }
}

pub fn context(title: &str) -> u16 {
    match title {
        "Log / Commits" => LOG_GRAPH,
        "Log / Branch Tree" => LOG_REFS,
        "Log / File Tree" => LOG_FILES,
        "Log / Commit Details" | "Log / File History Diff" => LOG_INFO,
        "Commit / Changes" => CHANGES,
        "Commit / Message" => MESSAGE,
        "Commit / Preview" => PREVIEW,
        "Push / Repos and Commits" => PUSH_REPOS,
        "Push / Changed Files" => PUSH_FILES,
        "Push / Diff and Details" => PUSH_DETAILS,
        "Merge / Conflicted Files" => MERGE_FILES,
        "Merge / Left Source" | "Merge / Result" | "Merge / Right Source" => MERGE_DOC,
        "Branch / Branch Tree" | "Branch / Repositories" => BRANCH,
        "Stash / Repositories" => STASH_LIST,
        "Stash / File Tree" => STASH_FILES,
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
        "yf" => "y f",
        "yn" => "y n",
        "yr" => "y r",
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
