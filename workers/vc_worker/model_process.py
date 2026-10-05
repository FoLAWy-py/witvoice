"""Bounded scalar model-process owner. Called only by the model task.

No Node credential, PCM, model object, arbitrary executable/path or test selector
crosses this boundary. Node's entire Job remains the authoritative cleanup.
"""
from __future__ import annotations
import ctypes
import io
import json
import os
from pathlib import Path
import subprocess
import time

MAX_BYTES = 65536
PREPARE_NS = 120_000_000_000
CLOSE_SECONDS = 2.0
BASE_PYTHON = Path(r"C:\Users\22198\.cache\codex-runtimes\codex-primary-runtime\dependencies\python\python.exe")
HOST_SCRIPT = Path(__file__).resolve().with_name("model_host_bootstrap.py")
MODEL_SHA = "01caafec9a3991a5514df9412952d24ae3370406358a01e20e03df41f2f5d515"
REFERENCE_ID = "00000000-0000-0000-0000-000000000011"


def _check(deadline_ns, clock):
    if type(deadline_ns) is not int or clock() >= deadline_ns:
        raise TimeoutError("model process deadline")


def read_frame(read, deadline_ns, clock=time.monotonic_ns):
    def exact(count):
        data = bytearray()
        while len(data) < count:
            _check(deadline_ns, clock)
            part = read(count - len(data), deadline_ns)
            _check(deadline_ns, clock)
            if not isinstance(part, bytes) or len(part) > count - len(data):
                raise ValueError("model reader contract")
            if not part:
                raise EOFError("model frame incomplete")
            data.extend(part)
        return bytes(data)
    length = int.from_bytes(exact(4), "big")
    if not 1 <= length <= MAX_BYTES:
        raise ValueError("model frame size")
    return exact(length)


def write_frame(write, payload, deadline_ns, clock=time.monotonic_ns):
    if not isinstance(payload, bytes) or not 1 <= len(payload) <= MAX_BYTES:
        raise ValueError("model frame size")
    packet = len(payload).to_bytes(4, "big") + payload
    offset = 0
    while offset < len(packet):
        _check(deadline_ns, clock)
        count = write(memoryview(packet)[offset:], deadline_ns)
        _check(deadline_ns, clock)
        if type(count) is not int or not 1 <= count <= len(packet) - offset:
            raise EOFError("model write incomplete")
        offset += count


class AnonymousReader:
    """Single model-task-owned handle. Peek never shares this handle with readers.

    Synchronous driver calls are not universally wallclock-bounded. Only model
    task performs them; Node independent deadline/Job termination is mandatory.
    """
    def __init__(self, stream):
        if os.name != "nt":
            raise OSError("Windows anonymous transport required")
        import msvcrt
        self.fd = stream.fileno()
        self.handle = msvcrt.get_osfhandle(self.fd)
        self.kernel = ctypes.WinDLL("kernel32", use_last_error=True)
        self.kernel.PeekNamedPipe.argtypes = [ctypes.c_void_p, ctypes.c_void_p,
                                              ctypes.c_uint32, ctypes.c_void_p,
                                              ctypes.POINTER(ctypes.c_uint32),
                                              ctypes.c_void_p]
        self.kernel.PeekNamedPipe.restype = ctypes.c_int

    def read(self, maximum, deadline_ns):
        while True:
            _check(deadline_ns, time.monotonic_ns)
            available = ctypes.c_uint32()
            if not self.kernel.PeekNamedPipe(self.handle, None, 0, None,
                                             ctypes.byref(available), None):
                error = ctypes.get_last_error()
                if error in (109, 232):  # broken pipe/no data after peer closed
                    return b""
                raise OSError(error, "model pipe unavailable")
            if available.value:
                return os.read(self.fd, min(maximum, available.value))
            time.sleep(0.001)


def _write_fd(fd, payload, deadline_ns):
    _check(deadline_ns, time.monotonic_ns)
    # Fixed request/capabilities are <4KiB; bounded chunks, no queued writes.
    # Synchronous write safety still relies on Node's independently owned Job.
    count = os.write(fd, payload[:4096])
    _check(deadline_ns, time.monotonic_ns)
    return count


def validate_request(payload):
    from control import decode_control
    request = decode_control(payload, "WorkerRequest")
    command = request["command"]
    if command["kind"] != "Warmup":
        raise ValueError("fixed warmup only")
    args = command["args"]
    if (args["model_sha256"], args["reference_id"], args["backend"]) != (
            MODEL_SHA, REFERENCE_ID, "Cuda"):
        raise ValueError("fixed warmup assets")
    return request


def response_payload(request, kind, capabilities=None):
    event = {"kind": kind}
    if kind == "Ready":
        event["args"] = {"capabilities": capabilities}
    elif kind == "Failed":
        event["args"] = {"code": "ENGINE_FAILED"}
    elif kind != "MemoryPressure":
        raise ValueError("closed model result")
    value = {"protocol_version": 1, "request_id": request["request_id"],
             "binding": request["binding"], "event": event}
    return json.dumps(value, allow_nan=False, separators=(",", ":")).encode("utf-8")


def decode_result(payload, request):
    from control import decode_control
    from warmup import HOST_BUDGET, DEVICE_BUDGET
    response = decode_control(payload, "WorkerResponse", request["binding"])
    if response["request_id"] != request["request_id"]:
        raise ValueError("model request mismatch")
    event = response["event"]
    if event["kind"] == "MemoryPressure":
        raise MemoryError("model memory pressure")
    if event["kind"] == "Failed" and event["args"]["code"] == "ENGINE_FAILED":
        raise RuntimeError("model failed muted")
    if event["kind"] != "Ready":
        raise ValueError("closed model result")
    cap = event["args"]["capabilities"]
    expected = {
        "engine_id": "meanvc2", "model_sha256": MODEL_SHA, "backend": "cuda",
        "native_input_rate": 16000, "native_output_rate": 16000,
        "chunk_samples": 2560, "lookahead_samples": 640,
        "conditioning_schema": "meanvc2-reference-local-v1",
        "duration_preserving": False,
        "capability_test_run_id": "T011-warmup-" + request["request_id"],
        "model_memory_budget_bytes": str(HOST_BUDGET),
        "device_memory_budget_bytes": str(DEVICE_BUDGET),
    }
    if cap != expected:
        raise ValueError("unverified model capability")
    return cap


class ModelProcess:
    """No __del__ waiting. Explicit close belongs exclusively to model task."""
    def __init__(self, process):
        self._process = process
        self._closed = False

    @property
    def pid(self):
        return self._process.pid

    def private_bytes(self):
        if self._closed or self._process.poll() is not None or os.name != "nt":
            return None
        from ctypes import wintypes
        class Counters(ctypes.Structure):
            _fields_ = [("cb", wintypes.DWORD), ("PageFaultCount", wintypes.DWORD),
                        ("PeakWorkingSetSize", ctypes.c_size_t),
                        ("WorkingSetSize", ctypes.c_size_t),
                        ("QuotaPeakPagedPoolUsage", ctypes.c_size_t),
                        ("QuotaPagedPoolUsage", ctypes.c_size_t),
                        ("QuotaPeakNonPagedPoolUsage", ctypes.c_size_t),
                        ("QuotaNonPagedPoolUsage", ctypes.c_size_t),
                        ("PagefileUsage", ctypes.c_size_t),
                        ("PeakPagefileUsage", ctypes.c_size_t),
                        ("PrivateUsage", ctypes.c_size_t)]
        info = Counters()
        info.cb = ctypes.sizeof(info)
        api = ctypes.WinDLL("psapi", use_last_error=True).GetProcessMemoryInfo
        api.argtypes = [ctypes.c_void_p, ctypes.c_void_p, wintypes.DWORD]
        api.restype = wintypes.BOOL
        if not api(int(self._process._handle), ctypes.byref(info), info.cb):
            return None
        return int(info.PrivateUsage)

    def close(self):
        if self._closed:
            return
        self._closed = True
        # No graceful model destructor on controller. Entire process is retired.
        if self._process.poll() is None:
            self._process.terminate()
        try:
            self._process.wait(timeout=CLOSE_SECONDS)
        finally:
            for stream in (self._process.stdin, self._process.stdout):
                if stream is not None:
                    stream.close()
        # Timeout is not cleanup success; Node kills/queries its whole Job.


def _launch(diagnostic):
    if os.name != "nt" or not BASE_PYTHON.is_file() or not HOST_SCRIPT.is_file():
        raise OSError("fixed Windows model host unavailable")
    args = [str(BASE_PYTHON), "-I", "-S", str(HOST_SCRIPT)]
    if diagnostic:
        args.append("--prepare-diagnostic")
    # No BREAKAWAY_FROM_JOB, Node credential inheritance or shell.
    return subprocess.Popen(args, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                            stderr=subprocess.DEVNULL, close_fds=True,
                            creationflags=subprocess.CREATE_NO_WINDOW, bufsize=0)


def prepare_isolated(request, diagnostic=False):
    if type(diagnostic) is not bool:
        raise ValueError("diagnostic boolean required")
    payload = json.dumps(request, allow_nan=False, separators=(",", ":")).encode("utf-8")
    fixed = validate_request(payload)  # Before process creation.
    deadline = time.monotonic_ns() + PREPARE_NS
    owner = ModelProcess(_launch(diagnostic))
    try:
        _check(deadline, time.monotonic_ns)
        write_frame(lambda part, limit: _write_fd(owner._process.stdin.fileno(), part, limit),
                    payload, deadline)
        reader = AnonymousReader(owner._process.stdout)
        result = read_frame(reader.read, deadline)
        capabilities = decode_result(result, fixed)
        _check(deadline, time.monotonic_ns)
        return owner, capabilities
    except BaseException:
        owner.close()
        raise
