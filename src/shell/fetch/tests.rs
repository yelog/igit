use super::*;
use crate::{
    core::git,
    shell::test_support::{fixture, key, screen, snapshot},
};
use std::fs;

fn remote(directory: &Path, repo: &Path) -> PathBuf {
    let path = directory.join("remote.git");
    git(
        directory,
        &[
            "clone",
            "-q",
            "--bare",
            repo.to_str().unwrap(),
            path.to_str().unwrap(),
        ],
    )
    .unwrap();
    git(repo, &["remote", "add", "origin", path.to_str().unwrap()]).unwrap();
    path
}

fn advanced_tip(repo: &Path, remote: &Path) -> String {
    let head = git(repo, &["rev-parse", "HEAD"]).unwrap();
    let tree = git(repo, &["rev-parse", "HEAD^{tree}"]).unwrap();
    let oid = git(
        repo,
        &["commit-tree", &tree, "-p", &head, "-m", "Remote advance"],
    )
    .unwrap();
    git(remote, &["fetch", "--", repo.to_str().unwrap(), &oid]).unwrap();
    git(remote, &["update-ref", "refs/heads/main", &oid]).unwrap();
    oid
}

fn target(app: &Shell) -> Target {
    backend::discover(&app.ctx.repos[0].path, false, &[], &[])
        .unwrap()
        .remove(0)
}

fn install(app: &mut Shell) -> Target {
    app.poll_auto_fetch();
    let target = target(app);
    app.auto_fetch.install(Discovery {
        inputs: app.auto_fetch.inputs.clone(),
        demanded: true,
        roots: vec![(target.path.clone(), Ok(vec![target.clone()]))],
    });
    target
}

fn hold(app: &mut Shell, target: Target) -> mpsc::Sender<Result<Outcome, String>> {
    let (sender, receiver) = mpsc::channel();
    app.auto_fetch.running.push(Running {
        key: target.key.clone(),
        target,
        control: ReadControl::default(),
        receiver,
    });
    sender
}

fn wait(app: &mut Shell, ready: impl Fn(&Shell) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        app.poll_auto_fetch();
        if ready(app) {
            break;
        }
        assert!(Instant::now() < deadline, "Automatic fetch timed out");
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn startup_is_inert_until_the_runtime_frame_and_delay() {
    let (directory, mut app) = fixture();
    let repo = app.ctx.repos[0].path.clone();
    remote(directory.path(), &repo);
    app.poll_auto_fetch();
    assert!(!app.auto_fetch.active);
    assert!(!app.auto_fetch.discovery.pending());
    app.auto_fetch.activate(Instant::now());
    app.poll_auto_fetch();
    assert!(!app.auto_fetch.discovery.pending());
    assert!(!app.auto_fetch.running());
    app.auto_fetch.startup = Some(Instant::now());
    wait(&mut app, |app| {
        !app.auto_fetch.items.is_empty()
            && app
                .auto_fetch
                .items
                .values()
                .all(|item| item.success.is_some())
    });
    assert!(git(&repo, &["rev-parse", "refs/remotes/origin/main"]).is_ok());
}

#[test]
fn automatic_fetch_preserves_head_index_working_files_and_drafts() {
    let (directory, mut app) = fixture();
    let repo = app.ctx.repos[0].path.clone();
    let remote = remote(directory.path(), &repo);
    let tip = advanced_tip(&repo, &remote);
    app.panel_event(&key(KeyCode::Char('c')));
    app.panel_event(&Event::Paste("Keep this draft".into()));
    app.panel_event(&key(KeyCode::Esc));
    assert!(!app.commit.editing());
    let before = snapshot(&repo);
    let index = fs::read(repo.join(".git/index")).unwrap();
    app.auto_fetch.activate(Instant::now());
    app.auto_fetch.startup = Some(Instant::now());
    wait(&mut app, |app| {
        app.auto_fetch
            .items
            .values()
            .any(|item| item.success.is_some())
    });
    assert_eq!(snapshot(&repo), before);
    assert_eq!(fs::read(repo.join(".git/index")).unwrap(), index);
    assert_eq!(
        git(&repo, &["rev-parse", "refs/remotes/origin/main"]).unwrap(),
        tip
    );
    let rendered = screen(&mut app, 120, 36);
    assert!(rendered.contains("Fetched"));
    assert!(rendered.contains("Keep this draft"));
    assert!(!screen(&mut app, 44, 12).contains("Stopping background"));
}

#[test]
fn freshness_requests_coalesce_and_periodic_deadlines_remain_distinct() {
    let (directory, app) = fixture();
    remote(directory.path(), &app.ctx.repos[0].path);
    let mut item = Item::new(target(&app));
    let now = Instant::now();
    let config = FetchConfig::default();
    item.succeeded(now);
    assert!(!item.due(now + Duration::from_secs(60), &config));
    item.demanded = true;
    assert!(!item.due(now + Duration::from_secs(59), &config));
    assert!(item.due(now + Duration::from_secs(60), &config));
    item.succeeded(now + Duration::from_secs(60));
    assert!(!item.due(now + Duration::from_secs(359), &config));
    assert!(item.due(now + Duration::from_secs(360), &config));
}

#[test]
fn a_fresh_focus_request_does_not_shorten_the_next_periodic_fetch() {
    let (directory, mut app) = fixture();
    remote(directory.path(), &app.ctx.repos[0].path);
    let target = install(&mut app);
    let now = Instant::now();
    let item = app.auto_fetch.items.get_mut(&target.key).unwrap();
    item.succeeded(now);
    item.demanded = true;
    app.auto_fetch
        .launch(&app.config.fetch, now + Duration::from_secs(5));
    let item = &app.auto_fetch.items[&target.key];
    assert!(!item.demanded);
    assert!(!item.due(now + Duration::from_secs(60), &app.config.fetch));
    assert!(!app.auto_fetch.running());
}

#[test]
fn failures_back_off_and_old_manual_results_do_not_resume_authentication() {
    let (directory, mut app) = fixture();
    remote(directory.path(), &app.ctx.repos[0].path);
    let target = install(&mut app);
    let now = Instant::now();
    let observation = Observation {
        path: target.path,
        remote: target.key.remote.clone(),
        endpoint: target.endpoint,
        branch: None,
        tip: None,
    };
    let item = app.auto_fetch.items.get_mut(&target.key).unwrap();
    item.succeeded(now);
    item.failed("Authentication failed".into(), now + Duration::from_secs(2));
    app.auto_fetch.apply_observation(&observation, now);
    assert!(app.auto_fetch.items[&target.key].authentication);
    app.auto_fetch
        .apply_observation(&observation, now + Duration::from_secs(3));
    assert!(!app.auto_fetch.items[&target.key].authentication);
    let item = app.auto_fetch.items.get_mut(&target.key).unwrap();
    item.failed("Network unavailable".into(), now + Duration::from_secs(4));
    assert!(!item.due(now + Duration::from_secs(63), &FetchConfig::default()));
    assert!(item.due(now + Duration::from_secs(304), &FetchConfig::default()));
}

#[test]
fn manual_success_does_not_mark_a_later_refspec_configuration_fresh() {
    let (directory, mut app) = fixture();
    let repo = app.ctx.repos[0].path.clone();
    remote(directory.path(), &repo);
    let initial = install(&mut app);
    app.auto_fetch.observe(
        Observation {
            path: initial.path.clone(),
            remote: initial.key.remote.clone(),
            endpoint: initial.endpoint.clone(),
            branch: None,
            tip: None,
        },
        Instant::now(),
    );
    app.auto_fetch.install(Discovery {
        inputs: app.auto_fetch.inputs.clone(),
        demanded: true,
        roots: vec![(repo.clone(), Ok(vec![initial]))],
    });
    assert!(app.auto_fetch.observations.is_empty());
    let previous = app.auto_fetch.items.values().next().unwrap().target.clone();
    let _sender = hold(&mut app, previous);
    git(
        &repo,
        &[
            "config",
            "--replace-all",
            "remote.origin.fetch",
            "+refs/heads/main:refs/remotes/origin/reviewed",
        ],
    )
    .unwrap();
    let changed = target(&app);
    app.auto_fetch.install(Discovery {
        inputs: app.auto_fetch.inputs.clone(),
        demanded: true,
        roots: vec![(repo, Ok(vec![changed.clone()]))],
    });
    assert!(app.auto_fetch.items[&changed.key].success.is_none());
    assert!(app.auto_fetch.running[0].control.cancelled());
}

#[test]
fn foreground_input_waits_for_cancelled_fetch_to_exit() {
    let (directory, mut app) = fixture();
    remote(directory.path(), &app.ctx.repos[0].path);
    let target = install(&mut app);
    let sender = hold(&mut app, target);
    let before = snapshot(&app.ctx.repos[0].path);
    app.panel_event(&key(KeyCode::Char('c')));
    assert!(!app.commit.editing());
    assert!(app.auto_fetch.running[0].control.cancelled());
    app.poll_auto_fetch();
    assert!(!app.commit.editing());
    sender.send(Err("Cancelled".into())).unwrap();
    app.poll_auto_fetch();
    assert!(app.commit.editing());
    assert_eq!(snapshot(&app.ctx.repos[0].path), before);
}

#[test]
fn quitting_never_waits_for_background_fetch() {
    let (directory, mut app) = fixture();
    remote(directory.path(), &app.ctx.repos[0].path);
    let target = install(&mut app);
    let _sender = hold(&mut app, target);
    app.request_quit();
    assert!(app.quit);
    assert!(app.auto_fetch.running[0].control.cancelled());
}

#[test]
fn scope_changes_discard_late_results_and_disabled_mode_never_launches() {
    let (directory, mut app) = fixture();
    let repo = app.ctx.repos[0].path.clone();
    remote(directory.path(), &repo);
    let target = install(&mut app);
    let sender = hold(&mut app, target);
    app.ctx.scope.clear();
    app.poll_auto_fetch();
    sender
        .send(Ok(Outcome {
            changed: true,
            tip: None,
        }))
        .unwrap();
    app.poll_auto_fetch();
    assert!(app.auto_fetch.eligible.is_empty());
    assert!(!app.auto_fetch.running());
    app.ctx.scope.insert(0);
    app.config.fetch.enabled = false;
    app.auto_fetch.activate(Instant::now());
    app.auto_fetch.startup = Some(Instant::now());
    app.poll_auto_fetch();
    assert!(!app.auto_fetch.discovery.pending());
    assert_eq!(app.auto_fetch.footer(), "Auto-fetch off");
    assert!(git(&repo, &["rev-parse", "refs/remotes/origin/main"]).is_err());
}

#[test]
fn excluded_roots_are_never_fetched() {
    let (directory, app) = fixture();
    let shown = app.ctx.repos[0].path.clone();
    let remote = remote(directory.path(), &shown);
    let hidden = directory.path().join("hidden");
    git(
        directory.path(),
        &[
            "clone",
            "-q",
            remote.to_str().unwrap(),
            hidden.to_str().unwrap(),
        ],
    )
    .unwrap();
    let before = git(&hidden, &["rev-parse", "refs/remotes/origin/main"]).unwrap();
    let tip = advanced_tip(&shown, &remote);
    let mut context = Context::discover(directory.path()).unwrap();
    context.state_dir = directory.path().join("state");
    context
        .scope
        .retain(|root| context.repos[*root].path == shown);
    assert_eq!(context.scope.len(), 1);
    let mut app = Shell::new(context, IconMode::Ascii);
    app.auto_fetch.activate(Instant::now());
    app.auto_fetch.startup = Some(Instant::now());
    wait(&mut app, |app| {
        app.auto_fetch
            .items
            .values()
            .any(|item| item.success.is_some())
    });
    assert_eq!(
        git(&shown, &["rev-parse", "refs/remotes/origin/main"]).unwrap(),
        tip
    );
    assert_eq!(
        git(&hidden, &["rev-parse", "refs/remotes/origin/main"]).unwrap(),
        before
    );
    assert!(app
        .auto_fetch
        .items
        .values()
        .all(|item| item.roots.len() == 1));
}

#[test]
fn linked_worktrees_share_one_ordinary_fetch_job() {
    let (directory, app) = fixture();
    let repo = app.ctx.repos[0].path.clone();
    remote(directory.path(), &repo);
    let linked = directory.path().join("linked");
    git(
        &repo,
        &["worktree", "add", "-qb", "linked", linked.to_str().unwrap()],
    )
    .unwrap();
    let mut context = Context::discover(directory.path()).unwrap();
    context.state_dir = directory.path().join("state");
    let mut app = Shell::new(context, IconMode::Ascii);
    app.auto_fetch.activate(Instant::now());
    app.auto_fetch.startup = Some(Instant::now());
    wait(&mut app, |app| {
        app.auto_fetch
            .items
            .values()
            .any(|item| item.success.is_some())
    });
    assert_eq!(app.auto_fetch.items.len(), 1);
    assert_eq!(app.auto_fetch.items.values().next().unwrap().roots.len(), 2);
}

#[test]
fn independent_fetches_are_staggered_and_limited_to_two_workers() {
    let (directory, app) = fixture();
    let repo = app.ctx.repos[0].path.clone();
    remote(directory.path(), &repo);
    let mut paths = vec![repo.clone()];
    for name in ["second", "third"] {
        let path = directory.path().join(name);
        git(
            directory.path(),
            &[
                "clone",
                "-q",
                repo.to_str().unwrap(),
                path.to_str().unwrap(),
            ],
        )
        .unwrap();
        paths.push(path.canonicalize().unwrap());
    }
    let inputs = Inputs {
        workspace: directory.path().into(),
        roots: paths
            .iter()
            .map(|path| Root {
                path: path.clone(),
                branch: "main".into(),
                push: Vec::new(),
            })
            .collect(),
    };
    let roots = paths
        .iter()
        .map(|path| {
            (
                path.clone(),
                backend::discover(path, false, &[], &[]).map_err(|error| error.to_string()),
            )
        })
        .collect();
    let mut state = State::default();
    state.sync(inputs.clone());
    state.install(Discovery {
        inputs,
        demanded: true,
        roots,
    });
    let now = Instant::now();
    state.launch(&app.config.fetch, now);
    assert_eq!(state.running.len(), 1);
    state.launch(&app.config.fetch, now + STAGGER / 2);
    assert_eq!(state.running.len(), 1);
    state.launch(&app.config.fetch, now + STAGGER);
    assert_eq!(state.running.len(), 2);
    state.launch(&app.config.fetch, now + STAGGER * 2);
    assert_eq!(state.running.len(), 2);
    let deadline = Instant::now() + Duration::from_secs(10);
    while state.running() {
        state.collect(Instant::now());
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(
        state
            .items
            .values()
            .filter(|item| item.success.is_some())
            .count(),
        2
    );
}

#[test]
fn push_receives_automatic_tips_from_its_actual_endpoint() {
    let (directory, mut app) = fixture();
    let repo = app.ctx.repos[0].path.clone();
    remote(directory.path(), &repo);
    let fetch_tip = git(&repo, &["rev-parse", "HEAD"]).unwrap();
    let push_remote = directory.path().join("push.git");
    git(
        directory.path(),
        &[
            "clone",
            "-q",
            "--bare",
            repo.to_str().unwrap(),
            push_remote.to_str().unwrap(),
        ],
    )
    .unwrap();
    advanced_tip(&repo, &push_remote);
    git(
        &repo,
        &[
            "remote",
            "set-url",
            "--push",
            "origin",
            push_remote.to_str().unwrap(),
        ],
    )
    .unwrap();
    let before = snapshot(&repo);
    app.go(Push);
    app.push.finish_loading(&app.ctx);
    app.auto_fetch.activate(Instant::now());
    app.auto_fetch.startup = Some(Instant::now());
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        app.poll();
        if app.auto_fetch.items.len() == 2
            && app
                .auto_fetch
                .items
                .values()
                .all(|item| item.success.is_some())
            && !app.push.reads_pending()
            && !app.refresh_state.pending(Push)
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "Push target did not become fresh"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(
        git(&repo, &["rev-parse", "refs/remotes/origin/main"]).unwrap(),
        fetch_tip
    );
    assert!(screen(&mut app, 120, 36).contains("v1"));
    assert_eq!(snapshot(&repo), before);
}
