"""Bounded phase evidence tests; no Torch/GPU or model execution."""
import io
import json
from pathlib import Path
import sys
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch
from prepare_diagnostics import MAX_BYTES, STAGES, StageJournal

class DiagnosticTests(unittest.TestCase):
    def test_closed_order_exact_16_and_no_overwrite(self):
        with tempfile.TemporaryDirectory() as d:
            path = Path(d) / "phase.ndjson"
            clock = iter(range(17))
            with StageJournal(path, lambda: next(clock)) as journal:
                for stage in STAGES:
                    for edge in ("before", "after"):
                        journal.record(stage, edge)
                with self.assertRaisesRegex(ValueError, "record limit"):
                    journal.record("finalize", "after")
            records = [json.loads(line) for line in path.read_bytes().splitlines()]
            self.assertEqual(len(records), 16)
            self.assertEqual(records[-1], {"stage":"finalize", "edge":"after", "elapsed_ns":16})
            self.assertLessEqual(path.stat().st_size, MAX_BYTES)
            before = path.read_bytes()
            with self.assertRaises(FileExistsError):
                StageJournal(path)
            self.assertEqual(path.read_bytes(), before)

    def test_invalid_stage_edge_order_and_clock_do_not_claim_progress(self):
        with tempfile.TemporaryDirectory() as d:
            path = Path(d) / "phase.ndjson"
            with StageJournal(path, lambda: 10) as journal:
                for stage, edge in (("private/path", "before"), (STAGES[0], "unknown"),
                                    (STAGES[1], "before"), (STAGES[0], "after")):
                    with self.assertRaises(ValueError):
                        journal.record(stage, edge)
                journal._clock = lambda: 9
                with self.assertRaisesRegex(ValueError, "clock"):
                    journal.record(STAGES[0], "before")
            self.assertEqual(path.read_bytes(), b"")

    def test_byte_limit_checks_before_write(self):
        with tempfile.TemporaryDirectory() as d:
            path = Path(d) / "phase.ndjson"
            with StageJournal(path, lambda: 0) as journal:
                journal._clock = lambda: 1 << 14000
                with self.assertRaisesRegex(ValueError, "byte limit"):
                    journal.record(STAGES[0], "before")
            self.assertEqual(path.read_bytes(), b"")

    def test_partial_write_or_flush_failure_is_incomplete(self):
        for operation in ("short", "flush"):
            with self.subTest(operation=operation), tempfile.TemporaryDirectory() as d:
                journal = StageJournal(Path(d) / "phase.ndjson", lambda: 0)
                journal._file.close()
                class FailingFile:
                    def write(self, data):
                        return len(data)-1 if operation == "short" else len(data)
                    def flush(self):
                        raise OSError("injected write failure")
                    def close(self):
                        pass
                journal._file = FailingFile()
                with self.assertRaises(OSError):
                    journal.record(STAGES[0], "before")
                self.assertEqual(journal._records, 0)
                journal.__exit__()

    def test_normal_worker_default_does_not_create_journal(self):
        import runtime
        from test_runtime import WARMUP, request
        class FakePipe:
            def __init__(self, *args):
                self.frames = iter((WARMUP, request("Stop")))
            def __enter__(self):
                return self
            def __exit__(self, *_):
                pass
            def read_frame(self, *_):
                return next(self.frames)
            def write_frame(self, *_):
                pass
        # Failure injection only: no real loader, auth, endpoints or CUDA.
        def unavailable(_):
            raise RuntimeError("injected loader")
        with patch("windows_pipe.PipeClient", FakePipe), \
             patch("warmup.prepare_fixed", unavailable), \
             patch.object(StageJournal, "fixed", side_effect=AssertionError("default logging")), \
             patch.object(sys, "argv", ["runtime.py", "--control", "c", "--media", "m", "--node-pid", "1"]), \
             patch.object(sys, "stdin", SimpleNamespace(buffer=io.BytesIO(b"x"*32))):
            self.assertEqual(runtime.main(), 0)

if __name__ == "__main__":
    unittest.main()
