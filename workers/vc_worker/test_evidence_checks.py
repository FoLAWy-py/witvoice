import unittest
import importlib.util
from pathlib import Path
import sys
from types import SimpleNamespace
from unittest.mock import Mock, patch
from evidence_checks import module_devices, paced_statistics, require_safe_torch


class EvidenceChecks(unittest.TestCase):
    def module(self, parameters, buffers):
        return SimpleNamespace(parameters=lambda: iter(SimpleNamespace(device=d) for d in parameters),
                               buffers=lambda: iter(SimpleNamespace(device=d) for d in buffers))

    def test_devices_cover_buffers_and_unknown(self):
        self.assertEqual(module_devices(self.module([], ["cpu"]))["actual"], "cpu")
        self.assertEqual(module_devices(self.module([], []))["actual"], "UNKNOWN")
        self.assertEqual(module_devices(self.module(["cuda:0"], ["cpu"]))["actual"], "MIXED")

    def test_short_tail_is_measured_against_its_new_input(self):
        stats = paced_statistics([{"new_samples": 2560, "processing_ms": 100, "lag_ms": 0},
                                  {"new_samples": 160, "processing_ms": 20, "lag_ms": 11}], 16000)
        self.assertEqual(stats["p99_step_rtf"], 2)
        self.assertEqual(stats["max_lag_chunk_ratio"], 1.1)

    def test_old_and_unknown_torch_block_before_loading(self):
        for version in ("2.5.1+cu121", "2.6.0+cu124", "2.9.1", "2.10.0a0", "unknown"):
            with self.assertRaises(RuntimeError):
                require_safe_torch(version)
        require_safe_torch("2.10.0+cu126")

    def test_both_entrypoints_reject_before_any_checkpoint_load(self):
        for version in ("2.5.1+cu121", "2.9.1", "unknown"):
            for name in ("prepare_config", "feasibility"):
                fake = SimpleNamespace(__version__=version, load=Mock(), jit=SimpleNamespace(load=Mock()))
                with patch.dict(sys.modules, {"torch": fake}):
                    spec = importlib.util.spec_from_file_location("guard_test_" + name, Path(__file__).with_name(name + ".py"))
                    module = importlib.util.module_from_spec(spec)
                    spec.loader.exec_module(module)
                    with patch.object(module, "audit") as audited:
                        with self.assertRaises(RuntimeError):
                            module.main() if name == "prepare_config" else module.load_runner("cpu")
                        audited.assert_not_called()
                fake.load.assert_not_called()
                fake.jit.load.assert_not_called()

    def test_legal_30_seconds_tail_misses_p99_gate(self):
        steps = [{"new_samples": 2560, "processing_ms": 100, "lag_ms": 0} for _ in range(187)]
        steps[0]["processing_ms"] = 170
        steps.append({"new_samples": 1280, "processing_ms": 100, "lag_ms": 0})
        stats = paced_statistics(steps, 16000)
        self.assertEqual(stats["new_input_seconds"], 30)
        self.assertLess(stats["average_rtf"], .7)
        self.assertEqual(stats["p99_step_ms"], 100)
        self.assertGreater(stats["p99_step_rtf"], 1)

    def test_invalid_steps_rejected(self):
        for samples, processing in ((0, 1), (1, float("nan")), (1, -1)):
            with self.assertRaises(ValueError):
                paced_statistics([{"new_samples": samples, "processing_ms": processing, "lag_ms": 0}], 16000)


if __name__ == "__main__":
    unittest.main()
