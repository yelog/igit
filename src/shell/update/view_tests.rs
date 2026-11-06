use super::*;
use crate::ui::{
    icons::IconMode,
    idea::{repo_tint, GREEN, RED, TAG},
    ACCENT, SEL,
};
use crossterm::event::{KeyEvent, MouseButton, MouseEvent};
use ratatui::{backend::TestBackend, buffer::Buffer, style::Modifier, Terminal};
use unicode_width::UnicodeWidthStr;

fn fixture(kind: Kind, count: usize) -> Dialog {
    Dialog {
        kind,
        strategy: Strategy::Merge,
        stage: Stage::Review(vec![]),
        ready: count,
        rows: (0..count)
            .map(|i| RepositoryRow {
                root: i,
                name: if i == 0 {
                    "workspace".into()
                } else {
                    format!("service-{i:02}")
                },
                submodule: i > 0,
                branch: "main".into(),
                destination: if kind == Kind::Pull {
                    "origin:main".into()
                } else {
                    "origin, upstream".into()
                },
                outcome: Outcome::Ready,
            })
            .collect(),
        pos: 0,
        offset: 0,
        list_area: Rect::default(),
        detail_area: Rect::default(),
        detail_scroll: 0,
        detail_max_scroll: 0,
        confirm_focused: false,
    }
}

fn draw(
    dialog: &mut Dialog,
    width: u16,
    height: u16,
    icons: IconMode,
) -> (Buffer, Vec<(Rect, usize)>, String) {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    let mut hits = vec![];
    terminal
        .draw(|frame| dialog.draw(frame, frame.area(), icons, &mut hits))
        .unwrap();
    let buffer = terminal.backend().buffer().clone();
    let text = (0..height)
        .map(|y| {
            let mut line = String::new();
            let mut x = 0;
            while x < width {
                let symbol = buffer[(x, y)].symbol();
                line.push_str(symbol);
                x += symbol.width().max(1) as u16;
            }
            line
        })
        .collect::<Vec<_>>()
        .join("\n");
    (buffer, hits, text)
}

fn key(code: K) -> Event {
    Event::Key(KeyEvent::new(code, M::NONE))
}

fn click(dialog: &mut Dialog, hits: &[(Rect, usize)], id: usize) -> bool {
    let rect = hits.iter().find(|(_, hit)| *hit == id).unwrap().0;
    dialog.event(
        &Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: rect.x,
            row: rect.y,
            modifiers: M::NONE,
        }),
        Some(id),
    )
}

#[test]
fn update_panels_fit_controls_columns_and_repository_icons_in_all_modes() {
    for kind in [Kind::Fetch, Kind::Pull] {
        for icons in [IconMode::NerdFont, IconMode::Unicode, IconMode::Ascii] {
            for (width, height) in [(44, 12), (80, 24), (120, 36), (180, 36)] {
                let mut dialog = fixture(kind, 11);
                let (buffer, hits, text) = draw(&mut dialog, width, height, icons);
                assert!(
                    text.contains("11 ready") && text.contains("11 in Roots"),
                    "{text}"
                );
                assert!(text.contains(&format!("y {} 11", kind.label())), "{text}");
                assert!(
                    text.contains("n Cancel") && text.contains("Details"),
                    "{text}"
                );
                assert!(
                    text.contains("workspace") && text.contains("Ready"),
                    "{text}"
                );
                let row = hits.iter().find(|(_, id)| *id == ROW).unwrap().0;
                assert_eq!(buffer[(row.x + 1, row.y)].fg, repo_tint(0));
                assert_eq!(buffer[(row.x, row.y)].bg, SEL);
                let action = hits.iter().find(|(_, id)| *id == CONFIRM).unwrap().0;
                let cancel = hits.iter().find(|(_, id)| *id == CANCEL).unwrap().0;
                assert_eq!(buffer[(action.x, action.y)].bg, SEL);
                assert_eq!(buffer[(cancel.x, cancel.y)].bg, TAG);
                assert!(buffer[(cancel.x, cancel.y)]
                    .modifier
                    .contains(Modifier::UNDERLINED));
                for (i, (rect, _)) in hits.iter().enumerate() {
                    assert!(buffer.area.contains((rect.x, rect.y).into()));
                    assert!(rect.right() <= width && rect.bottom() <= height && !rect.is_empty());
                    for (other, _) in hits.iter().skip(i + 1) {
                        assert!(
                            rect.intersection(*other).is_empty(),
                            "overlapping controls: {rect:?}, {other:?}"
                        );
                    }
                }
                if kind == Kind::Pull {
                    assert!(
                        text.contains("m Merge") && text.contains("r Rebase"),
                        "{text}"
                    );
                    assert!(
                        text.contains(&format!(
                            "main {} origin:main",
                            icons.glyph(crate::ui::icons::Icon::Left)
                        )),
                        "{text}"
                    );
                    assert!(!click(&mut dialog, &hits, REBASE));
                    assert!(dialog.strategy == Strategy::Rebase);
                } else {
                    assert!(text.contains("remote refs only"), "{text}");
                    assert!(!hits.iter().any(|(_, id)| *id == MERGE || *id == REBASE));
                }
                assert!(matches!(dialog.stage, Stage::Review(_)));
                assert!(dialog.event(&key(K::Enter), None)); // The primary color never changes initial Cancel focus.
            }
        }
    }
}

#[test]
fn update_browsing_and_resizing_keeps_the_current_repository_visible_without_changing_scope() {
    let mut dialog = fixture(Kind::Pull, 30);
    draw(&mut dialog, 44, 12, IconMode::Ascii);
    dialog.event(&key(K::End), None);
    let (_, hits, text) = draw(&mut dialog, 44, 12, IconMode::Ascii);
    assert!(text.contains("service-29") && text.contains("30/30"));
    assert!(hits.iter().any(|(_, id)| *id == ROW + 29));
    dialog.event(&key(K::PageUp), None);
    assert!(dialog.pos < 29);
    let (_, hits, _) = draw(&mut dialog, 120, 36, IconMode::Unicode);
    let target = hits.iter().find(|(_, id)| *id >= ROW).unwrap().1;
    assert!(!click(&mut dialog, &hits, target));
    assert_eq!(dialog.pos, target - ROW);
    dialog.event(&key(K::Home), None);
    dialog.event(&key(K::Char('j')), None);
    assert_eq!(dialog.pos, 1);
    assert_eq!(dialog.ready, 30);
    assert_eq!(dialog.rows.len(), 30);
    assert!(!dialog.confirm_focused);
    assert!(dialog.strategy == Strategy::Merge);
    assert!(matches!(dialog.stage, Stage::Review(_)));
}

#[test]
fn update_long_unicode_targets_and_errors_are_available_in_scrollable_details() {
    let mut dialog = fixture(Kind::Pull, 3);
    dialog.rows[0].name = "共享模块-界面-e\u{301}-🚀-very-long-repository-name".into();
    dialog.rows[0].branch = "feature/long-unicode-修复".into();
    dialog.rows[0].destination = "upstream:feature/remote-修复-long-branch".into();
    dialog.rows[0].outcome = Outcome::Failed(format!(
        "{}\nLAST ERROR LINE",
        "A detailed Git failure. ".repeat(30)
    ));
    dialog.stage = Stage::Results;
    let (_, _, text) = draw(&mut dialog, 44, 12, IconMode::Unicode);
    assert!(
        text.contains("共享模块") && text.contains("J/K scroll"),
        "{text}"
    );
    assert!(dialog.detail_max_scroll > 10);
    let mut all_details = String::new();
    for _ in 0..=dialog.detail_max_scroll {
        let (_, _, text) = draw(&mut dialog, 44, 12, IconMode::Unicode);
        all_details.push_str(&text);
        dialog.event(&key(K::Char('J')), None);
    }
    assert!(all_details.contains("LAST ERROR LINE"));
    assert!(all_details.contains("upstream:feature/remote-修复"));
    assert_eq!(dialog.pos, 0);
    dialog.event(&key(K::Char('j')), None);
    assert_eq!(dialog.detail_scroll, 0);
    assert_eq!(dialog.pos, 1);
}

#[test]
fn update_empty_and_unavailable_repositories_disable_confirmation() {
    for count in [0, 2] {
        let mut dialog = fixture(Kind::Fetch, count);
        dialog.ready = 0;
        for row in &mut dialog.rows {
            row.outcome = Outcome::Skipped("No remotes configured".into());
            row.destination.clear();
        }
        let (_, hits, text) = draw(&mut dialog, 44, 12, IconMode::Ascii);
        assert!(text.contains("0 ready"));
        assert!(!hits.iter().any(|(_, id)| *id == CONFIRM));
        assert!(!dialog.event(&key(K::Char('y')), None));
        assert!(matches!(dialog.stage, Stage::Review(_)));
        if count == 0 {
            assert!(text.contains("No repositories in Roots"));
        } else {
            assert!(text.contains("2 skipped") && text.contains("No remotes"));
        }
        assert!(click(&mut dialog, &hits, CANCEL));
    }
}

#[test]
fn update_running_and_results_keep_repository_rows_and_color_outcomes() {
    let mut dialog = fixture(Kind::Pull, 3);
    let (tx, rx) = mpsc::channel();
    dialog.stage = Stage::Running(rx);
    let (_, hits, text) = draw(&mut dialog, 120, 36, IconMode::Ascii);
    assert!(text.contains("Running") && text.contains("Pending"));
    assert!(!hits.iter().any(|(_, id)| *id < ROW));
    assert!(!dialog.event(&key(K::Esc), None));
    tx.send(Report {
        summary: "Pull: 1 succeeded · 1 failed · 1 skipped".into(),
        outcomes: vec![
            Outcome::Succeeded,
            Outcome::Failed("Remote changed; review again".into()),
            Outcome::Skipped("Detached HEAD".into()),
        ],
    })
    .unwrap();
    assert_eq!(
        dialog.poll().unwrap().1,
        "Pull: 1 succeeded · 1 failed · 1 skipped"
    );
    assert_eq!(dialog.pos, 1);
    let (buffer, hits, text) = draw(&mut dialog, 120, 36, IconMode::Unicode);
    assert!(text.contains("1 succeeded · 1 failed · 1 skipped"));
    assert!(text.contains("review again") && text.contains("Close / Esc"));
    for (id, label, color) in [(ROW, "OK", GREEN), (ROW + 1, "Failed", RED)] {
        let rect = hits.iter().find(|(_, hit)| *hit == id).unwrap().0;
        let x = (rect.x..rect.right())
            .find(|&x| buffer[(x, rect.y)].symbol() == &label[..1])
            .unwrap();
        assert_eq!(buffer[(x, rect.y)].fg, color);
    }
    assert!(buffer.content.iter().any(|cell| cell.fg == ACCENT));
    assert!(dialog.event(&key(K::Enter), None));
}
