use super::Action;
use std::path::{Path, PathBuf};

pub(crate) struct Target {
    absolute: PathBuf,
    relative: PathBuf,
    directory: bool,
}

impl Target {
    pub fn new(base: &Path, relative: &Path, directory: bool) -> Self {
        Self {
            absolute: if relative.as_os_str().is_empty() {
                base.to_path_buf()
            } else {
                base.join(relative)
            },
            relative: relative.to_path_buf(),
            directory,
        }
    }

    pub fn copy(&self, suffix: char) -> Action {
        let name = self
            .absolute
            .file_name()
            .unwrap_or(self.absolute.as_os_str());
        let (value, label) = match suffix {
            'y' => (self.absolute.as_os_str(), "absolute path"),
            'f' => (name, "name"),
            'n' => (
                if self.directory {
                    name
                } else {
                    self.absolute.file_stem().unwrap_or(name)
                },
                "name without extension",
            ),
            'r' => (
                if self.relative.as_os_str().is_empty() {
                    Path::new(".").as_os_str()
                } else {
                    self.relative.as_os_str()
                },
                "relative path",
            ),
            _ => return Action::None,
        };
        match value.to_str() {
            Some(text) => Action::Copy {
                text: text.to_owned(),
                label,
            },
            None => Action::Notice(
                super::NoticeLevel::Error,
                "Path cannot be represented as Unicode".into(),
            ),
        }
    }
}
