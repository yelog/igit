use super::*;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseEvent};
use ratatui::{
    backend::TestBackend,
    buffer::Buffer,
    style::{Modifier, Style},
    widgets::Paragraph,
    Terminal,
};
use unicode_width::UnicodeWidthStr;

fn key(code: KeyCode) -> Event {
    Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
}

fn mouse(kind: MouseEventKind, x: u16, y: u16) -> Event {
    Event::Mouse(MouseEvent {
        kind,
        column: x,
        row: y,
        modifiers: KeyModifiers::NONE,
    })
}

fn draw_live(center: &mut Center, width: u16, height: u16, icons: IconMode) -> Buffer {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal
        .draw(|frame| center.draw_live(frame, frame.area(), icons))
        .unwrap();
    terminal.backend().buffer().clone()
}

fn draw_history(center: &mut Center, state: &mut History, width: u16, height: u16) -> Buffer {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal
        .draw(|frame| center.draw_history(frame, frame.area(), IconMode::Unicode, state))
        .unwrap();
    terminal.backend().buffer().clone()
}

fn text(buffer: &Buffer) -> String {
    buffer
        .content
        .chunks(usize::from(buffer.area.width).max(1))
        .map(|row| row.iter().map(|cell| cell.symbol()).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn capacities_order_and_ids_survive_clear() {
    let mut center = Center::default();
    for index in 0..503 {
        assert_eq!(center.push(Level::Info, "Commit", index.to_string()), index);
    }
    assert_eq!(center.history().count(), 500);
    assert_eq!(center.history().next().unwrap().id, 502);
    assert_eq!(center.history().next_back().unwrap().id, 3);
    assert_eq!(
        center.live().iter().map(|live| live.id).collect::<Vec<_>>(),
        [502, 501, 500, 499]
    );
    let mut history = center.open_history(None);
    center.history_event(&key(KeyCode::Char('c')), &mut history);
    center.history_event(&key(KeyCode::Char('y')), &mut history);
    assert_eq!(center.history().count(), 0);
    assert!(center.live().is_empty());
    assert_eq!(center.push(Level::Success, "Push", "Done"), 503);
}

#[test]
fn expiry_is_exact_for_every_level_and_preserves_history() {
    let now = Instant::now();
    for (level, seconds) in [
        (Level::Success, 3),
        (Level::Info, 4),
        (Level::Warning, 6),
        (Level::Error, 8),
    ] {
        let mut center = Center::default();
        let id = center.push_at(level, "Commit".into(), "body".into(), now);
        let expires_at = now + Duration::from_secs(seconds);
        assert_eq!(center.live()[0].expires_at, expires_at);
        assert!(!center.expire(expires_at - Duration::from_nanos(1)));
        assert!(center.expire(expires_at));
        assert!(!center.expire(expires_at));
        assert_eq!(center.history().next().unwrap().id, id);
    }
}

#[test]
fn dismiss_all_only_clears_live_entries() {
    let mut center = Center::default();
    center.push(Level::Info, "Clipboard", "Copied");
    assert!(center.dismiss_all_live());
    assert!(!center.dismiss_all_live());
    assert_eq!(center.history().count(), 1);
}

#[test]
fn sanitization_preserves_unicode_and_exposes_terminal_controls() {
    let mut center = Center::default();
    center.push(
        Level::Warning,
        "提交\x1b\nCommit",
        "é 界 👩‍💻\n\t\x1b[31m\r\0\u{85}",
    );
    let entry = center.history().next().unwrap();
    assert_eq!(entry.source, "提交<ESC> Commit");
    assert_eq!(entry.body, "é 界 👩‍💻\n    <ESC>[31m<CR><0x00><0x85>");
    assert!(!entry
        .body
        .chars()
        .any(|character| character.is_control() && character != '\n'));
}

#[test]
fn utc_timestamp_covers_epoch_leap_day_and_day_rollover() {
    assert_eq!(timestamp(0, None), "1970-01-01 00:00:00 UTC");
    assert_eq!(timestamp(86_399, None), "1970-01-01 23:59:59 UTC");
    assert_eq!(timestamp(86_400, None), "1970-01-02 00:00:00 UTC");
    assert_eq!(timestamp(951_782_400, None), "2000-02-29 00:00:00 UTC");
    assert_eq!(timestamp(0, Some(-3600)), "1969-12-31 23:00:00");
    assert_eq!(timestamp(86_399, Some(28800)), "1970-01-02 07:59:59");
    assert_eq!(
        offset_from_ident("IGit <clock@invalid> 0 +0530"),
        Some(19800)
    );
    assert_eq!(
        offset_from_ident("IGit <clock@invalid> 0 -0330"),
        Some(-12600)
    );
    for invalid in ["+2460", "+0860", "0800", "+000", "oops", "+aa00"] {
        assert_eq!(offset_from_ident(invalid), None);
    }
}

#[test]
fn cards_follow_lazydb_geometry_and_clip_unicode_inside_the_viewport() {
    for icons in [IconMode::Ascii, IconMode::Unicode, IconMode::NerdFont] {
        for width in [24, 40, 80, 120] {
            let mut center = Center {
                utc_offset: None,
                ..Center::default()
            };
            for index in 0..4 {
                center.push(
                    Level::Warning,
                    format!("提交 {index} {}", "é👩‍💻".repeat(30)),
                    "界".repeat(200),
                );
            }
            let buffer = draw_live(&mut center, width, 38, icons);
            assert_eq!(center.hits.len(), 4);
            for (index, hit) in center.hits.iter().enumerate() {
                assert_eq!(hit.id, 3 - index as u64);
                assert_eq!(hit.area.right(), width);
                assert_eq!(hit.area.y, 1 + index as u16 * 9);
                assert_eq!(hit.area.height, 8);
                assert_eq!(hit.area.width, width.min(56));
                assert_eq!(buffer[(hit.area.x, hit.area.y)].fg, idea::GOLD);
                let close_x = hit.close.x + 1;
                assert_eq!(
                    buffer[(close_x, hit.close.y)].symbol(),
                    icons.glyph(Icon::Close)
                );
                assert!(text(&buffer).contains("UTC"));
            }
            assert!(text(&buffer).contains('…'));
            assert!(!text(&buffer).contains('\u{1b}'));
        }
    }
}

#[test]
fn cards_clear_underlying_styles_and_restore_background_after_expiry() {
    let mut center = Center::default();
    let now = Instant::now();
    center.push_at(Level::Success, "Commit".into(), "Done".into(), now);
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    let background = Style::new()
        .fg(idea::RED)
        .bg(idea::PANEL)
        .add_modifier(Modifier::UNDERLINED);
    let baseline = "X".repeat(80);
    let paint = |frame: &mut ratatui::Frame<'_>| {
        frame.render_widget(
            Paragraph::new(vec![baseline.as_str(); 24].join("\n")).style(background),
            frame.area(),
        );
    };
    terminal.draw(paint).unwrap();
    let before = terminal.backend().buffer().clone();
    terminal
        .draw(|frame| {
            paint(frame);
            center.draw_live(frame, frame.area(), IconMode::Ascii);
        })
        .unwrap();
    let hit = center.hits[0];
    let during = terminal.backend().buffer();
    for y in 0..24 {
        for x in 0..80 {
            if !crate::ui::inside(hit.area, x, y) {
                assert_eq!(during[(x, y)], before[(x, y)]);
            }
        }
    }
    assert!(!during[(hit.area.x + 1, hit.area.y + 1)]
        .modifier
        .contains(Modifier::UNDERLINED));
    assert!(center.expire(now + Duration::from_secs(3)));
    terminal
        .draw(|frame| {
            paint(frame);
            center.draw_live(frame, frame.area(), IconMode::Ascii);
        })
        .unwrap();
    assert_eq!(terminal.backend().buffer(), &before);
    assert!(center.hits.is_empty());
}

#[test]
fn close_uses_the_drawn_id_and_captures_the_entire_gesture() {
    let mut center = Center::default();
    let old = center.push(Level::Info, "Old", "Body");
    let target = center.push(Level::Warning, "Target", "Body");
    draw_live(&mut center, 80, 24, IconMode::Unicode);
    let hit = center.hits[0];
    let newest = center.push(Level::Error, "New", "Body");
    assert_eq!(
        center.live_event(
            &mouse(
                MouseEventKind::Down(MouseButton::Left),
                hit.close.x + 1,
                hit.close.y
            ),
            true
        ),
        LiveEvent::Handled,
    );
    assert_eq!(
        center.live().iter().map(|live| live.id).collect::<Vec<_>>(),
        [newest, old]
    );
    assert!(center.get(target).is_some());
    center.clear_hits();
    assert_eq!(
        center.live_event(&mouse(MouseEventKind::Drag(MouseButton::Left), 0, 23), true),
        LiveEvent::Handled,
    );
    assert_eq!(
        center.live_event(&mouse(MouseEventKind::Up(MouseButton::Left), 0, 23), true),
        LiveEvent::Handled,
    );
    assert_eq!(
        center.live_event(&mouse(MouseEventKind::Down(MouseButton::Left), 0, 23), true),
        LiveEvent::Pass,
    );
}

#[test]
fn every_mouse_gesture_over_a_card_is_consumed_and_popups_block_opening() {
    let mut center = Center::default();
    let selected = center.push(Level::Info, "Selected", "Body");
    draw_live(&mut center, 80, 24, IconMode::Ascii);
    let hit = center.hits[0];
    center.push(Level::Warning, "New arrival", "Body");
    for kind in [
        MouseEventKind::Down(MouseButton::Left),
        MouseEventKind::Down(MouseButton::Right),
        MouseEventKind::Down(MouseButton::Middle),
        MouseEventKind::Drag(MouseButton::Left),
        MouseEventKind::Up(MouseButton::Left),
        MouseEventKind::Up(MouseButton::Right),
        MouseEventKind::Up(MouseButton::Middle),
        MouseEventKind::Moved,
        MouseEventKind::ScrollDown,
        MouseEventKind::ScrollUp,
        MouseEventKind::ScrollLeft,
        MouseEventKind::ScrollRight,
    ] {
        assert_eq!(
            center.live_event(&mouse(kind, hit.area.x, hit.area.y), false),
            LiveEvent::Handled
        );
    }
    assert_eq!(
        center.live_event(
            &mouse(
                MouseEventKind::Down(MouseButton::Left),
                hit.area.x,
                hit.area.y
            ),
            true
        ),
        LiveEvent::Open(selected),
    );
    assert_eq!(
        center.live_event(&key(KeyCode::Enter), true),
        LiveEvent::Pass
    );
}

#[test]
fn hidden_tiny_and_history_views_invalidate_hits() {
    let mut center = Center::default();
    center.push(Level::Info, "Commit", "Done");
    draw_live(&mut center, 80, 24, IconMode::Ascii);
    let hit = center.hits[0];
    assert!(center.contains(hit.area.x, hit.area.y));
    center.clear_hits();
    assert!(!center.contains(hit.area.x, hit.area.y));
    draw_live(&mut center, 80, 24, IconMode::Ascii);
    draw_live(&mut center, 23, 24, IconMode::Ascii);
    assert!(center.hits.is_empty());
    draw_live(&mut center, 80, 24, IconMode::Ascii);
    center.live_event(&Event::Resize(80, 20), true);
    assert!(center.hits.is_empty());
    draw_live(&mut center, 80, 24, IconMode::Ascii);
    let mut history = center.open_history(None);
    draw_history(&mut center, &mut history, 80, 24);
    assert!(center.hits.is_empty());
}

#[test]
fn second_row_click_opens_detail_and_mouse_back_and_close_work() {
    let mut center = Center::default();
    let older = center.push(Level::Info, "Older", "Full older body");
    center.push(Level::Info, "Newer", "Body");
    let mut history = center.open_history(None);
    draw_history(&mut center, &mut history, 40, 24);
    let (row, _) = history.rows[1];
    let click = mouse(MouseEventKind::Down(MouseButton::Left), row.x, row.y);
    assert_eq!(
        center.history_event(&click, &mut history),
        HistoryEvent::Handled
    );
    assert_eq!(history.selected_id, Some(older));
    assert!(history.detail.is_none());
    assert_eq!(
        center.history_event(&click, &mut history),
        HistoryEvent::Handled
    );
    assert_eq!(history.detail.as_ref().unwrap().entry.id, older);
    draw_history(&mut center, &mut history, 40, 24);
    let close = history.close_area;
    let click_close = mouse(
        MouseEventKind::Down(MouseButton::Left),
        close.x + 1,
        close.y,
    );
    assert_eq!(
        center.history_event(&click_close, &mut history),
        HistoryEvent::Handled
    );
    assert!(history.detail.is_none());
    draw_history(&mut center, &mut history, 40, 24);
    assert_eq!(
        center.history_event(&click_close, &mut history),
        HistoryEvent::Close
    );
    assert_eq!(
        center.live_event(
            &mouse(MouseEventKind::Up(MouseButton::Left), close.x, close.y),
            true
        ),
        LiveEvent::Handled,
    );
}

#[test]
fn latest_body_is_empty_initially_and_follows_newest_history_after_dismissal() {
    let mut center = Center::default();
    assert_eq!(center.latest_body(), "");
    center.push(Level::Info, "Old", "First");
    center.push(Level::Success, "New", "Second");
    center.dismiss_all_live();
    assert_eq!(center.latest_body(), "Second");
}

#[test]
fn history_search_keeps_rows_highlights_matches_and_preserves_ids_on_arrival() {
    let mut center = Center::default();
    let oldest = center.push(Level::Error, "Push", "连接 refused");
    let middle = center.push(Level::Info, "Commit", "连接 retry");
    center.push(Level::Success, "Clipboard", "Copied");
    let mut history = center.open_history(None);
    center.history_event(&key(KeyCode::Char('/')), &mut history);
    center.history_event(&Event::Paste("连接".into()), &mut history);
    draw_history(&mut center, &mut history, 100, 26);
    assert_eq!(history.rows.len(), 3);
    center.history_event(&key(KeyCode::Enter), &mut history);
    assert_eq!(history.selected_id, Some(middle));
    assert!(history.detail.is_none());
    center.history_event(&key(KeyCode::Char('n')), &mut history);
    assert_eq!(history.selected_id, Some(oldest));
    center.history_event(&key(KeyCode::Char('n')), &mut history);
    assert_eq!(history.selected_id, Some(middle));
    center.history_event(&key(KeyCode::Char('N')), &mut history);
    assert_eq!(history.selected_id, Some(oldest));
    center.push(Level::Warning, "New arrival", "连接 new");
    let buffer = draw_history(&mut center, &mut history, 100, 26);
    assert_eq!(history.selected_id, Some(oldest));
    assert!(buffer
        .content
        .iter()
        .any(|cell| cell.bg == idea::GOLD && cell.symbol() == "连"));
    assert!(text(&buffer).contains("3 matches"));
    center.history_event(&key(KeyCode::Enter), &mut history);
    assert_eq!(history.detail.as_ref().unwrap().entry.id, oldest);
}

#[test]
fn search_ctrl_w_preserves_suffix_and_supports_undo_redo() {
    let mut center = Center::default();
    center.push(Level::Info, "Search", "alpha tail");
    let mut history = center.open_history(None);
    center.history_event(&key(KeyCode::Char('/')), &mut history);
    center.history_event(&Event::Paste("alpha 中文 👩‍💻 tail".into()), &mut history);
    for _ in 0..4 {
        center.history_event(&key(KeyCode::Left), &mut history);
    }
    let ctrl = |ch| Event::Key(KeyEvent::new(KeyCode::Char(ch), KeyModifiers::CONTROL));
    center.history_event(&ctrl('w'), &mut history);
    assert_eq!(history.query(), "alpha 中文 tail");
    assert_eq!(history.input.as_ref().unwrap().caret(), "alpha 中文 ".len());
    center.history_event(&ctrl('z'), &mut history);
    assert_eq!(history.query(), "alpha 中文 👩‍💻 tail");
    assert_eq!(
        history.input.as_ref().unwrap().caret(),
        "alpha 中文 👩‍💻 ".len()
    );
    center.history_event(&ctrl('y'), &mut history);
    assert_eq!(history.query(), "alpha 中文 tail");
    center.history_event(&ctrl('w'), &mut history);
    assert_eq!(history.query(), "alpha tail");
    center.history_event(&key(KeyCode::Enter), &mut history);
    assert_eq!(history.query(), "alpha tail");
}

#[test]
fn search_edits_whole_graphemes_and_supports_undo_redo_cancel_and_no_matches() {
    let mut center = Center::default();
    let id = center.push(Level::Info, "提交", "Body");
    let mut history = center.open_history(None);
    center.history_event(&key(KeyCode::Char('/')), &mut history);
    center.history_event(&Event::Paste("界e\u{301}👩‍💻".into()), &mut history);
    center.history_event(&key(KeyCode::Left), &mut history);
    center.history_event(&key(KeyCode::Backspace), &mut history);
    assert_eq!(history.query(), "界👩‍💻");
    center.history_event(
        &Event::Key(KeyEvent::new(KeyCode::Char('z'), KeyModifiers::CONTROL)),
        &mut history,
    );
    assert_eq!(history.query(), "界e\u{301}👩‍💻");
    center.history_event(
        &Event::Key(KeyEvent::new(
            KeyCode::Char('z'),
            KeyModifiers::CONTROL | KeyModifiers::SHIFT,
        )),
        &mut history,
    );
    assert_eq!(history.query(), "界👩‍💻");
    center.history_event(&key(KeyCode::Home), &mut history);
    center.history_event(&key(KeyCode::Delete), &mut history);
    assert_eq!(history.query(), "👩‍💻");
    center.history_event(&key(KeyCode::Enter), &mut history);
    assert_eq!(history.selected_id, Some(id));
    let buffer = draw_history(&mut center, &mut history, 80, 24);
    assert!(text(&buffer).contains("0 matches"));
    center.history_event(&key(KeyCode::Char('/')), &mut history);
    center.history_event(
        &Event::Key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL)),
        &mut history,
    );
    center.history_event(&Event::Paste("different".into()), &mut history);
    center.history_event(&key(KeyCode::Esc), &mut history);
    assert_eq!(history.query(), "👩‍💻");
    assert_eq!(
        center.history_event(&key(KeyCode::Esc), &mut history),
        HistoryEvent::Close
    );
}

#[test]
fn search_matches_source_level_and_utc_time_case_insensitively() {
    let mut center = Center {
        utc_offset: None,
        ..Center::default()
    };
    center.push(Level::Warning, "Commit", "Body");
    let entry = center.history().next().unwrap();
    for query in ["COMMIT", "WARNING", "UTC", "bOdY"] {
        assert!(entry.matches(query));
    }
}

#[test]
fn modal_confirmation_consumes_mouse_paste_and_unrelated_keys() {
    let mut center = Center::default();
    center.push(Level::Info, "Commit", "first");
    center.push(Level::Info, "Push", "second");
    let mut history = center.open_history(None);
    draw_history(&mut center, &mut history, 80, 24);
    let selected = history.selected_id;
    center.history_event(&key(KeyCode::Char('c')), &mut history);
    for event in [
        key(KeyCode::Enter),
        key(KeyCode::Char('j')),
        key(KeyCode::Char('q')),
        Event::Paste("y".into()),
        mouse(MouseEventKind::ScrollDown, 0, 0),
        mouse(MouseEventKind::Down(MouseButton::Left), 0, 0),
        Event::Key(KeyEvent::new_with_kind(
            KeyCode::Char('y'),
            KeyModifiers::NONE,
            KeyEventKind::Release,
        )),
    ] {
        assert_eq!(
            center.history_event(&event, &mut history),
            HistoryEvent::Handled
        );
        assert_eq!(history.selected_id, selected);
        assert_eq!(center.history().count(), 2);
    }
    center.history_event(&key(KeyCode::Esc), &mut history);
    assert!(!history.clear_confirm);
    center.history_event(&key(KeyCode::Char('c')), &mut history);
    center.history_event(&key(KeyCode::Char('n')), &mut history);
    assert_eq!(center.history().count(), 2);
}

#[test]
fn detail_scrolls_wrapped_rows_and_copies_the_complete_sanitized_body() {
    let mut center = Center::default();
    let raw = format!("{}\nlast line\x1b", "界 👩‍💻 e\u{301} ".repeat(600));
    let id = center.push(Level::Error, "Push", &raw);
    let body = center.get(id).unwrap().body.clone();
    let mut history = center.open_history(Some(id));
    center.history_event(&key(KeyCode::Enter), &mut history);
    let initial = draw_history(&mut center, &mut history, 40, 16);
    assert!(!text(&initial).contains("last line"));
    center.history_event(&key(KeyCode::End), &mut history);
    let bottom = draw_history(&mut center, &mut history, 40, 16);
    assert!(text(&bottom).contains("last line<ESC>"));
    assert_eq!(
        center.history_event(&key(KeyCode::Char('y')), &mut history),
        HistoryEvent::Copy(body.clone())
    );
    center.history_event(&key(KeyCode::Home), &mut history);
    assert_eq!(history.detail.as_ref().unwrap().scroll, 0);
    center.history_event(&key(KeyCode::PageDown), &mut history);
    assert!(history.detail.as_ref().unwrap().scroll > 0);
    center.history_event(&key(KeyCode::Esc), &mut history);
    assert!(history.detail.is_none());
    assert_eq!(history.selected_id, Some(id));
    assert_eq!(
        center.history_event(&key(KeyCode::Char('y')), &mut history),
        HistoryEvent::Copy(body)
    );
    assert_eq!(
        center.history_event(&key(KeyCode::Char('q')), &mut history),
        HistoryEvent::Close
    );
}

#[test]
fn history_mouse_rows_keep_drawn_ids_and_navigation_stays_visible() {
    let mut center = Center::default();
    for index in 0..40 {
        center.push(Level::Info, format!("Commit {index}"), "Body");
    }
    let mut history = center.open_history(None);
    draw_history(&mut center, &mut history, 100, 26);
    let (row, id) = history.rows[3];
    center.push(Level::Info, "New arrival", "Body");
    center.history_event(
        &mouse(MouseEventKind::Down(MouseButton::Left), row.x, row.y),
        &mut history,
    );
    assert_eq!(history.selected_id, Some(id));
    center.history_event(&key(KeyCode::End), &mut history);
    draw_history(&mut center, &mut history, 100, 26);
    assert_eq!(history.selected_id, Some(0));
    assert_eq!(history.rows.last().unwrap().1, 0);
    center.history_event(&key(KeyCode::PageUp), &mut history);
    assert!(history.selected < 40);
    center.history_event(&mouse(MouseEventKind::ScrollUp, row.x, row.y), &mut history);
    let selected = history.selected;
    center.history_event(
        &mouse(MouseEventKind::ScrollDown, row.x, row.y),
        &mut history,
    );
    assert_eq!(history.selected, (selected + 3).min(40));
    center.history_event(&key(KeyCode::Home), &mut history);
    assert_eq!(history.selected, 0);
    assert_eq!(history.offset, 0);
}

#[test]
fn selected_history_and_detail_reconcile_capacity_eviction() {
    let mut center = Center::default();
    let oldest = center.push(Level::Info, "Old", "Retained detail");
    let mut history = center.open_history(Some(oldest));
    center.history_event(&key(KeyCode::Enter), &mut history);
    for _ in 0..501 {
        center.push(Level::Info, "New", "Body");
    }
    draw_history(&mut center, &mut history, 80, 24);
    assert_eq!(
        center.history_event(&key(KeyCode::Char('y')), &mut history),
        HistoryEvent::Copy("Retained detail".into())
    );
    center.history_event(&key(KeyCode::Esc), &mut history);
    assert!(center.get(history.selected_id.unwrap()).is_some());
}

#[test]
fn history_layout_is_split_only_when_wide_and_fits_small_offset_rectangles() {
    let mut center = Center::default();
    center.push(Level::Info, "Commit", "preview-only-marker");
    for width in [24, 71, 72, 100, 160] {
        let mut history = center.open_history(None);
        let buffer = draw_history(&mut center, &mut history, width, 30);
        assert_eq!(text(&buffer).contains("preview-only-marker"), width >= 72);
        assert!(history.list_area.width <= 98);
        center.history_event(&key(KeyCode::Enter), &mut history);
        let detail = draw_history(&mut center, &mut history, width, 30);
        assert!(text(&detail).contains("preview-only-marker"));
    }
    for width in 0..25 {
        for height in 0..10 {
            let mut terminal = Terminal::new(TestBackend::new(40, 20)).unwrap();
            let mut history = center.open_history(None);
            terminal
                .draw(|frame| {
                    let area = Rect::new(3, 4, width, height);
                    center.draw_live(frame, area, IconMode::Ascii);
                    center.draw_history(frame, area, IconMode::Ascii, &mut history);
                })
                .unwrap();
            assert!(center.hits.is_empty());
        }
    }
}

#[test]
fn matching_and_clipping_keep_whole_unicode_graphemes() {
    let title = "界 e\u{301} 👩‍💻 very long title";
    for width in 0..20 {
        let clipped = idea::clipped(title, width);
        assert!(clipped.width() <= width);
        assert!(!clipped.ends_with('\u{200d}'));
    }
    let mut center = Center::default();
    center.push(Level::Info, "İstanbul", "Unicode folding");
    let mut history = center.open_history(None);
    center.history_event(&key(KeyCode::Char('/')), &mut history);
    center.history_event(&Event::Paste("i".into()), &mut history);
    let buffer = draw_history(&mut center, &mut history, 100, 24);
    assert!(buffer
        .content
        .iter()
        .any(|cell| cell.symbol() == "İ" && cell.bg == idea::GOLD));
}

fn help_events(history: &History, shortcut: &str) -> Vec<Event> {
    let content = history.keymap();
    let &(keys, _) = content
        .sections
        .iter()
        .flat_map(|section| section.bindings)
        .find(|(keys, _)| *keys == shortcut)
        .expect("shortcut belongs to this notification mode");
    crate::ui::help::HelpCommand {
        action: crate::config::keys::CommandAction::Keys(keys),
    }
    .events()
    .expect("notification help shortcut is executable")
}

fn run_help(center: &mut Center, history: &mut History, shortcut: &str) -> HistoryEvent {
    let mut result = HistoryEvent::Handled;
    for event in help_events(history, shortcut) {
        result = center.history_event(&event, history);
    }
    result
}

#[test]
fn history_keymaps_have_distinct_contexts_and_only_mode_specific_actions() {
    let mut center = Center::default();
    center.push(Level::Info, "Commit", "Body");
    let mut normal = center.open_history(None);
    let mut search = normal.clone();
    center.history_event(&key(KeyCode::Char('/')), &mut search);
    let mut detail = normal.clone();
    center.history_event(&key(KeyCode::Enter), &mut detail);
    let mut confirm = normal.clone();
    center.history_event(&key(KeyCode::Char('c')), &mut confirm);
    let contexts: std::collections::BTreeSet<_> = [&normal, &search, &detail, &confirm]
        .into_iter()
        .map(|history| history.keymap().context())
        .collect();
    assert_eq!(contexts.len(), 4);
    for history in [&normal, &search, &detail, &confirm] {
        let content = history.keymap();
        assert_eq!(content.sections.len(), 1);
        for (shortcut, _) in content.sections[0].bindings {
            assert_eq!(help_events(history, shortcut).len(), 1);
        }
    }
    let keys = |history: &History| {
        history.keymap().sections[0]
            .bindings
            .iter()
            .map(|(shortcut, _)| *shortcut)
            .collect::<Vec<_>>()
    };
    assert_eq!(keys(&confirm), ["y", "n", "Esc"]);
    for shortcut in ["j", "k", "y", "c", "/", "n", "N", "q"] {
        assert!(!keys(&search).contains(&shortcut));
    }
    for shortcut in ["c", "/", "n", "N", "Enter"] {
        assert!(!keys(&detail).contains(&shortcut));
    }
    assert_eq!(
        run_help(&mut center, &mut normal, "y"),
        HistoryEvent::Copy("Body".into())
    );
    assert_eq!(
        run_help(&mut center, &mut detail, "y"),
        HistoryEvent::Copy("Body".into())
    );
    assert_eq!(
        run_help(&mut center, &mut detail, "Esc"),
        HistoryEvent::Handled
    );
    assert!(detail.detail.is_none());
}

#[test]
fn clear_confirmation_help_cannot_offer_copy_and_replays_confirm_or_cancel() {
    use crate::ui::help::{HelpEvent, HelpState};

    let mut center = Center::default();
    center.push(Level::Info, "Commit", "Keep until confirmed");
    let mut history = center.open_history(None);
    center.history_event(&key(KeyCode::Char('c')), &mut history);
    let mut help = HelpState::new(history.keymap());
    help.event(&Event::Paste("copy".into()));
    assert_eq!(help.event(&key(KeyCode::Enter)), HelpEvent::Keep);
    assert_eq!(center.history().count(), 1);
    for shortcut in ["n", "Esc"] {
        assert_eq!(
            run_help(&mut center, &mut history, shortcut),
            HistoryEvent::Handled
        );
        assert!(!history.clear_confirm);
        assert_eq!(center.history().count(), 1);
        center.history_event(&key(KeyCode::Char('c')), &mut history);
    }
    assert_eq!(
        run_help(&mut center, &mut history, "y"),
        HistoryEvent::Handled
    );
    assert_eq!(center.history().count(), 0);
    assert!(center.live().is_empty());
    assert_eq!(history.keymap().context(), "Notifications / History");
}

#[test]
fn search_help_actions_edit_confirm_and_cancel_without_history_commands() {
    let mut center = Center::default();
    let target = center.push(Level::Info, "Commit", "界 target");
    center.push(Level::Info, "Push", "Other");
    let mut history = center.open_history(None);
    run_help(&mut center, &mut history, "/");
    center.history_event(&Event::Paste("界".into()), &mut history);
    run_help(&mut center, &mut history, "Ctrl-u");
    assert_eq!(history.query(), "");
    run_help(&mut center, &mut history, "Ctrl-z");
    assert_eq!(history.query(), "界");
    run_help(&mut center, &mut history, "Ctrl-y");
    assert_eq!(history.query(), "");
    run_help(&mut center, &mut history, "Ctrl-z");
    assert_eq!(
        run_help(&mut center, &mut history, "Enter"),
        HistoryEvent::Handled
    );
    assert_eq!(history.selected_id, Some(target));
    assert!(history.input.is_none() && history.detail.is_none());
    run_help(&mut center, &mut history, "/");
    run_help(&mut center, &mut history, "Ctrl-u");
    assert_eq!(
        run_help(&mut center, &mut history, "Esc"),
        HistoryEvent::Handled
    );
    assert_eq!(history.query(), "界");
    assert_eq!(center.history().count(), 2);
}

#[test]
fn history_clear_hits_disables_hidden_targets_without_resetting_navigation_or_detail() {
    let mut center = Center::default();
    for index in 0..30 {
        center.push(Level::Info, format!("Commit {index}"), "Body\n".repeat(80));
    }
    let mut history = center.open_history(None);
    draw_history(&mut center, &mut history, 80, 24);
    center.history_event(&key(KeyCode::End), &mut history);
    draw_history(&mut center, &mut history, 80, 24);
    let selected = history.selected_id;
    let offset = history.offset;
    let row = history.rows[0].0;
    let close = history.close_area;
    history.clear_hits();
    assert!(history.rows.is_empty());
    assert!(history.list_area.is_empty() && history.close_area.is_empty());
    assert_eq!(history.page_size, 0);
    for area in [row, close] {
        assert_eq!(
            center.history_event(
                &mouse(MouseEventKind::Down(MouseButton::Left), area.x, area.y),
                &mut history,
            ),
            HistoryEvent::Handled
        );
    }
    assert_eq!(history.selected_id, selected);
    assert_eq!(history.offset, offset);
    assert!(history.detail.is_none());
    draw_history(&mut center, &mut history, 80, 24);
    assert_eq!(history.offset, offset);
    assert!(!history.rows.is_empty());
    center.history_event(&key(KeyCode::Enter), &mut history);
    draw_history(&mut center, &mut history, 80, 24);
    center.history_event(&key(KeyCode::End), &mut history);
    let scroll = history.detail.as_ref().unwrap().scroll;
    assert!(scroll > 0);
    let close = history.close_area;
    history.clear_hits();
    assert_eq!(history.detail.as_ref().unwrap().scroll, scroll);
    assert_eq!(history.detail.as_ref().unwrap().page_size, 0);
    center.history_event(
        &mouse(MouseEventKind::Down(MouseButton::Left), close.x, close.y),
        &mut history,
    );
    assert!(history.detail.is_some());
    draw_history(&mut center, &mut history, 80, 24);
    assert_eq!(history.detail.as_ref().unwrap().scroll, scroll);
    center.history_event(&key(KeyCode::Esc), &mut history);
    center.history_event(&key(KeyCode::Char('/')), &mut history);
    center.history_event(&Event::Paste("界".into()), &mut history);
    history.clear_hits();
    assert_eq!(history.query(), "界");
    run_help(&mut center, &mut history, "Ctrl-z");
    assert_eq!(history.query(), "");
    draw_history(&mut center, &mut history, 80, 24);
    center.history_event(&Event::Resize(43, 11), &mut history);
    assert!(history.rows.is_empty() && history.close_area.is_empty());
}
