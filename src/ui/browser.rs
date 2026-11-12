use crate::core::{hosting, loading::Load};
use anyhow::{bail, ensure, Context as _, Result};
use std::{
    path::PathBuf,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

#[derive(Default)]
pub(crate) struct Browser {
    request: Load<Result<()>>,
}

impl Browser {
    pub(crate) fn open_commit(&mut self, repo: PathBuf, oid: String) -> Result<()> {
        ensure!(
            !self.pending(),
            "A commit is already opening in the browser"
        );
        #[cfg(test)]
        let capture = CAPTURE.with(|capture| capture.borrow().clone());
        self.request.request();
        self.request.start("igit-browser", move |_| {
            let url = hosting::commit_url(&repo, &oid)?;
            #[cfg(test)]
            if let Some(capture) = capture {
                let mut capture = capture.lock().unwrap();
                capture.urls.push(url);
                return if capture.fail {
                    Err(anyhow::anyhow!("Test browser could not open the URL"))
                } else {
                    Ok(())
                };
            }
            open(&url)
        })
    }

    pub(crate) fn poll(&mut self) -> Option<Result<()>> {
        self.request.take_result().map(Result::flatten)
    }

    pub(crate) fn pending(&self) -> bool {
        self.request.pending()
    }
}

fn open(url: &str) -> Result<()> {
    hosting::validate_web_url(url)?;
    let candidates: &[(&str, &[&str])] = if cfg!(target_os = "macos") {
        &[("open", &[])]
    } else if cfg!(windows) {
        &[("rundll32.exe", &["url.dll,FileProtocolHandler"])]
    } else {
        &[("xdg-open", &[]), ("gio", &["open"])]
    };
    let mut failures = Vec::new();
    for (program, args) in candidates {
        match open_with(Command::new(program).args(*args), url) {
            Ok(()) => return Ok(()),
            Err(error) => failures.push(format!("{program}: {error:#}")),
        }
    }
    bail!("Cannot open the default browser ({})", failures.join("; "))
}

fn open_with(command: &mut Command, url: &str) -> Result<()> {
    let mut child = command
        .arg(url)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .context("Failed to start browser command")?;
    let deadline = Instant::now() + Duration::from_millis(500);
    loop {
        if let Some(status) = child.try_wait().context("Cannot inspect browser command")? {
            ensure!(status.success(), "Browser command exited: {status}");
            return Ok(());
        }
        if Instant::now() >= deadline {
            std::thread::Builder::new()
                .name("igit-browser-wait".into())
                .spawn(move || {
                    let _ = child.wait();
                })
                .context("Cannot monitor browser command")?;
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[cfg(test)]
#[derive(Default)]
struct Captured {
    urls: Vec<String>,
    fail: bool,
}

#[cfg(test)]
type Capture = std::sync::Arc<std::sync::Mutex<Captured>>;

#[cfg(test)]
thread_local! {
    static CAPTURE: std::cell::RefCell<Option<Capture>> = const { std::cell::RefCell::new(None) };
}

#[cfg(test)]
pub(crate) struct BrowserCapture(Capture);

#[cfg(test)]
impl BrowserCapture {
    pub(crate) fn start(fail: bool) -> Self {
        let captured = std::sync::Arc::new(std::sync::Mutex::new(Captured {
            fail,
            ..Default::default()
        }));
        CAPTURE.with(|capture| *capture.borrow_mut() = Some(captured.clone()));
        Self(captured)
    }

    pub(crate) fn urls(&self) -> Vec<String> {
        self.0.lock().unwrap().urls.clone()
    }
}

#[cfg(test)]
impl Drop for BrowserCapture {
    fn drop(&mut self) {
        CAPTURE.with(|capture| *capture.borrow_mut() = None);
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn browser_command_receives_the_exact_url_as_one_argument() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("url.txt");
        let url = "https://forge.test/commit/123?view=diff&literal=$HOME;`command`";
        open_with(
            Command::new("sh")
                .args(["-c", "printf '%s' \"$2\" > \"$1\"", "browser-test"])
                .arg(&path),
            url,
        )
        .unwrap();
        assert_eq!(std::fs::read_to_string(path).unwrap(), url);
    }

    #[test]
    fn browser_command_reports_missing_executables_and_failed_exit_statuses() {
        for command in [
            &mut Command::new("/nonexistent/igit-browser-tool"),
            Command::new("sh").args(["-c", "exit 7"]),
        ] {
            assert!(open_with(command, "https://forge.test/commit/123").is_err());
        }
    }

    #[test]
    fn a_long_lived_browser_handler_does_not_block_subsequent_requests() {
        let directory = tempfile::tempdir().unwrap();
        let release = directory.path().join("release");
        let finished = directory.path().join("finished");
        let (send, receive) = std::sync::mpsc::channel();
        let release_arg = release.clone();
        let finished_arg = finished.clone();
        let worker = std::thread::spawn(move || {
            let result = open_with(
                Command::new("sh")
                    .args([
                        "-c",
                        "while [ ! -f \"$1\" ]; do sleep 0.01; done; printf done > \"$2\"",
                        "browser-wait-test",
                    ])
                    .arg(release_arg)
                    .arg(finished_arg),
                "https://forge.test/commit/123",
            );
            send.send(result).unwrap();
        });
        let launched = receive.recv_timeout(Duration::from_secs(3));
        std::fs::write(release, "").unwrap();
        worker.join().unwrap();
        launched
            .expect("Browser handler must be released before it exits")
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while !finished.exists() {
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    #[test]
    fn background_browser_errors_are_polled_and_do_not_start_duplicate_requests() {
        let directory = tempfile::tempdir().unwrap();
        let repo = directory.path().to_owned();
        crate::core::git(&repo, &["init", "-q"]).unwrap();
        crate::core::git(
            &repo,
            &[
                "config",
                "igit.webCommitUrl",
                "https://forge.test/commit/{commit}",
            ],
        )
        .unwrap();
        let capture = BrowserCapture::start(true);
        let mut browser = Browser::default();
        let oid = "a".repeat(40);
        browser.open_commit(repo.clone(), oid.clone()).unwrap();
        assert!(browser.open_commit(repo, oid.clone()).is_err());
        let deadline = Instant::now() + Duration::from_secs(5);
        let result = loop {
            if let Some(result) = browser.poll() {
                break result;
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(2));
        };
        assert!(result.unwrap_err().to_string().contains("Test browser"));
        assert_eq!(
            capture.urls(),
            vec![format!("https://forge.test/commit/{oid}")]
        );
        assert!(!browser.request.pending());
        assert!(browser.poll().is_none());
    }
}
