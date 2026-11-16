use super::*;
use std::{
    os::unix::fs::{symlink, PermissionsExt},
    process::Command,
    thread,
    time::Instant,
};

fn executable(path: &Path, contents: &str) {
    fs::write(path, contents).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
}

#[test]
fn reference_transaction_hooks_cannot_modify_reviewed_working_files() {
    let fixture = Fixture::new();
    let marker = fixture.repo.join("hook-ran");
    executable(
        &fixture.repo.join(".git/hooks/reference-transaction"),
        "#!/bin/sh\nprintf 'unexpected hook' > hook-ran\nexit 0\n",
    );
    fixture.advance_remote();
    execute(&fixture.target(), &ReadControl::default()).unwrap();
    assert!(!marker.exists());
}

#[test]
fn filesystem_links_and_dangling_symbolic_refs_are_refused() {
    let fixture = Fixture::new();
    let reference = fixture.repo.join(".git/refs/remotes/origin/symlink");
    symlink(fixture.repo.join(".git/refs/heads/main"), &reference).unwrap();
    let error = execute(&fixture.target(), &ReadControl::default()).unwrap_err();
    assert!(error.to_string().contains("filesystem-linked"));
}

#[test]
fn network_diagnostics_are_classified_and_redacted_without_echoing_stderr() {
    let fixture = Fixture::new();
    let target = fixture.target();
    for (diagnostic, expected) in [
        (
            "fatal: Authentication failed for https://user:private-token@example.invalid/?credential=hidden",
            "authentication failed",
        ),
        (
            "fatal: TLS certificate invalid for https://private-token@example.invalid/",
            "TLS verification failed",
        ),
        (
            "Host key verification failed: private-token",
            "host key verification failed",
        ),
        (
            "unknown failure involving private-token and credential=hidden",
            "failed; retry",
        ),
    ] {
        let mut command = Command::new("sh");
        command.args(["-c", "printf '%s' \"$1\" >&2; exit 1", "fetch-test", diagnostic]);
        let error = transport::run(
            &target,
            &ReadControl::default(),
            "automatic fetch",
            &mut command,
        )
        .unwrap_err();
        let message = format!("{error:#}");
        assert!(message.contains(expected), "{message}");
        assert!(!message.contains("private-token"), "{message}");
        assert!(!message.contains("credential=hidden"), "{message}");
    }
}

fn fetch_shim(fixture: &mut Fixture) -> PathBuf {
    let original_path = std::env::var_os("PATH").unwrap();
    let real_git = std::env::split_paths(&original_path)
        .map(|path| path.join("git"))
        .find(|path| path.is_file())
        .unwrap();
    let bin = fixture.directory.path().join("bin");
    fs::create_dir(&bin).unwrap();
    executable(
        &bin.join("git"),
        "#!/bin/sh\nfor argument do\n  if [ \"$argument\" = fetch ]; then\n    sleep 30 &\n    child=$!\n    printf '%s %s\\n' \"$$\" \"$child\" > \"$IGIT_FETCH_TEST_MARKER\"\n    wait\n    exit 0\n  fi\ndone\nexec \"$IGIT_FETCH_TEST_REAL_GIT\" \"$@\"\n",
    );
    let path =
        std::env::join_paths(std::iter::once(bin).chain(std::env::split_paths(&original_path)))
            .unwrap();
    let marker = fixture.directory.path().join("pids");
    fixture.environment.set("PATH", Some(&path));
    fixture
        .environment
        .set("IGIT_FETCH_TEST_REAL_GIT", Some(real_git.as_os_str()));
    fixture
        .environment
        .set("IGIT_FETCH_TEST_MARKER", Some(marker.as_os_str()));
    marker
}

fn wait_ids(marker: &Path) -> Vec<u32> {
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        if let Ok(text) = fs::read_to_string(marker) {
            let ids: Vec<u32> = text
                .split_whitespace()
                .map(|value| value.parse().unwrap())
                .collect();
            if ids.len() == 2 {
                return ids;
            }
        }
        assert!(Instant::now() < deadline, "The fetch shim never started");
        thread::sleep(Duration::from_millis(5));
    }
}

fn stopped(ids: &[u32]) {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let alive = ids.iter().any(|id| {
            let output = Command::new("ps")
                .args(["-o", "stat=", "-p", &id.to_string()])
                .output()
                .unwrap();
            output.status.success()
                && String::from_utf8_lossy(&output.stdout)
                    .split_whitespace()
                    .any(|state| !state.starts_with('Z'))
        });
        if !alive {
            return;
        }
        assert!(Instant::now() < deadline, "A fetch child survived cleanup");
        thread::sleep(Duration::from_millis(10));
    }
}

struct CancelOnDrop(ReadControl);

impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        self.0.cancel();
    }
}

#[test]
fn cancelling_a_running_fetch_stops_its_process_group() {
    let mut fixture = Fixture::new();
    let marker = fetch_shim(&mut fixture);
    let target = fixture.target();
    let control = CancelOnDrop(ReadControl::new(Duration::from_secs(10)));
    let worker_control = control.0.clone();
    let worker = thread::spawn(move || execute(&target, &worker_control));
    let ids = wait_ids(&marker);
    control.0.cancel();
    let error = worker.join().unwrap().unwrap_err();
    assert!(error.to_string().contains("cancelled"));
    stopped(&ids);
}

#[test]
fn a_running_fetch_times_out_and_stops_descendants() {
    let mut fixture = Fixture::new();
    let marker = fetch_shim(&mut fixture);
    let target = fixture.target();
    let control = CancelOnDrop(ReadControl::new(Duration::from_secs(3)));
    let worker_control = control.0.clone();
    let worker = thread::spawn(move || execute(&target, &worker_control));
    let ids = wait_ids(&marker);
    let started = Instant::now();
    let error = worker.join().unwrap().unwrap_err();
    assert!(error.to_string().contains("timed out"));
    assert!(started.elapsed() < Duration::from_secs(5));
    stopped(&ids);
}
