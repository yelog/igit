//! Opt-in Kitty pane control. Remote requests always target the originating window.
mod maximize;

use crate::ui::panes::Direction;
use std::{
    env,
    fs::File,
    io::{self, Read, Seek, SeekFrom, Write},
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    thread,
    time::{Duration, Instant},
};

pub(super) const RESIZE_STEP: u16 = 3;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(1);
const MAX_OUTPUT_BYTES: u64 = 1024 * 1024;

#[derive(Debug, PartialEq, Eq)]
pub(super) enum Outcome {
    Done,
    /// An explicit verified Kitty state, or a local toggle for a single group.
    Maximized(Option<bool>),
}

#[derive(Clone)]
struct Target {
    socket: String,
    window: u64,
}

impl Target {
    fn from_values(socket: Option<String>, window: Option<String>) -> Option<Self> {
        let socket = socket.filter(|s| !s.is_empty())?;
        let window = window?.parse::<u64>().ok().filter(|id| *id > 0)?;
        Some(Self { socket, window })
    }

    fn remote_command(&self, args: &[&str]) -> Command {
        let mut command = Command::new("kitten");
        command.args(["@", "--to", &self.socket]).args(args);
        command
    }

    fn command(&self, direction: Direction) -> Command {
        let direction = match direction {
            Direction::Left => "left",
            Direction::Down => "bottom",
            Direction::Up => "top",
            Direction::Right => "right",
        };
        self.remote_command(&[
            "action",
            "--match",
            &format!("id:{}", self.window),
            &format!("neighboring_window {direction}"),
        ])
    }

    fn resize_command(&self, direction: Direction) -> Command {
        let direction = match direction {
            Direction::Left => "left",
            Direction::Right => "right",
            Direction::Up => "up",
            Direction::Down => "down",
        };
        self.remote_command(&[
            "kitten",
            "--match",
            &format!("id:{}", self.window),
            "igit_resize.py",
            direction,
            &RESIZE_STEP.to_string(),
        ])
    }
}

struct Pending {
    receiver: mpsc::Receiver<Result<Outcome, String>>,
    cancelled: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}

impl Drop for Pending {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Relaxed);
        // Reap the bounded subprocess before the application can exit.
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

#[derive(Default)]
pub(super) struct Kitty {
    target: Option<Target>,
    pending: Option<Pending>,
    #[cfg(test)]
    pub requests: Vec<Direction>,
    #[cfg(test)]
    pub resizes: Vec<Direction>,
}

impl Kitty {
    pub fn from_env() -> Self {
        Self {
            target: Target::from_values(
                env::var("KITTY_LISTEN_ON").ok(),
                env::var("KITTY_WINDOW_ID").ok(),
            ),
            ..Self::default()
        }
    }

    pub fn available(&self) -> bool {
        self.target.is_some()
    }

    pub fn busy(&self) -> bool {
        self.pending.is_some()
    }

    pub fn focus(&mut self, direction: Direction) {
        if self.busy() {
            return;
        }
        #[cfg(test)]
        self.requests.push(direction);
        let Some(target) = &self.target else { return };
        self.start(target.command(direction));
    }

    pub fn resize(&mut self, direction: Direction) {
        if self.busy() {
            return;
        }
        #[cfg(test)]
        self.resizes.push(direction);
        let Some(target) = &self.target else { return };
        self.start(target.resize_command(direction));
    }

    pub fn toggle_maximized(&mut self) {
        if self.busy() {
            return;
        }
        let Some(target) = self.target.clone() else {
            return;
        };
        self.start_job(move |cancelled| {
            maximize::toggle(&target, cancelled, REQUEST_TIMEOUT).map(Outcome::Maximized)
        });
    }

    fn start(&mut self, command: Command) {
        self.start_job(move |cancelled| {
            run(command, cancelled, REQUEST_TIMEOUT).map(|()| Outcome::Done)
        });
    }

    fn start_job(
        &mut self,
        job: impl FnOnce(&AtomicBool) -> Result<Outcome, String> + Send + 'static,
    ) {
        let (sender, receiver) = mpsc::channel();
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_cancelled = cancelled.clone();
        let worker = thread::spawn(move || {
            let _ = sender.send(job(&worker_cancelled));
        });
        self.pending = Some(Pending {
            receiver,
            cancelled,
            worker: Some(worker),
        });
    }

    pub fn poll(&mut self) -> Option<Result<Outcome, String>> {
        let result = match self.pending.as_ref()?.receiver.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => return None,
            Err(mpsc::TryRecvError::Disconnected) => Err("Kitty pane worker stopped".into()),
        };
        self.pending = None;
        Some(result)
    }

    #[cfg(test)]
    pub fn set_pending_result(&mut self, result: Result<Outcome, String>) {
        let (sender, receiver) = mpsc::channel();
        sender.send(result).unwrap();
        self.pending = Some(Pending {
            receiver,
            cancelled: Arc::new(AtomicBool::new(false)),
            worker: None,
        });
    }
}

fn run(command: Command, cancelled: &AtomicBool, timeout: Duration) -> Result<(), String> {
    run_command(command, cancelled, timeout, false).map(|_| ())
}

fn run_output(
    command: Command,
    cancelled: &AtomicBool,
    timeout: Duration,
) -> Result<Vec<u8>, String> {
    run_command(command, cancelled, timeout, true)
}

fn run_command(
    mut command: Command,
    cancelled: &AtomicBool,
    timeout: Duration,
    capture: bool,
) -> Result<Vec<u8>, String> {
    let start = Instant::now();
    if cancelled.load(Ordering::Relaxed) {
        return Err("Kitty pane action cancelled".into());
    }
    if timeout.is_zero() {
        return Err("Kitty pane action timed out. Check Kitty remote control settings".into());
    }
    // A temporary file avoids filling a pipe while the worker polls the child.
    // Poll its size too, so a faulty command cannot grow it for the full timeout.
    let mut output = capture
        .then(tempfile::tempfile)
        .transpose()
        .map_err(|error| format!("Kitty pane output could not be captured: {error}"))?;
    let stdout = match &output {
        Some(file) => Stdio::from(
            file.try_clone()
                .map_err(|error| format!("Kitty pane output could not be captured: {error}"))?,
        ),
        None => Stdio::null(),
    };
    if cancelled.load(Ordering::Relaxed) {
        return Err("Kitty pane action cancelled".into());
    }
    if start.elapsed() >= timeout {
        return Err("Kitty pane action timed out. Check Kitty remote control settings".into());
    }
    let mut child = command
        .stdin(Stdio::null())
        .stdout(stdout)
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| match error.kind() {
            io::ErrorKind::NotFound => {
                "Kitty pane action unavailable: kitten is not on PATH".into()
            }
            _ => format!("Kitty pane action could not start: {error}"),
        })?;
    let result = (|| loop {
        if cancelled.load(Ordering::Relaxed) {
            return Err("Kitty pane action cancelled".into());
        }
        if start.elapsed() >= timeout {
            return Err("Kitty pane action timed out. Check Kitty remote control settings".into());
        }
        if let Some(file) = &output {
            check_output_size(file)?;
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                return if status.success() {
                    Ok(())
                } else {
                    Err("Kitty pane action failed. Check allow_remote_control and listen_on; resizing also requires igit_resize.py in Kitty's config directory".into())
                }
            }
            Ok(None) => (),
            Err(error) => return Err(format!("Kitty pane action failed: {error}")),
        }
        thread::sleep(Duration::from_millis(10));
    })();
    if result.is_err() {
        let _ = child.kill();
        let _ = child.wait();
    }
    result?;
    let mut bytes = Vec::new();
    if let Some(file) = &mut output {
        check_output_size(file)?;
        file.seek(SeekFrom::Start(0))
            .and_then(|_| file.take(MAX_OUTPUT_BYTES + 1).read_to_end(&mut bytes))
            .map_err(|error| format!("Kitty pane output could not be read: {error}"))?;
        if bytes.len() as u64 > MAX_OUTPUT_BYTES {
            return Err("Kitty pane output exceeded 1 MiB".into());
        }
    }
    Ok(bytes)
}

fn check_output_size(file: &File) -> Result<(), String> {
    if file
        .metadata()
        .map_err(|error| format!("Kitty pane output could not be read: {error}"))?
        .len()
        > MAX_OUTPUT_BYTES
    {
        return Err("Kitty pane output exceeded 1 MiB".into());
    }
    Ok(())
}

pub(super) fn set_user_var(writer: &mut impl Write, enabled: bool) -> io::Result<()> {
    writer.write_all(if enabled {
        b"\x1b]1337;SetUserVar=IS_IGIT=MQ==\x07"
    } else {
        b"\x1b]1337;SetUserVar=IS_IGIT\x07"
    })?;
    writer.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kitty_requires_explicit_socket_and_source_window() {
        for (socket, id) in [
            (None, Some("7")),
            (Some(""), Some("7")),
            (Some("unix:/test"), None),
            (Some("unix:/test"), Some("0")),
            (Some("unix:/test"), Some("all")),
        ] {
            assert!(
                Target::from_values(socket.map(str::to_owned), id.map(str::to_owned)).is_none()
            );
        }
        let target =
            Target::from_values(Some("unix:/test socket".into()), Some("7".into())).unwrap();
        for (direction, name) in [
            (Direction::Left, "left"),
            (Direction::Right, "right"),
            (Direction::Up, "top"),
            (Direction::Down, "bottom"),
        ] {
            let command = target.command(direction);
            let args: Vec<_> = command.get_args().map(|s| s.to_str().unwrap()).collect();
            assert_eq!(
                args,
                [
                    "@",
                    "--to",
                    "unix:/test socket",
                    "action",
                    "--match",
                    "id:7",
                    &format!("neighboring_window {name}")
                ]
            );
        }
    }

    #[test]
    fn kitty_smart_resize_targets_source_with_fixed_cell_step() {
        let target =
            Target::from_values(Some("unix:/test socket".into()), Some("7".into())).unwrap();
        for (direction, name) in [
            (Direction::Left, "left"),
            (Direction::Right, "right"),
            (Direction::Up, "up"),
            (Direction::Down, "down"),
        ] {
            let command = target.resize_command(direction);
            let args: Vec<_> = command.get_args().map(|s| s.to_str().unwrap()).collect();
            assert_eq!(
                args,
                [
                    "@",
                    "--to",
                    "unix:/test socket",
                    "kitten",
                    "--match",
                    "id:7",
                    "igit_resize.py",
                    name,
                    "3"
                ]
            );
        }
    }

    #[test]
    fn kitty_marker_is_scoped_and_cleared() {
        let mut bytes = vec![];
        set_user_var(&mut bytes, true).unwrap();
        set_user_var(&mut bytes, false).unwrap();
        assert_eq!(
            bytes,
            b"\x1b]1337;SetUserVar=IS_IGIT=MQ==\x07\x1b]1337;SetUserVar=IS_IGIT\x07"
        );
    }

    #[cfg(unix)]
    #[test]
    fn kitty_drop_cancels_and_joins_the_worker() {
        let directory = tempfile::tempdir().unwrap();
        let pid_path = directory.path().join("pid");
        let mut command = Command::new("/bin/sh");
        command.args([
            "-c",
            "printf '%s' \"$$\" > \"$1\"; exec /bin/sleep 5",
            "igit-test",
        ]);
        command.arg(&pid_path);
        let (sender, receiver) = mpsc::channel();
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_cancelled = cancelled.clone();
        let worker = thread::spawn(move || {
            let _ = sender.send(
                run_output(command, &worker_cancelled, Duration::from_secs(1))
                    .map(|_| Outcome::Done),
            );
        });
        let pending = Pending {
            receiver,
            cancelled: cancelled.clone(),
            worker: Some(worker),
        };
        let start = Instant::now();
        let pid = loop {
            if let Ok(pid) = std::fs::read_to_string(&pid_path) {
                if pid.parse::<u32>().is_ok() {
                    break pid;
                }
            }
            assert!(start.elapsed() < Duration::from_secs(2));
            thread::sleep(Duration::from_millis(5));
        };
        let start = Instant::now();
        drop(pending);
        assert!(cancelled.load(Ordering::Relaxed));
        assert!(start.elapsed() < Duration::from_millis(500));
        assert!(!Command::new("/bin/kill")
            .args(["-0", &pid])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .unwrap()
            .success());
    }

    #[cfg(unix)]
    #[test]
    fn kitty_failures_and_timeouts_are_bounded() {
        let cancelled = AtomicBool::new(false);
        assert!(run(
            Command::new("/usr/bin/true"),
            &cancelled,
            Duration::from_secs(1)
        )
        .is_ok());
        assert!(run(
            Command::new("/usr/bin/false"),
            &cancelled,
            Duration::from_secs(1)
        )
        .unwrap_err()
        .contains("remote_control"));
        assert!(run(
            Command::new("/igit-test-missing-kitten"),
            &cancelled,
            Duration::from_secs(1)
        )
        .unwrap_err()
        .contains("PATH"));
        let mut sleep = Command::new("/bin/sleep");
        sleep.arg("5");
        let start = Instant::now();
        assert!(run(sleep, &cancelled, Duration::from_millis(20))
            .unwrap_err()
            .contains("timed out"));
        assert!(start.elapsed() < Duration::from_secs(2));
        cancelled.store(true, Ordering::Relaxed);
        assert!(run(
            Command::new("/igit-test-missing-kitten"),
            &cancelled,
            Duration::ZERO
        )
        .unwrap_err()
        .contains("cancelled"));
    }

    #[cfg(unix)]
    #[test]
    fn kitty_captures_more_than_a_pipe_buffer_without_blocking() {
        let mut command = Command::new("/usr/bin/printf");
        command.args(["%131072s", "x"]);
        let output = run_output(command, &AtomicBool::new(false), REQUEST_TIMEOUT).unwrap();
        assert_eq!(output.len(), 131_072);
        assert_eq!(output.last(), Some(&b'x'));
    }

    #[cfg(unix)]
    #[test]
    fn kitty_output_limits_and_captured_timeouts_are_bounded() {
        let cancelled = AtomicBool::new(false);
        let mut command = Command::new("/usr/bin/printf");
        command.args(["%1048577s", "x"]);
        assert!(run_output(command, &cancelled, REQUEST_TIMEOUT)
            .unwrap_err()
            .contains("exceeded 1 MiB"));
        let mut command = Command::new("/bin/sleep");
        command.arg("5");
        let start = Instant::now();
        assert!(run_output(command, &cancelled, Duration::from_millis(20))
            .unwrap_err()
            .contains("timed out"));
        assert!(start.elapsed() < Duration::from_secs(2));
        assert!(run_output(
            Command::new("/igit-test-missing-kitten"),
            &cancelled,
            Duration::ZERO,
        )
        .unwrap_err()
        .contains("timed out"));
    }

    #[cfg(unix)]
    #[test]
    fn kitty_output_limit_terminates_and_reaps_a_running_process() {
        let directory = tempfile::tempdir().unwrap();
        let pid_path = directory.path().join("pid");
        let mut command = Command::new("/bin/sh");
        command.args([
            "-c",
            "printf '%s' \"$$\" > \"$1\"; exec /usr/bin/yes",
            "igit-test",
        ]);
        command.arg(&pid_path);
        let start = Instant::now();
        assert!(
            run_output(command, &AtomicBool::new(false), REQUEST_TIMEOUT)
                .unwrap_err()
                .contains("exceeded 1 MiB")
        );
        assert!(start.elapsed() < Duration::from_secs(2));
        let pid = std::fs::read_to_string(pid_path).unwrap();
        assert!(!Command::new("/bin/kill")
            .args(["-0", &pid])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .unwrap()
            .success());
    }

    #[test]
    fn kitty_maximize_requires_target_and_leaves_busy_request_alone() {
        let mut kitty = Kitty::default();
        kitty.toggle_maximized();
        assert!(!kitty.busy());
        kitty.target = Target::from_values(Some("unix:/test".into()), Some("4".into()));
        kitty.set_pending_result(Ok(Outcome::Maximized(Some(true))));
        kitty.toggle_maximized();
        kitty.focus(Direction::Left);
        kitty.resize(Direction::Left);
        assert!(kitty.busy());
        assert_eq!(kitty.poll(), Some(Ok(Outcome::Maximized(Some(true)))));
        assert!(!kitty.busy());
        assert!(kitty.requests.is_empty());
        assert!(kitty.resizes.is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn kitty_action_worker_reports_done_and_clears_pending() {
        let mut kitty = Kitty::default();
        kitty.start(Command::new("/usr/bin/true"));
        assert!(kitty.busy());
        let start = Instant::now();
        let result = loop {
            if let Some(result) = kitty.poll() {
                break result;
            }
            assert!(start.elapsed() < Duration::from_secs(2));
            thread::sleep(Duration::from_millis(5));
        };
        assert_eq!(result, Ok(Outcome::Done));
        assert!(!kitty.busy());
    }
}
