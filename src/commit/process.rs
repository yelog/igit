//! Bound subprocess IO and lifetime for one background AI job.
use crate::core::process::{Registry, Running};
use anyhow::{bail, Context as _, Result};
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom, Write},
    process::{Command, Output, Stdio},
    sync::Arc,
    thread,
    time::{Duration, Instant},
};

const STDERR_LIMIT: usize = 64 * 1024;

#[derive(Clone)]
pub(super) struct Control {
    deadline: Instant,
    shared: Arc<Registry>,
}

impl Control {
    pub fn new(timeout: Duration) -> Self {
        Self {
            deadline: Instant::now() + timeout,
            shared: Arc::default(),
        }
    }

    pub fn cancel(&self) {
        self.shared.cancel();
    }

    pub fn check(&self) -> Result<()> {
        if self.shared.cancelled() {
            bail!("AI generation cancelled because the selection or Roots changed");
        }
        if Instant::now() >= self.deadline {
            bail!("AI generation exceeded ai.total_timeout_seconds; increase the limit or select fewer files");
        }
        Ok(())
    }

    pub fn remaining(&self) -> Result<Duration> {
        self.check()?;
        Ok(self.deadline.saturating_duration_since(Instant::now()))
    }

    /// File-backed streams avoid blocking on a full pipe while checking timeouts.
    /// Read at most the allowed output even if a process races the size check.
    pub fn run(
        &self,
        mut command: Command,
        input: Option<&[u8]>,
        output_limit: usize,
    ) -> Result<Output> {
        self.check()?;
        let mut stdin = tempfile::tempfile()?;
        if let Some(bytes) = input {
            stdin.write_all(bytes)?;
            stdin.rewind()?;
        }
        let mut stdout = tempfile::tempfile()?;
        let mut stderr = tempfile::tempfile()?;
        command
            .stdin(Stdio::from(stdin))
            .stdout(Stdio::from(stdout.try_clone()?))
            .stderr(Stdio::from(stderr.try_clone()?));
        let mut process = self.spawn(command)?;
        let waited = loop {
            self.check()?;
            if stdout.metadata()?.len() > output_limit as u64
                || stderr.metadata()?.len() > STDERR_LIMIT as u64
            {
                bail!("AI subprocess output exceeded the safety limit; select fewer files or check the service");
            }
            match process.child().try_wait()? {
                Some(status) => break status,
                None => thread::sleep(Duration::from_millis(10)),
            }
        };
        // Descendants must stop writing before we inspect the final output.
        drop(process);
        self.check()?;
        Ok(Output {
            status: waited,
            stdout: read(&mut stdout, output_limit)?,
            stderr: read(&mut stderr, STDERR_LIMIT)?,
        })
    }

    fn spawn(&self, mut command: Command) -> Result<Running> {
        self.check()?;
        let process = self.shared.spawn(&mut command);
        self.check()?;
        let process = process.context("Could not start or isolate the AI subprocess")?;
        let activation = process.activate();
        self.check()?;
        activation.context("Could not start the isolated AI subprocess")?;
        Ok(process)
    }
}

fn read(file: &mut File, limit: usize) -> Result<Vec<u8>> {
    file.seek(SeekFrom::Start(0))?;
    let mut bytes = Vec::new();
    file.take(limit as u64 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > limit {
        bail!("AI subprocess output exceeded the safety limit; select fewer files or check the service");
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dropping_a_control_clone_does_not_cancel_the_job() {
        let control = Control::new(Duration::from_secs(5));
        drop(control.clone());
        control.check().unwrap();
        let temp = tempfile::tempdir().unwrap();
        let mut command = crate::core::command(temp.path());
        command.args(["--version"]);
        assert!(control.run(command, None, 1024).unwrap().status.success());
        assert!(control.shared.is_empty());
    }

    #[test]
    fn cancelled_and_expired_jobs_do_not_start_processes() {
        let control = Control::new(Duration::from_secs(5));
        control.cancel();
        assert!(control
            .run(Command::new("igit-nonexistent-program"), None, 1024)
            .unwrap_err()
            .to_string()
            .contains("cancelled"));
        assert!(Control::new(Duration::ZERO)
            .remaining()
            .unwrap_err()
            .to_string()
            .contains("total_timeout_seconds"));
    }

    #[test]
    fn output_is_bounded_even_when_process_exits_before_polling() {
        let temp = tempfile::tempdir().unwrap();
        let mut command = crate::core::command(temp.path());
        command.args(["--version"]);
        let error = Control::new(Duration::from_secs(5))
            .run(command, None, 1)
            .unwrap_err();
        assert!(error.to_string().contains("safety limit"));
    }

    #[cfg(unix)]
    #[test]
    fn deadline_stops_a_running_process_and_its_children() {
        let mut command = Command::new("sh");
        command.args(["-c", "sleep 30 & wait"]);
        let start = Instant::now();
        let error = Control::new(Duration::from_millis(100))
            .run(command, None, 1024)
            .unwrap_err();
        assert!(start.elapsed() < Duration::from_secs(3));
        assert!(error.to_string().contains("total_timeout_seconds"));
    }

    #[cfg(unix)]
    mod unix {
        use super::*;
        use std::{fs, path::Path};

        const PROBE_ENV: &str = "IGIT_AI_PROCESS_EXIT_PROBE";
        const PROBE_TEST: &str = "commit::process::tests::unix::application_exit_probe";

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
                tree_command(self.path(), parent_exits)
            }

            fn wait_ids(&self) -> Vec<u32> {
                wait_ids(self.path())
            }
        }

        impl Drop for Tree {
            fn drop(&mut self) {
                // Failed assertions must not leave the disposable probes alive.
                if let Some(ids) = ids(self.path()) {
                    for id in ids {
                        signal(id, 9);
                    }
                }
            }
        }

        fn tree_command(directory: &Path, parent_exits: bool) -> Command {
            let script = if parent_exits {
                "sleep 30 & child=$!; printf '%s %s\\n' \"$$\" \"$child\" > \"$1/pids\"; exit 0"
            } else {
                "sleep 30 & child=$!; printf '%s %s\\n' \"$$\" \"$child\" > \"$1/pids\"; wait"
            };
            let mut command = Command::new("sh");
            command
                .args(["-c", script, "igit-ai-process-test"])
                .arg(directory)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null());
            command
        }

        fn ids(directory: &Path) -> Option<Vec<u32>> {
            let text = fs::read_to_string(directory.join("pids")).ok()?;
            let ids: Vec<_> = text
                .split_whitespace()
                .map(str::parse)
                .collect::<std::result::Result<_, _>>()
                .ok()?;
            (ids.len() == 2).then_some(ids)
        }

        fn wait_ids(directory: &Path) -> Vec<u32> {
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                if let Some(ids) = ids(directory) {
                    return ids;
                }
                assert!(Instant::now() < deadline, "The probe never started");
                thread::sleep(Duration::from_millis(5));
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
            // An orphaned zombie is already terminated; its reaping belongs to
            // the host init process and must not make these tests flaky.
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
                    "AI subprocesses survived cleanup: {alive:?}"
                );
                thread::sleep(Duration::from_millis(10));
            }
        }

        #[test]
        fn cancellation_synchronously_signals_every_registered_group() {
            let control = Control::new(Duration::from_secs(10));
            let first = Tree::new();
            let second = Tree::new();
            // Keep the guards on this thread without polling. A cancellation
            // that only wakes a worker cannot pass this regression.
            let first_process = control.spawn(first.command(false)).unwrap();
            let second_process = control.spawn(second.command(false)).unwrap();
            let mut ids = first.wait_ids();
            ids.extend(second.wait_ids());
            assert!(ids.iter().all(|id| running(*id)));
            let start = Instant::now();
            control.cancel();
            assert!(
                start.elapsed() < Duration::from_secs(1),
                "UI cancellation waited for process cleanup"
            );
            assert_stopped(&ids);
            control.cancel();
            drop(first_process);
            drop(second_process);
            assert!(control.shared.is_empty());
        }

        #[test]
        fn cancelling_a_running_command_stops_it_and_returns_an_error() {
            let control = Control::new(Duration::from_secs(10));
            let tree = Tree::new();
            let command = tree.command(false);
            let worker_control = control.clone();
            let worker = thread::spawn(move || worker_control.run(command, None, 1024));
            let ids = tree.wait_ids();
            control.cancel();
            assert_stopped(&ids);
            assert!(worker
                .join()
                .unwrap()
                .unwrap_err()
                .to_string()
                .contains("cancelled"));
            assert!(control.shared.is_empty());
        }

        #[test]
        fn successful_parent_exit_still_terminates_lingering_children() {
            let control = Control::new(Duration::from_secs(10));
            let tree = Tree::new();
            let output = control.run(tree.command(true), None, 1024).unwrap();
            assert!(output.status.success());
            assert_stopped(&tree.wait_ids());
            assert!(control.shared.is_empty());
        }

        #[test]
        fn output_limit_errors_also_terminate_lingering_children() {
            let control = Control::new(Duration::from_secs(10));
            let tree = Tree::new();
            let mut command = Command::new("sh");
            command
                .args([
                    "-c",
                    "sleep 30 & child=$!; printf '%s %s\\n' \"$$\" \"$child\" > \"$1/pids\"; printf overflow; wait",
                    "igit-ai-process-test",
                ])
                .arg(tree.path());
            let error = control.run(command, None, 1).unwrap_err();
            assert!(error.to_string().contains("safety limit"));
            assert_stopped(&tree.wait_ids());
            assert!(control.shared.is_empty());
        }

        #[test]
        fn unwinding_the_worker_terminates_registered_children() {
            let control = Control::new(Duration::from_secs(10));
            let tree = Tree::new();
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let _process = control.spawn(tree.command(false)).unwrap();
                tree.wait_ids();
                panic!("Disposable worker panic");
            }));
            assert!(result.is_err());
            assert_stopped(&tree.wait_ids());
            assert!(control.shared.is_empty());
            control.check().unwrap();
        }

        #[test]
        fn cancellation_and_spawn_admission_cannot_leave_live_processes() {
            for _ in 0..12 {
                let control = Control::new(Duration::from_secs(10));
                let barrier = Arc::new(std::sync::Barrier::new(2));
                let tree = Tree::new();
                let command = tree.command(false);
                let worker_control = control.clone();
                let worker_barrier = Arc::clone(&barrier);
                let worker = thread::spawn(move || {
                    worker_barrier.wait();
                    worker_control.spawn(command)
                });
                barrier.wait();
                control.cancel();
                let process = worker.join().unwrap();
                // If admission won the race, hold the guard alive while
                // checking; cleanup cannot rely on its eventual destructor.
                if let Some(ids) = ids(tree.path()) {
                    assert_stopped(&ids);
                }
                drop(process);
                assert!(control.shared.is_empty());
            }
        }

        #[test]
        fn application_exit_after_cancellation_leaves_no_children() {
            let tree = Tree::new();
            let mut probe = Command::new(std::env::current_exe().unwrap())
                .args(["--exact", PROBE_TEST, "--nocapture", "--test-threads=1"])
                .env(PROBE_ENV, tree.path())
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap();
            let deadline = Instant::now() + Duration::from_secs(10);
            let status = loop {
                if let Some(status) = probe.try_wait().unwrap() {
                    break status;
                }
                if Instant::now() >= deadline {
                    let _ = probe.kill();
                    let _ = probe.wait();
                    panic!("The disposable application exit probe hung");
                }
                thread::sleep(Duration::from_millis(10));
            };
            assert!(status.success(), "The application exit probe failed");
            assert_stopped(&tree.wait_ids());
        }

        #[test]
        fn application_exit_probe() {
            let Some(directory) = std::env::var_os(PROBE_ENV) else {
                return;
            };
            let directory = std::path::PathBuf::from(directory);
            let control = Control::new(Duration::from_secs(15));
            let worker_control = control.clone();
            thread::spawn(move || {
                // This worker deliberately does not poll after spawning. The
                // application exits without running its RAII cleanup.
                let _process = worker_control
                    .spawn(tree_command(&directory, false))
                    .unwrap();
                let ids = wait_ids(&directory);
                assert!(ids.iter().all(|id| running(*id)));
                fs::write(directory.join("ready"), b"ready").unwrap();
                loop {
                    thread::park();
                }
            });
            let directory = std::path::PathBuf::from(std::env::var_os(PROBE_ENV).unwrap());
            let deadline = Instant::now() + Duration::from_secs(5);
            while !directory.join("ready").exists() {
                assert!(Instant::now() < deadline, "The probe worker never started");
                thread::sleep(Duration::from_millis(5));
            }
            struct PendingProbe(Control);
            impl Drop for PendingProbe {
                fn drop(&mut self) {
                    self.0.cancel();
                }
            }
            // Model Pending::drop followed immediately by application exit.
            drop(PendingProbe(control));
            std::process::exit(0);
        }
    }
}
