use super::*;

#[cfg(test)]
mod tests;

impl CommitPanel {
    pub(super) fn review_add(&mut self, c: &Context) -> Result<Action> {
        self.add_plans.clear();
        let row = self
            .active()
            .ok_or_else(|| anyhow::anyhow!("Select untracked files to add"))?;
        let mut files = BTreeMap::<usize, Vec<Change>>::new();
        for &i in &row.files {
            let file = &self.files[i];
            if c.scope.contains(&file.root) && file.change.xy == *b"??" && file.change.selectable()
            {
                files
                    .entry(file.root)
                    .or_default()
                    .push(file.change.clone());
            }
        }
        if files.is_empty() {
            bail!("No untracked files in the highlighted scope");
        }
        let mut plans = Vec::new();
        let mut body = String::from(
            "Stage the current contents of these untracked files.\nScope comes from the highlighted row, independently of commit checkboxes.\nWorking files and existing staged changes stay in place.\n",
        );
        let mut total = 0;
        for (root, files) in files {
            let repo = &c.repos[root];
            let plan = backend::add::review(root, &repo.path, files)
                .map_err(|error| anyhow::anyhow!("{}: {error}", repo.name))?;
            total += plan.files.len();
            body.push_str(&format!(
                "\n{} · {} files\n{}\n",
                repo.name,
                plan.files.len(),
                display_path(&repo.path)
            ));
            for file in &plan.files {
                body.push_str(&format!("  + {}\n", file.label()));
            }
            plans.push(plan);
        }
        self.add_plans = plans;
        Ok(Action::Confirm {
            title: format!("Add to Git · {total} files"),
            body,
            token: "commit-add".into(),
        })
    }

    pub(super) fn submit_add(&mut self, c: &Context) -> Action {
        let plans = std::mem::take(&mut self.add_plans);
        if plans.is_empty() {
            return Action::None;
        }
        let mut results = Vec::new();
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
            match backend::add::execute(&plan) {
                Ok(()) => results.push(format!(
                    "ADDED · {} · {} files",
                    repo.name,
                    plan.files.len()
                )),
                Err(error) => results.push(format!("FAILED · {}: {error}", repo.name)),
            }
        }
        self.request_refresh();
        Action::Refresh(format!("Add to Git results\n{}", results.join("\n")))
    }
}
