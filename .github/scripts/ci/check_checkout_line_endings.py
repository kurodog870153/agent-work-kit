"""Verify that checkout line endings match the repository policy."""

import re
import subprocess
from pathlib import Path


ROOT = Path(__file__).resolve().parents[3]
LF_PATHS = (
    "README.md",
    "skills/work/SKILL.md",
    "skills/tooling/SKILL.md",
    "rust/crates/work/work-infrastructure/fixtures/cases/discussion/assembly/valid/project/outputs/work/sources/example/SRC-001/source.txt",
)
CRLF_PATHS = (
    "os-scripts/windows/clean-codex-data.bat",
    "os-scripts/windows/install-agent-work-kit-from-source.bat",
    "os-scripts/windows/install-agent-work-kit-from-package.bat",
    "os-scripts/windows/install-agent-work-kit-from-github-release.bat",
)


def check_line_endings(root=ROOT):
    for expected, paths in (("lf", LF_PATHS), ("crlf", CRLF_PATHS)):
        for path in paths:
            result = subprocess.run(
                ["git", "ls-files", "--eol", "--", path],
                cwd=root,
                capture_output=True,
                text=True,
                check=True,
            )
            entry = result.stdout.strip()
            if not re.match(rf"^i/lf\s+w/{expected}\s", entry):
                raise ValueError(f"Checkout changed line endings for {path}: {entry or 'not tracked'}")


if __name__ == "__main__":
    check_line_endings()
