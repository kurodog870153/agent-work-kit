"""Regression checks for the public Installer final-message contract."""
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[4]
sys.path.insert(0, str(Path(__file__).resolve().parent / "common"))
from installer_console import PROMPT, interactive_environment, run_console


class InstallerExitTests(unittest.TestCase):
    def setUp(self):
        self.windows = sys.platform == "win32"
        self.temp = tempfile.TemporaryDirectory(prefix="installer-exit-")
        self.addCleanup(self.temp.cleanup)
        root = Path(self.temp.name)
        platform, suffix = ("windows", "bat") if self.windows else ("mac", "command")
        self.directory = root / "os-scripts" / platform
        self.directory.mkdir(parents=True)
        self.source = self.directory / f"install-agent-work-kit-from-source.{suffix}"
        self.package = self.directory / f"install-agent-work-kit-from-package.{suffix}"
        for entry in (self.source, self.package):
            shutil.copyfile(ROOT / "os-scripts" / platform / entry.name, entry)
        install = self.directory / "installer/install"
        package = self.directory / "installer/package"
        install.mkdir(parents=True)
        package.mkdir(parents=True)
        if self.windows:
            (install / "install-core.ps1").write_text(
                'Write-Output "final message $env:INSTALLER_EXIT_TEXT"\n'
                'exit [int]$env:INSTALLER_EXIT_CODE\n', encoding="ascii")
            (package / "verify-integrity.ps1").write_text(
                '[Console]::Error.WriteLine("early package verification error")\nexit 1\n', encoding="ascii")
        else:
            (install / "install-core.sh").write_text(
                'installer_run() { printf "final message %s\\n" "$INSTALLER_EXIT_TEXT"; '
                'return "$INSTALLER_EXIT_CODE"; }\n', encoding="ascii")
            (package / "verify-integrity.sh").write_text(
                'verify_release_package() { printf "early package verification error\\n" >&2; return 1; }\n',
                encoding="ascii")
        self.env = interactive_environment()

    def test_source_success_failure_cancel_and_dry_run(self):
        for name, code, options in (("success", 0, ()), ("failure", 17, ()),
                                    ("cancelled", 1, ()), ("dry run", 0, ("--dry-run",))):
            with self.subTest(name=name):
                env = {**self.env, "INSTALLER_EXIT_TEXT": name, "INSTALLER_EXIT_CODE": str(code)}
                actual, data = run_console(self.source, options, env, expect_pause=True)
                self.assertEqual(code, actual)
                self.assertIn(f"final message {name}".encode(), data)

    def test_early_package_failure(self):
        code, data = run_console(self.package, (), self.env, expect_pause=True)
        self.assertEqual(1, code)
        self.assertIn(b"early package verification error", data)

    def test_noninteractive_ci_nested_and_redirected_input(self):
        env = {**self.env, "INSTALLER_EXIT_TEXT": "failure", "INSTALLER_EXIT_CODE": "17"}
        for options, extra in ((("--non-interactive",), {}), ((), {"CI": "true"}),
                               ((), {"GITHUB_ACTIONS": "true"}), ((), {"AGENT_WORK_KIT_INSTALLER_NESTED": "1"})):
            with self.subTest(options=options, extra=extra):
                code, data = run_console(self.source, options, {**env, **extra}, expect_pause=False)
                self.assertEqual(17, code)
                self.assertNotIn(PROMPT, data)
        command = ["cmd.exe", "/D", "/C", str(self.source)] if self.windows else ["/bin/bash", str(self.source)]
        result = subprocess.run(command, env=env, stdin=subprocess.DEVNULL, capture_output=True, timeout=15)
        self.assertEqual(17, result.returncode)
        self.assertNotIn(PROMPT, result.stdout)


if __name__ == "__main__":
    unittest.main()
