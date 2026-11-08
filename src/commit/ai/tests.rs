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
        // Windows accepted sockets inherit the listener's nonblocking mode.
        // Keep accept bounded above, then read the request with a timeout.
        stream.set_nonblocking(false).unwrap();
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
fn generating_multi_repo_patches_fills_and_saves_the_shared_message() {
    let (_dir, c, mut panel) = fixture();
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
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(120, 36)).unwrap();
    terminal
        .draw(|frame| panel.draw(frame, frame.area(), &c))
        .unwrap();
    let button = panel
        .hits
        .iter()
        .find(|(_, hit)| matches!(hit, Hit::Ai))
        .unwrap()
        .0;
    let action = panel.event(
        &Event::Mouse(crossterm::event::MouseEvent {
            kind: M::Down(MouseButton::Left),
            column: button.x + 1,
            row: button.y,
            modifiers: Mod::NONE,
        }),
        &c,
    );
    assert!(matches!(action, Action::Notice(message) if message.contains("Generating")));
    assert!(panel.ai_running());
    assert!(panel.edit.is_none());
    assert!(panel.start_ai(&c).is_err());
    panel.launch(Hit::Ai, &c);
    assert!(
        panel.edit.is_none(),
        "duplicate clicks keep the loading state"
    );
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
    let serialized = payload.to_string();
    assert!(!serialized.contains("private marker"));
    assert!(!serialized.contains("unselected staged marker"));
    assert!(matches!(wait(&mut panel, &c), Action::Notice(_)));
    worker.join().unwrap();
    assert!(
        panel.edit.is_none(),
        "success does not open a candidate dialog"
    );
    assert_eq!(
        panel.saved.message.as_deref(),
        Some("feat: shared candidate\n\nDetails 中文")
    );
    let saved: Saved =
        serde_json::from_slice(&fs::read(c.state_dir.join("commit.json")).unwrap()).unwrap();
    assert_eq!(saved.message, panel.saved.message);
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
            fs::read_to_string(repo.path.join("mixed.txt")).unwrap(),
            "unstaged private marker\n"
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
fn ai_request_filters_hidden_roots_and_does_not_touch_git() {
    let (_dir, mut c, mut panel) = fixture();
    panel.ai_settings.endpoint = "http://127.0.0.1:1/mock".into();
    panel.ai_settings.model = "custom".into();
    c.scope = [0].into();
    let request = panel.prepare_ai(&c).unwrap();
    assert_eq!(request.patches.len(), 1);
    assert_eq!(request.patches[0].name, "app");
    c.scope = [1].into();
    let request = panel.prepare_ai(&c).unwrap();
    assert_eq!(request.patches.len(), 1);
    assert_eq!(request.patches[0].name, "api");
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
    let tx = pending(&mut panel);
    tx.send(Err("AI service returned HTTP 401".into())).unwrap();
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(100, 30)).unwrap();
    terminal
        .draw(|frame| panel.draw(frame, frame.area(), &c))
        .unwrap();
    assert!(matches!(panel.tick(&c), Action::Notice(message) if message.contains("HTTP 401")));
    assert_eq!(panel.saved.message, original);
    assert!(matches!(panel.edit, Some(Edit::AIError)));
    terminal
        .draw(|frame| panel.draw(frame, frame.area(), &c))
        .unwrap();
    let screen: String = terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|cell| cell.symbol())
        .collect();
    assert!(screen.contains("AI message failed") && screen.contains("HTTP 401"));
    panel.event(
        &Event::Key(crossterm::event::KeyEvent::new(K::Enter, Mod::NONE)),
        &c,
    );
    assert!(panel.edit.is_none());
    assert_eq!(panel.saved.message, original);
}

fn pending(panel: &mut CommitPanel) -> mpsc::Sender<Result<String, String>> {
    let (sender, receiver) = mpsc::channel();
    panel.ai_pending = Some(Pending {
        receiver,
        message: panel.saved.message.clone(),
        frame: 0,
    });
    sender
}

#[test]
fn missing_ai_configuration_opens_an_error_instead_of_settings() {
    let (_dir, c, mut panel) = fixture();
    let original = panel.saved.message.clone();
    assert!(
        matches!(panel.launch(Hit::Ai, &c), Action::Notice(message) if message.contains("ai.endpoint") && message.contains("ai.model"))
    );
    assert!(matches!(panel.edit, Some(Edit::AIError)));
    assert!(!panel.ai_running());
    assert_eq!(panel.saved.message, original);
}

#[test]
fn successful_generation_preserves_a_newer_draft_or_failed_save() {
    for edited in [false, true] {
        let (_dir, mut c, mut panel) = fixture();
        let tx = pending(&mut panel);
        if edited {
            panel.edit_message(&c);
            panel.buf = "User's newer message 中文".into();
            panel.finish_message(&c).unwrap();
        } else {
            c.state_dir = c.workspace.join("blocked");
            fs::write(&c.state_dir, "file").unwrap();
        }
        let original = panel.saved.message.clone();
        tx.send(Ok("feat: generated message".into())).unwrap();
        assert!(matches!(panel.tick(&c), Action::Notice(_)));
        assert_eq!(panel.saved.message, original);
        assert!(matches!(panel.edit, Some(Edit::AIError)));
        assert!(!panel.ai_running());
    }
}

#[test]
fn completion_waits_for_active_input_and_keeps_unsaved_edits() {
    let (_dir, c, mut panel) = fixture();
    let tx = pending(&mut panel);
    panel.edit_message(&c);
    panel.buf = "User's unsaved message 中文".into();
    tx.send(Ok("feat: generated message".into())).unwrap();
    assert!(matches!(panel.tick(&c), Action::None));
    assert!(panel.message_editing());
    assert!(panel.ai_running());
    panel.finish_message(&c).unwrap();
    assert!(matches!(panel.tick(&c), Action::Notice(_)));
    assert_eq!(
        panel.saved.message.as_deref(),
        Some("User's unsaved message 中文")
    );
    assert!(matches!(panel.edit, Some(Edit::AIError)));
}

#[test]
fn loading_animates_in_the_composer_and_success_fills_it_without_a_dialog() {
    let (_dir, c, mut panel) = fixture();
    for (width, height) in [(44, 12), (80, 24), (120, 36)] {
        panel.icons = IconMode::Ascii;
        let tx = pending(&mut panel);
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(width, height)).unwrap();
        terminal
            .draw(|frame| panel.draw(frame, frame.area(), &c))
            .unwrap();
        let before = terminal.backend().buffer().clone();
        assert!(panel.hits.iter().any(|(_, hit)| matches!(hit, Hit::Ai)));
        assert!(matches!(panel.tick(&c), Action::None));
        terminal
            .draw(|frame| panel.draw(frame, frame.area(), &c))
            .unwrap();
        assert_ne!(terminal.backend().buffer(), &before);
        tx.send(Ok("feat: generated message".into())).unwrap();
        assert!(matches!(panel.tick(&c), Action::Notice(_)));
        terminal
            .draw(|frame| panel.draw(frame, frame.area(), &c))
            .unwrap();
        let screen: String = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        assert!(
            screen.contains("feat: generated message"),
            "{width}x{height}: {screen}"
        );
        assert!(panel.edit.is_none());
    }
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
