use super::*;
use crate::commit::message_tests::fixture;
use std::{io::Read, net::TcpListener, thread};

fn server(
    status: u16,
    body: &str,
) -> (
    String,
    mpsc::Receiver<(String, serde_json::Value)>,
    thread::JoinHandle<()>,
) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!(
        "http://{}/v1/chat/completions",
        listener.local_addr().unwrap()
    );
    listener.set_nonblocking(true).unwrap();
    let body = body.to_string();
    let (tx, rx) = mpsc::channel();
    let worker = thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut stream = loop {
            if let Ok((stream, _)) = listener.accept() {
                break stream;
            }
            assert!(Instant::now() < deadline, "AI request never arrived");
            thread::sleep(Duration::from_millis(5));
        };
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut bytes = vec![];
        let (headers, payload) = loop {
            let mut buf = [0; 4096];
            let read = stream.read(&mut buf).unwrap();
            assert_ne!(read, 0);
            bytes.extend_from_slice(&buf[..read]);
            if let Some(end) = bytes.windows(4).position(|part| part == b"\r\n\r\n") {
                let headers = String::from_utf8(bytes[..end].to_vec()).unwrap();
                let len: usize = headers
                    .lines()
                    .find_map(|line| {
                        let (key, value) = line.split_once(':')?;
                        key.eq_ignore_ascii_case("content-length")
                            .then(|| value.trim().parse().unwrap())
                    })
                    .unwrap();
                if bytes.len() >= end + 4 + len {
                    break (
                        headers,
                        serde_json::from_slice(&bytes[end + 4..end + 4 + len]).unwrap(),
                    );
                }
            }
        };
        tx.send((headers, payload)).unwrap();
        write!(stream, "HTTP/1.1 {status} Response\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
    });
    (url, rx, worker)
}

fn completion() -> &'static str {
    r#"{"choices":[{"finish_reason":"stop","message":{"content":" feat: shared candidate\n\nDetails 中文 "}}]}"#
}

fn wait(panel: &mut CommitPanel, c: &Context) -> Action {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let action = panel.tick(c);
        if panel.ai_pending.is_none() {
            return action;
        }
        assert!(Instant::now() < deadline, "AI worker did not complete");
        thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn reviewed_multi_repo_patches_make_one_request_and_candidate_requires_adoption() {
    let (_dir, c, mut panel) = fixture();
    let original = panel.saved.message.clone();
    let before: Vec<_> = c
        .repos
        .iter()
        .map(|repo| crate::core::git(&repo.path, &["ls-files", "--stage"]).unwrap())
        .collect();
    let (endpoint, received, worker) = server(200, completion());
    panel.ai_settings = AiConfig {
        endpoint,
        model: "custom-model".into(),
        prompt: "Use Chinese and include a body. 中文".into(),
        api_key_env: "IGIT_TEST_NONEXISTENT_LLM_KEY".into(),
        ..Default::default()
    };
    let Action::Confirm { body, .. } = panel.ai_review(&c).unwrap() else {
        panic!("review");
    };
    assert!(body.contains("app staged") && body.contains("api staged"));
    assert!(!body.contains("private marker") && !body.contains("unselected staged marker"));
    assert!(received.try_recv().is_err()); // Review alone never sends.
    panel.ai_settings.model = "changed-after-review".into();
    panel.ai_settings.prompt = "changed-after-review".into();
    panel.start_ai(&c).unwrap();
    assert!(panel.start_ai(&c).is_err());
    let (headers, payload) = received.recv_timeout(Duration::from_secs(5)).unwrap();
    assert!(headers.starts_with("POST /v1/chat/completions "));
    assert!(!headers.to_lowercase().contains("authorization:"));
    assert_eq!(payload["model"], "custom-model");
    assert_eq!(
        payload["messages"][0]["content"],
        "Use Chinese and include a body. 中文"
    );
    let patches: serde_json::Value =
        serde_json::from_str(payload["messages"][1]["content"].as_str().unwrap()).unwrap();
    assert_eq!(patches.as_array().unwrap().len(), 2);
    assert_eq!(patches[0]["repository"], "app");
    assert!(patches[0]["patch"].as_str().unwrap().contains("app staged"));
    assert_eq!(patches[1]["repository"], "api");
    assert!(matches!(wait(&mut panel, &c), Action::Notice(_)));
    worker.join().unwrap();
    assert!(matches!(panel.edit, Some(Edit::AIDraft)));
    assert_eq!(panel.saved.message, original);
    panel.launch(Hit::EditCancel, &c);
    assert_eq!(panel.saved.message, original);
    // Adoption uses the same save path; a failed save preserves both candidate and draft.
    panel.edit = Some(Edit::AIDraft);
    let mut blocked = c.clone();
    blocked.state_dir = c.workspace.join("blocked");
    fs::write(&blocked.state_dir, "file").unwrap();
    assert!(panel.accept(&blocked).is_err());
    assert_eq!(panel.saved.message, original);
    assert!(matches!(panel.edit, Some(Edit::AIDraft)));
    panel.accept(&c).unwrap();
    assert_eq!(
        panel.saved.message.as_deref(),
        Some("feat: shared candidate\n\nDetails 中文")
    );
    panel.edit_message(&c);
    panel.buf.push_str(" edited");
    panel.finish_message(&c).unwrap();
    assert!(panel.saved.message.as_deref().unwrap().ends_with("edited"));
    for (i, repo) in c.repos.iter().enumerate() {
        assert_eq!(
            crate::core::git(&repo.path, &["ls-files", "--stage"]).unwrap(),
            before[i]
        );
        assert_eq!(
            crate::core::git(&repo.path, &["rev-list", "--count", "HEAD"])
                .unwrap()
                .trim(),
            "1"
        );
    }
}

#[test]
fn ai_scope_filters_hidden_roots_and_rechecks_scope_before_send() {
    let (_dir, mut c, mut panel) = fixture();
    panel.ai_settings.endpoint = "http://127.0.0.1:1/mock".into();
    panel.ai_settings.model = "custom".into();
    c.scope = [0].into();
    panel.ai_review(&c).unwrap();
    let request = panel.ai_request.as_ref().unwrap();
    assert_eq!(request.patches.len(), 1);
    assert_eq!(request.patches[0].name, "app");
    c.scope = [1].into();
    assert!(panel
        .start_ai(&c)
        .err()
        .unwrap()
        .to_string()
        .contains("scope changed"));
    assert!(panel.ai_pending.is_none());
}

#[test]
fn curl_authentication_and_errors_never_echo_credentials_or_service_bodies() {
    for (status, body, expected) in [
        (401, "sensitive-token private-patch", "HTTP 401"),
        (200, "sensitive-token private-patch", "invalid JSON"),
        (
            200,
            r#"{"error":{"message":"sensitive-token private-patch"}}"#,
            "returned an error",
        ),
    ] {
        let (endpoint, received, worker) = server(status, body);
        let request = Request {
            settings: AiConfig {
                endpoint,
                model: "mock".into(),
                ..Default::default()
            },
            patches: vec![],
        };
        let error = request
            .send_with_credential(Some("sensitive-token"))
            .unwrap_err()
            .to_string();
        assert!(error.contains(expected), "{error}");
        assert!(!error.contains("sensitive-token") && !error.contains("private-patch"));
        let (headers, _) = received.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(headers.contains("Authorization: Bearer sensitive-token"));
        worker.join().unwrap();
    }
}

#[test]
fn response_rejects_empty_truncated_filtered_and_invalid_messages() {
    for body in [
        r#"{"choices":[]}"#,
        r#"{"choices":[{"message":{"content":"  "}}]}"#,
        r#"{"choices":[{"finish_reason":"length","message":{"content":"partial"}}]}"#,
        r#"{"choices":[{"finish_reason":"content_filter","message":{"content":"filtered"}}]}"#,
        r#"{"choices":[{"message":{"content":"bad\u0000message"}}]}"#,
    ] {
        assert!(parse_response(body.as_bytes()).is_err(), "{body}");
    }
    assert_eq!(
        parse_response(completion().as_bytes()).unwrap(),
        "feat: shared candidate\n\nDetails 中文"
    );
}

#[test]
fn failed_request_keeps_draft_and_reports_error_even_after_drawing() {
    let (_dir, c, mut panel) = fixture();
    let original = panel.saved.message.clone();
    let (tx, rx) = mpsc::channel();
    panel.ai_pending = Some(rx);
    tx.send(Err("AI service returned HTTP 401".into())).unwrap();
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(100, 30)).unwrap();
    terminal
        .draw(|frame| panel.draw(frame, frame.area(), &c))
        .unwrap();
    assert!(matches!(panel.tick(&c), Action::Notice(message) if message.contains("HTTP 401")));
    assert_eq!(panel.saved.message, original);
    assert!(panel.edit.is_none());
}

#[test]
fn configured_ai_overrides_legacy_workspace_settings_and_survives_refresh() {
    let (_dir, c, mut panel) = fixture();
    panel.saved.endpoint = "https://legacy.invalid/chat/completions".into();
    panel.saved.model = "legacy".into();
    panel.saved.prompt = "legacy prompt".into();
    panel.save(&c).unwrap();
    let config = crate::config::AppConfig::from_toml("[ai]\nendpoint='http://localhost:11434/v1/chat/completions'\nmodel='custom'\nprompt='custom prompt'").unwrap();
    let mut reopened = CommitPanel {
        ai_config: config.ai.clone(),
        ..Default::default()
    };
    reopened.refresh(&c).unwrap();
    assert_eq!(reopened.ai_settings, config.ai.unwrap());
    reopened.launch(Hit::Settings, &c);
    reopened.buf = "http://localhost:1234/v1/chat/completions\nsession\nsession prompt".into();
    reopened.accept(&c).unwrap();
    reopened.refresh(&c).unwrap();
    assert_eq!(reopened.ai_settings.model, "session");
    let mut legacy = CommitPanel::default();
    legacy.refresh(&c).unwrap();
    assert_eq!(legacy.ai_settings.model, "session");
}
