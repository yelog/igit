use super::*;

fn menu_text(terminal: &Terminal<TestBackend>, area: Rect) -> String {
    (area.y..area.bottom())
        .map(|y| row_text(terminal, Rect::new(area.x, y, area.width, 1)))
        .collect::<Vec<_>>()
        .join("\n")
}

fn menu_choices(panel: &LogPanel) -> Vec<(usize, Rect)> {
    panel
        .hits
        .iter()
        .filter_map(|(area, hit)| match hit {
            Hit::Dropdown(DropdownHit::Choice(index)) => Some((*index, *area)),
            _ => None,
        })
        .collect()
}

fn many_authors(panel: &mut LogPanel) {
    for (index, revision) in panel.hist.revs.iter_mut().enumerate() {
        revision.author = format!("Author {index:02} 林悦 e\u{301} 👩‍💻 with a long display name");
    }
}

#[test]
fn author_menu_opens_with_guidance_groups_and_the_applied_choice() {
    let (_directory, context, mut panel) = navigation_history();
    panel.filter.author = "me".into();
    panel.begin(Field::Author, &context).unwrap();
    assert_eq!(panel.editor.as_ref().unwrap().1, "");
    assert_eq!(panel.choice, Some(1));

    let mut terminal = Terminal::new(TestBackend::new(120, 36)).unwrap();
    render(&mut panel, &mut terminal, &context);
    let area = target(&panel, |hit| {
        matches!(hit, Hit::Dropdown(DropdownHit::Surface))
    });
    let text = menu_text(&terminal, area);
    assert!(text.contains("Search authors..."), "{text}");
    assert!(text.contains("Quick filters"), "{text}");
    assert!(text.contains("Authors"), "{text}");
    assert!(text.contains("Git identity"), "{text}");
    let current = target(&panel, |hit| {
        matches!(hit, Hit::Dropdown(DropdownHit::Choice(1)))
    });
    assert!(row_text(&terminal, current).contains(panel.icons.glyph(Icon::Success)));
    assert_eq!(
        terminal.backend().buffer()[(current.x, current.y)].bg,
        SELECT
    );

    key(&mut panel, K::Down, Mods::NONE, &context);
    render(&mut panel, &mut terminal, &context);
    let current = target(&panel, |hit| {
        matches!(hit, Hit::Dropdown(DropdownHit::Choice(1)))
    });
    assert!(row_text(&terminal, current).contains(panel.icons.glyph(Icon::Success)));
    assert_ne!(
        terminal.backend().buffer()[(current.x, current.y)].bg,
        SELECT
    );
    key(&mut panel, K::Esc, Mods::NONE, &context);
    assert_eq!(panel.filter.author, "me");
}

#[test]
fn author_menu_no_matches_explains_custom_input_without_changing_other_filters() {
    let (_directory, context, mut panel) = navigation_history();
    panel.filter.branch = Some("HEAD".into());
    panel.filter.date = Some(DateFilter::Last7Days);
    panel.filter.author = "me".into();
    panel.begin(Field::Author, &context).unwrap();
    panel.event(
        &Event::Paste("unknown-author@example.invalid".into()),
        &context,
    );

    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    render(&mut panel, &mut terminal, &context);
    let area = target(&panel, |hit| {
        matches!(hit, Hit::Dropdown(DropdownHit::Surface))
    });
    let text = menu_text(&terminal, area);
    assert!(text.contains("No matching authors"), "{text}");
    assert!(text.contains("Apply text"), "{text}");
    assert!(panel.suggestions.is_empty());
    assert_eq!(panel.filter.author, "me");

    key(&mut panel, K::Enter, Mods::NONE, &context);
    assert_eq!(panel.filter.author, "unknown-author@example.invalid");
    assert_eq!(panel.filter.single_branch(), Some("HEAD"));
    assert_eq!(panel.filter.date, Some(DateFilter::Last7Days));
    assert!(!panel.editing());
}

#[test]
fn author_menu_resizes_scrolls_and_keeps_hover_targets_stable() {
    let (_directory, context, mut panel) = navigation_history();
    many_authors(&mut panel);
    for icons in [IconMode::Ascii, IconMode::Unicode, IconMode::NerdFont] {
        panel.icons = icons;
        for (width, height) in [(44, 12), (80, 24), (120, 36), (180, 36)] {
            panel.begin(Field::Author, &context).unwrap();
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            render(&mut panel, &mut terminal, &context);
            let trigger = target(&panel, |hit| matches!(hit, Hit::Filter(Field::Author)));
            let area = target(&panel, |hit| {
                matches!(hit, Hit::Dropdown(DropdownHit::Surface))
            });
            assert_eq!(area.y, trigger.bottom());
            assert!(area.right() <= width && area.bottom() <= height);
            assert!(menu_text(&terminal, area).contains("Select"));
            assert!(menu_choices(&panel).iter().any(|(index, _)| *index >= 2));

            key(&mut panel, K::End, Mods::NONE, &context);
            render(&mut panel, &mut terminal, &context);
            let last = panel.suggestions.len() - 1;
            assert_eq!(panel.choice, Some(last));
            let before = menu_choices(&panel);
            assert_eq!(before.last().unwrap().0, last);
            let (hovered, hover_area) = before[0];
            assert!(panel.hover_menu_at(hover_area.x + 2, hover_area.y));
            render(&mut panel, &mut terminal, &context);
            assert_eq!(panel.choice, Some(hovered));
            assert_eq!(menu_choices(&panel), before);
            assert_eq!(panel.filter.author, "");
            assert!(!panel.hover_menu_at(hover_area.x + 2, hover_area.y));

            key(&mut panel, K::Home, Mods::NONE, &context);
            render(&mut panel, &mut terminal, &context);
            assert_eq!(panel.choice, Some(0));
            assert_eq!(menu_choices(&panel)[0].0, 0);
            key(&mut panel, K::PageDown, Mods::NONE, &context);
            assert!(panel.choice.unwrap() > 0);
            key(&mut panel, K::PageUp, Mods::NONE, &context);
            assert_eq!(panel.choice, Some(0));
            key(&mut panel, K::Tab, Mods::NONE, &context);
            assert_eq!(panel.choice, Some(1));
            key(&mut panel, K::BackTab, Mods::SHIFT, &context);
            assert_eq!(panel.choice, Some(0));
            key(&mut panel, K::Esc, Mods::NONE, &context);
        }
    }
}

#[test]
fn author_menu_search_clear_and_scrollbar_do_not_apply_a_draft() {
    let (_directory, context, mut panel) = navigation_history();
    many_authors(&mut panel);
    panel.filter.author = "me".into();
    panel.begin(Field::Author, &context).unwrap();
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    panel.event(&Event::Paste("林悦".into()), &context);
    render(&mut panel, &mut terminal, &context);
    assert_eq!(panel.suggestions.len(), 80);
    let clear = target(&panel, |hit| {
        matches!(hit, Hit::Dropdown(DropdownHit::AuthorClear))
    });
    click_at(&mut panel, clear, &context);
    assert_eq!(panel.editor.as_ref().unwrap().1, "");
    assert_eq!(panel.choice, Some(1));
    assert_eq!(panel.filter.author, "me");
    render(&mut panel, &mut terminal, &context);

    let scroll = target(
        &panel,
        |hit| matches!(hit, Hit::Dropdown(DropdownHit::AuthorScroll(index)) if *index == 81),
    );
    click_at(&mut panel, scroll, &context);
    render(&mut panel, &mut terminal, &context);
    assert_eq!(panel.choice, Some(81));
    assert_eq!(panel.filter.author, "me");
    let input = target(&panel, |hit| {
        matches!(hit, Hit::Dropdown(DropdownHit::AuthorInput))
    });
    click_at(&mut panel, input, &context);
    assert_eq!(panel.choice, None);
    panel.event(&Event::Paste("custom draft".into()), &context);
    render(&mut panel, &mut terminal, &context);
    let cancel = target(&panel, |hit| {
        matches!(hit, Hit::Dropdown(DropdownHit::Cancel))
    });
    click_at(&mut panel, cancel, &context);
    assert!(!panel.editing());
    assert_eq!(panel.filter.author, "me");
}

#[test]
fn author_menu_retains_custom_applied_values_and_handles_short_windows() {
    let (_directory, context, mut panel) = navigation_history();
    for author in ["", "me", "ME", "not in loaded history", "林悦 e\u{301} 👩‍💻"] {
        panel.filter.author = author.into();
        for (width, height) in [(1, 1), (12, 4), (18, 8), (44, 8), (80, 12)] {
            panel.begin(Field::Author, &context).unwrap();
            let selected = panel.choice.unwrap();
            assert!(panel.author_applied(&panel.suggestions[selected].0));
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            render(&mut panel, &mut terminal, &context);
            let area = target(&panel, |hit| {
                matches!(hit, Hit::Dropdown(DropdownHit::Surface))
            });
            assert!(area.right() <= width && area.bottom() <= height);
            key(&mut panel, K::Esc, Mods::NONE, &context);
            assert_eq!(panel.filter.author, author);
        }
    }
}

#[test]
fn author_menu_mouse_selection_and_wheel_preserve_repository_data() {
    let (directory, context, mut panel) = navigation_history();
    let root = directory.path();
    std::fs::write(root.join("file"), "staged\n").unwrap();
    git(root, &["add", "file"]);
    std::fs::write(root.join("file"), "unstaged\n").unwrap();
    let head = git(root, &["rev-parse", "HEAD"]);
    let index = std::fs::read(root.join(".git/index")).unwrap();
    let working = std::fs::read(root.join("file")).unwrap();
    let status = git(root, &["status", "--porcelain"]);
    many_authors(&mut panel);
    panel.begin(Field::Author, &context).unwrap();
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    render(&mut panel, &mut terminal, &context);
    let area = target(&panel, |hit| {
        matches!(hit, Hit::Dropdown(DropdownHit::Surface))
    });
    panel.event(
        &Event::Mouse(MouseEvent {
            kind: M::ScrollDown,
            column: area.x + 2,
            row: area.y + 2,
            modifiers: Mods::NONE,
        }),
        &context,
    );
    assert_eq!(panel.choice, Some(3));
    render(&mut panel, &mut terminal, &context);
    let chosen = target(&panel, |hit| {
        matches!(hit, Hit::Dropdown(DropdownHit::Choice(3)))
    });
    let author = panel.suggestions[3].0.clone();
    click_at(&mut panel, chosen, &context);
    assert_eq!(panel.filter.author, author);
    assert!(!panel.editing());
    assert_eq!(git(root, &["rev-parse", "HEAD"]), head);
    assert_eq!(std::fs::read(root.join(".git/index")).unwrap(), index);
    assert_eq!(std::fs::read(root.join("file")).unwrap(), working);
    assert_eq!(git(root, &["status", "--porcelain"]), status);
}
