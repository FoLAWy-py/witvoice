# Windows audio: T007 first slice

`wasapi` exposes endpoint metadata and explicit UID format probes on normal
STA COM threads. Calls enumerate render/capture and all state bits; active endpoints
report their native shared-mode mix format. Inactive/probe failures stay explicit.
No call initializes or starts a stream, reads microphone samples or submits audio.
COM initialization is balanced on the calling thread and returned task memory is
freed on error paths too. Metadata/probe functions allocate and must never be
called from real-time callbacks. Calling from an existing MTA fails with its COM
HRESULT; the library does not change the caller's apartment or spawn a thread.
STA first use follows Microsoft's [IAudioClient guidance](https://learn.microsoft.com/en-us/windows/win32/api/audioclient/nn-audioclient-iaudioclient).

`inspect_endpoint` and `probe_format` require an active, exact persistent endpoint
UID and the expected flow. Missing, inactive and wrong-flow endpoints fail; there
is no default-device fallback. Names are not used to select or authenticate a
virtual line. Node's Windows virtual route is **render CABLE Input → capture
CABLE Output selected by the third-party application**. This API does not certify
a device as virtual or prevent choosing a feedback loop; that explicit route
validation remains required before any future stream starts.

`probe_format` distinguishes exact S_OK from closest S_FALSE; a closest format is
metadata only and does not automatically become an approved stream format.
Outgoing PCM24/32 probes own a complete WAVEFORMATEXTENSIBLE descriptor with
container-sized valid bits, mono-center/stereo-left-right mask and PCM subformat
GUID (cbSize 22). PCM8/16 and float32 keep their documented basic descriptor.
See Microsoft's [WAVEFORMATEX rules](https://learn.microsoft.com/en-us/windows/win32/api/mmreg/ns-mmreg-waveformatex)
and [extensible descriptors](https://learn.microsoft.com/en-us/windows-hardware/drivers/audio/extensible-wave-format-descriptors).
The adapter validates mono/stereo, 8–192 kHz, PCM8/16/24/32 or float32, block
alignment, byte rate and defined channel masks. Reduced valid bits (e.g. 24 in
32), compressed/multichannel/float64 formats fail adapter validation. Raw native
format fields remain visible when the adapter rejects them.

`format` helpers transform one frame to one frame in caller-owned buffers, with
no heap allocation, blocking or logging. Capture stereo downmix averages channels;
render applies finite gain 0–4 and safety limiting before duplicating mono to
stereo. All conversion errors erase the whole destination (PCM8 silence is 128).
These helpers do **not** resample: `needs_bus_resampling` explicitly marks formats
which require a verified single boundary resampler before the 48k mono bus.
This hard limiter is a safety clamp, not a quality-qualified mastering algorithm.

Build checks (no hardware access):

```powershell
. tools/dev/toolchain-env.ps1
cargo test -p witvoice-audio --locked
cargo clippy -p witvoice-audio --locked --all-targets -- -D warnings
cargo fmt --package witvoice-audio --check
```

Authorized metadata-only diagnostic, run separately by leader and retain IDs
only under the Git-ignored `.local` directory:

```powershell
cargo run -p witvoice-audio --locked --example list_endpoints > .local/audio-endpoints.json
```

No native device test was executed by this implementation task. Initialize/Start,
real capture/render, endpoint removal, notifications, SPSC/epoch/deadline output
governance, MMCSS, boundary resampling/ASRC, independent monitor clocks and
long-running hardware acceptance remain **NOT_RUN / not implemented** in this
slice. Their measurements (p95/p99, RTF, misses, memory) are UNKNOWN. The 8h
synthetic clock test and 2h Windows wall-clock audio gate remain distinct.
No Mac backend or cross-machine result is claimed. T007 cannot be closed from
metadata and unit tests alone; complete input/output/removal hardware evidence
and independent review are required. No GPU/audio hardware lease is held here.
