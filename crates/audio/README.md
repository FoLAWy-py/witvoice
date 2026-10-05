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

Packet GetBuffer/ReleaseBuffer/GetCurrentPadding invoke the cached native vtable
ABI directly and compare signed HRESULTs into a static operation/i32 error.
They do not call HRESULT::ok/map, construct windows::core::Error or query/release
thread ErrorInfo on failures. Normal Prepare/Start/Close retain the convenient
control-thread wrappers. Stack-owned literal-HRESULT mocks count zero Rust heap
allocations for capture acquisition/release failures, render acquisition/release
failures and the padding failure handler. This does not prove allocation or
blocking behavior inside the actual Windows audio service.

Render exposes only `submit_silence`: no PCM argument or capture-to-render link.
It primes silence before Start and releases every acquired frame with SILENT.
Stop/reset permanently retire the owner; a new explicit prepare is required.
Explicit close reports and retains the first Stop/reset error for repeat calls,
without claiming successful cleanup; Drop best-effort closes and releases the
same-thread references. There is no automatic restart/default fallback.

Every SharedStream now owns a NotificationWatch for that exact UID/flow; there
is no public unwatched constructor. Registration's InitialValidation marker may
be consumed once, only with no concurrent real change. The original sticky
invalidation is never cleared. A separate sticky atomic real-change flag is set
before callback batching, so consuming a batch cannot hide a prepare race or
restore authority. Prepare, Start, packet/silence operations before and after the
native call, and every scheduler wake/timeout check this flag and permanently
retire on any endpoint/default/property event. Capture output is fully erased
when a change occurs during acquisition or release. Callbacks only publish
bounded atomics; they do not Stop, query COM devices or release owner references.
Retirement forbids further delivery; successful cleanup is a separate result.
Stop/reset/unregistration remain normal same-STA close/Drop operations, retaining
the first failure. Unregister failure retains the callback under the previously
documented process-life quarantine; it does not mean resources were released.

The separate ordinary STA scheduler uses a bounded
[CoWaitForMultipleHandles](https://learn.microsoft.com/en-us/windows/win32/api/combaseapi/nf-combaseapi-cowaitformultiplehandles)
modal wait to dispatch notification calls/window messages; packet methods never
pump or wait. This slice does not wire a production session, output epoch,
deadline or ASRC into a stream (T008).

`cargo run -p witvoice-audio --locked --example stream_smoke -- capture UID 16000
1 pcm16 5 --approve-start` is an explicit hardware harness. The operator
must first obtain specific capture permission and an exclusive audio lease.
`silence-render` is the separate render mode. UID/flow/rate/channels/encoding and
1–5 seconds are mandatory, no default device is picked, no PCM file is saved or
uploaded, and stdout reports only counters and wall time. Do not run it as a
routine test. If the exact format is unsupported, it fails rather than substituting.
The request duration bounds scheduling; each wait is capped by the remaining
duration and expiry is checked again before another packet. OS wake/Stop/close
time can exceed it; active wall time and wall time including close are reported
separately. Leader's two historical 5s runs bind to 2355099; they do not validate
the subsequently added watched owner.

`removal_probe PRIVATE_SELECTION_JSON seconds(1..120) --approve-metadata` reads
the explicit existing Endpoint selection (absolute file, <=8192 bytes), registers
a watcher and prints/flushed `MONITORING_READY` before the operator's authorized
action. It never constructs a stream or calls Initialize/Start. Normal STA waits
are <=50ms, diagnostics are capped at 32 batches, and the finite timeout cannot
auto-restart. Output omits UID/private path and retains actual reason bits,
saved-UID revalidation errors, sticky state and close HRESULT. Timeout is
NOT_OBSERVED (exit2), not PASS. OBSERVED_SELECTED_UNAVAILABLE requires a real
change and failed saved-UID revalidation; physical unplug still requires operator
attestation. This metadata proof cannot establish a running stream stopped.

`format_support_probe PRIVATE_SELECTION_JSON RATE:CHANNELS:ENCODING [...]`
validates at most three explicit candidates before COM calls, records real
IsFormatSupported HRESULT/closest/error, and never Initialize/Start or adopts a
closest match. If all candidates are exact-supported, rejection remains NOT_RUN.
These new probes are only built by the author; actual registration, physical
unplug and format rejection are for leader after specific permission/lease.

Pure tests use local injected COM capture/render objects, not the Windows service.
Injected acquisition/release changes and a silent wait timeout exercise the same
pre/post guards used by the owner, including full-buffer zero and no retry even
after batches are drained. They do not constitute physical removal evidence.
No Initialize/Start, actual capture/render or physical removal test was executed
by this implementation task. SPSC/epoch/deadline governance, MMCSS, resampling/ASRC,
independent monitor clocks and long-running hardware acceptance remain
**NOT_RUN / not implemented**. Measurements (p95/p99, RTF, misses, memory) are UNKNOWN. The 8h
synthetic clock test and 2h Windows wall-clock audio gate remain distinct.
No Mac backend or cross-machine result is claimed. T007 cannot be closed from
metadata and unit tests alone; complete input/output/removal hardware evidence
and independent review are required. No GPU/audio hardware lease is held here.

## T008 implementation and evidence boundary

The preceding T007 description is historical. `realtime` now contains a
control-preallocated bounded SPSC with exclusive borrowed producer/consumer
endpoints. Its slots live on the control owner; callbacks cannot clone or drop
the final shared allocation. Cursor publication/acquisition and exact unsafe
slot-ownership invariants are documented in `spsc.rs`. Saturation rejects the
newest block and counts overflow; callbacks never drain-until-empty or wait to
fill capacity. Capacity, target occupancy, maximum age and policy are separate
configuration fields. Capture defaults to 10ms target / 20ms maximum age;
output defaults to 984 frames (20.5ms) target / 60ms maximum age / 80ms capacity.
Its explicit 24-frame margin covers interpolation lookahead at discrete packet
boundaries. The interpolator never prefetches beyond the final requested sample.

Opaque `CaptureBlock` and `ProcessedBlock` are separate paths. The trusted model
assembler must supply genuine converted 48k mono results in 480-frame blocks;
there is no CaptureBlock-to-playout adapter or identity production route. This
crate validates finite PCM, session/epoch, checked source intervals, monotonic
local timestamps, deadlines and ordering. It cannot authenticate the semantic
origin of a slice supplied by a trusted caller; T014 supplies the Node/worker
binding. Non-48k model or device rates require their explicit boundary adapter,
and are not capabilities granted by this same-rate drift corrector.

Every output/monitor has its own queue, interpolation state and occupancy clock.
Monitor permission starts muted and no monitor disconnect affects the virtual
gate. Linear interpolation preserves the source sample index with correction
limited to ±1000ppm. A backlog beyond twice the target for 100 consecutive callbacks
faults the owner; excessive negative drift faults on sustained underflow. Clipped
proportional feedback at packet boundaries is not a measured hardware drift.
This is limited drift correction, not general resampler quality
evidence. Underflow erases the entire callback (including partial PCM), is counted
separately from planned silence, and 30ms of consecutive missing output faults
the gate. No VAD timeline deletion, noise suppression or routine normal-block
eviction is used. Assembly counts and actual successful native sink commits are
separate diagnostics; saturating diagnostic counters never wrap.

`OutputGate` starts muted; explicit `arm` requires exclusive control ownership
before sharing. Mute/Stop first atomically and irreversibly invalidate this epoch,
before slow cleanup. Native `SharedStream::submit_processed` accepts a processed
playout owner, not arbitrary source PCM. Ordinary `bind_output_gate` must bind an
Arc-owned gate once before Start; submission rejects any unbound/different gate.
The notification signal holds the same gate in a OnceLock. Actual changes
atomically fault it without a blocking lookup, clone or final Arc drop in the
callback. Close invalidates it before Stop/Reset/unregister. One gate cannot
bind to multiple native owners, so monitor teardown cannot share the virtual
native gate. Construct/share/arm ordering is a trusted control operation; no
implicit Start/arm or renewed authorization is added. It acquires a bounded in-flight ticket
before assembly, rechecks permission/notifications/deadlines before commit,
and keeps the ticket through the raw-HRESULT `ReleaseBuffer`. Muted or failed
assembly submits SILENT. Callback operations do not query COM error information,
wait, allocate, format logs or destroy a COM owner. Construct/close/Drop remain
ordinary STA control operations. Control-thread `ack_ready` must be true before
reporting successful local Mute ACK; otherwise its bounded polling deadline is a
failure, not an ACK. There is no callback spin for this handshake. New prepare
requires new queues/gate and a fresh epoch; the old gate cannot re-arm.

Invalidate during an already-entered native commit may precede that commit's
exit; ACK waits for its ticket. OS audio already queued before ACK remains an
independent, UNKNOWN hardware tail. The software handshake does not revoke it.
Native submission uses the caller's same local monotonic clock for assembly and
precommit expiry; caller-provided wall clocks or arbitrary timestamps are invalid.

Pure tests cover queue concurrency/order/reuse, allocator counts on success and
failure, interval/counter extremes, expiry/gaps, separate starvation/planned
silence, independent disabled monitor, and injected native commit/mute failures.
The 8h tests use the production occupancy controller and source phase planner
at 10ms callback granularity with real SPSC metadata slots and discrete 480-frame
arrivals; finite startup inventory is 30ms. Separate ramp tests exercise real interpolation.
Each ±150/±500ppm report records 2,880,000 synthetic callbacks, 28,800 simulated
seconds, source/output counts, occupancy/correction bounds and actual wall time.
They do not run billions of PCM sample interpolations or prove real hardware
latency, real2h stability, two-machine LAN, Mac, model quality or virtual routing.
No hardware/GPU/LAN lease is held and no native Start is run by these tests.
Current validation status is pending the actual commands and independent review.

The T008 r1 repair uses one sticky `AtomicU32` publication handshake, independent
of notification batching. Real reasons occupy low bits; the gate-published bit is
set only after the control thread successfully completes `OnceLock::set`.
Both sides use AcqRel `fetch_or` on this same atomic. Its RMW modification order
ensures the later operation acquires the earlier operation's bits: if the change
is first, bind observes its reason and invalidates; if bind is first, the callback
observes publication and the preceding initialized gate, then invalidates.
Intervening RMWs retain both markers. The two operations cannot both miss; a
callback does not wait for a concurrent binder. After both complete, a changed
selection cannot retain a live bound gate. A bind that completes before the
notification's RMW may succeed, but that notification invalidates before it ends.
Closed-only reasons perform planned invalidation; any accumulated ordinary real
reason faults the gate. InitialValidation and taking a batch never clear or set
real-change permission. Failed/double binds cannot replace the retained gate.
Callbacks use nonwaiting `get`, borrow the retained Arc and never clone or drop
ownership. Test-only pause hooks exercise both RMW orders in the actual publish
branch; no waiting hook is reachable from a production native callback. Existing
in-flight tickets still prevent ACK until release; each monitor remains separate.

## T009 explicit virtual routing

`route::RouteSelection` keeps exact saved render/capture UIDs and local parent
evidence. Unknown or physical virtual endpoints, wrong direction, inactive or
unsupported format, different software parents and identical IDs are rejected.
Windows persistent identities use ASCII case-insensitive equality, never names.
Production `candidate` additionally requires a separately proven physical capture
source, excluding both virtual IDs. `virtual_candidate` is a diagnostic-only pair;
it cannot pass production source revalidation, and a production selection cannot
skip its source check using `revalidate_pair`. Neither method arms an OutputGate,
starts a device or declares model readiness/route continuity. Origin observations
come from trusted local control evidence; arbitrary UI assertions are not proof.
Revalidation/COM queries are control operations and may allocate. Packet-side
`check_changes` only tests sticky atomics and invalidates the output gate; batching
cannot revive the retired selection. Retain a registered route watch from before
current snapshots through close, with checks before/after native operations.

The `route_probe` example accepts an absolute private scope JSON (8192 bytes max)
and exactly `--metadata-only` or `--approve-route-probe`. It never opens a physical
input/output. Its current profile is only the locally audited VB-CABLE driver:
`route_profile=VB_CABLE`, `driver_provider=VB-Audio Software`,
`driver_service=VBAudioVACMME`, `driver_version=3.3.1.7`, `driver_inf=oem95.inf`.
Other required fields are `render_uid`, `capture_uid`, `software_parent`,
`software_evidence`, `render_channels`, `capture_channels`, `render_encoding`,
`capture_encoding` (explicit `pcm16` or `float32`; rate fixed 48k). The leader must
bind those exact IDs to fresh PnP parent/driver evidence, retain the sidecar hash
and authorize only CABLE Input render -> CABLE Output capture. This file is a
trusted operator artifact, not signed authentication of caller-supplied claims.
CABLE In 16ch, default devices, Steam candidates and same-name substitutions are
not this execution scope. Metadata-only does not register, Initialize or Start.

The opt-in mode requires the new specific once-only permission and exclusive
audio lease, checked by the trusted leader/operator, not granted by a CLI flag.
It synthesizes a fixed two-second, amplitude 0.01 marker in the example only,
through the existing typed processed queue and ticket-governed native sink. This
diagnostic is not converted voice or IdentityEngine production capability.
Negotiated native buffers and capture scratch are bounded at 1440 mono frames
(30ms at 48k); each processed render commit and its scratch remain at most 960
frames (20ms), the queue at eight 480-frame
blocks, target 1440 frames during feeding, with production expiry/underflow rules.
Each scheduled capture is checked against the remaining two-second window; no
loop retries failed Start, format, queue, device change or capture. Any failure
invalidates before normal STA Stop/Reset/unregister. Caller marker/capture/scratch
buffers use volatile zero overwrite, and all allocated queue slots are overwritten
before release. Compiler-created transient stack/register copies and OS queued
audio cannot be promised erased. No PCM or private IDs appear in the public output.

Overall deadline is 15s, with checks before/after phases; synchronous OS COM calls
cannot be interrupted by those checks. Execute only under the leader's external
approximately 13s process watchdog to enforce that limit including a hung call.
Watchdog termination means cleanup/erasure UNKNOWN, never success. Normal close
times and requested/actual active duration are separate scalars. Post-close finite
correlation examines 8192 samples over +/-200ms, threshold >=0.6, with sufficient
capture length, finite low-level samples, no output underflow and all close/ACK
successes required. This proves only a generated marker reaching the selected
virtual capture, not acoustic latency, a third-party app, model quality, the full
VC pipeline, two-hour stability or Mac. No probe was run by the author; T009
requires actual once-only routing evidence and independent review before DONE.

Any DATA_DISCONTINUITY (including the first packet) or uncertain timestamp rejects
marker evidence, with separate flag counters and a fixed error/HRESULT report.
Microsoft documents [discontinuity as a possible state transition or glitch](https://learn.microsoft.com/en-us/windows/win32/api/audioclient/ne-audioclient-_audclnt_bufferflags);
it does not guarantee that a first-packet flag is harmless. This probe retains
that strict failure and does not auto-retry. Close errors are reported separately.

T009's consumed once-only run on frozen 2ed6346 failed at render Prepare with the
old generic `Capacity`; it did not reach any Start or capture Prepare. That result
does not distinguish a zero OS buffer from a buffer above 960 frames, and does not
prove a driver fault. The diagnostic repair retains `GetBufferSize` as
`NegotiatedCapacity { actual_frames, maximum_frames }`, with pure injected 0/960/961
tests. That diagnostic repair kept Prepare's 200ms parameter bound, the probe's
maximum 960 frames and the packet submission limit unchanged. An early Prepare failure
prints its exact stage/requested/known actual capacity and native error, with
`native_start=false`, `cleanup=UNKNOWN` and no retry; RAII teardown is not a real
close measurement. No new probe, Initialize, Start, render or capture was executed
for this repair. The original executed binary and evidence remain historical.

The later single diagnostic run measured render capacity 1056 frames (22ms),
rejected before Start; capture capacity remains unknown. Its permission is spent.
The bounded software adaptation now permits at most 1440 negotiated frames for
this probe, rejects zero/over-bound capacity, and reports both actual capacities
when the owners are prepared. Capacity is separate from queue target and age.
Each `submit_processed` checks capacity minus padding and commits only the minimum
of available frames, 960, and nonempty caller scratch. It performs one native
GetBuffer/ReleaseBuffer pair, retains its gate ticket through release, and does
not loop to fill a larger OS buffer. Padding above capacity or empty scratch is
an error. Capture has a preallocated 1440-frame mono buffer; existing channel
extraction, flags, discontinuity/timestamp rejection and full failure zeroing
are preserved. The eight-slot queue, 480-frame blocks, deadlines, epoch checks,
two-second activity, 15-second process budget, amplitude and strict underflow
criterion have not changed. No hardware operation was authorized or performed
for this adaptation; it cannot establish continuity, model quality, OS tail or
successful native cleanup. A future probe requires new specific permission.
