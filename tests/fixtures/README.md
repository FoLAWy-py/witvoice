# T004 authorized fixture readiness

Authority: `docs/spec/03-model-runtime.md` sections 5 and 7, and `07-tests-acceptance.md` section 5. Raw/private media stays under ignored `.local/fixtures/`; this manifest contains only local relative identifiers, consent receipts, and hashes. Do not commit source recordings. Manifest additions require an existing authorization; this validator never grants consent, downloads, plays, or records audio.

Run the bundled Python executable (no third-party packages):

```powershell
& 'C:\Users\22198\.cache\codex-runtimes\codex-primary-runtime\dependencies\python\python.exe' -B tests/fixtures/validate_fixtures.py --manifest tests/fixtures/manifest.json
& 'C:\Users\22198\.cache\codex-runtimes\codex-primary-runtime\dependencies\python\python.exe' -B -m unittest discover -s tests/fixtures -p 'test_*.py' -v
```

Exit 0 means fixture readiness only; exit 1 means invalid input; exit 2 means BLOCKED/unconfigured. Missing manifest/media/authorization/hash/quality corpus does not become PASS. JSON contains no private absolute filenames. Do not equate this offline check with real model or quality acceptance.

Each fixture needs a unique `id`, `role` (`smoke_source`, `quality_source`, `reference`), `.local/fixtures/` relative `local_path`, SHA-256, `CONFIRMED_LOCAL_ONLY` authorization, and an auditable `consent_id`. Quality sources need `language` (`zh` or `en`) and reviewed `categories` drawn from normal, fast, short_pause, long_pause, soft, laughter, plosive, low_level. Exactly 12 valid unique sources per language are required. Coverage is checked across the whole set. A smoke source never counts as a quality source. References need distinct reviewed `target_id` values and `effective_reference_seconds` in 10–30 seconds, bounded by actual decoded duration. At least three authorized target voices are required; three files from one speaker do not qualify.

The authorized private source is approximately 7.53 seconds and is a smoke source, never a valid 10-second reference. The downloaded LibriSpeech WAV is available and hash-locked; its effective speech duration is UNKNOWN until reviewed, so its `effective_reference_seconds` remains null. Filling this field requires actual effective-content review, not copying file duration. Source/license/attribution and the fixed dataset revision are preserved in manifest provenance. The validator does not establish whether a particular real engine accepts the reference.

Both WAV and FLAC are limited to 50 MiB, 60 decoded seconds, and two channels. WAV PCM8/16/24/32 and float32/64 are actually decoded offline, with malformed chunks, truncation, NaN/Inf, and all-zero content rejected. Peak/RMS/clipped-sample counts are reported without claiming a sustained-clipping threshold that SPEC does not define. FLAC STREAMINFO can be inspected using stdlib, but **real FLAC decode is NOT_RUN and BLOCKED** until an approved decoder is selected and all decoded samples are checked. Metadata alone cannot establish decodability. WAV conversion must happen separately under authorization.

The complete 24/3 quality corpus is NOT_READY. Human listening is QUALITY_REVIEW_PENDING; the product owner must sign off after scoring intelligibility, naturalness, target voice, and rhythm preservation (each median ≥3/5). Offline media readiness PASS does not sign off quality. No model output, seam, length-retention, or sustained-clipping quality regression has yet run.
