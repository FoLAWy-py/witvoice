"""Offline fixed-asset audit; never imports Torch or executes downloaded code."""
from __future__ import annotations
import argparse
import hashlib
import json
from pathlib import Path
import zipfile

ROOT = Path(__file__).resolve().parents[2]
HF_REVISION = "39cdd19522fe896c227da691314d9a0e3b995486"
UPSTREAM_COMMIT = "13acf84c1bf135ea5edad9c245b345289b06b33e"
ASSETS = [
    (f".local/models/MeanVC2/{HF_REVISION}/fastu2pp_80ms.pt", 246292087, "4338739bd13e0f7718276373e9547ce1f9aa02ec0867452860df9e837f90e4d2", "torchscript", "ASLP-lab/MeanVC2 official LFS SHA256", "Apache-2.0 model card"),
    (f".local/models/MeanVC2/{HF_REVISION}/meanvc2_40ms_40ms.safetensors", 70839448, "01caafec9a3991a5514df9412952d24ae3370406358a01e20e03df41f2f5d515", "safetensors", "ASLP-lab/MeanVC2 official LFS SHA256", "Apache-2.0 model card"),
    (f".local/models/MeanVC2/{HF_REVISION}/vocos.pt", 33223674, "df1f2ba9f7ac35c96832579421ee1ed913a68c3a1d2f8a6536739f902f194a93", "torchscript", "ASLP-lab/MeanVC2 official LFS SHA256", "Apache-2.0 model card"),
    (".local/models/WavLM-official/WavLM-Large-drive.pt.partial", 1261965425, "6fb4b3c3e6aa567f0a997b30855859cb81528ee8078802af439f7b2da0bf100f", "restricted_state", "Microsoft WavLM README Drive ID12-cB34qCTvByWT-QtOcZaqwwO21FLSqU; local acquisition hash, NOT publisher digest", "Microsoft unilm project MIT; original checkpoint config source"),
    (".local/models/WavLM-official/wavlm_large_finetune.pth.partial", 1301926579, "51f07e3b94d9e0262a6a675ef5a087be3dd09e8c62e9d886827f44f82fe7f94b", "restricted_state", "Microsoft UniSpeech official Drive ID1-aE1NfzpRCLxA4GUxX9ITI3F9LlbtEGP; local acquisition hash, NOT publisher digest", "UniSpeech project CC-BY-SA-3.0; separate binary terms UNKNOWN; local official experiment only"),
]


def sha256(path):
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        while chunk := handle.read(1024 * 1024):
            digest.update(chunk)
    return digest.hexdigest()


def audit():
    report = {"upstream_commit": UPSTREAM_COMMIT, "hf_revision": HF_REVISION,
              "status": "VERIFIED_BYTES_NOT_LOADED", "assets": [],
              "distribution": "BLOCKED_LICENSE_CHAIN", "windows_model": "NOT_RUN", "macos_model": "UNTESTED"}
    for relative, size, expected, form, source, license_note in ASSETS:
        path = ROOT / relative
        record = {"path": relative, "expected_bytes": size, "sha256": expected,
                  "source": source, "license_note": license_note, "format": form, "status": "MISSING"}
        if path.is_file():
            record["actual_bytes"] = path.stat().st_size
            record["actual_sha256"] = sha256(path)
            record["status"] = "VERIFIED" if record["actual_bytes"] == size and record["actual_sha256"] == expected else "FAILED"
            if record["status"] == "VERIFIED" and form in {"restricted_state", "torchscript"}:
                try:
                    with zipfile.ZipFile(path) as archive:
                        names = archive.namelist()
                        script = any("/code/" in name or name.endswith("/constants.pkl") for name in names)
                        record["archive_member_count"] = len(names)
                        record["is_torchscript"] = script
                        if form == "restricted_state" and (script or not any(name.endswith("/data.pkl") for name in names)):
                            record["status"] = "FAILED"
                            record["error"] = "not a code-free Torch state archive"
                except zipfile.BadZipFile:
                    record["status"] = "FAILED"
                    record["error"] = "invalid Torch ZIP"
        if record["status"] != "VERIFIED":
            report["status"] = "BLOCKED"
        report["assets"].append(record)
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    report = audit()
    if args.output:
        path = args.output.resolve()
        if not path.is_relative_to(ROOT / "docs/evidence/model-feasibility"):
            raise SystemExit("output must be inside model evidence")
        path.parent.mkdir(parents=True, exist_ok=True)
        with path.open("x", encoding="utf-8") as handle:
            json.dump(report, handle, indent=2)
            handle.write("\n")
    print(json.dumps(report, indent=2))
    return 0 if report["status"] != "BLOCKED" else 2


if __name__ == "__main__":
    raise SystemExit(main())
