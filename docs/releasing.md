# Releasing IGit

The repository's [release skill](../.agents/skills/release/SKILL.md) prepares
beta/stable versions, checks the source, and coordinates publication through
GitHub Actions. This adapts LazyDB's version and Changelog workflow to IGit's
two commands and system-Git runtime. Creating this automation does not publish
the current `0.1.0` source.

## Maintainer entry points

In a coding agent that discovers `.agents/skills`, use `$release beta` or
`$release stable`. For the first release, supply the exact version:
`$release stable 0.1.0` or `$release beta 0.1.0-beta.1`. Add `prepare only` to
stop after local edits and checks. Other agents can read the same SKILL.md.

Start on clean `main` tracking `origin/main` in `yelog/igit`, with Git, `gh`,
Python 3.11+, and the Rust toolchain from `rust-toolchain.toml`. GitHub CLI must
be authenticated when publishing. Never include unrelated worktree changes.
Preparation changes only `Cargo.toml`, `Cargo.lock`, and `CHANGELOG.md`.

Versions follow LazyDB's decimal rollover policy: `0.1.8 -> 0.1.9 -> 0.2.0`.
Beta counters advance independently; stable promotion removes the Beta suffix.
No previous stable tag, a major boundary, or conflicting Beta lines requires
an explicit version. The read-only `next-version.py` helper validates choices;
the maintainer also verifies the baseline is a published GitHub Release.

```sh
python3 scripts/release/next-version.py stable --override 0.1.0
python3 scripts/release/metadata.py collect stable 0.1.0
python3 scripts/release/metadata.py info
```

After notes and versions are prepared, `metadata.py validate --pre-tag
vVERSION` checks the exact dated Changelog section and Cargo/lock consistency.
The default `validate vVERSION` additionally requires an annotated tag at HEAD,
a clean source tree, and ancestry on `origin/main`. It does not create tags.

## CI and release artifacts

[CI](../.github/workflows/ci.yml) runs on pull requests, main pushes, and manual
dispatch. It checks formatting, Clippy, release tooling, and Rust tests, then
builds both binaries and smoke-tests each extracted native archive.
[Release](../.github/workflows/release.yml) runs on `v*` tags or manual dispatch
with an existing tag. It validates that tag and invokes the same CI workflow
against its resolved commit SHA, then publishes those exact artifacts. It
never bumps versions or builds current main in place of a requested tag.

| Platform | Rust target | Runner | Archive |
| --- | --- | --- | --- |
| macOS Intel | `x86_64-apple-darwin` | `macos-15-intel` | `.tar.gz` |
| macOS Apple Silicon | `aarch64-apple-darwin` | `macos-15` | `.tar.gz` |
| Linux x86_64 | `x86_64-unknown-linux-musl` | `ubuntu-24.04` | `.tar.gz` |
| Linux ARM64 | `aarch64-unknown-linux-musl` | `ubuntu-24.04-arm` | `.tar.gz` |
| Windows x86_64 | `x86_64-pc-windows-msvc` | `windows-2025` | `.zip` |

The matrix describes configured build/test coverage. Terminal-specific keyboard,
mouse, clipboard, and Kitty integration still need interactive verification on
the target platform. Linux binaries use musl to avoid a host glibc dependency.
Git must be installed separately; optional AI requests also need system curl.
Windows ARM64, package managers, installers, self-updates, code signing, and
notarization are outside this release workflow.

Archives are named `igit_VERSION_TARGET.tar.gz` or `.zip`, with one directory
of the same base name. Each contains `igit` and `ig` (`.exe` on Windows),
README, Changelog, default configuration, the configuration guide, and the
optional Kitty resize helper. Install both commands into a directory on PATH.
Every published release contains exactly five archives and `SHA256SUMS`.
Compare your downloaded archive's SHA-256 with its entry before installing;
use `shasum -a 256 FILE` on macOS, `sha256sum FILE` on Linux, or
`Get-FileHash FILE -Algorithm SHA256` in PowerShell.

The Actions jobs need only the built-in `GITHUB_TOKEN`: read-only for planning
and testing, `contents: write` for publication. No repository secret, model
endpoint, or external tap is required. Action revisions and Rust are pinned.
Runner labels follow [GitHub's hosted runner reference](https://docs.github.com/en/actions/reference/runners/github-hosted-runners).

## Local verification

```sh
python3 -B -m unittest discover -s scripts/release -p 'test_*.py'
cargo fmt --all --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked --all-targets
cargo build --release --locked --bins
```

For a native macOS smoke test (choose the matching target for Intel):

```sh
release_dist="$(mktemp -d)"
release_version="$(python3 scripts/release/metadata.py version)"
release_archive="$(python3 scripts/release/distribution.py package aarch64-apple-darwin --bin-dir target/release --output "$release_dist")"
python3 scripts/release/distribution.py smoke "$release_archive" "$release_version" aarch64-apple-darwin
```

On Linux, install the native musl target and linker, then build with `--target
x86_64-unknown-linux-musl` or `--target aarch64-unknown-linux-musl`; pass
`target/TARGET/release` to packaging. On Windows use the MSVC target and the
matching `.exe` files. Do not label a host GNU binary as a musl build.

The smoke helper reads the final archive, runs both commands with `--version`,
`--help`, `--print-default-config`, and `--check-config`, and discovers a new
temporary Git repository. It does not require a terminal or write to the user's
configuration, repositories, or installation. Tests use temporary fixtures and
mock GitHub publication; they never publish releases.

With all five downloaded archives in a directory, validate the complete asset
set with `python3 scripts/release/distribution.py checksums DIR VERSION
--verify`. Without `--verify`, that command generates SHA256SUMS, so use it only
while assembling a release, not to verify a downloaded checksum file.

## Publication and recovery

The release skill creates a release commit and annotated `vVERSION` tag only
within the user's authorized scope, pushes main before the tag, and waits for
the exact CI/Release runs. The publisher uses
[`gh release create --verify-tag`](https://cli.github.com/manual/gh_release_create)
to create a draft. It uploads missing assets, verifies remote SHA-256 values
(downloading assets if the API has no digest), and only then publishes.
Beta releases are prereleases and are never marked Latest.

Manual dispatch resumes an existing tag. A matching draft can receive missing
assets; differing or unknown assets block the run. Existing published releases
are verified without changes. The publisher never uses
[`--clobber`](https://cli.github.com/manual/gh_release_upload), replaces a tag,
or silently rebuilds an old version from new source.

If a source or workflow correction is needed, commit it through the normal
development process and use a new release version. A retry of the same immutable
tag is appropriate only for an authorized transient-failure recovery. Inspect
actual refs, draft assets, and run logs before retrying a timed-out operation.
Report "pushed; release verification incomplete" until the exact workflows and
downloaded assets are verified; a successful push alone is not a completed release.
