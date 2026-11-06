use super::*;
use crate::config::AiConfig;
use anyhow::Context as _;
use std::{
    io::Write,
    process::{Command, Stdio},
    sync::mpsc,
};

#[cfg(test)]
mod tests;

pub(super) struct Patch {
    pub root: usize,
    pub name: String,
    pub content: String,
}

pub(super) struct Request {
    pub settings: AiConfig,
    pub patches: Vec<Patch>,
}

impl Request {
    fn payload(&self) -> serde_json::Value {
        let repositories: Vec<_> = self
            .patches
            .iter()
            .map(|p| serde_json::json!({"repository": p.name, "patch": p.content}))
            .collect();
        serde_json::json!({
            "model": self.settings.model,
            "stream": false,
            "messages": [
                {"role": "system", "content": self.settings.prompt},
                {"role": "user", "content": serde_json::to_string(&repositories).expect("serializable patches")}
            ]
        })
    }

    fn send(&self) -> Result<String> {
        let credential = match std::env::var(&self.settings.api_key_env) {
            Ok(key) => Some(key),
            Err(std::env::VarError::NotPresent) => None,
            Err(_) => bail!("The configured AI credential is not valid Unicode"),
        };
        self.send_with_credential(credential.as_deref())
    }

    fn send_with_credential(&self, credential: Option<&str>) -> Result<String> {
        self.settings.validate()?;
        let mut token_file = tempfile::NamedTempFile::new()?;
        let credential = credential.filter(|key| !key.is_empty());
        if let Some(token) = credential {
            if token.chars().any(char::is_control) {
                bail!("Invalid AI credential: control characters are not allowed")
            }
            writeln!(token_file, "Authorization: Bearer {token}")?;
        }
        let mut curl = Command::new("curl");
        curl.arg("-q")
            .args([
                "--silent",
                "--show-error",
                "--fail",
                "--max-time",
                &self.settings.timeout_seconds.to_string(),
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
        if credential.is_some() {
            curl.arg("--header")
                .arg(format!("@{}", token_file.path().display()));
        }
        let mut child = curl
            .args(["--data-binary", "@-", "--url", &self.settings.endpoint])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context("Could not start curl; install curl to generate a message")?;
        let write = child
            .stdin
            .take()
            .unwrap()
            .write_all(&serde_json::to_vec(&self.payload())?);
        if write.is_err() {
            let _ = child.kill();
            let _ = child.wait();
            bail!("Could not send the selected patches to curl")
        }
        let output = child.wait_with_output()?;
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
        // Never echo service bodies or curl stderr: either can contain credentials or patches.
        if status >= 300 {
            bail!("AI service returned HTTP {status}; check the endpoint, model and credentials")
        }
        if !output.status.success() || status < 200 {
            bail!(
                "AI request failed (curl exit {}); check connectivity and ai.timeout_seconds",
                output.status.code().unwrap_or(-1)
            )
        }
        parse_response(body)
    }
}

fn parse_response(body: &[u8]) -> Result<String> {
    let value: serde_json::Value = serde_json::from_slice(body)
        .map_err(|_| anyhow::anyhow!("AI service returned invalid JSON"))?;
    if value.get("error").is_some_and(|error| !error.is_null()) {
        bail!("AI service returned an error; check the endpoint, model and credentials")
    }
    let choice = &value["choices"][0];
    if let Some(reason) = choice["finish_reason"].as_str() {
        if reason != "stop" {
            bail!("AI service did not return a complete message; adjust the prompt or model and retry")
        }
    }
    let message = choice["message"]["content"]
        .as_str()
        .filter(|message| !message.trim().is_empty())
        .ok_or_else(|| anyhow::anyhow!("AI service returned no valid message.content"))?;
    if message.contains('\0') {
        bail!("AI service returned an invalid commit message")
    }
    Ok(message.trim().to_owned())
}

impl CommitPanel {
    pub(super) fn ai_review(&mut self, c: &Context) -> Result<Action> {
        self.ai_request = None;
        if self.ai_pending.is_some() {
            bail!("AI request in progress. Please wait")
        }
        if self.ai_settings.endpoint.trim().is_empty() || self.ai_settings.model.trim().is_empty() {
            self.launch(Hit::Settings, c);
            bail!("Set ai.endpoint and ai.model in settings.toml, or complete AI settings")
        }
        self.ai_settings.validate()?;
        let mut patches = vec![];
        let mut bytes = 0;
        let mut body = format!(
            "Send selected patches to {}\nModel: {}\nOne request generates one shared message for all selected repositories. Review the candidate before replacing your message.\n",
            self.ai_settings.endpoint, self.ai_settings.model
        );
        for root in self.checked_repos(c) {
            let files: Vec<_> = self
                .selection(c)
                .into_iter()
                .filter(|f| f.root == root)
                .map(|f| f.change.clone())
                .collect();
            let content = backend::patch(&c.repos[root].path, &files)?;
            bytes += content.len();
            if bytes > 160_000 {
                bail!("Selected patches exceed 160 KB. Select fewer files before generating a message")
            }
            body.push_str(&format!(
                "\n{} · {} bytes\n",
                c.repos[root].name,
                content.len()
            ));
            for file in &files {
                body.push_str(&format!("  {}\n", file.label()));
            }
            patches.push(Patch {
                root,
                name: c.repos[root].name.clone(),
                content,
            });
        }
        if patches.is_empty() {
            bail!("Select files first")
        }
        for patch in &patches {
            body.push_str(&format!(
                "\n{} / selected patch\n{}",
                patch.name, patch.content
            ));
        }
        self.ai_request = Some(Request {
            settings: self.ai_settings.clone(),
            patches,
        });
        Ok(Action::Confirm {
            title: "AI · Send selected patches?".into(),
            body,
            token: "commit-ai".into(),
        })
    }

    pub(super) fn start_ai(&mut self, c: &Context) -> Result<Action> {
        if self.ai_pending.is_some() {
            bail!("AI request in progress. Please wait")
        }
        let request = self
            .ai_request
            .take()
            .ok_or_else(|| anyhow::anyhow!("Review selected patches first"))?;
        if request
            .patches
            .iter()
            .any(|patch| !c.scope.contains(&patch.root))
        {
            bail!("Repository scope changed. Review AI patches again")
        }
        let (tx, rx) = mpsc::channel();
        self.ai_pending = Some(rx);
        self.ai_error.clear();
        std::thread::spawn(move || {
            let _ = tx.send(request.send().map_err(|error| error.to_string()));
        });
        Ok(err("Generating one commit message. You can keep browsing"))
    }

    pub(super) fn poll_ai(&mut self) {
        if self.edit.is_some() {
            return;
        }
        let Some(rx) = self.ai_pending.as_ref() else {
            return;
        };
        match rx.try_recv() {
            Ok(Ok(message)) => {
                self.ai_draft = message;
                self.ai_scroll = 0;
                self.edit = Some(Edit::AIDraft);
                self.ai_pending = None;
            }
            Ok(Err(error)) => {
                self.ai_error = error;
                self.ai_pending = None;
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                self.ai_error = "AI worker stopped".into();
                self.ai_pending = None;
            }
            Err(mpsc::TryRecvError::Empty) => {}
        }
    }
}
