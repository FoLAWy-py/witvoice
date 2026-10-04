# Windows audio: T007 native slices

`wasapi` exposes endpoint metadata and explicit UID format probes on normal
STA COM threads. Calls enumerate render/capture and all state bits; active endpoints
report their native shared-mode mix format. Inactive/probe failures stay explicit.
These metadata functions do not initialize/start streams or read/submit audio.
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

`notifications::NotificationWatch` adds an explicit, thread-bound native
IMMNotificationClient registration owned by a normal STA control thread. All
five callbacks ignore endpoint pointers and publish only atomic reason bits plus
sticky invalidation: no allocation, lock/wait, COM device query, log, register,
unregister or final-reference release. Any endpoint event conservatively invalidates
the selection; reason bits coalesce repeated events and are not event/frame counts.
The single consumer atomically takes a batch before checking the saved exact UID
and expected flow on the control thread. Events arriving during that check stay
pending in the next batch. No default-device or same-name replacement is selected.
The check returns metadata, never Ready, an epoch acknowledgement or permission
to resume audio; T008 must implement actual output governance and a fresh Prepare.

The owner keeps the callback, enumerator and COM apartment alive until normal
teardown. Windows registration does not AddRef the client. Explicit `close` reports
unregister errors to the control thread, retires polling and cannot clear sticky
invalidation or permit that signal owner to register again. Successful unregister
releases the COM objects before the apartment guard. If unregister still fails in
Drop, registration state is unknown: the one fixed-size callback/enumerator pair
and COM initialization are deliberately retained for process life to avoid a
dangling callback. This is **not** successful cleanup; the error HRESULT stays in
the shared signal. At most one registration can be claimed per signal owner,
there is no retry loop or automatic replacement/recovery, and this exceptional
retention must be reported. The library does not promise every teardown succeeds.
These lifetime rules follow Microsoft's [notification callback guidance](https://learn.microsoft.com/en-us/windows/win32/api/mmdeviceapi/nn-mmdeviceapi-immnotificationclient)
and [registration ownership requirements](https://learn.microsoft.com/en-us/windows/win32/api/mmdeviceapi/nf-mmdeviceapi-immdeviceenumerator-registerendpointnotificationcallback).

Notification tests invoke a locally constructed COM callback through all five
native methods and test atomic batching/retained references using injected
unregister results. They do **not** call Register/Unregister against the Windows
service, inspect devices, initialize/start audio or physically remove endpoints.
The callback allocation counter excludes object construction/teardown and counts
only actual native callback method invocation on the test thread.

Authorized metadata-only diagnostic, run separately by leader and retain IDs
only under the Git-ignored `.local` directory:

```powershell
cargo run -p witvoice-audio --locked --example list_endpoints > .local/audio-endpoints.json
```

`stream::SharedStream::prepare` is now an explicit native device operation on
one ordinary STA owner thread. It selects the exact active UID/flow, accepts only
exact shared-mode support, owns the full PCM24/32 descriptor and closest-match
task memory, then initializes an event-driven stream. It never calls Start.
The actual OS capacity must fit the configured bound (maximum 200ms); capacity
is not a latency or occupancy target. The owner is !Send/!Sync and releases COM
services before the client, event handle and apartment. `start(UserApproved)`
requires the trusted Node's explicit authorization declaration; this enum does
not itself acquire microphone consent, a hardware lease or session readiness.

Each capture call handles at most one complete packet in caller-preallocated
storage. Silent packets permit null data. Unknown flags, non-silent null,
capacity/length errors and NaN/Inf erase the entire destination and retire the
owner. GetBuffer/ReleaseBuffer are paired on the same thread, including rejected
packets (release0), while nonempty successful packets release their full size.
Release failure erases output and is not blindly retried. Device position,
discontinuity and QPC in 100ns remain native metadata, not a manufactured source
timeline. Packet calls do not wait, allocate, log or invoke another thread.
Event waiting is a separate bounded owner-thread scheduling method.

Render exposes only `submit_silence`: no PCM argument or capture-to-render link.
It primes silence before Start and releases every acquired frame with SILENT.
Stop/reset permanently retire the owner; a new explicit prepare is required.
Explicit close reports and retains the first Stop/reset error for repeat calls,
without claiming successful cleanup; Drop best-effort closes and releases the
same-thread references. There is no automatic restart/default fallback. Notification
invalidation must be applied by the future Node/T008 output governor; this slice
does not wire a production session, output epoch, deadline or ASRC into a stream.

`cargo run -p witvoice-audio --locked --example stream_smoke -- capture UID 16000
1 pcm16 5 --approve-start` is an **unexecuted** hardware harness. The operator
must first obtain specific capture permission and an exclusive audio lease.
`silence-render` is the separate render mode. UID/flow/rate/channels/encoding and
1–5 seconds are mandatory, no default device is picked, no PCM file is saved or
uploaded, and stdout reports only counters and wall time. Do not run it as a
routine test. If the exact format is unsupported, it fails rather than substituting.

Pure tests use local injected COM capture/render objects, not the Windows service.
No Initialize/Start, actual capture/render or physical removal test was executed
by this implementation task. SPSC/epoch/deadline governance, MMCSS, resampling/ASRC,
independent monitor clocks and long-running hardware acceptance remain
**NOT_RUN / not implemented**. Measurements (p95/p99, RTF, misses, memory) are UNKNOWN. The 8h
synthetic clock test and 2h Windows wall-clock audio gate remain distinct.
No Mac backend or cross-machine result is claimed. T007 cannot be closed from
metadata and unit tests alone; complete input/output/removal hardware evidence
and independent review are required. No GPU/audio hardware lease is held here.
