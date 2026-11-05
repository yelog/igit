//! Native clipboard tools keep platform integration out of panel event handling.
use anyhow::{bail, Context as _, Result};
use std::{
    io::Write,
    process::{Command, Stdio},
};

pub(crate) fn copy(text: &str) -> Result<()> {
    #[cfg(test)]
    if CAPTURE.with(|c| {
        let mut c = c.borrow_mut();
        if let Some(items) = c.as_mut() {
            items.push(text.to_owned());
            true
        } else {
            false
        }
    }) {
        return Ok(());
    }
    let candidates: &[(&str, &[&str])] = if cfg!(target_os = "macos") {
        &[("pbcopy", &[])]
    } else if cfg!(windows) {
        &[("clip.exe", &[])]
    } else {
        &[
            ("wl-copy", &[]),
            ("xclip", &["-selection", "clipboard"]),
            ("xsel", &["--clipboard", "--input"]),
        ]
    };
    let mut failures = Vec::new();
    for (program, args) in candidates {
        match copy_with(Command::new(program).args(*args), text) {
            Ok(()) => return Ok(()),
            Err(e) => failures.push(format!("{program}: {e:#}")),
        }
    }
    bail!(
        "Cannot write to the system clipboard ({})",
        failures.join("; ")
    )
}

// Shell integration tests exercise the real dispatch without touching the user's
// clipboard or changing process-global environment variables in parallel tests.
#[cfg(test)]
thread_local! { static CAPTURE: std::cell::RefCell<Option<Vec<String>>> = const { std::cell::RefCell::new(None) }; }

#[cfg(test)]
pub(crate) struct CopyCapture;
#[cfg(test)]
impl CopyCapture {
    pub fn start() -> Self {
        CAPTURE.with(|c| *c.borrow_mut() = Some(vec![]));
        Self
    }
    pub fn texts(&self) -> Vec<String> {
        CAPTURE.with(|c| c.borrow().clone().unwrap())
    }
}
#[cfg(test)]
impl Drop for CopyCapture {
    fn drop(&mut self) {
        CAPTURE.with(|c| *c.borrow_mut() = None);
    }
}

fn copy_with(command: &mut Command, text: &str) -> Result<()> {
    // clip.exe accepts Unicode input with a UTF-16LE BOM.
    #[cfg(windows)]
    let text: Vec<u8> = std::iter::once(0xfeff)
        .chain(text.encode_utf16())
        .flat_map(u16::to_le_bytes)
        .collect();
    #[cfg(windows)]
    let bytes = text.as_slice();
    #[cfg(not(windows))]
    let bytes = text.as_bytes();
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .context("Failed to start clipboard command")?;
    let written = child
        .stdin
        .take()
        .context("Clipboard command did not provide an input pipe")?
        .write_all(bytes);
    // Always close stdin and reap the child, including a broken input pipe.
    let status = child
        .wait()
        .context("Failed to wait for clipboard command")?;
    written.context("Failed to write to clipboard command")?;
    if !status.success() {
        bail!("Clipboard command exited: {status}");
    }
    Ok(())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn clipboard_command_receives_exact_text_on_stdin() {
        let d = tempfile::tempdir().unwrap();
        let path = d.path().join("copied.txt");
        let text =
            "Title ＵＩ\n\nBody e\u{301} 👩‍💻 $HOME `literal`\nAuthor <author@example.invalid>";
        copy_with(
            Command::new("sh")
                .args(["-c", "cat > \"$1\"", "clipboard-test"])
                .arg(&path),
            text,
        )
        .unwrap();
        assert_eq!(std::fs::read_to_string(path).unwrap(), text);
    }

    #[test]
    fn clipboard_command_errors_are_reported() {
        assert!(copy_with(
            Command::new("sh").args(["-c", "cat >/dev/null; exit 7"]),
            "text"
        )
        .is_err());
        assert!(copy_with(
            &mut Command::new("/nonexistent/igit-clipboard-tool"),
            "text"
        )
        .is_err());
    }
}
