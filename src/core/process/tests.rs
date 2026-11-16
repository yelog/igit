use super::*;
use crate::core::{command, git, git_bytes, read_operation};
use std::fs;

#[test]
fn explicit_file_input_is_preserved_while_ordinary_reads_keep_stdin_closed() {
    let directory = tempfile::tempdir().unwrap();
    git(directory.path(), &["init", "-q"]).unwrap();
    let path = directory.path().join("source");
    fs::write(&path, b"literal\0input\n").unwrap();
    let expected = git_bytes(directory.path(), &["hash-object", "source"]).unwrap();
    let supplied = read_output_with_input(
        directory.path(),
        "hash-object",
        command(directory.path()).args(["hash-object", "--stdin"]),
        fs::File::open(&path).unwrap(),
    )
    .unwrap();
    assert!(supplied.status.success());
    assert_eq!(supplied.stdout, expected);
    let closed = read_output(
        directory.path(),
        "hash-object",
        command(directory.path()).args(["hash-object", "--stdin"]),
    )
    .unwrap();
    assert!(closed.status.success());
    assert_ne!(closed.stdout, expected);
    let expired = ReadControl::new(Duration::ZERO);
    let failure = with_read_control(&expired, || {
        read_output_with_input(
            directory.path(),
            "hash-object",
            command(directory.path()).args(["hash-object", "--stdin"]),
            fs::File::open(&path).unwrap(),
        )
    })
    .unwrap_err();
    assert!(failure.to_string().contains("timed out"));
}

#[test]
fn cancelled_and_expired_reads_do_not_start_or_expose_arguments() {
    let directory = tempfile::tempdir().unwrap();
    let secret = "https://user:private-token@example.invalid/private?credential=hidden";
    for (control, expected) in [
        (ReadControl::new(Duration::ZERO), "timed out"),
        (ReadControl::default(), "cancelled"),
    ] {
        if expected == "cancelled" {
            control.cancel();
        }
        let mut command = Command::new("igit-nonexistent-read-program");
        command.arg(secret);
        let error = with_read_control(&control, || {
            read_output(directory.path(), "revision", &mut command)
        })
        .unwrap_err();
        let message = format!("{error:#}");
        assert!(message.contains(expected), "{message}");
        assert!(message.contains("revision"), "{message}");
        assert!(message.contains(&directory.path().display().to_string()));
        assert!(message.contains("after "), "{message}");
        assert!(!message.contains("private-token"), "{message}");
        assert!(!message.contains("credential"), "{message}");
        assert!(!message.contains("igit-nonexistent"), "{message}");
    }
}

#[test]
fn failed_git_reads_do_not_report_argument_or_stderr_secrets() {
    let directory = tempfile::tempdir().unwrap();
    git(directory.path(), &["init", "-q"]).unwrap();
    let error = git_bytes(
        directory.path(),
        &[
            "rev-parse",
            "--verify",
            "https://user:private-token@example.invalid/private?credential=hidden",
        ],
    )
    .unwrap_err();
    let message = format!("{error:#}");
    assert!(message.contains("rev-parse"), "{message}");
    assert!(message.contains("after "), "{message}");
    assert!(!message.contains("private-token"), "{message}");
    assert!(!message.contains("credential"), "{message}");
}

#[test]
fn read_controls_restore_after_nested_calls_and_unwinding() {
    let outer = ReadControl::default();
    let inner = ReadControl::default();
    inner.cancel();
    with_read_control(&outer, || {
        check_read_control().unwrap();
        assert!(with_read_control(&inner, check_read_control).is_err());
        check_read_control().unwrap();
        assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            with_read_control(&inner, || panic!("Disposable read worker panic"));
        }))
        .is_err());
        check_read_control().unwrap();
        outer.cancel();
        assert!(check_read_control().is_err());
    });
    check_read_control().unwrap();
}

#[test]
fn dropping_a_read_control_clone_does_not_cancel_the_request() {
    let control = ReadControl::default();
    drop(control.clone());
    let directory = tempfile::tempdir().unwrap();
    let output = with_read_control(&control, || {
        read_output(
            directory.path(),
            "version",
            command(directory.path()).arg("--version"),
        )
    })
    .unwrap();
    assert!(output.status.success());
    assert!(control.registry.is_empty());
}

#[test]
fn repository_discovery_propagates_read_cancellation_and_deadlines() {
    let directory = tempfile::tempdir().unwrap();
    let cancelled = ReadControl::default();
    cancelled.cancel();
    let error = with_read_control(&cancelled, || {
        crate::core::Context::discover(directory.path())
    })
    .unwrap_err();
    assert!(error.to_string().contains("cancelled"));
    let expired = ReadControl::new(Duration::ZERO);
    let error = with_read_control(&expired, || {
        crate::core::Context::discover(directory.path())
    })
    .unwrap_err();
    assert!(error.to_string().contains("timed out"));
}

#[test]
fn only_known_read_forms_receive_automatic_cancellation() {
    let reads: &[&[&str]] = &[
        &["--version"],
        &["--exec-path"],
        &["--literal-pathspecs", "diff", "--cached", "--", "file"],
        &["--no-optional-locks", "status", "--porcelain=v1", "-z"],
        &["-c", "color.ui=false", "rev-list", "--all"],
        &["-ccolor.ui=false", "log", "-1"],
        &["symbolic-ref", "-q", "HEAD"],
        &["config", "--bool", "--get", "core.filemode"],
        &["config", "--type=bool", "--get", "core.filemode"],
        &["config", "--get-all", "remote.origin.url"],
        &["config", "--null", "--show-origin", "--list"],
        &["config", "--file", "config", "user.name"],
        &["config", "get", "--all", "user.name"],
        &["remote"],
        &["remote", "get-url", "--push", "--all", "origin"],
        &["hash-object", "-t", "tree", "--stdin"],
        &["stash", "list", "--format=%H"],
        &["stash", "show", "--stat"],
        &["reflog", "show", "HEAD"],
    ];
    let writes: &[&[&str]] = &[
        &["add", "--", "file"],
        &["commit", "-qm", "message"],
        &["push", "origin", "HEAD"],
        &["fetch", "origin"],
        &["update-ref", "refs/heads/topic", "HEAD"],
        &["read-tree", "HEAD"],
        &["write-tree"],
        &["hash-object", "-w", "--stdin"],
        &["hash-object", "-wt", "blob", "--stdin"],
        &["symbolic-ref", "HEAD", "refs/heads/topic"],
        &["symbolic-ref", "--delete", "refs/test"],
        &["config", "user.name", "name"],
        &["config", "--replace-all", "user.name", "name"],
        &["config", "--get", "--unset", "user.name"],
        &["config", "set", "user.name", "name"],
        &["config", "--local", "--edit"],
        &["config", "--rename-section", "old", "new"],
        &["remote", "add", "origin", "../remote"],
        &["remote", "update"],
        &["branch", "topic"],
        &["tag", "release"],
        &["stash", "push"],
        &["reflog", "expire", "--expire=now", "--all"],
        &["diff", "--output=patch"],
        &["log", "--output", "history"],
        &["-c", "alias.unknown=status", "unknown"],
    ];
    for args in reads {
        assert!(read_operation(args).is_some(), "{args:?}");
    }
    for args in writes {
        assert!(read_operation(args).is_none(), "{args:?}");
    }
}

#[test]
fn cancelled_read_scope_does_not_cancel_staging_or_config_writes() {
    let directory = tempfile::tempdir().unwrap();
    let repo = directory.path();
    git(repo, &["init", "-q"]).unwrap();
    fs::write(repo.join("selected"), "staged selected\n").unwrap();
    fs::write(repo.join("sibling"), "staged sibling\n").unwrap();
    git(repo, &["add", "--", "selected", "sibling"]).unwrap();
    fs::write(repo.join("selected"), "working selected\n").unwrap();
    fs::write(repo.join("sibling"), "working sibling\n").unwrap();
    let index = fs::read(repo.join(".git/index")).unwrap();
    let control = ReadControl::default();
    control.cancel();
    with_read_control(&control, || {
        assert!(git_bytes(repo, &["ls-files", "--stage", "-z"]).is_err());
        assert_eq!(fs::read(repo.join(".git/index")).unwrap(), index);
        git(repo, &["add", "--", "selected"]).unwrap();
        git(repo, &["config", "igit.readControlTest", "write completed"]).unwrap();
    });
    assert_eq!(
        git(repo, &["show", ":selected"]).unwrap(),
        "working selected"
    );
    assert_eq!(git(repo, &["show", ":sibling"]).unwrap(), "staged sibling");
    assert_eq!(
        git(repo, &["config", "--get", "igit.readControlTest"]).unwrap(),
        "write completed"
    );
    assert_eq!(
        fs::read(repo.join("sibling")).unwrap(),
        b"working sibling\n"
    );
}

#[cfg(unix)]
mod unix {
    use super::*;

    struct Tree {
        directory: tempfile::TempDir,
    }

    impl Tree {
        fn new() -> Self {
            Self {
                directory: tempfile::tempdir().unwrap(),
            }
        }

        fn path(&self) -> &Path {
            self.directory.path()
        }

        fn command(&self, parent_exits: bool) -> Command {
            let script = if parent_exits {
                "sleep 30 & child=$!; printf '%s %s\\n' \"$$\" \"$child\" > \"$1/pids\"; printf ready; exit 0"
            } else {
                "sleep 30 & child=$!; printf '%s %s\\n' \"$$\" \"$child\" > \"$1/pids\"; printf '%s' \"$2\" >&2; wait"
            };
            let mut command = Command::new("sh");
            command
                .args(["-c", script, "igit-read-process-test"])
                .arg(self.path())
                .arg("https://user:private-token@example.invalid/private?credential=hidden")
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null());
            command
        }

        fn ids(&self) -> Option<Vec<u32>> {
            let text = fs::read_to_string(self.path().join("pids")).ok()?;
            let ids: Vec<_> = text
                .split_whitespace()
                .map(str::parse)
                .collect::<std::result::Result<_, _>>()
                .ok()?;
            (ids.len() == 2).then_some(ids)
        }

        fn wait_ids(&self) -> Vec<u32> {
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                if let Some(ids) = self.ids() {
                    return ids;
                }
                assert!(Instant::now() < deadline, "The read probe never started");
                thread::sleep(Duration::from_millis(5));
            }
        }
    }

    impl Drop for Tree {
        fn drop(&mut self) {
            if let Some(ids) = self.ids() {
                for id in ids {
                    signal(id, 9);
                }
            }
        }
    }

    fn signal(id: u32, signal: i32) -> i32 {
        unsafe extern "C" {
            fn kill(pid: i32, signal: i32) -> i32;
        }
        unsafe { kill(id as i32, signal) }
    }

    fn running(id: u32) -> bool {
        if signal(id, 0) != 0 {
            return false;
        }
        let output = Command::new("ps")
            .args(["-o", "stat=", "-p", &id.to_string()])
            .output()
            .unwrap();
        output.status.success()
            && String::from_utf8_lossy(&output.stdout)
                .split_whitespace()
                .any(|state| !state.starts_with('Z'))
    }

    fn assert_stopped(ids: &[u32]) {
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            let alive: Vec<_> = ids.iter().copied().filter(|id| running(*id)).collect();
            if alive.is_empty() {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "Read subprocesses survived cleanup: {alive:?}"
            );
            thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn read_deadline_terminates_a_running_parent_and_descendants() {
        let tree = Tree::new();
        let control = ReadControl::new(Duration::from_millis(300));
        let started = Instant::now();
        let error = with_read_control(&control, || {
            read_output(tree.path(), "history", &mut tree.command(false))
        })
        .unwrap_err();
        assert!(started.elapsed() < Duration::from_secs(3));
        assert!(error.to_string().contains("timed out"));
        assert!(!format!("{error:#}").contains("private-token"));
        assert_stopped(&tree.wait_ids());
        assert!(control.registry.is_empty());
    }

    #[test]
    fn cancellation_signals_all_registered_reads_without_worker_polling() {
        let control = ReadControl::default();
        let first = Tree::new();
        let second = Tree::new();
        let first_process = control.registry.spawn(&mut first.command(false)).unwrap();
        let second_process = control.registry.spawn(&mut second.command(false)).unwrap();
        let mut ids = first.wait_ids();
        ids.extend(second.wait_ids());
        let started = Instant::now();
        control.cancel();
        assert!(started.elapsed() < Duration::from_secs(1));
        assert_stopped(&ids);
        control.cancel();
        drop(first_process);
        drop(second_process);
        assert!(control.registry.is_empty());
    }

    #[test]
    fn cancelling_a_running_read_stops_descendants_and_returns_a_safe_error() {
        let tree = Tree::new();
        let repo = tree.path().to_path_buf();
        let mut command = tree.command(false);
        let control = ReadControl::default();
        let worker_control = control.clone();
        let worker = thread::spawn(move || {
            with_read_control(&worker_control, || {
                read_output(&repo, "status", &mut command)
            })
        });
        let ids = tree.wait_ids();
        control.cancel();
        assert_stopped(&ids);
        let error = worker.join().unwrap().unwrap_err();
        let message = format!("{error:#}");
        assert!(message.contains("cancelled"), "{message}");
        assert!(message.contains("status"), "{message}");
        assert!(message.contains("after "), "{message}");
        assert!(!message.contains("private-token"), "{message}");
        assert!(control.registry.is_empty());
    }

    #[test]
    fn a_successful_read_still_stops_a_descendant_after_parent_exit() {
        let tree = Tree::new();
        let output = read_output(tree.path(), "history", &mut tree.command(true)).unwrap();
        assert!(output.status.success());
        assert_eq!(output.stdout, b"ready");
        assert_stopped(&tree.wait_ids());
    }

    #[test]
    fn oversized_output_is_rejected_even_after_a_fast_parent_exit() {
        let directory = tempfile::tempdir().unwrap();
        for script in [
            "dd if=/dev/zero bs=1024 count=16385 2>/dev/null",
            "dd if=/dev/zero bs=1024 count=65 >&2 2>/dev/null",
        ] {
            let mut command = Command::new("sh");
            command.args(["-c", script]);
            let error = read_output(directory.path(), "patch", &mut command).unwrap_err();
            assert!(error.to_string().contains("output safety limit"));
        }
    }

    #[test]
    fn prefix_truncation_preserves_preview_bytes_and_stops_descendants() {
        let tree = Tree::new();
        let mut command = Command::new("sh");
        command
            .args([
                "-c",
                "sleep 30 & child=$!; printf '%s %s\\n' \"$$\" \"$child\" > \"$1/pids\"; printf overflow; wait",
                "igit-read-process-test",
            ])
            .arg(tree.path());
        let started = Instant::now();
        let (bytes, cut) = read_prefix(tree.path(), "diff", &mut command, 4).unwrap();
        assert_eq!(bytes, b"over");
        assert!(cut);
        assert!(started.elapsed() < Duration::from_secs(3));
        assert_stopped(&tree.wait_ids());
    }

    #[test]
    fn output_limit_failure_terminates_descendants() {
        let tree = Tree::new();
        let mut command = Command::new("sh");
        command
            .args([
                "-c",
                "sleep 30 & child=$!; printf '%s %s\\n' \"$$\" \"$child\" > \"$1/pids\"; printf overflow; wait",
                "igit-read-process-test",
            ])
            .arg(tree.path());
        let control = ReadControl::default();
        assert!(matches!(
            control.capture(&mut command, Instant::now(), 4, false),
            Err(Failure::Limit)
        ));
        assert_stopped(&tree.wait_ids());
    }

    #[test]
    fn cancellation_racing_spawn_admission_cannot_leave_live_children() {
        for _ in 0..8 {
            let control = ReadControl::default();
            let tree = Tree::new();
            let repo = tree.path().to_path_buf();
            let mut command = tree.command(false);
            let barrier = Arc::new(std::sync::Barrier::new(2));
            let worker_control = control.clone();
            let worker_barrier = barrier.clone();
            let worker = thread::spawn(move || {
                worker_barrier.wait();
                with_read_control(&worker_control, || {
                    read_output(&repo, "history", &mut command)
                })
            });
            barrier.wait();
            control.cancel();
            assert!(worker
                .join()
                .unwrap()
                .unwrap_err()
                .to_string()
                .contains("cancelled"));
            if let Some(ids) = tree.ids() {
                assert_stopped(&ids);
            }
            assert!(control.registry.is_empty());
        }
    }
}
