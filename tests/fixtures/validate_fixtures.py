"""Offline fixture validation. No capture, playback, downloads, or model execution.

Exit codes: 0 = fixture readiness PASS, 1 = invalid input FAIL, 2 = BLOCKED.
PASS here does not attest model quality, hardware, or product acceptance.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import math
from pathlib import Path, PurePosixPath
import re
import struct
import sys

MAX_BYTES = 50 * 1024 * 1024
MAX_SECONDS = 60
CATEGORIES = {"normal", "fast", "short_pause", "long_pause", "soft", "laughter", "plosive", "low_level"}
ROLES = {"smoke_source", "quality_source", "reference"}


class InvalidFixture(ValueError):
    """Malformed or policy-incompatible media; do not include private paths."""


def inspect_wav(data: bytes) -> dict:
    if len(data) < 12 or data[:4] != b"RIFF" or data[8:12] != b"WAVE":
        raise InvalidFixture("invalid WAV header")
    if int.from_bytes(data[4:8], "little") + 8 != len(data):
        raise InvalidFixture("RIFF length mismatch")
    position = 12
    fmt = None
    pcm = None
    while position < len(data):
        if position + 8 > len(data):
            raise InvalidFixture("truncated WAV chunk")
        chunk = data[position:position + 4]
        size = int.from_bytes(data[position + 4:position + 8], "little")
        start = position + 8
        end = start + size
        if end > len(data):
            raise InvalidFixture("truncated WAV payload")
        if chunk == b"fmt ":
            if fmt is not None or size < 16:
                raise InvalidFixture("invalid or duplicate fmt chunk")
            fmt = struct.unpack_from("<HHIIHH", data, start)
            if fmt[0] == 0xFFFE:
                if size < 40 or int.from_bytes(data[start + 16:start + 18], "little") < 22:
                    raise InvalidFixture("invalid extensible WAV format")
                guid = data[start + 24:start + 40]
                if guid[4:] != bytes.fromhex("00001000800000aa00389b71"):
                    raise InvalidFixture("unsupported extensible WAV subtype")
                fmt = (int.from_bytes(guid[:4], "little"), *fmt[1:])
        elif chunk == b"data":
            if pcm is not None:
                raise InvalidFixture("duplicate data chunk")
            pcm = memoryview(data)[start:end]
        position = end + size % 2
        if position > len(data):
            raise InvalidFixture("missing chunk padding")
    if fmt is None or pcm is None:
        raise InvalidFixture("WAV lacks fmt or data")
    encoding, channels, rate, byte_rate, alignment, bits = fmt
    if channels not in (1, 2) or rate <= 0:
        raise InvalidFixture("unsupported channels or sample rate")
    if (encoding == 1 and bits not in (8, 16, 24, 32)) or (encoding == 3 and bits not in (32, 64)) or encoding not in (1, 3):
        raise InvalidFixture("unsupported WAV sample encoding")
    sample_bytes = bits // 8
    if alignment != channels * sample_bytes or byte_rate != rate * alignment or len(pcm) % alignment:
        raise InvalidFixture("inconsistent WAV format or partial frame")
    frames = len(pcm) // alignment
    duration = frames / rate
    if frames == 0 or duration > MAX_SECONDS:
        raise InvalidFixture("empty media or decoded duration exceeds 60s")
    peak = scaled_square_sum = 0.0
    clipped = sample_count = 0
    for offset in range(0, len(pcm), sample_bytes):
        if encoding == 3:
            sample = struct.unpack_from("<f" if bits == 32 else "<d", pcm, offset)[0]
        elif bits == 8:
            sample = (pcm[offset] - 128) / 128
        else:
            sample = int.from_bytes(pcm[offset:offset + sample_bytes], "little", signed=True) / (1 << (bits - 1))
        if not math.isfinite(sample):
            raise InvalidFixture("nonfinite decoded sample")
        absolute = abs(sample)
        if absolute > peak:
            scaled_square_sum = scaled_square_sum * (peak / absolute) ** 2 + 1
            peak = absolute
        elif peak:
            scaled_square_sum += (absolute / peak) ** 2
        clipped += abs(sample) >= 0.999
        sample_count += 1
    if peak == 0:
        raise InvalidFixture("all-zero media is not a speech fixture")
    return {"format": "WAV", "decode_status": "VERIFIED", "frames": frames,
            "sample_rate_hz": rate, "channels": channels, "duration_seconds": duration,
            "peak": peak, "rms": peak * math.sqrt(scaled_square_sum / sample_count),
            "clipped_sample_count": clipped, "sample_count": sample_count}


def inspect_flac(data: bytes) -> dict:
    if len(data) < 42 or data[:4] != b"fLaC" or data[4] & 0x7F != 0 or int.from_bytes(data[5:8], "big") != 34:
        raise InvalidFixture("FLAC lacks initial STREAMINFO")
    packed = int.from_bytes(data[18:26], "big")
    rate = packed >> 44
    channels = ((packed >> 41) & 7) + 1
    frames = packed & ((1 << 36) - 1)
    if rate == 0 or frames == 0 or channels > 2 or frames / rate > MAX_SECONDS:
        raise InvalidFixture("invalid FLAC metadata or declared media limits")
    # STREAMINFO is not proof that compressed audio can decode. An explicitly
    # selected, locked decoder must later decode and check every output sample.
    return {"format": "FLAC", "decode_status": "NOT_RUN", "declared_frames": frames,
            "sample_rate_hz": rate, "channels": channels,
            "declared_duration_seconds": frames / rate}


def fixture_path(root: Path, local_path: object) -> Path:
    if not isinstance(local_path, str) or "\\" in local_path or ":" in local_path:
        raise InvalidFixture("local_path must be a private relative POSIX path")
    relative = PurePosixPath(local_path)
    if relative.is_absolute() or ".." in relative.parts or relative.parts[:2] != (".local", "fixtures") or len(relative.parts) < 3:
        raise InvalidFixture("local_path must stay inside .local/fixtures")
    candidate = root.joinpath(*relative.parts).resolve()
    private_root = (root / ".local" / "fixtures").resolve()
    if not private_root.is_relative_to(root.resolve()) or not candidate.is_relative_to(private_root):
        raise InvalidFixture("private media path escapes allowed root")
    if candidate.suffix.lower() not in (".wav", ".flac"):
        raise InvalidFixture("only WAV and FLAC are accepted")
    return candidate


def validate_manifest(manifest: object, root: Path) -> dict:
    errors, blocked, inspected = [], [], []
    quality_counts = {"zh": 0, "en": 0}
    category_coverage = {"zh": set(), "en": set()}
    references = 0
    ids, hashes, targets = set(), set(), set()
    if not isinstance(manifest, dict) or type(manifest.get("schema_version")) is not int or manifest.get("schema_version") != 1 or not isinstance(manifest.get("fixtures"), list):
        return {"status": "FAIL", "errors": ["invalid manifest schema"], "blocked": [], "fixtures": []}
    for entry in manifest["fixtures"]:
        if not isinstance(entry, dict):
            errors.append("fixture entry must be an object")
            continue
        fixture_id = entry.get("id")
        if not isinstance(fixture_id, str) or not re.fullmatch(r"[a-zA-Z0-9_-]{1,80}", fixture_id) or fixture_id in ids:
            errors.append("fixture ID invalid or duplicated")
            continue
        ids.add(fixture_id)
        before = len(errors), len(blocked)
        role = entry.get("role")
        if not isinstance(role, str) or role not in ROLES:
            errors.append(f"{fixture_id}: invalid role")
            continue
        if entry.get("authorization") != "CONFIRMED_LOCAL_ONLY" or not isinstance(entry.get("consent_id"), str) or not entry["consent_id"].strip():
            blocked.append(f"{fixture_id}: local authorization receipt required")
            continue
        expected_hash = entry.get("sha256")
        if expected_hash is None:
            blocked.append(f"{fixture_id}: SHA-256 not locked")
        elif not isinstance(expected_hash, str) or not re.fullmatch(r"[0-9a-f]{64}", expected_hash):
            errors.append(f"{fixture_id}: invalid SHA-256")
        try:
            path = fixture_path(root, entry.get("local_path"))
            if not path.is_file():
                blocked.append(f"{fixture_id}: private media unavailable")
                continue
            if path.stat().st_size > MAX_BYTES:
                raise InvalidFixture("file exceeds 50MiB")
            data = path.read_bytes()
            if len(data) > MAX_BYTES:
                raise InvalidFixture("file exceeds 50MiB")
            digest = hashlib.sha256(data).hexdigest()
            if expected_hash is not None and expected_hash != digest:
                errors.append(f"{fixture_id}: SHA-256 mismatch")
            if digest in hashes and role in {"quality_source", "reference"}:
                errors.append(f"{fixture_id}: duplicate media cannot count toward quality coverage")
            hashes.add(digest)
            facts = inspect_wav(data) if path.suffix.lower() == ".wav" else inspect_flac(data)
            inspected.append({"id": fixture_id, "sha256": digest, **facts})
            if facts["decode_status"] != "VERIFIED":
                blocked.append(f"{fixture_id}: FLAC needs real decoded-sample validation")
            duration = facts.get("duration_seconds", facts.get("declared_duration_seconds"))
            if role == "reference":
                target_id = entry.get("target_id")
                if not isinstance(target_id, str) or not re.fullmatch(r"[a-zA-Z0-9_-]{1,80}", target_id):
                    errors.append(f"{fixture_id}: reviewed target identity required")
                elif target_id in targets:
                    errors.append(f"{fixture_id}: duplicate target cannot count toward three voices")
                else:
                    targets.add(target_id)
                if duration < 10:
                    errors.append(f"{fixture_id}: reference shorter than 10s")
                effective = entry.get("effective_reference_seconds")
                if effective is None:
                    blocked.append(f"{fixture_id}: effective authorized speech duration not reviewed")
                elif isinstance(effective, bool) or not isinstance(effective, (int, float)) or not math.isfinite(effective) or not 10 <= effective <= 30 or effective > duration:
                    errors.append(f"{fixture_id}: effective reference must be 10-30s within decoded duration")
                if before == (len(errors), len(blocked)):
                    references += 1
            elif role == "quality_source":
                language = entry.get("language")
                categories = entry.get("categories")
                if not isinstance(language, str) or language not in quality_counts or not isinstance(categories, list) or not categories or any(not isinstance(c, str) or c not in CATEGORIES for c in categories):
                    errors.append(f"{fixture_id}: reviewed language/categories required")
                elif before == (len(errors), len(blocked)):
                    quality_counts[language] += 1
                    category_coverage[language].update(categories)
        except InvalidFixture as exc:
            errors.append(f"{fixture_id}: {exc}")
        except OSError:
            blocked.append(f"{fixture_id}: media read unavailable")
    for language, count in quality_counts.items():
        if count != 12:
            blocked.append(f"quality corpus {language}: {count}/12 valid unique sources")
    missing = sorted(CATEGORIES - category_coverage["zh"] - category_coverage["en"])
    if missing:
        blocked.append(f"quality corpus: missing categories {','.join(missing)}")
    if references < 3:
        blocked.append(f"quality corpus: {references}/3 valid unique target references")
    return {"status": "FAIL" if errors else "BLOCKED" if blocked else "PASS", "errors": errors,
            "blocked": blocked, "fixtures": inspected,
            "measurement_scope": "offline fixture readiness only; no model, hardware, or human quality acceptance"}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--manifest", required=True, type=Path)
    args = parser.parse_args()
    try:
        manifest = json.loads(args.manifest.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, json.JSONDecodeError):
        print(json.dumps({"status": "BLOCKED", "errors": [], "blocked": ["manifest unavailable or unconfigured"]}))
        return 2
    result = validate_manifest(manifest, Path(__file__).resolve().parents[2])
    print(json.dumps(result, indent=2, allow_nan=False))
    return {"PASS": 0, "FAIL": 1, "BLOCKED": 2}[result["status"]]


if __name__ == "__main__":
    sys.exit(main())
