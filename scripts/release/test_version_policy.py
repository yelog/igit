"""Version policy tests; no repository mutation or network access."""

import importlib.util
import unittest
from pathlib import Path

spec = importlib.util.spec_from_file_location(
    "next_version", Path(__file__).with_name("next-version.py")
)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
recommend = module.recommend


class VersionPolicyTests(unittest.TestCase):
    def test_stable_rollover(self):
        for current, expected in [
            ("0.1.1", "0.1.2"),
            ("0.1.8", "0.1.9"),
            ("0.1.9", "0.2.0"),
            ("1.2.9", "1.3.0"),
        ]:
            with self.subTest(current=current):
                self.assertEqual(
                    recommend(["v" + current], "stable")["version"], expected
                )

    def test_numeric_baseline(self):
        result = recommend(["v0.1.9", "v0.2.0", "junk", "0.8.0"], "stable")
        self.assertEqual(result["baseline"], "v0.2.0")
        self.assertEqual(result["version"], "0.2.1")

    def test_beta_and_promotion(self):
        tags = ["v0.1.1", "v0.1.0-beta.20"]
        self.assertEqual(recommend(tags, "beta")["version"], "0.1.2-beta.1")
        tags += ["v0.1.2-beta.3", "v0.1.2-beta.9"]
        self.assertEqual(recommend(tags, "beta")["version"], "0.1.2-beta.10")
        self.assertEqual(recommend(tags, "stable")["version"], "0.1.2")

    def test_undefined_defaults(self):
        for tags in [
            [],
            ["v0.9.9"],
            ["v0.1.10"],
            ["v0.10.0"],
            ["v0.1.1", "v0.2.0-beta.1"],
        ]:
            for channel in ("stable", "beta"):
                with (
                    self.subTest(tags=tags, channel=channel),
                    self.assertRaises(ValueError),
                ):
                    recommend(tags, channel)

    def test_explicit_override(self):
        for tags, channel, override, expected in [
            ([], "stable", "0.1.0", "0.1.0"),
            (["v0.9.9"], "stable", "v1.0.0", "1.0.0"),
            (["v0.1.1"], "stable", "0.1.10", "0.1.10"),
            (["v0.1.1", "v0.2.0-beta.1"], "stable", "0.2.0", "0.2.0"),
        ]:
            with self.subTest(override=override):
                self.assertEqual(
                    recommend(tags, channel, override)["version"], expected
                )

    def test_rejected_overrides(self):
        tags = ["v0.1.1", "v0.1.2-beta.9"]
        for channel, version in [
            ("stable", "0.1.1"),
            ("stable", "0.1.0"),
            ("stable", "0.1.2-beta.10"),
            ("beta", "0.1.2"),
            ("beta", "0.1.2-beta.9"),
            ("beta", "0.1.2-beta.8"),
            ("stable", "0.01.2"),
            ("beta", "0.1.2-beta.0"),
        ]:
            with self.subTest(version=version), self.assertRaises(ValueError):
                recommend(tags, channel, version)


if __name__ == "__main__":
    unittest.main()
