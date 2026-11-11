use super::*;
use std::collections::HashMap;

pub(super) fn execute(repo: &Path, head: &Head, commits: &[String], input: &str) -> Result<()> {
    let selected = commits.first().context("No commit selected")?;
    let mut rewritten = HashMap::<String, String>::new();
    for oid in commits {
        let raw = git_bytes(repo, &["cat-file", "commit", oid])?;
        let message = (oid == selected).then_some(input);
        let data = replace(&raw, &rewritten, message)?;
        let replacement = git_input(
            repo,
            &["hash-object", "-t", "commit", "-w", "--stdin"],
            &data,
        )?;
        rewritten.insert(oid.clone(), replacement);
    }
    let tip = rewritten
        .get(&head.oid)
        .context("Rewritten branch tip is missing")?;
    let transaction =
        transaction::Transaction::prepare(repo, head, Some(tip), "igit: edit commit message")?;
    ensure_idle(repo)?;
    ensure_unpublished(repo, selected)?;
    transaction.commit()
}

pub(super) fn replace(
    raw: &[u8],
    rewritten: &HashMap<String, String>,
    message: Option<&str>,
) -> Result<Vec<u8>> {
    let separator = raw
        .windows(2)
        .position(|pair| pair == b"\n\n")
        .context("Invalid commit headers")?;
    let mut data = Vec::with_capacity(raw.len());
    let mut skip = false;
    for line in raw[..separator].split(|byte| *byte == b'\n') {
        if !line.starts_with(b" ") {
            skip = line.starts_with(b"gpgsig ")
                || line.starts_with(b"gpgsig-sha256 ")
                || line.starts_with(b"mergetag ")
                || message.is_some() && line.starts_with(b"encoding ");
        }
        if skip {
            continue;
        }
        if let Some(parent) = line.strip_prefix(b"parent ") {
            let parent = std::str::from_utf8(parent).context("Invalid parent object ID")?;
            if let Some(replacement) = rewritten.get(parent) {
                data.extend_from_slice(format!("parent {replacement}\n").as_bytes());
                continue;
            }
        }
        data.extend_from_slice(line);
        data.push(b'\n');
    }
    data.push(b'\n');
    if let Some(message) = message {
        data.extend_from_slice(message.as_bytes());
        data.push(b'\n');
    } else {
        data.extend_from_slice(&raw[separator + 2..]);
    }
    Ok(data)
}
