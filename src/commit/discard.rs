use super::*;
use backend::discard::Target;

impl CommitPanel {
    pub(super) fn discard_review(&mut self, c: &Context) -> Result<Action> {
        self.rollback = None;
        let row = self
            .active()
            .ok_or(Constraint("Select changes to roll back"))?;
        if !self.files.iter().any(|f| c.scope.contains(&f.root)) {
            bail!(Constraint("No changes to review"))
        }
        let mut targets = vec![];
        match &row.kind {
            Kind::File(i) => {
                let file = &self.files[*i];
                targets.push((file.root, Target::Files([file.change.path.clone()].into())));
            }
            Kind::Repo(root) => targets.push((*root, Target::Directory(Default::default()))),
            Kind::Directory(path) => {
                let roots: BTreeSet<_> = row.files.iter().map(|&i| self.files[i].root).collect();
                let first = *roots
                    .first()
                    .ok_or(Constraint("No changes in this directory"))?;
                let base = if self.options.modules {
                    &c.repos[first].path
                } else {
                    tree::directory_root(c)
                };
                let directory = base.join(path);
                for root in roots {
                    let repo = &c.repos[root].path;
                    let relative = if let Ok(path) = directory.strip_prefix(repo) {
                        path.to_path_buf()
                    } else if repo.starts_with(&directory) {
                        Default::default()
                    } else {
                        continue;
                    };
                    targets.push((root, Target::Directory(relative)));
                }
            }
            Kind::Group | Kind::Conflicts => {
                let mut paths = BTreeMap::<usize, BTreeSet<_>>::new();
                for &i in &row.files {
                    let file = &self.files[i];
                    if c.scope.contains(&file.root) {
                        paths
                            .entry(file.root)
                            .or_default()
                            .insert(file.change.path.clone());
                    }
                }
                targets.extend(
                    paths
                        .into_iter()
                        .map(|(root, paths)| (root, Target::Files(paths))),
                );
            }
        }
        let prechecked = targets.into_iter().collect();
        self.rollback = Some(Box::new(Rollback::new(self, c, &prechecked)));
        Ok(Action::ReviewRollback)
    }

    pub(crate) fn take_rollback(&mut self) -> Option<Box<Rollback>> {
        self.rollback.take()
    }
}
