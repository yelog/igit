//! Run independent repository operations with bounded concurrency.
use crate::core::git;
use std::{
    any::Any,
    collections::BTreeSet,
    fs,
    panic::{catch_unwind, AssertUnwindSafe},
    path::{Path, PathBuf},
    sync::mpsc,
    thread,
};

pub(crate) const LIMIT: usize = 4;

pub(crate) enum TaskResult<T> {
    Completed(T),
    Panicked(String),
    Blocked(Vec<usize>),
}

pub(crate) fn run<J, T>(
    jobs: &[J],
    prerequisites: &[Vec<usize>],
    conflicts: impl Fn(usize, usize) -> bool,
    task: impl Fn(usize, &J) -> T + Sync,
    succeeded: impl Fn(&T) -> bool,
    mut complete: impl FnMut(usize, &TaskResult<T>),
) -> Vec<TaskResult<T>>
where
    J: Sync,
    T: Send,
{
    assert_eq!(jobs.len(), prerequisites.len());
    let count = jobs.len();
    let mut results: Vec<Option<TaskResult<T>>> = (0..count).map(|_| None).collect();
    let mut running = BTreeSet::new();
    let task = &task;
    let succeeded = &succeeded;

    thread::scope(|scope| {
        let (sender, receiver) = mpsc::channel();
        let mut handles = (0..count)
            .map(|_| None)
            .collect::<Vec<Option<thread::ScopedJoinHandle<'_, ()>>>>();
        loop {
            let mut advanced = true;
            while advanced {
                advanced = false;
                for i in 0..count {
                    if results[i].is_some() || running.contains(&i) {
                        continue;
                    }
                    if !prerequisites[i]
                        .iter()
                        .all(|required| results[*required].is_some())
                    {
                        continue;
                    }
                    let blocked_by: Vec<_> = prerequisites[i]
                        .iter()
                        .copied()
                        .filter(|required| {
                            !matches!(
                                &results[*required],
                                Some(TaskResult::Completed(value)) if succeeded(value)
                            )
                        })
                        .collect();
                    if !blocked_by.is_empty() {
                        let result = TaskResult::Blocked(blocked_by);
                        complete(i, &result);
                        results[i] = Some(result);
                        advanced = true;
                        continue;
                    }
                    if running.len() >= LIMIT {
                        continue;
                    }
                    if running
                        .iter()
                        .any(|&other| conflicts(i, other) || conflicts(other, i))
                    {
                        continue;
                    }

                    running.insert(i);
                    let sender = sender.clone();
                    handles[i] = Some(scope.spawn(move || {
                        let result = match catch_unwind(AssertUnwindSafe(|| task(i, &jobs[i]))) {
                            Ok(value) => TaskResult::Completed(value),
                            Err(payload) => TaskResult::Panicked(panic_message(payload)),
                        };
                        let _ = sender.send((i, result));
                    }));
                    advanced = true;
                }
            }
            if running.is_empty() {
                break;
            }
            let Ok((i, result)) = receiver.recv() else {
                break;
            };
            running.remove(&i);
            if let Some(handle) = handles[i].take() {
                let _ = handle.join();
            }
            complete(i, &result);
            results[i] = Some(result);
        }
        drop(sender);
        for handle in handles.into_iter().flatten() {
            let _ = handle.join();
        }
    });

    for (i, result) in results.iter_mut().enumerate() {
        if result.is_none() {
            let unresolved = TaskResult::Blocked(Vec::new());
            complete(i, &unresolved);
            *result = Some(unresolved);
        }
    }
    results
        .into_iter()
        .map(|result| result.expect("every scheduled task has a result"))
        .collect()
}

fn panic_message(payload: Box<dyn Any + Send>) -> String {
    if let Some(message) = payload.downcast_ref::<&str>() {
        (*message).to_owned()
    } else if let Some(message) = payload.downcast_ref::<String>() {
        message.clone()
    } else {
        "unknown panic".into()
    }
}

#[derive(Clone, Debug)]
pub(crate) struct RepoLock {
    path: PathBuf,
    common_dir: Option<PathBuf>,
}

impl RepoLock {
    pub(crate) fn read(path: &Path) -> Self {
        let path = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        let common_dir = common_dir(&path);
        Self { path, common_dir }
    }

    pub(crate) fn conflicts(&self, other: &Self) -> bool {
        paths_overlap(&self.path, &other.path) || self.shares_git_state(other)
    }

    pub(crate) fn shares_git_state(&self, other: &Self) -> bool {
        match (&self.common_dir, &other.common_dir) {
            (Some(left), Some(right)) => left == right,
            // Unknown Git storage must never be treated as independent.
            _ => true,
        }
    }
}

fn paths_overlap(left: &Path, right: &Path) -> bool {
    left == right || left.starts_with(right) || right.starts_with(left)
}

fn common_dir(repo: &Path) -> Option<PathBuf> {
    let value = git(
        repo,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )
    .or_else(|_| git(repo, &["rev-parse", "--git-common-dir"]))
    .ok()?;
    let path = PathBuf::from(value);
    let path = if path.is_absolute() {
        path
    } else {
        repo.join(path)
    };
    fs::canonicalize(path).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };
    use std::time::Duration;

    #[test]
    fn independent_tasks_run_in_parallel_with_at_most_four_active() {
        let active = Arc::new(AtomicUsize::new(0));
        let maximum = Arc::new(AtomicUsize::new(0));
        let jobs = (0..10).collect::<Vec<_>>();
        let results = run(
            &jobs,
            &vec![Vec::new(); jobs.len()],
            |_, _| false,
            |_, _| {
                let now = active.fetch_add(1, Ordering::SeqCst) + 1;
                maximum.fetch_max(now, Ordering::SeqCst);
                thread::sleep(Duration::from_millis(60));
                active.fetch_sub(1, Ordering::SeqCst);
                true
            },
            |value| *value,
            |_, _| {},
        );
        assert!(results
            .iter()
            .all(|result| matches!(result, TaskResult::Completed(true))));
        assert_eq!(maximum.load(Ordering::SeqCst), LIMIT);
    }

    #[test]
    fn failed_prerequisites_block_dependents_without_running_them() {
        let ran_dependent = std::sync::atomic::AtomicBool::new(false);
        let jobs = [0, 1];
        let results = run(
            &jobs,
            &[Vec::new(), vec![0]],
            |_, _| false,
            |i, _| {
                if i == 0 {
                    false
                } else {
                    ran_dependent.store(true, Ordering::SeqCst);
                    true
                }
            },
            |value| *value,
            |_, _| {},
        );
        assert!(matches!(results[0], TaskResult::Completed(false)));
        assert!(matches!(&results[1], TaskResult::Blocked(blocked) if blocked == &[0]));
        assert!(!ran_dependent.load(Ordering::SeqCst));
    }

    #[test]
    fn conflicting_tasks_never_overlap() {
        let active = Arc::new(AtomicUsize::new(0));
        let maximum = Arc::new(AtomicUsize::new(0));
        let jobs = [0, 1];
        let results = run(
            &jobs,
            &[Vec::new(), Vec::new()],
            |_, _| true,
            |_, _| {
                let now = active.fetch_add(1, Ordering::SeqCst) + 1;
                maximum.fetch_max(now, Ordering::SeqCst);
                thread::sleep(Duration::from_millis(40));
                active.fetch_sub(1, Ordering::SeqCst);
                true
            },
            |value| *value,
            |_, _| {},
        );
        assert!(results
            .iter()
            .all(|result| matches!(result, TaskResult::Completed(true))));
        assert_eq!(maximum.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn a_panicking_task_reports_failure_and_does_not_deadlock() {
        let jobs = [0, 1];
        let results = run(
            &jobs,
            &[Vec::new(), Vec::new()],
            |_, _| false,
            |i, _| {
                if i == 0 {
                    panic!("worker exploded");
                }
                true
            },
            |value| *value,
            |_, _| {},
        );
        assert!(
            matches!(&results[0], TaskResult::Panicked(message) if message == "worker exploded")
        );
        assert!(matches!(results[1], TaskResult::Completed(true)));
    }

    #[test]
    fn linked_worktrees_share_git_state_while_unrelated_repositories_do_not() {
        let directory = tempfile::tempdir().unwrap();
        let main = directory.path().join("main");
        fs::create_dir(&main).unwrap();
        git(&main, &["init", "-q", "-b", "main"]).unwrap();
        for (key, value) in [
            ("user.name", "IGit test"),
            ("user.email", "test@invalid"),
            ("commit.gpgsign", "false"),
        ] {
            git(&main, &["config", key, value]).unwrap();
        }
        fs::write(main.join("a"), "a").unwrap();
        git(&main, &["add", "a"]).unwrap();
        git(&main, &["commit", "-qm", "base"]).unwrap();
        let linked = directory.path().join("linked");
        git(
            &main,
            &[
                "worktree",
                "add",
                "-q",
                "-b",
                "linked",
                linked.to_str().unwrap(),
            ],
        )
        .unwrap();
        let other = directory.path().join("other");
        fs::create_dir(&other).unwrap();
        git(&other, &["init", "-q", "-b", "main"]).unwrap();

        let main = RepoLock::read(&main);
        let linked = RepoLock::read(&linked);
        let other = RepoLock::read(&other);
        assert!(main.shares_git_state(&linked));
        assert!(main.conflicts(&linked));
        assert!(!main.shares_git_state(&other));
        assert!(!main.conflicts(&other));
    }

    #[test]
    fn nested_repositories_conflict_without_sharing_git_state() {
        let directory = tempfile::tempdir().unwrap();
        let parent = directory.path().join("parent");
        let child = parent.join("child");
        fs::create_dir_all(&child).unwrap();
        git(&parent, &["init", "-q", "-b", "main"]).unwrap();
        git(&child, &["init", "-q", "-b", "main"]).unwrap();

        let parent = RepoLock::read(&parent);
        let child = RepoLock::read(&child);
        assert!(parent.conflicts(&child));
        assert!(!parent.shares_git_state(&child));
    }
}
