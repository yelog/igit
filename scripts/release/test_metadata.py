import subprocess
import tempfile
import unittest
from pathlib import Path

import metadata

BODY = """### Added

- A release feature.

### Commits

- [`abcdef0`](https://github.com/yelog/igit/commit/abcdef0123456789) feat: release feature
"""


class MetadataTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        (self.root / "Cargo.toml").write_text(
            '[package]\nname = "igit-workbench"\nversion = "0.1.0"\n\n[dependencies]\nother = "1"\n',
            encoding="utf-8",
        )
        self.dependency = '\n[[package]]\nname = "other"\nversion = "0.1.0"\nsource = "registry+https://example.invalid"\nchecksum = "unchanged"\n'
        (self.root / "Cargo.lock").write_text(
            'version = 4\n\n[[package]]\nname = "igit-workbench"\nversion = "0.1.0"\ndependencies = ["other"]\n'
            + self.dependency,
            encoding="utf-8",
        )
        (self.root / "CHANGELOG.md").write_text(
            "# Changelog\n\n## Unreleased\n\n- Future work.\n", encoding="utf-8"
        )

    def git(self, *args):
        return metadata.git(
            self.root, "-c", "commit.gpgSign=false", "-c", "tag.gpgSign=false", *args
        )

    def repository(self):
        self.git("init", "-q", "-b", "main")
        self.git("config", "user.name", "Release Test")
        self.git("config", "user.email", "release@example.invalid")
        self.git("config", "core.hooksPath", str(self.root / "no-hooks"))
        self.commit("initial")

    def commit(self, message):
        self.git("add", ".")
        self.git("commit", "--allow-empty", "-qm", message)
        return self.git("rev-parse", "HEAD")

    def test_set_version_changes_only_the_local_package(self):
        metadata.set_version(self.root, "0.2.0-beta.3")
        self.assertEqual(metadata.cargo_version(self.root), "0.2.0-beta.3")
        self.assertTrue(
            (self.root / "Cargo.lock").read_text().endswith(self.dependency)
        )
        self.assertIn(
            '[dependencies]\nother = "1"', (self.root / "Cargo.toml").read_text()
        )

    def test_version_mismatch_and_invalid_version_leave_files_unchanged(self):
        lock = self.root / "Cargo.lock"
        lock.write_text(
            lock.read_text().replace('version = "0.1.0"', 'version = "0.2.0"', 1)
        )
        before = [
            (self.root / name).read_bytes() for name in ("Cargo.toml", "Cargo.lock")
        ]
        for version in ("0.3.0", "0.01.0", "0.2.0-beta.0"):
            with self.assertRaises(ValueError):
                metadata.set_version(self.root, version)
            self.assertEqual(
                before,
                [
                    (self.root / name).read_bytes()
                    for name in ("Cargo.toml", "Cargo.lock")
                ],
            )

    def test_changelog_insert_and_collapsed_notes(self):
        metadata.update_changelog(self.root, "0.1.0", BODY, "2026-11-06")
        text = (self.root / "CHANGELOG.md").read_text()
        self.assertLess(text.index("## [0.1.0]"), text.index("## Unreleased"))
        self.assertNotIn("<details>", text)
        notes = metadata.release_notes(self.root, "0.1.0")
        self.assertIn("<details>", notes)
        self.assertNotIn("Future work", notes)
        with self.assertRaises(ValueError):
            metadata.update_changelog(self.root, "0.1.0", BODY, "2026-11-07")
        self.assertEqual((self.root / "CHANGELOG.md").read_text(), text)

    def test_changelog_rejects_missing_commits_duplicates_and_invalid_dates(self):
        for body, date in [
            ("### Added\n\n- A change.\n", "2026-11-06"),
            (BODY + BODY, "2026-11-06"),
            (BODY, "2026-02-30"),
            (BODY.replace("abcdef0123456789", "0123456789abcdef"), "2026-11-06"),
        ]:
            with self.subTest(body=body, date=date), self.assertRaises(ValueError):
                metadata.update_changelog(self.root, "0.1.0", body, date)

    def test_exact_version_does_not_match_other_headings(self):
        text = "## [0x1x0] - 2026-11-06\n\n" + BODY
        with self.assertRaises(ValueError):
            metadata.changelog_section(text, "0.1.0")

    def test_collect_includes_initial_commit_and_uses_beta_stable_baselines(self):
        self.repository()
        self.assertEqual(
            len(metadata.collect_commits(self.root, "stable", "0.1.0")["commits"]), 1
        )
        self.git("tag", "-a", "v0.1.0", "-m", "stable")
        self.commit("feat: Unicode 名称")
        self.git("tag", "-a", "v0.1.1-beta.1", "-m", "beta")
        self.commit("fix: after beta")
        beta = metadata.collect_commits(self.root, "beta", "0.1.1")
        stable = metadata.collect_commits(self.root, "stable", "0.1.1")
        self.assertEqual(beta["baseline"], "v0.1.1-beta.1")
        self.assertEqual([c["subject"] for c in beta["commits"]], ["fix: after beta"])
        self.assertEqual(len(stable["commits"]), 2)
        self.assertEqual(stable["commits"][0]["subject"], "feat: Unicode 名称")

    def test_tag_validation_rejects_wrong_head_dirty_source_and_lightweight_tags(self):
        metadata.update_changelog(self.root, "0.1.0", BODY, "2026-11-06")
        self.repository()
        self.git("tag", "v0.1.0")
        self.git("update-ref", "refs/remotes/origin/main", "HEAD")
        with self.assertRaises(ValueError):
            metadata.validate(self.root, "v0.1.0")
        # Only this isolated fixture rewrites its test tag.
        self.git("tag", "-d", "v0.1.0")
        self.git("tag", "-a", "v0.1.0", "-m", "release")
        self.assertEqual(
            metadata.validate(self.root, "v0.1.0")["sha"], self.git("rev-parse", "HEAD")
        )
        (self.root / "unreviewed").write_text("work")
        with self.assertRaises(ValueError):
            metadata.validate(self.root, "v0.1.0")
        self.commit("new work")
        with self.assertRaises(ValueError):
            metadata.validate(self.root, "v0.1.0")

    def test_tag_must_be_reachable_from_origin_main(self):
        metadata.update_changelog(self.root, "0.1.0", BODY, "2026-11-06")
        self.repository()
        self.git("update-ref", "refs/remotes/origin/main", "HEAD")
        self.commit("not pushed to main")
        self.git("tag", "-a", "v0.1.0", "-m", "release")
        with self.assertRaises(subprocess.CalledProcessError):
            metadata.validate(self.root, "v0.1.0")
        self.assertEqual(
            metadata.validate(self.root, "v0.1.0", pre_tag=True)["version"], "0.1.0"
        )


if __name__ == "__main__":
    unittest.main()
