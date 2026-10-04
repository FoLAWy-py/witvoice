"""Extract primitive WavLM cfg only after fixed byte/type audit; no GPU use."""
import hashlib
import json
from pathlib import Path
import torch

from audit_assets import ROOT, audit


def primitive(value):
    if value is None or type(value) in (str, int, float, bool):
        return value
    if type(value) in (tuple, list):
        return [primitive(item) for item in value]
    if type(value) is dict and all(type(key) is str for key in value):
        return {key: primitive(item) for key, item in value.items()}
    raise ValueError("config contains unsupported nonprimitive type")


def main():
    report = audit()
    if report["status"] != "VERIFIED_BYTES_NOT_LOADED":
        raise SystemExit("fixed assets not verified")
    base = ROOT / ".local/models/WavLM-official/WavLM-Large-drive.pt.partial"
    # Type audit explicitly rejected TorchScript before this restricted load.
    checkpoint = torch.load(base, map_location="cpu", weights_only=True)
    if type(checkpoint) is not dict or "cfg" not in checkpoint:
        raise SystemExit("official checkpoint has no cfg")
    config = primitive(checkpoint["cfg"])
    if type(config) is not dict or config.get("encoder_layers") != 24 or config.get("encoder_embed_dim") != 1024:
        raise SystemExit("unexpected WavLM Large architecture")
    destination = ROOT / ".local/models/WavLM-official/wavlm_large_cfg.pt"
    if destination.exists():
        raise SystemExit("config already exists; preserve existing bytes")
    torch.save(config, destination)
    summary = {"torch": torch.__version__, "load": "weights_only=True; code-free archive",
               "base_sha256": report["assets"][3]["sha256"], "config": config,
               "config_sha256": hashlib.sha256(destination.read_bytes()).hexdigest(),
               "config_bytes": destination.stat().st_size, "model_inference": "NOT_RUN"}
    record = destination.with_suffix(".json")
    with record.open("x", encoding="utf-8") as handle:
        json.dump(summary, handle, indent=2, allow_nan=False)
    print(json.dumps(summary, indent=2, allow_nan=False))


if __name__ == "__main__":
    main()
