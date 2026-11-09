use super::*;
use crate::core::git;
use std::path::PathBuf;

fn finish(panel: &mut CommitPanel, c: &Context) -> Action {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        let action = panel.poll_execution(c);
        if matches!(action, Action::Committed { .. }) {
            return action;
        }
        assert!(Instant::now() < deadline, "Commit worker did not finish");
        std::thread::yield_now();
    }
}

#[test]
fn background_commits_preserve_mixed_staging_and_shared_message_behavior() {
    let (_dir, c, mut panel) = super::super::message_tests::fixture();
    panel.enable_background_execution();
    let action = panel.submit(&c, false).unwrap();
    assert!(matches!(action, Action::Notice(_)));
    assert!(panel.execution_running());
    assert!(panel.submit(&c, false).is_err());
    let Action::Committed { repos, .. } = finish(&mut panel, &c) else {
        unreachable!()
    };
    assert_eq!(repos, [0, 1]);
    assert!(!panel.execution_running());
    assert_eq!(panel.saved.message.as_deref(), Some(""));
    for repo in &c.repos {
        assert_eq!(
            git(&repo.path, &["show", "HEAD:mixed.txt"]).unwrap(),
            format!("{} staged", repo.name)
        );
        assert_eq!(
            git(&repo.path, &["show", ":unselected.txt"]).unwrap(),
            "unselected staged marker"
        );
        assert_eq!(
            fs::read_to_string(repo.path.join("mixed.txt")).unwrap(),
            "unstaged private marker\n"
        );
    }
}

#[cfg(unix)]
#[test]
fn slow_hook_keeps_browsing_responsive_and_stop_skips_only_unstarted_repositories() {
    use std::os::unix::fs::PermissionsExt;
    let (dir, c, mut panel) = super::super::message_tests::fixture();
    let marker = dir.path().join("hook-ready");
    let release = dir.path().join("hook-release");
    struct Release(PathBuf);
    impl Drop for Release {
        fn drop(&mut self) {
            let _ = fs::write(&self.0, "go");
        }
    }
    let _release = Release(release.clone());
    let hooks = c.repos[0].path.join(".git/hooks");
    fs::create_dir_all(&hooks).unwrap();
    let hook = hooks.join("pre-commit");
    let quote = |path: &Path| path.to_string_lossy().replace('\'', "'\\''");
    fs::write(
        &hook,
        format!(
            "#!/bin/sh\nprintf ready > '{}'\nwhile [ ! -f '{}' ]; do sleep 0.01; done\n",
            quote(&marker),
            quote(&release)
        ),
    )
    .unwrap();
    fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).unwrap();
    git(
        &c.repos[0].path,
        &["config", "core.hooksPath", hooks.to_str().unwrap()],
    )
    .unwrap();
    let second_head = git(&c.repos[1].path, &["rev-parse", "HEAD"]).unwrap();
    let message = panel.saved.message.clone();
    panel.enable_background_execution();
    assert!(matches!(
        panel.submit(&c, false).unwrap(),
        Action::Notice(_)
    ));
    let deadline = Instant::now() + Duration::from_secs(10);
    while !marker.exists() {
        panel.poll_execution(&c);
        assert!(Instant::now() < deadline, "Hook did not start");
        std::thread::yield_now();
    }
    let cursor = panel.cursor;
    panel.event(
        &Event::Key(crossterm::event::KeyEvent::new(K::Down, Mod::NONE)),
        &c,
    );
    assert!(panel.cursor != cursor);
    assert!(panel.execution_running());
    let checks = panel.checks.clone();
    panel.event(
        &Event::Key(crossterm::event::KeyEvent::new(K::Char(' '), Mod::NONE)),
        &c,
    );
    assert_eq!(panel.checks, checks);
    panel.stop_execution();
    fs::write(&release, "go").unwrap();
    let Action::Committed {
        repos,
        message: result,
        ..
    } = finish(&mut panel, &c)
    else {
        unreachable!()
    };
    assert_eq!(repos, [0]);
    assert!(result.contains("batch stopped"), "{result}");
    assert_eq!(panel.saved.message, message);
    assert_eq!(
        git(&c.repos[1].path, &["rev-parse", "HEAD"]).unwrap(),
        second_head
    );
    assert_eq!(
        git(&c.repos[0].path, &["show", ":unselected.txt"]).unwrap(),
        "unselected staged marker"
    );
}
