use super::*;
use std::time::{Duration, Instant};

fn heading(panel: &BranchPanel) -> Rect {
    let list = panel.viewport.area;
    Rect::new(list.x, list.y - 1, list.width, 1)
}

#[test]
fn refresh_indicator_preserves_list_geometry_and_view_options() {
    let (_temp, context, mut panel) = fixture();
    tracking(&context, "main", "main");
    reload(&mut panel, &context);
    let mut baselines = Vec::new();
    for icons in [IconMode::Ascii, IconMode::Unicode, IconMode::NerdFont] {
        panel.icons = icons;
        for (width, height) in [(44, 12), (80, 24), (120, 36)] {
            let ready = render(&mut panel, &context, width, height);
            let row = hit_rect(&panel, |hit| matches!(hit, Hit::Row(0)));
            let eye = hit_rect(&panel, |hit| matches!(hit, Hit::ViewOptions));
            baselines.push((
                icons,
                width,
                height,
                panel.viewport.area,
                eye,
                row,
                row_cells(&ready, row),
                row_cells(&ready, heading(&panel)),
            ));
        }
    }
    let cursor = panel.tree.cursor;
    panel.refresh(&context).unwrap();
    let release = panel.hold_loading(&context);
    let start = Instant::now();
    assert!(!panel.tick_animation(start));
    for (icons, width, height, list, eye, row, row_text, old_heading) in baselines {
        panel.icons = icons;
        let refreshing = render(&mut panel, &context, width, height);
        let header = heading(&panel);
        let label = row_cells(&refreshing, header);
        assert!(
            label.starts_with(&format!("{} Refresh", icons.spinner(0))),
            "{label}"
        );
        for metadata in ["Upstream", "Last Commit"] {
            if let Some(offset) = old_heading.find(metadata) {
                let column = old_heading[..offset].width() as u16;
                let area = Rect::new(header.x + column, header.y, metadata.len() as u16, 1);
                assert_eq!(row_cells(&refreshing, area), metadata);
            }
        }
        assert_eq!(
            refreshing.backend().buffer()[(header.x, header.y)].fg,
            ACCENT
        );
        assert_eq!(panel.viewport.area, list);
        assert_eq!(hit_rect(&panel, |hit| matches!(hit, Hit::ViewOptions)), eye);
        assert_eq!(hit_rect(&panel, |hit| matches!(hit, Hit::Row(0))), row);
        assert_eq!(row_cells(&refreshing, row), row_text);
        assert_eq!(panel.tree.cursor, cursor);
        assert_eq!(header.y + 1, row.y);
    }
    assert!(!panel.tick_animation(start + Duration::from_millis(99)));
    assert!(panel.tick_animation(start + Duration::from_millis(100)));
    let next = render(&mut panel, &context, 120, 36);
    let header = heading(&panel);
    assert_eq!(
        next.backend().buffer()[(header.x, header.y)].symbol(),
        panel.icons.spinner(1).to_string()
    );
    release();
    panel.finish_loading(&context);
    assert!(panel.tick_animation(start + Duration::from_millis(101)));
    let ready = render(&mut panel, &context, 120, 36);
    assert!(row_cells(&ready, heading(&panel)).starts_with(" Branch"));
    assert!(!panel.tick_animation(start + Duration::from_secs(1)));
}

#[test]
fn first_load_has_one_indicator_and_empty_roots_stop_it() {
    let (_temp, mut context, _) = fixture();
    let mut panel = BranchPanel::default();
    panel.refresh(&context).unwrap();
    let start = Instant::now();
    panel.tick_animation(start);
    for icons in [IconMode::Ascii, IconMode::Unicode, IconMode::NerdFont] {
        panel.icons = icons;
        for (width, height) in [(44, 12), (80, 24), (120, 36)] {
            let loading = screen(&mut panel, &context, width, height);
            assert!(loading.contains(&format!("{} Loading branches…", icons.spinner(0))));
            assert_eq!(loading.matches("Loading").count(), 1);
            assert!(!loading.contains("No repositories"));
        }
    }
    assert!(panel.tick_animation(start + Duration::from_millis(100)));
    context.scope.clear();
    panel.refresh(&context).unwrap();
    assert!(panel.tick_animation(start + Duration::from_millis(101)));
    let empty = screen(&mut panel, &context, 80, 24);
    assert!(empty.contains("No repositories in Roots"));
    assert!(!empty.contains("Loading"));
}

#[test]
fn failed_refresh_keeps_cached_branches_and_stops_the_indicator() {
    let (_temp, context, mut panel) = fixture();
    panel.refresh(&context).unwrap();
    let release = panel.hold_loading(&context);
    let start = Instant::now();
    panel.tick_animation(start);
    assert!(panel.tick_animation(start + Duration::from_millis(100)));
    drop(release);
    assert!(panel.poll_loading(&context).is_err());
    assert!(panel.tick_animation(start + Duration::from_millis(101)));
    let failed = render(&mut panel, &context, 120, 36);
    let status = heading(&panel);
    assert!(row_cells(&failed, status).contains("Background reader stopped"));
    assert_eq!(
        failed.backend().buffer()[(status.x, status.y)].fg,
        idea::RED
    );
    assert!(screen(&mut panel, &context, 120, 36).contains("main (current)"));
    assert!(!panel.tick_animation(start + Duration::from_secs(1)));
}
