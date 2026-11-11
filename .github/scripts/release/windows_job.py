"""Keep a Windows installer and its descendants in one terminable Job Object."""

import ctypes
import time
from ctypes import wintypes


CREATE_SUSPENDED = 0x00000004
JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE = 0x00002000
JOB_OBJECT_EXTENDED_LIMIT_INFORMATION = 9
JOB_OBJECT_BASIC_ACCOUNTING_INFORMATION = 1
TH32CS_SNAPTHREAD = 0x00000004
THREAD_SUSPEND_RESUME = 0x0002


class BasicLimitInformation(ctypes.Structure):
    _fields_ = [("PerProcessUserTimeLimit", ctypes.c_int64),
                ("PerJobUserTimeLimit", ctypes.c_int64),
                ("LimitFlags", wintypes.DWORD),
                ("MinimumWorkingSetSize", ctypes.c_size_t),
                ("MaximumWorkingSetSize", ctypes.c_size_t),
                ("ActiveProcessLimit", wintypes.DWORD),
                ("Affinity", ctypes.c_size_t),
                ("PriorityClass", wintypes.DWORD),
                ("SchedulingClass", wintypes.DWORD)]


class IoCounters(ctypes.Structure):
    _fields_ = [(name, ctypes.c_uint64) for name in (
        "ReadOperationCount", "WriteOperationCount", "OtherOperationCount",
        "ReadTransferCount", "WriteTransferCount", "OtherTransferCount")]


class ExtendedLimitInformation(ctypes.Structure):
    _fields_ = [("BasicLimitInformation", BasicLimitInformation),
                ("IoInfo", IoCounters),
                ("ProcessMemoryLimit", ctypes.c_size_t),
                ("JobMemoryLimit", ctypes.c_size_t),
                ("PeakProcessMemoryUsed", ctypes.c_size_t),
                ("PeakJobMemoryUsed", ctypes.c_size_t)]


class BasicAccountingInformation(ctypes.Structure):
    _fields_ = [("TotalUserTime", ctypes.c_int64),
                ("TotalKernelTime", ctypes.c_int64),
                ("ThisPeriodTotalUserTime", ctypes.c_int64),
                ("ThisPeriodTotalKernelTime", ctypes.c_int64),
                ("TotalPageFaultCount", wintypes.DWORD),
                ("TotalProcesses", wintypes.DWORD),
                ("ActiveProcesses", wintypes.DWORD),
                ("TotalTerminatedProcesses", wintypes.DWORD)]


class ThreadEntry(ctypes.Structure):
    _fields_ = [("dwSize", wintypes.DWORD),
                ("cntUsage", wintypes.DWORD),
                ("th32ThreadID", wintypes.DWORD),
                ("th32OwnerProcessID", wintypes.DWORD),
                ("tpBasePri", wintypes.LONG),
                ("tpDeltaPri", wintypes.LONG),
                ("dwFlags", wintypes.DWORD)]


def _kernel32():
    kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
    kernel32.CreateJobObjectW.argtypes = [ctypes.c_void_p, wintypes.LPCWSTR]
    kernel32.CreateJobObjectW.restype = wintypes.HANDLE
    kernel32.SetInformationJobObject.argtypes = [wintypes.HANDLE, ctypes.c_int,
                                                  ctypes.c_void_p, wintypes.DWORD]
    kernel32.SetInformationJobObject.restype = wintypes.BOOL
    kernel32.AssignProcessToJobObject.argtypes = [wintypes.HANDLE, wintypes.HANDLE]
    kernel32.AssignProcessToJobObject.restype = wintypes.BOOL
    kernel32.TerminateJobObject.argtypes = [wintypes.HANDLE, wintypes.UINT]
    kernel32.TerminateJobObject.restype = wintypes.BOOL
    kernel32.QueryInformationJobObject.argtypes = [wintypes.HANDLE, ctypes.c_int,
                                                    ctypes.c_void_p, wintypes.DWORD,
                                                    ctypes.c_void_p]
    kernel32.QueryInformationJobObject.restype = wintypes.BOOL
    kernel32.CreateToolhelp32Snapshot.argtypes = [wintypes.DWORD, wintypes.DWORD]
    kernel32.CreateToolhelp32Snapshot.restype = wintypes.HANDLE
    kernel32.Thread32First.argtypes = [wintypes.HANDLE, ctypes.POINTER(ThreadEntry)]
    kernel32.Thread32First.restype = wintypes.BOOL
    kernel32.Thread32Next.argtypes = [wintypes.HANDLE, ctypes.POINTER(ThreadEntry)]
    kernel32.Thread32Next.restype = wintypes.BOOL
    kernel32.OpenThread.argtypes = [wintypes.DWORD, wintypes.BOOL, wintypes.DWORD]
    kernel32.OpenThread.restype = wintypes.HANDLE
    kernel32.ResumeThread.argtypes = [wintypes.HANDLE]
    kernel32.ResumeThread.restype = wintypes.DWORD
    kernel32.CloseHandle.argtypes = [wintypes.HANDLE]
    kernel32.CloseHandle.restype = wintypes.BOOL
    return kernel32


def _raise_windows_error():
    raise ctypes.WinError(ctypes.get_last_error())


class WindowsInstallerJob:
    def __init__(self):
        self.api = _kernel32()
        self.handle = self.api.CreateJobObjectW(None, None)
        if not self.handle:
            _raise_windows_error()
        limits = ExtendedLimitInformation()
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
        if not self.api.SetInformationJobObject(self.handle, JOB_OBJECT_EXTENDED_LIMIT_INFORMATION,
                                                ctypes.byref(limits), ctypes.sizeof(limits)):
            error = ctypes.get_last_error()
            self.close()
            raise ctypes.WinError(error)

    def attach_and_resume(self, process):
        # The suspended parent cannot spawn a child before it enters the job.
        if not self.api.AssignProcessToJobObject(self.handle, int(process._handle)):
            _raise_windows_error()
        snapshot = self.api.CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0)
        if snapshot == ctypes.c_void_p(-1).value:
            _raise_windows_error()
        try:
            entry = ThreadEntry()
            entry.dwSize = ctypes.sizeof(entry)
            found = bool(self.api.Thread32First(snapshot, ctypes.byref(entry)))
            while found:
                if entry.th32OwnerProcessID == process.pid:
                    thread = self.api.OpenThread(THREAD_SUSPEND_RESUME, False, entry.th32ThreadID)
                    if not thread:
                        _raise_windows_error()
                    try:
                        if self.api.ResumeThread(thread) == 0xFFFFFFFF:
                            _raise_windows_error()
                    finally:
                        self.api.CloseHandle(thread)
                    return
                found = bool(self.api.Thread32Next(snapshot, ctypes.byref(entry)))
            raise RuntimeError(f"suspended installer thread not found: pid={process.pid}")
        finally:
            self.api.CloseHandle(snapshot)

    def active_processes(self):
        accounting = BasicAccountingInformation()
        if not self.api.QueryInformationJobObject(self.handle, JOB_OBJECT_BASIC_ACCOUNTING_INFORMATION,
                                                   ctypes.byref(accounting), ctypes.sizeof(accounting), None):
            _raise_windows_error()
        return accounting.ActiveProcesses

    def terminate_and_wait(self, grace=30):
        if not self.api.TerminateJobObject(self.handle, 1):
            _raise_windows_error()
        deadline = time.monotonic() + grace
        while time.monotonic() < deadline:
            if self.active_processes() == 0:
                return True
            time.sleep(0.05)
        return self.active_processes() == 0

    def close(self):
        if self.handle:
            self.api.CloseHandle(self.handle)
            self.handle = None
