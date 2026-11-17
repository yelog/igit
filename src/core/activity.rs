use super::{
    process::{with_read_control, ReadControl},
    state::Native,
};
use anyhow::{ensure, Context as _, Result};
use serde::{Deserialize, Serialize};
use std::{
    cell::RefCell,
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

const CAPACITY: usize = 200;
const MAX_RECORD: u64 = 64 * 1024;

pub(crate) struct Journal {
    pub records: Vec<Record>,
    pub unreadable: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct Record {
    version: u8,
    pub id: String,
    pub operation: String,
    pub repository: Native,
    pub before: Option<String>,
    pub after: Option<String>,
    pub status: String,
}

thread_local! {
    static DIRECTORY: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
}

pub(crate) fn with_directory<T>(state: &Path, run: impl FnOnce() -> T) -> T {
    struct Restore(Option<PathBuf>);
    impl Drop for Restore {
        fn drop(&mut self) {
            DIRECTORY.with(|slot| {
                slot.replace(self.0.take());
            });
        }
    }
    let _restore = Restore(DIRECTORY.with(|slot| slot.replace(Some(state.join("activity")))));
    run()
}

fn head(repo: &Path) -> Option<String> {
    with_read_control(&ReadControl::default(), || {
        super::git(repo, &["rev-parse", "--verify", "HEAD"]).ok()
    })
}

pub(crate) struct Pending {
    directory: PathBuf,
    path: PathBuf,
    record: Record,
}

pub(crate) fn record<T>(
    repo: &Path,
    operation: &str,
    run: impl FnOnce() -> Result<T>,
) -> Result<T> {
    let pending = Pending::start(repo, operation)?;
    let result = run();
    if let Some(pending) = pending {
        pending.finish(repo, if result.is_ok() { "Succeeded" } else { "Needs verification" })
            .context("Repository action returned, but Activity could not be saved. Inspect Git state; do not automatically retry")?;
    }
    result
}

impl Pending {
    pub(crate) fn start(repo: &Path, operation: &str) -> Result<Option<Self>> {
        let Some(directory) = DIRECTORY.with(|slot| slot.borrow().clone()) else {
            return Ok(None);
        };
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let id = format!(
            "{timestamp:032x}-{:08x}-{:08x}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        );
        fs::create_dir_all(&directory)?;
        let pending = Self {
            path: directory.join(format!("{id}.json")),
            directory,
            record: Record {
                version: 1,
                id,
                operation: operation
                    .chars()
                    .filter(|ch| !ch.is_control())
                    .take(100)
                    .collect(),
                repository: Native::new(repo),
                before: head(repo),
                after: None,
                status: "Unfinished".into(),
            },
        };
        pending.save()?;
        Ok(Some(pending))
    }

    fn save(&self) -> Result<()> {
        let mut file = tempfile::NamedTempFile::new_in(&self.directory)?;
        file.write_all(&serde_json::to_vec(&self.record)?)?;
        file.as_file().sync_all()?;
        file.persist(&self.path)?;
        Ok(())
    }

    pub(crate) fn finish(mut self, repo: &Path, status: &str) -> Result<()> {
        self.record.after = head(repo);
        self.record.status = status.into();
        self.save()?;
        let mut records = files(&self.directory)?;
        records.sort();
        let remove = records.len().saturating_sub(CAPACITY);
        for path in records.into_iter().take(remove) {
            if let Ok(bytes) = fs::read(&path) {
                if serde_json::from_slice::<Record>(&bytes)
                    .is_ok_and(|record| record.status != "Unfinished")
                {
                    let _ = fs::remove_file(path);
                }
            }
        }
        Ok(())
    }
}

fn files(directory: &Path) -> Result<Vec<PathBuf>> {
    if !directory.exists() {
        return Ok(Vec::new());
    }
    Ok(fs::read_dir(directory)?
        .filter_map(|entry| entry.ok())
        .filter(|entry| {
            entry.file_type().is_ok_and(|kind| kind.is_file())
                && entry
                    .path()
                    .extension()
                    .is_some_and(|extension| extension == "json")
        })
        .map(|entry| entry.path())
        .collect())
}

pub(crate) fn load(state: &Path) -> Result<Journal> {
    let mut paths = files(&state.join("activity"))?;
    paths.sort();
    let mut records = Vec::new();
    let mut unreadable = 0;
    for path in paths.into_iter().rev().take(CAPACITY) {
        let parsed = (|| -> Result<Record> {
            ensure!(
                fs::metadata(&path)?.len() <= MAX_RECORD,
                "Activity record exceeds the size limit"
            );
            let record: Record = serde_json::from_slice(&fs::read(path)?)?;
            ensure!(record.version == 1, "Unsupported activity version");
            Ok(record)
        })();
        match parsed {
            Ok(record) => records.push(record),
            Err(_) => unreadable += 1,
        }
    }
    Ok(Journal {
        records,
        unreadable,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unfinished_and_failed_operations_survive_restart_without_replay() {
        let directory = tempfile::tempdir().unwrap();
        with_directory(directory.path(), || {
            let pending = Pending::start(directory.path(), "merge").unwrap().unwrap();
            assert_eq!(
                load(directory.path()).unwrap().records[0].status,
                "Unfinished"
            );
            pending.finish(directory.path(), "Failed").unwrap();
        });
        let records = load(directory.path()).unwrap().records;
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].status, "Failed");
        assert_eq!(records[0].repository.path().unwrap(), directory.path());
        assert!(records[0].before.is_none());
    }

    #[test]
    fn malformed_records_do_not_hide_other_results_and_arguments_are_not_saved() {
        let directory = tempfile::tempdir().unwrap();
        with_directory(directory.path(), || {
            let secret = "token-in-command-arguments";
            record(directory.path(), "Reviewed operation", || -> Result<()> {
                anyhow::bail!("{secret}")
            })
            .unwrap_err();
        });
        fs::write(directory.path().join("activity/broken.json"), b"malformed").unwrap();
        let journal = load(directory.path()).unwrap();
        assert_eq!(journal.unreadable, 1);
        assert_eq!(journal.records.len(), 1);
        assert_eq!(journal.records[0].status, "Needs verification");
        let path = files(&directory.path().join("activity"))
            .unwrap()
            .into_iter()
            .find(|path| path.file_name().unwrap() != "broken.json")
            .unwrap();
        assert!(!String::from_utf8_lossy(&fs::read(path).unwrap()).contains("token-in-command"));
    }
}
