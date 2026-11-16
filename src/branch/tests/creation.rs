use super::*;

fn remote_branch(c: &Context, remote: &str, name: &str) -> String {
    let repo = &c.repos[0].path;
    run(repo, &["remote", "add", remote, "../unused-local-remote"]);
    let reference = format!("refs/remotes/{remote}/{name}");
    let oid = run(
        repo,
        &[
            "commit-tree",
            "HEAD^{tree}",
            "-p",
            "HEAD",
            "-m",
            "remote tip",
        ],
    );
    run(repo, &["update-ref", &reference, &oid]);
    reference
}

#[test]
fn remote_create_prefills_name_and_tracks_without_switching() {
    let (_temp, c, mut panel) = fixture();
    let repo = &c.repos[0].path;
    let reference = remote_branch(&c, "origin", "develop");
    reload(&mut panel, &c);
    select(&mut panel, 0, &reference);
    let before = state(repo);
    let config = fs::read(repo.join(".git/config")).unwrap();
    let other = state(&c.repos[1].path);

    assert!(matches!(key(&mut panel, &c, K::Char('a')), Action::None));
    let input = panel.input.as_ref().unwrap();
    assert_eq!(input.operation, Operation::Create);
    assert_eq!(input.name, "develop");
    for (width, height) in [(44, 12), (80, 25), (120, 36)] {
        let text = screen(&mut panel, &c, width, height);
        assert!(text.contains("Create branch"), "{text}");
        assert!(text.contains("develop"), "{text}");
        assert!(text.contains("Enter Review"), "{text}");
    }
    key(&mut panel, &c, K::Esc);
    assert!(panel.input.is_none());
    assert_eq!(state(repo), before);
    assert_eq!(fs::read(repo.join(".git/config")).unwrap(), config);

    key(&mut panel, &c, K::Char('a'));
    let Action::Confirm { title, body, token } = key(&mut panel, &c, K::Enter) else {
        panic!("remote creation must open a review");
    };
    assert_eq!(title, "Create branch");
    assert_eq!(token, "branch:apply");
    assert!(body.contains("Create local branch: develop"), "{body}");
    assert!(body.contains("Track: origin/develop"), "{body}");
    assert!(body.contains("Stay on: main"), "{body}");
    assert!(!body.contains("no upstream"), "{body}");
    assert_eq!(state(repo), before);
    assert_eq!(fs::read(repo.join(".git/config")).unwrap(), config);
    assert!(matches!(
        panel.confirm(&token, &c),
        Action::Refresh(NoticeLevel::Success, _)
    ));
    let created = source(&c, "refs/heads/develop");
    assert_eq!(created.oid, source(&c, &reference).oid);
    assert_ne!(created.oid, before[0]);
    assert_eq!(created.upstream, reference);
    assert!(!created.current);
    assert_eq!(
        run(repo, &["config", "--get", "branch.develop.remote"]),
        "origin"
    );
    assert_eq!(
        run(repo, &["config", "--get", "branch.develop.merge"]),
        "refs/heads/develop"
    );
    assert_eq!(state(repo)[..4], before[..4]);
    assert_eq!(state(&c.repos[1].path), other);
}

#[test]
fn remote_create_preserves_nested_names_and_uses_the_selected_remote() {
    for (remote, name) in [
        ("upstream", "feature/中文"),
        ("team/origin", "fix/nested/topic"),
    ] {
        let (_temp, c, mut panel) = fixture();
        let repo = &c.repos[0].path;
        let reference = remote_branch(&c, remote, name);
        remote_branch(&c, "origin", name);
        reload(&mut panel, &c);
        select(&mut panel, 0, &reference);
        let before = state(repo);
        key(&mut panel, &c, K::Char('a'));
        assert_eq!(panel.input.as_ref().unwrap().name, name);
        assert!(matches!(
            key(&mut panel, &c, K::Enter),
            Action::Confirm { .. }
        ));
        assert!(matches!(
            panel.confirm("branch:apply", &c),
            Action::Refresh(NoticeLevel::Success, _)
        ));
        let created = source(&c, &format!("refs/heads/{name}"));
        assert_eq!(created.oid, source(&c, &reference).oid);
        assert_eq!(created.upstream, reference);
        assert_eq!(
            run(repo, &["config", "--get", &format!("branch.{name}.remote")]),
            remote
        );
        assert_eq!(
            run(repo, &["config", "--get", &format!("branch.{name}.merge")]),
            format!("refs/heads/{name}")
        );
        assert_eq!(state(repo)[..4], before[..4]);
    }
}

#[test]
fn remote_create_custom_name_tracks_despite_git_defaults_and_preserves_mixed_staging() {
    for auto_setup in ["false", "inherit", "simple"] {
        let (_temp, c, mut panel) = fixture();
        let repo = &c.repos[0].path;
        let reference = remote_branch(&c, "origin", "develop");
        run(repo, &["config", "branch.autoSetupMerge", auto_setup]);
        fs::write(repo.join("file"), "staged\n").unwrap();
        fs::write(repo.join("other"), "other staged\n").unwrap();
        run(repo, &["add", "."]);
        fs::write(repo.join("file"), "unstaged\n").unwrap();
        fs::write(repo.join("untracked"), "keep untracked\n").unwrap();
        reload(&mut panel, &c);
        select(&mut panel, 0, &reference);
        let before = state(repo);
        let index = fs::read(repo.join(".git/index")).unwrap();
        let other = state(&c.repos[1].path);

        key(&mut panel, &c, K::Char('a'));
        panel.event(&Event::Key(KeyEvent::new(K::Char('u'), Mods::CONTROL)), &c);
        panel.event(&Event::Paste("local/中文\n".into()), &c);
        assert_eq!(panel.input.as_ref().unwrap().name, "local/中文");
        let Action::Confirm { body, token, .. } = key(&mut panel, &c, K::Enter) else {
            panic!("custom remote creation must open a review");
        };
        assert!(body.contains("Create local branch: local/中文"), "{body}");
        assert!(body.contains("Track: origin/develop"), "{body}");
        assert!(matches!(
            panel.confirm(&token, &c),
            Action::Refresh(NoticeLevel::Success, _)
        ));
        let created = source(&c, "refs/heads/local/中文");
        assert_eq!(created.upstream, reference, "{auto_setup}");
        assert_eq!(created.oid, source(&c, &reference).oid);
        assert_eq!(state(repo)[..4], before[..4]);
        assert_eq!(fs::read(repo.join(".git/index")).unwrap(), index);
        assert_eq!(
            fs::read_to_string(repo.join("untracked")).unwrap(),
            "keep untracked\n"
        );
        assert_eq!(state(&c.repos[1].path), other);
    }
}

#[test]
fn remote_create_keeps_existing_local_branch_and_retains_the_editable_name() {
    let (_temp, c, mut panel) = fixture();
    let repo = &c.repos[0].path;
    let reference = remote_branch(&c, "origin", "develop");
    run(repo, &["branch", "develop"]);
    let existing = source(&c, "refs/heads/develop");
    reload(&mut panel, &c);
    select(&mut panel, 0, &reference);
    let before = state(repo);
    let config = fs::read(repo.join(".git/config")).unwrap();
    key(&mut panel, &c, K::Char('a'));
    assert!(matches!(
        key(&mut panel, &c, K::Enter),
        Action::Notice(NoticeLevel::Error, _)
    ));
    let input = panel.input.as_ref().unwrap();
    assert_eq!(input.name, "develop");
    assert!(input.error.contains("Local branch already exists: develop"));
    assert!(panel.review.is_none());
    assert_eq!(state(repo), before);
    assert_eq!(fs::read(repo.join(".git/config")).unwrap(), config);

    panel.event(&Event::Paste("-copy".into()), &c);
    assert!(matches!(
        key(&mut panel, &c, K::Enter),
        Action::Confirm { .. }
    ));
    assert!(matches!(
        panel.confirm("branch:apply", &c),
        Action::Refresh(NoticeLevel::Success, _)
    ));
    assert_eq!(source(&c, "refs/heads/develop"), existing);
    assert_eq!(source(&c, "refs/heads/develop-copy").upstream, reference);
    assert_eq!(state(repo)[..4], before[..4]);
}

#[test]
fn remote_create_rejects_unconfigured_sources_without_writes() {
    let (_temp, c, mut panel) = fixture();
    let repo = &c.repos[0].path;
    let reference = "refs/remotes/missing/develop";
    run(repo, &["update-ref", reference, "HEAD"]);
    reload(&mut panel, &c);
    select(&mut panel, 0, reference);
    let before = state(repo);
    let config = fs::read(repo.join(".git/config")).unwrap();
    assert!(matches!(
        key(&mut panel, &c, K::Char('a')),
        Action::Notice(NoticeLevel::Error, _)
    ));
    assert!(panel.input.is_none());
    assert!(Review::prepare(&c, 0, &source(&c, reference), Operation::Create, "develop").is_err());
    assert_eq!(state(repo), before);
    assert_eq!(fs::read(repo.join(".git/config")).unwrap(), config);
}

#[test]
fn remote_create_rejects_stale_reviews_without_writes() {
    for change in ["source", "name", "remote", "scope"] {
        let (_temp, mut c, _) = fixture();
        let repo = c.repos[0].path.clone();
        let reference = remote_branch(&c, "origin", "develop");
        let plan = review(&c, &reference, Operation::Create, "develop");
        match change {
            "source" => {
                run(&repo, &["update-ref", &reference, "HEAD"]);
            }
            "name" => {
                run(&repo, &["branch", "develop"]);
            }
            "remote" => {
                run(&repo, &["remote", "remove", "origin"]);
            }
            "scope" => {
                c.scope.remove(&0);
            }
            _ => unreachable!(),
        }
        let before = state(&repo);
        let config = fs::read(repo.join(".git/config")).unwrap();
        assert!(plan.execute(&c).is_err(), "{change}");
        assert_eq!(state(&repo), before, "{change}");
        assert_eq!(fs::read(repo.join(".git/config")).unwrap(), config);
    }
}

#[test]
fn local_create_keeps_empty_default_and_does_not_inherit_tracking() {
    let (_temp, c, mut panel) = fixture();
    let repo = &c.repos[0].path;
    tracking(&c, "main", "main");
    run(repo, &["config", "branch.autoSetupMerge", "always"]);
    reload(&mut panel, &c);
    select(&mut panel, 0, "refs/heads/main");
    let before = state(repo);
    key(&mut panel, &c, K::Char('a'));
    assert!(panel.input.as_ref().unwrap().name.is_empty());
    panel.event(&Event::Paste("local-copy".into()), &c);
    let Action::Confirm { body, token, .. } = key(&mut panel, &c, K::Enter) else {
        panic!("local creation must open a review");
    };
    assert!(body.contains("The new branch has no upstream."), "{body}");
    assert!(matches!(
        panel.confirm(&token, &c),
        Action::Refresh(NoticeLevel::Success, _)
    ));
    assert!(source(&c, "refs/heads/local-copy").upstream.is_empty());
    assert_eq!(state(repo)[..4], before[..4]);
}
