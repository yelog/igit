use super::*;
use notify::event::{AccessKind, AccessMode, DataChange, Flag, ModifyKind, RenameMode};

fn paths() -> Paths {
    let root = |worktree: &str, git: &str, common: &str, parent| Root {
        worktree: worktree.into(),
        git: git.into(),
        common: common.into(),
        parent,
    };
    Paths {
        roots: vec![
            root("/ws/app", "/ws/app/.git", "/ws/app/.git", None),
            root(
                "/ws/app/lib",
                "/ws/app/.git/modules/lib",
                "/ws/app/.git/modules/lib",
                Some(0),
            ),
            root("/ws/api", "/ws/api/.git", "/ws/api/.git", None),
            root(
                "/ws/linked",
                "/ws/app/.git/worktrees/linked",
                "/ws/app/.git",
                None,
            ),
        ],
        state: "/ws/app/state".into(),
    }
}

fn changed(path: &str) -> FileEvent {
    FileEvent::new(EventKind::Modify(ModifyKind::Data(DataChange::Any))).add_path(path.into())
}

#[test]
fn paths_route_worktrees_submodule_heads_and_shared_refs_without_object_noise() {
    let paths = paths();
    for (path, roots) in [
        ("/ws/app/src/file.rs", vec![0]),
        ("/ws/app/lib/file.rs", vec![1]),
        ("/ws/api/file.rs", vec![2]),
        ("/ws/linked/file.rs", vec![3]),
        ("/ws/app/.git/index", vec![0]),
        ("/ws/app/.git/HEAD", vec![0]),
        ("/ws/app/.git/worktrees/linked/index", vec![3]),
        ("/ws/app/.git/modules/lib/index", vec![1]),
        ("/ws/app/.git/modules/lib/HEAD", vec![0, 1]),
        ("/ws/app/.git/modules/lib/refs/heads/main", vec![0, 1]),
        ("/ws/app/.git/refs/heads/main", vec![0, 3]),
        ("/ws/app/.git/reftable/tables.list", vec![0, 3]),
        ("/ws/app/.git/info/exclude", vec![0, 3]),
        ("/ws/app/.git/objects/ab/cdef", vec![]),
        ("/ws/app/.git/logs/HEAD", vec![]),
        ("/ws/app/.git/index.lock", vec![]),
        ("/ws/app/.git/refs/heads/main.lock", vec![]),
        ("/ws/app/.git/COMMIT_EDITMSG", vec![]),
        ("/ws/app/state/commit.json", vec![]),
        ("/other/file.rs", vec![]),
    ] {
        assert_eq!(
            paths.changed(&changed(path)),
            roots.into_iter().collect(),
            "{path}"
        );
    }
    let rename = FileEvent::new(EventKind::Modify(ModifyKind::Name(RenameMode::Both)))
        .add_path("/ws/app/file.rs".into())
        .add_path("/ws/api/file.rs".into());
    assert_eq!(paths.changed(&rename), [0, 2].into());
    assert_eq!(
        paths.changed(
            &FileEvent::new(EventKind::Access(AccessKind::Close(AccessMode::Read)))
                .add_path("/ws/app/src/file.rs".into())
        ),
        BTreeSet::new()
    );
    let mut overflow = FileEvent::new(EventKind::Other);
    overflow.attrs.set_flag(Flag::Rescan);
    assert_eq!(paths.changed(&overflow), [0, 1, 2, 3].into());
    // Recursive ancestors cover nested worktrees/admin directories once.
    assert_eq!(
        paths.targets().into_iter().collect::<BTreeSet<_>>(),
        ["/ws/app", "/ws/api", "/ws/linked"]
            .map(PathBuf::from)
            .into()
    );
}

#[test]
fn debounce_is_per_repository_bounded_and_keeps_hidden_or_later_events() {
    let now = Instant::now();
    let mut events = Events::default();
    events.mark([0, 1].into(), now);
    events.mark([0].into(), now + Duration::from_millis(200));
    assert!(events
        .take(&[0, 1].into(), now + Duration::from_millis(250), false)
        .is_empty());
    assert_eq!(
        events.take(&[0, 1].into(), now + Duration::from_millis(300), false),
        [1].into()
    );
    assert_eq!(
        events.take(&[0, 1].into(), now + Duration::from_millis(500), false),
        [0].into()
    );
    for n in 0..20 {
        events.mark([0, 1].into(), now + Duration::from_millis(n * 100));
    }
    assert_eq!(
        events.dirty.len(),
        2,
        "bursts store roots, not an unbounded event queue"
    );
    assert_eq!(events.take(&[0].into(), now + MAX_DELAY, false), [0].into());
    assert!(events.dirty.contains_key(&1), "hidden roots stay queued");
    events.mark([0].into(), now + MAX_DELAY);
    assert_eq!(events.take(&[1].into(), now + MAX_DELAY, true), [1].into());
    assert_eq!(
        events.take(&[0].into(), now + MAX_DELAY + DEBOUNCE, false),
        [0].into()
    );
}

#[test]
fn git_directory_resolution_handles_linked_worktrees_and_native_paths() {
    use crate::core::git;
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("repo 中文");
    let linked = dir.path().join("linked 中文");
    fs::create_dir(&repo).unwrap();
    git(&repo, &["init", "-q", "-b", "main"]).unwrap();
    git(
        &repo,
        &[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "-c",
            "core.hooksPath=/dev/null",
            "commit",
            "--no-gpg-sign",
            "--allow-empty",
            "-qm",
            "base",
        ],
    )
    .unwrap();
    git(
        &repo,
        &["worktree", "add", "--detach", linked.to_str().unwrap()],
    )
    .unwrap();
    let c = Context::discover(dir.path()).unwrap();
    let paths = Paths::new(&c);
    let a = c
        .repos
        .iter()
        .position(|r| r.path == canonical(repo.clone()))
        .unwrap();
    let b = c
        .repos
        .iter()
        .position(|r| r.path == canonical(linked.clone()))
        .unwrap();
    assert_eq!(paths.roots[a].common, paths.roots[b].common);
    assert_ne!(paths.roots[a].git, paths.roots[b].git);
    assert!(paths.roots[b].git.join("HEAD").exists());
    assert_eq!(
        paths.changed(&changed(paths.roots[b].git.join("index").to_str().unwrap())),
        [b].into()
    );
}
