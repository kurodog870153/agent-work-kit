"""Tests for release publication gates."""

import hashlib
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from release_gate import check_assets, check_immutable, expected_assets


class ReleaseGateTests(unittest.TestCase):
    def test_immutable_must_be_explicitly_enabled(self):
        with patch("release_gate.api", return_value={"enabled": True}):
            check_immutable("owner/repo")
        with patch("release_gate.api", return_value={"enabled": False}):
            with self.assertRaises(ValueError):
                check_immutable("owner/repo")

    def test_draft_requires_exact_three_assets_and_digests(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            assets = []
            for name in sorted(expected_assets("1.0.0")):
                data = name.encode("ascii")
                (root / name).write_bytes(data)
                assets.append({"name": name, "size": len(data), "digest": "sha256:" + hashlib.sha256(data).hexdigest()})
            release = {"tag_name": "v1.0.0", "draft": True, "prerelease": False, "assets": assets}
            with patch("release_gate.api", return_value=release):
                check_assets("owner/repo", "v1.0.0", root, published=False)
                with self.assertRaises(ValueError):
                    check_assets("owner/repo", "v1.0.0", root, published=True)
                assets[0]["digest"] = "sha256:" + "0" * 64
                with self.assertRaises(ValueError):
                    check_assets("owner/repo", "v1.0.0", root, published=False)


if __name__ == "__main__":
    unittest.main()
