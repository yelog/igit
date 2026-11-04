//! Native clipboard tools keep platform integration out of panel event handling.
use anyhow::{bail, Context as _, Result};
use std::{
    io::Write,
    process::{Command, Stdio},
};

pub(crate) fn copy(text: &str) -> Result<()> {
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
    bail!("无法写入系统剪贴板（{}）", failures.join("；"))
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
        .context("启动剪贴板命令失败")?;
    let written = child
        .stdin
        .take()
        .context("剪贴板命令未提供输入管道")?
        .write_all(bytes);
    // Always close stdin and reap the child, including a broken input pipe.
    let status = child.wait().context("等待剪贴板命令失败")?;
    written.context("写入剪贴板命令失败")?;
    if !status.success() {
        bail!("剪贴板命令退出：{status}");
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
        let text = "标题\n\n正文 $HOME `literal`\n作者 <author@example.invalid>";
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
