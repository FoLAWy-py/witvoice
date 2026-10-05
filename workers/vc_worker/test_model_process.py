"""Software-only isolation tests: no Torch, real model, media or devices."""
import ctypes
import io
import json
import os
from pathlib import Path
import subprocess
import threading
import time
import unittest
from unittest.mock import patch

import model_process as mp
import model_host as mh

REQUEST = {
    "protocol_version": 1, "request_id": "00000000-0000-0000-0000-000000000011",
    "binding": {"session_tag": "11", "epoch": 1},
    "command": {"kind": "Warmup", "args": {"model_sha256": mp.MODEL_SHA,
                  "reference_id": mp.REFERENCE_ID, "backend": "Cuda"}},
}
CAP = {
    "engine_id": "meanvc2", "model_sha256": mp.MODEL_SHA, "backend": "cuda",
    "native_input_rate": 16000, "native_output_rate": 16000, "chunk_samples": 2560,
    "lookahead_samples": 640, "conditioning_schema": "meanvc2-reference-local-v1",
    "duration_preserving": False,
    "capability_test_run_id": "T011-warmup-" + REQUEST["request_id"],
    "model_memory_budget_bytes": str(6 * 1024 ** 3),
    "device_memory_budget_bytes": str(4 * 1024 ** 3),
}


class Frames(unittest.TestCase):
    def test_oversize_rejected_before_body_read(self):
        calls = []
        def read(count, deadline):
            calls.append(count)
            return (mp.MAX_BYTES + 1).to_bytes(4, "big")
        with self.assertRaises(ValueError):
            mp.read_frame(read, 100, clock=lambda: 0)
        self.assertEqual(calls, [4])

    def test_zero_prefix_rejected_before_body_read(self):
        calls = []
        def read(count, deadline):
            calls.append(count)
            return bytes(4)
        with self.assertRaises(ValueError):
            mp.read_frame(read, 100, clock=lambda: 0)
        self.assertEqual(calls, [4])

    def test_partial_reads_keep_one_deadline(self):
        stream = io.BytesIO(bytes.fromhex("00000003") + b"abc")
        deadlines = []
        def read(count, deadline):
            deadlines.append(deadline)
            return stream.read(min(count, 1))
        self.assertEqual(mp.read_frame(read, 100, clock=lambda: 0), b"abc")
        self.assertEqual(deadlines, [100] * 7)

    def test_partial_prefix_and_body_eof_fail(self):
        for packet in (b"\0\0", bytes.fromhex("00000003") + b"ab"):
            with self.subTest(packet=packet):
                stream = io.BytesIO(packet)
                with self.assertRaises(EOFError):
                    mp.read_frame(lambda count, deadline: stream.read(count),
                                  100, clock=lambda: 0)

    def test_deadline_after_partial_read_does_not_renew(self):
        values = iter((0, 0, 0, 100))
        with self.assertRaises(TimeoutError):
            mp.read_frame(lambda count, deadline: b"x", 100,
                          clock=lambda: next(values))

    def test_partial_writes_keep_one_deadline_and_exact_bytes(self):
        output = bytearray()
        deadlines = []
        def write(part, deadline):
            output.extend(part[:1])
            deadlines.append(deadline)
            return 1
        mp.write_frame(write, b"abc", 100, clock=lambda: 0)
        self.assertEqual(output, bytes.fromhex("00000003") + b"abc")
        self.assertEqual(deadlines, [100] * 7)

    def test_zero_oversize_and_incomplete_write_rejected(self):
        for payload in (b"", bytes(mp.MAX_BYTES + 1)):
            with self.assertRaises(ValueError):
                mp.write_frame(lambda part, deadline: 1, payload, 100, clock=lambda: 0)
        with self.assertRaises(EOFError):
            mp.write_frame(lambda part, deadline: 0, b"x", 100, clock=lambda: 0)


class ClosedResults(unittest.TestCase):
    def test_valid_fixed_request_and_exact_caps(self):
        self.assertEqual(mp.validate_request(json.dumps(REQUEST).encode()), REQUEST)
        self.assertEqual(mp.decode_result(mp.response_payload(REQUEST, "Ready", CAP), REQUEST), CAP)

    def test_unapproved_request_cannot_launch(self):
        wrong = json.loads(json.dumps(REQUEST))
        wrong["command"]["args"]["backend"] = "Cpu"
        with patch.object(mp, "_launch") as launch:
            with self.assertRaises(ValueError):
                mp.prepare_isolated(wrong)
            launch.assert_not_called()

    def test_corrupt_duplicate_and_wrong_binding_result_rejected(self):
        payload = mp.response_payload(REQUEST, "Ready", CAP)
        wrong = json.loads(payload)
        wrong["binding"]["epoch"] = 2
        for invalid in (b"not json", payload.replace(b'"protocol_version":1',
                        b'"protocol_version":1,"protocol_version":1'),
                        json.dumps(wrong).encode()):
            with self.subTest(invalid=invalid[:30]):
                with self.assertRaises(ValueError):
                    mp.decode_result(invalid, REQUEST)

    def test_stale_request_id_and_changed_caps_rejected(self):
        value = json.loads(mp.response_payload(REQUEST, "Ready", CAP))
        value["request_id"] = "00000000-0000-0000-0000-000000000012"
        with self.assertRaises(ValueError):
            mp.decode_result(json.dumps(value).encode(), REQUEST)
        for field, changed in (("lookahead_samples", 639), ("duration_preserving", True),
                               ("model_memory_budget_bytes", "1")):
            value = dict(CAP)
            value[field] = changed
            with self.assertRaises(ValueError):
                mp.decode_result(mp.response_payload(REQUEST, "Ready", value), REQUEST)

    def test_failure_mapping_is_closed(self):
        with self.assertRaises(MemoryError):
            mp.decode_result(mp.response_payload(REQUEST, "MemoryPressure"), REQUEST)
        with self.assertRaises(RuntimeError):
            mp.decode_result(mp.response_payload(REQUEST, "Failed"), REQUEST)
        invalid = {"protocol_version": 1, "request_id": REQUEST["request_id"],
                   "binding": REQUEST["binding"], "event": {"kind": "Heartbeat"}}
        with self.assertRaises(ValueError):
            mp.decode_result(json.dumps(invalid).encode(), REQUEST)
        invalid["event"] = {"kind": "Failed", "args": {"code": "INTERNAL"}}
        with self.assertRaises((ValueError, RuntimeError)):
            mp.decode_result(json.dumps(invalid).encode(), REQUEST)

    def test_no_job_guard_prevents_loader_and_only_fixed_failure(self):
        output = bytearray()
        with patch.object(mh, "in_any_job", return_value=False), patch.object(mh, "_prepare") as loader:
            runner = mh.serve(REQUEST, False,
                lambda part, deadline: (output.extend(part) or len(part)),
                time.monotonic_ns() + 1_000_000_000)
            self.assertIsNone(runner)
            loader.assert_not_called()
        payload = bytes(output[4:])
        with self.assertRaises(RuntimeError):
            mp.decode_result(payload, REQUEST)
        self.assertNotIn(b"model host", payload)

    def test_child_memory_error_maps_without_exception_text(self):
        output = bytearray()
        with patch.object(mh, "in_any_job", return_value=True), patch.object(
                mh, "_prepare", side_effect=MemoryError("secret path must not cross")):
            mh.serve(REQUEST, False,
                     lambda part, deadline: (output.extend(part) or len(part)),
                     time.monotonic_ns() + 1_000_000_000)
        self.assertNotIn(b"secret", output)
        with self.assertRaises(MemoryError):
            mp.decode_result(bytes(output[4:]), REQUEST)


class Ownership(unittest.TestCase):
    def test_receive_failure_closes_owned_process(self):
        process = unittest.mock.Mock()
        process.poll.return_value = None
        process.stdin.fileno.return_value = 1
        with patch.object(mp, "_launch", return_value=process), patch.object(
                mp, "_write_fd", side_effect=lambda fd, part, deadline: len(part)), patch.object(
                mp, "AnonymousReader", side_effect=EOFError):
            with self.assertRaises(EOFError):
                mp.prepare_isolated(REQUEST)
        process.terminate.assert_called_once()
        process.wait.assert_called_once_with(timeout=mp.CLOSE_SECONDS)
        process.stdin.close.assert_called_once()
        process.stdout.close.assert_called_once()

    def test_ready_decode_past_original_deadline_cannot_return_capabilities(self):
        process = unittest.mock.Mock()
        process.poll.return_value = None
        payload = mp.response_payload(REQUEST, "Ready", CAP)
        with patch.object(mp, "_launch", return_value=process), patch.object(
                mp, "write_frame"), patch.object(mp, "AnonymousReader"), patch.object(
                mp, "read_frame", return_value=payload), patch.object(
                mp.time, "monotonic_ns", side_effect=[0, 0, mp.PREPARE_NS]):
            with self.assertRaises(TimeoutError):
                mp.prepare_isolated(REQUEST)
        process.terminate.assert_called_once()
        process.wait.assert_called_once_with(timeout=mp.CLOSE_SECONDS)

    def test_close_timeout_is_not_resource_release_success(self):
        process = unittest.mock.Mock()
        process.poll.return_value = None
        process.wait.side_effect = subprocess.TimeoutExpired("private", 2)
        owner = mp.ModelProcess(process)
        with self.assertRaises(subprocess.TimeoutExpired):
            owner.close()
        process.stdin.close.assert_called_once()
        process.stdout.close.assert_called_once()

    @unittest.skipUnless(os.name == "nt", "Windows native GIL holding software child")
    def test_child_native_gil_sleep_does_not_block_parent_heartbeat_or_stop(self):
        # Test code only: no production selector or model host is invoked.
        script = ("import os,ctypes;os.write(1,b'R');"
                  "k=ctypes.PyDLL('kernel32');k.Sleep.argtypes=[ctypes.c_uint32];"
                  "k.Sleep(900)")
        process = subprocess.Popen([str(mp.BASE_PYTHON), "-I", "-S", "-c", script],
                 stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
                 close_fds=True, creationflags=subprocess.CREATE_NO_WINDOW, bufsize=0)
        owner = mp.ModelProcess(process)
        try:
            reader = mp.AnonymousReader(process.stdout)
            self.assertEqual(reader.read(1, time.monotonic_ns() + 2_000_000_000), b"R")
            from runtime import WarmupSession
            release = threading.Event()
            def load(request):
                release.wait(2)
                return owner, CAP
            session = WarmupSession(load)
            session.handle(json.dumps(REQUEST).encode())
            heartbeat = json.loads(json.dumps(REQUEST))
            heartbeat["command"] = {"kind": "Heartbeat"}
            gaps = []
            for _ in range(5):
                start = time.monotonic()
                responses = session.handle(json.dumps(heartbeat).encode())
                gaps.append(time.monotonic() - start)
                self.assertEqual(json.loads(responses[0])["event"]["kind"], "Heartbeat")
                time.sleep(0.04)
            self.assertIsNone(process.poll())
            self.assertLess(max(gaps), 0.5)
            stop = dict(heartbeat)
            stop["command"] = {"kind": "Stop"}
            start = time.monotonic()
            response = session.handle(json.dumps(stop).encode())
            self.assertEqual(json.loads(response[0])["event"]["kind"], "Stopped")
            self.assertLess(time.monotonic() - start, 0.5)
            # Stop ACK does not wait for load or process release.
            self.assertIsNone(process.poll())
            session.close()
            release.set()
            limit = time.monotonic() + 2.0
            while process.poll() is None and time.monotonic() < limit:
                time.sleep(0.01)
            self.assertIsNotNone(process.poll())
        finally:
            owner.close()


if __name__ == "__main__":
    unittest.main()
