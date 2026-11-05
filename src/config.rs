//! User preferences are loaded once, separately from repository/workspace state.
pub(crate) mod keys;

use crate::ui::icons::IconMode;
use anyhow::{bail, Context, Result};
use crossterm::event::{KeyCode as K, KeyModifiers as M};
use keys::{catalogue_sequence, Command, Sequence, COMMANDS};
use serde::Deserialize;
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

pub const DEFAULT_TOML: &str = include_str!("../config/default.toml");
type Group = BTreeMap<String, Vec<String>>;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Settings {
    version: u32,
    terminal: Terminal,
    ui: Ui,
    keybindings: Keybindings,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Terminal {
    mouse: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Ui {
    icons: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Keybindings {
    hint_delay_ms: u64,
    global: Group,
    panes: Group,
    log: Group,
    commit: Group,
    push: Group,
    merge: Group,
}
impl Keybindings {
    fn groups(&self) -> [(&str, &Group); 6] {
        [
            ("global", &self.global),
            ("panes", &self.panes),
            ("log", &self.log),
            ("commit", &self.commit),
            ("push", &self.push),
            ("merge", &self.merge),
        ]
    }
    fn group(&self, name: &str) -> &Group {
        self.groups()
            .into_iter()
            .find(|(n, _)| *n == name)
            .unwrap()
            .1
    }
}

#[derive(Clone, Debug)]
pub struct BoundCommand {
    pub command: &'static Command,
    pub sequences: Vec<Sequence>,
    pub defaults: Vec<Sequence>,
}
impl BoundCommand {
    pub fn changed(&self) -> bool {
        self.sequences != self.defaults
    }
}

#[derive(Clone, Debug)]
pub struct AppConfig {
    pub icons: IconMode,
    pub mouse: bool,
    pub hint_delay: Duration,
    pub commands: Vec<BoundCommand>,
}
impl Default for AppConfig {
    fn default() -> Self {
        Self::from_toml("").expect("valid embedded settings")
    }
}
impl AppConfig {
    pub fn load(path: &Path, required: bool) -> Result<Self> {
        let contents = match fs::read_to_string(path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound && !required => {
                String::new()
            }
            Err(error) => {
                return Err(error).with_context(|| format!("Read settings {}", path.display()))
            }
        };
        Self::from_toml(&contents).with_context(|| format!("Invalid settings {}", path.display()))
    }

    pub fn from_toml(overrides: &str) -> Result<Self> {
        let defaults: Settings = toml::from_str(DEFAULT_TOML)?;
        let mut merged: toml::Value = toml::from_str(DEFAULT_TOML)?;
        merge(&mut merged, toml::from_str(overrides)?);
        let settings: Settings = merged.try_into()?;
        if settings.version != 1 {
            bail!(
                "Unsupported settings version {}; expected 1",
                settings.version
            );
        }
        let icons = settings
            .ui
            .icons
            .parse()
            .map_err(|error: &str| anyhow::anyhow!("ui.icons: {error}"))?;
        if settings.keybindings.hint_delay_ms > 10_000 {
            bail!("keybindings.hint_delay_ms must be between 0 and 10000");
        }
        for (name, group) in settings.keybindings.groups() {
            for action in group.keys() {
                if !COMMANDS.iter().any(|c| c.group == name && c.name == action) {
                    bail!("Unknown command keybindings.{name}.{action}");
                }
            }
        }
        let mut commands = vec![];
        for command in COMMANDS {
            let parse = |bindings: &Keybindings| -> Result<Vec<Sequence>> {
                bindings.group(command.group)[command.name]
                    .iter()
                    .map(|text| {
                        keys::parse_sequence(text).with_context(|| {
                            format!("keybindings.{}.{}", command.group, command.name)
                        })
                    })
                    .collect()
            };
            commands.push(BoundCommand {
                command,
                sequences: parse(&settings.keybindings)?,
                defaults: parse(&defaults.keybindings)?,
            });
        }
        validate(&commands)?;
        Ok(Self {
            icons,
            mouse: settings.terminal.mouse,
            hint_delay: Duration::from_millis(settings.keybindings.hint_delay_ms),
            commands,
        })
    }

    pub fn active(&self, context: u16) -> impl Iterator<Item = &BoundCommand> {
        self.commands
            .iter()
            .filter(move |c| c.command.contexts & context != 0)
    }

    pub fn changed(&self) -> bool {
        self.commands.iter().any(BoundCommand::changed)
    }

    pub fn smart_panes_enabled(&self) -> bool {
        self.commands
            .iter()
            .any(|c| c.command.action.is_smart_pane() && !c.sequences.is_empty())
    }

    pub fn label(&self, group: &str, name: &str) -> String {
        self.commands
            .iter()
            .find(|c| c.command.group == group && c.command.name == name)
            .and_then(|c| c.sequences.first())
            .map(|s| keys::label(s))
            .unwrap_or_else(|| "—".into())
    }

    pub fn help_labels(&self, context: u16, key: &str) -> Option<Vec<String>> {
        // F1 is the fixed recovery/help key. Text selection also keeps its keys.
        if matches!(key, "F1" | "Esc" | "Ctrl-a" | "Ctrl-Insert") {
            return None;
        }
        let target = catalogue_sequence(key);
        self.active(context)
            .find(|c| {
                (c.command
                    .action
                    .keys()
                    .is_some_and(|k| catalogue_sequence(k) == target)
                    || c.defaults.contains(&target))
                    && c.changed()
            })
            .map(|c| c.sequences.iter().map(|s| keys::label(s)).collect())
    }

    pub fn shortcut_labels(&self, group: &str) -> BTreeMap<&'static str, String> {
        self.commands
            .iter()
            .filter(|c| c.changed() && (c.command.group == group || c.command.group == "global"))
            .filter_map(|c| {
                Some((
                    c.command.action.keys()?,
                    self.label(c.command.group, c.command.name),
                ))
            })
            .collect()
    }
}

fn merge(base: &mut toml::Value, overrides: toml::Value) {
    if let (toml::Value::Table(base), toml::Value::Table(overrides)) = (&mut *base, &overrides) {
        for (key, value) in overrides {
            if let Some(existing) = base.get_mut(key) {
                merge(existing, value.clone());
            } else {
                base.insert(key.clone(), value.clone());
            }
        }
    } else {
        *base = overrides;
    }
}

fn validate(commands: &[BoundCommand]) -> Result<()> {
    for (i, first) in commands.iter().enumerate() {
        for (j, sequence) in first.sequences.iter().enumerate() {
            if first.command.action.is_smart_pane()
                && (sequence.len() != 1
                    || !sequence[0]
                        .modifiers
                        .intersects(M::CONTROL | M::ALT | M::SUPER))
            {
                bail!("keybindings.{}.{}: smart pane commands require a single Ctrl, Alt or Super modified chord", first.command.group, first.command.name);
            }
            if sequence.len() > 1
                && sequence[0] == catalogue_sequence("Ctrl-w")[0]
                && matches!(sequence[1].code, K::Char('0'..='9'))
                && sequence[1].modifiers.is_empty()
            {
                bail!(
                    "keybindings.{}.{}: digits after Ctrl-w are reserved for window counts",
                    first.command.group,
                    first.command.name
                );
            }
            for key in sequence {
                let cancel = key.code == K::Char('c') && key.modifiers == M::CONTROL;
                let selection = (key.code == K::Char('a') || key.code == K::Insert)
                    && key.modifiers == M::CONTROL;
                if matches!(key.code, K::Esc | K::F(1))
                    || selection
                    || cancel && !(first.command.name == "quit" && sequence.len() == 1)
                {
                    bail!("keybindings.{}.{}: {} is reserved for help, cancellation or text selection", first.command.group, first.command.name, keys::label(&[*key]));
                }
            }
            for second in &commands[i..] {
                if first.command.contexts & second.command.contexts == 0 {
                    continue;
                }
                let start = if std::ptr::eq(first, second) {
                    j + 1
                } else {
                    0
                };
                for other in &second.sequences[start..] {
                    if sequence.starts_with(other) || other.starts_with(sequence) {
                        bail!("Conflicting keybindings.{}.{} ({}) and keybindings.{}.{} ({}): duplicate or ambiguous prefix", first.command.group, first.command.name, keys::label(sequence), second.command.group, second.command.name, keys::label(other));
                    }
                }
            }
        }
    }
    Ok(())
}

pub fn settings_path(explicit: Option<PathBuf>) -> Result<(PathBuf, bool)> {
    if let Some(path) = explicit {
        return Ok((path, true));
    }
    Ok((
        config_home(|key| std::env::var_os(key))?.join("settings.toml"),
        false,
    ))
}

fn config_home(env: impl Fn(&str) -> Option<OsString>) -> Result<PathBuf> {
    let value = |key| {
        env(key)
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
    };
    if let Some(path) = value("IGIT_CONFIG_HOME") {
        return Ok(path);
    }
    if cfg!(windows) {
        return value("APPDATA")
            .map(|p| p.join("igit"))
            .context("APPDATA is unavailable; use --config PATH or IGIT_CONFIG_HOME");
    }
    if let Some(path) = value("XDG_CONFIG_HOME") {
        return Ok(path.join("igit"));
    }
    value("HOME")
        .map(|p| p.join(".config/igit"))
        .context("HOME is unavailable; use --config PATH or IGIT_CONFIG_HOME")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn defaults_and_partial_overrides() {
        let config = AppConfig::from_toml(
            "[ui]\nicons = 'ascii'\n[keybindings.commit]\nreview = ['Ctrl-r']\n",
        )
        .unwrap();
        assert_eq!(config.icons, IconMode::Ascii);
        assert!(config.mouse);
        assert_eq!(config.hint_delay, Duration::from_millis(200));
        assert_eq!(config.label("commit", "review"), "Ctrl-r");
        assert_eq!(config.label("global", "quit"), "q");
        assert!(!AppConfig::default().changed());
        assert_eq!(config.commands.len(), COMMANDS.len());
        let disabled = AppConfig::from_toml("[keybindings.commit]\nreview = []").unwrap();
        assert_eq!(disabled.label("commit", "review"), "—");
    }
    #[test]
    fn unknown_fields_values_and_ambiguous_bindings_fail() {
        for text in [
            "version = 2",
            "typo = true",
            "[ui]\nicon = 'ascii'",
            "[ui]\nicons = 'bad'",
            "[terminal]\nmouse = 'yes'",
            "[keybindings]\nhint_delay_ms = 10001",
            "[keybindings]\nunknown = {}",
            "[keybindings.commit]\nreveiw = ['x']",
            "[keybindings.commit]\nreview = ['i']",
            "[keybindings.global]\nquit = ['j']",
            "[keybindings.commit]\nreview = ['g c']",
            "[keybindings.commit]\nreview = ['Esc']",
            "[keybindings.commit]\nreview = ['F1']",
            "[keybindings.commit]\nreview = ['x', 'x']",
            "[keybindings.commit]\nreview = ['Ctrl-a']",
            "[keybindings.commit]\nreview = ['gg']",
        ] {
            assert!(AppConfig::from_toml(text).is_err(), "{text}");
        }
        assert!(AppConfig::from_toml(
            "[keybindings.commit]\nreview = ['Ctrl-r']\n[keybindings.push]\nreview = ['Ctrl-r']"
        )
        .is_ok());
        assert!(
            AppConfig::from_toml("[keybindings.commit]\nreview = ['i']\nedit-message = ['c']")
                .is_ok()
        );
    }
    #[test]
    fn smart_focus_is_opt_in_and_requires_unambiguous_modified_chords() {
        assert!(!AppConfig::default().smart_panes_enabled());
        let config =
            AppConfig::from_toml("[keybindings.panes]\nsmart-focus-pane-left=['Cmd+Ctrl+h']")
                .unwrap();
        assert!(config.smart_panes_enabled());
        for binding in ["h", "H", "Shift-h", "Ctrl-w h", "Ctrl-w", "Ctrl-a", "F1"] {
            let text = format!("[keybindings.panes]\nsmart-focus-pane-left=['{binding}']");
            assert!(AppConfig::from_toml(&text).is_err(), "{binding}");
        }
        assert!(AppConfig::from_toml("[keybindings.panes]\nsmart-focus-pane-left=['Cmd+Ctrl+h']\nsmart-focus-pane-right=['Ctrl+Super+h']").is_err());
    }

    #[test]
    fn smart_resize_accepts_shift_chords_and_enables_resize_only_sessions() {
        let config = AppConfig::from_toml(
            "[keybindings.panes]\nsmart-resize-pane-left=['Cmd+Ctrl+Shift+h']",
        )
        .unwrap();
        assert!(config.smart_panes_enabled());
        assert_eq!(
            config.label("panes", "smart-resize-pane-left"),
            "Ctrl-Super-H"
        );
        assert_eq!(
            keys::parse_sequence("Cmd+Ctrl+Shift+h").unwrap(),
            keys::parse_sequence("Ctrl+Super+H").unwrap()
        );
        for chord in ["h", "Shift+h", "Ctrl-w h", "Ctrl-w", "F1", "Ctrl-a"] {
            assert!(
                AppConfig::from_toml(&format!(
                    "[keybindings.panes]\nsmart-resize-pane-left=['{chord}']"
                ))
                .is_err(),
                "{chord}"
            );
        }
        assert!(AppConfig::from_toml("[keybindings.panes]\nsmart-focus-pane-left=['Cmd+Ctrl+h']\nsmart-resize-pane-left=['Cmd+Ctrl+Shift+h']").is_ok());
        assert!(AppConfig::from_toml("[keybindings.panes]\nsmart-focus-pane-left=['Cmd+Ctrl+h']\nsmart-resize-pane-left=['Cmd+Ctrl+h']").is_err());
    }

    #[test]
    fn key_parser_normalizes_terminal_encodings() {
        use keys::{normalize, parse_sequence as parse};
        assert_eq!(parse("Shift-j").unwrap(), parse("J").unwrap());
        assert_eq!(parse("Control+x").unwrap(), parse("Ctrl-x").unwrap());
        assert_eq!(
            parse("Shift-Tab").unwrap(),
            vec![normalize(crossterm::event::KeyEvent::new(
                K::BackTab,
                M::SHIFT
            ))]
        );
        for text in [
            "g g",
            "Ctrl-w +",
            "Ctrl--",
            "Alt+Enter",
            "Super-h",
            "F24",
            "你",
            "Space x",
        ] {
            assert_eq!(
                parse(&keys::label(&parse(text).unwrap())).unwrap(),
                parse(text).unwrap()
            );
        }
        for text in ["", "Ctrl-", "Meta-x", "F25", "Ctrl-Ctrl-a", "a b c d e"] {
            assert!(parse(text).is_err(), "{text}");
        }
    }
    #[test]
    fn optional_missing_file_explicit_errors_and_path_precedence() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.toml");
        assert!(AppConfig::load(&path, false).is_ok());
        assert!(AppConfig::load(&path, true).is_err());
        fs::write(&path, "[ui]\nicons='ascii'").unwrap();
        assert_eq!(AppConfig::load(&path, true).unwrap().icons, IconMode::Ascii);
        fs::write(&path, "invalid = [").unwrap();
        assert!(format!("{:#}", AppConfig::load(&path, false).unwrap_err())
            .contains(path.to_str().unwrap()));
        assert!(AppConfig::load(dir.path(), false).is_err());
        assert_eq!(
            config_home(|k| (k == "IGIT_CONFIG_HOME").then(|| OsString::from("custom"))).unwrap(),
            PathBuf::from("custom")
        );
        if !cfg!(windows) {
            assert_eq!(
                config_home(|k| match k {
                    "XDG_CONFIG_HOME" => Some("xdg".into()),
                    "HOME" => Some("home".into()),
                    _ => None,
                })
                .unwrap(),
                PathBuf::from("xdg/igit")
            );
            assert_eq!(
                config_home(|k| (k == "HOME").then(|| "home".into())).unwrap(),
                PathBuf::from("home/.config/igit")
            );
        }
        assert_eq!(settings_path(Some(path.clone())).unwrap(), (path, true));
    }
}
