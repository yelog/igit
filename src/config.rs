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
    ai: AiConfig,
    updates: UpdateConfig,
    keybindings: Keybindings,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateConfig {
    pub check_on_startup: bool,
    pub check_interval_hours: u64,
}

impl UpdateConfig {
    pub fn interval(&self) -> Duration {
        Duration::from_secs(self.check_interval_hours * 60 * 60)
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AiConfig {
    pub endpoint: String,
    pub model: String,
    pub prompt: String,
    pub api_key_env: String,
    /// Timeout for each AI request, in seconds.
    pub timeout_seconds: u64,
    pub input_budget_bytes: usize,
    /// Total requests allowed, including reductions and final generation.
    pub max_requests: usize,
    /// Deadline for the entire AI job, in seconds.
    pub total_timeout_seconds: u64,
}

impl Default for AiConfig {
    fn default() -> Self {
        toml::from_str::<Settings>(DEFAULT_TOML)
            .expect("valid embedded AI settings")
            .ai
    }
}

impl AiConfig {
    pub(crate) fn validate(&self) -> Result<()> {
        if !self.endpoint.is_empty() {
            let valid = self
                .endpoint
                .split_once("://")
                .is_some_and(|(scheme, rest)| {
                    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
                    let local = ["localhost:", "127.0.0.1:", "[::1]:"].iter().any(|prefix| {
                        authority
                            .strip_prefix(prefix)
                            .is_some_and(|port| port.parse::<u16>().is_ok_and(|port| port != 0))
                    });
                    !authority.is_empty() && (scheme == "https" || (scheme == "http" && local))
                });
            if !valid
                || self.endpoint.contains(['@', '#', '\\'])
                || self
                    .endpoint
                    .chars()
                    .any(|c| c.is_whitespace() || c.is_control())
            {
                bail!("ai.endpoint: use a full HTTPS URL or localhost HTTP URL with a port; credentials belong in the configured environment variable")
            }
        }
        if self.prompt.trim().is_empty() {
            bail!("ai.prompt must not be empty")
        }
        let mut chars = self.api_key_env.chars();
        if !chars
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
            || !chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
        {
            bail!("ai.api_key_env must be an environment variable name")
        }
        if !(1..=600).contains(&self.timeout_seconds) {
            bail!("ai.timeout_seconds must be between 1 and 600")
        }
        if !(8_192..=2_097_152).contains(&self.input_budget_bytes) {
            bail!("ai.input_budget_bytes must be between 8192 and 2097152")
        }
        if !(1..=128).contains(&self.max_requests) {
            bail!("ai.max_requests must be between 1 and 128")
        }
        if !(1..=3_600).contains(&self.total_timeout_seconds) {
            bail!("ai.total_timeout_seconds must be between 1 and 3600")
        }
        Ok(())
    }
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
    branch: Group,
    stash: Group,
}
impl Keybindings {
    fn groups(&self) -> [(&str, &Group); 8] {
        [
            ("global", &self.global),
            ("panes", &self.panes),
            ("log", &self.log),
            ("commit", &self.commit),
            ("push", &self.push),
            ("merge", &self.merge),
            ("branch", &self.branch),
            ("stash", &self.stash),
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
    pub ai: Option<AiConfig>,
    pub updates: UpdateConfig,
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
        let overrides: toml::Value = toml::from_str(overrides)?;
        let has_ai = overrides.get("ai").is_some();
        merge(&mut merged, overrides);
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
        settings.ai.validate()?;
        if settings.updates.check_interval_hours == 0
            || settings.updates.check_interval_hours > u64::MAX / 3600
        {
            bail!(
                "updates.check_interval_hours must be between 1 and {}",
                u64::MAX / 3600
            )
        }
        Ok(Self {
            icons,
            mouse: settings.terminal.mouse,
            ai: has_ai.then_some(settings.ai),
            updates: settings.updates,
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
        let contexts = COMMANDS
            .iter()
            .filter(|command| command.group == group)
            .fold(0, |contexts, command| contexts | command.contexts);
        self.commands
            .iter()
            .filter(|c| {
                c.changed()
                    && (c.command.group == group
                        || c.command.group == "global" && c.command.contexts & contexts != 0)
            })
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
    fn ai_configuration_merges_defaults_and_accepts_custom_prompt() {
        assert!(AppConfig::default().ai.is_none());
        let defaults = AiConfig::default();
        assert_eq!(defaults.timeout_seconds, 60);
        assert_eq!(defaults.input_budget_bytes, 160_000);
        assert_eq!(defaults.max_requests, 32);
        assert_eq!(defaults.total_timeout_seconds, 300);
        let config = AppConfig::from_toml("[ai]\nendpoint='http://localhost:11434/v1/chat/completions'\nmodel='my-model'\nprompt='''Write a subject.\nThen explain in 中文.'''\napi_key_env='CUSTOM_LLM_KEY'\ntimeout_seconds=120").unwrap();
        let ai = config.ai.unwrap();
        assert_eq!(ai.model, "my-model");
        assert_eq!(ai.prompt, "Write a subject.\nThen explain in 中文.");
        assert_eq!(ai.api_key_env, "CUSTOM_LLM_KEY");
        assert_eq!(ai.timeout_seconds, 120);
        assert_eq!(ai.input_budget_bytes, defaults.input_budget_bytes);
        assert_eq!(ai.max_requests, defaults.max_requests);
        assert_eq!(ai.total_timeout_seconds, defaults.total_timeout_seconds);
        let partial = AppConfig::from_toml("[ai]\nmodel='local'")
            .unwrap()
            .ai
            .unwrap();
        assert_eq!(partial.prompt, AiConfig::default().prompt);
        assert_eq!(partial.api_key_env, "IGIT_LLM_API_KEY");
        assert_eq!(
            partial,
            AiConfig {
                model: "local".into(),
                ..defaults
            }
        );
    }

    #[test]
    fn ai_configuration_merges_individual_limits_with_defaults() {
        for (text, expected) in [
            (
                "input_budget_bytes=65536",
                AiConfig {
                    input_budget_bytes: 65_536,
                    ..Default::default()
                },
            ),
            (
                "max_requests=8",
                AiConfig {
                    max_requests: 8,
                    ..Default::default()
                },
            ),
            (
                "total_timeout_seconds=900",
                AiConfig {
                    total_timeout_seconds: 900,
                    ..Default::default()
                },
            ),
        ] {
            let ai = AppConfig::from_toml(&format!("[ai]\n{text}"))
                .unwrap()
                .ai
                .unwrap();
            assert_eq!(ai, expected, "{text}");
        }
    }

    #[test]
    fn ai_configuration_accepts_explicit_limits_and_inclusive_boundaries() {
        for (input_budget_bytes, max_requests, total_timeout_seconds) in
            [(8_192, 1, 1), (65_536, 8, 900), (2_097_152, 128, 3_600)]
        {
            let ai = AppConfig::from_toml(&format!(
                "[ai]\ninput_budget_bytes={input_budget_bytes}\nmax_requests={max_requests}\ntotal_timeout_seconds={total_timeout_seconds}"
            ))
            .unwrap()
            .ai
            .unwrap();
            assert_eq!(ai.input_budget_bytes, input_budget_bytes);
            assert_eq!(ai.max_requests, max_requests);
            assert_eq!(ai.total_timeout_seconds, total_timeout_seconds);
            assert_eq!(ai.timeout_seconds, 60);
        }
    }

    #[test]
    fn ai_configuration_rejects_out_of_range_limits() {
        for (field, value) in [
            ("input_budget_bytes", 0),
            ("input_budget_bytes", 8_191),
            ("input_budget_bytes", 2_097_153),
            ("max_requests", 0),
            ("max_requests", 129),
            ("total_timeout_seconds", 0),
            ("total_timeout_seconds", 3_601),
        ] {
            let text = format!("[ai]\n{field}={value}");
            let error = AppConfig::from_toml(&text).unwrap_err().to_string();
            assert!(error.contains(&format!("ai.{field}")), "{text}: {error}");
        }
    }

    #[test]
    fn ai_configuration_rejects_unknown_fields_and_inline_credentials() {
        for text in [
            "api_key='not-allowed'",
            "input_budget=160000",
            "max_request=32",
            "total_timeout=300",
        ] {
            for settings in [format!("[ai]\n{text}"), format!("ai={{ {text} }}")] {
                assert!(AppConfig::from_toml(&settings).is_err(), "{settings}");
            }
        }
    }

    #[test]
    fn ai_configuration_rejects_invalid_destination_credentials_and_limits() {
        for text in [
            "endpoint='http://remote.invalid/v1/chat/completions'",
            "endpoint='http://localhost:abc/v1/chat/completions'",
            "endpoint='http://localhost:1234@remote.invalid/v1/chat/completions'",
            "endpoint='https:///v1/chat/completions'",
            "endpoint='https://user:secret@service.invalid/chat/completions'",
            "endpoint='https://service.invalid/chat#fragment'",
            "endpoint='file:///tmp/patches'",
            "api_key='not-allowed'",
            "api_key_env='BAD KEY'",
            "api_key_env='1KEY'",
            "prompt='  '",
            "timeout_seconds=0",
            "timeout_seconds=601",
            "input_budget_bytes=-1",
            "input_budget_bytes='160000'",
            "input_budget_bytes=160000.0",
            "max_requests=-1",
            "max_requests='32'",
            "max_requests=32.0",
            "total_timeout_seconds=-1",
            "total_timeout_seconds='300'",
            "total_timeout_seconds=300.0",
        ] {
            assert!(
                AppConfig::from_toml(&format!("[ai]\n{text}")).is_err(),
                "{text}"
            );
        }
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
