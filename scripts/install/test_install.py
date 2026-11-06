import copy
import importlib.util
import io
import json
import os
import tarfile
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("igit_installer", Path(__file__).with_name("install.py"))
installer = importlib.util.module_from_spec(spec)
spec.loader.exec_module(installer)


def manifest(version, digest="0" * 64):
    return {
        "schema": 1, "product": "igit", "channel": installer.channel_for(version),
        "version": version, "tag": "v" + version, "prerelease": "-beta." in version,
        "assets": {
            target: {
                "url": "https://github.com/yelog/igit/releases/download/v%s/igit_%s_%s%s" % (
                    version, version, target, ".zip" if "windows" in target else ".tar.gz",
                ),
                "sha256": digest,
            } for target in installer.TARGETS
        },
    }


class ManifestTests(unittest.TestCase):
    def test_channels_and_numeric_version_order(self):
        target = installer.TARGETS[0]
        for version in ("0.1.0", "0.1.0-beta.10"):
            data = manifest(version)
            self.assertEqual(installer.validate_manifest(data, data["channel"], target, installer.BASE_URL)[0], version)
        self.assertLess(installer.version_key("0.1.0-beta.9"), installer.version_key("0.1.0-beta.10"))
        self.assertLess(installer.version_key("0.1.0-beta.10"), installer.version_key("0.1.0"))
        with self.assertRaises(ValueError):
            installer.validate_manifest(manifest("0.1.0-beta.1"), "stable", target, installer.BASE_URL)

    def test_manifest_rejects_wrong_identity_target_and_download_source(self):
        original = manifest("0.1.0-beta.1")
        mutations = [
            lambda data: data.update(product="another-app"),
            lambda data: data.update(tag="v0.1.0"),
            lambda data: data.update(prerelease=False),
            lambda data: data.update(assets=list(installer.TARGETS)),
            lambda data: data["assets"].pop(installer.TARGETS[-1]),
            lambda data: data["assets"][installer.TARGETS[0]].update(sha256="invalid"),
            lambda data: data["assets"][installer.TARGETS[0]].update(url="https://github.com/another/project/file"),
        ]
        for mutate in mutations:
            data = copy.deepcopy(original)
            mutate(data)
            with self.assertRaises(ValueError):
                installer.validate_manifest(data, "beta", installer.TARGETS[0], installer.BASE_URL)
        for url in ("file:///tmp/manifest.json", "http://example.com/file", "https://user:secret@example.com/file", "https:"):
            self.assertFalse(installer.allowed_url(url))
        self.assertTrue(installer.allowed_url("http://127.0.0.1:1234/channels"))


@unittest.skipIf(os.name == "nt", "POSIX installer; Windows has native PowerShell fixtures")
class NativeTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="igit-installer-test-")
        self.addCleanup(self.temporary.cleanup)
        self.directory = Path(self.temporary.name).resolve()
        self.root = self.directory / "data [literal]"
        self.bin = self.directory / "bin [literal]"
        self.target = installer.host_target()

    def archive(self, version, bad_binary=False, extra=None):
        archive = self.directory / (version + ("-bad" if bad_binary else "") + ".tar.gz")
        prefix = "igit_%s_%s/" % (version, self.target)
        with tarfile.open(archive, "w:gz") as bundle:
            for name in ("igit", "ig"):
                reported = "0.0.0" if name == "ig" and bad_binary else version
                body = ("#!/bin/sh\nprintf '%s\\n' 'IGit %s — terminal Git workbench'\n" % ("%s", reported)).encode()
                info = tarfile.TarInfo(prefix + name)
                info.size, info.mode = len(body), 0o755
                bundle.addfile(info, io.BytesIO(body))
            if extra:
                bundle.addfile(extra)
        return archive

    def args(self, *extra):
        return installer.parser().parse_args([
            "--install-root", str(self.root), "--install-dir", str(self.bin),
            "--channel", "beta", "--no-modify-path", *extra,
        ])

    def run_version(self, version, args=None, bad_checksum=False, bad_binary=False):
        archive = self.archive(version, bad_binary)
        data = manifest(version, "0" * 64 if bad_checksum else installer.sha256(archive))

        def fetch(url, destination, limit=installer.MAX_ARCHIVE):
            if url.endswith(".json"):
                destination.write_text(json.dumps(data), encoding="utf-8")
            else:
                destination.write_bytes(archive.read_bytes())

        with patch.object(installer, "download", side_effect=fetch), patch.object(installer, "configure_path"):
            return installer.run(args or self.args())

    def snapshot(self):
        return {
            "current": os.readlink(self.root / "current"),
            "owner": (self.root / "install.json").read_bytes(),
            "igit": (self.bin / "igit").read_bytes(),
            "ig": (self.bin / "ig").read_bytes(),
        }

    def test_install_update_and_channel_persistence(self):
        self.assertEqual(self.run_version("0.1.0-beta.1")["status"], "installed")
        old = (self.root / "current").resolve()
        report = self.run_version("0.1.0-beta.2")
        self.assertEqual(report["status"], "updated")
        self.assertEqual(report["current_version"], "0.1.0-beta.1")
        self.assertTrue(old.is_dir())
        self.assertEqual(installer.inspect_installation(self.root)["channel"], "beta")
        for name in ("igit", "ig"):
            self.assertEqual((self.bin / name).resolve().parent, (self.root / "current").resolve())

    def test_check_is_read_only_and_missing_stable_is_not_beta(self):
        self.assertEqual(self.run_version("0.1.0-beta.1", self.args("--check"))["status"], "update_available")
        self.assertFalse(self.root.exists())
        self.assertFalse(self.bin.exists())
        self.run_version("0.1.0-beta.1")
        before = self.snapshot()
        self.assertEqual(self.run_version("0.1.0-beta.2", self.args("--check"))["status"], "update_available")
        self.assertEqual(self.snapshot(), before)
        with patch.object(installer, "download", side_effect=ValueError("HTTP 404")):
            with self.assertRaisesRegex(ValueError, "stable release channel"):
                installer.run(self.args("--channel", "stable"))
        self.assertEqual(self.snapshot(), before)

    def test_checksum_and_either_binary_failure_preserve_active_installation(self):
        self.run_version("0.1.0-beta.1")
        before = self.snapshot()
        for options in ({"bad_checksum": True}, {"bad_binary": True}):
            with self.assertRaises(ValueError):
                self.run_version("0.1.0-beta.2", **options)
            self.assertEqual(self.snapshot(), before)
        self.assertFalse((self.root / ".install.lock").exists())

    def test_foreign_or_modified_commands_are_never_replaced(self):
        self.bin.mkdir()
        foreign = self.bin / "ig"
        foreign.write_text("another application's command")
        with self.assertRaisesRegex(ValueError, "existing command"):
            self.run_version("0.1.0-beta.1")
        self.assertEqual(foreign.read_text(), "another application's command")
        self.assertFalse(self.root.exists())
        foreign.unlink()
        self.run_version("0.1.0-beta.1")
        (self.bin / "igit").write_text("modified")
        with self.assertRaisesRegex(ValueError, "installed file has changed"):
            self.run_version("0.1.0-beta.2")

    def test_traversal_and_symlinks_never_extract(self):
        for name, kind in (("../outside", tarfile.REGTYPE), ("igit_0.1.0-beta.1_%s/link" % self.target, tarfile.SYMTYPE)):
            entry = tarfile.TarInfo(name)
            entry.type = kind
            entry.linkname = "/tmp/outside"
            archive = self.archive("0.1.0-beta.1", extra=entry)
            with tempfile.TemporaryDirectory(dir=str(self.directory)) as destination:
                with self.assertRaises(ValueError):
                    installer.extract_archive(archive, Path(destination), "0.1.0-beta.1", self.target)
                self.assertEqual(list(Path(destination).iterdir()), [])

    def test_activation_failure_rolls_back_and_lock_is_not_stolen(self):
        self.run_version("0.1.0-beta.1")
        before = self.snapshot()
        with patch.object(installer, "activate", side_effect=OSError("rename denied")):
            with self.assertRaises(OSError):
                self.run_version("0.1.0-beta.2")
        self.assertEqual(self.snapshot(), before)
        lock = self.root / ".install.lock"
        lock.write_text("unknown owner")
        with self.assertRaisesRegex(ValueError, "another installer"):
            self.run_version("0.1.0-beta.2")
        self.assertEqual(lock.read_text(), "unknown owner")

    def test_downgrade_requires_explicit_opt_in(self):
        self.run_version("0.2.0-beta.1")
        before = self.snapshot()
        self.assertEqual(self.run_version("0.1.0-beta.1")["status"], "up_to_date")
        self.assertEqual(self.snapshot(), before)
        self.assertEqual(self.run_version("0.1.0-beta.1", self.args("--allow-downgrade"))["status"], "updated")

    def test_failed_first_activation_can_be_retried_with_nested_bin(self):
        self.bin = self.root / "bin"
        with patch.object(installer, "activate", side_effect=OSError("rename denied")):
            with self.assertRaises(OSError):
                self.run_version("0.1.0-beta.1")
        self.assertEqual(list(self.root.iterdir()), [])
        self.assertEqual(self.run_version("0.1.0-beta.1")["status"], "installed")

    def test_running_executable_must_belong_to_the_active_version(self):
        self.run_version("0.1.0-beta.1")
        old = (self.bin / "igit").resolve()
        args = self.args("--from-executable", str(old), "--running-version", "0.1.0-beta.1")
        args.channel = None
        self.assertEqual(self.run_version("0.1.0-beta.2", args)["channel"], "beta")
        before = self.snapshot()
        with self.assertRaisesRegex(ValueError, "not the active native release"):
            self.run_version("0.1.0-beta.3", args)
        self.assertEqual(before, self.snapshot())


if __name__ == "__main__":
    unittest.main()
