"""Negative-path checks for doctor evidence, no third-party dependencies."""
import json
import os
from pathlib import Path
import sys
import tempfile
import time
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

    def test_inventory_wrong_json_shapes_unknown(self):
        for value in (None, [], 42, "text"):
            with self.subTest(value=value), patch.object(doctor, "find_exe", return_value="pwsh.exe"), patch.object(
                doctor, "probe", return_value={"status": "PASS", "stdout": json.dumps(value)}
            ):
                self.assertEqual(doctor.inventory()["status"], "UNKNOWN")

    def test_vswhere_wrong_json_shapes_unknown(self):
        for value in (None, {}, [None], [{}], [{"installationPath": 1}]):
            with self.subTest(value=value), patch.object(Path, "is_file", return_value=True), patch.object(
                doctor, "probe", return_value={"status": "PASS", "stdout": json.dumps(value)}
            ):
                self.assertEqual(doctor.msvc()["status"], "UNKNOWN")

    def test_collect_invalid_inventory_shape_is_blocked(self):
        with patch.object(doctor, "inventory", return_value={"status": "UNKNOWN", "data": []}), patch.object(
            doctor, "git_snapshot", return_value={"commit": None, "dirty": None}
        ), patch.object(doctor, "find_exe", return_value=None), patch.object(
            doctor, "version", return_value=doctor.missing("test missing")
        ), patch.object(doctor, "node_cli", return_value=doctor.missing("test missing")), patch.object(
            doctor, "msvc", return_value=doctor.missing("test missing")
        ):
            result = doctor.collect()
        self.assertEqual(result["result"], "BLOCKED")
        self.assertEqual(result["environment"], {})
        self.assertIn("windows_powershell", result["blockers"])

    def test_shallow_node_cli_paths(self):
        for wrapper in ("C:/pnpm.cmd", "C:/nodejs/pnpm.cmd", "D:/Software/nodejs/pnpm.cmd"):
            with self.subTest(wrapper=wrapper), patch.object(doctor, "find_exe", return_value=wrapper), patch.object(
                Path, "is_file", return_value=True
            ), patch.object(doctor, "probe", return_value={"status": "PASS", "stdout": "11.19.0"}):
                self.assertEqual(doctor.node_cli("pnpm", "node.exe")["status"], "PASS")
            with self.subTest(missing=wrapper), patch.object(doctor, "find_exe", return_value=wrapper), patch.object(
                Path, "is_file", return_value=False
            ):
                self.assertEqual(doctor.node_cli("pnpm", "node.exe")["status"], "UNKNOWN")

    @unittest.skipUnless(os.name == "nt", "Windows descendant cleanup requires Windows")
    def test_windows_timeout_kills_descendant(self):
        with tempfile.TemporaryDirectory() as directory:
            marker = Path(directory) / "descendant-wrote.txt"
            child = "import time; from pathlib import Path; time.sleep(1.5); Path(" + repr(str(marker)) + ").write_text('orphan')"
            parent = "import subprocess,sys,time; child=subprocess.Popen([sys.executable,'-c'," + repr(child) + "]); print(child.pid,flush=True); time.sleep(10)"
            result = doctor.probe([sys.executable, "-c", parent], timeout=0.5)
            self.assertEqual(result["status"], "TIMEOUT")
            self.assertTrue(result["stdout"].isdigit())
            self.assertEqual(result["tree_cleanup_exit_code"], 0)
            time.sleep(1.7)
            self.assertFalse(marker.exists(), "Timed-out descendant continued writing")

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
