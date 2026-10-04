"""M0 MeanVC2 file/paced experiment. No microphones, playback or network.

Not a production worker. Only the fixed audited upstream and assets may load.
"""
from __future__ import annotations
import argparse
import ctypes
from ctypes import wintypes
from datetime import datetime, timezone
import importlib.util
import json
import math
from pathlib import Path
import subprocess
import sys
import time
import traceback
import zipfile

from audit_assets import ROOT, HF_REVISION, UPSTREAM_COMMIT, audit, sha256

CONFIG_SHA = "ae048d63e63566d36c34d8b669df640f0b2617d1e0d9e6573b651eddf2db2423"
SOURCE = ROOT / ".local/fixtures/source-authorized.wav"
REFERENCE = ROOT / ".local/fixtures/reference-librispeech.wav"


def utc():
    return datetime.now(timezone.utc).isoformat()


def quantile(values, fraction):
    return sorted(values)[max(0, math.ceil(len(values) * fraction) - 1)]


def private_bytes():
    if sys.platform != "win32":
        return None
    class Counters(ctypes.Structure):
        _fields_ = [("cb", wintypes.DWORD), ("PageFaultCount", wintypes.DWORD)] + [
            (name, ctypes.c_size_t) for name in ("PeakWorkingSetSize", "WorkingSetSize", "QuotaPeakPagedPoolUsage",
            "QuotaPagedPoolUsage", "QuotaPeakNonPagedPoolUsage", "QuotaNonPagedPoolUsage", "PagefileUsage", "PeakPagefileUsage", "PrivateUsage")]
    counters = Counters()
    counters.cb = ctypes.sizeof(counters)
    kernel = ctypes.WinDLL("kernel32", use_last_error=True)
    psapi = ctypes.WinDLL("psapi", use_last_error=True)
    kernel.GetCurrentProcess.restype = wintypes.HANDLE
    psapi.GetProcessMemoryInfo.argtypes = [wintypes.HANDLE, ctypes.c_void_p, wintypes.DWORD]
    return counters.PrivateUsage if psapi.GetProcessMemoryInfo(kernel.GetCurrentProcess(), ctypes.byref(counters), counters.cb) else None


def load_runner(device):
    import torch
    report = audit()
    if report["status"] != "VERIFIED_BYTES_NOT_LOADED":
        raise RuntimeError("asset byte/type audit failed")
    upstream = ROOT / ".local/upstream/MeanVC2"
    head = subprocess.run(["git", "-C", str(upstream), "rev-parse", "HEAD"], capture_output=True, text=True, timeout=10, check=True).stdout.strip()
    dirty = subprocess.run(["git", "-C", str(upstream), "diff", "--exit-code"], capture_output=True, timeout=10)
    if head != UPSTREAM_COMMIT or dirty.returncode:
        raise RuntimeError("upstream source not frozen")
    config = ROOT / ".local/models/WavLM-official/wavlm_large_cfg.pt"
    if sha256(config) != CONFIG_SHA:
        raise RuntimeError("config digest mismatch")
    allowed = {str((ROOT / asset["path"]).resolve()) for asset in report["assets"] if asset["format"] == "restricted_state"} | {str(config.resolve())}
    original_load = torch.load
    original_state_load = torch.nn.Module.load_state_dict

    def restricted_load(path, *args, **kwargs):
        if not isinstance(path, (str, Path)) or str(Path(path).resolve()) not in allowed or kwargs.get("weights_only") is False:
            raise RuntimeError("checkpoint not approved for restricted load")
        with zipfile.ZipFile(path) as archive:
            if any("/code/" in item or item.endswith("/constants.pkl") for item in archive.namelist()):
                raise RuntimeError("TorchScript forbidden in state loader")
        kwargs["weights_only"] = True
        return original_load(path, *args, **kwargs)

    def checked_state_load(model, state, *args, **kwargs):
        result = original_state_load(model, state, *args, **kwargs)
        if result.missing_keys or result.unexpected_keys:
            raise RuntimeError("incomplete checkpoint: missing=" + str(result.missing_keys) + " unexpected=" + str(result.unexpected_keys))
        return result

    torch.load = restricted_load
    torch.nn.Module.load_state_dict = checked_state_load
    sys.path.insert(0, str(upstream / "runtime"))
    spec = importlib.util.spec_from_file_location("meanvc2_fixed_runtime", upstream / "runtime/run_rt.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    model_root = ROOT / f".local/models/MeanVC2/{HF_REVISION}"
    module.VOCODER_PATH = str(model_root / "vocos.pt")
    module.SPEAKER_MODEL_PATH = str(ROOT / report["assets"][4]["path"])
    module.WAVLM_CONFIG_PATH = str(config)
    module.MODEL_PATHS["40ms"]["ckpt"] = str(model_root / "meanvc2_40ms_40ms.safetensors")
    module.MODEL_PATHS["40ms"]["asr_ckpt"] = str(model_root / "fastu2pp_80ms.pt")

    # Compatibility repair 1: fixed upstream introduced a legacy mel-cache
    # layer absent from this checkpoint. Its only use requires cache!=None;
    # the fixed streaming runner always passes cache=None (KV cache is separate).
    # Remove its parameters and reject that unused path, then strictly load
    # every remaining/active parameter. Never fabricate missing trained weights.
    class ForbiddenLegacyCache(torch.nn.Module):
        def forward(self, *_args, **_kwargs):
            raise RuntimeError("untrained legacy mel-cache path is forbidden")

    def complete_vc_loader(config_path, ckpt_path, device="cpu"):
        from safetensors.torch import load_file
        with open(config_path, encoding="utf-8") as handle:
            configuration = json.load(handle)
        model = module.DiT(**configuration["model"])
        model.cache_embed = ForbiddenLegacyCache()
        weights = load_file(ckpt_path)
        model.load_state_dict(weights, strict=True)
        return model.to(device).float().eval()

    module._load_vc_model = complete_vc_loader
    runner = module.VCRunner(str(REFERENCE), device=device, model="40ms")
    return runner, report


def run(args):
    import numpy as np
    import soundfile as sf
    import torch
    report = {"schema_version": 1, "task_id": "T002", "requirements": ["REQ-05", "REQ-19", "REQ-32"],
              "kind": "MEANVC2_M0_" + args.mode.upper(), "evidence_level": "C", "started_at": utc(),
              "commit": subprocess.run(["git", "rev-parse", "HEAD"], capture_output=True, text=True, timeout=10, check=True).stdout.strip(),
              "backend_requested": args.device, "result": "FAILED", "windows": "NOT_VERIFIED", "macos": "UNTESTED",
              "quality_review": "QUALITY_REVIEW_PENDING", "distribution": "BLOCKED_LICENSE_CHAIN",
              "not_tested": ["physical_mic", "virtual_cable", "LAN", "Mac", "human_voice_signoff"],
              "measurement_boundary": "fixed file PCM submitted to upstream process_chunk; no audio endpoint/production Node",
              "command": sys.argv}
    started = time.perf_counter()
    try:
        if sha256(SOURCE) != "7a4172d80ae660e173c3b059c4ae855fc088deab4bed20b4437624f3fdee3436" or sha256(REFERENCE) != "8954b7c5a7758b06fc305904596ad36633696235b6fc31a8bf3a636250b48921":
            raise RuntimeError("fixture digest mismatch")
        if args.device == "cuda" and not torch.cuda.is_available():
            raise RuntimeError("requested CUDA is unavailable; no silent backend fallback")
        report["torch"] = torch.__version__
        report["python"] = sys.version
        if args.device == "cuda":
            torch.cuda.reset_peak_memory_stats()
            report["gpu"] = torch.cuda.get_device_name()
        source, rate = sf.read(SOURCE, dtype="float32")
        reference, ref_rate = sf.read(REFERENCE, dtype="float32")
        if rate != 16000 or ref_rate != 16000 or source.ndim != 1 or reference.ndim != 1 or not 10 <= len(reference) / ref_rate <= 30:
            raise RuntimeError("unsupported fixed fixture format")
        init_start = time.perf_counter()
        runner, assets = load_runner(args.device)
        report["initialization_seconds"] = time.perf_counter() - init_start
        report["assets"] = assets
        report["backend_actual"] = str(next(runner.vc.parameters()).device)
        report["speaker_backend_actual"] = str(next(runner.spk_model.parameters()).device)
        report["asr_backend_actual"] = "cpu"
        report["private_bytes_after_init"] = private_bytes()
        report["runtime_input_chunk_samples"] = runner.CHUNK
        report["compatibility_repairs"] = ["1: reject unused untrained legacy mel-cache module; active checkpoint strict=True"]
        report["native_input_rate"] = rate
        # Upstream runtime writes 16k and uses 160-sample vocoder hop per 10ms.
        report["native_output_rate"] = 16000
        output_path = ROOT / (".local/fixtures/converted-" + args.mode + ".wav")
        if output_path.exists():
            raise RuntimeError("output already exists; preserve prior run")
        if args.mode == "file":
            runner.process_file(str(SOURCE), str(output_path))
        else:
            warm_start = time.perf_counter()
            for pos in range(0, len(source), runner.CHUNK):
                runner.process_chunk(source[pos:pos + runner.CHUNK])
            if args.device == "cuda": torch.cuda.synchronize()
            report["warmup_seconds"] = time.perf_counter() - warm_start
            runner._init_cache()
            torch.manual_seed(42)
            total_samples = int(args.seconds * rate)
            # New source timeline, with repeated utterance fixtures; not repeated context.
            samples = np.resize(source, total_samples)
            steps, outputs = [], []
            step_samples = runner.CHUNK
            origin = time.perf_counter()
            for position in range(0, total_samples, step_samples):
                chunk = samples[position:position + step_samples]
                ready_at = origin + (position + len(chunk)) / rate
                if time.perf_counter() < ready_at:
                    time.sleep(ready_at - time.perf_counter())
                begin = time.perf_counter()
                converted = runner.process_chunk(chunk)
                if args.device == "cuda": torch.cuda.synchronize()
                end = time.perf_counter()
                steps.append({"new_samples": len(chunk), "processing_ms": (end - begin) * 1000, "lag_ms": max(0, (begin - ready_at) * 1000), "output_samples": 0 if converted is None else len(converted)})
                if converted is not None:
                    if not np.isfinite(converted).all(): raise RuntimeError("nonfinite converted samples")
                    outputs.append(converted)
            if not outputs: raise RuntimeError("stream produced no converted audio")
            sf.write(output_path, np.concatenate(outputs), 16000, subtype="FLOAT")
            durations = [step["processing_ms"] for step in steps]
            new_seconds = sum(step["new_samples"] for step in steps) / rate
            rtf = sum(durations) / (new_seconds * 1000)
            p99 = quantile(durations, 0.99)
            report["steps"] = steps
            report["statistics"] = {"sample_count": len(steps), "average_rtf": rtf, "p50_step_ms": quantile(durations, 0.5), "p95_step_ms": quantile(durations, 0.95), "p99_step_ms": p99, "new_input_seconds": new_seconds, "max_lag_ms": max(step["lag_ms"] for step in steps)}
            if rtf > 0.70 or p99 > step_samples / rate * 1000 or report["statistics"]["max_lag_ms"] > step_samples / rate * 1000:
                raise RuntimeError("stream hard threshold failed; no reduced gate")
        converted, output_rate = sf.read(output_path, dtype="float32")
        if converted.ndim != 1 or not len(converted) or not np.isfinite(converted).all() or np.max(np.abs(converted)) == 0:
            raise RuntimeError("invalid or blank output")
        report["output"] = {"path": str(output_path.relative_to(ROOT)), "sha256": sha256(output_path), "samples": len(converted), "sample_rate": output_rate, "duration_seconds": len(converted) / output_rate, "peak": float(np.max(np.abs(converted))), "rms": float(np.sqrt(np.mean(converted.astype(np.float64) ** 2))), "finite": True}
        report["result"] = "PASS_EXPERIMENT_ONLY"
        report["windows"] = "EXPERIMENT_EXECUTED_NOT_PRODUCT_VERIFIED"
    except Exception as exc:
        report["error"] = str(exc)
        report["traceback"] = traceback.format_exc()
    report["elapsed_seconds"] = time.perf_counter() - started
    report["finished_at"] = utc()
    report["private_bytes_end"] = private_bytes()
    if args.device == "cuda" and torch.cuda.is_initialized():
        report["gpu_peak_allocated_bytes"] = torch.cuda.max_memory_allocated()
        report["gpu_peak_reserved_bytes"] = torch.cuda.max_memory_reserved()
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--mode", choices=["file", "paced"], required=True)
    parser.add_argument("--device", choices=["cpu", "cuda"], required=True)
    parser.add_argument("--seconds", type=int, default=60)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    output = args.output.resolve()
    if not output.is_relative_to(ROOT / "docs/evidence/model-feasibility") or output.exists() or not 30 <= args.seconds <= 120:
        raise SystemExit("invalid output or finite test duration")
    report = run(args)
    output.parent.mkdir(parents=True, exist_ok=True)
    with output.open("x", encoding="utf-8") as handle:
        json.dump(report, handle, indent=2, allow_nan=False)
    print(json.dumps({key: value for key, value in report.items() if key not in ("assets", "steps")}, indent=2, allow_nan=False))
    return 0 if report["result"] == "PASS_EXPERIMENT_ONLY" else 2


if __name__ == "__main__":
    raise SystemExit(main())
