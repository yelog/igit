import io
import tarfile
import tempfile
import unittest
from pathlib import Path

import distribution


class DistributionTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        (self.root / "Cargo.toml").write_text(
            '[package]\nname = "igit-workbench"\nversion = "0.1.0"\n'
        )
        (self.root / "Cargo.lock").write_text(
            '[[package]]\nname = "igit-workbench"\nversion = "0.1.0"\n'
        )
        for name in distribution.DOCUMENTS:
            path = self.root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(f"Fixture for {name}\n")
        self.binaries = self.root / "bin"
        self.binaries.mkdir()
        for name in ("igit", "ig", "igit.exe", "ig.exe"):
            (self.binaries / name).write_bytes(b"binary fixture: " + name.encode())
        self.output = self.root / "dist"

    def package(self, target):
        return distribution.package(
            self.root, self.binaries, self.output, target, "0.1.0"
        )

    def test_all_archives_contain_both_commands_and_are_repeatable(self):
        for target in distribution.TARGETS:
            with self.subTest(target=target):
                archive = self.package(target)
                contents = distribution.inspect_archive(archive, "0.1.0", target)
                for name in distribution.binary_names(target):
                    self.assertEqual(
                        contents[name], (self.binaries / name).read_bytes()
                    )
                repeated = distribution.package(
                    self.root, self.binaries, self.root / "again", target, "0.1.0"
                )
                self.assertEqual(archive.read_bytes(), repeated.read_bytes())
                with self.assertRaises(ValueError):
                    self.package(target)

    def test_missing_alias_or_wrong_version_blocks_packaging(self):
        (self.binaries / "ig").unlink()
        with self.assertRaises(ValueError):
            self.package(distribution.TARGETS[0])
        self.assertFalse(self.output.exists())
        with self.assertRaises(ValueError):
            distribution.package(
                self.root, self.binaries, self.output, distribution.TARGETS[0], "0.2.0"
            )

    def test_checksums_require_the_complete_inventory_and_detect_tampering(self):
        self.package(distribution.TARGETS[0])
        with self.assertRaises(ValueError):
            distribution.checksums(self.output, "0.1.0")
        for target in distribution.TARGETS[1:]:
            self.package(target)
        distribution.checksums(self.output, "0.1.0")
        distribution.checksums(self.output, "0.1.0", verify=True)
        sums = self.output / "SHA256SUMS"
        sums.write_text(sums.read_text().replace(sums.read_text()[:64], "0" * 64))
        with self.assertRaises(ValueError):
            distribution.checksums(self.output, "0.1.0", verify=True)
        (self.output / "unreviewed.txt").write_text("unexpected")
        with self.assertRaises(ValueError):
            distribution.checksums(self.output, "0.1.0")

    def test_archive_rejects_links_and_missing_execute_permissions(self):
        target = distribution.TARGETS[0]
        archive = self.package(target)
        entries = distribution.inspect_archive(archive, "0.1.0", target)
        for symlink in (False, True):
            with tarfile.open(archive, "w:gz") as bundle:
                for name, data in entries.items():
                    info = tarfile.TarInfo(f"igit_0.1.0_{target}/{name}")
                    info.size = len(data)
                    info.mode = 0o644 if name == "igit" else 0o755
                    if name == "igit" and symlink:
                        info.type = tarfile.SYMTYPE
                        info.linkname = "../../outside"
                    bundle.addfile(info, io.BytesIO(data))
            with self.assertRaises(ValueError):
                distribution.inspect_archive(archive, "0.1.0", target)


if __name__ == "__main__":
    unittest.main()
