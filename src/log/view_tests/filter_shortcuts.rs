use super::*;
use crate::ui::key_sequence::KeySequence;
use ratatui::style::Modifier;

#[test]
fn log_space_filters_keep_labels_values_and_clear_targets() {
    let (_repo, context, mut panel) = navigation_history();
    panel.filter.branch = Some("feature/中文-long-branch-name".into());
    panel.filter.author = "User 中文".into();
    for icons in [IconMode::Ascii, IconMode::Unicode, IconMode::NerdFont] {
        panel.icons = icons;
        for (width, height) in [(44, 12), (80, 24), (120, 36), (180, 36)] {
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            render(&mut panel, &mut terminal, &context);
            let targets = panel.filter_shortcut_targets().unwrap();
            let before = terminal.backend().buffer().clone();
            key(&mut panel, K::Char(' '), Mods::NONE, &context);
            render(&mut panel, &mut terminal, &context);
            assert_eq!(panel.filter_shortcut_targets().unwrap(), targets);
            for rect in targets {
                let cell = &terminal.backend().buffer()[(rect.x, rect.y)];
                assert!(cell
                    .modifier
                    .contains(Modifier::BOLD | Modifier::UNDERLINED));
                for offset in 0..rect.width {
                    assert_eq!(
                        terminal.backend().buffer()[(rect.x + offset, rect.y)].symbol(),
                        before[(rect.x + offset, rect.y)].symbol()
                    );
                }
            }
            panel.cancel_key_sequence();
        }
    }
    let mut terminal = Terminal::new(TestBackend::new(180, 36)).unwrap();
    render(&mut panel, &mut terminal, &context);
    let clear = target(&panel, |hit| matches!(hit, Hit::Clear(Field::Author)));
    key(&mut panel, K::Char(' '), Mods::NONE, &context);
    render(&mut panel, &mut terminal, &context);
    assert_eq!(
        target(&panel, |hit| matches!(hit, Hit::Clear(Field::Author))),
        clear
    );
    assert!(!terminal.backend().buffer()[(clear.x, clear.y)]
        .modifier
        .contains(Modifier::UNDERLINED));
    click_at(&mut panel, clear, &context);
    assert!(panel.filter.author.is_empty());
    assert_eq!(
        panel
            .filter
            .branch
            .as_ref()
            .and_then(|branch| branch.single()),
        Some("feature/中文-long-branch-name")
    );
    assert!(panel.key_sequence().is_none());
}

#[test]
fn log_space_filters_incomplete_toolbar_keeps_shortcuts_and_pick_mode_keeps_space() {
    let (_repo, context, mut panel) = navigation_history();
    let mut terminal = Terminal::new(TestBackend::new(18, 12)).unwrap();
    render(&mut panel, &mut terminal, &context);
    assert!(panel.filter_shortcut_targets().is_none());
    key(&mut panel, K::Char(' '), Mods::NONE, &context);
    assert_eq!(panel.key_sequence(), Some(KeySequence::LogFilters));
    key(&mut panel, K::Char('p'), Mods::NONE, &context);
    assert!(matches!(panel.editor, Some((Field::Path, _))));
    key(&mut panel, K::Esc, Mods::NONE, &context);
    key(&mut panel, K::Char('v'), Mods::NONE, &context);
    key(&mut panel, K::Char(' '), Mods::NONE, &context);
    assert_eq!(panel.picks.len(), 1);
    assert!(panel.key_sequence().is_none());
}
