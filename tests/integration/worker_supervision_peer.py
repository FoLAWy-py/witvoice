"""Test target only: finite authenticated native peer, never a model or engine.
Not referenced by any production selector; no Torch, PCM, recording or network.
"""
import argparse
import json
import os
from pathlib import Path
import subprocess
import sys
import time

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "workers" / "vc_worker"))
from windows_pipe import PipeClient

CASES = {"valid", "bad_request", "bad_binding", "bad_cap", "eof", "no_heartbeat",
         "partial", "late_old", "no_media", "bad_token", "delayed_shared", "memory_pressure",
         "ready_eof", "bad_media_token", "bad_budget", "bad_run"}

def frame(value):
    return json.dumps(value, allow_nan=False, separators=(",", ":")).encode("utf-8")

def ready(warmup):
    return {"protocol_version": 1, "request_id": warmup["request_id"],
            "binding": dict(warmup["binding"]), "event": {"kind": "Ready", "args": {"capabilities": {
                "engine_id": "meanvc2", "model_sha256": warmup["command"]["args"]["model_sha256"],
                "backend": "cuda", "native_input_rate": 16000, "native_output_rate": 16000,
                "chunk_samples": 2560, "lookahead_samples": 640,
                "conditioning_schema": "meanvc2-reference-local-v1", "duration_preserving": False,
                "capability_test_run_id": "T011-warmup-" + warmup["request_id"],
                "model_memory_budget_bytes": str(8 * 1024 ** 3),
                "device_memory_budget_bytes": str(4 * 1024 ** 3)}}}}

def response(request, kind):
    return {"protocol_version": 1, "request_id": request["request_id"],
            "binding": request["binding"], "event": {"kind": kind}}

def main():
    if not sys.flags.isolated or not sys.flags.no_site:
        raise ValueError("isolated no-site test interpreter required")
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--control", required=True)
    parser.add_argument("--media", required=True)
    parser.add_argument("--node-pid", required=True, type=int)
    parser.add_argument("--case", required=True, choices=sorted(CASES))
    args = parser.parse_args()
    token = sys.stdin.buffer.read(33)
    if len(token) != 32 or args.control == args.media:
        raise ValueError("invalid finite test bootstrap")
    startup = time.monotonic_ns() + 2_500_000_000
    with PipeClient(args.control, args.node_pid, b"\0" * 32 if args.case == "bad_token" else token, startup) as control:
        if args.case == "no_media":
            time.sleep(4)
            return
        with PipeClient(args.media, args.node_pid, b"\0" * 32 if args.case == "bad_media_token" else token, startup):
            # A finite descendant proves cleanup covers more than controller exit.
            leaf = subprocess.Popen([sys.executable, "-I", "-S", "-c", "import time; time.sleep(12)"],
                                    stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL,
                                    stderr=subprocess.DEVNULL, close_fds=True, creationflags=subprocess.CREATE_NO_WINDOW)
            # Fixed private test-only scalar witness, no token/PCM/user path.
            witness = Path(__file__).resolve().parents[2] / ".local" / "t011-supervisor-author" / (args.control + ".members.json")
            witness.write_text(json.dumps({"controller_pid": os.getpid(), "leaf_pid": leaf.pid}), encoding="utf-8")
            expiry = time.monotonic_ns() + 10_000_000_000
            warmup = json.loads(control.read_frame(65536, min(expiry, time.monotonic_ns() + 2_000_000_000)))
            if warmup["command"]["kind"] != "Warmup":
                raise ValueError("test warmup required")
            ready_sent = False
            while time.monotonic_ns() < expiry:
                limit = min(expiry, time.monotonic_ns() + 2_000_000_000)
                request = json.loads(control.read_frame(65536, limit))
                if request["command"]["kind"] == "Stop":
                    control.write_frame(frame(response(request, "Stopped")), 65536, limit)
                    # ACK is deliberately followed by live descendants/controller.
                    time.sleep(3)
                    return
                if request["command"]["kind"] != "Heartbeat":
                    raise ValueError("test command rejected")
                if args.case == "eof" or (args.case == "ready_eof" and ready_sent) or (args.case == "late_old" and ready_sent and warmup["binding"]["epoch"] == 1):
                    return  # leaf remains in owned Job and must be reclaimed
                if args.case == "no_heartbeat":
                    time.sleep(2)
                    return
                if args.case in ("partial", "delayed_shared"):
                    value = frame(response(request, "Heartbeat"))
                    wire = len(value).to_bytes(4, "big") + value
                    if args.case == "delayed_shared":
                        time.sleep(.22)
                    control.write(wire[:2], limit)
                    time.sleep(.23 if args.case == "delayed_shared" else 1)
                    control.write(wire[2:], limit)
                    return
                if not ready_sent:
                    value = ready(warmup)
                    if args.case == "bad_request":
                        value["request_id"] = "00000000-0000-0000-0000-ffffffffffff"
                    elif args.case == "bad_binding":
                        value["binding"]["epoch"] += 1
                    elif args.case == "bad_cap":
                        value["event"]["args"]["capabilities"]["lookahead_samples"] = 639
                    elif args.case == "bad_budget":
                        value["event"]["args"]["capabilities"]["model_memory_budget_bytes"] = str(6 * 1024 ** 3)
                    elif args.case == "bad_run":
                        value["event"]["args"]["capabilities"]["capability_test_run_id"] = "T011-warmup-wrong-request"
                    elif args.case == "late_old" and warmup["binding"]["epoch"] > 1:
                        value["binding"]["epoch"] -= 1
                    elif args.case == "memory_pressure":
                        value = response(warmup, "MemoryPressure")
                    control.write_frame(frame(value), 65536, limit)
                    ready_sent = True
                control.write_frame(frame(response(request, "Heartbeat")), 65536, limit)
            # Referenced locally only; Node owns actual tree termination.
            assert leaf.pid != os.getpid()

if __name__ == "__main__":
    main()
