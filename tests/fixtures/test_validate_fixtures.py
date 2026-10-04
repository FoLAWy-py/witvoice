"""Real boundary tests using synthetic files, never audio hardware."""

import hashlib
from pathlib import Path
import struct
import tempfile
import unittest

from validate_fixtures import InvalidFixture, fixture_path, inspect_flac, inspect_wav, validate_manifest


def wav(seconds=1, encoding=1, samples=None):
    rate, channels, bits = 100, 1, 32 if encoding == 3 else 16
    pcm = samples if samples is not None else struct.pack("<h", 1024) * int(seconds * rate)
    fmt = struct.pack("<HHIIHH", encoding, channels, rate, rate * bits // 8, bits // 8, bits)
    body = b"WAVEfmt " + struct.pack("<I", len(fmt)) + fmt + b"data" + struct.pack("<I", len(pcm)) + pcm
    return b"RIFF" + struct.pack("<I", len(body)) + body


class FixtureTests(unittest.TestCase):
    def test_decodes_actual_frames_and_rms(self):
        facts = inspect_wav(wav(10))
        self.assertEqual(facts["frames"], 1000)
        self.assertEqual(facts["duration_seconds"], 10)
        self.assertEqual(facts["rms"], 1024 / 32768)
        self.assertEqual(facts["clipped_sample_count"], 0)

    def test_rejects_nonfinite_float(self):
        with self.assertRaisesRegex(InvalidFixture, "nonfinite"):
            inspect_wav(wav(encoding=3, samples=struct.pack("<f", float("nan"))))

    def test_rejects_all_zero_fake_speech(self):
        with self.assertRaisesRegex(InvalidFixture, "all-zero"):
            inspect_wav(wav(samples=b"\x00\x00" * 100))

    def test_rejects_truncated_media(self):
        with self.assertRaisesRegex(InvalidFixture, "length mismatch"):
            inspect_wav(wav()[:-1])

    def test_rejects_over_60_seconds(self):
        with self.assertRaisesRegex(InvalidFixture, "exceeds 60s"):
            inspect_wav(wav(60.01))

    def test_rejects_path_escape_and_unaccepted_extensions(self):
        for path in ("../secret.wav", ".local/fixtures/../../../secret.wav", "C:/secret.wav", ".local/fixtures/code.py", ".local\\fixtures\\sample.wav"):
            with self.subTest(path=path), self.assertRaises(InvalidFixture):
                fixture_path(Path.cwd(), path)

    def test_flac_metadata_never_attests_decode(self):
        packed = (16000 << 44) | (15 << 36) | 160000
        streaminfo = bytes(10) + packed.to_bytes(8, "big") + bytes(16)
        facts = inspect_flac(b"fLaC\x80\x00\x00\x22" + streaminfo)
        self.assertEqual(facts["declared_duration_seconds"], 10)
        self.assertEqual(facts["decode_status"], "NOT_RUN")

    def test_short_source_is_not_valid_reference(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            private = root / ".local" / "fixtures"
            private.mkdir(parents=True)
            data = wav(7.57)
            (private / "short.wav").write_bytes(data)
            result = validate_manifest({"schema_version": 1, "fixtures": [{
                "id": "short-reference", "role": "reference", "target_id": "one-speaker",
                "local_path": ".local/fixtures/short.wav", "sha256": hashlib.sha256(data).hexdigest(),
                "authorization": "CONFIRMED_LOCAL_ONLY", "consent_id": "test-only-synthetic",
                "effective_reference_seconds": 7.57
            }]}, root)
            self.assertEqual(result["status"], "FAIL")
            self.assertTrue(any("shorter than 10s" in error for error in result["errors"]))
            self.assertTrue(any("0/3" in reason for reason in result["blocked"]))

    def test_empty_corpus_is_blocked_not_pass(self):
        result = validate_manifest({"schema_version": 1, "fixtures": []}, Path.cwd())
        self.assertEqual(result["status"], "BLOCKED")
        self.assertIn("quality corpus zh: 0/12 valid unique sources", result["blocked"])
        self.assertIn("quality corpus en: 0/12 valid unique sources", result["blocked"])

    def test_hash_mismatch_fails(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            private = root / ".local" / "fixtures"
            private.mkdir(parents=True)
            (private / "source.wav").write_bytes(wav())
            result = validate_manifest({"schema_version": 1, "fixtures": [{
                "id": "source", "role": "smoke_source", "local_path": ".local/fixtures/source.wav",
                "sha256": "0" * 64, "authorization": "CONFIRMED_LOCAL_ONLY", "consent_id": "synthetic"
            }]}, root)
            self.assertEqual(result["status"], "FAIL")
            self.assertIn("source: SHA-256 mismatch", result["errors"])

    def test_malformed_roles_fail_without_exception(self):
        result = validate_manifest({"schema_version": 1, "fixtures": [{"id": "bad", "role": []}]}, Path.cwd())
        self.assertEqual(result["status"], "FAIL")

    def test_full_synthetic_corpus_checks_distinct_targets(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            private = root / ".local" / "fixtures"
            private.mkdir(parents=True)
            entries = []
            for index in range(27):
                data = wav(10, samples=struct.pack("<h", 1000 + index) * 1000)
                name = f"synthetic-{index}"
                (private / f"{name}.wav").write_bytes(data)
                entry = {"id": name, "role": "quality_source" if index < 24 else "reference",
                         "local_path": f".local/fixtures/{name}.wav", "sha256": hashlib.sha256(data).hexdigest(),
                         "authorization": "CONFIRMED_LOCAL_ONLY", "consent_id": "synthetic-test-only"}
                if index < 24:
                    entry.update(language="zh" if index < 12 else "en", categories=["normal", "fast", "short_pause", "long_pause", "soft", "laughter", "plosive", "low_level"])
                else:
                    entry.update(target_id=f"synthetic-speaker-{index}", effective_reference_seconds=10)
                entries.append(entry)
            manifest = {"schema_version": 1, "fixtures": entries}
            self.assertEqual(validate_manifest(manifest, root)["status"], "PASS")
            entries[-1]["target_id"] = entries[-2]["target_id"]
            duplicate = validate_manifest(manifest, root)
            self.assertEqual(duplicate["status"], "FAIL")
            self.assertTrue(any("duplicate target" in error for error in duplicate["errors"]))


if __name__ == "__main__":
    unittest.main()
