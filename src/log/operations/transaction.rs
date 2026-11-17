use super::*;
use std::{
    io::{BufRead, BufReader},
    process::{ChildStdout, Stdio},
};

pub(super) struct Transaction {
    child: crate::core::process::Running,
    output: BufReader<ChildStdout>,
    errors: NamedTempFile,
    activity: Option<crate::core::activity::Pending>,
    repository: PathBuf,
    commit_requested: bool,
}

impl Transaction {
    pub(super) fn prepare(
        repo: &Path,
        head: &Head,
        new: Option<&str>,
        reason: &str,
    ) -> Result<Self> {
        let errors = NamedTempFile::new()?;
        let (mut child, activity) = crate::core::process::start_write(
            repo,
            "reviewed ref transaction",
            command(repo)
                .args(["update-ref", "-m", reason, "--stdin"])
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(errors.reopen()?),
        )
        .context("Cannot lock the reviewed branch")?;
        let output = BufReader::new(
            child
                .child()
                .stdout
                .take()
                .context("Git output unavailable")?,
        );
        let mut transaction = Self {
            child,
            output,
            errors,
            activity,
            repository: repo.to_owned(),
            commit_requested: false,
        };
        let change = new.map_or_else(
            || format!("delete HEAD {}\n", head.oid),
            |new| format!("update HEAD {new} {}\n", head.oid),
        );
        transaction.exchange(
            &format!("start\nupdate ORIG_HEAD {}\n{change}prepare\n", head.oid),
            &["start: ok", "prepare: ok"],
        )?;
        ensure!(
            Head::read(repo)? == *head,
            "Branch or HEAD changed; review again"
        );
        Ok(transaction)
    }

    pub(super) fn commit(mut self) -> Result<()> {
        self.commit_requested = true;
        self.exchange("commit\n", &["commit: ok"])?;
        self.child.child().stdin.take();
        ensure!(self.child.child().wait()?.success(), "{}", self.error());
        if let Some(activity) = self.activity.take() {
            activity.finish(&self.repository, "Succeeded")?;
        }
        Ok(())
    }

    fn exchange(&mut self, input: &str, responses: &[&str]) -> Result<()> {
        self.child
            .child()
            .stdin
            .as_mut()
            .context("Git transaction is closed")?
            .write_all(input.as_bytes())?;
        for expected in responses {
            let mut response = String::new();
            self.output.read_line(&mut response)?;
            ensure!(response.trim_end() == *expected, "{}", self.error());
        }
        Ok(())
    }

    fn error(&self) -> String {
        let bytes = std::fs::read(self.errors.path()).unwrap_or_default();
        let message = String::from_utf8_lossy(&bytes);
        if message.trim().is_empty() {
            "Git ref transaction failed; inspect the branch before retrying".to_owned()
        } else {
            message.trim().to_owned()
        }
    }
}

impl Drop for Transaction {
    fn drop(&mut self) {
        self.child.child().stdin.take();
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(250);
        while self
            .child
            .child()
            .try_wait()
            .is_ok_and(|status| status.is_none())
            && std::time::Instant::now() < deadline
        {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        if !self.commit_requested
            && self
                .child
                .child()
                .try_wait()
                .is_ok_and(|status| status.is_some_and(|status| status.success()))
        {
            if let Some(activity) = self.activity.take() {
                let _ = activity.finish(&self.repository, "Aborted");
            }
        }
    }
}
