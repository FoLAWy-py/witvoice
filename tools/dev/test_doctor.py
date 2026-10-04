"""Negative-path checks for doctor evidence, no third-party dependencies."""
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

import doctor


class DoctorTests(unittest.TestCase):
    def test_nonzero_is_not_pass(self):
        result = doctor.probe([sys.executable, "-c", "import sys; print('failed probe'); sys.exit(7)"])
        self.assertEqual((result["status"], result["exit_code"]), ("FAILED", 7))
        self.assertIn("failed probe", result["stdout"])

    def test_timeout_not_pass(self):
        result = doctor.probe([sys.executable, "-c", "import time; time.sleep(10)"], timeout=0.1)
        self.assertEqual(result["status"], "TIMEOUT")
        self.assertIsNone(result["exit_code"])
        self.assertLess(result["elapsed_seconds"], 5)

    def test_nonexistent_executable_unknown(self):
        result = doctor.probe(["witvoice-nonexistent-probe-58d32.exe"])
        self.assertEqual(result["status"], "UNKNOWN")
        self.assertIsNone(result["exit_code"])

    def test_output_bounded(self):
        result = doctor.probe([sys.executable, "-c", "print('x' * 100000)"])
        self.assertEqual(result["status"], "PASS")
        self.assertEqual(len(result["stdout"]), doctor.OUTPUT_LIMIT)
        self.assertTrue(result["stdout_truncated"])

    def test_unicode_output_preserved(self):
        result = doctor.probe([sys.executable, "-X", "utf8", "-c", "print('Windows 专业版')"])
        self.assertEqual(result["status"], "PASS")
        self.assertEqual(result["stdout"], "Windows 专业版")

    def test_missing_and_wrong_host_block(self):
        checks = {"rustc": {"status": "PASS", "stdout": "host: x86_64-unknown-linux-gnu"},
                  "msvc": doctor.missing("no compiler")}
        result, blockers = doctor.assess(checks)
        self.assertEqual(result, "BLOCKED")
        self.assertEqual(set(blockers), {"msvc", "rust_msvc_host"})

    def test_inventory_invalid_json_unknown(self):
        with patch.object(doctor, "find_exe", return_value="pwsh.exe"), patch.object(
            doctor, "probe", return_value={"status": "PASS", "stdout": "not JSON"}
        ):
            self.assertEqual(doctor.inventory()["status"], "UNKNOWN")

    def test_report_preserves_existing_file_and_restricts_destination(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            with patch.object(doctor, "ROOT", root):
                path = root / "docs/evidence/probe.json"
                doctor.write_report(path, {"result": "BLOCKED"})
                with self.assertRaises(FileExistsError):
                    doctor.write_report(path, {"result": "PASS"})
                self.assertEqual(json.loads(path.read_text())["result"], "BLOCKED")
                with self.assertRaises(ValueError):
                    doctor.write_report(root / "outside.json", {})


if __name__ == "__main__":
    unittest.main()
