"""The same real Package permission smoke test on Windows and macOS."""
import csv
from contextlib import redirect_stdout, redirect_stderr
import json
import ntpath
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

from release_install_e2e import (
    HostLayout, assert_installation, assert_success, extract_checked,
    run_installer_for_host, verify_readme_download,
)

# Stage only these test modules and the representative release archive.
MODULES = ("permission_smoke.py", "release_install_e2e.py",
           "release_package_integrity.py", "rust_toolchain.py")


def stage_package(repository, stage, platform):
    repository, stage = Path(repository), Path(stage)
    if not stage.is_dir() or any(stage.iterdir()):
        raise ValueError("permission smoke staging must be empty")
    version = (repository / "VERSION").read_text(encoding="utf-8").strip()
    archive = repository / "dist" / f"agent-work-kit-v{version}-codex-{platform}.zip"
    checksums = repository / "dist/SHA256SUMS.txt"
    verify_readme_download(archive, checksums, platform)
    shutil.copyfile(archive, stage / archive.name)
    shutil.copyfile(checksums, stage / checksums.name)
    release = stage / "release"
    release.mkdir()
    modules = MODULES + (("windows_job.py",) if platform == "windows-x64-msvc" else ())
    for name in modules:
        shutil.copyfile(Path(__file__).with_name(name), release / name)
    print(f"Staged representative Codex package: {archive.name}", flush=True)


def run_permission_smoke(stage, home, platform):
    stage, home = Path(stage).resolve(), Path(home).resolve()
    if not home.is_dir() or any((home / name).exists() for name in (".agents", ".codex", ".claude")):
        raise AssertionError("permission smoke requires a fresh test home")
    archives = list(stage.glob(f"agent-work-kit-v*-codex-{platform}.zip"))
    if len(archives) != 1:
        raise AssertionError("permission smoke requires exactly one Codex ZIP")
    archive = archives[0]
    verify_readme_download(archive, stage / "SHA256SUMS.txt", platform)
    with tempfile.TemporaryDirectory(prefix="package-", dir=stage) as directory:
        package = Path(directory)
        extract_checked(archive, package)
        info = json.loads((package / "release-info.json").read_text(encoding="utf-8"))
        if info["host"] != "codex":
            raise AssertionError("wrong representative Host")
        suffix = ".bat" if platform == "windows-x64-msvc" else ".command"
        script = package / f"install-agent-work-kit-from-package{suffix}"
        layout = HostLayout(home, "codex", platform)
        options = ("--non-interactive", "--global-instructions", "yes")
        for attempt in (1, 2):
            result = run_installer_for_host(script, home, options, host="codex")
            assert_success(result)
            assert_installation(layout, info, global_instructions=True)
            snapshot = layout.snapshot()
            if attempt == 1:
                first = snapshot
            elif snapshot != first:
                raise AssertionError("permission smoke reinstall changed installed resources")
    print(f"Package permission smoke passed: {platform}, Codex, install and reinstall.", flush=True)


ADMIN_SID = "S-1-5-32-544"
USERS_SID = "S-1-5-32-545"


def validate_identity(expected_sid, actual_sid, groups, is_admin, profile, registered_profile):
    if actual_sid != expected_sid or is_admin or ADMIN_SID in groups or USERS_SID not in groups:
        raise AssertionError("acceptance process is not the expected local Standard User")
    if not profile or not registered_profile or (
            ntpath.normcase(ntpath.normpath(profile)) !=
            ntpath.normcase(ntpath.normpath(registered_profile))):
        raise AssertionError("acceptance process does not have its own loaded User Profile")


def whoami_rows(option):
    output = subprocess.run(["whoami.exe", option, "/fo", "csv", "/nh"],
                            stdin=subprocess.DEVNULL, capture_output=True, check=True, timeout=30).stdout
    return list(csv.reader(output.decode("utf-8", errors="replace").splitlines()))


def validate_windows_environment(expected_sid, commit):
    import ctypes
    from ctypes import wintypes
    import winreg
    shell = ctypes.WinDLL("shell32", use_last_error=True)
    shell.IsUserAnAdmin.restype = wintypes.BOOL
    profile = os.environ.get("USERPROFILE", "")
    with winreg.OpenKey(winreg.HKEY_LOCAL_MACHINE,
                        r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\ProfileList" + "\\" + expected_sid) as key:
        registered, _ = winreg.QueryValueEx(key, "ProfileImagePath")
    registered = os.path.expandvars(registered)
    user = whoami_rows("/user")
    if len(user) != 1 or len(user[0]) < 2:
        raise AssertionError("cannot determine acceptance account SID")
    groups = {row[2] for row in whoami_rows("/groups") if len(row) >= 3}
    validate_identity(expected_sid, user[0][1], groups, bool(shell.IsUserAnAdmin()),
                      profile, registered)
    with winreg.OpenKey(winreg.HKEY_LOCAL_MACHINE,
                        r"SOFTWARE\Microsoft\Windows\CurrentVersion\AppModelUnlock") as key:
        developer_mode, _ = winreg.QueryValueEx(key, "AllowDevelopmentWithoutDevLicense")
    if developer_mode != 0:
        raise AssertionError("Developer Mode must be disabled during acceptance")
    if not Path(profile).is_dir():
        raise AssertionError("Standard User profile directory is missing")
    os.environ["CI"] = "true"
    print(f"Windows Standard User: SID={expected_sid}; IsUserAnAdmin=False; Developer Mode=0; "
          f"Profile={profile}; Commit={commit}", flush=True)
    return Path(profile)


def windows_smoke(stage, expected_sid, commit):
    # Open the log in the actual Standard User process, not through inherited
    # handles from the elevated runner. Start-Process owns process creation.
    import traceback

    with (Path(stage) / "acceptance.log").open("x", encoding="utf-8", buffering=1) as log:
        with redirect_stdout(log), redirect_stderr(log):
            try:
                home = validate_windows_environment(expected_sid, commit)
                run_permission_smoke(stage, home, "windows-x64-msvc")
            except Exception:
                traceback.print_exc()
                return 1
    return 0


def macos_smoke(stage, home, expected_uid, commit):
    groups = subprocess.check_output(["/usr/bin/id", "-Gn"], text=True).split()
    if os.getuid() != int(expected_uid) or os.getuid() == 0 or "admin" in groups or "wheel" in groups:
        raise AssertionError("macOS permission smoke requires the expected non-admin UID")
    if Path(os.environ["HOME"]).resolve() != Path(home).resolve():
        raise AssertionError("macOS permission smoke requires its test home")
    print(f"macOS non-admin UID={os.getuid()}; Home={home}; Commit={commit}", flush=True)
    run_permission_smoke(stage, home, "macos-arm64")


if __name__ == "__main__":
    if len(sys.argv) == 5 and sys.argv[1] == "--stage":
        stage_package(*sys.argv[2:])
    elif len(sys.argv) == 5 and sys.argv[1] == "--windows":
        sys.exit(windows_smoke(*sys.argv[2:]))
    elif len(sys.argv) == 6 and sys.argv[1] == "--macos":
        macos_smoke(*sys.argv[2:])
    else:
        raise SystemExit("unknown permission smoke arguments")
