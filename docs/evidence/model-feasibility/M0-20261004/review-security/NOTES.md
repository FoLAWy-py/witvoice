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
  At that point no new patched-runtime capability was claimed. The subsequently
executed file and controlled isolated experiment are recorded below; current
selection awaits independent r2. These are safety/report repairs; compatibility budget
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
licenses. Full production s3prl dependencies remain NOT_VERIFIED; the actual
patched pip check result is recorded below.

## Actual patched combination

Torch/torchaudio2.10.0+cu126 installation exit0 (10m30s preparation, uv0.9.28);
SymPy1.13.3 satisfies actual Torch metadata >=1.13.3.33 installed distributions
archived in dependencies-210.json. Actual pip check exit1 reports3 absent declared
s3prl dependencies: omegaconf,transformers,protobuf. Complete production install
is NOT_VERIFIED; only the executed inference import closure is evidenced.

Frozen harness4fd6be043b9c5a627df36328ff00e3ef03d61ace:

- file-210.json exit0:7.510s finite nonblank16k output; actual parameter+buffer
  device sets VC/speaker cuda:0,ASR cpu.
- paced-210.json exit2:375steps60s,averageRTF0.580077,p99259.537ms and
  maxlag986.025ms FAIL. Output was written before performance gate failed, so
  output field is null; offline-210.json separately checks its existing WAV
  without changing FAILED. This sample is retained, never replaced by a pass.
- One predetermined environment-isolation retest, same model/code/input/gates,
  all owned compile/tests paused; no power/driver changes. Baseline CPU8%,
  balanced power; GPU query initially56% graphics-inclusive utilization/P8,
  after5%/P5. This is not proof of globally idle GPU or causal blame for failure.
  Earlier backend test timing was not captured, final17:46:21 test occurred
  after failed run ended17:45:26; cannot attribute failure to it.
- paced-210-isolated.json exit0:375steps60s,averageRTF0.59434065,
  p99149.3883ms (ratio0.933677<=1),maxlag13.7232ms; output59.94s finite,
  no clipping. Only this single controlled C experiment passed, not robustness
  under concurrent application/game load or2h Windows acceptance.

Independent standard-library decode/hash/all375-step-statistics check exit0
offline-210.json. First checker attempt exit1 was a return-contract KeyError
(status versus decode_status), fixed without changing decoder/results. Guards,
raw experiments and real command exits remain distinct. GPU lease released;
no microphone,playback,network media or Mac operation occurred.
