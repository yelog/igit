//! Translate only affected key prefixes; unchanged shortcuts retain their original path.
use super::*;
use crate::config::keys::{self, CommandAction, Sequence};
use crate::ui::key_sequence::PickerEvent;

struct Candidate {
    sequence: Sequence,
    target: &'static str,
    description: &'static str,
}

pub(super) struct PendingKeys {
    context: u16,
    prefix: Sequence,
    candidates: Vec<Candidate>,
    count: Option<u16>,
}

impl PendingKeys {
    pub fn prefix_label(&self) -> String {
        let prefix = keys::label(&self.prefix);
        self.count
            .map_or_else(|| prefix.clone(), |count| format!("{prefix} {count}"))
    }
    pub fn hints(&self) -> Vec<(String, &str)> {
        self.candidates
            .iter()
            .map(|c| (keys::label(&c.sequence[self.prefix.len()..]), c.description))
            .collect()
    }
}

impl Shell {
    // Consume configured smart chords even in editors/reviews, where an ordinary
    // character handler could otherwise insert text or trigger an action.
    pub(super) fn smart_pane_event(&mut self, ev: &Event) -> bool {
        let Event::Key(key) = ev else { return false };
        let action = self.config.commands.iter().find_map(|command| {
            if !command.command.action.is_smart_pane() {
                return None;
            }
            command
                .sequences
                .iter()
                .any(|s| s.as_slice() == [keys::normalize(*key)])
                .then_some(command.command.action)
        });
        let Some(action) = action else {
            return false;
        };
        match action {
            CommandAction::SmartFocus(direction) if key.kind == KeyEventKind::Press => {
                self.smart_focus(direction)
            }
            CommandAction::SmartMaximize if key.kind == KeyEventKind::Press => {
                self.smart_maximize()
            }
            CommandAction::SmartResize(direction)
                if matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) =>
            {
                self.smart_resize(direction)
            }
            _ => (),
        }
        true
    }

    fn binding_context(&self) -> u16 {
        if self.pop.is_some() || self.panel().editing() {
            return 0;
        }
        if self.ctx.repos.is_empty() {
            return keys::ALL + 1;
        }
        keys::context(self.help_content().context())
    }

    fn run_configured(&mut self, target: &'static str, count: Option<u16>) {
        self.cancel_key_sequences();
        let mut sequence = keys::catalogue_sequence(target);
        if target.starts_with("Ctrl-w ") {
            if let Some(count) = count {
                sequence.splice(
                    1..1,
                    count
                        .to_string()
                        .chars()
                        .map(|ch| KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE)),
                );
            }
        }
        for key in sequence {
            self.dispatch_builtin(&Event::Key(key));
        }
    }

    pub(super) fn configured_event(&mut self, ev: &Event) -> bool {
        if !self.config.changed() {
            return false;
        }
        let context = self.binding_context();
        if context == 0
            || self
                .pending_keys
                .as_ref()
                .is_some_and(|p| p.context != context)
        {
            self.cancel_key_sequences();
            if context == 0 {
                return false;
            }
        }
        if matches!(ev, Event::Resize(_, _) | Event::FocusLost | Event::Paste(_)) {
            self.cancel_key_sequences();
            return false;
        }
        // These context-owned keys must work even when they are also normal-mode shortcuts.
        if matches!(ev, Event::Key(k) if k.code == KeyCode::F(1)
            || self.pending_keys.is_none() && self.key_sequence().is_none() && self.panel().selected_text().is_some() && (k.code == KeyCode::Char('y') && k.modifiers.is_empty()
                || k.modifiers == KeyModifiers::CONTROL && matches!(k.code, KeyCode::Char('c') | KeyCode::Insert)))
        {
            self.cancel_key_sequences();
            return false;
        }
        if let Some(pending) = &self.pending_keys {
            // Explicit continuations win over the picker's arrows/Enter.
            let continuation = matches!(ev, Event::Key(k) if pending.candidates.iter()
                .any(|c| c.sequence.get(pending.prefix.len()) == Some(&keys::normalize(*k))));
            let event = if continuation {
                PickerEvent::Pass
            } else {
                self.key_picker.event_items(ev, pending.candidates.len())
            };
            match event {
                PickerEvent::Handled => return true,
                PickerEvent::Cancel => {
                    self.cancel_key_sequences();
                    return true;
                }
                PickerEvent::Run(index) => {
                    let target = pending.candidates[index].target;
                    let count = pending.count;
                    self.run_configured(target, count);
                    return true;
                }
                PickerEvent::Pass => (),
            }
        }
        let Event::Key(raw_key) = ev else {
            if self.pending_keys.is_some()
                && matches!(ev, Event::Mouse(m) if m.kind != MouseEventKind::Moved)
            {
                self.cancel_key_sequences();
            }
            return false;
        };
        let key = keys::normalize(*raw_key);
        if let Some(mut pending) = self.pending_keys.take() {
            // Numeric counts remain available on the built-in window prefix.
            if pending.prefix == keys::catalogue_sequence("Ctrl-w") && key.modifiers.is_empty() {
                if let KeyCode::Char(ch) = key.code {
                    if let Some(n) = ch.to_digit(10) {
                        pending.count = Some(
                            pending
                                .count
                                .unwrap_or(0)
                                .saturating_mul(10)
                                .saturating_add(n as u16),
                        );
                        self.pending_keys = Some(pending);
                        return true;
                    }
                }
            }
            pending.prefix.push(key);
            pending
                .candidates
                .retain(|c| c.sequence.starts_with(&pending.prefix));
            if let Some(candidate) = pending
                .candidates
                .iter()
                .find(|c| c.sequence.len() == pending.prefix.len())
            {
                if candidate.target != "Ctrl-w f" || raw_key.kind == KeyEventKind::Press {
                    self.run_configured(candidate.target, pending.count);
                }
                return true;
            }
            if !pending.candidates.is_empty() {
                self.pending_keys = Some(pending);
                self.key_picker =
                    KeySequencePicker::pending_with_delay(Instant::now(), self.config.hint_delay);
                return true;
            }
            let removed_default = self.config.commands.iter().any(|command| {
                command.command.contexts & context != 0
                    && command.changed()
                    && command.defaults.contains(&pending.prefix)
            });
            self.cancel_key_sequences();
            if removed_default {
                return true;
            }
            // An invalid continuation may itself start a different shortcut.
        } else if let Some(sequence) = self.key_sequence() {
            if self.key_picker.accepts_key(raw_key, sequence) {
                return false;
            }
            self.cancel_key_sequences();
        }
        let content = self.help_content();
        let commands = self
            .config
            .commands
            .iter()
            .filter(|c| {
                if self.ctx.repos.is_empty() {
                    c.command.group == "global"
                } else {
                    c.command.contexts & context != 0
                }
            })
            .collect::<Vec<_>>();
        let affected = commands.iter().filter(|c| c.changed()).any(|c| {
            c.defaults
                .iter()
                .chain(&c.sequences)
                .any(|s| s.first() == Some(&key))
        });
        if !affected {
            // Some legacy handlers match character codes without checking modifiers.
            // A removed key must not remain reachable through an incidental Alt/Ctrl alias.
            let registered = commands.iter().any(|c| {
                c.defaults
                    .iter()
                    .chain(&c.sequences)
                    .any(|s| s.first() == Some(&key))
            });
            if !registered
                && commands
                    .iter()
                    .filter(|c| c.changed())
                    .any(|c| c.defaults.iter().any(|s| s[0].code == key.code))
            {
                return true;
            }
            return false;
        }
        let candidates = commands
            .iter()
            .filter_map(|c| c.command.action.keys().map(|target| (c, target)))
            .filter(|(_, target)| {
                // Window geometry determines which directions/resizes are available.
                !target.starts_with("Ctrl-w ")
                    || content
                        .sections
                        .iter()
                        .flat_map(|s| s.bindings)
                        .any(|(key, _)| key == target)
            })
            .flat_map(|(c, target)| {
                let description = content
                    .sections
                    .iter()
                    .flat_map(|s| s.bindings)
                    .find(|(key, _)| {
                        keys::catalogue_sequence(key) == keys::catalogue_sequence(target)
                            || c.defaults.contains(&keys::catalogue_sequence(key))
                    })
                    .map_or(c.command.name, |(_, description)| *description);
                c.sequences
                    .iter()
                    .filter(|s| s.first() == Some(&key))
                    .map(move |s| Candidate {
                        sequence: s.clone(),
                        target,
                        description,
                    })
            })
            .collect::<Vec<_>>();
        if let Some(candidate) = candidates.iter().find(|c| c.sequence.len() == 1) {
            if candidate.target != "Ctrl-w f" || raw_key.kind == KeyEventKind::Press {
                self.run_configured(candidate.target, None);
            }
        } else if !candidates.is_empty() {
            self.cancel_key_sequences();
            self.cancel_drags();
            self.pending_keys = Some(PendingKeys {
                context,
                prefix: vec![key],
                candidates,
                count: None,
            });
            self.key_picker =
                KeySequencePicker::pending_with_delay(Instant::now(), self.config.hint_delay);
        }
        // An emptied old prefix is consumed, so removed defaults cannot reach panel matches.
        true
    }

    pub(super) fn configured_footer(&self) -> String {
        if self.tab == Stash {
            return format!(
                "{}/{} Browse  {} Panes  {} Diff  {} Refresh  F1 Help",
                self.config.label("stash", "move-down"),
                self.config.label("stash", "move-up"),
                self.config.label("panes", "focus-next"),
                self.config.label("stash", "open"),
                self.config.label("global", "refresh")
            );
        }
        if self.tab == Branch {
            return format!(
                "{} New  {} Delete  {} Switch  {} Merge  {} Rename  F1 Help",
                self.config.label("branch", "create"),
                self.config.label("branch", "delete"),
                self.config.label("branch", "checkout"),
                self.config.label("branch", "merge"),
                self.config.label("branch", "rename")
            );
        }
        let conflicts = if self.tab_available(Conflicts) {
            format!(
                "  {} Conflicts",
                self.config.label("global", Conflicts.binding())
            )
        } else {
            String::new()
        };
        format!(
            "{} Pull  {} Fetch  {} Panes  {} Push{conflicts}  {} Actions  F1 Help",
            self.config.label("global", "pull"),
            self.config.label("global", "fetch"),
            self.config.label("panes", "focus-next"),
            self.config.label("global", Push.binding()),
            self.config.label("global", "actions")
        )
    }
}
