use super::{test_support::*, *};
use crate::config::keys::parse_sequence;

fn send(app: &mut Shell, keys: &str) {
    for key in parse_sequence(keys).unwrap() {
        app.dispatch(&Event::Key(key));
    }
}

fn focus_file(app: &mut Shell, name: &str) {
    let text = screen(app, 140, 36);
    let (row, line, col) = text
        .lines()
        .enumerate()
        .find_map(|(y, line)| line.find(name).map(|x| (y, line, x)))
        .unwrap_or_else(|| panic!("Missing {name}: {text}"));
    app.dispatch(&Event::Mouse(crossterm::event::MouseEvent {
        column: line[..col].width() as u16,
        row: row as u16,
        kind: MouseEventKind::Down(MouseButton::Left),
        modifiers: KeyModifiers::NONE,
    }));
}

#[test]
fn edit_file_uses_highlight_not_checked_paths_and_keeps_index_head_and_draft() {
    let (_dir, mut app) = fixture();
    let repo = app.ctx.repos[0].path.clone();
    let before = snapshot(&repo);
    focus_file(&mut app, "mixed.txt");
    send(&mut app, "Space");
    let before_screen = screen(&mut app, 140, 36);
    for binding in ["e", "E"] {
        send(&mut app, binding);
        let request = app.pending_edit.take().unwrap();
        assert_eq!(request.repo, repo);
        assert_eq!(request.path, repo.join("mixed.txt"));
        assert!(app.pop.is_none());
    }
    assert_eq!(screen(&mut app, 140, 36), before_screen);
    assert_eq!(snapshot(&repo), before);
    send(&mut app, "Enter e"); // Diff focus cannot edit another object implicitly.
    assert!(app.pending_edit.is_none());
    send(&mut app, "Esc i e"); // Text input owns the character.
    assert!(app.commit.editing());
    assert!(app.pending_edit.is_none());
    assert_eq!(snapshot(&repo), before);
}

#[test]
fn edit_file_respects_container_scope_remapping_help_and_message_editing() {
    let (_dir, mut app) = fixture();
    send(&mut app, "g e");
    assert!(app.pending_edit.is_none());
    assert!(app.notifications.latest_body().contains("Select a file"));
    app.config = AppConfig::from_toml(
        "[keybindings.global]\nnotification-history=[]\n[keybindings.commit]\nedit-file = ['F8']",
    )
    .unwrap();
    focus_file(&mut app, "other.txt");
    send(&mut app, "e E");
    assert!(app.pending_edit.is_none());
    send(&mut app, "F8");
    assert!(app.pending_edit.take().unwrap().path.ends_with("other.txt"));
    send(&mut app, "c F8");
    assert!(app.commit.message_editing());
    assert!(app.pending_edit.is_none());
    send(&mut app, "Esc Tab F1");
    for ch in "Edit highlighted working file".chars() {
        app.dispatch(&key(KeyCode::Char(ch)));
    }
    send(&mut app, "Enter");
    assert!(app.pending_edit.take().unwrap().path.ends_with("other.txt"));
}

#[test]
fn show_file_at_revision_dispatches_from_both_panes_and_supports_remapping_and_help() {
    let (_dir, mut app) = fixture();
    let repo = app.ctx.repos[0].path.clone();
    let before = snapshot(&repo);
    app.log
        .set_file_history(&app.ctx, &repo.join("mixed.txt"))
        .unwrap();
    app.go(Log);
    app.log.refresh(&app.ctx).unwrap();
    app.config =
        AppConfig::from_toml("[keybindings.log]\nshow-file-at-revision = ['Ctrl-e']").unwrap();
    let rendered = screen(&mut app, 140, 36);
    assert!(rendered.contains("Show File at Revision"), "{rendered}");
    for navigation in ["", "Tab"] {
        if !navigation.is_empty() {
            send(&mut app, navigation);
        }
        send(&mut app, "e");
        assert!(app.pending_edit.is_none());
        send(&mut app, "Ctrl-e");
        let request = app.pending_edit.take().unwrap();
        assert_eq!(request.repo, repo);
        assert_eq!(request.path, std::path::Path::new("mixed.txt"));
        assert_eq!(request.revision.as_deref(), Some(before[0].as_str()));
    }
    send(&mut app, "F1");
    for ch in "Show File at Revision".chars() {
        app.dispatch(&key(KeyCode::Char(ch)));
    }
    send(&mut app, "Enter");
    assert!(app.pending_edit.take().unwrap().revision.is_some());
    send(&mut app, "Esc / e");
    assert!(app.pending_edit.is_none());
    assert_eq!(snapshot(&repo), before);
}
