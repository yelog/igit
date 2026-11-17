use super::*;

fn toolbar(panel: &PushPanel) -> Rect {
    let pane = panel.panes.rect(PaneId::Outgoing).unwrap();
    Rect::new(pane.x + 1, pane.y + 1, pane.width - 2, 1)
}

fn first_row(panel: &PushPanel) -> Rect {
    panel
        .hits
        .iter()
        .find_map(|(area, hit)| matches!(hit, Hit::Row(0)).then_some(*area))
        .unwrap()
}

#[test]
fn refresh_indicator_keeps_cached_rows_and_strategy_controls_stable() {
    for icons in [IconMode::Ascii, IconMode::Unicode, IconMode::NerdFont] {
        for (width, height) in [(44, 12), (80, 24), (120, 36)] {
            let (mut panel, context) = fixture(icons);
            let before = draw(&mut panel, &context, width, height);
            let row = first_row(&panel);
            let row_text = text_at(&before, row);
            let checked = panel.checked.clone();
            panel.load.request();
            let completion = panel.load.hold();
            let start = Instant::now();
            assert!(!panel.tick_animation(start));

            let refreshing = draw(&mut panel, &context, width, height);
            assert_eq!(first_row(&panel), row);
            assert_eq!(text_at(&refreshing, row), row_text);
            let toolbar = toolbar(&panel);
            let label = text_at(&refreshing, toolbar);
            assert!(
                label.starts_with(&format!("{} Refresh", icons.spinner(0))),
                "{label}"
            );
            let strategy = panel
                .hits
                .iter()
                .find_map(|(area, hit)| matches!(hit, Hit::Strategy).then_some(*area))
                .unwrap();
            assert!(text_at(&refreshing, strategy).contains("Merge"));
            assert_eq!(strategy.right(), toolbar.right());
            assert_eq!(refreshing[(strategy.x - 1, toolbar.y)].symbol(), " ");
            assert_eq!(refreshing[(toolbar.x, toolbar.y)].fg, ACCENT);
            assert_eq!(refreshing[(toolbar.x + 2, toolbar.y)].fg, MUTED);
            assert!(!panel
                .hits
                .iter()
                .any(|(_, hit)| matches!(hit, Hit::Fetch | Hit::Send)));

            assert!(!panel.tick_animation(start + Duration::from_millis(99)));
            assert!(panel.tick_animation(start + Duration::from_millis(100)));
            let next = draw(&mut panel, &context, width, height);
            assert_eq!(
                next[(toolbar.x, toolbar.y)].symbol(),
                icons.spinner(1).to_string()
            );
            assert_eq!(first_row(&panel), row);
            assert_eq!(panel.checked, checked);
            assert!(matches!(panel.review(&context, false), Action::Notice(..)));

            completion.send(panel.entries.clone()).unwrap();
            panel.load.take_result().unwrap().unwrap();
            assert!(panel.tick_animation(start + Duration::from_millis(101)));
            let ready = draw(&mut panel, &context, width, height);
            assert!(text_at(&ready, toolbar).contains("fetch"));
            assert_eq!(first_row(&panel), row);
            assert_eq!(text_at(&ready, row), row_text);
            assert!(!panel.tick_animation(start + Duration::from_secs(1)));
        }
    }
}

#[test]
fn first_load_animates_and_read_errors_remain_static() {
    for icons in [IconMode::Ascii, IconMode::Unicode, IconMode::NerdFont] {
        for (width, height) in [(44, 12), (80, 24), (120, 36)] {
            let (mut panel, context) = fixture(icons);
            panel.entries.clear();
            panel.load.request();
            let start = Instant::now();
            panel.tick_animation(start);
            let first = draw(&mut panel, &context, width, height);
            let toolbar = toolbar(&panel);
            let label = text_at(&first, toolbar);
            assert!(
                label.starts_with(&format!("{} Loading", icons.spinner(0))),
                "{label}"
            );
            assert!(panel.tick_animation(start + Duration::from_millis(100)));
            panel.load.cancel();
            panel.load_error = Some("Read failed; press F5 to retry".into());
            assert!(panel.tick_animation(start + Duration::from_millis(101)));
            let failed = draw(&mut panel, &context, width, height);
            let label = text_at(&failed, toolbar);
            assert!(label.starts_with(" Read"), "{label}");
            assert_eq!(failed[(toolbar.x + 1, toolbar.y)].fg, RED);
            assert!(!panel.tick_animation(start + Duration::from_secs(1)));
        }
    }
}
