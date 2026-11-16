use anyhow::{bail, Result};
use std::{
    cell::RefCell,
    fs::File,
    io::{self, Read, Seek, SeekFrom},
    path::Path,
    process::{Child, Command, ExitStatus, Output, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, MutexGuard,
    },
    thread,
    time::{Duration, Instant},
};

const READ_TIMEOUT: Duration = Duration::from_secs(30);
const OUTPUT_LIMIT: usize = 16 * 1024 * 1024;
const STDERR_LIMIT: usize = 64 * 1024;
const REAP_TIMEOUT: Duration = Duration::from_millis(250);

#[derive(Default)]
pub(crate) struct Registry {
    cancelled: AtomicBool,
    active: Mutex<Vec<Arc<platform::Group>>>,
}

impl Registry {
    fn active(&self) -> MutexGuard<'_, Vec<Arc<platform::Group>>> {
        self.active
            .lock()
            .unwrap_or_else(|error| error.into_inner())
    }

    pub(crate) fn cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }

    pub(crate) fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
        for group in self.active().iter() {
            group.terminate();
        }
    }

    pub(crate) fn spawn(self: &Arc<Self>, command: &mut Command) -> io::Result<Running> {
        let process = {
            let mut active = self.active();
            if self.cancelled() {
                return Err(io::ErrorKind::Interrupted.into());
            }
            let (child, group) = platform::spawn(command)?;
            let process = Running {
                child: Some(child),
                group: Arc::new(group),
                registry: self.clone(),
            };
            active.push(process.group.clone());
            process
        };
        if self.cancelled() {
            return Err(io::ErrorKind::Interrupted.into());
        }
        Ok(process)
    }

    #[cfg(test)]
    pub(crate) fn is_empty(&self) -> bool {
        self.active().is_empty()
    }
}

pub(crate) struct Running {
    child: Option<Child>,
    group: Arc<platform::Group>,
    registry: Arc<Registry>,
}

impl Running {
    pub(crate) fn child(&mut self) -> &mut Child {
        self.child.as_mut().expect("The running process is owned")
    }

    pub(crate) fn activate(&self) -> io::Result<()> {
        self.group.activate()
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        {
            let mut active = self.registry.active();
            self.group.terminate();
            active.retain(|group| !Arc::ptr_eq(group, &self.group));
        }
        if let Some(child) = self.child.take() {
            reap(child);
        }
    }
}

#[derive(Clone)]
pub(crate) struct ReadControl {
    timeout: Duration,
    registry: Arc<Registry>,
    deadline: Option<Instant>,
}

impl Default for ReadControl {
    fn default() -> Self {
        Self::new(READ_TIMEOUT)
    }
}

impl ReadControl {
    pub(crate) fn new(timeout: Duration) -> Self {
        Self {
            timeout,
            registry: Arc::default(),
            deadline: None,
        }
    }

    pub(crate) fn scoped(&self) -> Self {
        let mut control = self.clone();
        if let Some(deadline) = Instant::now().checked_add(self.timeout) {
            control.deadline = Some(self.deadline.map_or(deadline, |outer| outer.min(deadline)));
        }
        control
    }

    pub(crate) fn cancel(&self) {
        self.registry.cancel();
    }

    pub(crate) fn cancelled(&self) -> bool {
        self.registry.cancelled()
    }

    pub(crate) fn check(&self) -> Result<()> {
        if self.cancelled() {
            bail!("Git read cancelled; refresh to retry");
        }
        if self
            .deadline
            .is_some_and(|deadline| Instant::now() >= deadline)
        {
            bail!("Git operation timed out; retry with a smaller scope");
        }
        Ok(())
    }

    fn check_running(&self, started: Instant) -> std::result::Result<(), Failure> {
        if self.cancelled() {
            Err(Failure::Cancelled)
        } else if started.elapsed() >= self.timeout
            || self
                .deadline
                .is_some_and(|deadline| Instant::now() >= deadline)
        {
            Err(Failure::Timeout)
        } else {
            Ok(())
        }
    }

    fn capture(
        &self,
        command: &mut Command,
        started: Instant,
        limit: usize,
        truncate: bool,
    ) -> std::result::Result<Captured, Failure> {
        self.capture_with_input(command, started, limit, truncate, Stdio::null())
    }

    fn capture_with_input(
        &self,
        command: &mut Command,
        started: Instant,
        limit: usize,
        truncate: bool,
        input: Stdio,
    ) -> std::result::Result<Captured, Failure> {
        self.check_running(started)?;
        let mut stdout = tempfile::tempfile().map_err(|_| Failure::Io)?;
        let mut stderr = tempfile::tempfile().map_err(|_| Failure::Io)?;
        command
            .env("GIT_OPTIONAL_LOCKS", "0")
            .env("GIT_NO_LAZY_FETCH", "1")
            .stdin(input)
            .stdout(Stdio::from(stdout.try_clone().map_err(|_| Failure::Io)?))
            .stderr(Stdio::from(stderr.try_clone().map_err(|_| Failure::Io)?));
        let spawned = self.registry.spawn(command);
        self.check_running(started)?;
        let mut process = spawned.map_err(|_| Failure::Io)?;
        let activation = process.activate();
        self.check_running(started)?;
        activation.map_err(|_| Failure::Io)?;
        let status = loop {
            self.check_running(started)?;
            if stderr.metadata().map_err(|_| Failure::Io)?.len() > STDERR_LIMIT as u64 {
                return Err(Failure::Limit);
            }
            if stdout.metadata().map_err(|_| Failure::Io)?.len() > limit as u64 {
                if truncate {
                    break None;
                }
                return Err(Failure::Limit);
            }
            match process.child().try_wait().map_err(|_| Failure::Io)? {
                Some(status) => break Some(status),
                None => thread::sleep(Duration::from_millis(10)),
            }
        };
        drop(process);
        self.check_running(started)?;
        let mut bytes = read(&mut stdout, limit)?;
        let cut = bytes.len() > limit;
        if cut && !truncate {
            return Err(Failure::Limit);
        }
        bytes.truncate(limit);
        let stderr = read(&mut stderr, STDERR_LIMIT)?;
        if stderr.len() > STDERR_LIMIT {
            return Err(Failure::Limit);
        }
        self.check_running(started)?;
        Ok(Captured {
            status,
            stdout: bytes,
            stderr,
            cut,
        })
    }
}

thread_local! {
    static READ_CONTROL: RefCell<Option<ReadControl>> = const { RefCell::new(None) };
}

pub(crate) fn with_read_control<T>(control: &ReadControl, read: impl FnOnce() -> T) -> T {
    struct Restore(Option<ReadControl>);

    impl Drop for Restore {
        fn drop(&mut self) {
            READ_CONTROL.with(|slot| {
                slot.replace(self.0.take());
            });
        }
    }

    let _restore = Restore(READ_CONTROL.with(|slot| slot.replace(Some(control.clone()))));
    read()
}

pub(crate) fn check_read_control() -> Result<()> {
    READ_CONTROL.with(|slot| match slot.borrow().as_ref() {
        Some(control) => control.check(),
        None => Ok(()),
    })
}

fn current_control() -> ReadControl {
    READ_CONTROL.with(|slot| slot.borrow().clone().unwrap_or_default())
}

pub(crate) fn read_output(repo: &Path, operation: &str, command: &mut Command) -> Result<Output> {
    read_output_using(repo, operation, command, Stdio::null())
}

pub(crate) fn controlled_output(
    repo: &Path,
    operation: &str,
    command: &mut Command,
    control: &ReadControl,
) -> Result<Output> {
    let started = Instant::now();
    let output = control
        .capture(command, started, OUTPUT_LIMIT, false)
        .map_err(|failure| failure.error(repo, operation, started))?;
    Ok(Output {
        status: output
            .status
            .expect("Complete commands have an exit status"),
        stdout: output.stdout,
        stderr: output.stderr,
    })
}

pub(crate) fn read_output_with_input(
    repo: &Path,
    operation: &str,
    command: &mut Command,
    input: File,
) -> Result<Output> {
    read_output_using(repo, operation, command, Stdio::from(input))
}

fn read_output_using(
    repo: &Path,
    operation: &str,
    command: &mut Command,
    input: Stdio,
) -> Result<Output> {
    let started = Instant::now();
    let output = current_control()
        .capture_with_input(command, started, OUTPUT_LIMIT, false, input)
        .map_err(|failure| failure.error(repo, operation, started))?;
    Ok(Output {
        status: output.status.expect("Complete reads have an exit status"),
        stdout: output.stdout,
        stderr: output.stderr,
    })
}

pub(crate) fn read_prefix(
    repo: &Path,
    operation: &str,
    command: &mut Command,
    limit: usize,
) -> Result<(Vec<u8>, bool)> {
    let started = Instant::now();
    let output = current_control()
        .capture(command, started, limit, true)
        .map_err(|failure| failure.error(repo, operation, started))?;
    if !output.cut && !output.status.is_some_and(|status| status.success()) {
        bail!(
            "Git {operation} failed in {} after {:.3}s; refresh to retry",
            repo.display(),
            started.elapsed().as_secs_f64()
        );
    }
    Ok((output.stdout, output.cut))
}

#[derive(Debug)]
struct Captured {
    status: Option<ExitStatus>,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    cut: bool,
}

#[derive(Debug)]
enum Failure {
    Cancelled,
    Timeout,
    Limit,
    Io,
}

impl Failure {
    fn error(self, repo: &Path, operation: &str, started: Instant) -> anyhow::Error {
        let (reason, action) = match self {
            Self::Cancelled => (
                "cancelled because the request was replaced or closed",
                "refresh to retry",
            ),
            Self::Timeout => (
                "timed out",
                "check the repository or retry with a smaller scope",
            ),
            Self::Limit => (
                "exceeded the output safety limit",
                "narrow the requested scope",
            ),
            Self::Io => (
                "could not start, isolate, or read the subprocess",
                "check Git and refresh to retry",
            ),
        };
        anyhow::anyhow!(
            "Git {operation} in {} {reason} after {:.3}s; {action}",
            repo.display(),
            started.elapsed().as_secs_f64()
        )
    }
}

fn read(file: &mut File, limit: usize) -> std::result::Result<Vec<u8>, Failure> {
    file.seek(SeekFrom::Start(0)).map_err(|_| Failure::Io)?;
    let mut bytes = Vec::new();
    file.take(limit.saturating_add(1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| Failure::Io)?;
    Ok(bytes)
}

fn reap(mut child: Child) {
    let _ = child.kill();
    let deadline = Instant::now() + REAP_TIMEOUT;
    loop {
        match child.try_wait() {
            Ok(Some(_)) | Err(_) => return,
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(5)),
            Ok(None) => break,
        }
    }
    let _ = thread::Builder::new()
        .name("igit-process-reaper".into())
        .spawn(move || {
            let _ = child.wait();
        });
}

#[cfg(unix)]
mod platform {
    use super::*;
    use std::os::unix::process::CommandExt;

    pub(super) struct Group {
        id: u32,
        active: AtomicBool,
    }

    pub(super) fn spawn(command: &mut Command) -> io::Result<(Child, Group)> {
        let child = command.process_group(0).spawn()?;
        let group = Group {
            id: child.id(),
            active: AtomicBool::new(true),
        };
        Ok((child, group))
    }

    impl Group {
        pub(super) fn activate(&self) -> io::Result<()> {
            Ok(())
        }

        pub(super) fn terminate(&self) {
            if !self.active.swap(false, Ordering::AcqRel) {
                return;
            }
            unsafe {
                kill(-(self.id as i32), 9);
            }
        }
    }

    unsafe extern "C" {
        fn kill(pid: i32, signal: i32) -> i32;
    }
}

#[cfg(windows)]
mod platform {
    use super::*;
    use std::{
        ffi::c_void,
        mem::size_of,
        os::windows::{
            io::{AsRawHandle, FromRawHandle, OwnedHandle, RawHandle},
            process::CommandExt,
        },
        ptr,
    };

    const CREATE_SUSPENDED: u32 = 0x0000_0004;
    const JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE: u32 = 0x0000_2000;
    const JOB_OBJECT_EXTENDED_LIMIT_INFORMATION: i32 = 9;
    const TH32CS_SNAPTHREAD: u32 = 0x0000_0004;
    const THREAD_SUSPEND_RESUME: u32 = 0x0000_0002;

    #[repr(C)]
    #[derive(Default)]
    struct BasicLimits {
        process_time: i64,
        job_time: i64,
        flags: u32,
        minimum_working_set: usize,
        maximum_working_set: usize,
        active_processes: u32,
        affinity: usize,
        priority_class: u32,
        scheduling_class: u32,
    }

    #[repr(C)]
    #[derive(Default)]
    struct IoCounters {
        read_operations: u64,
        write_operations: u64,
        other_operations: u64,
        read_bytes: u64,
        write_bytes: u64,
        other_bytes: u64,
    }

    #[repr(C)]
    #[derive(Default)]
    struct ExtendedLimits {
        basic: BasicLimits,
        io: IoCounters,
        process_memory: usize,
        job_memory: usize,
        peak_process_memory: usize,
        peak_job_memory: usize,
    }

    #[repr(C)]
    struct ThreadEntry {
        size: u32,
        usage: u32,
        thread_id: u32,
        owner_id: u32,
        base_priority: i32,
        delta_priority: i32,
        flags: u32,
    }

    pub(super) struct Group {
        job: OwnedHandle,
        primary_thread: OwnedHandle,
    }

    pub(super) fn spawn(command: &mut Command) -> io::Result<(Child, Group)> {
        let job = owned(unsafe { CreateJobObjectW(ptr::null(), ptr::null()) })?;
        let mut limits = ExtendedLimits::default();
        limits.basic.flags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        if unsafe {
            SetInformationJobObject(
                job.as_raw_handle(),
                JOB_OBJECT_EXTENDED_LIMIT_INFORMATION,
                (&limits as *const ExtendedLimits).cast(),
                size_of::<ExtendedLimits>() as u32,
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        let child = command.creation_flags(CREATE_SUSPENDED).spawn()?;
        let setup = (|| {
            let assigned =
                unsafe { AssignProcessToJobObject(job.as_raw_handle(), child.as_raw_handle()) };
            if assigned == 0 {
                return Err(io::Error::last_os_error());
            }
            primary_thread(child.id())
        })();
        match setup {
            Ok(primary_thread) => Ok((
                child,
                Group {
                    job,
                    primary_thread,
                },
            )),
            Err(error) => {
                unsafe {
                    TerminateProcess(child.as_raw_handle(), 1);
                }
                let _ = thread::Builder::new()
                    .name("igit-process-reaper".into())
                    .spawn(move || {
                        let mut child = child;
                        let _ = child.wait();
                    });
                Err(error)
            }
        }
    }

    fn owned(handle: RawHandle) -> io::Result<OwnedHandle> {
        if handle.is_null() || handle as isize == -1 {
            Err(io::Error::last_os_error())
        } else {
            Ok(unsafe { OwnedHandle::from_raw_handle(handle) })
        }
    }

    fn primary_thread(process_id: u32) -> io::Result<OwnedHandle> {
        let snapshot = owned(unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) })?;
        let mut entry = ThreadEntry {
            size: size_of::<ThreadEntry>() as u32,
            usage: 0,
            thread_id: 0,
            owner_id: 0,
            base_priority: 0,
            delta_priority: 0,
            flags: 0,
        };
        let mut found = unsafe { Thread32First(snapshot.as_raw_handle(), &mut entry) };
        while found != 0 {
            if entry.owner_id == process_id {
                return owned(unsafe { OpenThread(THREAD_SUSPEND_RESUME, 0, entry.thread_id) });
            }
            found = unsafe { Thread32Next(snapshot.as_raw_handle(), &mut entry) };
        }
        Err(io::Error::last_os_error())
    }

    impl Group {
        pub(super) fn activate(&self) -> io::Result<()> {
            if unsafe { ResumeThread(self.primary_thread.as_raw_handle()) } == u32::MAX {
                Err(io::Error::last_os_error())
            } else {
                Ok(())
            }
        }

        pub(super) fn terminate(&self) {
            unsafe {
                TerminateJobObject(self.job.as_raw_handle(), 1);
            }
        }
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn CreateJobObjectW(attributes: *const c_void, name: *const u16) -> RawHandle;
        fn SetInformationJobObject(
            job: RawHandle,
            class: i32,
            information: *const c_void,
            length: u32,
        ) -> i32;
        fn AssignProcessToJobObject(job: RawHandle, process: RawHandle) -> i32;
        fn TerminateJobObject(job: RawHandle, exit_code: u32) -> i32;
        fn TerminateProcess(process: RawHandle, exit_code: u32) -> i32;
        fn CreateToolhelp32Snapshot(flags: u32, process_id: u32) -> RawHandle;
        fn Thread32First(snapshot: RawHandle, entry: *mut ThreadEntry) -> i32;
        fn Thread32Next(snapshot: RawHandle, entry: *mut ThreadEntry) -> i32;
        fn OpenThread(access: u32, inherit: i32, thread_id: u32) -> RawHandle;
        fn ResumeThread(thread: RawHandle) -> u32;
    }
}

#[cfg(not(any(unix, windows)))]
mod platform {
    use super::*;

    pub(super) struct Group;

    pub(super) fn spawn(_: &mut Command) -> io::Result<(Child, Group)> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Subprocess isolation is unavailable on this platform",
        ))
    }

    impl Group {
        pub(super) fn activate(&self) -> io::Result<()> {
            Ok(())
        }

        pub(super) fn terminate(&self) {}
    }
}

#[cfg(test)]
mod tests;

#[cfg(all(test, unix))]
mod controlled_tests {
    use super::*;

    #[test]
    fn explicit_controls_bound_mutating_commands_without_classifying_them_as_reads() {
        let directory = tempfile::tempdir().unwrap();
        let inherited = ReadControl::default();
        inherited.cancel();
        let explicit = ReadControl::default();
        let output = with_read_control(&inherited, || {
            controlled_output(
                directory.path(),
                "controlled mutation",
                Command::new("sh").args(["-c", "printf completed"]),
                &explicit,
            )
        })
        .unwrap();
        assert!(output.status.success());
        assert_eq!(output.stdout, b"completed");
        explicit.cancel();
        let error = controlled_output(
            directory.path(),
            "controlled mutation",
            Command::new("sh").args(["-c", "printf unexpected"]),
            &explicit,
        )
        .unwrap_err();
        assert!(error.to_string().contains("cancelled"));
        assert!(explicit.registry.is_empty());
    }

    #[test]
    fn explicit_controls_enforce_timeout_and_both_output_budgets() {
        let directory = tempfile::tempdir().unwrap();
        for (script, timeout, reason) in [
            ("sleep 30", Duration::from_millis(100), "timed out"),
            (
                "dd if=/dev/zero bs=1024 count=16385 2>/dev/null",
                Duration::from_secs(5),
                "output safety limit",
            ),
            (
                "dd if=/dev/zero bs=1024 count=65 >&2 2>/dev/null",
                Duration::from_secs(5),
                "output safety limit",
            ),
        ] {
            let control = ReadControl::new(timeout);
            let error = controlled_output(
                directory.path(),
                "controlled mutation",
                Command::new("sh").args(["-c", script]),
                &control,
            )
            .unwrap_err();
            assert!(error.to_string().contains(reason));
            assert!(control.registry.is_empty());
        }
    }

    #[test]
    fn scoped_controls_share_one_deadline_and_the_cancellation_registry() {
        let directory = tempfile::tempdir().unwrap();
        let control = ReadControl::new(Duration::from_millis(250));
        let scoped = control.scoped();
        assert!(Arc::ptr_eq(&scoped.registry, &control.registry));
        let first = controlled_output(
            directory.path(),
            "controlled mutation",
            Command::new("sh").args(["-c", "sleep 0.15"]),
            &scoped,
        )
        .unwrap();
        assert!(first.status.success());
        let error = controlled_output(
            directory.path(),
            "controlled mutation",
            Command::new("sh").args(["-c", "sleep 0.15"]),
            &scoped.scoped(),
        )
        .unwrap_err();
        assert!(error.to_string().contains("timed out"));
        assert!(scoped.check().is_err());
        control.cancel();
        assert!(scoped.cancelled());
        assert!(control.registry.is_empty());
    }
}
