use super::*;
use crate::ui::idea::{CHECK, DIM, GOLD};

fn control(panel: &PushPanel, predicate: impl Fn(&Hit) -> bool) -> Rect {
    panel
        .hits
        .iter()
        .find_map(|(area, hit)| predicate(hit).then_some(*area))
        .unwrap()
}

#[test]
fn push_panes_keep_complete_focus_borders_above_the_footer() {
    for icons in [IconMode::Ascii, IconMode::Unicode, IconMode::NerdFont] {
        for (width, height) in [(38, 10), (44, 12), (80, 24), (120, 36), (180, 36)] {
            for details in [false, true] {
                let (mut panel, context) = fixture(icons);
                panel.show_details = details;
                for focused in 0..if details { 3 } else { 2 } {
                    panel.focus = focused;
                    for maximized in [false, true] {
                        panel.panes.maximized = maximized;
                        let buffer = draw(&mut panel, &context, width, height);
                        let footer = control(&panel, |hit| matches!(hit, Hit::Tags));
                        for id in PushPanel::PANES {
                            let Some(area) = panel.panes.rect(id).filter(|area| !area.is_empty())
                            else {
                                continue;
                            };
                            let color = if id == panel.focused_pane() {
                                ACCENT
                            } else {
                                LINE
                            };
                            let bottom = area.bottom() - 1;
                            assert_eq!(
                                buffer[(area.x, bottom)].symbol(),
                                "└",
                                "{icons:?} {width}×{height} {id:?}"
                            );
                            assert_eq!(buffer[(area.right() - 1, bottom)].symbol(), "┘");
                            for column in area.x..area.right() {
                                assert_eq!(buffer[(column, bottom)].fg, color);
                            }
                            assert!(area.bottom() <= footer.y);
                            for (row, hit) in &panel.hits {
                                if matches!(hit, Hit::Row(_) | Hit::File(_))
                                    && area.intersects(*row)
                                {
                                    assert!(row.bottom() <= bottom);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn push_controls_show_mnemonics_and_keep_strategy_at_the_toolbar_right_edge() {
    for icons in [IconMode::Ascii, IconMode::Unicode, IconMode::NerdFont] {
        for (width, height) in [(38, 10), (44, 12), (80, 24), (120, 36), (180, 36)] {
            let (mut panel, context) = fixture(icons);
            for strategy in [Strategy::Merge, Strategy::Rebase] {
                panel.strategy = strategy;
                let buffer = draw(&mut panel, &context, width, height);
                let outgoing = panel.panes.rect(PaneId::Outgoing).unwrap();
                let fetch = control(&panel, |hit| matches!(hit, Hit::Fetch));
                let strategy = control(&panel, |hit| matches!(hit, Hit::Strategy));
                assert_eq!(strategy.y, fetch.y);
                assert_eq!(strategy.right(), outgoing.right() - 1);
                assert!(fetch.right() <= strategy.x);
                if width >= 80 {
                    assert_eq!(
                        text_at(&buffer, strategy),
                        format!("Update strategy: {}", panel.strategy.label())
                    );
                    assert!(text_at(&buffer, fetch).contains(" fetch "));
                }
                assert!(!text_at(&buffer, Rect::new(0, fetch.y, width / 2, 1)).contains("Target"));
                for (predicate, needle) in [
                    (
                        (|hit: &Hit| matches!(hit, Hit::Fetch)) as fn(&Hit) -> bool,
                        "f",
                    ),
                    (|hit: &Hit| matches!(hit, Hit::Strategy), "s"),
                    (|hit: &Hit| matches!(hit, Hit::Send), "P"),
                    (|hit: &Hit| matches!(hit, Hit::Tags), "F"),
                    (|hit: &Hit| matches!(hit, Hit::Hooks), "H"),
                ] {
                    let area = control(&panel, predicate);
                    let text = text_at(&buffer, area);
                    let column = text
                        .char_indices()
                        .find_map(|(offset, character)| {
                            character
                                .eq_ignore_ascii_case(&needle.chars().next().unwrap())
                                .then_some(area.x + text[..offset].width() as u16)
                        })
                        .unwrap_or_else(|| panic!("Mnemonic {needle} missing from {text:?}"));
                    let cell = &buffer[(column, area.y)];
                    assert_eq!(cell.symbol(), needle, "{text:?}");
                    assert_eq!(cell.fg, GOLD, "{text:?}");
                    assert!(cell
                        .modifier
                        .contains(Modifier::BOLD | Modifier::UNDERLINED));
                }
                for (index, (area, _)) in panel.hits.iter().enumerate() {
                    assert!(!area.is_empty());
                    assert!(area.right() <= width && area.bottom() <= height);
                    for (other, _) in panel.hits.iter().skip(index + 1) {
                        if area.y == fetch.y && other.y == fetch.y {
                            assert!(area.intersection(*other).is_empty());
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn push_option_shortcuts_keep_repository_selection_and_match_mouse_toggles() {
    let (mut panel, context) = fixture(IconMode::Ascii);
    let checked = panel.checked.clone();
    let selected = panel.selected();
    for focus in 0..3 {
        panel.show_details = true;
        panel.focus = focus;
        key(&mut panel, &context, KeyCode::Char('F'));
        key(&mut panel, &context, KeyCode::Char('H'));
        assert!(panel.follow_tags && panel.skip_hooks);
        draw(&mut panel, &context, 80, 24);
        let tags = control(&panel, |hit| matches!(hit, Hit::Tags));
        let hooks = control(&panel, |hit| matches!(hit, Hit::Hooks));
        click(&mut panel, &context, tags);
        click(&mut panel, &context, hooks);
        assert!(!panel.follow_tags && !panel.skip_hooks);
        for character in ['v', 'x'] {
            key(&mut panel, &context, KeyCode::Char(character));
        }
        assert!(!panel.follow_tags && !panel.skip_hooks);
        assert_eq!(panel.checked, checked);
        assert_eq!(panel.selected(), selected);
    }
}

#[test]
fn push_footer_checkboxes_share_the_repository_checkbox_style() {
    let (mut panel, context) = fixture(IconMode::Unicode);
    let buffer = draw(&mut panel, &context, 80, 24);
    let tags = control(&panel, |hit| matches!(hit, Hit::Tags));
    let hooks = control(&panel, |hit| matches!(hit, Hit::Hooks));
    let repository = control(&panel, |hit| matches!(hit, Hit::Check(0)));

    let tags = &buffer[(tags.x, tags.y)];
    let hooks = &buffer[(hooks.x, hooks.y)];
    let repository = &buffer[(repository.x, repository.y)];
    assert_eq!(tags.symbol(), panel.icons.check(false));
    assert_eq!(tags.fg, DIM);
    assert!(!tags.modifier.contains(Modifier::BOLD));
    assert_eq!(hooks.symbol(), panel.icons.check(true));
    assert_eq!(hooks.fg, CHECK);
    assert!(hooks.modifier.contains(Modifier::BOLD));
    assert_eq!(hooks.fg, repository.fg);
    assert_eq!(hooks.modifier, repository.modifier);
}

#[test]
fn push_controls_hide_unavailable_default_mnemonics() {
    let (mut panel, context) = fixture(IconMode::Ascii);
    panel.shortcut_labels = [
        ("f", "Ctrl-f"),
        ("s", "Ctrl-Alt-u"),
        ("P", "Ctrl-p"),
        ("F", "Ctrl-t"),
        ("H", "—"),
    ]
    .into_iter()
    .map(|(key, label)| (key, label.to_string()))
    .collect();
    for (width, height) in [(38, 10), (44, 12), (120, 36)] {
        let buffer = draw(&mut panel, &context, width, height);
        control(&panel, |hit| matches!(hit, Hit::Fetch));
        control(&panel, |hit| matches!(hit, Hit::Strategy));
        for (area, hit) in &panel.hits {
            if matches!(
                hit,
                Hit::Fetch | Hit::Strategy | Hit::Send | Hit::Tags | Hit::Hooks
            ) {
                assert!((area.x..area.right()).all(|column| {
                    buffer[(column, area.y)].fg != GOLD
                        && !buffer[(column, area.y)]
                            .modifier
                            .contains(Modifier::UNDERLINED)
                }));
            }
        }
    }
    panel.shortcut_labels.clear();
    panel.checked.clear();
    let buffer = draw(&mut panel, &context, 120, 36);
    let footer = control(&panel, |hit| matches!(hit, Hit::Cancel));
    assert!(
        (footer.right()..buffer.area.right()).all(|column| buffer[(column, footer.y)].fg != GOLD)
    );
    let (_sender, receiver) = mpsc::channel();
    panel.inflight = Some(receiver);
    panel.busy = "Pushing…".into();
    let buffer = draw(&mut panel, &context, 120, 36);
    let outgoing = panel.panes.rect(PaneId::Outgoing).unwrap();
    let toolbar = Rect::new(outgoing.x + 1, outgoing.y + 1, outgoing.width - 2, 1);
    assert!(text_at(&buffer, toolbar).contains("Update strategy: Merge"));
    assert!((toolbar.x..toolbar.right()).all(|column| buffer[(column, toolbar.y)].fg != GOLD));
    assert!(!panel
        .hits
        .iter()
        .any(|(_, hit)| matches!(hit, Hit::Strategy | Hit::Fetch | Hit::Send)));
}
