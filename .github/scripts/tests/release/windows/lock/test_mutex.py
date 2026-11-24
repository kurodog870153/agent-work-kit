"""Check user lock identity, contention, normal and abnormal release."""
import csv
import ctypes
from ctypes import wintypes
from pathlib import Path
import subprocess
import tempfile
import time

ROOT = Path(__file__).resolve().parents[6]
CASE = Path(__file__).with_suffix(".ps1")


def command(mode, *args):
    return ["powershell.exe", "-NoProfile", "-NonInteractive", "-File",
            str(CASE), "-Mode", mode, *map(str, args)]


def run_case(mode, *args, capture=False):
    return subprocess.run(command(mode, *args), cwd=ROOT, stdin=subprocess.DEVNULL,
                          capture_output=capture, check=True, timeout=60).stdout


def check_contention(kernel, mutex, directory):
    # Keep the named object alive so forced termination exercises recovery of
    # an abandoned mutex in the same abnormal-exit case.
    observer = kernel.CreateMutexW(None, False, mutex)
    if not observer:
        raise ctypes.WinError(ctypes.get_last_error())
    try:
        marker = directory / "ready"
        log_path = directory / "holder.log"
        with log_path.open("wb") as log:
            holder = subprocess.Popen(command("hold", "-Marker", marker),
                                      cwd=ROOT, stdin=subprocess.DEVNULL, stdout=log,
                                      stderr=subprocess.STDOUT)
        try:
            deadline = time.monotonic() + 45
            while not marker.is_file():
                if holder.poll() is not None:
                    raise RuntimeError(f"lock holder exited: {holder.returncode}")
                if time.monotonic() >= deadline:
                    raise TimeoutError("lock holder did not acquire mutex")
                time.sleep(0.1)
            if holder.poll() is not None:
                raise RuntimeError("lock holder exited before contention test")
            run_case("contend")
            if holder.poll() is not None:
                raise RuntimeError("lock holder exited before abandonment test")
            holder.kill()
            if holder.wait(timeout=10) == 0:
                raise AssertionError("lock holder was not terminated")
            run_case("recover")
        finally:
            if holder.poll() is None:
                holder.kill()
                holder.wait(timeout=10)
            print(log_path.read_text(encoding="utf-8", errors="replace"), flush=True)
    finally:
        if not kernel.CloseHandle(observer):
            raise ctypes.WinError(ctypes.get_last_error())


def main():
    run_case("normal")
    run_case("normal")
    sid = next(csv.reader(subprocess.check_output(
        ["whoami.exe", "/user", "/fo", "csv", "/nh"], text=True).strip().splitlines()))[1]
    kernel = ctypes.WinDLL("kernel32", use_last_error=True)
    kernel.CreateMutexW.argtypes = [ctypes.c_void_p, wintypes.BOOL, wintypes.LPCWSTR]
    kernel.CreateMutexW.restype = wintypes.HANDLE
    kernel.CloseHandle.argtypes = [wintypes.HANDLE]
    kernel.CloseHandle.restype = wintypes.BOOL
    with tempfile.TemporaryDirectory(prefix=".installer-test-lock.", dir=ROOT) as scratch:
        directory = Path(scratch)
        name = run_case("user-name", capture=True).decode().splitlines()[-1].strip()
        if name != r"Global\AgentWorkKitInstall.User." + sid:
            raise AssertionError("installation mutex must identify the actual account SID")
        check_contention(kernel, name, directory)
    print("Windows user installation lock cases passed.", flush=True)


if __name__ == "__main__":
    main()
