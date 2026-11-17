use super::*;

#[derive(Clone, Default)]
pub(crate) struct WriteControl {
    registry: Arc<Registry>,
    phase: Arc<Mutex<String>>,
}

impl WriteControl {
    pub(crate) fn interrupt(&self) {
        self.registry.cancel();
    }

    pub(crate) fn phase(&self) -> String {
        self.phase
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone()
    }
}

thread_local! {
    static WRITE_CONTROL: RefCell<Option<WriteControl>> = const { RefCell::new(None) };
}

pub(crate) fn with_write_control<T>(control: &WriteControl, run: impl FnOnce() -> T) -> T {
    struct Restore(Option<WriteControl>);
    impl Drop for Restore {
        fn drop(&mut self) {
            WRITE_CONTROL.with(|slot| {
                slot.replace(self.0.take());
            });
        }
    }
    let _restore = Restore(WRITE_CONTROL.with(|slot| slot.replace(Some(control.clone()))));
    let read_control = ReadControl {
        registry: control.registry.clone(),
        ..ReadControl::default()
    };
    with_read_control(&read_control, run)
}

fn drain(mut source: impl Read, limit: usize) -> io::Result<(Vec<u8>, bool)> {
    let mut captured = Vec::new();
    let mut truncated = false;
    let mut buffer = [0; 8192];
    loop {
        let count = source.read(&mut buffer)?;
        if count == 0 {
            return Ok((captured, truncated));
        }
        let keep = count.min(limit.saturating_sub(captured.len()));
        captured.extend_from_slice(&buffer[..keep]);
        truncated |= keep < count;
    }
}

pub(crate) fn write_output(repo: &Path, operation: &str, command: &mut Command) -> Result<Output> {
    write_output_with_input(repo, operation, command, None)
}

pub(crate) fn start_write(
    repo: &Path,
    operation: &str,
    command: &mut Command,
) -> Result<(Running, Option<crate::core::activity::Pending>)> {
    let control = WRITE_CONTROL.with(|slot| slot.borrow().clone().unwrap_or_default());
    *control
        .phase
        .lock()
        .unwrap_or_else(|error| error.into_inner()) = format!(
        "Git {operation} · {}",
        repo.file_name().unwrap_or_default().to_string_lossy()
    );
    anyhow::ensure!(
        !control.registry.cancelled(),
        "Operation interrupted; inspect Git state before retrying"
    );
    let activity = crate::core::activity::Pending::start(repo, operation)?;
    let process = control.registry.spawn(command)?;
    process.activate()?;
    Ok((process, activity))
}

pub(crate) fn write_output_with_input(
    repo: &Path,
    operation: &str,
    command: &mut Command,
    input: Option<File>,
) -> Result<Output> {
    let control = WRITE_CONTROL.with(|slot| slot.borrow().clone().unwrap_or_default());
    *control
        .phase
        .lock()
        .unwrap_or_else(|error| error.into_inner()) = format!(
        "Git {operation} · {}",
        repo.file_name().unwrap_or_default().to_string_lossy()
    );
    anyhow::ensure!(
        !control.registry.cancelled(),
        "Operation interrupted; inspect Git state before retrying"
    );
    let activity = crate::core::activity::Pending::start(repo, operation)?;
    command
        .stdin(input.map_or_else(Stdio::null, Stdio::from))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut process = control.registry.spawn(command)?;
    let stdout = process.child().stdout.take().expect("Piped stdout");
    let stderr = process.child().stderr.take().expect("Piped stderr");
    let output = thread::scope(|scope| -> Result<Output> {
        let stdout = scope.spawn(move || drain(stdout, OUTPUT_LIMIT));
        let stderr = scope.spawn(move || drain(stderr, STDERR_LIMIT));
        if let Err(error) = process.activate() {
            drop(process);
            return Err(error.into());
        }
        let status = process.child().wait();
        drop(process);
        let (stdout, stdout_cut) = stdout
            .join()
            .map_err(|_| anyhow::anyhow!("Output reader stopped"))??;
        let (mut stderr, stderr_cut) = stderr
            .join()
            .map_err(|_| anyhow::anyhow!("Error reader stopped"))??;
        if stdout_cut || stderr_cut {
            stderr.extend_from_slice(
                b"\nIGit: command output truncated; inspect the repository for full details.\n",
            );
        }
        Ok(Output {
            status: status?,
            stdout,
            stderr,
        })
    })?;
    if let Some(activity) = activity {
        activity.finish(repo, if control.registry.cancelled() { "Unknown" }
            else if output.status.success() { "Succeeded" } else { "Failed" })
            .map_err(|error| anyhow::anyhow!("Git {operation} returned {}, but Activity could not be saved. Inspect Git state before retrying: {error}", output.status))?;
    }
    anyhow::ensure!(
        !control.registry.cancelled(),
        "Git {operation} interrupted in {}; result is unknown. Inspect HEAD, index, working files and any native operation before retrying",
        repo.display()
    );
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    #[test]
    fn bounded_capture_drains_excess_without_aborting_a_write() {
        let (bytes, truncated) = drain(&b"abcdefgh"[..], 4).unwrap();
        assert_eq!(bytes, b"abcd");
        assert!(truncated);
    }

    fn probe(directory: &Path, mode: &str) -> Command {
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .args([
                "--exact",
                "core::process::write::tests::write_probe_child",
                "--nocapture",
            ])
            .env("IGIT_WRITE_PROBE", mode)
            .env("IGIT_WRITE_PROBE_DIR", directory);
        command
    }

    #[test]
    fn write_probe_child() {
        let Ok(mode) = std::env::var("IGIT_WRITE_PROBE") else {
            return;
        };
        let directory = std::path::PathBuf::from(std::env::var_os("IGIT_WRITE_PROBE_DIR").unwrap());
        match mode.as_str() {
            "tree" => {
                let mut child = probe(&directory, "leaf").spawn().unwrap();
                child.wait().unwrap();
            }
            "leaf" => {
                let started = Instant::now();
                while started.elapsed() < Duration::from_secs(15) {
                    std::fs::write(
                        directory.join("heartbeat"),
                        started.elapsed().as_nanos().to_string(),
                    )
                    .unwrap();
                    thread::sleep(Duration::from_millis(20));
                }
            }
            "noisy" => {
                let buffer = [b'x'; 8192];
                for _ in 0..(OUTPUT_LIMIT / buffer.len() + 4) {
                    std::io::stdout().write_all(&buffer).unwrap();
                }
                for _ in 0..(STDERR_LIMIT / buffer.len() + 4) {
                    std::io::stderr().write_all(&buffer).unwrap();
                }
            }
            _ => panic!("Unknown probe"),
        }
    }

    #[test]
    fn interrupt_stops_owned_write_and_descendants_and_records_unknown() {
        let directory = tempfile::tempdir().unwrap();
        let repo = directory.path().to_owned();
        let control = WriteControl::default();
        let worker_control = control.clone();
        let worker = thread::spawn(move || {
            crate::core::activity::with_directory(&repo, || {
                with_write_control(&worker_control, || {
                    write_output(&repo, "probe write", &mut probe(&repo, "tree"))
                })
            })
        });
        let deadline = Instant::now() + Duration::from_secs(5);
        let heartbeat = directory.path().join("heartbeat");
        while !heartbeat.exists() {
            if Instant::now() >= deadline {
                control.interrupt();
                panic!("Write process tree did not start");
            }
            thread::sleep(Duration::from_millis(10));
        }
        control.interrupt();
        let error = worker.join().unwrap().unwrap_err();
        assert!(error.to_string().contains("unknown"), "{error:#}");
        assert!(control.registry.is_empty());
        let stopped = std::fs::read(&heartbeat).unwrap();
        thread::sleep(Duration::from_millis(150));
        assert_eq!(std::fs::read(&heartbeat).unwrap(), stopped);
        let records = crate::core::activity::load(directory.path())
            .unwrap()
            .records;
        assert_eq!(records[0].status, "Unknown");
    }

    #[test]
    fn verbose_writes_finish_with_bounded_output_despite_expired_read_scope() {
        let directory = tempfile::tempdir().unwrap();
        let output = with_read_control(&ReadControl::new(Duration::ZERO), || {
            write_output(
                directory.path(),
                "noisy write",
                &mut probe(directory.path(), "noisy"),
            )
        })
        .unwrap();
        assert!(output.status.success());
        assert_eq!(output.stdout.len(), OUTPUT_LIMIT);
        assert!(output.stderr.len() < STDERR_LIMIT + 256);
        assert!(String::from_utf8_lossy(&output.stderr).contains("truncated"));
    }
}
