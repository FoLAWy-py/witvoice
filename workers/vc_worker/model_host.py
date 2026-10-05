"""Fixed child model host. Only this interpreter loads/destroys model objects."""
from __future__ import annotations
import ctypes
import os
import sys
import time
from model_process import (PREPARE_NS, AnonymousReader, _write_fd, read_frame,
                           write_frame, validate_request, response_payload,
                           decode_result)


def in_any_job():
    if os.name != "nt":
        return False
    kernel = ctypes.WinDLL("kernel32", use_last_error=True)
    kernel.GetCurrentProcess.restype = ctypes.c_void_p
    kernel.IsProcessInJob.argtypes = [ctypes.c_void_p, ctypes.c_void_p,
                                     ctypes.POINTER(ctypes.c_int)]
    kernel.IsProcessInJob.restype = ctypes.c_int
    member = ctypes.c_int()
    if not kernel.IsProcessInJob(kernel.GetCurrentProcess(), None, ctypes.byref(member)):
        raise OSError(ctypes.get_last_error(), "job query failed")
    # ANY Job only. Specific Node ownership requires actual Node Job PID witness.
    return bool(member.value)


def _prepare(request, diagnostic):
    if diagnostic:
        from prepare_diagnostics import prepare_with_diagnostics
        return prepare_with_diagnostics(request)
    from warmup import prepare_fixed
    return prepare_fixed(request)


def serve(request, diagnostic, write, deadline):
    """Exactly one scalar result. Loader injection exists only via test patches."""
    runner = None
    try:
        if not in_any_job():
            raise RuntimeError("model host must be job contained")
        runner, capabilities = _prepare(request, diagnostic)
        payload = response_payload(request, "Ready", capabilities)
        decode_result(payload, request)  # Never publish unverified capabilities.
    except MemoryError:
        payload = response_payload(request, "MemoryPressure")
    except Exception:
        payload = response_payload(request, "Failed")
    write_frame(write, payload, deadline)
    return runner


def main(result_fd, diagnostic=False):
    deadline = time.monotonic_ns() + PREPARE_NS
    source = AnonymousReader(sys.stdin.buffer)
    request = validate_request(read_frame(source.read, deadline))
    runner = serve(request, diagnostic,
                   lambda part, limit: _write_fd(result_fd, part, limit), deadline)
    # Retain runner exclusively in this process until owner closes its request
    # pipe/terminates, or the original absolute preparation lease expires.
    # No second result frame, endless queue, or renewed deadline.
    if runner is not None:
        try:
            while source.read(1, deadline):
                raise ValueError("unexpected post-warmup request")
        finally:
            del runner
    return 0
