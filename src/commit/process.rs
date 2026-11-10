//! Bound subprocess IO and lifetime for one background AI job.
use anyhow::{bail, Context as _, Result};
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom, Write},
    process::{Child, Command, Output, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, MutexGuard,
    },
    thread,
    time::{Duration, Instant},
};

const STDERR_LIMIT: usize = 64 * 1024;
const REAP_TIMEOUT: Duration = Duration::from_millis(250);

#[derive(Clone)]
pub(super) struct Control {
    deadline: Instant,
    shared: Arc<Shared>,
}

#[derive(Default)]
struct Shared {
    cancelled: AtomicBool,
    active: Mutex<Vec<Arc<platform::Group>>>,
}

impl Shared {
    fn active(&self) -> MutexGuard<'_, Vec<Arc<platform::Group>>> {
        // A worker panic must not disable cancellation during application exit.
        self.active
            .lock()
            .unwrap_or_else(|error| error.into_inner())
    }
}

struct Running {
    child: Option<Child>,
    group: Arc<platform::Group>,
    shared: Arc<Shared>,
}

impl Running {
    fn child(&mut self) -> &mut Child {
        self.child.as_mut().expect("The running process is owned")
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        {
            let mut active = self.shared.active();
            // Kill descendants even if the direct child has already exited.
            // Serialize termination and removal with cancel(), so it cannot
            // retain a stale process-group ID after this guard is dropped.
            self.group.terminate();
            active.retain(|group| !Arc::ptr_eq(group, &self.group));
        }
        if let Some(child) = self.child.take() {
            reap(child);
        }
    }
}

impl Control {
    pub fn new(timeout: Duration) -> Self {
        Self {
            deadline: Instant::now() + timeout,
            shared: Arc::default(),
        }
    }

    pub fn cancel(&self) {
        self.shared.cancelled.store(true, Ordering::Release);
        // Application exit may stop the worker before its next poll. Signal all
        // registered groups here; never wait for the job or its output on UI.
        for group in self.shared.active().iter() {
            group.terminate();
        }
    }

    pub fn check(&self) -> Result<()> {
        if self.shared.cancelled.load(Ordering::Acquire) {
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
        let process = {
            // Admission and registration must be atomic with cancellation:
            // cancel() cannot return while a spawned child is still invisible.
            // Only OS spawn/setup runs under this lock, never polling or IO.
            let mut active = self.shared.active();
            self.check()?;
            let (child, group) = platform::spawn(&mut command)
                .context("Could not start or isolate the AI subprocess")?;
            let process = Running {
                child: Some(child),
                group: Arc::new(group),
                shared: Arc::clone(&self.shared),
            };
            active.push(Arc::clone(&process.group));
            process
        };
        self.check()?;
        let activation = process.group.activate();
        // Prefer cancellation/deadline errors if a Windows process was killed
        // between registration and resuming its suspended primary thread.
        self.check()?;
        activation.context("Could not start the isolated AI subprocess")?;
        Ok(process)
    }
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
    // A process blocked in the kernel must not stall cleanup indefinitely.
    // Reap it later if the application remains open.
    let _ = thread::Builder::new()
        .name("igit-ai-reaper".into())
        .spawn(move || {
            let _ = child.wait();
        });
}

#[cfg(unix)]
mod platform {
    use super::*;
    use std::{io, os::unix::process::CommandExt};

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
            // Cancellation and guard cleanup may both reach this group. Once
            // signalled, never target an ID that the OS could later reuse.
            if !self.active.swap(false, Ordering::AcqRel) {
                return;
            }
            // Every subprocess owns its process group, including Git filters.
            // A negative PID targets that group; 9 is SIGKILL on supported Unix.
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
        io,
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

    // Only the Windows ABI needed here is declared locally; no extra runtime
    // dependency or external taskkill process is required for cancellation.
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

        // Assign before running any child code, so even an immediately spawned
        // Git filter belongs to the job and cannot escape via a parent exit.
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
                // This is still under the admission lock. Terminate now, then
                // let a separate reaper wait without blocking UI cancellation.
                unsafe {
                    TerminateProcess(child.as_raw_handle(), 1);
                }
                let _ = thread::Builder::new()
                    .name("igit-ai-reaper".into())
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
            // These APIs return a fresh, non-inheritable handle owned here.
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
            // The owned job stays valid through registry removal and cleanup,
            // even after its original process has exited.
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
    use std::io;

    pub(super) struct Group;

    pub(super) fn spawn(_: &mut Command) -> io::Result<(Child, Group)> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "AI subprocess isolation is unavailable on this platform",
        ))
    }

    impl Group {
        pub(super) fn activate(&self) -> io::Result<()> {
            Ok(())
        }

        pub(super) fn terminate(&self) {}
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
        assert!(control.shared.active().is_empty());
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
            assert!(control.shared.active().is_empty());
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
            assert!(control.shared.active().is_empty());
        }

        #[test]
        fn successful_parent_exit_still_terminates_lingering_children() {
            let control = Control::new(Duration::from_secs(10));
            let tree = Tree::new();
            let output = control.run(tree.command(true), None, 1024).unwrap();
            assert!(output.status.success());
            assert_stopped(&tree.wait_ids());
            assert!(control.shared.active().is_empty());
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
            assert!(control.shared.active().is_empty());
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
            assert!(control.shared.active().is_empty());
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
                assert!(control.shared.active().is_empty());
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
