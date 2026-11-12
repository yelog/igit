use super::*;

fn reviewed(panel: &PushPanel, context: &Context, root: usize, set_upstream: bool) -> Job {
    let entry = &panel.entries[root];
    Job {
        idx: root,
        path: context.repos[root].path.clone(),
        branch: entry.source.clone(),
        head: entry.head.clone(),
        t: entry.t.clone(),
        tracking: entry.tracking.clone(),
        url: "local-fixture".into(),
        config: entry.config.clone(),
        set_upstream,
        focused_ref: None,
    }
}

fn render(
    panel: &PushPanel,
    context: &Context,
    width: u16,
    height: u16,
    scroll: &mut u16,
) -> (String, Vec<(Rect, usize)>) {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    let mut hits = vec![];
    terminal
        .draw(|frame| {
            assert!(panel.draw_review(frame, frame.area(), context, scroll, true, &mut hits));
        })
        .unwrap();
    let buffer = terminal.backend().buffer();
    let text = (0..height)
        .map(|row| text_at(buffer, Rect::new(0, row, width, 1)))
        .collect::<Vec<_>>()
        .join("\n");
    (text, hits)
}

#[test]
fn push_review_batch_shows_only_reviewed_jobs_and_retains_update_and_tracking_choices() {
    for icons in [IconMode::Ascii, IconMode::Unicode, IconMode::NerdFont] {
        let (mut panel, context) = fixture(icons);
        for root in [0, 7] {
            panel.entries[root].head = "1234567890abcdef".repeat(3);
        }
        panel.entries[7].source = "topic".into();
        panel.entries[7].t.dst = "published-topic".into();
        panel.checked = (0..context.repos.len()).collect();
        panel.review = vec![
            reviewed(&panel, &context, 0, false),
            reviewed(&panel, &context, 7, true),
        ];
        panel.review_strategy = Some(Strategy::Rebase);
        panel.skip_hooks = true;
        panel.follow_tags = true;
        let (text, _) = render(&panel, &context, 184, 48, &mut 0);
        for label in [
            "Push 2 repositories",
            "From  master",
            "From  topic",
            "To    origin/master",
            "To    origin/published-topic",
            "1234567890ab",
            "2 commits",
            "123 commits",
            "moss-web",
            "mes-isg",
            "keep unchanged",
            "origin:published-topic after push succeeds",
            "Update: Rebase",
            "Rebase rewrites local commits and preserves merge topology.",
            "No force push",
            "Hooks  Skipped",
            "Follow tags  Enabled (annotated)",
            "Each repository runs separately; successes are retained.",
        ] {
            assert!(text.contains(label), "{label}\n{text}");
        }
        assert!(!text.contains("aps-hudong") && !text.contains("devicemate"));
        assert!(!text.contains("1234567890abcdef"));
        assert_eq!(panel.checked.len(), context.repos.len());
    }
}

#[test]
fn push_review_long_unicode_batches_scroll_without_moving_the_controls() {
    for icons in [IconMode::Ascii, IconMode::Unicode, IconMode::NerdFont] {
        let (mut panel, mut context) = fixture(icons);
        panel.entries[0].source = format!("feature/{}", "推送分支".repeat(20));
        panel.entries[0].t.dst = format!("publish/{}", "远端目标".repeat(20));
        panel.entries[0].head = "abcdef1234567890".repeat(3);
        context.repos[0].name = "仓库".repeat(20);
        context.repos[0].path = Path::new("/workspace").join("目录".repeat(30));
        panel.review = vec![reviewed(&panel, &context, 0, true)];
        panel.review_strategy = Some(Strategy::Merge);
        let mut scroll = 0;
        let (first, initial_hits) = render(&panel, &context, 44, 12, &mut scroll);
        assert!(first.contains("feature/推送分支"), "{first}");
        let mut rendered = first;
        for _ in 0..100 {
            let previous = scroll;
            scroll += 1;
            let (text, hits) = render(&panel, &context, 44, 12, &mut scroll);
            assert_eq!(hits, initial_hits);
            assert!(
                text.contains("Cancel") && text.contains("Confirm"),
                "{text}"
            );
            rendered.push_str(&text);
            if scroll == previous {
                break;
            }
        }
        let compact = |text: &str| {
            text.chars()
                .filter(|character| character.is_alphanumeric())
                .collect::<String>()
        };
        let observed = compact(&rendered);
        for label in [
            "publish/远端目标",
            "after push succeeds",
            "Repository",
            "仓库仓库",
            "目录目录",
            "Update: Merge",
            "No force push",
            "Hooks  Enabled",
            "Follow tags  Disabled",
            "successes are retained.",
        ] {
            assert!(
                observed.contains(&compact(label)),
                "{label} missing after scrolling {scroll} rows"
            );
        }
        assert!(scroll > 0);
        let (expanded, _) = render(&panel, &context, 184, 80, &mut scroll);
        assert_eq!(scroll, 0);
        let expanded = compact(&expanded);
        for value in [
            &panel.review[0].branch,
            &panel.review[0].t.dst,
            &context.repos[0].name,
        ] {
            assert!(
                expanded.contains(&compact(value)),
                "Expanded review is missing {value}"
            );
        }
    }
}
