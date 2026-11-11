use super::{reader, text, Source, LIMIT};
use anyhow::{bail, ensure, Context as _, Result};
use std::{
    fs,
    io::Read,
    path::{Component, Path},
    process::Output,
};

pub(crate) const UNTRACKED_LIMIT: usize = 96 * 1024;

fn checked(output: Output) -> Result<Vec<u8>> {
    ensure!(
        output.status.success(),
        "Git: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    );
    Ok(output.stdout)
}

fn absent() -> Source {
    Source {
        text: String::new(),
        description: "Absent from working tree".into(),
    }
}

fn limit(untracked: bool) -> usize {
    if untracked {
        UNTRACKED_LIMIT
    } else {
        LIMIT
    }
}

fn check_size(size: u64, untracked: bool) -> Result<()> {
    if size > limit(untracked) as u64 {
        if untracked {
            bail!("Untracked split preview is limited to 96 KiB; showing the bounded Git patch");
        }
        bail!("Split preview is limited to 512 KiB per revision; showing the Git patch");
    }
    Ok(())
}

pub(crate) fn load(
    repo: &Path,
    path: &Path,
    untracked: bool,
    index_mode: Option<&str>,
) -> Result<Source> {
    let parts: Vec<_> = path.components().collect();
    ensure!(
        !parts.is_empty()
            && parts
                .iter()
                .all(|part| matches!(part, Component::Normal(_))),
        "Split preview requires a repository-relative file path"
    );
    let mut parent = repo.to_path_buf();
    for part in &parts[..parts.len() - 1] {
        parent.push(part);
        match fs::symlink_metadata(&parent) {
            Ok(meta) if meta.is_dir() && !parent.join(".git").exists() => (),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(absent()),
            _ => bail!("Split preview cannot traverse symlink parents, nested repositories, or file/directory replacements; showing the Git patch"),
        }
    }
    let path_on_disk = repo.join(path);
    let metadata = match fs::symlink_metadata(&path_on_disk) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(absent()),
        Err(error) => {
            return Err(error).context("Cannot inspect working file; showing the Git patch")
        }
    };
    if metadata.is_symlink() {
        let target = fs::read_link(path_on_disk)?;
        let bytes = target
            .to_str()
            .context("Binary or non-UTF-8 symlink text; showing the Git patch")?
            .as_bytes()
            .to_vec();
        check_size(bytes.len() as u64, untracked)?;
        return text(bytes, "120000");
    }
    if !metadata.is_file() || index_mode == Some("120000") {
        bail!("Split preview is unavailable for directories, special files, or emulated symlinks; showing the Git patch");
    }
    check_size(metadata.len(), untracked)?;
    let attributes = attributes(repo, path)?;
    let mut bytes = Vec::new();
    fs::File::open(&path_on_disk)?
        .take((limit(untracked) + 1) as u64)
        .read_to_end(&mut bytes)?;
    check_size(bytes.len() as u64, untracked)?;
    check_line_endings(repo, &attributes, &bytes)?;
    #[cfg(unix)]
    let mode = {
        use std::os::unix::fs::PermissionsExt;
        let output = reader(repo)
            .args(["config", "--type=bool", "--get", "core.filemode"])
            .output()?;
        let filemode = if output.status.code() == Some(1) {
            true
        } else {
            String::from_utf8(checked(output)?)?.trim() == "true"
        };
        if !filemode {
            index_mode.unwrap_or("100644")
        } else if metadata.permissions().mode() & 0o100 != 0 {
            "100755"
        } else {
            "100644"
        }
    };
    #[cfg(not(unix))]
    let mode = index_mode.unwrap_or("100644");
    text(bytes, mode)
}

fn attributes(repo: &Path, path: &Path) -> Result<Vec<u8>> {
    let output = checked(
        reader(repo)
            .args([
                "--literal-pathspecs",
                "check-attr",
                "-z",
                "text",
                "eol",
                "crlf",
                "working-tree-encoding",
                "filter",
                "ident",
                "diff",
                "--",
            ])
            .arg(path)
            .output()?,
    )?;
    let fields: Vec<_> = output.split(|&byte| byte == 0).collect();
    if fields.len() != 22 || fields.last().is_none_or(|field| !field.is_empty()) {
        bail!("Cannot verify Git attributes; showing the Git patch");
    }
    for triple in fields[..21].chunks_exact(3) {
        if triple[0] != path.as_os_str().as_encoded_bytes() {
            bail!("Git returned attributes for a different path; showing the Git patch");
        }
        let (name, value) = (triple[1], triple[2]);
        if value == b"unspecified" {
            continue;
        }
        if name == b"diff" && value == b"unset" {
            bail!("Git marks this path as binary; showing the Git patch");
        }
        let supported = match name {
            b"text" => matches!(value, b"set" | b"unset" | b"auto"),
            b"eol" => matches!(value, b"unset" | b"lf" | b"crlf"),
            b"diff" => value == b"set",
            b"working-tree-encoding" | b"filter" | b"ident" => value == b"unset",
            _ => false,
        };
        if !supported {
            bail!(
                "The {} attribute may convert working contents; split preview does not run conversions or textconv, showing the Git patch",
                String::from_utf8_lossy(name)
            );
        }
    }
    Ok(output)
}

fn check_line_endings(repo: &Path, attributes: &[u8], bytes: &[u8]) -> Result<()> {
    let fields: Vec<_> = attributes.split(|&byte| byte == 0).collect();
    let attr = |name: &[u8]| {
        fields
            .chunks_exact(3)
            .find(|triple| triple[1] == name)
            .map(|triple| triple[2])
            .unwrap_or(b"unspecified")
    };
    if attr(b"text") == b"unset" {
        return Ok(());
    }
    let output = reader(repo)
        .args(["config", "--type=bool-or-str", "--get", "core.autocrlf"])
        .output()?;
    let autocrlf = if output.status.code() == Some(1) {
        "false".into()
    } else {
        String::from_utf8(checked(output)?)?
            .trim()
            .to_ascii_lowercase()
    };
    if !matches!(autocrlf.as_str(), "false" | "true" | "input") {
        bail!("Cannot verify core.autocrlf conversions; showing the Git patch");
    }
    let normalize = matches!(attr(b"text"), b"set" | b"auto")
        || matches!(attr(b"eol"), b"lf" | b"crlf")
        || autocrlf != "false";
    if normalize && bytes.windows(2).any(|pair| pair == b"\r\n") {
        bail!("Git normalizes working-tree line endings; split preview does not run conversions, showing the Git patch");
    }
    Ok(())
}
