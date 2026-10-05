"""Real fixed MeanVC2 preparation; no endpoints, downloads or PCM files written."""
from runtime import MODEL_SHA, REFERENCE_ID

HOST_BUDGET = 8 * 1024 ** 3  # CR-0006: explicit human-approved warmup host gate
DEVICE_BUDGET = 4 * 1024 ** 3


def finalize_preparation(runner, synchronize, host_bytes, device_bytes, resource_observer=None):
    # Cache reset itself allocates. The Ready check must follow the final reset.
    runner._init_cache()
    synchronize()
    host = host_bytes()
    device = device_bytes()
    if resource_observer is not None:
        resource_observer(host, device)  # scalar diagnostics on model task only
    if host is None:
        raise RuntimeError("memory budget cannot be checked")
    if type(host) is not int or type(device) is not int or host < 0 or device < 0:
        raise RuntimeError("memory budget measurement invalid")
    if host > HOST_BUDGET or device > DEVICE_BUDGET:
        raise MemoryError("warmup resource budget exceeded")


def prepare_fixed(request, observer=None, resource_observer=None):
    def phase(stage, edge):
        if observer is not None:
            observer(stage, edge)
    # This function runs only on the single model task, never the control owner.
    phase("imports_array_audio", "before")
    import numpy as np
    import soundfile as sf
    phase("imports_array_audio", "after")
    phase("imports_torch", "before")
    import torch
    phase("imports_torch", "after")
    phase("imports_adapter", "before")
    from audit_assets import sha256
    from evidence_checks import module_devices, require_safe_torch
    from feasibility import SOURCE, REFERENCE, load_runner
    from process_memory import model_tree_private_bytes

    phase("imports_adapter", "after")
    phase("fixtures_check", "before")
    args = request["command"]["args"]
    if (args["model_sha256"], args["reference_id"], args["backend"]) != (
            MODEL_SHA, REFERENCE_ID, "Cuda"):
        raise ValueError("unapproved preparation")
    require_safe_torch(torch.__version__)
    if not torch.cuda.is_available():
        raise RuntimeError("verified CUDA unavailable")
    if sha256(SOURCE) != "7a4172d80ae660e173c3b059c4ae855fc088deab4bed20b4437624f3fdee3436":
        raise ValueError("authorized source changed")
    if sha256(REFERENCE) != "8954b7c5a7758b06fc305904596ad36633696235b6fc31a8bf3a636250b48921":
        raise ValueError("authorized reference changed")
    source, rate = sf.read(SOURCE, dtype="float32")
    reference, ref_rate = sf.read(REFERENCE, dtype="float32")
    if (rate != 16000 or ref_rate != 16000 or source.ndim != 1 or reference.ndim != 1
            or len(source) < 3 * 2560 or not 10 <= len(reference) / ref_rate <= 30
            or not np.isfinite(source).all() or not np.isfinite(reference).all()):
        raise ValueError("fixed authorized fixture format")
    phase("fixtures_check", "after")
    phase("model_load", "before")
    runner, _audit = load_runner("cuda")
    phase("model_load", "after")
    phase("placement_validate", "before")
    for model in (runner.vc, runner.spk_model, runner.vocoder):
        if not module_devices(model)["actual"].startswith("cuda:"):
            raise RuntimeError("actual CUDA model placement not verified")
    if module_devices(runner.asr)["actual"] != "cpu":
        raise RuntimeError("actual ASR placement changed")
    if (runner.CHUNK, runner.block_size, runner.upsample_factor) != (2560, 4, 160):
        raise RuntimeError("fixed runtime shape changed")
    phase("placement_validate", "after")
    phase("convert_warmup", "before")
    produced = 0
    for offset in range(0, 3 * 2560, 2560):
        result = runner.process_chunk(source[offset:offset + 2560])
        if result is not None:
            if result.ndim != 1 or not np.isfinite(result).all() or not 1 <= len(result) <= 4096:
                raise RuntimeError("invalid real warmup output")
            if np.max(np.abs(result)) > 0:
                produced += len(result)
    if not produced or not runner.vc.streaming_guard_calls:
        raise RuntimeError("real warmup did not execute conversion")
    phase("convert_warmup", "after")
    phase("finalize", "before")
    finalize_preparation(runner, torch.cuda.synchronize, model_tree_private_bytes,
                         torch.cuda.memory_reserved, resource_observer)
    phase("finalize", "after")
    capabilities = {
        "engine_id": "meanvc2", "model_sha256": MODEL_SHA, "backend": "cuda",
        "native_input_rate": 16000, "native_output_rate": 16000, "chunk_samples": 2560,
        # VC future context only; ASR/transport/end-to-end delay are not measured here.
        "lookahead_samples": runner.block_size * runner.upsample_factor,
        "conditioning_schema": "meanvc2-reference-local-v1", "duration_preserving": False,
        "capability_test_run_id": "T011-warmup-" + request["request_id"],
        "model_memory_budget_bytes": str(HOST_BUDGET),
        "device_memory_budget_bytes": str(DEVICE_BUDGET),
    }
    return runner, capabilities
