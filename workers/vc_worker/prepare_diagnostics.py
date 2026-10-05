"""Opt-in bounded model-phase diagnostics; no control-owner file operations."""
import json
import os
from pathlib import Path
import time

STAGES = ("imports_array_audio", "imports_torch", "imports_adapter", "fixtures_check",
          "model_load", "placement_validate", "convert_warmup", "finalize")
MAX_RECORDS = 16
MAX_BYTES = 4096

class StageJournal:
    def __init__(self, path, clock=time.monotonic_ns):
        # Constructor is called only by the model task. Exclusive creation means
        # a consumed diagnostic cannot overwrite its last progress evidence.
        self._clock = clock
        self._origin = clock()
        if type(self._origin) is not int or self._origin < 0:
            raise ValueError("diagnostic incomplete: monotonic origin")
        self._file = Path(path).open("xb", buffering=0)
        self._last = self._origin
        self._records = 0
        self._bytes = 0

    @classmethod
    def fixed(cls):
        root = Path(__file__).resolve().parents[2]
        return cls(root / ".local/t011-finalize-warmup-once/prepare-stages.ndjson")

    def record(self, stage, edge):
        if stage not in STAGES or edge not in ("before", "after"):
            raise ValueError("diagnostic incomplete: invalid closed stage")
        expected_stage = STAGES[self._records // 2] if self._records < MAX_RECORDS else None
        expected_edge = "before" if self._records % 2 == 0 else "after"
        if stage != expected_stage or edge != expected_edge:
            raise ValueError("diagnostic incomplete: phase order or record limit")
        now = self._clock()
        if type(now) is not int or now < self._last:
            raise ValueError("diagnostic incomplete: monotonic clock")
        payload = (json.dumps({"stage": stage, "edge": edge,
                             "elapsed_ns": now - self._origin},
                            separators=(",", ":")) + "\n").encode("ascii")
        if self._bytes + len(payload) > MAX_BYTES:
            raise ValueError("diagnostic incomplete: byte limit")
        if self._file.write(payload) != len(payload):
            raise OSError("diagnostic incomplete: short write")
        self._file.flush()
        self._last = now
        self._records += 1
        self._bytes += len(payload)

    def __enter__(self):
        return self

    def __exit__(self, *_):
        self._file.close()


def write_resources(path, host, device):
    from warmup import HOST_BUDGET, DEVICE_BUDGET
    if any(value is not None and (type(value) is not int or not 0 <= value < 2 ** 64)
           for value in (host, device)):
        raise ValueError("diagnostic invalid resource scalar")
    payload = json.dumps({"host_private_bytes": host, "device_reserved_bytes": device,
                          "host_budget_bytes": HOST_BUDGET, "device_budget_bytes": DEVICE_BUDGET},
                         separators=(",", ":"), allow_nan=False).encode("ascii")
    if len(payload) > 512:
        raise ValueError("diagnostic resource byte limit")
    with Path(path).open("xb", buffering=0) as output:
        if output.write(payload) != len(payload):
            raise OSError("diagnostic resource incomplete")


def prepare_with_diagnostics(request):
    from warmup import prepare_fixed
    root = Path(__file__).resolve().parents[2]
    base = root / ".local/t011-finalize-warmup-once"
    identity = base / "model-process-private.json"
    with identity.open("x", encoding="ascii") as output:
        json.dump({"controller_pid": os.getppid(), "model_pid": os.getpid()}, output, separators=(",", ":"))
    with StageJournal.fixed() as journal:
        return prepare_fixed(request, observer=journal.record,
                             resource_observer=lambda host, device:
                             write_resources(base / "resources.json", host, device))
