"""Single-owner local worker client: authenticated Win32 pipes, bounded frame I/O.

No model, audio device, network or automatic reconnect. Node owns the process Job
and startup watchdog. Cancellation is drained before OVERLAPPED memory is freed;
this safety rule does not promise a universal OS cancellation completion time.
"""
from __future__ import annotations
import ctypes as c
from ctypes import wintypes as w
import re
import os
import sys
import threading
import time
from protocol import read_frame, write_frame


class _Overlapped(c.Structure):
    _fields_ = [("Internal", c.c_size_t), ("InternalHigh", c.c_size_t),
                ("Offset", w.DWORD), ("OffsetHigh", w.DWORD), ("hEvent", w.HANDLE)]


class _TokenUser(c.Structure):
    _fields_ = [("Sid", c.c_void_p), ("Attributes", w.DWORD)]


class _Api:
    def __init__(self):
        if sys.platform != "win32":
            raise RuntimeError("Windows worker transport required")
        self.k = c.WinDLL("kernel32", use_last_error=True)
        self.a = c.WinDLL("advapi32", use_last_error=True)
        functions = {
            "CreateFileW": ([w.LPCWSTR,w.DWORD,w.DWORD,c.c_void_p,w.DWORD,w.DWORD,w.HANDLE],w.HANDLE),
            "WaitNamedPipeW": ([w.LPCWSTR,w.DWORD],w.BOOL),
            "GetNamedPipeServerProcessId": ([w.HANDLE,c.POINTER(w.ULONG)],w.BOOL),
            "CreateEventW": ([c.c_void_p,w.BOOL,w.BOOL,w.LPCWSTR],w.HANDLE),
            "ReadFile": ([w.HANDLE,c.c_void_p,w.DWORD,c.POINTER(w.DWORD),c.POINTER(_Overlapped)],w.BOOL),
            "WriteFile": ([w.HANDLE,c.c_void_p,w.DWORD,c.POINTER(w.DWORD),c.POINTER(_Overlapped)],w.BOOL),
            "CancelIoEx": ([w.HANDLE,c.POINTER(_Overlapped)],w.BOOL),
            "GetOverlappedResult": ([w.HANDLE,c.POINTER(_Overlapped),c.POINTER(w.DWORD),w.BOOL],w.BOOL),
            "WaitForSingleObject": ([w.HANDLE,w.DWORD],w.DWORD),
            "CloseHandle": ([w.HANDLE],w.BOOL),
            "OpenProcess": ([w.DWORD,w.BOOL,w.DWORD],w.HANDLE),
            "GetCurrentProcess": ([],w.HANDLE),
            "LocalFree": ([c.c_void_p],c.c_void_p),
        }
        for name,(args,result) in functions.items():
            fn=getattr(self.k,name);fn.argtypes=args;fn.restype=result
        functions = {
            "OpenProcessToken": ([w.HANDLE,w.DWORD,c.POINTER(w.HANDLE)],w.BOOL),
            "GetTokenInformation": ([w.HANDLE,c.c_int,c.c_void_p,w.DWORD,c.POINTER(w.DWORD)],w.BOOL),
            "ConvertSidToStringSidW": ([c.c_void_p,c.POINTER(w.LPWSTR)],w.BOOL),
        }
        for name,(args,result) in functions.items():
            fn=getattr(self.a,name);fn.argtypes=args;fn.restype=result
        if c.sizeof(_Overlapped) != (32 if c.sizeof(c.c_void_p)==8 else 20):
            raise RuntimeError("unsupported OVERLAPPED ABI")

    def error(self):
        # Scalar Win32 code only: no paths, payload, SID or authentication bytes.
        return OSError(c.get_last_error(), "worker Windows operation failed")

    def sid(self, process):
        token=w.HANDLE()
        if not self.a.OpenProcessToken(process,8,c.byref(token)): raise self.error()
        try:
            size=w.DWORD()
            self.a.GetTokenInformation(token,1,None,0,c.byref(size))
            if not 1 <= size.value <= 4096: raise ValueError("worker SID bound")
            # ctypes buffer is sufficiently aligned for TOKEN_USER.
            buf=c.create_string_buffer(size.value)
            if not self.a.GetTokenInformation(token,1,buf,len(buf),c.byref(size)): raise self.error()
            text=w.LPWSTR()
            user=c.cast(buf,c.POINTER(_TokenUser)).contents
            if not self.a.ConvertSidToStringSidW(user.Sid,c.byref(text)): raise self.error()
            try: return text.value
            finally: self.k.LocalFree(c.cast(text,c.c_void_p))
        finally: self.k.CloseHandle(token)


def _deadline(deadline_ns):
    if type(deadline_ns) is not int:
        raise ValueError("worker absolute deadline required")
    remaining=deadline_ns-time.monotonic_ns()
    if remaining <= 0: raise TimeoutError("worker I/O deadline")
    if remaining > 3_000_000_000: raise ValueError("worker I/O horizon exceeds three seconds")
    return remaining


class PipeClient:
    """One owning thread per channel. Every failure retires the handle."""
    def __init__(self, tag: str, expected_server_pid: int, secret: bytes, deadline_ns: int):
        self._handle=None;self._owner=threading.get_ident();self._api=_Api()
        if not isinstance(tag,str) or not re.fullmatch(r"[A-Za-z0-9-]{1,32}",tag):
            raise ValueError("invalid worker endpoint tag")
        if type(expected_server_pid) is not int or not 1 <= expected_server_pid <= 0xffffffff:
            raise ValueError("invalid worker server identity")
        if not isinstance(secret,bytes) or len(secret)!=32:
            raise ValueError("invalid worker bootstrap credential")
        _deadline(deadline_ns)
        api=self._api
        sid=api.sid(api.k.GetCurrentProcess())
        name=rf"\\.\pipe\witvoice.{sid}.{tag}.control.v1"
        try:
            while True:
                remaining=_deadline(deadline_ns)
                handle=api.k.CreateFileW(name,0xc0000000,0,None,3,0x40110000,None)
                if handle != c.c_void_p(-1).value:
                    if not handle: raise api.error()
                    self._handle=handle;break
                error=c.get_last_error()
                if error not in (2,231): raise OSError(error,"worker pipe unavailable")
                api.k.WaitNamedPipeW(name,min(50,max(1,remaining//1_000_000)))
                if error==2: time.sleep(min(.002,remaining/1_000_000_000))
            pid=w.ULONG()
            if not api.k.GetNamedPipeServerProcessId(self._handle,c.byref(pid)): raise api.error()
            if pid.value != expected_server_pid: raise PermissionError("worker server PID mismatch")
            process=api.k.OpenProcess(0x1000,False,pid.value)
            if not process: raise api.error()
            try:
                if api.sid(process)!=sid: raise PermissionError("worker server user mismatch")
            finally: api.k.CloseHandle(process)
            self._write_all(secret,deadline_ns)
            if self.read(1,deadline_ns)!=b"\xa5": raise PermissionError("worker authentication refused")
            _deadline(deadline_ns)
        except BaseException:
            self.close();raise

    def _check_owner(self):
        if self._owner != threading.get_ident(): raise RuntimeError("worker pipe owner thread mismatch")
        if self._handle is None: raise EOFError("worker pipe closed")

    def _transfer(self, count, deadline_ns, payload=None):
        self._check_owner();_deadline(deadline_ns)
        if type(count) is not int or not 1 <= count <= 65536: raise ValueError("worker transfer bound")
        api=self._api
        operation=_Overlapped()
        buf=c.create_string_buffer(count)
        if payload is not None: c.memmove(buf,payload,count)
        transferred=w.DWORD()
        event=api.k.CreateEventW(None,True,False,None)
        if not event: raise api.error()
        issued=False;completed=False
        try:
            operation.hEvent=event
            fn=api.k.ReadFile if payload is None else api.k.WriteFile
            # Set before entering ctypes: Python can raise after the native call
            # submitted work but before its return value reaches this assignment.
            issued=True
            started=fn(self._handle,buf,count,None,c.byref(operation))
            if not started:
                error=c.get_last_error()
                if error!=997: raise OSError(error,"worker pipe transfer failed")
                wait=api.k.WaitForSingleObject(event,max(0,(deadline_ns-time.monotonic_ns())//1_000_000))
                if wait!=0:
                    if wait==258: raise TimeoutError("worker pipe transfer deadline")
                    raise api.error()
            if not api.k.GetOverlappedResult(self._handle,c.byref(operation),c.byref(transferred),False):
                raise api.error()
            completed=True
            _deadline(deadline_ns)
            if not 1 <= transferred.value <= count: raise EOFError("worker pipe transfer incomplete")
            return buf.raw[:transferred.value] if payload is None else transferred.value
        except BaseException:
            if issued and not completed:
                try:
                    api.k.CancelIoEx(self._handle,c.byref(operation))
                    # This covers every exceptional exit after native submission,
                    # including Python interruption at a ctypes return boundary.
                    api.k.GetOverlappedResult(self._handle,c.byref(operation),c.byref(transferred),True)
                    if operation.Internal == 0x103:  # STATUS_PENDING
                        os._exit(70)
                except BaseException:
                    # A second interruption during cancellation cannot unwind live
                    # kernel buffers. Retire this owned worker address space; Node
                    # must handle the unexpected process exit as FailedMuted.
                    os._exit(70)
            self.close();raise
        finally: api.k.CloseHandle(event)

    def read(self,count,deadline_ns):
        self._check_owner()
        try: return self._transfer(count,deadline_ns)
        except BaseException: self.close();raise
    def write(self,buf,deadline_ns):
        self._check_owner()
        try:
            if not 1 <= len(buf) <= 65536: raise ValueError("worker write bound")
            payload=bytes(buf)
            return self._transfer(len(payload),deadline_ns,payload)
        except BaseException: self.close();raise
    def _write_all(self,payload,deadline_ns):
        offset=0
        while offset<len(payload): offset+=self.write(payload[offset:],deadline_ns)
    def read_frame(self,maximum,deadline_ns):
        try: return read_frame(self.read,deadline_ns,maximum)
        except BaseException: self.close();raise
    def write_frame(self,payload,maximum,deadline_ns):
        try: write_frame(self.write,payload,deadline_ns,maximum)
        except BaseException: self.close();raise
    def close(self):
        if self._owner != threading.get_ident(): raise RuntimeError("worker pipe owner thread mismatch")
        if self._handle is not None:
            self._api.k.CloseHandle(self._handle);self._handle=None
    def __enter__(self): return self
    def __exit__(self,*_): self.close()
