"""Fail-closed identity checks for the Windows Standard User CI entry."""
import sys
from pathlib import Path
import unittest
from unittest.mock import patch
import tempfile

RELEASE = Path(__file__).resolve().parents[2] / "release"
sys.path.insert(0, str(RELEASE))
from permission_smoke import ADMIN_SID, USERS_SID, validate_identity, windows_smoke


class StandardUserIdentityTests(unittest.TestCase):
    def test_expected_account_and_loaded_profile(self):
        validate_identity("S-1-5-21-1", "S-1-5-21-1", {USERS_SID},
                          False, r"C:\Users\awk-ci", r"c:\users\AWK-CI")

    def test_reject_wrong_account_group_or_profile(self):
        for actual, groups, is_admin, profile in (
            ("S-1-5-21-2", {USERS_SID}, False, r"C:\Users\awk-ci"),
            ("S-1-5-21-1", {USERS_SID, ADMIN_SID}, False, r"C:\Users\awk-ci"),
            ("S-1-5-21-1", {USERS_SID}, True, r"C:\Users\awk-ci"),
            ("S-1-5-21-1", set(), False, r"C:\Users\awk-ci"),
            ("S-1-5-21-1", {USERS_SID}, False, r"C:\Users\runneradmin"),
            ("S-1-5-21-1", {USERS_SID}, False, ""),
        ):
            with self.subTest(actual=actual, groups=groups, is_admin=is_admin, profile=profile):
                with self.assertRaises(AssertionError):
                    validate_identity("S-1-5-21-1", actual, groups, is_admin, profile,
                                      r"C:\Users\awk-ci")


    def test_windows_entry_runs_shared_smoke_only_after_identity_check(self):
        with tempfile.TemporaryDirectory() as directory:
            stage = Path(directory)
            home = stage / "home"
            with patch("permission_smoke.validate_windows_environment", return_value=home) as identity, \
                    patch("permission_smoke.run_permission_smoke") as smoke:
                self.assertEqual(0, windows_smoke(stage, "expected-sid", "commit"))
                identity.assert_called_once_with("expected-sid", "commit")
                smoke.assert_called_once_with(stage, home, "windows-x64-msvc")
            self.assertTrue((stage / "acceptance.log").is_file())

    def test_windows_entry_rejects_identity_failure_before_installation(self):
        with tempfile.TemporaryDirectory() as directory:
            stage = Path(directory)
            with patch("permission_smoke.validate_windows_environment",
                       side_effect=AssertionError("wrong user")) as identity, \
                    patch("permission_smoke.run_permission_smoke") as smoke:
                self.assertEqual(1, windows_smoke(stage, "expected-sid", "commit"))
                identity.assert_called_once_with("expected-sid", "commit")
                smoke.assert_not_called()
            self.assertIn("wrong user", (stage / "acceptance.log").read_text(encoding="utf-8"))


if __name__ == "__main__":
    unittest.main()
