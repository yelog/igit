use super::*;
use crate::log::tests::{commit, git, init};
use crossterm::event::{KeyEvent, MouseEvent};
use ratatui::{backend::TestBackend, Terminal};
use std::{collections::BTreeSet, fs, path::Path};

fn finish(comparison: &mut Comparison, context: &Context) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        comparison.poll(context).unwrap();
        if !comparison.pending() && comparison.preview_wakeup().is_none() {
            break;
        }
        assert!(Instant::now() < deadline, "comparison did not finish");
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn comparison(context: &Context, target: &str) -> Comparison {
    Comparison::new(
        Target {
            repository: context.repos[0].path.clone(),
            current: "refs/heads/main".into(),
            target: target.into(),
        },
        context,
        IconMode::Ascii,
    )
}

fn ids(panel: &LogPanel) -> BTreeSet<String> {
    panel
        .visible
        .iter()
        .map(|index| panel.hist.revs[*index].oid.clone())
        .collect()
}

fn snapshot(path: &Path) -> (String, String, Vec<u8>, String, String) {
    (
        git(path, &["rev-parse", "HEAD"]),
        git(path, &["show-ref"]),
        fs::read(path.join(".git/index")).unwrap(),
        git(path, &["diff"]),
        git(path, &["reflog", "--all", "--format=%H %gs"]),
    )
}

fn render(
    comparison: &mut Comparison,
    context: &Context,
    panes: &mut PaneLayout,
    width: u16,
    height: u16,
) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal
        .draw(|frame| comparison.draw(frame, frame.area(), context, panes))
        .unwrap();
    (0..height)
        .map(|row| {
            let mut text = String::new();
            let mut column = 0;
            while column < width {
                let symbol = terminal.backend().buffer()[(column, row)].symbol();
                text.push_str(symbol);
                column += cells(symbol).max(1);
            }
            text
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn compare_uses_both_reachable_differences_without_writing_git_state() {
    let temp = init();
    let path = temp.path();
    commit(path, "file", "base");
    git(path, &["branch", "topic"]);
    commit(path, "file", "current only");
    git(path, &["checkout", "-q", "topic"]);
    commit(path, "topic", "target only");
    git(path, &["checkout", "-q", "main"]);
    fs::write(path.join("file"), "staged\n").unwrap();
    git(path, &["add", "file"]);
    fs::write(path.join("file"), "unstaged\n").unwrap();
    let before = snapshot(path);
    let context = Context::discover(path).unwrap();
    let mut view = comparison(&context, "refs/heads/topic");
    assert!(view.pending());
    finish(&mut view, &context);
    for (index, range) in ["main..topic", "topic..main"].into_iter().enumerate() {
        let expected = git(path, &["rev-list", range])
            .lines()
            .map(str::to_owned)
            .collect();
        assert_eq!(ids(&view.panels[index]), expected);
    }
    assert_eq!(view.panels[0].details.files[0].raw, b"topic");
    assert_eq!(view.panels[1].details.files[0].raw, b"file");
    let mut panes = PaneLayout::default();
    for (width, height) in [(44, 12), (80, 25), (120, 36), (184, 48)] {
        let text = render(&mut view, &context, &mut panes, width, height);
        assert!(text.contains("Compare: topic and main"), "{text}");
        assert!(
            text.contains("target only") && text.contains("current only"),
            "{text}"
        );
        assert_eq!(panes.nodes().len(), 6);
        assert!(panes
            .nodes()
            .iter()
            .all(|(_, rect)| rect.right() <= width && rect.bottom() <= height));
    }
    for code in [
        K::Char('d'),
        K::Char('u'),
        K::Char('R'),
        K::Char('C'),
        K::Char('a'),
        K::F(2),
    ] {
        let action = view.event(
            &Event::Key(KeyEvent::new(code, Mods::NONE)),
            &context,
            &panes,
        );
        assert!(!matches!(
            action,
            Action::Confirm { .. } | Action::Refresh(..)
        ));
        assert!(!view.active().editing());
    }
    assert_eq!(snapshot(path), before);
}

#[test]
fn compare_empty_directions_distinguish_ancestry_filters_and_errors() {
    let temp = init();
    commit(temp.path(), "file", "base");
    git(temp.path(), &["branch", "topic"]);
    commit(temp.path(), "file", "current only");
    let context = Context::discover(temp.path()).unwrap();
    let mut view = comparison(&context, "refs/heads/topic");
    finish(&mut view, &context);
    let mut panes = PaneLayout::default();
    let text = render(&mut view, &context, &mut panes, 184, 48);
    assert!(
        text.contains("main contains all commits from topic"),
        "{text}"
    );
    assert!(text.contains("Select commit to view changes") && text.contains("Commit details"));
    assert!(view.panels[0].visible.is_empty());
    assert_eq!(view.panels[1].visible.len(), 1);
    assert_eq!(view.focused_pane(), PaneId::CompareCurrentGraph);
    view.focus_pane(PaneId::CompareCurrentGraph);
    view.active_mut().filter.query = "absent".into();
    view.active_mut().rebuild_visible(None).unwrap();
    let text = render(&mut view, &context, &mut panes, 184, 48);
    assert!(text.contains("No matching commits"), "{text}");
    view.event(
        &Event::Key(KeyEvent::new(K::Char('x'), Mods::NONE)),
        &context,
        &panes,
    );
    finish(&mut view, &context);
    assert_eq!(view.panels[1].visible.len(), 1);
    assert!(view
        .panels
        .iter()
        .all(|panel| panel.filter.comparison.is_some()));
    git(temp.path(), &["branch", "-D", "topic"]);
    view.refresh(&context);
    finish(&mut view, &context);
    let text = render(&mut view, &context, &mut panes, 184, 48);
    assert!(text.contains("Branch is unavailable: topic"), "{text}");
    assert!(!text.contains("contains all commits"), "{text}");
}

#[test]
fn compare_mouse_selection_diff_and_filters_stay_independent() {
    let temp = init();
    commit(temp.path(), "base", "base");
    git(temp.path(), &["branch", "topic"]);
    commit(temp.path(), "file", "current first");
    commit(temp.path(), "file", "current second");
    git(temp.path(), &["checkout", "-q", "topic"]);
    commit(temp.path(), "topic", "target first");
    commit(temp.path(), "topic", "target second");
    git(temp.path(), &["checkout", "-q", "main"]);
    let context = Context::discover(temp.path()).unwrap();
    let mut view = comparison(&context, "refs/heads/topic");
    finish(&mut view, &context);
    let mut panes = PaneLayout::default();
    render(&mut view, &context, &mut panes, 120, 36);
    let hit = view.panels[1]
        .hits
        .iter()
        .find(|(_, hit)| matches!(hit, Hit::Commit(1)))
        .unwrap()
        .0;
    view.event(
        &Event::Mouse(MouseEvent {
            kind: M::Down(MouseButton::Left),
            column: hit.x,
            row: hit.y,
            modifiers: Mods::NONE,
        }),
        &context,
        &panes,
    );
    finish(&mut view, &context);
    assert_eq!(view.focused_pane(), PaneId::CompareCurrentGraph);
    assert_eq!(view.panels[1].cursor, 1);
    assert_eq!(view.panels[0].cursor, 0);
    view.event(
        &Event::Key(KeyEvent::new(K::Enter, Mods::NONE)),
        &context,
        &panes,
    );
    finish(&mut view, &context);
    assert!(view.active().diff.as_ref().unwrap().comparison.is_some());
    assert!(!view.wants_close(&Event::Key(KeyEvent::new(K::Esc, Mods::NONE))));
    view.event(
        &Event::Key(KeyEvent::new(K::Esc, Mods::NONE)),
        &context,
        &panes,
    );
    assert!(view.active().diff.is_none());
    assert!(view.wants_close(&Event::Key(KeyEvent::new(K::Esc, Mods::NONE))));
    view.event(
        &Event::Key(KeyEvent::new(K::Char(']'), Mods::NONE)),
        &context,
        &panes,
    );
    assert_eq!(view.active, 0);
    view.event(
        &Event::Key(KeyEvent::new(K::Char('/'), Mods::NONE)),
        &context,
        &panes,
    );
    view.event(&Event::Paste("target first".into()), &context, &panes);
    view.event(
        &Event::Key(KeyEvent::new(K::Enter, Mods::NONE)),
        &context,
        &panes,
    );
    finish(&mut view, &context);
    assert_eq!(view.panels[0].visible.len(), 1);
    assert_eq!(view.panels[1].visible.len(), 2);
    view.refresh(&context);
    finish(&mut view, &context);
    assert_eq!(view.panels[1].cursor, 1);
    assert_eq!(view.panels[0].filter.query, "target first");
}

#[test]
fn compare_merge_remote_unicode_and_identical_tips_follow_git() {
    let temp = init();
    let path = temp.path();
    commit(path, "base", "base");
    git(path, &["branch", "topic"]);
    commit(path, "main", "main work");
    git(path, &["checkout", "-q", "topic"]);
    commit(path, "topic", "topic work");
    git(path, &["checkout", "-qb", "side"]);
    commit(path, "side", "side work");
    git(path, &["checkout", "-q", "topic"]);
    git(path, &["merge", "--no-ff", "-qm", "merge side", "side"]);
    let remote = "refs/remotes/origin/功能";
    git(path, &["update-ref", remote, "topic"]);
    git(path, &["checkout", "-q", "main"]);
    let context = Context::discover(path).unwrap();
    let mut view = comparison(&context, remote);
    finish(&mut view, &context);
    assert_eq!(view.panels[0].rev().unwrap().parents.len(), 2);
    for (index, range) in [
        "main..refs/remotes/origin/功能",
        "refs/remotes/origin/功能..main",
    ]
    .into_iter()
    .enumerate()
    {
        assert_eq!(
            ids(&view.panels[index]),
            git(path, &["rev-list", range])
                .lines()
                .map(str::to_owned)
                .collect()
        );
    }
    for icons in [IconMode::Ascii, IconMode::Unicode, IconMode::NerdFont] {
        for panel in &mut view.panels {
            panel.icons = icons;
        }
        for (width, height) in [(44, 12), (80, 25), (120, 36), (184, 48)] {
            let text = render(
                &mut view,
                &context,
                &mut PaneLayout::default(),
                width,
                height,
            );
            assert!(
                text.contains("origin/功能") && text.contains("merge side"),
                "{text}"
            );
        }
    }
    let scoped = view.context(&context);
    view.active_mut().open_diff(&scoped).unwrap();
    assert!(view
        .active()
        .diff
        .as_ref()
        .unwrap()
        .comparison
        .as_ref()
        .unwrap()
        .is_three_way());
    git(path, &["update-ref", remote, "main"]);
    view.refresh(&context);
    finish(&mut view, &context);
    assert!(view.panels.iter().all(|panel| panel.visible.is_empty()));
    let text = render(&mut view, &context, &mut PaneLayout::default(), 184, 48);
    assert!(text.contains("main contains all commits from origin/功能"));
    assert!(text.contains("origin/功能 contains all commits from main"));
}

#[test]
fn compare_scope_changes_discard_inflight_results_and_never_include_other_roots() {
    let temp = tempfile::tempdir().unwrap();
    for name in ["one", "two"] {
        let path = temp.path().join(name);
        fs::create_dir(&path).unwrap();
        git(&path, &["init", "-q", "-b", "main"]);
        git(&path, &["config", "user.name", "Testing"]);
        git(&path, &["config", "user.email", "tests@example.invalid"]);
        commit(&path, "base", "base");
        git(&path, &["branch", "topic"]);
        commit(&path, "file", name);
    }
    let mut context = Context::discover(temp.path()).unwrap();
    let mut view = comparison(&context, "refs/heads/topic");
    let stale = view.target.read(&context).unwrap();
    let sender = view.loading.hold();
    context.scope = [1].into();
    view.poll(&context).unwrap();
    sender.send(Ok(stale)).unwrap();
    finish(&mut view, &context);
    assert!(view.status.as_ref().unwrap().contains("outside Roots"));
    assert!(view.panels.iter().all(|panel| panel.visible.is_empty()));
    context.scope = [0, 1].into();
    finish(&mut view, &context);
    assert!(view.status.is_none());
    assert_eq!(view.panels[1].visible.len(), 1);
    assert!(view
        .panels
        .iter()
        .flat_map(|panel| &panel.hist.revs)
        .all(|rev| rev.root == 0));
    let before = ids(&view.panels[1]);
    view.panels[1].filter.roots = Some([1].into());
    let scoped = view.context(&context);
    view.panels[1].rebuild_visible(None).unwrap();
    assert!(view.panels[1].visible.is_empty());
    view.panels[1].click(Hit::Reset, &scoped).unwrap();
    finish(&mut view, &context);
    assert_eq!(ids(&view.panels[1]), before);
}

#[test]
fn compare_pagination_refresh_and_dividers_preserve_each_side() {
    let temp = init();
    let path = temp.path();
    commit(path, "base", "base");
    git(path, &["branch", "topic"]);
    let tree = git(path, &["rev-parse", "HEAD^{tree}"]);
    let mut parent = git(path, &["rev-parse", "HEAD"]);
    for index in 0..170 {
        parent = git(
            path,
            &[
                "commit-tree",
                &tree,
                "-p",
                &parent,
                "-m",
                &format!("extra {index}"),
            ],
        );
    }
    git(path, &["update-ref", "refs/heads/topic", &parent]);
    let context = Context::discover(path).unwrap();
    let mut view = comparison(&context, "refs/heads/topic");
    finish(&mut view, &context);
    assert_eq!(view.panels[0].visible.len(), 160);
    assert!(view.panels[0].hist.more.contains(&0));
    let mut panes = PaneLayout::default();
    render(&mut view, &context, &mut panes, 120, 36);
    view.event(
        &Event::Key(KeyEvent::new(K::Char('+'), Mods::NONE)),
        &context,
        &panes,
    );
    finish(&mut view, &context);
    assert_eq!(view.panels[0].visible.len(), 170);
    assert!(view.panels[0].hist.more.is_empty());
    let before = panes.rect(PaneId::CompareTargetGraph).unwrap();
    assert!(panes.resize(before, Axis::Width, 5));
    assert!(panes.resize(before, Axis::Height, 2));
    render(&mut view, &context, &mut panes, 120, 36);
    let after = panes.rect(PaneId::CompareTargetGraph).unwrap();
    assert_eq!(after.width, before.width + 5);
    assert_eq!(after.height, before.height + 2);
    assert_eq!(
        after.width,
        panes.rect(PaneId::CompareCurrentGraph).unwrap().width
    );
    view.focus_pane(PaneId::CompareCurrentInfo);
    panes.maximized = true;
    render(&mut view, &context, &mut panes, 120, 36);
    assert_eq!(
        panes
            .nodes()
            .iter()
            .filter(|(_, rect)| !rect.is_empty())
            .count(),
        1
    );
    assert!(!panes.rect(PaneId::CompareCurrentInfo).unwrap().is_empty());
}

#[test]
fn compare_refresh_drains_obsolete_preview_when_a_branch_disappears() {
    let temp = init();
    commit(temp.path(), "file", "base");
    git(temp.path(), &["branch", "topic"]);
    commit(temp.path(), "file", "current change");
    let context = Context::discover(temp.path()).unwrap();
    let mut view = comparison(&context, "refs/heads/topic");
    finish(&mut view, &context);
    let scoped = view.context(&context);
    view.panels[1].request_preview(Duration::ZERO);
    view.panels[1].poll_preview(&scoped).unwrap();
    assert!(view.preview_wakeup().is_some());
    git(temp.path(), &["branch", "-D", "topic"]);
    view.refresh(&context);
    finish(&mut view, &context);
    assert!(view.status.as_ref().unwrap().contains("unavailable"));
    assert!(view.preview_wakeup().is_none());
    assert!(view
        .panels
        .iter()
        .all(|panel| panel.details.files.is_empty()));
}

#[test]
fn compare_revalidates_the_current_branch_before_the_initial_read() {
    let temp = init();
    commit(temp.path(), "file", "base");
    git(temp.path(), &["branch", "topic"]);
    let context = Context::discover(temp.path()).unwrap();
    let mut view = comparison(&context, "refs/heads/topic");
    git(temp.path(), &["checkout", "-qb", "other"]);
    finish(&mut view, &context);
    assert!(view
        .status
        .as_ref()
        .unwrap()
        .contains("Current branch changed"));
    assert!(view.panels.iter().all(|panel| panel.visible.is_empty()));
}

#[test]
fn compare_compact_dividers_do_not_capture_commit_or_file_rows() {
    let temp = init();
    commit(temp.path(), "base", "base");
    git(temp.path(), &["branch", "topic"]);
    commit(temp.path(), "main", "current first");
    git(temp.path(), &["checkout", "-q", "topic"]);
    commit(temp.path(), "topic", "target first");
    commit(temp.path(), "topic", "target second");
    git(temp.path(), &["checkout", "-q", "main"]);
    let context = Context::discover(temp.path()).unwrap();
    let mut view = comparison(&context, "refs/heads/topic");
    finish(&mut view, &context);
    let mut panes = PaneLayout::default();
    render(&mut view, &context, &mut panes, 44, 10);
    let hits: Vec<_> = view
        .panels
        .iter()
        .flat_map(|panel| &panel.hits)
        .filter_map(|(area, hit)| matches!(hit, Hit::Commit(_) | Hit::File(_)).then_some(*area))
        .collect();
    assert!(!hits.is_empty());
    for area in hits {
        assert!(!panes.mouse(&MouseEvent {
            kind: M::Down(MouseButton::Left),
            column: area.x + area.width.saturating_sub(2) / 2,
            row: area.y,
            modifiers: Mods::NONE,
        }));
    }
}
