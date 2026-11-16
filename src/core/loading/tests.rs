use super::*;
use std::sync::mpsc;
use std::time::{Duration, Instant};

fn finish<T: Send + 'static>(load: &mut Load<T>) -> Result<T> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(value) = load.take_result() {
            return value;
        }
        assert!(Instant::now() < deadline, "background read did not finish");
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn newer_requests_cancel_old_results_and_coalesce_without_extra_workers() {
    let mut load = Load::default();
    let (release, wait) = mpsc::channel();
    load.request();
    load.start("igit-test-read", move |version| {
        wait.recv().unwrap();
        assert!(version.cancelled());
        "old"
    })
    .unwrap();
    for _ in 0..10 {
        load.request();
        load.start("igit-test-read", |_| {
            panic!("a second reader started before the first finished")
        })
        .unwrap();
    }
    assert!(load.loading());
    assert!(load.take_result().is_none());
    release.send(()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while load.running.is_some() {
        assert!(load.take_result().is_none(), "stale result escaped");
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
    load.start("igit-test-read", |_| "latest").unwrap();
    assert_eq!(finish(&mut load).unwrap(), "latest");
    assert!(!load.pending());
}

#[test]
fn cancelled_disconnection_is_ignored_and_current_failure_can_retry() {
    let mut load: Load<()> = Load::default();
    let (sender, receiver) = mpsc::channel();
    load.running = Some((0, receiver));
    load.cancel();
    drop(sender);
    assert!(load.take_result().is_none());
    assert!(!load.pending());

    let (sender, receiver) = mpsc::channel();
    load.running = Some((load.generation.load(Ordering::Relaxed), receiver));
    drop(sender);
    assert!(load.take_result().unwrap().is_err());
    load.request();
    load.start("igit-test-read", |_| ()).unwrap();
    finish(&mut load).unwrap();
    assert!(!load.pending());
}

#[cfg(unix)]
mod processes {
    use super::*;
    use crate::core::{command, read_output};
    use std::{fs, path::Path, process::Command};

    fn hung_read(directory: &Path) -> Command {
        let mut command = Command::new("sh");
        command
            .args([
                "-c",
                "printf started > \"$1/started\"; sleep 30",
                "igit-load-process-test",
            ])
            .arg(directory);
        command
    }

    fn wait_started(directory: &Path) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !directory.join("started").exists() {
            assert!(
                Instant::now() < deadline,
                "The read subprocess never started"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn superseded_hung_read_is_reclaimed_before_a_fresh_request_runs() {
        let directory = tempfile::tempdir().unwrap();
        let repo = directory.path().to_path_buf();
        let mut load = Load::default();
        load.request();
        load.start("igit-test-hung-read", move |_| {
            read_output(&repo, "old history", &mut hung_read(&repo))
                .unwrap_err()
                .to_string()
        })
        .unwrap();
        wait_started(directory.path());
        let started = Instant::now();
        load.request();
        while !load.needs_start() {
            assert!(load.take_result().is_none(), "A superseded result escaped");
            assert!(started.elapsed() < Duration::from_secs(3));
            std::thread::sleep(Duration::from_millis(1));
        }
        let repo = directory.path().to_path_buf();
        load.start("igit-test-fresh-read", move |_| {
            let output = read_output(&repo, "version", command(&repo).arg("--version")).unwrap();
            assert!(output.status.success());
            String::from_utf8(output.stdout).unwrap()
        })
        .unwrap();
        assert!(finish(&mut load).unwrap().starts_with("git version "));
        assert!(!load.pending());
    }

    #[test]
    fn cancelling_a_load_stops_all_repository_worker_subprocesses() {
        let directory = tempfile::tempdir().unwrap();
        let jobs: Vec<_> = (0..4)
            .map(|index| {
                let repo = directory.path().join(index.to_string());
                fs::create_dir(&repo).unwrap();
                repo
            })
            .collect();
        let worker_jobs = jobs.clone();
        let (completed, results) = mpsc::channel();
        let mut load = Load::default();
        load.request();
        load.start("igit-test-parallel-reads", move |version| {
            let errors = read_repositories(&worker_jobs, &version, |repo| {
                read_output(repo, "parallel history", &mut hung_read(repo))
                    .unwrap_err()
                    .to_string()
            });
            completed.send(errors).unwrap();
        })
        .unwrap();
        for repo in &jobs {
            wait_started(repo);
        }
        let started = Instant::now();
        load.cancel();
        let errors = results.recv_timeout(Duration::from_secs(3)).unwrap();
        assert_eq!(errors.len(), 4);
        assert!(errors.iter().all(|error| error.contains("cancelled")));
        assert!(started.elapsed() < Duration::from_secs(3));
        let deadline = Instant::now() + Duration::from_secs(3);
        while load.pending() {
            assert!(load.take_result().is_none(), "A cancelled result escaped");
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    #[test]
    fn dropping_a_load_terminates_its_active_read_subprocess() {
        let directory = tempfile::tempdir().unwrap();
        let repo = directory.path().to_path_buf();
        let (completed, result) = mpsc::channel();
        let mut load = Load::default();
        load.request();
        load.start("igit-test-drop-read", move |_| {
            let error = read_output(&repo, "status", &mut hung_read(&repo)).unwrap_err();
            completed.send(error.to_string()).unwrap();
        })
        .unwrap();
        wait_started(directory.path());
        drop(load);
        assert!(result
            .recv_timeout(Duration::from_secs(3))
            .unwrap()
            .contains("cancelled"));
    }

    #[test]
    fn custom_workers_can_inherit_a_version_read_control() {
        let directory = tempfile::tempdir().unwrap();
        let repo = directory.path().to_path_buf();
        let (completed, result) = mpsc::channel();
        let mut load = Load::default();
        load.request();
        load.start("igit-test-custom-read", move |version| {
            let inherited = version.clone();
            std::thread::spawn(move || {
                inherited.with_read_control(|| {
                    let error =
                        read_output(&repo, "custom history", &mut hung_read(&repo)).unwrap_err();
                    completed.send(error.to_string()).unwrap();
                });
            })
            .join()
            .unwrap();
        })
        .unwrap();
        wait_started(directory.path());
        load.cancel();
        assert!(result
            .recv_timeout(Duration::from_secs(3))
            .unwrap()
            .contains("cancelled"));
    }
}
