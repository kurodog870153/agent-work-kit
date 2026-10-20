"""Tests for the pull request release version check."""

import sys
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "release"))
from release_version_availability import check_version_available


class ReleaseVersionAvailabilityTests(unittest.TestCase):
    def test_available_version(self):
        with patch("release_version_availability.checked_version", return_value="1.0.2"), \
             patch("release_version_availability.tag_exists", return_value=False), \
             patch("release_version_availability.release_exists", return_value=False):
            self.assertEqual(check_version_available(Path("."), "owner/repo"), "v1.0.2")

    def test_existing_tag_or_release_fails(self):
        for existing_tag, existing_release in ((True, False), (False, True)):
            with self.subTest(tag=existing_tag, release=existing_release), \
                 patch("release_version_availability.checked_version", return_value="1.0.2"), \
                 patch("release_version_availability.tag_exists", return_value=existing_tag), \
                 patch("release_version_availability.release_exists", return_value=existing_release):
                with self.assertRaisesRegex(ValueError, "already used"):
                    check_version_available(Path("."), "owner/repo")


if __name__ == "__main__":
    unittest.main()
