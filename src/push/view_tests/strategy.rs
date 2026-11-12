use super::*;
use crate::ui::idea::{BLUE, LOCAL_REF};

fn hit(panel: &PushPanel, predicate: impl Fn(&Hit) -> bool) -> Rect {
    panel.hits.iter().find(|(_, hit)| predicate(hit)).unwrap().0
}

fn activate(panel: &mut PushPanel, context: &Context, area: Rect) -> Action {
    panel.event(
        &Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: area.x,
            row: area.y,
            modifiers: KeyModifiers::NONE,
        }),
        context,
    )
}

#[test]
fn push_divergence_uses_branch_colours_and_keeps_both_counts_when_names_clip() {
    for icons in [IconMode::NerdFont, IconMode::Unicode, IconMode::Ascii] {
        for (width, height) in [(44, 12), (80, 24), (120, 36), (180, 36)] {
            let (mut panel, mut context) = fixture(icons);
            context.repos[0].name = "仓库-very-long-repository-name".repeat(3);
            panel.entries[0].source = "feature/long-local-branch-name".repeat(3);
            panel.entries[0].n = 1;
            panel.entries[0].behind = Some(2);
            panel.strategy = Strategy::Rebase;
            let checked = panel.checked.clone();
            let buffer = draw(&mut panel, &context, width, height);
            let row = panel
                .hits
                .iter()
                .find_map(|(area, hit)| matches!(hit, Hit::Row(0)).then_some(*area))
                .unwrap();
            let ahead = format!("{}1", icons.glyph(Icon::Ahead));
            let behind = format!("{}2", icons.glyph(Icon::Behind));
            let ahead_column = find_text(&buffer, row, &ahead);
            let behind_column = find_text(&buffer, row, &behind);
            assert_eq!(buffer[(ahead_column, row.y)].fg, LOCAL_REF);
            assert_eq!(buffer[(behind_column, row.y)].fg, BLUE);
            let strategy = hit(&panel, |hit| matches!(hit, Hit::Strategy));
            assert!(text_at(&buffer, strategy).contains("Rebase"));
            if width >= 80 {
                assert_eq!(text_at(&buffer, strategy), "Update strategy: Rebase");
            }
            assert!(matches!(
                activate(&mut panel, &context, strategy),
                Action::ToggleUpdateStrategy
            ));
            assert_eq!(panel.checked, checked);
        }
    }
}

#[test]
fn update_strategy_and_conflict_controls_respect_busy_state() {
    let (mut panel, context) = fixture(IconMode::Ascii);
    panel.conflicted.insert(0);
    draw(&mut panel, &context, 120, 36);
    let conflicts = hit(&panel, |hit| matches!(hit, Hit::Conflicts));
    assert!(matches!(
        activate(&mut panel, &context, conflicts),
        Action::OpenConflicts
    ));
    let (_sender, receiver) = mpsc::channel();
    panel.inflight = Some(receiver);
    draw(&mut panel, &context, 44, 12);
    assert!(!panel
        .hits
        .iter()
        .any(|(_, hit)| matches!(hit, Hit::Strategy)));
    assert!(matches!(
        panel.event(
            &Event::Key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE)),
            &context
        ),
        Action::Notice(NoticeLevel::Info, _)
    ));
}
