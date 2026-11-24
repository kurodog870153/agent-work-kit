"""Exercise a real terminal/console rather than treating a redirected pipe as interactive."""
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time

PROMPT = b"Press Enter to exit..."


def interactive_environment(environment=None):
    env = dict(os.environ if environment is None else environment)
    for name in ("CI", "GITHUB_ACTIONS", "AGENT_WORK_KIT_INSTALLER_NESTED"):
        env.pop(name, None)
    return env


def press_windows_enter(pid):
    import ctypes
    from ctypes import wintypes

    class KeyEvent(ctypes.Structure):
        _fields_ = [("down", wintypes.BOOL), ("repeat", wintypes.WORD),
                    ("virtual_key", wintypes.WORD), ("scan_code", wintypes.WORD),
                    ("character", wintypes.WCHAR), ("control_state", wintypes.DWORD)]

    class InputRecord(ctypes.Structure):
        _fields_ = [("type", wintypes.WORD), ("event", KeyEvent)]

    api = ctypes.WinDLL("kernel32", use_last_error=True)
    api.FreeConsole()
    api.AttachConsole.argtypes = [wintypes.DWORD]
    api.AttachConsole.restype = wintypes.BOOL
    if not api.AttachConsole(pid):
        raise ctypes.WinError(ctypes.get_last_error())
    try:
        api.CreateFileW.argtypes = [wintypes.LPCWSTR, wintypes.DWORD, wintypes.DWORD, ctypes.c_void_p, wintypes.DWORD, wintypes.DWORD, wintypes.HANDLE]
        api.CreateFileW.restype = wintypes.HANDLE
        api.CloseHandle.argtypes = [wintypes.HANDLE]
        api.WriteConsoleInputW.argtypes = [wintypes.HANDLE, ctypes.POINTER(InputRecord),
                                          wintypes.DWORD, ctypes.POINTER(wintypes.DWORD)]
        api.WriteConsoleInputW.restype = wintypes.BOOL
        records = (InputRecord * 2)(
            InputRecord(1, KeyEvent(True, 1, 13, 28, "\r", 0)),
            InputRecord(1, KeyEvent(False, 1, 13, 28, "\r", 0)),
        )
        count = wintypes.DWORD()
        handle = api.CreateFileW("CONIN$", 0xC0000000, 3, None, 3, 0, None)
        if handle == ctypes.c_void_p(-1).value:
            raise ctypes.WinError(ctypes.get_last_error())
        try:
            if not api.WriteConsoleInputW(handle, records, 2, ctypes.byref(count)) or count.value != 2:
                raise ctypes.WinError(ctypes.get_last_error())
        finally:
            api.CloseHandle(handle)
    finally:
        api.FreeConsole()


def run_console(entry, options, env, *, expect_pause, before_enter=None, timeout=45):
    """Observe the final prompt while the process is alive and release it with Enter."""
    windows = sys.platform == "win32"
    with tempfile.TemporaryDirectory(prefix="installer-console-") as directory:
        output = Path(directory) / "output.txt"
        master = None
        if windows:
            # cmd redirects output inside a new console, leaving its input attached
            # to that console. Do not use subprocess.PIPE for its stdin.
            command = f'"{os.environ["ComSpec"]}" /D /S /C ""{entry}"'
            command += " " + subprocess.list2cmdline(list(options)) if options else ""
            command += f' > "{output}" 2>&1"'
            process = subprocess.Popen(command, env=env, creationflags=subprocess.CREATE_NEW_CONSOLE)
        else:
            import pty
            master, slave = pty.openpty()
            try:
                with output.open("wb") as log:
                    process = subprocess.Popen(["/bin/bash", str(entry), *options], env=env,
                                               stdin=slave, stdout=log, stderr=subprocess.STDOUT)
            finally:
                os.close(slave)
        try:
            deadline = time.monotonic() + timeout
            sent = False
            while time.monotonic() < deadline:
                data = output.read_bytes() if output.exists() else b""
                if PROMPT in data and not sent:
                    if not expect_pause:
                        raise AssertionError(f"unexpected pause: {data!r}")
                    if process.poll() is not None:
                        raise AssertionError("installer exited without waiting for Enter")
                    if before_enter:
                        before_enter()
                    if windows:
                        subprocess.run([sys.executable, str(Path(__file__).resolve()), str(process.pid)],
                                       check=True, timeout=10)
                    else:
                        os.write(master, b"\n")
                    sent = True
                code = process.poll()
                if code is not None:
                    data = output.read_bytes() if output.exists() else b""
                    if sent != expect_pause or data.count(PROMPT) != int(expect_pause):
                        raise AssertionError(f"incorrect prompt count: {data!r}")
                    return code, data
                time.sleep(0.1)
            raise TimeoutError(f"installer console did not finish: {data!r}")
        finally:
            if process.poll() is None:
                if windows:
                    subprocess.run(["taskkill", "/PID", str(process.pid), "/T", "/F"],
                                   stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=10)
                else:
                    process.kill()
                process.wait(timeout=10)
            if master is not None:
                os.close(master)


if __name__ == "__main__":
    press_windows_enter(int(sys.argv[1]))
