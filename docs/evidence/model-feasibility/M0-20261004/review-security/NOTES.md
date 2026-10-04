# T002 review repairs

Owner /root; independent reviewer /root/reviewer_t001. Original inference and
performance evidence remains bound to 429b7c27713c4549ba6072d25dda18f48ddd4ee9
and Torch 2.5.1+cu121. Original JSON and audio are preserved. New helper unit
tests are CPU-only, without importing Torch or opening audio devices.

- Source isolation now rejects staged and unstaged edits, untracked/ignored
  runtime files and linked paths; imports a fresh snapshot of fixed Git blobs.
  Existing clone bytecode is preserved but never copied/imported. Fixed configs
  under src/config accompany runtime in the snapshot.
  Actual snapshot check first failed on the prior working-copy configuration
  digest: checkout core.autocrlf=true produced CRLF. Fixed Git LF configuration
  SHA256 is1207e631961b8b0a983f248b77dee0d9b8441c521c8955ffbec89d251064ba43;
  prior checkout digest1f62b9441f55d8a0622ff87ac94987d8780d09af72f51b88a858b310d5e0835b.
  A separate read-only check proved JSON equality and byte equality after CRLF
  normalization; no model configuration values/weights were changed.
- Actual device reporting now enumerates parameters AND buffers. Empty is
  UNKNOWN, multiple devices MIXED. Historical ASR=cpu was hardcoded from upstream
  source, not measured; treat that original field as SOURCE_INFERRED, not an
  actual backend probe. Historical VC/speaker parameter probes remain as recorded.
- Paced gates use per-step processing/new-input-duration ratios, including short
  tails; old 60s/375 complete chunks are unaffected. The legal 30s tail regression
  demonstrates why a constant 160ms threshold was insufficient.
- Both prepare_config and load_runner reject Torch <2.10.0 or unrecognized
  versions before checkpoint loading, based on the two official advisories below.
  This addresses identified advisories, not a blanket safety certification.

Official [2025 advisory](https://github.com/pytorch/pytorch/security/advisories/GHSA-53q9-r3pm-6pq6)
affects <=2.5.1; [2026 advisory](https://github.com/pytorch/pytorch/security/advisories/GHSA-63cw-57p8-fm3p)
affects <=2.9.1, fixed >=2.10.0. ZIP structure plus weights_only was not a
sufficient safe-deserialization claim. Fixed asset hashes constrain replacement;
there is no evidence of compromise and no exploit/unknown checkpoint is tested.

The first remediation download targeted 2.6.0 after checking only the older
advisory; it was interrupted (exec session38673, exit1) before installation and
no uv process remained. Partial cache retained on D. An authorized replacement
download targets exactly torch/torchaudio2.10.0+cu126 from the official index,
as documented in [official versions](https://pytorch.org/get-started/previous-versions/).
No new patched-runtime inference/compatibility/performance PASS is claimed until
actually executed. Current safe runtime capability BLOCKED_SAFE_RUNTIME pending
that experiment. These are safety/report repairs; model compatibility budget
remains 2/2 and candidate replacements 0/1.

Full acquisition references (not publisher signatures):

- [Fixed MeanVC2 source](https://github.com/ASLP-lab/MeanVC2/tree/13acf84c1bf135ea5edad9c245b345289b06b33e)
- [Fixed official HF assets](https://huggingface.co/ASLP-lab/MeanVC2/tree/39cdd19522fe896c227da691314d9a0e3b995486)
- [Official WavLM README](https://github.com/microsoft/unilm/blob/master/wavlm/README.md),
  [linked original WavLM file](https://drive.google.com/file/d/12-cB34qCTvByWT-QtOcZaqwwO21FLSqU/view)
- [Official speaker README](https://github.com/microsoft/UniSpeech/blob/main/downstreams/speaker_verification/README.md),
  [linked speaker file](https://drive.google.com/file/d/1-aE1NfzpRCLxA4GUxX9ITI3F9LlbtEGP/view)

HF three digests are official LFS values. Microsoft two acquisition hashes are
local only. lawlict-derived code and separate speaker binary terms remain
BLOCKED_LICENSE_CHAIN for packaging; network permission does not grant those
licenses. Full production s3prl dependencies and pip check remain NOT_RUN.
