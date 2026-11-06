use super::*;
use backend::stash::{self, Plan};
use std::path::PathBuf;

#[cfg(test)]
mod tests;

#[derive(Default)]
pub(super) struct State {
    message: String,
    plans: Vec<Plan>,
    errors: Vec<String>,
    // A published stash is a success even if Git could not clean the worktree.
    // Do not silently save that same residual content again on a batch retry.
    incomplete: BTreeMap<PathBuf, String>,
}

impl CommitPanel {
    pub(super) fn edit_stash(&mut self, c: &Context) -> Action {
        if c.scope.is_empty() {
            return err("No repositories in Roots");
        }
        self.stash.plans.clear();
        self.stash.errors.clear();
        self.buf = self.stash.message.clone();
        self.caret = self.buf.len();
        self.message_selection.clear();
        self.edit = Some(Edit::Stash);
        Action::None
    }

    pub(super) fn review_stash(&mut self, c: &Context) -> Result<Action> {
        let message = self.buf.trim();
        if message.is_empty() {
            bail!("Stash message is required");
        }
        self.stash.message = message.into();
        self.stash.plans.clear();
        self.stash.errors.clear();
        for &root in &c.scope {
            let repo = &c.repos[root];
            match stash::plan(root, &repo.path, &self.stash.message) {
                Ok(Some(plan)) => {
                    if let Some(hash) = self.stash.incomplete.get(&repo.path) {
                        self.stash.errors.push(format!(
                            "{}: stash {hash} is already saved; resolve the remaining working-tree changes before retrying", repo.name
                        ));
                    } else {
                        self.stash.plans.push(plan);
                    }
                }
                Ok(None) => {
                    self.stash.incomplete.remove(&repo.path);
                }
                Err(error) => self.stash.errors.push(format!("{}: {error}", repo.name)),
            }
        }
        if self.stash.plans.is_empty() {
            bail!(
                "{}",
                if self.stash.errors.is_empty() {
                    "No changes to stash in Roots".into()
                } else {
                    self.stash.errors.join("\n")
                }
            );
        }
        let mut body = format!(
            "Stash all staged, unstaged, and untracked changes in these repositories.\nCheckboxes do not limit this action. Ignored files stay in place.\nEach repository is saved independently, then its changes are cleaned.\n\nShared message: {}\n",
            self.stash.message
        );
        for error in &self.stash.errors {
            body.push_str(&format!("\nSKIP · {error}\n"));
        }
        for plan in &self.stash.plans {
            let repo = &c.repos[plan.root];
            body.push_str(&format!(
                "\n{} · {} · {} files\n{}\n",
                repo.name,
                plan.branch(),
                plan.files().len(),
                display_path(&plan.repo)
            ));
            for file in plan.files() {
                body.push_str(&format!(
                    "  {}{} {}\n",
                    file.xy[0] as char,
                    file.xy[1] as char,
                    file.label()
                ));
            }
            body.push_str(&plan.preview);
        }
        self.edit = None;
        Ok(Action::Confirm {
            title: format!("Review stash · {} repos", self.stash.plans.len()),
            body,
            token: "commit-stash".into(),
        })
    }

    pub(super) fn submit_stash(&mut self, c: &Context) -> Action {
        let plans = std::mem::take(&mut self.stash.plans);
        if plans.is_empty() {
            return Action::None;
        }
        let mut results: Vec<_> = std::mem::take(&mut self.stash.errors)
            .into_iter()
            .map(|error| format!("SKIP · {error}"))
            .collect();
        for plan in plans {
            let Some(repo) = c.repos.get(plan.root).filter(|repo| repo.path == plan.repo) else {
                results.push(format!(
                    "SKIP · {}: repository changed; review again",
                    display_path(&plan.repo)
                ));
                continue;
            };
            if !c.scope.contains(&plan.root) {
                results.push(format!("SKIP · {}: outside Roots", repo.name));
                continue;
            }
            match stash::execute(&plan) {
                Ok(outcome) => {
                    let short = &outcome.hash[..outcome.hash.len().min(12)];
                    if let Some(warning) = outcome.warning {
                        results.push(format!("SAVED · {} · {short}: {warning}. Resolve remaining changes before retrying", repo.name));
                        self.stash.incomplete.insert(plan.repo, outcome.hash);
                    } else {
                        results.push(format!("SAVED · {} · {short}", repo.name));
                    }
                }
                Err(error) => results.push(format!("FAILED · {}: {error}", repo.name)),
            }
        }
        self.focus = Focus::Tree;
        self.request_refresh();
        Action::Refresh(format!("Stash results\n{}", results.join("\n")))
    }
}
