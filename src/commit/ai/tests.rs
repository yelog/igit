use super::*;
use crate::commit::message_tests::fixture;
use std::{
    io::{Read, Write},
    net::TcpListener,
    thread,
};

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
        let (mut stream, headers, payload) = read_request(&listener);
        let _ = tx.send((headers, payload));
        respond(&mut stream, status, &body);
    });
    (url, rx, worker)
}

fn read_request(listener: &TcpListener) -> (std::net::TcpStream, String, serde_json::Value) {
    let deadline = Instant::now() + Duration::from_secs(15);
    let mut stream = loop {
        if let Ok((stream, _)) = listener.accept() {
            break stream;
        }
        assert!(Instant::now() < deadline, "AI request never arrived");
        thread::sleep(Duration::from_millis(5));
    };
    stream.set_nonblocking(false).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(15)))
        .unwrap();
    let mut bytes = vec![];
    let mut continued = false;
    loop {
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
            // curl can wait for 100 Continue for sufficiently large requests.
            if !continued
                && headers
                    .to_ascii_lowercase()
                    .contains("expect: 100-continue")
            {
                stream.write_all(b"HTTP/1.1 100 Continue\r\n\r\n").unwrap();
                continued = true;
            }
            if bytes.len() >= end + 4 + len {
                let payload = serde_json::from_slice(&bytes[end + 4..end + 4 + len]).unwrap();
                return (stream, headers, payload);
            }
        }
    }
}

fn respond(stream: &mut std::net::TcpStream, status: u16, body: &str) {
    write!(stream, "HTTP/1.1 {status} Response\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
}

fn response(message: &str) -> String {
    serde_json::json!({"choices":[{"finish_reason":"stop","message":{"content":message}}]})
        .to_string()
}

fn ai_settings(endpoint: String, budget: usize) -> AiConfig {
    AiConfig {
        endpoint,
        model: "local-mock".into(),
        api_key_env: "IGIT_TEST_NONEXISTENT_LLM_KEY".into(),
        input_budget_bytes: budget,
        ..Default::default()
    }
}

fn service(
    mut handler: impl FnMut(&serde_json::Value) -> (u16, String, bool) + Send + 'static,
) -> (
    String,
    mpsc::Receiver<serde_json::Value>,
    thread::JoinHandle<()>,
) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let url = format!(
        "http://{}/v1/chat/completions",
        listener.local_addr().unwrap()
    );
    let (sender, receiver) = mpsc::channel();
    let worker = thread::spawn(move || loop {
        let (mut stream, _, payload) = read_request(&listener);
        let (status, body, done) = handler(&payload);
        let _ = sender.send(payload);
        respond(&mut stream, status, &body);
        if done {
            break;
        }
    });
    (url, receiver, worker)
}

fn completion() -> &'static str {
    r#"{"choices":[{"finish_reason":"stop","message":{"content":" feat: shared candidate\n\nDetails 中文 "}}]}"#
}

fn wait(panel: &mut CommitPanel, c: &Context) -> Action {
    let deadline = Instant::now() + Duration::from_secs(15);
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
fn message_action_shortcuts_generate_with_g_without_changing_git() {
    let (_dir, c, mut panel) = fixture();
    let before: Vec<_> = c
        .repos
        .iter()
        .map(|repo| {
            (
                crate::core::git(&repo.path, &["rev-parse", "HEAD"]).unwrap(),
                crate::core::git(&repo.path, &["diff", "--cached", "--binary"]).unwrap(),
                crate::core::git(&repo.path, &["diff", "--binary"]).unwrap(),
            )
        })
        .collect();
    let checks = panel.checks.clone();
    let (endpoint, received, worker) = server(200, completion());
    panel.ai_settings = ai_settings(endpoint, 160_000);
    panel.focus_pane(PaneId::Message);
    let action = panel.event(
        &Event::Key(crossterm::event::KeyEvent::new(K::Char('g'), Mod::NONE)),
        &c,
    );
    assert!(matches!(action, Action::Notice(NoticeLevel::Info, _)));
    assert!(panel.ai_running());
    received.recv_timeout(Duration::from_secs(5)).unwrap();
    assert!(matches!(
        wait(&mut panel, &c),
        Action::Notice(NoticeLevel::Success, _)
    ));
    worker.join().unwrap();
    assert_eq!(
        panel.saved.message.as_deref(),
        Some("feat: shared candidate\n\nDetails 中文")
    );
    assert_eq!(panel.checks, checks);
    assert_eq!(panel.focused_pane(), PaneId::Message);
    assert!(!panel.message_editing());
    for (repo, (head, index, worktree)) in c.repos.iter().zip(before) {
        assert_eq!(
            crate::core::git(&repo.path, &["rev-parse", "HEAD"]).unwrap(),
            head
        );
        assert_eq!(
            crate::core::git(&repo.path, &["diff", "--cached", "--binary"]).unwrap(),
            index
        );
        assert_eq!(
            crate::core::git(&repo.path, &["diff", "--binary"]).unwrap(),
            worktree
        );
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
    let key = |code| Event::Key(crossterm::event::KeyEvent::new(code, Mod::NONE));
    panel.focus_pane(PaneId::Message);
    panel.event(&key(K::Right), &c);
    panel.event(&key(K::Right), &c);
    let action = panel.event(&key(K::Enter), &c);
    assert!(
        matches!(action, Action::Notice(NoticeLevel::Info, message) if message.contains("Generating"))
    );
    assert!(panel.ai_running());
    assert!(panel.edit.is_none());
    assert!(panel.start_ai(&c).is_err());
    panel.event(&key(K::Enter), &c);
    assert!(
        panel.edit.is_none(),
        "a repeated Generate action keeps the loading state"
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
    let files = patches["files"].as_array().unwrap();
    assert_eq!(files.len(), 2);
    assert_eq!(files[0]["repository"], "app");
    assert!(files[0]["patch"].as_str().unwrap().contains("app staged"));
    assert_eq!(files[1]["repository"], "api");
    let serialized = payload.to_string();
    assert!(!serialized.contains("private marker"));
    assert!(!serialized.contains("unselected staged marker"));
    assert!(matches!(
        wait(&mut panel, &c),
        Action::Notice(NoticeLevel::Success, _)
    ));
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
    assert_eq!(request.repositories.len(), 1);
    assert_eq!(request.repositories[0].name, "app");
    c.scope = [1].into();
    let request = panel.prepare_ai(&c).unwrap();
    assert_eq!(request.repositories.len(), 1);
    assert_eq!(request.repositories[0].name, "api");
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
        let settings = AiConfig {
            endpoint,
            model: "mock".into(),
            ..Default::default()
        };
        let control = Control::new(Duration::from_secs(5));
        let error =
            transport::Client::with_credential(&settings, &control, Some("sensitive-token"))
                .unwrap()
                .send(&settings.prompt, &serde_json::json!({"files":[]}), None)
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
fn curl_preserves_large_escaped_json_and_the_bearer_header_over_stdin() {
    let (endpoint, received, worker) = server(200, completion());
    let settings = ai_settings(endpoint, 2_097_152);
    let user = serde_json::json!({"files": [{"patch": "\\\"中文\n".repeat(100_000)}]});
    let expected = input::payload(&settings, &settings.prompt, &user, None);
    assert!((1_048_576..=settings.input_budget_bytes).contains(&expected.to_string().len()));
    let token = r#"fake-"key\test"#;
    let control = Control::new(Duration::from_secs(15));
    let message = transport::Client::with_credential(&settings, &control, Some(token))
        .unwrap()
        .send(&settings.prompt, &user, None)
        .unwrap();
    assert_eq!(message, "feat: shared candidate\n\nDetails 中文");
    let (headers, payload) = received.recv_timeout(Duration::from_secs(5)).unwrap();
    assert!(headers.contains(&format!("Authorization: Bearer {token}")));
    assert_eq!(payload, expected);
    worker.join().unwrap();
}

#[test]
fn endpoint_braces_are_literal_and_cannot_expand_one_send_into_multiple_requests() {
    let (endpoint, received, worker) = server(200, completion());
    let endpoint = endpoint.replace("/v1/chat/", "/v1/{chat,other}/");
    let settings = ai_settings(endpoint, 8192);
    let control = Control::new(Duration::from_secs(5));
    let message = transport::Client::with_credential(&settings, &control, None)
        .unwrap()
        .send(&settings.prompt, &serde_json::json!({"files":[]}), None)
        .unwrap();
    assert_eq!(message, "feat: shared candidate\n\nDetails 中文");
    let (headers, _) = received.recv_timeout(Duration::from_secs(5)).unwrap();
    assert!(headers.starts_with("POST /v1/{chat,other}/completions "));
    worker.join().unwrap();
    assert!(received.try_recv().is_err());
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
    panel.focus = Focus::Diff;
    let tx = pending(&mut panel, &c);
    tx.send(Report::Generated(
        Err("AI service returned HTTP 401".into()),
    ))
    .unwrap();
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(100, 30)).unwrap();
    terminal
        .draw(|frame| panel.draw(frame, frame.area(), &c))
        .unwrap();
    assert!(
        matches!(panel.tick(&c), Action::Notice(NoticeLevel::Error, message) if message == "AI service returned HTTP 401")
    );
    assert_eq!(panel.saved.message, original);
    assert!(panel.edit.is_none());
    assert!(panel.focus == Focus::Diff);
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
    assert!(!screen.contains("AI message failed") && !screen.contains("HTTP 401"));
    assert!(panel.edit.is_none());
    assert!(panel.focus == Focus::Diff);
    assert_eq!(panel.saved.message, original);
}

#[test]
fn failed_request_reports_full_error_without_interrupting_current_input() {
    for edit in [Edit::Message, Edit::AISettings, Edit::Group, Edit::Stash] {
        let (_dir, c, mut panel) = fixture();
        let original = panel.saved.message.clone();
        let tx = pending(&mut panel, &c);
        panel.focus = Focus::Message;
        panel.edit = Some(edit.clone());
        panel.buf = "Unsaved input 中文".into();
        panel.caret = "Unsaved".len();
        panel.edit_view = (Rect::new(2, 3, 40, 8), 4, 2);
        let error = format!(
            "AI service failed\n{}\nLast detail",
            "Long error detail\n".repeat(100)
        );
        tx.send(Report::Generated(Err(error.clone()))).unwrap();
        let Action::Notice(NoticeLevel::Error, message) = panel.tick(&c) else {
            panic!("An AI failure must report an error during input");
        };
        assert_eq!(message, error);
        assert_eq!(panel.ai_error, error);
        assert_eq!(panel.saved.message, original);
        assert!(panel.focus == Focus::Message);
        assert_eq!(
            panel.edit.as_ref().map(std::mem::discriminant),
            Some(std::mem::discriminant(&edit))
        );
        assert_eq!(panel.buf, "Unsaved input 中文");
        assert_eq!(panel.caret, "Unsaved".len());
        assert_eq!(panel.edit_view, (Rect::new(2, 3, 40, 8), 4, 2));
        assert!(!panel.ai_running());
    }
}

fn pending(panel: &mut CommitPanel, c: &Context) -> mpsc::Sender<Report> {
    let (sender, receiver) = mpsc::channel();
    panel.ai_pending = Some(Pending {
        receiver,
        sender: sender.clone(),
        message: panel.saved.message.clone(),
        selection: Selection::new(panel, c),
        control: Control::new(Duration::from_secs(30)),
        candidate: None,
        finished: None,
        progress: "Preparing changes…".into(),
        frame: 0,
        revision: 0,
    });
    sender
}

fn checked(message: &str) -> Report {
    Report::Checked {
        revision: 0,
        completed: Instant::now(),
        result: Ok(Candidate {
            message: message.into(),
            snapshots: vec![],
        }),
    }
}

#[test]
fn missing_ai_configuration_reports_error_without_opening_an_editor() {
    let (_dir, c, mut panel) = fixture();
    let original = panel.saved.message.clone();
    panel.focus = Focus::Diff;
    assert!(
        matches!(panel.launch(Hit::MessageAction(MessageAction::Generate), &c), Action::Notice(NoticeLevel::Error, message) if message.contains("ai.endpoint") && message.contains("ai.model"))
    );
    assert!(panel.edit.is_none());
    assert!(panel.focus == Focus::Diff);
    assert!(!panel.ai_running());
    assert_eq!(panel.saved.message, original);
}

#[test]
fn successful_generation_preserves_a_newer_draft_or_failed_save() {
    for edited in [false, true] {
        let (_dir, mut c, mut panel) = fixture();
        let tx = pending(&mut panel, &c);
        if edited {
            panel.edit_message(&c);
            panel.buf = "User's newer message 中文".into();
            panel.finish_message(&c).unwrap();
        } else {
            c.state_dir = c.workspace.join("blocked");
            fs::write(&c.state_dir, "file").unwrap();
        }
        let original = panel.saved.message.clone();
        tx.send(checked("feat: generated message")).unwrap();
        let focus = panel.focus;
        assert!(matches!(
            wait(&mut panel, &c),
            Action::Notice(NoticeLevel::Error, _)
        ));
        assert_eq!(panel.saved.message, original);
        assert!(panel.edit.is_none());
        assert!(panel.focus == focus);
        assert!(!panel.ai_running());
    }
}

#[test]
fn completion_waits_for_active_input_and_keeps_unsaved_edits() {
    let (_dir, c, mut panel) = fixture();
    let tx = pending(&mut panel, &c);
    panel.edit_message(&c);
    panel.buf = "User's unsaved message 中文".into();
    tx.send(checked("feat: generated message")).unwrap();
    assert!(matches!(panel.tick(&c), Action::None));
    assert!(panel.message_editing());
    assert!(panel.ai_running());
    panel.finish_message(&c).unwrap();
    assert!(matches!(
        wait(&mut panel, &c),
        Action::Notice(NoticeLevel::Error, _)
    ));
    assert_eq!(
        panel.saved.message.as_deref(),
        Some("User's unsaved message 中文")
    );
    assert!(panel.edit.is_none());
    assert!(panel.focus == Focus::Message);
}

#[test]
fn loading_animates_in_the_composer_and_success_fills_it_without_a_dialog() {
    let (_dir, c, mut panel) = fixture();
    for (width, height) in [(44, 12), (80, 24), (120, 36)] {
        panel.icons = IconMode::Ascii;
        let tx = pending(&mut panel, &c);
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(width, height)).unwrap();
        terminal
            .draw(|frame| panel.draw(frame, frame.area(), &c))
            .unwrap();
        let before = terminal.backend().buffer().clone();
        assert!(panel
            .hits
            .iter()
            .any(|(_, hit)| matches!(hit, Hit::MessageAction(MessageAction::Generate))));
        assert!(matches!(panel.tick(&c), Action::None));
        terminal
            .draw(|frame| panel.draw(frame, frame.area(), &c))
            .unwrap();
        assert_ne!(terminal.backend().buffer(), &before);
        tx.send(checked("feat: generated message")).unwrap();
        assert!(matches!(
            panel.tick(&c),
            Action::Notice(NoticeLevel::Success, _)
        ));
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

#[test]
fn large_multi_repo_selection_summarizes_every_patch_then_saves_one_message() {
    let (_dir, c, mut panel) = fixture();
    let mut expected = Vec::new();
    let mut before = Vec::new();
    for (index, repo) in c.repos.iter().enumerate() {
        let patch = (0..2200)
            .map(|line| format!("{} change {line:04} 中文 \\\" material value\n", repo.name))
            .collect::<String>();
        fs::write(repo.path.join("mixed.txt"), &patch).unwrap();
        crate::core::git(&repo.path, &["add", "mixed.txt"]).unwrap();
        fs::write(repo.path.join("mixed.txt"), "working private marker\n").unwrap();
        expected.push((index, patch));
        before.push(crate::core::git(&repo.path, &["ls-files", "--stage"]).unwrap());
    }
    let prompt = panel.ai_settings.prompt.clone();
    let final_prompt = prompt.clone();
    let (endpoint, received, worker) = service(move |payload| {
        let final_request = payload["messages"][0]["content"] == final_prompt;
        if final_request {
            (
                200,
                response("feat: summarize both repositories 中文"),
                true,
            )
        } else {
            (200, response("Updated material behavior and tests."), false)
        }
    });
    panel.ai_settings = ai_settings(endpoint, 32_000);
    panel.start_ai(&c).unwrap();
    let action = wait(&mut panel, &c);
    assert!(
        matches!(action, Action::Notice(NoticeLevel::Success, ref message) if message.contains("generated")),
        "{:?}",
        panel.ai_error
    );
    worker.join().unwrap();
    let requests: Vec<_> = received.try_iter().collect();
    assert!(requests.len() > 2);
    for request in &requests {
        assert!(serde_json::to_vec(request).unwrap().len() <= 32_000);
        assert!(!request.to_string().contains("private marker"));
    }
    let all = requests.iter().map(ToString::to_string).collect::<String>();
    assert!(all.contains("app") && all.contains("api"));
    assert!(all.contains("change 0000") && all.contains("change 2199"));
    assert_eq!(requests.last().unwrap()["messages"][0]["content"], prompt);
    assert_eq!(
        panel.saved.message.as_deref(),
        Some("feat: summarize both repositories 中文")
    );
    for ((index, staged), repo) in expected.iter().zip(&c.repos) {
        assert_eq!(
            crate::core::git(&repo.path, &["ls-files", "--stage"]).unwrap(),
            before[*index]
        );
        assert_eq!(
            crate::core::git(&repo.path, &["show", ":mixed.txt"]).unwrap(),
            staged.trim_end()
        );
        assert_eq!(
            fs::read_to_string(repo.path.join("mixed.txt")).unwrap(),
            "working private marker\n"
        );
        assert_eq!(
            crate::core::git(&repo.path, &["rev-list", "--count", "HEAD"]).unwrap(),
            "1"
        );
    }
}

#[test]
fn intermediate_http_failure_keeps_the_draft_and_never_sends_final_generation() {
    let (_dir, c, mut panel) = fixture();
    let repo = &c.repos[0].path;
    fs::write(
        repo.join("mixed.txt"),
        "selected change 中文\n".repeat(2000),
    )
    .unwrap();
    crate::core::git(repo, &["add", "mixed.txt"]).unwrap();
    let original = panel.saved.message.clone();
    let mut count = 0;
    let (endpoint, received, worker) = service(move |_| {
        count += 1;
        if count == 2 {
            (503, "sensitive response patch".into(), true)
        } else {
            (200, response("Changed selected behavior."), false)
        }
    });
    panel.ai_settings = ai_settings(endpoint, 8192);
    panel.start_ai(&c).unwrap();
    wait(&mut panel, &c);
    worker.join().unwrap();
    assert_eq!(received.try_iter().count(), 2);
    assert_eq!(panel.saved.message, original);
    assert!(panel.ai_error.contains("503"), "{}", panel.ai_error);
    assert!(!panel.ai_error.contains("sensitive"));
}

#[test]
fn request_limit_fails_before_network_and_keeps_the_draft() {
    let (_dir, c, mut panel) = fixture();
    let repo = &c.repos[0].path;
    fs::write(repo.join("mixed.txt"), "large change\n".repeat(2000)).unwrap();
    crate::core::git(repo, &["add", "mixed.txt"]).unwrap();
    let original = panel.saved.message.clone();
    panel.ai_settings = ai_settings("http://127.0.0.1:1/mock".into(), 8192);
    panel.ai_settings.max_requests = 1;
    panel.start_ai(&c).unwrap();
    wait(&mut panel, &c);
    assert_eq!(panel.saved.message, original);
    assert!(
        panel.ai_error.contains("max_requests"),
        "{}",
        panel.ai_error
    );
    assert!(!panel.ai_error.contains("curl"), "{}", panel.ai_error);
}

#[test]
fn stale_selection_or_roots_cancels_the_job_and_discards_completion() {
    for change_scope in [false, true] {
        let (_dir, mut c, mut panel) = fixture();
        let original = panel.saved.message.clone();
        let sender = pending(&mut panel, &c);
        let control = panel.ai_pending.as_ref().unwrap().control.clone();
        if change_scope {
            c.scope.remove(&1);
        } else {
            let key = panel.selection(&c)[0].key.clone();
            panel.checks.insert(key, false);
        }
        sender.send(checked("obsolete message")).unwrap();
        panel.tick(&c);
        assert!(control.check().is_err());
        assert_eq!(panel.saved.message, original);
        assert!(
            panel.ai_error.contains("Roots changed"),
            "{}",
            panel.ai_error
        );
        assert!(!panel.ai_running());
    }
}

#[test]
#[cfg(unix)]
fn preparation_runs_in_the_background_and_ui_progress_remains_available() {
    let (_dir, c, mut panel) = fixture();
    let repo = &c.repos[0].path;
    let selected = panel
        .files
        .iter_mut()
        .find(|file| file.root == 0 && file.change.path == Path::new("mixed.txt"))
        .unwrap();
    crate::core::git(repo, &["reset", "-q", "HEAD", "--", "mixed.txt"]).unwrap();
    selected.change = backend::status(repo)
        .unwrap()
        .into_iter()
        .find(|file| file.path == Path::new("mixed.txt"))
        .unwrap();
    // An external index lock would not block reads, but a delayed Git filter does.
    // Use a file handshake so the test proves preparation and UI overlap.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let script = repo.join(".git/ai-filter.sh");
        fs::write(&script, "#!/bin/sh\n: > .git/ai-filter-started\nwhile [ ! -f .git/ai-filter-release ]; do sleep 0.02; done\ncat\n").unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
        crate::core::git(
            repo,
            &["config", "filter.ai-test.clean", "sh .git/ai-filter.sh"],
        )
        .unwrap();
        fs::write(repo.join(".gitattributes"), "mixed.txt filter=ai-test\n").unwrap();
        let (endpoint, _, worker) = server(200, completion());
        panel.ai_settings = ai_settings(endpoint, 160_000);
        let started = Instant::now();
        panel.start_ai(&c).unwrap();
        assert!(
            started.elapsed() < Duration::from_secs(1),
            "UI waited for patch preparation"
        );
        let deadline = Instant::now() + Duration::from_secs(10);
        while !repo.join(".git/ai-filter-started").exists() {
            panel.tick(&c);
            assert!(panel.ai_running(), "{}", panel.ai_error);
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(5));
        }
        assert!(panel.ai_progress().contains("Preparing"));
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(120, 36)).unwrap();
        terminal
            .draw(|frame| panel.draw(frame, frame.area(), &c))
            .unwrap();
        assert!(panel
            .hits
            .iter()
            .any(|(_, hit)| matches!(hit, Hit::MessageAction(MessageAction::Generate))));
        assert!(panel.start_ai(&c).is_err());
        fs::write(repo.join(".git/ai-filter-release"), "").unwrap();
        wait(&mut panel, &c);
        worker.join().unwrap();
        assert!(panel.ai_error.is_empty(), "{}", panel.ai_error);
    }
}

#[test]
fn changed_staged_source_is_rejected_after_http_but_unstaged_edits_are_allowed() {
    for change_index in [false, true] {
        let (_dir, c, mut panel) = fixture();
        let repo = c.repos[0].path.clone();
        let original = panel.saved.message.clone();
        let (endpoint, _, worker) = service(move |_| {
            fs::write(repo.join("mixed.txt"), "new working private marker\n").unwrap();
            if change_index {
                crate::core::git(&repo, &["add", "mixed.txt"]).unwrap();
            }
            (200, response("feat: original snapshot"), true)
        });
        panel.ai_settings = ai_settings(endpoint, 160_000);
        panel.start_ai(&c).unwrap();
        wait(&mut panel, &c);
        worker.join().unwrap();
        if change_index {
            assert_eq!(panel.saved.message, original);
            assert!(!panel.ai_error.is_empty());
        } else {
            assert_eq!(
                panel.saved.message.as_deref(),
                Some("feat: original snapshot"),
                "{}",
                panel.ai_error
            );
        }
    }
}

#[test]
fn deadline_expires_without_overwriting_a_draft_during_active_input() {
    let (_dir, c, mut panel) = fixture();
    let _sender = pending(&mut panel, &c);
    panel.ai_pending.as_mut().unwrap().control = Control::new(Duration::ZERO);
    panel.edit_message(&c);
    panel.buf = "User editing after timeout".into();
    let original = panel.saved.message.clone();
    assert!(
        matches!(panel.tick(&c), Action::Notice(NoticeLevel::Error, message) if message.contains("total_timeout_seconds"))
    );
    assert!(panel.message_editing());
    assert_eq!(panel.saved.message, original);
    assert_eq!(panel.buf, "User editing after timeout");
    assert!(!panel.ai_running());
    panel.finish_message(&c).unwrap();
    panel.tick(&c);
    assert_eq!(
        panel.saved.message.as_deref(),
        Some("User editing after timeout")
    );
    assert!(panel.ai_error.contains("total_timeout_seconds"));
}

#[test]
fn curl_request_timeout_and_job_deadline_keep_service_content_private() {
    for overall in [false, true] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let endpoint = format!("http://{}/mock", listener.local_addr().unwrap());
        let (arrived, received) = mpsc::channel();
        let (release, released) = mpsc::channel();
        let worker = thread::spawn(move || {
            let (_stream, _, _) = read_request(&listener);
            arrived.send(()).unwrap();
            // Hold the connection open without a response until the client stops.
            released.recv_timeout(Duration::from_secs(5)).unwrap();
        });
        let mut settings = ai_settings(endpoint, 8192);
        settings.timeout_seconds = 1;
        let control = Control::new(if overall {
            Duration::from_millis(300)
        } else {
            Duration::from_secs(5)
        });
        let client =
            transport::Client::with_credential(&settings, &control, Some("sensitive-token"))
                .unwrap();
        let started = Instant::now();
        let error = client
            .send(
                &settings.prompt,
                &serde_json::json!({"patch": "private-patch"}),
                None,
            )
            .unwrap_err()
            .to_string();
        assert!(started.elapsed() < Duration::from_secs(3), "{error}");
        received.recv_timeout(Duration::from_secs(1)).unwrap();
        release.send(()).unwrap();
        worker.join().unwrap();
        assert!(!error.contains("sensitive-token") && !error.contains("private-patch"));
        assert!(
            error.contains("timeout_seconds") || error.contains("configured limits"),
            "{error}"
        );
    }
}

#[test]
fn source_is_revalidated_after_completion_was_deferred_for_message_editing() {
    let (_dir, c, mut panel) = fixture();
    let control = Control::new(Duration::from_secs(30));
    let mut snapshots = Vec::new();
    for root in 0..c.repos.len() {
        let files = panel
            .selection(&c)
            .into_iter()
            .filter(|file| file.root == root)
            .map(|file| file.change.clone())
            .collect::<Vec<_>>();
        snapshots
            .push(backend::ai::Snapshot::capture(&c.repos[root].path, &files, &control).unwrap());
    }
    let original = panel.saved.message.clone();
    let sender = pending(&mut panel, &c);
    panel.edit_message(&c);
    sender
        .send(Report::Checked {
            revision: 0,
            completed: Instant::now(),
            result: Ok(Candidate {
                message: "feat: obsolete source".into(),
                snapshots,
            }),
        })
        .unwrap();
    panel.tick(&c);
    assert!(panel.message_editing());
    fs::write(
        c.repos[0].path.join("mixed.txt"),
        "a newer staged version\n",
    )
    .unwrap();
    crate::core::git(&c.repos[0].path, &["add", "mixed.txt"]).unwrap();
    panel.finish_message(&c).unwrap();
    wait(&mut panel, &c);
    assert_eq!(panel.saved.message, original);
    assert!(panel.ai_error.contains("changed"), "{}", panel.ai_error);
}

#[test]
fn queued_validation_is_repeated_after_an_intervening_editor_refresh_or_delayed_poll() {
    for intervening in ["editor", "refresh", "delay"] {
        let (_dir, c, mut panel) = fixture();
        let files = panel
            .selection(&c)
            .into_iter()
            .filter(|file| file.root == 0)
            .map(|file| file.change.clone())
            .collect::<Vec<_>>();
        let snapshot = backend::ai::Snapshot::capture(
            &c.repos[0].path,
            &files,
            &Control::new(Duration::from_secs(30)),
        )
        .unwrap();
        let original = panel.saved.message.clone();
        let sender = pending(&mut panel, &c);
        sender
            .send(Report::Checked {
                revision: 0,
                completed: Instant::now()
                    - if intervening == "delay" {
                        Duration::from_secs(2)
                    } else {
                        Duration::ZERO
                    },
                result: Ok(Candidate {
                    message: "feat: obsolete queued validation".into(),
                    snapshots: vec![snapshot],
                }),
            })
            .unwrap();
        if intervening == "editor" {
            panel.edit_message(&c);
        }
        fs::write(
            c.repos[0].path.join("mixed.txt"),
            "a newer staged version\n",
        )
        .unwrap();
        crate::core::git(&c.repos[0].path, &["add", "mixed.txt"]).unwrap();
        if intervening == "editor" {
            // Keep the text unchanged so only the source check protects adoption.
            panel.finish_message(&c).unwrap();
        } else if intervening == "refresh" {
            panel.request_refresh();
        }
        wait(&mut panel, &c);
        assert_eq!(panel.saved.message, original, "{intervening}");
        assert!(
            panel.ai_error.contains("changed"),
            "{intervening}: {}",
            panel.ai_error
        );
    }
}

#[test]
fn completion_preserves_search_input_and_open_panel_menus() {
    for search in [false, true] {
        let (_dir, c, mut panel) = fixture();
        let original = panel.saved.message.clone();
        let sender = pending(&mut panel, &c);
        panel.focus_pane(PaneId::Changes);
        if search {
            panel.event(
                &Event::Key(crossterm::event::KeyEvent::new(K::Char('/'), Mod::NONE)),
                &c,
            );
            panel.event(&Event::Paste("mixed".into()), &c);
            assert!(panel.focused_search().unwrap().editing());
        } else {
            panel.toggle_view_menu();
            assert!(panel.view_menu.is_some());
        }
        assert!(panel.editing());
        sender
            .send(checked("feat: deferred until input closes"))
            .unwrap();
        assert!(matches!(panel.tick(&c), Action::None));
        assert_eq!(panel.saved.message, original);
        assert!(panel.editing());
        panel.event(
            &Event::Key(crossterm::event::KeyEvent::new(K::Esc, Mod::NONE)),
            &c,
        );
        assert!(!panel.editing());
        assert!(matches!(
            wait(&mut panel, &c),
            Action::Notice(NoticeLevel::Success, _)
        ));
        assert_eq!(
            panel.saved.message.as_deref(),
            Some("feat: deferred until input closes")
        );
        assert!(panel.ai_error.is_empty());
    }
}

#[test]
fn changed_working_source_after_the_request_keeps_the_original_draft() {
    let (_dir, c, mut panel) = fixture();
    let repo = c.repos[0].path.clone();
    crate::core::git(&repo, &["reset", "-q", "HEAD", "--", "mixed.txt"]).unwrap();
    panel
        .files
        .iter_mut()
        .find(|file| file.root == 0 && file.change.path == Path::new("mixed.txt"))
        .unwrap()
        .change = backend::status(&repo)
        .unwrap()
        .into_iter()
        .find(|file| file.path == Path::new("mixed.txt"))
        .unwrap();
    let original = panel.saved.message.clone();
    let (endpoint, _, worker) = service(move |_| {
        fs::write(repo.join("mixed.txt"), "newer working source\n").unwrap();
        (200, response("feat: obsolete working source"), true)
    });
    panel.ai_settings = ai_settings(endpoint, 160_000);
    panel.start_ai(&c).unwrap();
    wait(&mut panel, &c);
    worker.join().unwrap();
    assert_eq!(panel.saved.message, original);
    assert!(panel.ai_error.contains("changed"), "{}", panel.ai_error);
}

#[cfg(unix)]
#[test]
fn cancelling_during_preparation_stops_the_filter_without_an_http_request() {
    let (_dir, c, mut panel) = fixture();
    let repo = &c.repos[0].path;
    crate::core::git(repo, &["reset", "-q", "HEAD", "--", "mixed.txt"]).unwrap();
    panel
        .files
        .iter_mut()
        .find(|file| file.root == 0 && file.change.path == Path::new("mixed.txt"))
        .unwrap()
        .change = backend::status(repo)
        .unwrap()
        .into_iter()
        .find(|file| file.path == Path::new("mixed.txt"))
        .unwrap();
    fs::write(
        repo.join(".git/ai-cancel-filter.sh"),
        "#!/bin/sh\nprintf '%s\\n' \"$$\" > .git/ai-filter-pid\nwhile :; do sleep 0.02; done\n",
    )
    .unwrap();
    crate::core::git(
        repo,
        &[
            "config",
            "filter.ai-test.clean",
            "sh .git/ai-cancel-filter.sh",
        ],
    )
    .unwrap();
    fs::write(repo.join(".gitattributes"), "mixed.txt filter=ai-test\n").unwrap();
    panel.ai_settings = ai_settings("http://127.0.0.1:1/mock".into(), 160_000);
    let original = panel.saved.message.clone();
    panel.start_ai(&c).unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while !repo.join(".git/ai-filter-pid").exists() {
        panel.tick(&c);
        assert!(panel.ai_running(), "{}", panel.ai_error);
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(5));
    }
    let pid = fs::read_to_string(repo.join(".git/ai-filter-pid")).unwrap();
    let key = panel.selection(&c)[0].key.clone();
    panel.checks.insert(key, false);
    wait(&mut panel, &c);
    assert_eq!(panel.saved.message, original);
    assert!(
        panel.ai_error.contains("Roots changed"),
        "{}",
        panel.ai_error
    );
    // The filter belongs to the killed Git process group, including its shell.
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let output = std::process::Command::new("ps")
            .args(["-o", "stat=", "-p", pid.trim()])
            .output()
            .unwrap();
        let live = output.status.success()
            && String::from_utf8_lossy(&output.stdout)
                .split_whitespace()
                .any(|state| !state.starts_with('Z'));
        if !live {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "Git's clean filter survived cancellation"
        );
        thread::sleep(Duration::from_millis(10));
    }
}
