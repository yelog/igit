//! Read configured remotes without contacting them or changing Git state.
use crate::core::{command, Context};
use anyhow::{bail, Result};
use std::{collections::BTreeMap, path::Path, process::Output};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Remote {
    pub(super) name: String,
    pub(super) urls: Vec<String>,
    pub(super) push_urls: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Repository {
    pub(super) root: usize,
    pub(super) remotes: Vec<Remote>,
    pub(super) error: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ConfigEntry {
    pub(super) origin: Vec<u8>,
    pub(super) key: String,
    pub(super) value: Option<Vec<u8>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ConfigSnapshot {
    pub(super) entries: Vec<ConfigEntry>,
}

impl ConfigSnapshot {
    pub(super) fn read(repo: &Path) -> Result<Self> {
        let output = command(repo)
            .args(["config", "--null", "--show-origin", "--includes", "--list"])
            .output()
            .map_err(|error| anyhow::anyhow!("Cannot read Git configuration: {error}"))?;
        if !output.status.success() {
            bail!("{}", detail(&output));
        }
        let mut fields = output.stdout.split(|byte| *byte == 0);
        let mut entries = Vec::new();
        while let Some(origin) = fields.next() {
            let Some(item) = fields.next() else {
                if origin.is_empty() {
                    break;
                }
                bail!("Git configuration output is incomplete");
            };
            if origin.is_empty() && item.is_empty() {
                break;
            }
            if origin.is_empty() {
                bail!("Git configuration origin is missing");
            }
            let (key, value) = match item.iter().position(|byte| *byte == b'\n') {
                Some(index) => (&item[..index], Some(item[index + 1..].to_vec())),
                None => (item, None),
            };
            let key = String::from_utf8_lossy(key).into_owned();
            if key.is_empty() {
                bail!("Git configuration contains an empty key");
            }
            entries.push(ConfigEntry {
                origin: origin.to_vec(),
                key,
                value,
            });
        }
        Ok(Self { entries })
    }

    pub(super) fn remotes(&self) -> Vec<Remote> {
        let mut remotes = BTreeMap::<String, Remote>::new();
        for entry in &self.entries {
            let Some((name, variable)) = remote_parts(&entry.key) else {
                continue;
            };
            let remote = remotes.entry(name.into()).or_insert_with(|| Remote {
                name: name.into(),
                urls: vec![],
                push_urls: vec![],
            });
            let value =
                String::from_utf8_lossy(entry.value.as_deref().unwrap_or_default()).into_owned();
            if variable.eq_ignore_ascii_case("url") {
                remote.urls.push(value);
            } else if variable.eq_ignore_ascii_case("pushurl") {
                remote.push_urls.push(value);
            }
        }
        remotes.into_values().collect()
    }

    pub(super) fn remote(&self, name: &str) -> Option<Remote> {
        let exists = self
            .entries
            .iter()
            .filter_map(|entry| remote_parts(&entry.key))
            .any(|(candidate, _)| candidate == name);
        if !exists {
            return None;
        }
        self.remotes()
            .into_iter()
            .find(|remote| remote.name == name)
    }

    pub(super) fn has_remote(&self, name: &str) -> bool {
        self.entries
            .iter()
            .filter_map(|entry| remote_parts(&entry.key))
            .any(|(candidate, _)| candidate == name)
    }

    pub(super) fn values(&self, key: &str) -> Vec<&[u8]> {
        self.entries
            .iter()
            .filter(|entry| entry.key == key)
            .map(|entry| entry.value.as_deref().unwrap_or_default())
            .collect()
    }
}

pub(super) fn remote_parts(key: &str) -> Option<(&str, &str)> {
    let rest = key.strip_prefix("remote.")?;
    let (name, variable) = rest.rsplit_once('.')?;
    (!name.is_empty()).then_some((name, variable))
}

fn detail(output: &Output) -> String {
    let text = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );
    if text.trim().is_empty() {
        format!("Git exited with {}", output.status)
    } else {
        label(text.trim())
    }
}

pub(super) fn load(c: &Context) -> Vec<Repository> {
    c.scope
        .iter()
        .filter_map(|&root| {
            let repo = c.repos.get(root)?;
            let (remotes, error) = match read(&repo.path) {
                Ok(remotes) => (remotes, None),
                Err(error) => (vec![], Some(label(&format!("{error:#}")))),
            };
            Some(Repository {
                root,
                remotes,
                error,
            })
        })
        .collect()
}

pub(super) fn read(repo: &Path) -> Result<Vec<Remote>> {
    Ok(ConfigSnapshot::read(repo)?.remotes())
}

pub(super) fn label(text: &str) -> String {
    let redacted = redact_url_userinfo(text);
    let mut output = String::with_capacity(redacted.len());
    for character in redacted.chars() {
        if character.is_control() {
            output.extend(character.escape_default());
        } else {
            output.push(character);
        }
    }
    output
}

fn redact_url_userinfo(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    let mut rest = text;
    loop {
        let Some(separator) = rest.find("://") else {
            output.push_str(rest);
            break;
        };
        let authority_start = separator + 3;
        output.push_str(&rest[..authority_start]);
        let after_scheme = &rest[authority_start..];
        let authority_end = after_scheme
            .find(|character: char| {
                matches!(character, '/' | '?' | '#')
                    || character.is_whitespace()
                    || character.is_control()
            })
            .unwrap_or(after_scheme.len());
        let authority = &after_scheme[..authority_end];
        if let Some(at) = authority.rfind('@') {
            output.push_str("<redacted>@");
            output.push_str(&authority[at + 1..]);
        } else {
            output.push_str(authority);
        }
        rest = &after_scheme[authority_end..];
    }
    output
}

#[cfg(test)]
mod tests;
