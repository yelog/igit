use super::*;
use crate::core::git;
use ratatui::{backend::TestBackend, Terminal};

fn select_mixed(panel: &mut CommitPanel) {
    panel.cursor = panel
        .view
        .iter()
        .position(|row| {
            row.file().is_some_and(|i| {
                panel.files[i].root == 0 && panel.files[i].change.path == Path::new("mixed.txt")
            })
        })
        .unwrap();
}

fn snapshot(c: &Context) -> (String, String, Vec<u8>) {
    let repo = &c.repos[0].path;
    (
        git(repo, &["rev-parse", "HEAD"]).unwrap(),
        git(repo, &["ls-files", "--stage"]).unwrap(),
        fs::read(repo.join("mixed.txt")).unwrap(),
    )
}

fn fixture() -> (tempfile::TempDir, Context, CommitPanel) {
    let (dir, c, panel) = super::super::message_tests::fixture();
    for repo in &c.repos {
        git(&repo.path, &["config", "core.autocrlf", "false"]).unwrap();
    }
    (dir, c, panel)
}

#[test]
fn hunk_view_browsing_and_review_cancellation_preserve_selection_and_git_state() {
    let (_dir, c, mut panel) = fixture();
    select_mixed(&mut panel);
    let before = snapshot(&c);
    let checks = panel.checks.clone();
    let message = panel.saved.message.clone();
    panel.open_staging(&c).unwrap();
    assert!(panel.editing());
    for (width, height) in [(44, 12), (80, 24), (120, 36)] {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal.draw(|f| panel.draw(f, f.area(), &c)).unwrap();
        let text: String = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        assert!(
            text.contains("Unstaged")
                && text.contains("Staged")
                && text.contains("Space Stage hunk"),
            "{text}"
        );
        let control = panel
            .staging
            .as_ref()
            .unwrap()
            .hits
            .iter()
            .find(|(_, control)| matches!(control, Control::Side(Side::Staged)))
            .unwrap()
            .0;
        panel.event(
            &Event::Mouse(crossterm::event::MouseEvent {
                kind: M::Down(MouseButton::Left),
                column: control.x,
                row: control.y,
                modifiers: Mod::NONE,
            }),
            &c,
        );
        assert_eq!(panel.staging.as_ref().unwrap().snapshot.side, Side::Staged);
        panel.event(
            &Event::Key(crossterm::event::KeyEvent::new(K::Tab, Mod::NONE)),
            &c,
        );
    }
    let review = panel.event(
        &Event::Key(crossterm::event::KeyEvent::new(K::Char(' '), Mod::NONE)),
        &c,
    );
    assert!(matches!(review, Action::Confirm { ref token, .. } if token == "commit-hunk"));
    panel.event(
        &Event::Key(crossterm::event::KeyEvent::new(K::Esc, Mod::NONE)),
        &c,
    );
    assert!(panel.staging.is_none() && panel.staging_plan.is_none());
    assert_eq!(snapshot(&c), before);
    assert_eq!(panel.checks, checks);
    assert_eq!(panel.saved.message, message);
}

#[test]
fn hunk_review_stages_only_its_file_and_rejects_external_edits() {
    let (_dir, c, mut panel) = fixture();
    select_mixed(&mut panel);
    let before = snapshot(&c);
    let unrelated = git(&c.repos[0].path, &["show", ":unselected.txt"]).unwrap();
    panel.open_staging(&c).unwrap();
    panel.event(
        &Event::Key(crossterm::event::KeyEvent::new(K::Enter, Mod::NONE)),
        &c,
    );
    fs::write(c.repos[0].path.join("mixed.txt"), "changed after review\n").unwrap();
    assert!(matches!(
        panel.confirm_staging(&c),
        Action::Notice(NoticeLevel::Error, _)
    ));
    assert_eq!(
        git(&c.repos[0].path, &["ls-files", "--stage"]).unwrap(),
        before.1
    );
    fs::write(c.repos[0].path.join("mixed.txt"), &before.2).unwrap();
    panel
        .staging
        .as_mut()
        .unwrap()
        .reload(Side::Unstaged)
        .unwrap();
    panel.event(
        &Event::Key(crossterm::event::KeyEvent::new(K::Enter, Mod::NONE)),
        &c,
    );
    assert!(matches!(
        panel.confirm_staging(&c),
        Action::Refresh(NoticeLevel::Success, _)
    ));
    assert_eq!(
        git(&c.repos[0].path, &["show", ":mixed.txt"]).unwrap(),
        "unstaged private marker"
    );
    assert_eq!(
        git(&c.repos[0].path, &["show", ":unselected.txt"]).unwrap(),
        unrelated
    );
    assert_eq!(
        fs::read(c.repos[0].path.join("mixed.txt")).unwrap(),
        before.2
    );
    assert_eq!(
        git(&c.repos[0].path, &["rev-parse", "HEAD"]).unwrap(),
        before.0
    );
}
