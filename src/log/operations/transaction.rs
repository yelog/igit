use super::*;
use std::{
    io::{BufRead, BufReader},
    process::{Child, ChildStdout, Stdio},
};

pub(super) struct Transaction {
    child: Child,
    output: BufReader<ChildStdout>,
    errors: NamedTempFile,
}

impl Transaction {
    pub(super) fn prepare(
        repo: &Path,
        head: &Head,
        new: Option<&str>,
        reason: &str,
    ) -> Result<Self> {
        let errors = NamedTempFile::new()?;
        let mut child = command(repo)
            .args(["update-ref", "-m", reason, "--stdin"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(errors.reopen()?)
            .spawn()
            .context("Cannot lock the reviewed branch")?;
        let output = BufReader::new(child.stdout.take().context("Git output unavailable")?);
        let mut transaction = Self {
            child,
            output,
            errors,
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
        self.exchange("commit\n", &["commit: ok"])?;
        self.child.stdin.take();
        ensure!(self.child.wait()?.success(), "{}", self.error());
        Ok(())
    }

    fn exchange(&mut self, input: &str, responses: &[&str]) -> Result<()> {
        self.child
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
        self.child.stdin.take();
        let _ = self.child.wait();
    }
}
