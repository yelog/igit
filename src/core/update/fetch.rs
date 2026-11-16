use crate::core::{git, redact_remote_error};
use anyhow::{ensure, Context as _, Result};
use std::path::Path;

pub(crate) fn fetch_upstream(
    repo: &Path,
    remote: &str,
    reference: &str,
    tracking: &str,
    endpoint: &str,
) -> Result<String> {
    let fetch = || -> Result<String> {
        let source = if remote == "." { "." } else { endpoint };
        let read_upload_pack = || {
            if remote == "." {
                None
            } else {
                git(
                    repo,
                    &["config", "--get", &format!("remote.{remote}.uploadpack")],
                )
                .ok()
            }
        };
        let upload_pack = read_upload_pack();
        let upload_pack_option = upload_pack
            .as_ref()
            .map(|value| format!("--upload-pack={value}"));
        let revalidate = || -> Result<()> {
            ensure!(
                remote == "." || git(repo, &["remote", "get-url", "--", remote])? == endpoint,
                "Remote URL changed during fetch; review again"
            );
            ensure!(
                git(repo, &["ls-remote", "--get-url", "--", source])? == source,
                "Fetch URL resolves to a different endpoint; inspect URL rewrite rules before retrying"
            );
            ensure!(
                read_upload_pack() == upload_pack,
                "Remote upload command changed during fetch; review again"
            );
            Ok(())
        };
        git(repo, &["check-ref-format", reference]).context("Invalid upstream reference")?;
        ensure!(
            remote == "." || tracking.starts_with("refs/remotes/"),
            "Fetch requires a remote-tracking reference"
        );
        git(repo, &["check-ref-format", tracking])
            .context("Invalid upstream tracking reference")?;
        revalidate()?;
        let previous = git(repo, &["rev-parse", "--verify", tracking]).ok();
        let mut query = vec!["ls-remote", "--refs"];
        if let Some(option) = &upload_pack_option {
            query.push(option);
        }
        query.extend(["--", source, reference]);
        let advertised = git(repo, &query)
            .map_err(|error| anyhow::anyhow!("Cannot query upstream at <remote>: {error}"))?;
        let mut captured = None;
        for line in advertised.lines() {
            let Some((oid, name)) = line.split_once('\t') else {
                continue;
            };
            if name != reference {
                continue;
            }
            ensure!(
                captured.is_none()
                    && matches!(oid.len(), 40 | 64)
                    && oid.bytes().all(|byte| byte.is_ascii_hexdigit()),
                "Cannot verify the exact upstream commit"
            );
            captured = Some(oid.to_owned());
        }
        let tip = captured.context("Upstream reference is no longer available; review again")?;
        revalidate()?;
        let mut transfer = vec![
            "fetch",
            "--no-tags",
            "--no-prune",
            "--no-prune-tags",
            "--no-recurse-submodules",
            "--no-write-fetch-head",
            "--refmap=",
        ];
        if let Some(option) = &upload_pack_option {
            transfer.push(option);
        }
        transfer.extend(["--", source, &tip]);
        git(repo, &transfer)?;
        ensure!(
            git(repo, &["cat-file", "-t", &tip])? == "commit",
            "Fetched upstream is not a commit"
        );
        revalidate()?;
        if remote != "." && git(repo, &["symbolic-ref", "-q", tracking]).is_err() {
            let empty = "0".repeat(tip.len());
            let _ = git(
                repo,
                &[
                    "update-ref",
                    "--no-deref",
                    tracking,
                    &tip,
                    previous.as_deref().unwrap_or(&empty),
                ],
            );
        }
        Ok(tip)
    };
    fetch().map_err(|error| anyhow::anyhow!(redact_remote_error(&error.to_string(), endpoint)))
}
