---
name: release
description: Prepare, publish, or resume IGit beta and stable releases using this repository's version, Changelog, packaging, and GitHub Actions tooling. Use for an explicit IGit release request; editing this skill or CI does not authorize a release.
---

# IGit Release

Run from the IGit repository root. `$release beta`, `$release stable`, and plain
language release requests use the same process. Read
[Changelog guidance](references/changelog-format.md) when drafting notes and
[the release guide](../../../docs/releasing.md) for targets and recovery.
The repository scripts and workflows are the source of truth.

## Scope and authorization

- Prepare-only means local version/Changelog edits and checks, without commit,
  tag, push, or publication. An analysis-only request makes no edits.
- A new release starts with a clean worktree/index on `main`, tracking
  `origin/main` in `yelog/igit`. Stop for unrelated changes. Do not stash, reset,
  revert, merge, rebase, force-push, replace tags, or bundle application fixes.
- Preparation may change only `Cargo.toml`, `Cargo.lock`, and `CHANGELOG.md`.
  The package is `igit-workbench`; the two commands are `igit` and `ig`.
- Preserve authorization already given for the exact version and source scope.
  If publication needs confirmation, first prepare and validate the concrete
  release diff, then ask once for its release commit, annotated tag, main/tag
  pushes, and CI publication. Explain that the approval covers all those steps.
  Do not add separate commit/push approval gates. An exact, explicit publish
  request already authorizes those release steps unless the user narrows it.
- A channel-only request with no exact version needs the selected version
  confirmed before publication. A valid override selects that version, but
  reprepare and revalidate its diff before publishing. Never interpret an
  ambiguous reply as approval.

## Version policy (shared with LazyDB)

Run `python3 scripts/release/next-version.py CHANNEL` after fetching tags; use
`--override VERSION` for an explicit selection. Python 3.11+ is required for the
other release scripts. Do not duplicate version arithmetic in ad hoc commands.

- Starting at the highest published stable tag, increment PATCH below 9; at
  PATCH 9, reset it to 0 and increment MINOR (`0.1.9 -> 0.2.0`). Commit type and
  breaking changes inform notes and compatibility warnings, not the default bump.
- Beta uses the next stable base and increases the same-line Beta counter.
  Stable promotion drops the suffix without another bump.
- No stable tag, a major boundary (`0.9.9`), out-of-range decimal components, or
  conflicting unpublished Beta lines require an explicit version. For the
  initial release, use the maintainer's choice, such as `0.1.0` or
  `0.1.0-beta.1`; never invent a previous published version.
- Overrides must match the channel, advance the stable/Beta baseline, and be
  absent locally and remotely. Unexpected or unpublished baseline tags block
  preparation. Check the GitHub Release, not just the existence of a Git tag.

## Prepare and validate

1. Inspect status, index, branch/upstream, origin fetch/push URLs, tool versions,
   and GitHub authentication. Require Git, Rust 1.94.0 with rustfmt/clippy,
   Python 3.11+, and `gh`. Never print credentials. Record source HEAD.
2. Fetch `origin/main` and tags without changing user files. Record remote-main
   SHA. Require local main to contain remote main; inspect outgoing commits.
   Behind/diverged branches block the release rather than triggering a merge.
3. Calculate the version using `next-version.py`. Inspect the baseline's
   published release and ancestry. Check target tag absence with both local Git
   and `git ls-remote --tags origin refs/tags/vVERSION refs/tags/vVERSION^{}`.
4. Run `python3 scripts/release/metadata.py collect CHANNEL BASE_VERSION`.
   Inspect that JSON and the actual diff from its baseline; when baseline is
   null, inspect the whole initial history. Keep merge/revert/internal commits.
   First Beta uses the previous stable; later Betas use the previous same-line
   Beta; stable includes changes since the previous stable.
5. Draft concise English categorized notes and one complete `### Commits` list.
   Save the body to a temporary file outside the repository. Run:

   ```sh
   python3 scripts/release/metadata.py update-changelog VERSION BODY_FILE YYYY-MM-DD
   python3 scripts/release/metadata.py set-version VERSION
   python3 scripts/release/metadata.py validate --pre-tag vVERSION
   ```

   Pass the release date in the maintainer's timezone. The version helper edits
   only this package's manifest/lock entries; dependency upgrades are excluded.
6. Run checks sequentially for commands sharing Cargo's build directory:

   ```sh
   python3 -B -m unittest discover -s scripts/release -p 'test_*.py'
   python3 -B -m unittest discover -s scripts/install -p 'test_*.py'
   cargo fmt --all --check
   cargo clippy --locked --all-targets -- -D warnings
   cargo test --locked --all-targets
   cargo build --release --locked --bins
   ```

   Then package and smoke-test both native binaries with `distribution.py` and
   `scripts/install/smoke.py`, following the release guide. Use a temporary output directory; never replace
   the maintainer's installation or use their repositories. On Linux, build the
   native musl target used by CI instead of labeling a GNU binary as musl.
   Follow running command sessions to their actual exit status; yielding output
   is not completion or a timeout. A failed check blocks publication.
7. Inspect the complete three-file diff and staged state, verify source HEAD
   has not changed, and summarize the version, baseline, commit count, checks,
   repository, branch, and exact publication scope. Stop here for prepare-only.
   Obtain the single publication confirmation only if it is still missing.

## Commit, publish, and verify

1. Recheck local/remote tag absence and unchanged remote-main SHA. Stage only
   the three allowed files and inspect the staged diff. Create
   `chore(release): prepare vVERSION`, then annotated tag `vVERSION` with message
   `IGit vVERSION`. Verify its parent is the inspected source HEAD, its diff
   matches preparation, and the worktree/index are clean.
2. Push `main` to `origin`, then `vVERSION`. Stop on either failure; never force
   a push. The tag starts Release, which validates the tag's exact commit,
   reuses CI to test/build five targets, checks archives and SHA256SUMS, uploads
   to a draft, and publishes after verifying the complete uploaded asset set.
3. Identify `CI` and `Release` runs for the exact release commit/tag using `gh
   run list` / `gh run view`; wait on those run IDs. Release's reusable CI jobs
   are part of its own run. Do not use an older successful run as evidence.
   Use bounded polling and report failures/timeouts with the actual run URL.
4. Read the published release: exact tag, expected prerelease status, five
   target archives plus `SHA256SUMS`, and no draft flag. Download its assets to
   a temporary directory, run `distribution.py checksums DIR VERSION --verify`,
   and smoke-test the native archive if the host matches a release target.
5. Follow the Pages run triggered by that successful Release. Verify deployment
   and the public installation matrix, then confirm the matching channel at
   `https://igit.yelog.org/channels/CHANNEL.json` names the expected version and
   checksums. A preexisting site run is not evidence for the new release. Pages
   reads published Releases and never changes their tags or assets. Setup and
   recovery are in the release guide; missing DNS/HTTPS is an outstanding step,
   not a reason to alter the release. Old binaries such as `v0.1.0-beta.1` need
   installer-based checks; new CI archives must include the built-in updater.
6. Report complete only after the exact CI/Release runs, downloaded asset
   verification, and Pages installation checks pass. Include the release and
   run URLs. Otherwise report "pushed; release verification incomplete" with
   the outstanding step. Package registries and Homebrew taps are not configured.

## Resume and failures

Retain the authorized scope, version/channel, source HEAD, inspected remote-main
SHA, release commit/tag, push results, and run IDs in the conversation. On
resume, reconcile with Git/GitHub before skipping completed stages. Changes or
tags created by this authorized run are expected, not unrelated dirt.

- Verify prepared files belong to this run; never insert a duplicate Changelog
  section. Existing release commits must have the expected parent and diff;
  existing tags must be annotated and point to that commit.
- A matching remote release commit/tag permits continuing the remaining stages
  without another approval. A changed source, remote branch, channel, or scope
  blocks further writes under the old authorization.
- Publication resumes only missing assets in a matching draft. Different or
  unknown assets block it; already published matching assets are read-only.
  Never delete an asset or use `--clobber` to recover.
- A failed run is not permission to rerun publication, move tags, modify code,
  or bypass checks. Inspect and report the failed stage and completed writes.
  An explicitly authorized rerun must use the same tag; corrected source needs
  a new version. Manual dispatch checks out the tagged commit, never current main.
- On stop, leave completed work intact and report its state. Clean up only
  temporary files created by this release preparation.
