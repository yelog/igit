import copy
import json
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

import publish


class PublishTests(unittest.TestCase):
    def test_publish_draft_then_resume_and_verify_without_replacing_assets(self):
        for prerelease in (False, True):
            with (
                self.subTest(prerelease=prerelease),
                tempfile.TemporaryDirectory() as temporary,
            ):
                root = Path(temporary)
                (root / "SHA256SUMS").write_text("fixture")
                (root / "igit.tar.gz").write_bytes(b"fixture archive")
                version = "0.1.0-beta.1" if prerelease else "0.1.0"
                tag = "v" + version
                info = {
                    "version": version,
                    "sha": "source-sha",
                    "prerelease": prerelease,
                }
                state = {"release": None}
                mutations = []

                def api(path, missing_ok=False, state=state):
                    if "/git/ref/" in path:
                        return {"object": {"type": "tag", "sha": "tag-sha"}}
                    if (
                        "/releases/tags/" in path
                        and state["release"]
                        and state["release"]["draft"]
                    ):
                        return None
                    return copy.deepcopy(state["release"])

                def gh(
                    *args,
                    mutations=mutations,
                    state=state,
                    tag=tag,
                    prerelease=prerelease,
                ):
                    if args[0] == "api":
                        return json.dumps(
                            [[state["release"]] if state["release"] else []]
                        )
                    mutations.append(args)
                    if args[:2] == ("release", "create"):
                        state["release"] = {
                            "id": 42,
                            "tag_name": tag,
                            "target_commitish": "source-sha",
                            "draft": True,
                            "prerelease": prerelease,
                            "assets": [],
                            "body": "Release notes\n",
                            "html_url": "https://example.invalid/release",
                        }
                    elif args[:2] == ("release", "upload"):
                        state["release"]["assets"] += [
                            {
                                "name": Path(path).name,
                                "digest": publish.digest(Path(path)),
                            }
                            for path in args[5:]
                        ]
                    elif args[:2] == ("release", "edit"):
                        state["release"]["draft"] = False

                with (
                    patch.object(publish, "validate", return_value=info),
                    patch.object(publish, "checksums"),
                    patch.object(publish, "git", return_value="tag-sha"),
                    patch.object(
                        publish, "release_notes", return_value="Release notes\n"
                    ),
                    patch.object(publish, "api", side_effect=api),
                    patch.object(publish, "gh", side_effect=gh),
                ):
                    publish.publish(root, tag, root, "yelog/igit")
                    self.assertFalse(state["release"]["draft"])
                    self.assertEqual(
                        [args[1] for args in mutations], ["create", "upload", "edit"]
                    )
                    self.assertIn("--verify-tag", mutations[0])
                    if prerelease:
                        self.assertIn("--latest=false", mutations[-1])
                    mutations.clear()
                    # The exact published release is read-only on resume.
                    publish.publish(root, tag, root, "yelog/igit")
                    self.assertEqual(mutations, [])
                    # An incomplete matching draft uploads only its missing asset.
                    state["release"]["draft"] = True
                    state["release"]["assets"] = state["release"]["assets"][:1]
                    publish.publish(root, tag, root, "yelog/igit")
                    self.assertEqual(
                        [args[1] for args in mutations], ["upload", "edit"]
                    )
                    self.assertEqual(len(mutations[0][5:]), 1)
                    self.assertNotIn("--clobber", mutations[0])
                    mutations.clear()
                    (root / "igit.tar.gz").write_bytes(b"different build")
                    with self.assertRaises(ValueError):
                        publish.publish(root, tag, root, "yelog/igit")
                    self.assertEqual(mutations, [])

    def test_find_release_uses_the_paginated_list_for_drafts(self):
        draft = {"tag_name": "v0.1.0", "draft": True, "id": 42}
        with (
            patch.object(publish, "api", return_value=None),
            patch.object(
                publish,
                "gh",
                return_value=json.dumps([[{"tag_name": "v0.0.1"}], [draft]]),
            ) as gh,
        ):
            self.assertEqual(publish.find_release("yelog/igit", "v0.1.0"), draft)
            gh.assert_called_once_with(
                "api", "--paginate", "--slurp", "repos/yelog/igit/releases?per_page=100"
            )

    def test_matching_assets_resume_only_missing_files(self):
        expected = {"a.tar.gz": "sha256:aaa", "SHA256SUMS": "sha256:bbb"}
        release = {"assets": [{"name": "a.tar.gz", "digest": "sha256:aaa"}]}
        self.assertEqual(
            publish.matching_assets(release, expected, "v0.1.0", "yelog/igit"),
            ["SHA256SUMS"],
        )

    def test_different_duplicate_and_unknown_assets_block_before_upload(self):
        for assets in (
            [{"name": "a.tar.gz", "digest": "sha256:wrong"}],
            [{"name": "unknown", "digest": "sha256:aaa"}],
            [{"name": "a.tar.gz", "digest": "sha256:aaa"}] * 2,
        ):
            with self.subTest(assets=assets), patch.object(publish, "gh") as gh:
                with self.assertRaises(ValueError):
                    publish.matching_assets(
                        {"assets": assets},
                        {"a.tar.gz": "sha256:aaa"},
                        "v0.1.0",
                        "yelog/igit",
                    )
                gh.assert_not_called()

    def test_missing_server_digest_downloads_and_compares_existing_asset(self):
        def download(*args):
            Path(args[-1], "a.tar.gz").write_bytes(b"existing")

        import hashlib

        expected = {"a.tar.gz": "sha256:" + hashlib.sha256(b"existing").hexdigest()}
        with patch.object(publish, "gh", side_effect=download):
            self.assertEqual(
                publish.matching_assets(
                    {"assets": [{"name": "a.tar.gz"}]}, expected, "v0.1.0", "yelog/igit"
                ),
                [],
            )

    def test_gh_failure_is_not_treated_as_a_missing_release(self):
        import subprocess

        for stderr in ("gh: forbidden (HTTP 403)", "connection timed out"):
            with (
                patch.object(
                    publish,
                    "gh",
                    side_effect=subprocess.CalledProcessError(1, "gh", stderr=stderr),
                ),
                self.assertRaises(subprocess.CalledProcessError),
            ):
                publish.api("repos/yelog/igit/releases/tags/v0.1.0", missing_ok=True)


if __name__ == "__main__":
    unittest.main()
