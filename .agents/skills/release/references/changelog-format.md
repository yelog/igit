# Release Notes

Use one exact dated heading per version and English Keep a Changelog categories:
Added, Changed, Fixed, Security, Deprecated, Removed, or Internal. Include at
least one summary category and a nonempty Commits list. Do not claim a feature
is delivered solely because it appears in a prototype or unverified PRD item.

```markdown
## [0.1.0-beta.1] - 2026-11-06

### Added

- Describe an implemented user-facing change.

### Commits

- [`abc1234`](https://github.com/yelog/igit/commit/abc1234) feat: describe the change
```

Use the actual SHAs from `metadata.py collect`; each collected commit appears
exactly once in Commits, including initial, merge, revert, documentation, test,
and CI commits. Full SHA link targets are preferable. Cross-check the collected
range before writing; the format validator checks links and duplicates, not
whether prose accurately describes every commit.

The tracked Changelog keeps Commits expanded. `metadata.py notes VERSION` wraps
only that subsection in a collapsed details block for GitHub Releases. Insert
the release above the first existing section, preserving Unreleased and older
entries. Clear or revise Unreleased bullets only when they are included in the
new release; do not erase future work.
