use super::*;
use crate::core::state::Native;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Default, Serialize, Deserialize)]
pub(crate) struct SavedSession {
    views: Vec<SavedView>,
    active: usize,
}

#[derive(Clone, Default, Serialize, Deserialize)]
struct SavedView {
    query: String,
    author: String,
    date: Option<model::DateFilter>,
    branch: Option<model::BranchFilter>,
    paths: Vec<(Option<Native>, Native)>,
    roots: Option<Vec<Native>>,
    regex: bool,
    case_sensitive: bool,
}

impl LogPanel {
    pub(crate) fn saved_session(&self, context: &Context) -> SavedSession {
        let mut views = self.views.clone();
        views[self.view_index] = self.snapshot();
        let active = self.view_index;
        let mut saved = Vec::new();
        let mut active_saved = 0;
        for (index, view) in views.into_iter().enumerate().take(16) {
            if view.filter.file_history.is_some() || view.filter.comparison.is_some() {
                continue;
            }
            if index == active {
                active_saved = saved.len();
            }
            saved.push(SavedView {
                query: view.filter.query,
                author: view.filter.author,
                date: view.filter.date,
                branch: view.filter.branch,
                paths: view
                    .filter
                    .paths
                    .0
                    .into_iter()
                    .filter_map(|(root, path)| {
                        let repo = match root {
                            Some(root) => Some(Native::new(&context.repos.get(root)?.path)),
                            None => None,
                        };
                        Some((repo, Native::new(&path)))
                    })
                    .collect(),
                roots: view.filter.roots.map(|roots| {
                    roots
                        .into_iter()
                        .filter_map(|root| context.repos.get(root))
                        .map(|repo| Native::new(&repo.path))
                        .collect()
                }),
                regex: view.regex,
                case_sensitive: view.case_sensitive,
            });
        }
        SavedSession {
            views: saved,
            active: active_saved,
        }
    }

    pub(crate) fn restore_session(
        &mut self,
        saved: &SavedSession,
        context: &Context,
    ) -> Result<()> {
        let find_root = |path: &Native| -> Option<usize> {
            let path = path.path().ok()?;
            context.repos.iter().position(|repo| repo.path == path)
        };
        let mut views = Vec::new();
        for view in saved.views.iter().take(16) {
            filters::matcher(&view.query, view.regex, view.case_sensitive)?;
            let paths: BTreeSet<_> = view
                .paths
                .iter()
                .filter_map(|(root, path)| {
                    Some((
                        match root {
                            Some(root) => Some(find_root(root)?),
                            None => None,
                        },
                        path.path().ok()?,
                    ))
                })
                .collect();
            let mut roots = view
                .roots
                .as_ref()
                .map(|roots| roots.iter().filter_map(find_root).collect());
            if !view.paths.is_empty() && paths.is_empty() {
                roots = Some(BTreeSet::new());
            }
            views.push(ViewState {
                filter: Filter {
                    query: view.query.clone(),
                    author: view.author.clone(),
                    date: view.date.clone(),
                    branch: view.branch.clone(),
                    paths: model::PathFilter(paths),
                    roots,
                    search: (!view.query.is_empty()).then(|| model::HistorySearch {
                        query: view.query.clone(),
                        regex: view.regex,
                        case_sensitive: view.case_sensitive,
                    }),
                    ..Default::default()
                },
                regex: view.regex,
                case_sensitive: view.case_sensitive,
                ..Default::default()
            });
        }
        if views.is_empty() {
            return Ok(());
        }
        self.view_index = saved.active.min(views.len() - 1);
        self.views = views;
        let view = &self.views[self.view_index];
        self.filter = view.filter.clone();
        self.regex = view.regex;
        self.case_sensitive = view.case_sensitive;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::Repo;

    #[test]
    fn log_filters_roundtrip_and_remap_paths_without_broadening_missing_roots() {
        let context = Context {
            repos: ["one", "two"]
                .into_iter()
                .map(|name| Repo {
                    path: std::env::temp_dir().join("igit-session-test").join(name),
                    name: name.into(),
                    branch: "main".into(),
                    submodule: false,
                    parent: None,
                })
                .collect(),
            scope: [0, 1].into(),
            workspace: std::env::temp_dir(),
            state_dir: std::env::temp_dir(),
        };
        let mut panel = LogPanel::default();
        panel.filter.query = "needle 中文".into();
        panel.filter.author = "me".into();
        panel.filter.date = Some(model::DateFilter::Last7Days);
        panel.filter.branch = model::BranchFilter::parse(
            "main | -release.*",
            &BTreeSet::from(["HEAD", "refs/heads/main", "refs/heads/release/old"]),
        )
        .unwrap();
        panel.filter.paths.0.insert((Some(1), "src/中文.rs".into()));
        panel.filter.roots = Some([1].into());
        panel.case_sensitive = true;
        let encoded = serde_json::to_vec(&panel.saved_session(&context)).unwrap();
        let saved = serde_json::from_slice(&encoded).unwrap();
        let mut reordered = context.clone();
        reordered.repos.swap(0, 1);
        let mut restored = LogPanel::default();
        restored.restore_session(&saved, &reordered).unwrap();
        assert_eq!(restored.filter.query, "needle 中文");
        assert_eq!(restored.filter.author, "me");
        assert_eq!(restored.filter.roots, Some([0].into()));
        assert_eq!(
            restored.filter.paths.0,
            [(Some(0), "src/中文.rs".into())].into()
        );
        assert_eq!(
            restored.filter.branch.as_ref().unwrap().text(false),
            panel.filter.branch.as_ref().unwrap().text(false)
        );
        assert!(restored.filter.search.is_some());
        reordered.repos.remove(0);
        restored.restore_session(&saved, &reordered).unwrap();
        assert_eq!(restored.filter.roots, Some(BTreeSet::new()));
        assert!(restored.filter.paths.is_empty());
    }
}
