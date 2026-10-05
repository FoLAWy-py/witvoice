"""Model-task-only private-memory accounting; no PCM or VM contents read."""
import ctypes
from ctypes import wintypes
import os
import sys

class Counters(ctypes.Structure):
    _fields_ = [("cb", wintypes.DWORD), ("PageFaultCount", wintypes.DWORD)] + [
        (name, ctypes.c_size_t) for name in ("PeakWorkingSetSize", "WorkingSetSize",
        "QuotaPeakPagedPoolUsage", "QuotaPagedPoolUsage", "QuotaPeakNonPagedPoolUsage",
        "QuotaNonPagedPoolUsage", "PagefileUsage", "PeakPagefileUsage", "PrivateUsage")]

def process_private_bytes(pid):
    if sys.platform != "win32" or type(pid) is not int or not 0 < pid <= 0xffffffff:
        return None
    kernel = ctypes.WinDLL("kernel32", use_last_error=True)
    psapi = ctypes.WinDLL("psapi", use_last_error=True)
    kernel.OpenProcess.argtypes = [wintypes.DWORD, wintypes.BOOL, wintypes.DWORD]
    kernel.OpenProcess.restype = wintypes.HANDLE
    kernel.CloseHandle.argtypes = [wintypes.HANDLE]
    kernel.CloseHandle.restype = wintypes.BOOL
    psapi.GetProcessMemoryInfo.argtypes = [wintypes.HANDLE, ctypes.c_void_p, wintypes.DWORD]
    psapi.GetProcessMemoryInfo.restype = wintypes.BOOL
    # Windows11 API needs QUERY_LIMITED_INFORMATION, not VM_READ.
    handle = kernel.OpenProcess(0x1000, False, pid)
    if not handle:
        return None
    result = None
    counters = Counters()
    counters.cb = ctypes.sizeof(counters)
    try:
        if psapi.GetProcessMemoryInfo(handle, ctypes.byref(counters), counters.cb):
            result = counters.PrivateUsage
    finally:
        closed = kernel.CloseHandle(handle)
    return result if closed else None

def model_tree_private_bytes(query=process_private_bytes):
    # This runs inside the fixed child. Its actual parent is the controller;
    # both are accounted before Ready, rather than hiding the second process.
    own, parent = os.getpid(), os.getppid()
    if own == parent or not own or not parent:
        return None
    values = (query(own), query(parent))
    if any(type(value) is not int or value < 0 for value in values):
        return None
    return sum(values)
