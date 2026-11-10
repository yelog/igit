use anyhow::{Context as _, Result};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::ErrorKind,
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum Strategy {
    #[default]
    Merge,
    Rebase,
}

impl Strategy {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Merge => "Merge",
            Self::Rebase => "Rebase",
        }
    }
}

fn file(state_dir: &Path) -> PathBuf {
    state_dir
        .parent()
        .unwrap_or(state_dir)
        .join("update-strategy.json")
}

pub(crate) fn load(state_dir: &Path) -> Result<Strategy> {
    let bytes = match fs::read(file(state_dir)) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(Strategy::default()),
        Err(error) => return Err(error).context("Cannot read default update strategy"),
    };
    serde_json::from_slice(&bytes).context("Cannot parse default update strategy")
}

pub(crate) fn save(state_dir: &Path, strategy: Strategy) -> Result<()> {
    let path = file(state_dir);
    let directory = path.parent().unwrap_or(state_dir);
    fs::create_dir_all(directory).context("Cannot create update strategy directory")?;
    let mut temporary =
        tempfile::NamedTempFile::new_in(directory).context("Cannot create update strategy file")?;
    serde_json::to_writer(&mut temporary, &strategy).context("Cannot encode update strategy")?;
    temporary
        .as_file()
        .sync_all()
        .context("Cannot flush update strategy")?;
    temporary
        .persist(path)
        .context("Cannot save default update strategy")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_strategy_defaults_to_merge_without_creating_state() {
        let directory = tempfile::tempdir().unwrap();
        let state = directory.path().join("workspace");
        assert_eq!(Strategy::default(), Strategy::Merge);
        assert_eq!(load(&state).unwrap(), Strategy::Merge);
        assert!(!file(&state).exists());
        assert!(!state.exists());
    }

    #[test]
    fn strategy_is_shared_by_workspaces_and_replaced_atomically() {
        let directory = tempfile::tempdir().unwrap();
        let first = directory.path().join("application").join("first");
        let second = directory.path().join("application").join("second");
        for strategy in [Strategy::Rebase, Strategy::Merge, Strategy::Rebase] {
            save(&first, strategy).unwrap();
            assert_eq!(load(&first).unwrap(), strategy);
            assert_eq!(load(&second).unwrap(), strategy);
            assert_eq!(
                serde_json::from_slice::<Strategy>(&fs::read(file(&first)).unwrap()).unwrap(),
                strategy
            );
            assert_eq!(fs::read_dir(first.parent().unwrap()).unwrap().count(), 1);
        }
        assert!(!first.exists());
        assert!(!second.exists());
    }

    #[test]
    fn invalid_and_unreadable_preferences_are_reported_without_overwriting_them() {
        let directory = tempfile::tempdir().unwrap();
        let state = directory.path().join("workspace");
        for invalid in [b"not json".as_slice(), b"\"Unknown\"", b"null"] {
            fs::write(file(&state), invalid).unwrap();
            assert!(load(&state).is_err());
            assert_eq!(fs::read(file(&state)).unwrap(), invalid);
        }
        fs::remove_file(file(&state)).unwrap();
        fs::create_dir(file(&state)).unwrap();
        assert!(load(&state).is_err());
        assert!(save(&state, Strategy::Rebase).is_err());
        assert!(file(&state).is_dir());
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
    }

    #[test]
    fn save_reports_an_unavailable_application_directory() {
        let directory = tempfile::tempdir().unwrap();
        let application = directory.path().join("blocked");
        fs::write(&application, "keep").unwrap();
        assert!(save(&application.join("workspace"), Strategy::Rebase).is_err());
        assert_eq!(fs::read_to_string(application).unwrap(), "keep");
    }

    #[test]
    fn a_directory_without_a_parent_is_used_directly() {
        let root = std::env::current_dir()
            .unwrap()
            .ancestors()
            .last()
            .unwrap()
            .to_path_buf();
        assert_eq!(file(&root), root.join("update-strategy.json"));
    }
}
