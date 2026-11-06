use super::*;
use crate::{
    core::Context,
    merge::{Hit, MergePanel},
    ui::{icons::IconMode, Action, Panel},
};
use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::{backend::TestBackend, buffer::Buffer, layout::Rect, Terminal};

fn versions() -> [String; 3] {
    let mut base = (1..=40)
        .map(|n| format!("// context {n}\n"))
        .collect::<Vec<_>>();
    base[1] = " * Git 多客户端同步示例：销售订单统计。\n".into();
    base[7] = "  customer: \"Alice\",\n".into();
    base[15] = "  throw new TypeError(\"订单必须包含非空的 items 数组。\");\n".into();
    base[23] = "  throw new TypeError(\"orders 必须为数组。\");\n".into();
    base[31] = "  \"销售订单统计报告\",\n".into();
    let mut left = base.clone();
    let mut right = base.clone();
    left[1] = " * Git 多客户端冲突演练：销售订单统计。\n".into();
    right[1] = left[1].clone();
    left[7] = "  customer: \"Alice (client1 本地修改)\",\n".into();
    right[7] = "  customer: \"Alice (client2 远端修改)\",\n".into();
    left[15] = "  throw new TypeError(\"client1: 订单必须包含至少一个商品。\");\n".into();
    right[23] = "  throw new TypeError(\"client2: orders 必须为订单数组。\");\n".into();
    left[31] = "  \"销售订单统计报告 (client1 本地版)\",\n".into();
    right[31] = "  \"销售订单统计报告 (client2 远端版)\",\n".into();
    [base.concat(), left.concat(), right.concat()]
}

fn press(panel: &mut MergePanel, ctx: &Context, code: KeyCode) -> Action {
    panel.event(&Event::Key(KeyEvent::new(code, KeyModifiers::NONE)), ctx)
}

fn draw(panel: &mut MergePanel, ctx: &Context, width: u16) -> Buffer {
    let mut terminal = Terminal::new(TestBackend::new(width, 30)).unwrap();
    terminal.draw(|f| panel.draw(f, f.area(), ctx)).unwrap();
    terminal.backend().buffer().clone()
}

fn text(buffer: &Buffer, area: Rect) -> String {
    (area.y..area.bottom())
        .map(|y| {
            (area.x..area.right())
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn state(panel: &MergePanel) -> Vec<(String, bool, bool)> {
    panel
        .doc
        .as_ref()
        .unwrap()
        .chunks
        .iter()
        .map(|chunk| {
            (
                chunk.result.clone(),
                chunk.pending(FileSide::Current),
                chunk.pending(FileSide::Incoming),
            )
        })
        .collect()
}

#[test]
fn automatic_resolution_applies_three_changes_and_leaves_two_conflicts() {
    let [base, left, right] = versions();
    let td = fixture_with(&base, &left, &right);
    let repo = td.path();
    let ctx = Context::discover(repo).unwrap();
    let path = repo.join("config with spaces.txt");
    let original = fs::read(&path).unwrap();
    let index = g(repo, &["ls-files", "--stage"]);
    let head = g(repo, &["rev-parse", "HEAD"]);
    let mut panel = MergePanel::default();
    panel.refresh(&ctx).unwrap();
    press(&mut panel, &ctx, KeyCode::Enter);
    let before = state(&panel);
    let buffer = draw(&mut panel, &ctx, 184);
    assert!(text(&buffer, Rect::new(0, 1, 184, 1)).contains("3 changes, 2 conflicts"));
    assert!(
        panel
            .hits
            .iter()
            .any(|(_, hit)| matches!(hit, Hit::ResolveAutomatically)),
        "automatic resolution must be available for ordinary non-conflicting changes"
    );

    press(&mut panel, &ctx, KeyCode::Char('m'));
    let doc = panel.doc.as_ref().unwrap();
    assert_eq!(doc.pending(), 2);
    assert!(doc
        .chunks
        .iter()
        .filter(|c| c.kind == Kind::Safe)
        .all(Chunk::done));
    assert!(doc
        .chunks
        .iter()
        .filter(|c| c.kind == Kind::Conflict)
        .all(|c| { c.result == c.base && FileSide::ALL.into_iter().all(|side| c.pending(side)) }));
    let expected = base
        .replace("Git 多客户端同步示例", "Git 多客户端冲突演练")
        .replace(
            "订单必须包含非空的 items 数组。",
            "client1: 订单必须包含至少一个商品。",
        )
        .replace("orders 必须为数组。", "client2: orders 必须为订单数组。");
    assert_eq!(
        doc.chunks
            .iter()
            .map(|c| c.result.as_str())
            .collect::<String>(),
        expected
    );
    assert!(!doc.chunks[panel.idx].done());
    assert_eq!(panel.hist.len(), 1);
    let after = state(&panel);
    let buffer = draw(&mut panel, &ctx, 184);
    assert!(text(&buffer, Rect::new(0, 1, 184, 1)).contains("0 changes, 2 conflicts"));
    assert!(!panel
        .hits
        .iter()
        .any(|(_, hit)| matches!(hit, Hit::ResolveAutomatically)));
    press(&mut panel, &ctx, KeyCode::Char('m'));
    assert_eq!(state(&panel), after);
    assert_eq!(panel.hist.len(), 1, "a no-op must not consume an undo step");
    assert!(!matches!(
        press(&mut panel, &ctx, KeyCode::Char('A')),
        Action::Confirm { .. }
    ));

    press(&mut panel, &ctx, KeyCode::Char('u'));
    assert_eq!(state(&panel), before);
    press(&mut panel, &ctx, KeyCode::Char('m'));
    press(&mut panel, &ctx, KeyCode::Char('s'));
    press(&mut panel, &ctx, KeyCode::Enter);
    assert_eq!(
        state(&panel),
        after,
        "session draft must retain automatic decisions"
    );
    assert_eq!(fs::read(&path).unwrap(), original);
    assert_eq!(g(repo, &["ls-files", "--stage"]), index);
    assert_eq!(g(repo, &["rev-parse", "HEAD"]), head);
}

#[test]
fn automatic_resolution_keeps_manual_decisions_and_undoes_a_mixed_batch() {
    let td = fixture();
    let ctx = Context::discover(td.path()).unwrap();
    let mut panel = MergePanel::default();
    panel.refresh(&ctx).unwrap();
    press(&mut panel, &ctx, KeyCode::Enter);
    // Separate blocks exercise both automatic rules and three existing decisions.
    let mut chunks = Vec::new();
    chunks.extend(diff3("safe\n", "changed\n", "safe\n"));
    chunks.extend(diff3(
        "开始 = 0 / 结束\r\n",
        "左 = 0 / 结束\r\n",
        "开始 = 0 / 右\r\n",
    ));
    let mut manual = diff3("manual\n", "left\n", "right\n").remove(0);
    manual.resolve("my edited result\n".into());
    chunks.push(manual);
    let mut ignored = diff3("ignored\n", "left\n", "ignored\n").remove(0);
    ignored.decide(FileSide::Current, false);
    chunks.push(ignored);
    let mut partial = diff3("开始 = 0 / 结束\n", "左 = 0 / 结束\n", "开始 = 0 / 右\n").remove(0);
    partial.decide(FileSide::Current, true);
    chunks.push(partial);
    // Two inserts at the same boundary must never be concatenated automatically.
    chunks.extend(diff3("abc\n", "aXbc\n", "aYbc\n"));
    panel.doc.as_mut().unwrap().chunks = chunks;
    panel.idx = 0;
    let before = state(&panel);
    press(&mut panel, &ctx, KeyCode::Char('a'));
    assert_eq!(state(&panel)[0], ("changed\n".into(), false, false));
    assert_eq!(
        &state(&panel)[1..],
        &before[1..],
        "All only applies non-conflicting changes"
    );
    press(&mut panel, &ctx, KeyCode::Char('u'));
    assert_eq!(state(&panel), before);
    press(&mut panel, &ctx, KeyCode::Char('m'));
    let after = state(&panel);
    assert_eq!(after[0], ("changed\n".into(), false, false));
    assert_eq!(after[1], ("左 = 0 / 右\r\n".into(), false, false));
    assert_eq!(&after[2..], &before[2..]);
    assert_eq!(panel.hist.len(), 1);
    press(&mut panel, &ctx, KeyCode::Char('u'));
    assert_eq!(state(&panel), before);
}

#[test]
fn automatic_resolution_toolbar_is_readable_and_clickable_at_all_widths() {
    let [base, left, right] = versions();
    let td = fixture_with(&base, &left, &right);
    let ctx = Context::discover(td.path()).unwrap();
    let mut panel = MergePanel::default();
    panel.refresh(&ctx).unwrap();
    press(&mut panel, &ctx, KeyCode::Enter);
    let before = state(&panel);
    press(&mut panel, &ctx, KeyCode::Char('m'));
    let expected = state(&panel);
    press(&mut panel, &ctx, KeyCode::Char('u'));

    for mode in [IconMode::NerdFont, IconMode::Unicode, IconMode::Ascii] {
        panel.icons = mode;
        for width in [44, 80, 120, 184] {
            let buffer = draw(&mut panel, &ctx, width);
            let button = panel
                .hits
                .iter()
                .find(|(_, hit)| matches!(hit, Hit::ResolveAutomatically))
                .expect("automatic action must stay visible when the toolbar shrinks")
                .0;
            let label = text(&buffer, button);
            assert!(
                label.contains("Auto") && label.contains('3') && label.contains("[m]"),
                "{width}: {label}"
            );
            if width == 184 {
                assert!(label.contains("Resolve Automatically"));
            }
            assert!(button.right() <= width);
            for (other, _) in panel.hits.iter().filter(|(r, _)| *r != button) {
                assert!(button.intersection(*other).is_empty());
            }
            let click = Event::Mouse(MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: button.right() - 2,
                row: button.y,
                modifiers: KeyModifiers::NONE,
            });
            panel.event(&click, &ctx);
            assert_eq!(state(&panel), expected);
            draw(&mut panel, &ctx, width);
            assert!(!panel
                .hits
                .iter()
                .any(|(_, hit)| matches!(hit, Hit::ResolveAutomatically)));
            press(&mut panel, &ctx, KeyCode::Char('u'));
            assert_eq!(state(&panel), before);
        }
    }
    panel.shortcut_labels.insert("m", "Ctrl-m".into());
    let buffer = draw(&mut panel, &ctx, 184);
    let button = panel
        .hits
        .iter()
        .find(|(_, hit)| matches!(hit, Hit::ResolveAutomatically))
        .unwrap()
        .0;
    assert!(text(&buffer, button).contains("[Ctrl-m]"));
    press(&mut panel, &ctx, KeyCode::Char('i'));
    draw(&mut panel, &ctx, 184);
    assert!(!panel
        .hits
        .iter()
        .any(|(_, hit)| matches!(hit, Hit::ResolveAutomatically)));
}
