"""Check shared Rust validation commands and cross-platform step parity."""

import os
import sys
import unittest
from pathlib import Path
from unittest.mock import patch

import yaml

ROOT = Path(__file__).resolve().parents[4]
sys.path.insert(0, str(ROOT / ".github/scripts/ci"))
from rust_validation_steps import build_release_binaries, set_up_toolchain, smoke_test_binaries, verify_host_activation


class RustValidationTests(unittest.TestCase):
    @patch("rust_validation_steps.run_checked")
    def test_setup_installs_pinned_toolchain_and_components(self, run_checked):
        with patch.dict(os.environ, {"RUSTUP_TOOLCHAIN": "1.90.0-x86_64-pc-windows-msvc"}):
            set_up_toolchain()
        self.assertEqual(
            [call.args for call in run_checked.call_args_list],
            [
                ("rustup", "toolchain", "install", "1.90.0-x86_64-pc-windows-msvc"),
                ("rustup", "component", "add", "clippy", "rustfmt", "--toolchain", "1.90.0-x86_64-pc-windows-msvc"),
            ],
        )

    @patch("rust_validation_steps.run_checked")
    def test_build_and_smoke_cover_both_binaries(self, run_checked):
        build_release_binaries()
        self.assertEqual([call.args[-1] for call in run_checked.call_args_list], ["work-cli", "tooling-cli"])
        for platform, suffix in (("darwin", ""), ("win32", ".exe")):
            with self.subTest(platform=platform), patch("rust_validation_steps.sys.platform", platform):
                run_checked.reset_mock()
                smoke_test_binaries()
                self.assertEqual(
                    [Path(call.args[0]).name for call in run_checked.call_args_list],
                    [f"work{suffix}", f"tooling{suffix}"],
                )
                run_checked.reset_mock()
                verify_host_activation()
                command = run_checked.call_args.args
                self.assertIn(f"target/release/work{suffix}", command)
                self.assertIn(f"target/release/tooling{suffix}", command)
                self.assertIn(".ci-perf/host-activation.json", command)

    def test_platform_jobs_have_the_same_steps_and_doc_test_gate(self):
        workflow = yaml.safe_load((ROOT / ".github/workflows/rust-validation.yml").read_text(encoding="utf-8"))
        mac = workflow["jobs"]["validate-and-build-rust-macos"]["steps"]
        windows = workflow["jobs"]["validate-and-build-rust-windows"]["steps"]
        self.assertEqual(
            [step.get("name", step.get("uses")) for step in mac],
            [step.get("name", step.get("uses")) for step in windows],
        )
        for name in ("Check out source for Rust validation", "Install cargo-nextest"):
            self.assertEqual(
                next(step for step in mac if step.get("name") == name),
                next(step for step in windows if step.get("name") == name),
            )
        for steps in (mac, windows):
            setup = next(step for step in steps if step.get("name") == "Set up Python for CI tests")
            self.assertEqual(setup["uses"], "./.github/actions/setup-rust-validation-python")
            for name in ("Report Rust performance", "Enforce Rust test results"):
                step = next(step for step in steps if step.get("name") == name)
                self.assertIn("--min-doc-tests 1", step["run"])

        setup_action = yaml.safe_load(
            (ROOT / ".github/actions/setup-rust-validation-python/action.yml").read_text(encoding="utf-8")
        )
        setup_steps = setup_action["runs"]["steps"]
        self.assertEqual(setup_steps[0]["uses"], "actions/setup-python@v7")
        self.assertEqual(setup_steps[0]["with"]["python-version"], "3.15")
        self.assertIn("PYYAML_FORCE_LIBYAML=0 python -m pip install PyYAML==6.0.3", setup_steps[1]["run"])
        self.assertIn("import yaml", setup_steps[1]["run"])


if __name__ == "__main__":
    unittest.main()
