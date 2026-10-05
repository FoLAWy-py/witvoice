"""Authenticated, Prepare-only worker. Media inference belongs to T012.

The control owner never imports Torch or waits for model work. One bounded result
slot connects a single daemon model task to this owner. Node must enforce its
independent heartbeat/warmup deadlines and terminate its Job on every failure.
"""
from __future__ import annotations
import argparse
import json
import queue
import sys
import threading
import time
from control import decode_control

MODEL_SHA = "01caafec9a3991a5514df9412952d24ae3370406358a01e20e03df41f2f5d515"
REFERENCE_ID = "00000000-0000-0000-0000-000000000011"


class WarmupSession:
    def __init__(self, loader):
        self._loader = loader
        self._result = queue.Queue(maxsize=1)
        self._stopped = threading.Event()
        self._model_release = threading.Event()
        self._binding = None
        self._warmup_request = None
        self._phase = "NEW"

    def _response(self, request, kind, args=None):
        event = {"kind": kind}
        if args is not None:
            event["args"] = args
        value = {"protocol_version": 1, "request_id": request["request_id"],
                 "binding": request["binding"], "event": event}
        payload = json.dumps(value, allow_nan=False, separators=(",", ":")).encode("utf-8")
        decode_control(payload, "WorkerResponse", self._binding)
        return payload

    def _load(self, request):
        runner = None
        try:
            runner, capabilities = self._loader(request)
            # Only scalar capabilities cross into the control owner. The model
            # task retains the last runner reference and owns its destruction.
            record = (True, capabilities)
        except MemoryError:
            record = (False, "MemoryPressure")
        except Exception:
            # Never put paths, checkpoint errors, voice data or tracebacks on wire.
            record = (False, "Failed")
        if not self._stopped.is_set():
            try:
                self._result.put_nowait(record)
            except queue.Full:
                self._stopped.set()
        if runner is not None:
            # close() signals this only after the control loop has written any
            # Stopped ack. Node must still kill/query its Job if cleanup stalls.
            self._model_release.wait()

    def handle(self, payload):
        request = decode_control(payload, "WorkerRequest", self._binding)
        command = request["command"]
        kind = command["kind"]
        if self._phase in ("STOPPED", "FAILED"):
            raise ValueError("retired worker")
        if kind == "Warmup":
            if self._phase != "NEW":
                raise ValueError("duplicate warmup")
            args = command["args"]
            if (args["model_sha256"], args["reference_id"], args["backend"]) != (
                    MODEL_SHA, REFERENCE_ID, "Cuda"):
                raise ValueError("unapproved fixed model/reference/backend")
            self._binding = request["binding"]
            self._warmup_request = request
            self._phase = "WARMING"
            threading.Thread(target=self._load, args=(request,), daemon=True,
                             name="meanvc2-prepare").start()
            return []
        if self._phase == "NEW":
            raise ValueError("warmup required before control")
        if kind == "Stop":
            self._stopped.set()
            self._phase = "STOPPED"
            while True:
                try:
                    self._result.get_nowait()
                except queue.Empty:
                    break
            # Stopped is a protocol ack. Only Node's Job query proves GPU release.
            return [self._response(request, "Stopped")]
        if kind != "Heartbeat":
            raise ValueError("unsupported worker command")
        return [self._response(request, "Heartbeat")]

    def poll_ready(self):
        if self._phase != "WARMING":
            return []
        try:
            success, result = self._result.get_nowait()
        except queue.Empty:
            return []
        if not success:
            self._phase = "FAILED"
            if result == "MemoryPressure":
                return [self._response(self._warmup_request, result)]
            return [self._response(self._warmup_request, "Failed", {"code": "ENGINE_FAILED"})]
        # Validate the real adapter result before holding Ready resources.
        response = self._response(self._warmup_request, "Ready", {"capabilities": result})
        self._phase = "READY"
        return [response]

    @property
    def retired(self):
        return self._phase in ("STOPPED", "FAILED")

    def close(self):
        self._stopped.set()
        self._phase = "STOPPED"
        self._model_release.set()


def main():
    from windows_pipe import PipeClient
    from warmup import prepare_fixed
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--control", required=True)
    parser.add_argument("--media", required=True)
    parser.add_argument("--node-pid", required=True, type=int)
    parser.add_argument("--prepare-diagnostic", action="store_true")
    args = parser.parse_args()
    secret = sys.stdin.buffer.read(33)
    if len(secret) != 32 or args.control == args.media:
        raise ValueError("invalid worker bootstrap")
    startup = time.monotonic_ns() + 2_500_000_000
    loader = prepare_fixed
    if args.prepare_diagnostic:
        from prepare_diagnostics import prepare_with_diagnostics
        loader = prepare_with_diagnostics
    session = WarmupSession(loader)
    try:
        with PipeClient(args.control, args.node_pid, secret, startup) as control:
            with PipeClient(args.media, args.node_pid, secret, startup):
                secret = b""
                expiry = time.monotonic_ns() + 120_000_000_000
                while not session.retired:
                    if time.monotonic_ns() >= expiry:
                        raise TimeoutError("worker preparation deadline")
                    deadline = min(expiry, time.monotonic_ns() + 450_000_000)
                    responses = session.handle(control.read_frame(65536, deadline))
                    # Reply to heartbeat before emitting an asynchronous Ready.
                    responses.extend(session.poll_ready())
                    for response in responses:
                        control.write_frame(response, 65536, deadline)
    finally:
        session.close()
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception:
        # Fixed scalar only; Node records the failure and performs owned cleanup.
        print("WORKER_FAILED_MUTED", file=sys.stderr)
        raise SystemExit(1)
