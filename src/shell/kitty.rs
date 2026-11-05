//! Opt-in Kitty pane control. Remote requests always target the originating window.
use crate::ui::panes::Direction;
use std::{
    env,
    io::{self, Write},
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    thread,
    time::{Duration, Instant},
};

pub(super) const RESIZE_STEP: u16 = 3;

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

    fn command(&self, direction: Direction) -> Command {
        let direction = match direction {
            Direction::Left => "left",
            Direction::Down => "bottom",
            Direction::Up => "top",
            Direction::Right => "right",
        };
        let mut command = Command::new("kitten");
        command.args([
            "@",
            "--to",
            &self.socket,
            "action",
            "--match",
            &format!("id:{}", self.window),
            &format!("neighboring_window {direction}"),
        ]);
        command
    }

    fn resize_command(&self, direction: Direction) -> Command {
        let direction = match direction {
            Direction::Left => "left",
            Direction::Right => "right",
            Direction::Up => "up",
            Direction::Down => "down",
        };
        let mut command = Command::new("kitten");
        command.args([
            "@",
            "--to",
            &self.socket,
            "kitten",
            "--match",
            &format!("id:{}", self.window),
            "igit_resize.py",
            direction,
            &RESIZE_STEP.to_string(),
        ]);
        command
    }
}

struct Pending {
    receiver: mpsc::Receiver<Result<(), String>>,
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

    fn start(&mut self, command: Command) {
        let (sender, receiver) = mpsc::channel();
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_cancelled = cancelled.clone();
        let worker = thread::spawn(move || {
            let _ = sender.send(run(command, &worker_cancelled, Duration::from_secs(1)));
        });
        self.pending = Some(Pending {
            receiver,
            cancelled,
            worker: Some(worker),
        });
    }

    pub fn poll(&mut self) -> Option<Result<(), String>> {
        let result = match self.pending.as_ref()?.receiver.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => return None,
            Err(mpsc::TryRecvError::Disconnected) => Err("Kitty pane worker stopped".into()),
        };
        self.pending = None;
        Some(result)
    }
}

fn run(mut command: Command, cancelled: &AtomicBool, timeout: Duration) -> Result<(), String> {
    if cancelled.load(Ordering::Relaxed) {
        return Ok(());
    }
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| match error.kind() {
            io::ErrorKind::NotFound => {
                "Kitty pane action unavailable: kitten is not on PATH".into()
            }
            _ => format!("Kitty pane action could not start: {error}"),
        })?;
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                return if status.success() {
                    Ok(())
                } else {
                    Err("Kitty pane action failed. Check allow_remote_control and listen_on; resizing also requires igit_resize.py in Kitty's config directory".into())
                }
            }
            Ok(None) => (),
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("Kitty pane action failed: {error}"));
            }
        }
        if cancelled.load(Ordering::Relaxed) || start.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            return if cancelled.load(Ordering::Relaxed) {
                Ok(())
            } else {
                Err("Kitty pane action timed out. Check Kitty remote control settings".into())
            };
        }
        thread::sleep(Duration::from_millis(10));
    }
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
        let (sender, receiver) = mpsc::channel();
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_cancelled = cancelled.clone();
        let worker = thread::spawn(move || {
            let mut command = Command::new("/bin/sleep");
            command.arg("5");
            let _ = sender.send(run(command, &worker_cancelled, Duration::from_secs(1)));
        });
        let pending = Pending {
            receiver,
            cancelled: cancelled.clone(),
            worker: Some(worker),
        };
        thread::sleep(Duration::from_millis(20));
        let start = Instant::now();
        drop(pending);
        assert!(cancelled.load(Ordering::Relaxed));
        assert!(start.elapsed() < Duration::from_millis(500));
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
        .is_ok());
    }
}
