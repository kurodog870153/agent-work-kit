"""Check public installer entries at the release boundary."""

import csv
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[4]


class InstallerEntryTests(unittest.TestCase):
    def test_both_parsers_use_one_option_catalog(self):
        catalog = ROOT / "os-scripts/common/installer/options.tsv"
        with catalog.open(encoding="utf-8", newline="") as stream:
            rows = list(csv.DictReader(stream, delimiter="\t"))
        self.assertEqual({"kind", "name", "path"}, set(rows[0]))
        for kind in ("option", "skill"):
            names = [row["name"] for row in rows if row["kind"] == kind]
            self.assertTrue(names)
            self.assertEqual(len(names), len(set(names)))
        self.assertTrue(all(row["kind"] in ("option", "skill") for row in rows))
        for platform, suffix in (("mac", "sh"), ("windows", "ps1")):
            parser = ROOT / "os-scripts" / platform / "installer/cli" / f"parse-options.{suffix}"
            self.assertIn("options.tsv", parser.read_text(encoding="utf-8"))

    def test_only_formal_entries_are_exposed(self):
        for platform, suffix in (("mac", "command"), ("windows", "bat")):
            directory = ROOT / "os-scripts" / platform
            for source in ("source", "package", "github-release"):
                self.assertTrue((directory / f"install-work-from-{source}.{suffix}").is_file())
            for obsolete in (f"install-work.{suffix}", f"install-release.{suffix}"):
                self.assertFalse((directory / obsolete).exists(), obsolete)

    def test_source_and_package_entries_use_shared_core(self):
        mac = ROOT / "os-scripts" / "mac"
        source = (mac / "install-work-from-source.command").read_text(encoding="utf-8")
        package = (mac / "install-work-from-package.command").read_text(encoding="utf-8")
        core = (mac / "installer" / "runtime" / "install.sh").read_text(encoding="utf-8")
        transaction = (mac / "installer" / "transaction" / "work.sh").read_text(encoding="utf-8")
        self.assertIn("installer_run source", source)
        self.assertIn("verify_release_package", package)
        self.assertIn("installer_run package", package)
        self.assertIn("work-install-lock", core)
        self.assertIn("execute-locked.sh", core)
        self.assertIn("prepared", transaction)
        self.assertIn("previous", transaction)

        windows = ROOT / "os-scripts" / "windows"
        source = (windows / "install-work-from-source.bat").read_text(encoding="utf-8")
        package = (windows / "install-work-from-package.bat").read_text(encoding="utf-8")
        core = (windows / "installer" / "runtime" / "install.ps1").read_text(encoding="utf-8")
        transaction = (windows / "installer" / "transaction" / "work.ps1").read_text(encoding="utf-8")
        self.assertIn("install.ps1", source)
        self.assertIn("verify-integrity.ps1", package)
        self.assertIn("install.ps1", package)
        self.assertIn("Invoke-InstallerLocked", core)
        self.assertIn("cargo build --release --locked", core)
        self.assertIn("prepared", transaction)
        self.assertIn("previous", transaction)

    def test_windows_entries_use_crlf(self):
        for source in ("source", "package", "github-release"):
            path = ROOT / "os-scripts" / "windows" / f"install-work-from-{source}.bat"
            data = path.read_bytes()
            self.assertIn(b"\r\n", data)
            self.assertNotIn(b"\n", data.replace(b"\r\n", b""))


if __name__ == "__main__":
    unittest.main()
