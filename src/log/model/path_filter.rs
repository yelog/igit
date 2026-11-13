use crate::{core::Context, ui::idea::clipped};
use anyhow::{bail, ensure, Result};
use std::{
    collections::BTreeSet,
    path::{Component, Path, PathBuf},
};
use unicode_width::UnicodeWidthStr;

/// An unqualified path applies in every root. A qualified empty path means that
/// whole repository, including commits which did not change any files.
#[derive(Clone, Default, Debug, Eq, PartialEq)]
pub(in crate::log) struct PathFilter(pub BTreeSet<(Option<usize>, PathBuf)>);

impl PathFilter {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// None skips this root; an empty argument list asks Git for its full history.
    pub fn for_root(&self, root: usize) -> Option<Vec<&Path>> {
        let mut paths = Vec::new();
        for (id, path) in &self.0 {
            if id.is_none_or(|i| i == root) {
                if path.as_os_str().is_empty() {
                    return Some(Vec::new());
                }
                paths.push(path.as_path());
            }
        }
        (self.is_empty() || !paths.is_empty()).then_some(paths)
    }

    // A repository prefix needs a slash: a bare name in Select is always a path.
    // "./" explicitly requests a path relative to each root, even for "web/src".
    fn split<'a>(line: &'a str, c: &Context) -> (Option<usize>, &'a str) {
        if let Some(literal) = line.strip_prefix("./") {
            return (None, literal);
        }
        c.repos
            .iter()
            .enumerate()
            .filter_map(|(id, r)| {
                let relative = line.strip_prefix(&r.name)?.strip_prefix('/')?;
                Some((Some(id), relative))
            })
            .min_by_key(|(_, rest)| rest.len())
            .unwrap_or((None, line))
    }

    pub fn parse(text: &str, c: &Context) -> Result<Self> {
        let mut paths = BTreeSet::new();
        for (n, line) in text.lines().enumerate().filter(|(_, s)| !s.is_empty()) {
            let (root, rest) = Self::split(line, c);
            ensure!(
                !line.contains('\0') && !Path::new(line).is_absolute(),
                "Line {}: enter a repository-relative path",
                n + 1
            );
            let mut path = PathBuf::new();
            for part in Path::new(rest).components() {
                match part {
                    Component::Normal(name) => path.push(name),
                    Component::CurDir => (),
                    _ => bail!("Line {}: enter a relative path without ..", n + 1),
                }
            }
            paths.insert((root, path));
        }
        Ok(Self(paths))
    }

    fn line(root: Option<usize>, path: &Path, c: &Context) -> String {
        let relative = path.to_string_lossy();
        match root {
            Some(id) => format!("{}/{}", c.repos[id].name, relative),
            None if relative.is_empty() => ".".into(),
            None if Self::split(&relative, c).0.is_some() => format!("./{relative}"),
            None => relative.into_owned(),
        }
    }

    pub fn text(&self, c: &Context) -> Result<String> {
        let mut lines = Vec::new();
        for (root, path) in &self.0 {
            let line = Self::line(*root, path, c);
            ensure!(
                path.to_str().is_some() && !line.contains(['\r', '\n']),
                "Some selected filenames cannot be represented as one path per line. Use Select in Tree"
            );
            lines.push(line);
        }
        Ok(lines.join("\n"))
    }

    pub fn label(&self, c: &Context) -> String {
        let Some((root, path)) = self.0.first() else {
            return String::new();
        };
        let line = Self::line(*root, path, c);
        let line = super::plain(line.as_bytes());
        match self.0.len() {
            1 => line,
            n => format!("{line} + {}", n - 1),
        }
    }

    /// Fill the available cells with whole path labels, then append a remaining
    /// count only when it fits. Clip the first label only when even it is too wide.
    pub fn recent_label(&self, c: &Context, width: usize) -> String {
        let labels: Vec<_> = self
            .0
            .iter()
            .map(|(root, path)| {
                let line = Self::line(*root, path, c);
                super::plain(line.as_bytes())
            })
            .collect();
        let Some(first) = labels.first() else {
            return String::new();
        };
        const SEPARATOR: &str = ", ";
        let mut used = 0;
        let mut shown = 0;
        for label in &labels {
            let next = used + label.width() + if shown == 0 { 0 } else { SEPARATOR.width() };
            if next > width {
                break;
            }
            used = next;
            shown += 1;
        }
        if shown == 0 {
            return clipped(first, width);
        }
        let mut text = labels[..shown].join(SEPARATOR);
        let hidden = labels.len() - shown;
        let overflow = format!(" + {hidden}");
        if hidden > 0 && used + overflow.width() <= width {
            text.push_str(&overflow);
        }
        text
    }
}
