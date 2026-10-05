"""Control ownership tests with injected model work, not real model evidence."""
import json
import threading
import time
import unittest
from runtime import MODEL_SHA, REFERENCE_ID, WarmupSession


def request(kind, epoch=1, args=None):
    command = {"kind": kind}
    if args is not None:
        command["args"] = args
    return json.dumps({"protocol_version": 1,
                       "request_id": "00000000-0000-0000-0000-000000000011",
                       "binding": {"session_tag": "1", "epoch": epoch},
                       "command": command}).encode()


WARMUP = request("Warmup", args={"model_sha256": MODEL_SHA,
                                  "reference_id": REFERENCE_ID, "backend": "Cuda"})


class RuntimeTests(unittest.TestCase):
    def test_reset_allocations_exceeding_ready_budget_never_publish_ready(self):
        from warmup import DEVICE_BUDGET, HOST_BUDGET, finalize_preparation
        for exceed_host in (True, False):
            with self.subTest(exceed_host=exceed_host):
                memory = {"host": HOST_BUDGET - 1, "device": DEVICE_BUDGET - 1}
                class AllocatingModel:
                    def _init_cache(self):
                        memory["host" if exceed_host else "device"] += 2
                def preparation(_):
                    runner = AllocatingModel()
                    finalize_preparation(runner, lambda: None, lambda: memory["host"],
                                         lambda: memory["device"])
                    self.fail("over-budget reset returned Ready")
                session = WarmupSession(preparation)
                session.handle(WARMUP)
                until = time.monotonic() + 1
                responses = []
                while not responses and time.monotonic() < until:
                    responses = session.poll_ready()
                    time.sleep(.001)
                self.assertEqual(json.loads(responses[0])["event"], {"kind": "MemoryPressure"})
                self.assertTrue(session.retired)
                session.close()

    def test_stop_never_destroys_ready_or_queued_model_on_control_owner(self):
        for publish_ready in (False, True):
            with self.subTest(publish_ready=publish_ready):
                destructing = threading.Event()
                destroyed = threading.Event()
                allow_destruction = threading.Event()
                destructor_owner = []
                class BlockingModel:
                    def __del__(self):
                        destructor_owner.append(threading.get_ident())
                        destructing.set()
                        allow_destruction.wait(2)
                        destroyed.set()
                cap = {
                    "engine_id": "meanvc2", "model_sha256": MODEL_SHA, "backend": "cuda",
                    "native_input_rate": 16000, "native_output_rate": 16000,
                    "chunk_samples": 2560, "lookahead_samples": 640,
                    "conditioning_schema": "test-only-fixture", "duration_preserving": False,
                    "capability_test_run_id": "test-only-no-model",
                    "model_memory_budget_bytes": "1", "device_memory_budget_bytes": "1",
                }
                session = WarmupSession(lambda _: (BlockingModel(), cap))
                session.handle(WARMUP)
                until = time.monotonic() + 1
                while session._result.empty() and time.monotonic() < until:
                    time.sleep(.001)
                self.assertFalse(session._result.empty())
                if publish_ready:
                    self.assertEqual(json.loads(session.poll_ready()[0])["event"]["kind"], "Ready")
                stopped = session.handle(request("Stop"))
                self.assertEqual(json.loads(stopped[0])["event"]["kind"], "Stopped")
                self.assertFalse(destructing.is_set())
                session.close()
                try:
                    self.assertTrue(destructing.wait(1))
                    self.assertNotEqual(destructor_owner, [threading.get_ident()])
                finally:
                    allow_destruction.set()
                    self.assertTrue(destroyed.wait(1))

    def test_process_owner_cleanup_waits_for_stop_release_off_control(self):
        closing = threading.Event()
        closed = threading.Event()
        allow = threading.Event()
        owners = []
        class ProcessOwner:
            def close(self):
                owners.append(threading.get_ident())
                closing.set()
                allow.wait(2)
                closed.set()
        cap = {
            "engine_id": "meanvc2", "model_sha256": MODEL_SHA, "backend": "cuda",
            "native_input_rate": 16000, "native_output_rate": 16000,
            "chunk_samples": 2560, "lookahead_samples": 640,
            "conditioning_schema": "test-only-fixture", "duration_preserving": False,
            "capability_test_run_id": "test-only-no-model",
            "model_memory_budget_bytes": "1", "device_memory_budget_bytes": "1",
        }
        session = WarmupSession(lambda _: (ProcessOwner(), cap))
        session.handle(WARMUP)
        until = time.monotonic() + 1
        while session._result.empty() and time.monotonic() < until:
            time.sleep(.001)
        self.assertFalse(session._result.empty())
        self.assertEqual(json.loads(session.poll_ready()[0])["event"]["kind"], "Ready")
        self.assertEqual(json.loads(session.handle(request("Stop"))[0])["event"]["kind"], "Stopped")
        self.assertFalse(closing.is_set())
        session.close()
        try:
            self.assertTrue(closing.wait(1))
            self.assertNotEqual(owners, [threading.get_ident()])
        finally:
            allow.set()
            self.assertTrue(closed.wait(1))

    def test_ready_only_after_completion_and_cannot_repeat(self):
        entered = threading.Event()
        release = threading.Event()
        completed = threading.Event()
        capabilities = {
            "engine_id": "meanvc2", "model_sha256": MODEL_SHA, "backend": "cuda",
            "native_input_rate": 16000, "native_output_rate": 16000,
            "chunk_samples": 2560, "lookahead_samples": 640,
            "conditioning_schema": "test-only-fixture", "duration_preserving": False,
            "capability_test_run_id": "test-only-no-model",
            "model_memory_budget_bytes": "1", "device_memory_budget_bytes": "1",
        }
        def delayed(_):
            entered.set()
            release.wait(2)
            completed.set()
            return object(), capabilities
        session = WarmupSession(delayed)
        session.handle(WARMUP)
        self.assertTrue(entered.wait(1))
        self.assertEqual(session.poll_ready(), [])
        release.set()
        self.assertTrue(completed.wait(1))
        until = time.monotonic() + 1
        ready = []
        while not ready and time.monotonic() < until:
            ready = session.poll_ready()
            time.sleep(.001)
        event = json.loads(ready[0])
        self.assertEqual(event["request_id"], json.loads(WARMUP)["request_id"])
        self.assertEqual(event["event"]["kind"], "Ready")
        self.assertEqual(session.poll_ready(), [])
        session.handle(request("Stop"))
        self.assertTrue(session.retired)
        session.close()

    def test_blocked_model_does_not_block_control_or_stop(self):
        entered = threading.Event()
        release = threading.Event()
        def blocked(_):
            entered.set()
            release.wait(2)
            raise RuntimeError("private model error must not be disclosed")
        session = WarmupSession(blocked)
        self.assertEqual(session.handle(WARMUP), [])
        self.assertTrue(entered.wait(1))
        try:
            heartbeat = json.loads(session.handle(request("Heartbeat"))[0])
            self.assertEqual(heartbeat["event"], {"kind": "Heartbeat"})
            with self.assertRaises(ValueError):
                session.handle(request("Heartbeat", epoch=2))
            with self.assertRaises(ValueError):
                session.handle(WARMUP)
            self.assertEqual(json.loads(session.handle(request("Stop"))[0])["event"]["kind"], "Stopped")
            self.assertEqual(session.poll_ready(), [])
            self.assertTrue(session.retired)
        finally:
            release.set()
            session.close()

    def test_model_failure_is_fixed_scalar_and_never_ready(self):
        def failing(_):
            raise RuntimeError("private source path and checkpoint detail")
        session = WarmupSession(failing)
        session.handle(WARMUP)
        until = time.monotonic() + 1
        result = []
        while not result and time.monotonic() < until:
            result = session.poll_ready()
            time.sleep(.001)
        self.assertEqual(json.loads(result[0])["event"],
                         {"kind": "Failed", "args": {"code": "ENGINE_FAILED"}})
        self.assertTrue(session.retired)
        self.assertNotIn(b"private", result[0])

    def test_rejects_unapproved_reference_before_model_work(self):
        called = []
        session = WarmupSession(lambda _: called.append(True))
        invalid = json.loads(WARMUP)
        invalid["command"]["args"]["reference_id"] = "00000000-0000-0000-0000-000000000012"
        with self.assertRaises(ValueError):
            session.handle(json.dumps(invalid).encode())
        self.assertEqual(called, [])
        with self.assertRaises(ValueError):
            session.handle(request("Heartbeat"))


if __name__ == "__main__":
    unittest.main()
