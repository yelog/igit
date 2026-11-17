use super::*;
use crate::{
    shell::{test_support, Popup, Shell},
    ui::MUTED,
};
use std::time::{Duration, Instant};

fn checking(kind: Kind) -> Dialog {
    let mut dialog = fixture(kind, 2);
    dialog.stage = Stage::Loading;
    dialog.ready = 0;
    dialog.confirm_focused = false;
    for row in &mut dialog.rows {
        row.outcome = Outcome::Checking;
        row.comparison = Comparison::checking();
        row.destination.clear();
    }
    dialog
}

fn no_process_labels(text: &str) {
    let lower = text.to_lowercase();
    for label in ["checking", "pending", "running", "updating"] {
        assert!(!lower.contains(label), "{label} remains in:\n{text}");
    }
}

fn status_position(
    buffer: &Buffer,
    hits: &[(Rect, usize)],
    index: usize,
    symbol: char,
) -> (u16, u16) {
    let row = hits.iter().find(|(_, id)| *id == ROW + index).unwrap().0;
    let column = (row.x..row.right())
        .find(|&column| buffer[(column, row.y)].symbol() == symbol.to_string())
        .unwrap_or_else(|| panic!("Missing status {symbol} for row {index}"));
    (column, row.y)
}

#[test]
fn checking_spinners_advance_without_moving_rows_or_exposing_confirmation() {
    for kind in [Kind::Pull, Kind::Fetch] {
        for icons in [IconMode::Ascii, IconMode::Unicode, IconMode::NerdFont] {
            for (width, height) in [(44, 12), (80, 24), (120, 36), (180, 36)] {
                let mut dialog = checking(kind);
                let start = Instant::now();
                assert!(!dialog.tick_animation(start));
                let (first, hits, text) = draw(&mut dialog, width, height, icons);
                no_process_labels(&text);
                assert!(text.contains(&format!("{} ", icons.spinner(0))));
                assert!(!hits.iter().any(|(_, id)| *id == CONFIRM));
                let positions = hits
                    .iter()
                    .filter(|(_, id)| *id >= ROW)
                    .map(|(_, id)| status_position(&first, &hits, id - ROW, icons.spinner(0)))
                    .collect::<Vec<_>>();
                assert!(!positions.is_empty());
                let list = dialog.list_area;
                assert!(!dialog.tick_animation(start + Duration::from_millis(99)));
                assert!(dialog.tick_animation(start + Duration::from_millis(100)));
                let (next, next_hits, text) = draw(&mut dialog, width, height, icons);
                no_process_labels(&text);
                assert_eq!(dialog.list_area, list);
                assert_eq!(hits, next_hits);
                for position in positions {
                    assert_eq!(next[position].symbol(), icons.spinner(1).to_string());
                    assert_eq!(next[position].fg, ACCENT);
                }
                assert!(text.contains(&format!("workspace: {}", icons.spinner(1))));
                assert!(dialog.event(&key(K::Esc), None));
            }
        }
    }
}

#[test]
fn queued_and_active_spinners_yield_to_individual_results_in_every_layout() {
    for kind in [Kind::Pull, Kind::Fetch] {
        for icons in [IconMode::Ascii, IconMode::Unicode, IconMode::NerdFont] {
            for (width, height) in [(44, 12), (80, 24), (120, 36), (180, 36)] {
                let mut dialog = fixture(kind, 2);
                let (sender, receiver) = mpsc::channel();
                dialog.stage = Stage::Running(receiver);
                dialog.rows[1].outcome = Outcome::Running;
                let start = Instant::now();
                dialog.tick_animation(start);
                let (first, hits, text) = draw(&mut dialog, width, height, icons);
                no_process_labels(&text);
                assert!(!text.contains("Ready"));
                assert!(text.contains("0/2"));
                assert!(!hits.iter().any(|(_, id)| *id < ROW));
                let queued = status_position(&first, &hits, 0, icons.spinner(0));
                let active = status_position(&first, &hits, 1, icons.spinner(0));
                assert_eq!(first[queued].fg, MUTED);
                assert_eq!(first[active].fg, ACCENT);

                sender.send(Report::Started(0)).unwrap();
                assert!(dialog.poll().is_none());
                assert!(dialog.tick_animation(start + Duration::from_millis(100)));
                let (next, next_hits, text) = draw(&mut dialog, width, height, icons);
                no_process_labels(&text);
                assert_eq!(hits, next_hits);
                for position in [queued, active] {
                    assert_eq!(next[position].symbol(), icons.spinner(1).to_string());
                    assert_eq!(next[position].fg, ACCENT);
                }

                sender
                    .send(Report::Row(
                        0,
                        Outcome::Succeeded,
                        dialog.rows[0].comparison.clone(),
                    ))
                    .unwrap();
                assert!(dialog.poll().is_none());
                let (partial, _, text) = draw(&mut dialog, width, height, icons);
                no_process_labels(&text);
                assert!(text.contains("1/2"));
                assert_eq!(partial[queued].symbol(), "O");
                assert_eq!(partial[queued].fg, GREEN);
                assert_eq!(partial[active].symbol(), icons.spinner(1).to_string());

                sender
                    .send(Report::Row(
                        1,
                        Outcome::Failed("Remote unavailable".into()),
                        dialog.rows[1].comparison.clone(),
                    ))
                    .unwrap();
                sender.send(Report::Done).unwrap();
                assert!(dialog.poll().is_some());
                assert!(dialog.tick_animation(start + Duration::from_millis(101)));
                assert!(!dialog.tick_animation(start + Duration::from_secs(1)));
                let (_, hits, text) = draw(&mut dialog, width, height, icons);
                no_process_labels(&text);
                assert!(text.contains("Failed"));
                assert!(hits.iter().any(|(_, id)| *id == CANCEL));
                assert!(!dialog.animation_running());
                for _ in 0..dialog.detail_max_scroll {
                    dialog.event(&key(K::Char('J')), None);
                }
                let (_, _, details) = draw(&mut dialog, width, height, icons);
                assert!(details.contains("Remote unavailable"), "{details}");
            }
        }
    }
}

#[test]
fn completed_checks_and_failed_readers_stop_the_animation() {
    for kind in [Kind::Pull, Kind::Fetch] {
        let mut dialog = checking(kind);
        let start = Instant::now();
        dialog.tick_animation(start);
        dialog.load.request();
        let sender = dialog.load.hold();
        sender
            .send(Prepared {
                targets: vec![],
                rows: fixture(kind, 2).rows,
                ready: 2,
            })
            .unwrap();
        assert!(dialog.poll_loading().unwrap());
        assert!(dialog.tick_animation(start + Duration::from_millis(100)));
        assert!(!dialog.tick_animation(start + Duration::from_millis(200)));
        let (_, hits, text) = draw(&mut dialog, 80, 24, IconMode::Unicode);
        assert!(text.contains("Ready"));
        assert!(hits.iter().any(|(_, id)| *id == CONFIRM));
        assert!(!text.contains(IconMode::Unicode.spinner(0)));

        let mut dialog = checking(kind);
        dialog.tick_animation(start);
        assert!(dialog.fail_loading(anyhow::anyhow!("Read failed")).is_err());
        assert!(dialog.tick_animation(start + Duration::from_millis(100)));
        assert!(!dialog.tick_animation(start + Duration::from_millis(200)));
        let (_, hits, text) = draw(&mut dialog, 80, 24, IconMode::Unicode);
        assert!(text.contains("Failed") && text.contains("Read failed"));
        assert!(!hits.iter().any(|(_, id)| *id == CONFIRM));
        assert!(!text.contains(IconMode::Unicode.spinner(0)));
    }
}

fn wait_for_frame(app: &mut Shell, frame: usize) {
    let marker = format!("{} ", app.icons.spinner(frame));
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        if app.poll() && test_support::screen(app, 120, 36).contains(&marker) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "Update animation did not advance"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn shell_animates_checks_and_execution_without_worker_messages_or_keypresses() {
    for kind in [Kind::Pull, Kind::Fetch] {
        let (_directory, mut app) = test_support::fixture();
        let mut dialog = checking(kind);
        dialog.load.request();
        let _check_sender = dialog.load.hold();
        app.pop = Some(Popup::Update(dialog));
        app.poll();
        assert!(app.poll_timeout() <= Duration::from_millis(100));
        wait_for_frame(&mut app, 1);

        let mut dialog = fixture(kind, 2);
        let (_sender, receiver) = mpsc::channel();
        dialog.stage = Stage::Running(receiver);
        app.pop = Some(Popup::Update(dialog));
        app.poll();
        assert!(app.poll_timeout() <= Duration::from_millis(100));
        wait_for_frame(&mut app, 1);
    }
}
