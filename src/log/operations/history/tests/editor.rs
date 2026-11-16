use super::*;
use crate::{
    log::*,
    ui::{
        icons::IconMode,
        selection::{TextLayout, Wrap},
    },
};
use crossterm::event::{KeyEvent, MouseEvent};
use ratatui::{backend::TestBackend, Terminal};

fn key(panel: &mut LogPanel, code: K, modifiers: Mods, c: &Context) -> Action {
    panel.event(&Event::Key(KeyEvent::new(code, modifiers)), c)
}

fn text(terminal: &Terminal<TestBackend>) -> String {
    let buffer = terminal.backend().buffer();
    let area = buffer.area;
    (area.y..area.bottom())
        .map(|row| {
            (area.x..area.right())
                .map(|column| buffer[(column, row)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn label(terminal: &Terminal<TestBackend>, needle: &str) -> (u16, u16) {
    let buffer = terminal.backend().buffer();
    for row in buffer.area.y..buffer.area.bottom() {
        let line = (buffer.area.x..buffer.area.right())
            .map(|column| buffer[(column, row)].symbol())
            .collect::<String>();
        if let Some((before, _)) = line.split_once(needle) {
            return (buffer.area.x + cells(before), row);
        }
    }
    panic!("Missing label {needle}: {}", text(terminal));
}

fn click(panel: &mut LogPanel, at: (u16, u16), c: &Context) -> Action {
    panel.event(
        &Event::Mouse(MouseEvent {
            kind: M::Down(MouseButton::Left),
            column: at.0,
            row: at.1,
            modifiers: Mods::NONE,
        }),
        c,
    )
}

fn panel(c: &Context, oid: &str) -> LogPanel {
    let mut panel = LogPanel {
        icons: IconMode::Ascii,
        ..Default::default()
    };
    panel.refresh(c).unwrap();
    let row = panel
        .visible
        .iter()
        .position(|index| panel.hist.revs[*index].oid == oid)
        .unwrap();
    panel.select_row(row);
    panel
}

#[test]
fn history_menu_opens_editable_plan_and_separate_review_at_narrow_and_wide_sizes() {
    for (width, height) in [(44, 12), (80, 24), (120, 36)] {
        let (dir, c, [_, first, head]) = fixture();
        let repo = dir.path();
        let mut panel = panel(&c, &first);
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        let before = snapshot(repo);
        key(&mut panel, K::Char('a'), Mods::NONE, &c);
        for _ in 0..Kind::ALL
            .iter()
            .position(|kind| *kind == Kind::History)
            .unwrap()
        {
            key(&mut panel, K::Char('j'), Mods::NONE, &c);
        }
        terminal
            .draw(|frame| panel.draw(frame, frame.area(), &c))
            .unwrap();
        label(&terminal, Kind::History.title());
        assert!(matches!(
            key(&mut panel, K::Enter, Mods::NONE, &c),
            Action::None
        ));
        assert_eq!(panel.keymap().context(), "Log / Linear History Plan");
        terminal
            .draw(|frame| panel.draw(frame, frame.area(), &c))
            .unwrap();
        label(&terminal, "Branch: main");
        label(&terminal, "1. pick");
        label(&terminal, "2. pick");
        let at = label(&terminal, "2. pick");
        click(&mut panel, at, &c);
        assert!(matches!(
            key(&mut panel, K::Char('s'), Mods::NONE, &c),
            Action::None
        ));
        assert_eq!(panel.keymap().context(), "Log / Squash Message");
        terminal
            .draw(|frame| panel.draw(frame, frame.area(), &c))
            .unwrap();
        label(&terminal, "Save F2");
        key(&mut panel, K::Char('u'), Mods::CONTROL, &c);
        panel.event(&Event::Paste("Squashed 中文\n\n# exact message".into()), &c);
        key(&mut panel, K::F(2), Mods::NONE, &c);
        assert_eq!(panel.keymap().context(), "Log / Linear History Plan");
        terminal
            .draw(|frame| panel.draw(frame, frame.area(), &c))
            .unwrap();
        label(&terminal, "2. squash");
        let at = label(&terminal, "Review F2");
        let Action::Confirm { title, body, token } = click(&mut panel, at, &c) else {
            panic!("History plan should request a separate review");
        };
        assert_eq!(title, Kind::History.title());
        assert_eq!(token, REVIEW_TOKEN);
        assert!(body.contains(&format!("1. pick {first}")));
        assert!(body.contains(&format!("2. squash {head}")));
        assert!(body.contains("Squashed 中文\n\n# exact message"));
        assert!(body.contains("Backup ref: refs/igit/backups/history-"));
        assert!(body.contains("Tracked working tree and index must remain clean"));
        assert!(body.contains("Log path filters do not narrow"));
        assert_eq!(snapshot(repo), before);
        let mut scroll = 0;
        let mut hits = Vec::new();
        for _ in 0..3 {
            terminal
                .draw(|frame| {
                    crate::ui::review::Dialog {
                        title: &title,
                        icons: IconMode::Ascii,
                        letter_keys: false,
                    }
                    .draw(
                        frame,
                        frame.area(),
                        |available| TextLayout::plain(&body, available, Wrap::Word),
                        &mut scroll,
                        false,
                        &mut hits,
                    );
                })
                .unwrap();
            label(&terminal, "Cancel");
            label(&terminal, "Confirm");
            scroll = scroll.saturating_add(12);
        }
        assert!(panel.commit_dialog.is_some());
        key(&mut panel, K::Esc, Mods::NONE, &c);
        assert!(panel.commit_dialog.is_none());
        assert!(panel.commit_review.is_none());
        assert!(matches!(panel.confirm(&token, &c), Action::None));
        assert_eq!(snapshot(repo), before);
        assert!(backups(repo).is_empty());
    }
}

#[test]
fn ui_fixup_reorder_and_confirmation_execute_once_and_only_after_review() {
    let (dir, c, [_, first, head]) = fixture();
    let repo = dir.path();
    let mut panel = panel(&c, &first);
    panel.open_history_plan(&c).unwrap();
    key(&mut panel, K::Char('j'), Mods::NONE, &c);
    key(&mut panel, K::Char('K'), Mods::SHIFT, &c);
    key(&mut panel, K::Char('j'), Mods::NONE, &c);
    key(&mut panel, K::Char('f'), Mods::NONE, &c);
    assert_eq!(run_git(repo, &["rev-parse", "HEAD"]), head);
    let Action::Confirm { body, token, .. } = key(&mut panel, K::F(2), Mods::NONE, &c) else {
        panic!("F2 must open history review");
    };
    assert!(body.contains(&format!("1. pick {head}")));
    assert!(body.contains(&format!("2. fixup {first}")));
    assert!(matches!(
        panel.confirm(&token, &c),
        Action::Refresh(NoticeLevel::Success, _)
    ));
    assert_eq!(run_git(repo, &["log", "-1", "--format=%B"]), "second");
    assert_eq!(run_git(repo, &["rev-list", "--count", "HEAD"]), "2");
    let after = snapshot(repo);
    assert!(matches!(panel.confirm(&token, &c), Action::None));
    assert_eq!(snapshot(repo), after);
}

#[test]
fn returning_to_plan_invalidates_old_review_and_preserves_edited_squash_draft() {
    let (dir, c, [_, first, _]) = fixture();
    let mut panel = panel(&c, &first);
    panel.open_history_plan(&c).unwrap();
    key(&mut panel, K::Char('j'), Mods::NONE, &c);
    key(&mut panel, K::Char('s'), Mods::NONE, &c);
    key(&mut panel, K::Char('u'), Mods::CONTROL, &c);
    panel.event(&Event::Paste("Kept draft".into()), &c);
    key(&mut panel, K::F(2), Mods::NONE, &c);
    let Action::Confirm { token, .. } = key(&mut panel, K::F(2), Mods::NONE, &c) else {
        panic!("Plan should produce a review");
    };
    let before = snapshot(dir.path());
    key(&mut panel, K::Char('m'), Mods::NONE, &c);
    assert!(matches!(panel.confirm(&token, &c), Action::None));
    assert_eq!(panel.keymap().context(), "Log / Squash Message");
    let mut terminal = Terminal::new(TestBackend::new(44, 12)).unwrap();
    terminal
        .draw(|frame| panel.draw(frame, frame.area(), &c))
        .unwrap();
    label(&terminal, "Kept draft");
    assert_eq!(snapshot(dir.path()), before);
}
