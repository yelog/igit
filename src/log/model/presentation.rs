//! Optional, read-only metadata for Log presentation; never run from draw().
use super::*;

impl History {
    pub(in crate::log) fn read_presentation(
        &mut self,
        c: &Context,
        f: &Filter,
        signatures: bool,
        cherry: bool,
    ) {
        let roots: BTreeSet<_> = self.revs.iter().map(|r| r.root).collect();
        for root in roots {
            let Some(repo) = c.repos.get(root) else {
                continue;
            };
            if let Ok(ident) = git(&repo.path, &["var", "GIT_AUTHOR_IDENT"]) {
                if let Some((email, _)) =
                    ident.rsplit_once('<').and_then(|(_, s)| s.split_once('>'))
                {
                    self.identities.insert(root, email.to_owned());
                }
            }
            if signatures {
                if let Err(e) = self.read_signatures(&repo.path, root) {
                    self.errors
                        .push((root, format!("{} signatures: {e}", repo.name)));
                }
            }
            if let Some(branch) = f.single_branch().filter(|b| {
                cherry
                    && (b.starts_with("refs/heads/") || b.starts_with("refs/remotes/"))
                    && self.current.get(&root).map(String::as_str) != Some(*b)
            }) {
                match picked(&repo.path, branch) {
                    Ok(ids) => {
                        self.cherry_picked.insert(root, ids);
                    }
                    Err(e) => self
                        .errors
                        .push((root, format!("{} cherry: {e}", repo.name))),
                }
            }
        }
    }

    fn read_signatures(&mut self, path: &Path, root: usize) -> Result<()> {
        let ids: Vec<_> = self
            .revs
            .iter()
            .filter(|r| r.root == root)
            .map(|r| r.oid.clone())
            .collect();
        for chunk in ids.chunks(128) {
            let mut args = vec![
                "log",
                "--no-walk=unsorted",
                "--no-show-signature",
                "--no-notes",
                "--no-decorate",
                "--no-color",
                "--format=%H%x00%G?",
                "-z",
            ];
            args.extend(chunk.iter().map(String::as_str));
            args.push("--");
            let data = git_bytes(path, &args)?;
            for pair in data.split(|&b| b == 0).collect::<Vec<_>>().chunks_exact(2) {
                if let Some(&status) = pair[1].first() {
                    self.signatures
                        .insert((root, plain(pair[0])), char::from(status));
                }
            }
        }
        Ok(())
    }

    pub(in crate::log) fn is_mine(&self, r: &Rev) -> bool {
        !r.email.is_empty()
            && self
                .identities
                .get(&r.root)
                .is_some_and(|email| email.eq_ignore_ascii_case(&r.email))
    }

    pub(in crate::log) fn is_cherry_picked(&self, r: &Rev) -> bool {
        self.cherry_picked
            .get(&r.root)
            .is_some_and(|ids| ids.contains(&r.oid) || self.on_head(r))
    }

    pub(in crate::log) fn signature(&self, r: &Rev) -> (&'static str, char) {
        let status = self
            .signatures
            .get(&(r.root, r.oid.clone()))
            .copied()
            .unwrap_or('?');
        (
            match status {
                'G' => "Good",
                'B' => "Bad",
                'U' => "Untrusted",
                'X' => "Expired",
                'Y' => "Key exp.",
                'R' => "Revoked",
                'E' => "Unchecked",
                'N' => "Unsigned",
                _ => "Unknown",
            },
            status,
        )
    }
}

fn picked(path: &Path, branch: &str) -> Result<HashSet<String>> {
    // Resolve the reference before handing it to a command without --end-of-options.
    let tip = git(
        path,
        &[
            "rev-parse",
            "--verify",
            "--end-of-options",
            &format!("{branch}^{{commit}}"),
        ],
    )?;
    let data = git_bytes(path, &["cherry", "HEAD", tip.trim()])?;
    Ok(data
        .split(|&b| b == b'\n')
        .filter_map(|s| s.strip_prefix(b"- "))
        .map(plain)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::log::tests::{commit, git as g, init};

    #[test]
    fn presentation_keeps_both_dates_and_matches_identity_per_root() {
        let d = init();
        let p = d.path();
        let out = crate::core::command(p)
            .args(["commit", "--allow-empty", "-qm", "different dates"])
            .env("GIT_AUTHOR_DATE", "2025-01-02T01:02:00Z")
            .env("GIT_COMMITTER_DATE", "2026-03-04T03:04:00Z")
            .output()
            .unwrap();
        assert!(out.status.success());
        let copy = d.path().join("copy");
        g(
            p,
            &["clone", "-q", "--no-local", ".", copy.to_str().unwrap()],
        );
        g(&copy, &["config", "user.email", "someone@example.invalid"]);
        let c = Context::discover(p).unwrap();
        let mut h = load(&c, &Filter::default(), 20);
        h.read_presentation(&c, &Filter::default(), false, false);
        assert!(h.errors.is_empty(), "{:?}", h.errors);
        assert_eq!(h.revs.len(), 2);
        assert_eq!(h.revs[0].oid, h.revs[1].oid);
        for r in &h.revs {
            assert!(r.authored_date.starts_with("2025/1/2, "));
            assert!(r.date.starts_with("2026/3/4, "));
            assert_eq!(r.email, "test@example.invalid");
            assert_eq!(
                h.is_mine(r),
                c.repos[r.root].path == p.canonicalize().unwrap()
            );
            assert_eq!(h.signature(r).1, '?');
        }
    }

    #[test]
    fn cherry_highlight_uses_patch_equivalence_and_does_not_write_git_state() {
        let d = init();
        let p = d.path();
        commit(p, "base", "base");
        g(p, &["checkout", "-qb", "topic"]);
        commit(p, "picked", "already applied patch");
        let picked = g(p, &["rev-parse", "HEAD"]);
        commit(p, "pending", "still pending patch");
        let pending = g(p, &["rev-parse", "HEAD"]);
        g(p, &["checkout", "-q", "main"]);
        commit(p, "other", "main diverges");
        g(p, &["cherry-pick", &picked]);
        assert_ne!(g(p, &["rev-parse", "HEAD"]), picked);
        let before = [
            g(p, &["rev-parse", "HEAD"]),
            g(p, &["ls-files", "--stage"]),
            g(p, &["status", "--porcelain=v1"]),
        ];
        let c = Context::discover(p).unwrap();
        let f = Filter {
            branch: Some("refs/heads/topic".into()),
            ..Default::default()
        };
        let mut h = load(&c, &f, 20);
        h.read_presentation(&c, &f, false, true);
        assert!(h.errors.is_empty(), "{:?}", h.errors);
        for r in &h.revs {
            assert_eq!(h.is_cherry_picked(r), r.oid != pending);
        }
        assert!(h.revs.iter().any(|r| r.oid == picked));
        assert_eq!(
            before,
            [
                g(p, &["rev-parse", "HEAD"]),
                g(p, &["ls-files", "--stage"]),
                g(p, &["status", "--porcelain=v1"])
            ]
        );
    }

    #[test]
    fn signatures_are_loaded_only_when_requested_and_distinguish_signed_commits() {
        let d = init();
        let p = d.path();
        commit(p, "unsigned", "unsigned revision");
        let c = Context::discover(p).unwrap();
        let mut h = load(&c, &Filter::default(), 20);
        h.read_presentation(&c, &Filter::default(), false, false);
        assert!(h.signatures.is_empty());
        h.read_presentation(&c, &Filter::default(), true, false);
        assert!(h.errors.is_empty(), "{:?}", h.errors);
        assert_eq!(h.signature(&h.revs[0]), ("Unsigned", 'N'));

        // SSH signatures use Git's same %G? status path without a real user's keys.
        let key = p.join("test-signing-key");
        let Ok(out) = std::process::Command::new("ssh-keygen")
            .args(["-q", "-t", "ed25519", "-N", "", "-f"])
            .arg(&key)
            .output()
        else {
            return;
        };
        assert!(out.status.success());
        let public = std::fs::read_to_string(key.with_extension("pub")).unwrap();
        let signers = p.join("allowed-signers");
        std::fs::write(&signers, format!("test@example.invalid {public}")).unwrap();
        g(p, &["config", "gpg.format", "ssh"]);
        g(p, &["config", "user.signingkey", key.to_str().unwrap()]);
        g(
            p,
            &[
                "config",
                "gpg.ssh.allowedSignersFile",
                signers.to_str().unwrap(),
            ],
        );
        g(
            p,
            &["commit", "-S", "--allow-empty", "-qm", "signed revision"],
        );
        let mut h = load(&c, &Filter::default(), 20);
        h.read_presentation(&c, &Filter::default(), true, false);
        assert!(h.errors.is_empty(), "{:?}", h.errors);
        assert_eq!(h.signature(&h.revs[0]), ("Good", 'G'));
        assert_eq!(h.signature(&h.revs[1]), ("Unsigned", 'N'));
    }
}
