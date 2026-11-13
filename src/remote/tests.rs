use super::*;
use crate::core::git;
use crossterm::event::{KeyEvent, MouseEvent};
use ratatui::{backend::TestBackend, Terminal};
use std::fs;

fn fixture() -> (tempfile::TempDir, Context, RemotePanel) {
    let temp = tempfile::tempdir().unwrap();
    for name in ["api", "web", "empty"] {
        let path = temp.path().join(name);
        fs::create_dir(&path).unwrap();
        git(&path, &["init", "-q", "-b", "main"]).unwrap();
        if name != "empty" {
            git(
                &path,
                &["remote", "add", "origin", &format!("../{name}.git")],
            )
            .unwrap();
        }
    }
    let mut c = Context::discover(temp.path()).unwrap();
    c.state_dir = temp.path().join("state");
    let mut panel = RemotePanel {
        icons: IconMode::Ascii,
        ..Default::default()
    };
    panel.refresh(&c).unwrap();
    (temp, c, panel)
}

fn key(panel: &mut RemotePanel, c: &Context, code: K) -> Action {
    panel.event(&Event::Key(KeyEvent::new(code, Mods::NONE)), c)
}

fn ctrl(panel: &mut RemotePanel, c: &Context, character: char) -> Action {
    panel.event(
        &Event::Key(KeyEvent::new(K::Char(character), Mods::CONTROL)),
        c,
    )
}

fn replace(panel: &mut RemotePanel, c: &Context, text: &str) {
    ctrl(panel, c, 'u');
    panel.event(&Event::Paste(text.into()), c);
}

fn root(c: &Context, name: &str) -> usize {
    c.repos.iter().position(|repo| repo.name == name).unwrap()
}

fn screen(panel: &mut RemotePanel, c: &Context, width: u16, height: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|f| panel.draw(f, f.area(), c)).unwrap();
    terminal
        .backend()
        .buffer()
        .content()
        .chunks(usize::from(width))
        .map(|row| row.iter().map(|cell| cell.symbol()).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn lists_every_scoped_repository_and_all_configured_urls() {
    let (_temp, mut c, mut panel) = fixture();
    let api = root(&c, "api");
    git(
        &c.repos[api].path,
        &["config", "--add", "remote.origin.url", "../second.git"],
    )
    .unwrap();
    git(
        &c.repos[api].path,
        &["config", "--add", "remote.origin.pushurl", "../publish.git"],
    )
    .unwrap();
    panel.refresh(&c).unwrap();
    let text = screen(&mut panel, &c, 120, 36);
    for expected in [
        "api",
        "web",
        "empty",
        "origin",
        "../second.git",
        "../publish.git",
        "No remotes",
    ] {
        assert!(text.contains(expected), "{text}");
    }
    c.scope = [api].into();
    panel.refresh(&c).unwrap();
    assert_eq!(panel.repos.len(), 1);
    assert!(panel.rows.iter().all(|row| row.root() == api));
}

#[test]
fn groups_navigation_and_canceled_editor_leave_configuration_unchanged() {
    let (_temp, c, mut panel) = fixture();
    let api = root(&c, "api");
    let before = fs::read(c.repos[api].path.join(".git/config")).unwrap();
    panel.restore(api, Some("origin"));
    key(&mut panel, &c, K::Left);
    assert_eq!(panel.rows[panel.cursor], Row::Repository(api));
    key(&mut panel, &c, K::Left);
    assert!(panel.folded.contains(&api));
    key(&mut panel, &c, K::Right);
    assert!(!panel.folded.contains(&api));
    key(&mut panel, &c, K::Right);
    assert!(panel.selection().is_some());
    key(&mut panel, &c, K::Enter);
    assert!(panel.editing());
    replace(&mut panel, &c, "未保存");
    key(&mut panel, &c, K::Esc);
    assert!(!panel.editing());
    assert_eq!(
        fs::read(c.repos[api].path.join(".git/config")).unwrap(),
        before
    );
}

#[test]
fn editor_adds_multiline_endpoints_only_after_confirmation_in_the_selected_root() {
    let (_temp, c, mut panel) = fixture();
    let empty = root(&c, "empty");
    let web = root(&c, "web");
    let web_before = fs::read(c.repos[web].path.join(".git/config")).unwrap();
    panel.restore(empty, None);
    key(&mut panel, &c, K::Char('a'));
    replace(&mut panel, &c, "上游");
    key(&mut panel, &c, K::Tab);
    replace(&mut panel, &c, "../one.git\n../two.git");
    key(&mut panel, &c, K::Tab);
    replace(&mut panel, &c, "../push-one.git\n../push-two.git");
    assert!(model::read(&c.repos[empty].path).unwrap().is_empty());
    assert!(matches!(ctrl(&mut panel, &c, 's'), Action::Confirm { .. }));
    assert!(model::read(&c.repos[empty].path).unwrap().is_empty());
    assert!(matches!(
        panel.confirm(REVIEW_TOKEN, &c),
        Action::Refresh(NoticeLevel::Success, _)
    ));
    panel.refresh(&c).unwrap();
    let remotes = model::read(&c.repos[empty].path).unwrap();
    assert_eq!(remotes[0].name, "上游");
    assert_eq!(remotes[0].urls, ["../one.git", "../two.git"]);
    assert_eq!(remotes[0].push_urls, ["../push-one.git", "../push-two.git"]);
    assert_eq!(panel.selection().unwrap().1.name, "上游");
    assert_eq!(
        fs::read(c.repos[web].path.join(".git/config")).unwrap(),
        web_before
    );
}

#[test]
fn editor_keeps_invalid_draft_and_edit_removal_require_review() {
    let (_temp, c, mut panel) = fixture();
    let api = root(&c, "api");
    panel.restore(api, Some("origin"));
    key(&mut panel, &c, K::Char('e'));
    replace(&mut panel, &c, "-invalid");
    assert!(matches!(
        ctrl(&mut panel, &c, 's'),
        Action::Notice(NoticeLevel::Error, _)
    ));
    assert!(panel.editing());
    replace(&mut panel, &c, "upstream");
    assert!(matches!(ctrl(&mut panel, &c, 's'), Action::Confirm { .. }));
    assert_eq!(model::read(&c.repos[api].path).unwrap()[0].name, "origin");
    assert!(matches!(
        panel.confirm(REVIEW_TOKEN, &c),
        Action::Refresh(NoticeLevel::Success, _)
    ));
    panel.refresh(&c).unwrap();
    assert_eq!(panel.selection().unwrap().1.name, "upstream");
    assert!(matches!(
        key(&mut panel, &c, K::Delete),
        Action::Confirm { .. }
    ));
    assert_eq!(model::read(&c.repos[api].path).unwrap().len(), 1);
    assert!(matches!(
        panel.confirm(REVIEW_TOKEN, &c),
        Action::Refresh(NoticeLevel::Success, _)
    ));
    panel.refresh(&c).unwrap();
    assert!(model::read(&c.repos[api].path).unwrap().is_empty());
    assert_eq!(panel.rows[panel.cursor], Row::Repository(api));
}

#[test]
fn stale_review_keeps_the_draft_and_external_configuration() {
    let (_temp, c, mut panel) = fixture();
    let api = root(&c, "api");
    panel.restore(api, Some("origin"));
    key(&mut panel, &c, K::Char('e'));
    key(&mut panel, &c, K::Tab);
    replace(&mut panel, &c, "../draft.git");
    assert!(matches!(ctrl(&mut panel, &c, 's'), Action::Confirm { .. }));
    git(
        &c.repos[api].path,
        &["remote", "set-url", "origin", "../external.git"],
    )
    .unwrap();
    assert!(matches!(
        panel.confirm(REVIEW_TOKEN, &c),
        Action::Refresh(NoticeLevel::Error, _)
    ));
    assert!(panel.editing());
    assert_eq!(
        panel.editor.as_ref().unwrap().target().urls,
        ["../draft.git"]
    );
    assert_eq!(
        model::read(&c.repos[api].path).unwrap()[0].urls,
        ["../external.git"]
    );
}

#[test]
fn mouse_toolbar_and_editor_hit_targets_fit_supported_sizes() {
    let (_temp, c, mut panel) = fixture();
    for (width, height) in [(120, 34), (80, 22), (44, 10)] {
        let text = screen(&mut panel, &c, width, height);
        assert!(text.contains("Name") && text.contains("URL"), "{text}");
        let (rect, _) = panel
            .hits
            .iter()
            .find(|(_, hit)| matches!(hit, Hit::Add))
            .unwrap();
        let mouse = Event::Mouse(MouseEvent {
            kind: M::Down(MouseButton::Left),
            column: rect.x,
            row: rect.y,
            modifiers: Mods::NONE,
        });
        panel.event(&mouse, &c);
        let text = screen(&mut panel, &c, width, height);
        assert!(
            text.contains("Fetch URLs") && text.contains("Push URLs"),
            "{text}"
        );
        assert!(text.contains("Review") && text.contains("Cancel"), "{text}");
        key(&mut panel, &c, K::Tab);
        panel.event(&Event::Paste("../仓库 with spaces.git".into()), &c);
        key(&mut panel, &c, K::Home);
        key(&mut panel, &c, K::Right);
        key(&mut panel, &c, K::Backspace);
        screen(&mut panel, &c, width, height);
        key(&mut panel, &c, K::Esc);
    }
}

#[test]
fn write_guard_and_release_events_do_not_prepare_remote_changes() {
    let (_temp, c, mut panel) = fixture();
    let api = root(&c, "api");
    panel.restore(api, Some("origin"));
    panel.block_writes(true);
    assert!(matches!(
        key(&mut panel, &c, K::Char('d')),
        Action::Notice(NoticeLevel::Warning, _)
    ));
    assert!(panel.review.is_none());
    let event = Event::Key(KeyEvent::new_with_kind(
        K::Char('e'),
        Mods::NONE,
        KeyEventKind::Release,
    ));
    panel.event(&event, &c);
    assert!(!panel.editing());
}

#[test]
fn remote_navigation_uses_gg_for_first_and_uppercase_g_for_last() {
    let (_temp, c, mut panel) = fixture();
    assert!(panel.rows.len() > 2);
    panel.cursor = 1;
    key(&mut panel, &c, K::Char('g'));
    assert_eq!(panel.cursor, 1);
    assert_eq!(panel.key_sequence(), Some(KeySequence::FirstItem));
    key(&mut panel, &c, K::Char('g'));
    assert_eq!(panel.cursor, 0);
    key(&mut panel, &c, K::Char('G'));
    assert_eq!(panel.cursor, panel.rows.len() - 1);
    key(&mut panel, &c, K::Home);
    assert_eq!(panel.cursor, 0);
    key(&mut panel, &c, K::End);
    assert_eq!(panel.cursor, panel.rows.len() - 1);
}
