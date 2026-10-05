"""Read-only memory accounting tests, no model or audio hardware."""
import ctypes
import os
import sys
import unittest
from process_memory import Counters, model_tree_private_bytes, process_private_bytes

class MemoryTests(unittest.TestCase):
    def test_both_processes_count_and_unknown_never_becomes_zero(self):
        seen = []
        def measured(pid):
            seen.append(pid)
            return 100 if pid == os.getpid() else 200
        self.assertEqual(model_tree_private_bytes(measured), 300)
        self.assertEqual(seen, [os.getpid(), os.getppid()])
        for unknown in (None, -1, True, 1.5):
            self.assertIsNone(model_tree_private_bytes(lambda pid: 100 if pid == os.getpid() else unknown))

    def test_invalid_pid_rejected_before_open(self):
        for value in (0, -1, True, 1.0, 0x100000000):
            self.assertIsNone(process_private_bytes(value))

    @unittest.skipUnless(sys.platform == "win32", "actual Windows query only")
    def test_native_limited_query_self_and_parent(self):
        self.assertEqual(ctypes.sizeof(Counters), 80)
        own = process_private_bytes(os.getpid())
        combined = model_tree_private_bytes()
        self.assertIsInstance(own, int)
        self.assertGreater(own, 0)
        self.assertIsInstance(combined, int)
        self.assertGreater(combined, own)

if __name__ == "__main__":
    unittest.main()
