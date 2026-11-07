use crate::installation::Inspection;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

#[derive(Deserialize, Serialize, PartialEq, Eq)]
struct Identity {
    executable: PathBuf,
    ownership: Option<Vec<u8>>,
}

impl Identity {
    fn current() -> Option<Self> {
        let executable = std::env::current_exe().ok()?.canonicalize().ok()?;
        let ownership = executable
            .parent()?
            .parent()?
            .parent()
            .and_then(|root| fs::read(root.join("install.json")).ok());
        Some(Self {
            executable,
            ownership,
        })
    }
}

#[derive(Deserialize, Serialize)]
struct Cache {
    schema: u32,
    checked_at: u64,
    identity: Identity,
    report: Inspection,
}

pub(super) fn read(path: &Path, now: SystemTime, interval: Duration) -> Option<Inspection> {
    let cache: Cache = serde_json::from_reader(fs::File::open(path).ok()?).ok()?;
    if cache.schema != 1 || cache.identity != Identity::current()? || cache.checked_at == 0 {
        return None;
    }
    let checked = UNIX_EPOCH.checked_add(Duration::from_secs(cache.checked_at))?;
    if now.duration_since(checked).ok()? > interval {
        return None;
    }
    cache.report.validate().ok()?;
    if cache.report.status == crate::installation::Status::Updated {
        return None;
    }
    Some(cache.report)
}

pub(super) fn write(path: &Path, report: &Inspection, now: SystemTime) -> anyhow::Result<()> {
    report.validate()?;
    let Some(identity) = Identity::current() else {
        return Ok(());
    };
    let parent = path.parent().unwrap_or(Path::new("."));
    fs::create_dir_all(parent)?;
    let cache = Cache {
        schema: 1,
        checked_at: now.duration_since(UNIX_EPOCH)?.as_secs(),
        identity,
        report: report.clone(),
    };
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    serde_json::to_writer(&mut temporary, &cache)?;
    temporary.persist(path)?;
    Ok(())
}
