import copy
import hashlib
import subprocess
import tempfile
import unittest
from pathlib import Path

from distribution import TARGETS, archive_name

import pages

ROOT = Path(__file__).resolve().parents[2]


def release(version):
    names = sorted(archive_name(version, target) for target in TARGETS)
    checksums = "".join("%s  %s\n" % (hashlib.sha256(name.encode()).hexdigest(), name) for name in names)
    assets = []
    for name in [*names, "SHA256SUMS"]:
        digest = hashlib.sha256(checksums.encode() if name == "SHA256SUMS" else name.encode()).hexdigest()
        assets.append({
            "name": name, "state": "uploaded", "size": 100,
            "digest": "sha256:" + digest,
            "browser_download_url": "https://github.com/yelog/igit/releases/download/v%s/%s" % (version, name),
        })
    return {
        "tag_name": "v" + version, "draft": False, "prerelease": "-beta." in version,
        "published_at": "2026-11-06T09:11:22Z", "assets": assets,
    }, checksums


class PagesTests(unittest.TestCase):
    def test_channels_select_published_numeric_versions_independently(self):
        values = [release(version)[0] for version in ("0.1.0", "0.1.1-beta.2", "0.1.1-beta.10", "0.1.1-beta.9")]
        draft = release("0.2.0")[0]
        draft["draft"] = True
        selected = pages.select_releases([draft, *reversed(values)])
        self.assertEqual(selected["stable"]["tag_name"], "v0.1.0")
        self.assertEqual(selected["beta"]["tag_name"], "v0.1.1-beta.10")
        self.assertEqual(set(pages.select_releases([values[1]])), {"beta"})

    def test_manifest_uses_complete_verified_assets(self):
        value, checksums = release("0.1.0-beta.1")
        data = pages.manifest(value, checksums)
        self.assertEqual(set(data["assets"]), set(TARGETS))
        self.assertTrue(data["prerelease"])
        mutations = [
            lambda item: item["assets"].pop(),
            lambda item: item["assets"].append(item["assets"][0]),
            lambda item: item["assets"][0].update(browser_download_url="https://example.com/archive"),
            lambda item: item["assets"][0].update(digest="sha256:" + "0" * 64),
        ]
        for mutate in mutations:
            changed = copy.deepcopy(value)
            mutate(changed)
            with self.assertRaises(ValueError):
                pages.manifest(changed, checksums)
        with self.assertRaises(ValueError):
            pages.manifest(value, checksums + checksums.splitlines()[0] + "\n")

    def test_site_has_only_public_files_and_no_fabricated_stable_channel(self):
        value, checksums = release("0.1.0-beta.1")
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary) / "site"
            result = pages.assemble(ROOT, output, [value], {value["tag_name"]: checksums})
            self.assertEqual(result, [{"channel": "beta", "version": "0.1.0-beta.1"}])
            actual = {path.relative_to(output).as_posix() for path in output.rglob("*") if path.is_file()}
            self.assertEqual(actual, {"install.sh", "install-beta.sh", "install.ps1", "install.ps1.txt", "CNAME", "index.html", ".nojekyll", "channels/beta.json"})
            self.assertEqual((output / "install.ps1").read_bytes(), (output / "install.ps1.txt").read_bytes())
            self.assertEqual((output / "CNAME").read_text().strip(), "igit.yelog.org")
            with self.assertRaises(ValueError):
                pages.assemble(ROOT, output, [value], {value["tag_name"]: checksums})

    def test_public_shell_scripts_work_from_stdin_without_a_checkout(self):
        with tempfile.TemporaryDirectory() as directory:
            for channel in ("stable", "beta"):
                script = pages.unix_script(ROOT, channel)
                output = subprocess.run(["sh", "-s", "--", "--help"], input=script, text=True, encoding="utf-8", cwd=directory, capture_output=True, timeout=15)
                self.assertEqual(output.returncode, 0, output.stderr)
                self.assertIn("--install-dir", output.stdout)
                self.assertIn("--check", output.stdout)


if __name__ == "__main__":
    unittest.main()
