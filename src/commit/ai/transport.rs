//! Send bounded compatible requests without exposing credentials or response bodies.
use super::{input, parse_response};
use crate::{commit::process::Control, config::AiConfig};
use anyhow::{bail, Result};
use std::process::Command;

const RESPONSE_LIMIT: usize = 512 * 1024;

pub(super) struct Client {
    settings: AiConfig,
    control: Control,
    credential: Option<String>,
}

impl Client {
    pub fn new(settings: &AiConfig, control: &Control) -> Result<Self> {
        let credential = match std::env::var(&settings.api_key_env) {
            Ok(key) => Some(key),
            Err(std::env::VarError::NotPresent) => None,
            Err(_) => bail!("The configured AI credential is not valid Unicode"),
        };
        Self::with_credential(settings, control, credential.as_deref())
    }

    pub fn with_credential(
        settings: &AiConfig,
        control: &Control,
        credential: Option<&str>,
    ) -> Result<Self> {
        settings.validate()?;
        control.check()?;
        let credential = credential.filter(|key| !key.is_empty());
        if credential.is_some_and(|token| token.chars().any(char::is_control)) {
            bail!("Invalid AI credential: control characters are not allowed");
        }
        Ok(Self {
            settings: settings.clone(),
            control: control.clone(),
            credential: credential.map(str::to_owned),
        })
    }

    pub fn send(
        &self,
        system: &str,
        user: &serde_json::Value,
        output_tokens: Option<usize>,
    ) -> Result<String> {
        let body =
            serde_json::to_string(&input::payload(&self.settings, system, user, output_tokens))?;
        if body.len() > self.settings.input_budget_bytes {
            bail!("AI request exceeds ai.input_budget_bytes; increase the budget or shorten ai.prompt");
        }
        let timeout = self
            .control
            .remaining()?
            .as_secs_f64()
            .min(self.settings.timeout_seconds as f64);
        let mut curl = Command::new("curl");
        curl.arg("-q")
            .args([
                "--globoff",
                "--silent",
                "--show-error",
                "--fail",
                "--max-time",
                &format!("{timeout:.3}"),
                "--connect-timeout",
                "8",
                "--header",
                "Content-Type: application/json",
                "--proto",
                "=http,https",
                "--write-out",
                "\n%{http_code}",
            ])
            .env_remove(&self.settings.api_key_env)
            .env_remove("IGIT_LLM_API_KEY");
        // Both values travel through anonymous stdin. A named credential file
        // could survive if the terminal exits before this worker returns.
        let mut stdin = String::with_capacity(body.len());
        if let Some(token) = &self.credential {
            curl_option(
                &mut stdin,
                "header",
                &format!("Authorization: Bearer {token}"),
            );
        }
        curl_option(&mut stdin, "data-binary", &body);
        curl.args(["--config", "-", "--url", &self.settings.endpoint]);
        let output = self
            .control
            .run(curl, Some(stdin.as_bytes()), RESPONSE_LIMIT)
            .map_err(|error| {
                anyhow::anyhow!(
                    "Could not complete the AI request; check curl and the configured limits: {error}"
                )
            })?;
        let end = output
            .stdout
            .iter()
            .rposition(|b| *b == b'\n')
            .ok_or_else(|| anyhow::anyhow!("AI service returned no HTTP status"))?;
        let (body, status) = (&output.stdout[..end], &output.stdout[end + 1..]);
        let status = std::str::from_utf8(status)
            .unwrap_or("")
            .parse::<u16>()
            .unwrap_or(0);
        // Service bodies and curl stderr can contain credentials and patches.
        if status >= 300 {
            bail!("AI service returned HTTP {status}; check the endpoint, model, credentials and input budget");
        }
        if !output.status.success() || status < 200 {
            bail!(
                "AI request failed (curl exit {}); check connectivity and ai.timeout_seconds",
                output.status.code().unwrap_or(-1)
            );
        }
        parse_response(body)
    }
}

fn curl_option(config: &mut String, name: &str, value: &str) {
    // The JSON serializer escapes control characters and credentials reject
    // them. Quoting these two characters keeps each option on one config line
    // while preserving the exact HTTP body and header.
    config.push_str(name);
    config.push_str(" = \"");
    for ch in value.chars() {
        if matches!(ch, '\\' | '"') {
            config.push('\\');
        }
        config.push(ch);
    }
    config.push_str("\"\n");
}
