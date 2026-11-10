use super::*;
mod diff;
mod text_selection;
use crossterm::event::{KeyEvent, MouseEvent};
use ratatui::{backend::TestBackend, buffer::Buffer, Terminal};
use unicode_width::UnicodeWidthStr;

fn fixture(icons: IconMode) -> (PushPanel, Context) {
    let names = [
        "moss-web",
        "aps-hudong",
        "dcc",
        "devicemate",
        "ｕｉ-ui",
        "maas",
        "mes",
        "mes-isg",
        "mes-mbg",
        "sep",
        "qds",
    ];
    let c = Context {
        workspace: "/workspace".into(),
        state_dir: "/state".into(),
        scope: (0..names.len()).collect(),
        repos: names
            .iter()
            .enumerate()
            .map(|(i, name)| Repo {
                path: Path::new("/workspace").join(name),
                name: name.to_string(),
                branch: "master".into(),
                submodule: i > 0,
                parent: (i > 0).then_some(0),
            })
            .collect(),
    };
    let files = [
        ("dev/iframe-proxy/compose.yaml", b'A'),
        ("dev/iframe-proxy/nginx.conf", b'D'),
        ("dev/iframe-proxy/README.md", b'A'),
        ("src/layouts/customer-service/index.vue", b'M'),
        ("src/locales/en_US/system.ts", b'M'),
        ("src/locales/zh_CN/system.ts", b'A'),
        ("package.json", b'T'),
        ("scripts/renamed.mjs", b'R'),
        ("src/copied.js", b'C'),
    ]
    .into_iter()
    .map(|(path, status)| Change {
        path: path.into(),
        previous: None,
        status,
        commit: None,
    })
    .collect::<Vec<_>>();
    let p = PushPanel {
        icons,
        entries: names
            .iter()
            .enumerate()
            .map(|(idx, _)| Entry {
                idx,
                source: "master".into(),
                t: Target {
                    remote: "origin".into(),
                    dst: "master".into(),
                },
                n: if idx == 0 {
                    2
                } else if idx == 7 {
                    123
                } else {
                    0
                },
                ..Default::default()
            })
            .collect(),
        checked: [0].into(),
        preview: Preview {
            tree: FileTree::new(&files, Grouping::default()),
            files,
            body: "master → origin:master".into(),
            summary: "master → origin:master".into(),
        },
        ..Default::default()
    };
    (p, c)
}

fn draw(p: &mut PushPanel, c: &Context, width: u16, height: u16) -> Buffer {
    let mut term = Terminal::new(TestBackend::new(width, height)).unwrap();
    term.draw(|f| p.draw(f, f.area(), c)).unwrap();
    term.backend().buffer().clone()
}

fn text_at(buf: &Buffer, row: Rect) -> String {
    let mut x = row.x;
    let mut out = String::new();
    while x < row.right() {
        let symbol = buf[(x, row.y)].symbol();
        out.push_str(symbol);
        x += symbol.width().max(1) as u16;
    }
    out
}

fn find_text(buf: &Buffer, row: Rect, needle: &str) -> u16 {
    let text = text_at(buf, row);
    let i = text
        .find(needle)
        .unwrap_or_else(|| panic!("{needle:?} missing in {text:?}"));
    row.x + text[..i].width() as u16
}

fn key(p: &mut PushPanel, c: &Context, code: KeyCode) {
    p.event(&Event::Key(KeyEvent::new(code, KeyModifiers::NONE)), c);
}

fn click(p: &mut PushPanel, c: &Context, row: Rect) {
    p.event(
        &Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: row.right() - 1,
            row: row.y,
            modifiers: KeyModifiers::NONE,
        }),
        c,
    );
}

#[test]
fn push_group_by_menu_supports_mouse_and_keys_without_checking_a_root() {
    for icons in [IconMode::NerdFont, IconMode::Unicode, IconMode::Ascii] {
        for (width, height) in [(44, 12), (80, 24), (120, 36)] {
            let (mut p, c) = fixture(icons);
            let initial = draw(&mut p, &c, width, height);
            let eye = p
                .hits
                .iter()
                .find(|(_, h)| matches!(h, Hit::GroupBy))
                .unwrap()
                .0;
            assert_eq!(
                text_at(&initial, eye).split_whitespace().next(),
                Some(icons.glyph(Icon::Preview))
            );
            if width == 80 {
                click(&mut p, &c, eye);
            } else {
                key(&mut p, &c, KeyCode::Char('o'));
            }
            let buf = draw(&mut p, &c, width, height);
            assert!(p.editing(), "Group By must capture input");
            let item = |buf: &Buffer, label: &str, checked| {
                let needle = format!("{} {label}", icons.check(checked));
                (0..height)
                    .find_map(|y| {
                        let row = Rect::new(0, y, width, 1);
                        text_at(buf, row).contains(&needle).then(|| {
                            Rect::new(find_text(buf, row, &needle), y, needle.width() as u16, 1)
                        })
                    })
                    .unwrap_or_else(|| panic!("{needle} missing at {width}×{height}"))
            };
            item(&buf, "Directory", true);
            let repo = item(&buf, "Repository", true);
            item(&buf, "Module", false);
            let popup = p
                .hits
                .iter()
                .find(|(_, h)| matches!(h, Hit::MenuSurface))
                .unwrap()
                .0;
            click(&mut p, &c, Rect::new(popup.x, popup.bottom() - 1, 1, 1));
            assert!(p.editing() && !p.show_details);
            click(&mut p, &c, repo);
            key(&mut p, &c, KeyCode::Char('j'));
            key(&mut p, &c, KeyCode::Char(' ')); // Module
            assert!(p.grouping.enabled(Group::Module));
            key(&mut p, &c, KeyCode::Char(' '));
            key(&mut p, &c, KeyCode::Home);
            key(&mut p, &c, KeyCode::Enter); // Directory
            let buf = draw(&mut p, &c, width, height);
            for label in ["Directory", "Repository", "Module"] {
                let unchecked = format!("{} {label}", icons.check(false));
                assert!((0..height)
                    .any(|y| text_at(&buf, Rect::new(0, y, width, 1)).contains(&unchecked)));
            }
            key(&mut p, &c, KeyCode::Esc);
            assert!(!p.editing());
            assert_eq!(p.focus, usize::from(width == 80));
            assert_eq!(p.selected(), Some((0, None)));
            assert_eq!(p.checked, BTreeSet::from([0]));
            assert_eq!(p.preview.tree.visible.len(), p.preview.files.len());
            assert!(p
                .preview
                .tree
                .visible
                .iter()
                .all(|&id| matches!(p.preview.tree.nodes[id].kind, Kind::File(_))));
            assert!(!p.show_details);
        }
    }
}

#[test]
fn flat_rows_show_distinct_parent_paths_and_their_original_status() {
    let (mut p, c) = fixture(IconMode::Ascii);
    key(&mut p, &c, KeyCode::Char('o'));
    key(&mut p, &c, KeyCode::Enter);
    key(&mut p, &c, KeyCode::Esc);
    for width in [80, 150] {
        let buf = draw(&mut p, &c, width, 24);
        for (name, color) in [("en_US", ACCENT), ("zh_CN", GREEN)] {
            let row = p.hits.iter().find_map(|(r, h)| match *h {
                Hit::File(i) if matches!(p.preview.tree.nodes[p.preview.tree.visible[i]].kind, Kind::File(file)
                    if p.preview.files[file].path == Path::new(&format!("src/locales/{name}/system.ts"))) => Some(*r),
                _ => None,
            }).unwrap();
            let line = text_at(&buf, row);
            assert!(
                line.contains("system.ts  ") && line.contains(name),
                "{line}"
            );
            assert_eq!(buf[(find_text(&buf, row, "system.ts"), row.y)].fg, color);
            assert_eq!(buf[(find_text(&buf, row, name), row.y)].fg, MUTED);
        }
    }
}

#[test]
fn push_columns_align_sources_targets_counts_and_submodules_in_terminal_cells() {
    for icons in [IconMode::NerdFont, IconMode::Unicode, IconMode::Ascii] {
        for width in [120, 180, 300] {
            let (mut p, c) = fixture(icons);
            p.results.insert(0, Ok(PushOutcome::Complete));
            p.results.insert(3, Ok(PushOutcome::Complete));
            p.results.insert(7, Err("offline".into()));
            let buf = draw(&mut p, &c, width, 36);
            let rows: Vec<_> = p
                .hits
                .iter()
                .filter_map(|(r, h)| matches!(h, Hit::Row(_)).then_some(*r))
                .collect();
            let positions = |needle: &str, rows: &[Rect]| {
                rows.iter()
                    .map(|&r| find_text(&buf, r, needle))
                    .collect::<BTreeSet<_>>()
            };
            assert_eq!(positions("master", &rows).len(), 1, "{icons:?} {width}");
            assert_eq!(positions("origin:", &rows).len(), 1, "{icons:?} {width}");
            if width >= 180 {
                assert_eq!(positions("submodule", &rows[1..]).len(), 1);
                find_text(&buf, rows[3], "devicemate");
            }
            if width >= 180 || icons != IconMode::Ascii {
                assert_eq!(positions(icons.glyph(Icon::Ahead), &rows).len(), 1);
            }
            let target = find_text(&buf, rows[0], "origin:");
            assert_eq!(buf[(target, rows[0].y)].fg, ACCENT);
            assert_eq!(buf[(target, rows[1].y)].fg, MUTED);
        }
    }
}

#[test]
fn long_repo_and_branch_columns_clip_without_moving_controls_or_targets() {
    for icons in [IconMode::NerdFont, IconMode::Unicode, IconMode::Ascii] {
        for (width, height) in [(38, 10), (44, 12), (80, 24), (120, 36), (180, 36)] {
            let (mut p, mut c) = fixture(icons);
            c.repos[0].name = "ｕｉ👩‍💻-a-very-long-repository-name".repeat(3);
            p.entries[0].source = "feature/client-service-with-a-long-branch".into();
            p.entries[0].t.dst = "release/a-very-long-target-branch".into();
            let buf = draw(&mut p, &c, width, height);
            for (r, _) in &p.hits {
                assert!(
                    r.right() <= width && r.bottom() <= height,
                    "{icons:?}: {r:?}"
                );
            }
            let targets: Vec<_> = p
                .hits
                .iter()
                .filter_map(|(r, h)| match h {
                    Hit::Target(id) if p.panes.at(r.x, r.y) == Some(PaneId::Outgoing) => {
                        Some((*id, *r))
                    }
                    _ => None,
                })
                .collect();
            assert!(
                targets
                    .iter()
                    .map(|(_, r)| r.x)
                    .collect::<BTreeSet<_>>()
                    .len()
                    <= 1
            );
            if width >= 120 {
                assert!(!targets.is_empty());
                assert!(text_at(&buf, targets[0].1).contains('…'));
            }
            let check = p
                .hits
                .iter()
                .find(|(_, h)| matches!(h, Hit::Check(0)))
                .unwrap()
                .0;
            assert_eq!(text_at(&buf, check), icons.check(true));
            click(&mut p, &c, check);
            assert!(p.checked.is_empty());
            assert_eq!(p.selected(), Some((0, None)));
        }
    }
}

#[test]
fn push_file_tree_uses_change_colours_and_independent_file_type_icons() {
    for icons in [IconMode::NerdFont, IconMode::Unicode, IconMode::Ascii] {
        let (mut p, c) = fixture(icons);
        let buf = draw(&mut p, &c, 180, 36);
        assert!(!p.show_details);
        assert_eq!(
            p.panes.rect(PaneId::PushFiles).unwrap().height,
            p.panes.rect(PaneId::Outgoing).unwrap().height
        );
        for (i, file) in p.preview.files.iter().enumerate() {
            let visible = p
                .preview
                .tree
                .visible
                .iter()
                .position(
                    |&n| matches!(p.preview.tree.nodes[n].kind, Kind::File(file) if file == i),
                )
                .unwrap();
            let row = p
                .hits
                .iter()
                .find(|(_, h)| matches!(h, Hit::File(v) if *v == visible))
                .unwrap()
                .0;
            let name = file.path.file_name().unwrap().to_str().unwrap();
            let x = find_text(&buf, row, name);
            let color = match file.status {
                b'A' | b'C' => GREEN,
                b'D' => RED,
                b'R' | b'T' => crate::ui::idea::CONFLICT,
                _ => ACCENT,
            };
            assert_eq!(buf[(x, row.y)].fg, color, "{name}");
            let (glyph, tint) = icons.file(&file.path);
            let x = find_text(&buf, row, glyph);
            assert_eq!(buf[(x, row.y)].fg, tint, "{name}");
            assert!(text_at(&buf, row).trim_end().ends_with(name));
        }
        assert!((0..buf.area.height)
            .any(|y| text_at(&buf, Rect::new(90, y, 90, 1)).contains("dev/iframe-proxy 3 files")));
    }
}

#[test]
fn push_toolbars_and_footer_fit_without_overlapping_in_every_icon_mode() {
    for icons in [IconMode::NerdFont, IconMode::Unicode, IconMode::Ascii] {
        for (width, height) in [(38, 10), (44, 12), (80, 24), (120, 36), (180, 36)] {
            let (mut p, c) = fixture(icons);
            let buf = draw(&mut p, &c, width, height);
            let hit = |p: &PushPanel, matches: fn(&Hit) -> bool| {
                p.hits.iter().find(|(_, h)| matches(h)).unwrap().0
            };
            let send = hit(&p, |h| matches!(h, Hit::Send));
            let cancel = hit(&p, |h| matches!(h, Hit::Cancel));
            let tags = hit(&p, |h| matches!(h, Hit::Tags));
            let hooks = hit(&p, |h| matches!(h, Hit::Hooks));
            assert_eq!(text_at(&buf, send), " Push ");
            assert_eq!(text_at(&buf, cancel), " Cancel ");
            assert_eq!(send.right(), width);
            assert_eq!(cancel.y, send.y);
            assert!(cancel.right() < send.x);
            assert!(hooks.y < cancel.y || hooks.right() < cancel.x);
            assert_eq!(buf[(tags.x, tags.y)].bg, BG);
            assert_eq!(buf[(send.x, send.y)].bg, SEL);
            if width == 38 {
                assert!(text_at(&buf, Rect::new(19, 0, 19, 1)).contains("Files · 9"));
            }
            assert_eq!(
                hit(&p, |h| matches!(h, Hit::Row(0))).y,
                hit(&p, |h| matches!(h, Hit::File(0))).y
            );
            assert!(!p
                .preview
                .tree
                .visible
                .iter()
                .any(|&id| { matches!(p.preview.tree.nodes[id].kind, Kind::Module) }));
            for action in [Hit::EditTarget, Hit::Fetch] {
                let r = p
                    .hits
                    .iter()
                    .find(|(_, h)| std::mem::discriminant(h) == std::mem::discriminant(&action))
                    .unwrap()
                    .0;
                assert_eq!(p.panes.at(r.x, r.y), Some(PaneId::Outgoing));
            }
            assert!(!p.hits.iter().any(|(_, h)| matches!(h, Hit::Retry)));
            p.results.insert(7, Err("offline".into()));
            draw(&mut p, &c, width, height);
            let retry = hit(&p, |h| matches!(h, Hit::Retry));
            assert_eq!(p.panes.at(retry.x, retry.y), Some(PaneId::Outgoing));
            for (r, _) in &p.hits {
                assert!(!r.is_empty() && r.right() <= width && r.bottom() <= height);
            }
            assert_eq!(p.checked, BTreeSet::from([0]));
            assert!(p.review.is_empty() && p.inflight.is_none());
        }
    }
}

#[test]
fn push_footer_disables_empty_hidden_and_busy_selections() {
    for icons in [IconMode::NerdFont, IconMode::Unicode, IconMode::Ascii] {
        let (mut p, mut c) = fixture(icons);
        p.checked.clear();
        draw(&mut p, &c, 44, 12);
        assert!(!p.hits.iter().any(|(_, h)| matches!(h, Hit::Send)));
        p.checked.insert(0);
        let (_tx, rx) = mpsc::channel();
        p.inflight = Some(rx);
        p.busy = "Pushing…".into();
        let buf = draw(&mut p, &c, 44, 12);
        assert!(!p
            .hits
            .iter()
            .any(|(_, h)| matches!(h, Hit::Send | Hit::Fetch)));
        assert!((0..12).any(|y| text_at(&buf, Rect::new(0, y, 44, 1)).contains("Pushing…")));
        let close = p
            .hits
            .iter()
            .find(|(_, h)| matches!(h, Hit::Cancel))
            .unwrap()
            .0;
        assert_eq!(text_at(&buf, close), " Close ");
        p.inflight = None;
        c.scope.remove(&0);
        p.entries.retain(|e| e.idx != 0);
        let buf = draw(&mut p, &c, 44, 12);
        assert!(!p.hits.iter().any(|(_, h)| matches!(h, Hit::Send)));
        assert!((0..12).any(|y| {
            text_at(&buf, Rect::new(0, y, 44, 1)).contains("1 selected roots hidden")
        }));
        assert_eq!(p.checked, BTreeSet::from([0]));
    }
}

#[test]
fn file_tree_mouse_keyboard_scrolling_and_pane_cycle_keep_push_selection() {
    for icons in [IconMode::NerdFont, IconMode::Unicode, IconMode::Ascii] {
        let (mut p, c) = fixture(icons);
        p.preview.files.extend((0..30).map(|i| Change {
            path: format!("src/locales/extra-{i}/system.ts").into(),
            previous: None,
            status: b'M',
            commit: None,
        }));
        p.preview.tree = FileTree::new(&p.preview.files, p.grouping);
        draw(&mut p, &c, 80, 24);
        key(&mut p, &c, KeyCode::Tab);
        assert_eq!(p.focus, 1);
        key(&mut p, &c, KeyCode::Tab);
        assert_eq!(p.focus, 0);
        let arrow = p
            .hits
            .iter()
            .find(|(_, h)| matches!(h, Hit::FileFold(0)))
            .unwrap()
            .0;
        click(&mut p, &c, arrow);
        assert_eq!(p.preview.tree.visible, [0]);
        assert_eq!(p.checked, BTreeSet::from([0]));
        assert!(!p.show_details);
        key(&mut p, &c, KeyCode::Char('l'));
        assert_eq!(p.preview.tree.visible.len() + 1, p.preview.tree.nodes.len());
        key(&mut p, &c, KeyCode::Char('['));
        assert_eq!(p.preview.tree.visible, [0]);
        key(&mut p, &c, KeyCode::Char(']'));
        let len = p.preview.tree.visible.len();
        for _ in 0..len + 5 {
            key(&mut p, &c, KeyCode::Char('j'));
        }
        draw(&mut p, &c, 80, 24);
        assert_eq!(p.preview.tree.cursor, len - 1);
        assert!(p
            .hits
            .iter()
            .any(|(_, h)| matches!(h, Hit::File(v) if *v == len - 1)));
        assert!(p.preview.tree.offset > 0);
        let collapse = p
            .hits
            .iter()
            .find(|(_, h)| matches!(h, Hit::FoldAll(true)))
            .unwrap()
            .0;
        click(&mut p, &c, collapse);
        assert_eq!(p.preview.tree.visible, [0]);
        key(&mut p, &c, KeyCode::Char('d'));
        assert!(p.show_details);
        assert_eq!(p.preview.body, p.preview.summary);
        key(&mut p, &c, KeyCode::Tab);
        assert_eq!(p.focus, 2);
        key(&mut p, &c, KeyCode::Esc);
        assert_eq!(p.focus, 1);
        assert!(!p.show_details);
        assert_eq!(p.checked, BTreeSet::from([0]));
    }
}

#[test]
fn pane_navigation_push_resizes_around_details_without_copying_or_checking() {
    let (mut p, c) = fixture(IconMode::Ascii);
    p.show_details = true;
    p.focus_pane(PaneId::PushDetails);
    let mouse = |p: &mut PushPanel, kind, x, y| {
        p.event(
            &Event::Mouse(MouseEvent {
                kind,
                column: x,
                row: y,
                modifiers: KeyModifiers::NONE,
            }),
            &c,
        )
    };
    let drag = |p: &mut PushPanel, from: (u16, u16), to: (u16, u16)| {
        for (kind, (x, y)) in [
            (MouseEventKind::Down(MouseButton::Left), from),
            (MouseEventKind::Drag(MouseButton::Left), to),
            (MouseEventKind::Up(MouseButton::Left), to),
        ] {
            assert!(matches!(mouse(p, kind, x, y), Action::None));
        }
    };
    let initial = draw(&mut p, &c, 120, 36);
    let left = p.panes.rect(PaneId::Outgoing).unwrap();
    let files = p.panes.rect(PaneId::PushFiles).unwrap();
    let diff = p.panes.rect(PaneId::PushDetails).unwrap();
    assert_eq!(initial[(diff.x, diff.y)].fg, ACCENT);
    assert_eq!(initial[(left.x, left.y)].fg, LINE);
    p.event(
        &Event::Key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL)),
        &c,
    );

    let x = files.x; // Start on the right pane's left border.
    drag(&mut p, (x, diff.y + 1), (x + 7, diff.y + 1));
    draw(&mut p, &c, 120, 36);
    let y = diff.y; // Start on the lower pane's top border.
    drag(&mut p, (files.right() - 2, y), (files.right() - 2, y - 3));
    draw(&mut p, &c, 120, 36);
    assert_eq!(
        p.panes.rect(PaneId::Outgoing).unwrap().width,
        left.width + 7
    );
    let resized = p.panes.rect(PaneId::PushDetails).unwrap();
    assert_eq!(resized.height, diff.height + 3);
    assert_eq!(resized.x, diff.x + 7);
    assert_eq!(p.focused_pane(), PaneId::PushDetails);
    assert_eq!(p.selected_text(), Some(p.preview.body.as_str()));

    // Wheel and hover may browse another pane; only a click takes its keyboard focus.
    mouse(&mut p, MouseEventKind::Moved, left.x + 3, left.bottom() - 2);
    mouse(
        &mut p,
        MouseEventKind::ScrollDown,
        resized.x + 2,
        resized.y - 2,
    );
    assert_eq!(p.focused_pane(), PaneId::PushDetails);
    assert_eq!(p.preview.tree.cursor, 1);
    mouse(
        &mut p,
        MouseEventKind::Down(MouseButton::Left),
        left.x + 3,
        left.bottom() - 2,
    );
    assert_eq!(p.focused_pane(), PaneId::Outgoing);
    key(&mut p, &c, KeyCode::Char('d'));
    draw(&mut p, &c, 120, 36);
    assert!(p.panes.rect(PaneId::PushDetails).is_none());
    assert!(!p
        .panes
        .can_resize(p.panes.rect(PaneId::PushFiles).unwrap(), Axis::Height));
    assert_eq!(p.checked, BTreeSet::from([0]));
    assert_eq!(p.selected(), Some((0, None)));
    assert!(p.review.is_empty() && p.inflight.is_none());
}

#[test]
fn maximized_push_preserves_footer_scope_and_saved_dividers() {
    let (mut p, c) = fixture(IconMode::Ascii);
    p.show_details = true;
    draw(&mut p, &c, 120, 36);
    let outgoing = p.panes.rect(PaneId::Outgoing).unwrap();
    assert!(p.panes.resize(outgoing, Axis::Width, 7));
    let details = p.panes.rect(PaneId::PushDetails).unwrap();
    assert!(p.panes.resize(details, Axis::Height, 3));
    draw(&mut p, &c, 120, 36);
    let restored = p.panes.nodes().to_vec();
    let checked = p.checked.clone();
    let selected = p.selected();
    let file_cursor = p.preview.tree.cursor;

    p.focus_pane(PaneId::PushDetails);
    draw(&mut p, &c, 120, 36);
    p.detail_selection.select_all();
    assert!(p.detail_selection.selected_text().is_some());
    p.panes.maximized = true;
    for id in PushPanel::PANES {
        p.focus_pane(id);
        draw(&mut p, &c, 120, 36);
        assert_eq!(
            p.panes
                .nodes()
                .iter()
                .map(|(id, _)| *id)
                .collect::<Vec<_>>(),
            PushPanel::PANES
        );
        for &(other, area) in p.panes.nodes() {
            assert_eq!(!area.is_empty(), id == other);
            if other == id {
                assert_eq!(area, Rect::new(0, 0, 120, 34));
                assert!(!p.panes.can_resize(area, Axis::Width));
                assert!(!p.panes.can_resize(area, Axis::Height));
            }
        }
        for (_, hit) in &p.hits {
            match hit {
                Hit::Row(_) | Hit::Check(_) | Hit::Fold(_) | Hit::EditTarget | Hit::Fetch => {
                    assert_eq!(id, PaneId::Outgoing)
                }
                Hit::File(_) | Hit::FileFold(_) | Hit::FoldAll(_) | Hit::GroupBy | Hit::Details => {
                    assert_eq!(id, PaneId::PushFiles)
                }
                _ => {}
            }
        }
        assert!(p.hits.iter().any(|(_, hit)| matches!(hit, Hit::Send)));
        assert!(p.hits.iter().any(|(_, hit)| matches!(hit, Hit::Cancel)));
        assert!(p.detail_selection.selected_text().is_none());
        assert_eq!(p.checked, checked);
        assert_eq!(p.selected(), selected);
        assert_eq!(p.preview.tree.cursor, file_cursor);
    }
    p.panes.maximized = false;
    draw(&mut p, &c, 120, 36);
    assert_eq!(p.panes.nodes(), restored);
    assert!(p.review.is_empty() && p.inflight.is_none());
}

#[test]
fn maximized_push_hides_details_selection_geometry_and_skips_closed_details() {
    let (mut p, c) = fixture(IconMode::Ascii);
    p.show_details = true;
    p.focus_pane(PaneId::PushDetails);
    draw(&mut p, &c, 120, 36);
    let detail = pane("", true).inner(p.panes.rect(PaneId::PushDetails).unwrap());
    p.panes.maximized = true;
    p.focus_pane(PaneId::Outgoing);
    draw(&mut p, &c, 120, 36);
    assert!(p
        .detail_selection
        .mouse(&MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: detail.x,
            row: detail.y,
            modifiers: KeyModifiers::NONE,
        })
        .is_none());
    key(&mut p, &c, KeyCode::Char('d'));
    assert!(!p.show_details);
    for (width, height) in [(44, 12), (1, 1)] {
        draw(&mut p, &c, width, height);
        assert_eq!(
            p.panes
                .nodes()
                .iter()
                .map(|(id, _)| *id)
                .collect::<Vec<_>>(),
            [PaneId::Outgoing, PaneId::PushFiles]
        );
        assert_eq!(p.focused_pane(), PaneId::Outgoing);
    }
}

#[test]
fn target_tracking_choice_and_save_cancel_are_reachable_at_supported_sizes() {
    for icons in [IconMode::Ascii, IconMode::Unicode, IconMode::NerdFont] {
        for (width, height) in [(44, 12), (80, 24), (120, 36)] {
            let (mut panel, c) = fixture(icons);
            let checked = panel.checked.clone();
            let initial = draw(&mut panel, &c, width, height);
            let target = panel
                .hits
                .iter()
                .find_map(|(r, h)| matches!(h, Hit::EditTarget).then_some(*r))
                .unwrap();
            assert!(!text_at(&initial, target).is_empty());
            click(&mut panel, &c, target);
            let buf = draw(&mut panel, &c, width, height);
            assert!(panel.editing());
            assert!(!panel.target_edit.as_ref().unwrap().set_upstream);
            let hit = |p: &PushPanel, predicate: fn(&Hit) -> bool| {
                p.hits
                    .iter()
                    .rev()
                    .find_map(|(r, h)| predicate(h).then_some(*r))
                    .unwrap()
            };
            let checkbox = hit(&panel, |h| matches!(h, Hit::Upstream));
            assert!(text_at(&buf, checkbox).contains("Set upstream after push"));
            assert!(text_at(&buf, checkbox).contains("[Tab]"));
            let save = hit(&panel, |h| matches!(h, Hit::SaveTarget));
            let cancel = hit(&panel, |h| matches!(h, Hit::CancelTarget));
            assert_eq!(text_at(&buf, save), " Save Enter ");
            assert_eq!(text_at(&buf, cancel), " Cancel Esc ");
            assert!(cancel.right() < save.x);
            for (rect, _) in &panel.hits {
                assert!(!rect.is_empty() && rect.right() <= width && rect.bottom() <= height);
            }
            click(&mut panel, &c, checkbox);
            assert!(panel.target_edit.as_ref().unwrap().set_upstream);
            key(&mut panel, &c, KeyCode::BackTab);
            assert!(!panel.target_edit.as_ref().unwrap().set_upstream);
            key(&mut panel, &c, KeyCode::Tab);
            assert!(panel.target_edit.as_ref().unwrap().set_upstream);
            // Pane controls stay covered; clicking Push under the editor cannot
            // approve anything or save the staged tracking choice.
            let send = hit(&panel, |h| matches!(h, Hit::Send));
            click(&mut panel, &c, send);
            assert!(panel.editing() && panel.review.is_empty());
            click(&mut panel, &c, cancel);
            assert!(!panel.editing());
            assert!(panel.tracking_choices.is_empty() && panel.targets.is_empty());
            assert_eq!(panel.checked, checked);
            key(&mut panel, &c, KeyCode::Char('t'));
            key(&mut panel, &c, KeyCode::Tab);
            key(&mut panel, &c, KeyCode::Esc);
            assert!(!panel.editing());
            assert_eq!(panel.checked, checked);
        }
    }
}

#[test]
fn pane_search_push_reveals_outgoing_commits_and_changed_files_without_checking_repos() {
    let (mut p, c) = fixture(IconMode::Ascii);
    p.entries[0].logs.push(Out {
        id: "aaaa".into(),
        msg: "First needle".into(),
    });
    p.entries[7].logs.push(Out {
        id: "bbbb".into(),
        msg: "Second NEEDLE".into(),
    });
    let preview = std::mem::take(&mut p.preview);
    let checks = p.checked.clone();
    let find = |p: &mut PushPanel, term: &str| {
        key(p, &c, KeyCode::Char('/'));
        assert!(p.editing());
        p.event(&Event::Paste(term.into()), &c);
        key(p, &c, KeyCode::Enter);
    };
    find(&mut p, "needle");
    assert_eq!(p.selected(), Some((0, Some(0))));
    assert!(!p.expanded.contains(&7));
    key(&mut p, &c, KeyCode::Char('n'));
    assert_eq!(p.selected(), Some((7, Some(0))));
    assert!(p.expanded.contains(&7));
    key(&mut p, &c, KeyCode::Char('n'));
    assert_eq!(p.selected(), Some((0, Some(0))));
    assert_eq!(p.checked, checks);

    p.preview_load.cancel();
    p.preview_request = None;
    p.preview = preview;
    p.focus_pane(PaneId::PushFiles);
    key(&mut p, &c, KeyCode::Char('['));
    find(&mut p, "system.ts");
    let path = |p: &PushPanel| {
        p.preview.files[p.preview.tree.file_index().unwrap()]
            .path
            .clone()
    };
    assert_eq!(path(&p), Path::new("src/locales/en_US/system.ts"));
    let buffer = draw(&mut p, &c, 80, 24);
    assert!(buffer.content().iter().any(|cell| cell.symbol() == "s"
        && (Some(cell.bg) == crate::ui::search::CURRENT.bg
            || cell.modifier.contains(Modifier::REVERSED))));
    key(&mut p, &c, KeyCode::Char('N'));
    assert_eq!(path(&p), Path::new("src/locales/zh_CN/system.ts"));
    assert_eq!(p.checked, checks);
    assert!(p.diff.is_none() && !p.busy());
}
