use super::*;
use crate::core::update::{integrate_reviewed, is_ancestor, ready};

fn fetch_target(job: &Job) -> Result<Option<String>> {
    let destination = format!("refs/heads/{}", job.t.dst);
    let Some(tip) = advertised_tip(&job.path, &job.url, &destination)? else {
        return Ok(None);
    };
    git(
        &job.path,
        &[
            "fetch",
            "--no-tags",
            "--no-prune",
            "--no-prune-tags",
            "--no-recurse-submodules",
            "--no-write-fetch-head",
            "--refmap=",
            "--",
            &job.url,
            &tip,
        ],
    )?;
    job.revalidate()?;
    if git(&job.path, &["remote", "get-url", "--", &job.t.remote])? == job.url {
        let reference = format!("refs/remotes/{}/{}", job.t.remote, job.t.dst);
        let previous = remote_head(&job.path, &job.t);
        let empty = "0".repeat(tip.len());
        git(
            &job.path,
            &[
                "update-ref",
                &reference,
                &tip,
                previous.as_deref().unwrap_or(&empty),
            ],
        )?;
    }
    Ok(Some(tip))
}

pub(super) fn execute(
    job: &Job,
    strategy: Strategy,
    tags: bool,
    nohooks: bool,
    mut progress: impl FnMut(&'static str),
) -> Result<PushOutcome> {
    job.revalidate()?;
    ready(&job.path)?;
    progress("Fetching…");
    let tip = fetch_target(job).map_err(|error| {
        anyhow::anyhow!(
            "Fetch failed; nothing pushed: {}",
            redact_remote_error(&error.to_string(), &job.url)
        )
    })?;
    if tip.is_none() && job.tracking.is_some() {
        anyhow::bail!("Push target was deleted; refresh and review before recreating it");
    }
    job.revalidate()?;
    let mut updated = job.clone();
    updated.tracking = tip.clone();
    let mut integrated = false;
    if let Some(target) = &tip {
        if !is_ancestor(&job.path, target, &job.head)? {
            progress(if strategy == Strategy::Merge {
                "Merging…"
            } else {
                "Rebasing…"
            });
            job.revalidate()?;
            updated.head = integrate_reviewed(
                &job.path,
                target,
                strategy,
                &format!("refs/heads/{}", job.branch),
                &job.head,
            ).map_err(|error| {
                let details = redact_remote_error(&error.to_string(), &job.url);
                if crate::core::update::conflicts(&job.path) {
                    anyhow::anyhow!(
                        "Update stopped: conflicts during {}. Nothing pushed. Resolve conflicts, continue, then push again.\n{details}",
                        strategy.label()
                    )
                } else {
                    anyhow::anyhow!("Update failed; nothing pushed: {details}")
                }
            })?;
            integrated = true;
        }
        updated.revalidate()?;
        if target == &updated.head && !updated.set_upstream {
            return Ok(PushOutcome::UpToDate);
        }
    }
    progress("Pushing…");
    let outcome = push_one(&updated, tags, nohooks).map_err(|error| {
        if integrated {
            anyhow::anyhow!("Updated successfully; push failed: {error}")
        } else {
            anyhow::anyhow!("Push failed: {error}")
        }
    })?;
    Ok(if integrated && outcome == PushOutcome::Complete {
        PushOutcome::Updated(strategy)
    } else {
        outcome
    })
}

#[cfg(test)]
mod tests;
