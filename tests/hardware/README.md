# T004 measurement and resource plan

Authority: `docs/spec/02-audio-engine.md`, `03-model-runtime.md`, and `07-tests-acceptance.md`. `measurement-plan.json` is an unexecuted report skeleton, not a hardware result. All unavailable measurements are null (UNKNOWN) and tests are NOT_RUN; never replace unknown with zero. Copy into a leader-assigned evidence run directory when authorized to run. Include commit, machine, OS/toolchain/dependency versions, complete command, input/model SHA-256, UTC start/end, operator/date for manual checks, exit code, measurement boundary, and artifacts. Keep private recordings and absolute filenames outside Git.

## Resource gates and minimum resources

T004 holds only the fixture/hardware preparation file-path lease. It holds no GPU or audio lease. Prior to real microphone capture, render/capture endpoint activation, monitoring, playback, or long tests, obtain existing user authorization covering the operation and an exclusive lease naming the agent, resources, start, and bounded expiry. Coordinate the single GPU/audio test slot; no training, model benchmarking, or competing audio operation may run during a long test. If hardware or authorization is absent, record BLOCKED and the minimum missing item. Release on completion/failure, and stop capture/output before waiting for worker teardown.

Windows minimum: a usable input, explicitly selected virtual render/capture pair, licensed installed virtual cable, real model and authorized smoke source/reference, approved hardware permissions. Reference target is Windows 11 x64 / 16 GiB RAM class / NVIDIA 8 GiB VRAM class; record actual hardware instead of assuming it. A CPU backend must pass the same thresholds independently. Never install a driver or change system default microphone/firewall as part of a test without explicit authorization. A doctor report establishes environment only, not WASAPI or virtual cable success.

Mac/LAN minimum (later M6): actual Apple Silicon Mac, 16 GiB unified-memory reference class, installed authorized virtual endpoint, compatible tested backend, both machines on the same router over wired interfaces, recorded MTU and interface, and approved connectivity. Wi-Fi gets a separate experience report when available. Single-machine loopback, mocked Mac, or Windows cross-compilation cannot substitute for two-machine evidence. MPS presence is not model execution proof.

## Endpoint and safety procedure (NOT_RUN)

1. After the lease/authorization gate, identify persistent device UIDs and supported formats. Save names for display only; never replace a missing UID with a same-name physical microphone. Verify the input is not the virtual endpoint being rendered into. Monitor starts disabled; enabling needs separate covered authorization and headphone advice.
2. Negotiate actual sample rate/channels. Internal bus is 48 kHz mono float32; network bus 48 kHz mono PCM16 little-endian. Resample only at boundaries. For two-channel virtual render, apply gain/limiting before copying mono to both channels.
3. Windows route: Node writes processed audio to CABLE Input **render/playback**; target application reads CABLE Output **capture/recording**. Confirm selected persistent UIDs and actual third-party application input. Mac route: render to BlackHole output, third-party application captures BlackHole input.
4. Prepare permissions, devices, real-model warmup, checked voice profile, applicable trust/lease, and output buffers before source user's Start. No source capture before Start. Log counters/timing only outside the hard realtime callback. Callback audit forbids allocations, waiting locks, files, logs, network, IPC, Python/model, and GPU synchronization; use preallocated SPSC and atomics.
5. Maintain separate virtual and monitor playout buffers/clock tracking. Disconnect monitor and verify virtual output continues. Trace each queue's capacity, target occupancy, maximum age and overflow policy. Initial capture target ≤20 ms; virtual output target 10–30 ms. Capacity is not target delay.
6. Inject Mute/Stop, worker crash/OOM, device loss, reconnect and old epoch/expired frames using permitted operations. Sink observation must prove no unconverted source marker reaches the output; UI text cannot prove fail-closed. Planned mute, fault silence, underflow, misses and rejected old media need separate raw counts. Short PLC ≤30 ms, never infinite repetition. Start each recovered timeline under a fresh epoch and empty queues.
7. Stop/close capture and invalidate output governance first, then stop network/worker and release resources. Account for OS buffered tail separately; device disappearance always fails muted.

## Fixed acceptance boundaries and thresholds

| Measurement | Required boundary/threshold |
|---|---|
| Windows local real model | source frame ready → matching converted output submitted to virtual render; p95 ≤200 ms, p99 ≤250 ms |
| Each real-model LAN direction | same application boundary traced on source timeline; p95 ≤250 ms, p99 ≤300 ms |
| Wired IdentityEngine | additional network/application buffering p95 ≤50 ms; test-only engine, no voice quality claim |
| Worker steady state | average RTF ≤0.70; p99 per-step processing ≤that step's new-input duration; no sustained backlog |
| Non-realtime IPC | round trip p99 ≤2 ms, excluding model |
| Stop/Mute | Node confirmation → submission of zero output ≤50 ms; separate measured OS tail |
| Windows real audio soak | ≥2 actual wall-clock hours, uninterrupted audio; no crash/leak/unexplained sustained queue growth |
| Each remote direction soak | ≥60 actual wall-clock minutes per direction |
| Clock simulation | ±150 and ±500 ppm, each 8 simulated hours, bounded buffers and monotonic sample timeline; correction within ±1000 ppm |
| Steady missing frames | union of deadline-miss/underflow frames divided by eligible steady frames <0.1%; retain both individual counts, exclude only explicit injected faults/user mute |
| Private memory after warmup | 30–120 minute window, fixed active resources; growth ≤max(128 MiB,10% of window-start private memory), no persistent upward trend |

Use source sample counters and session/epoch/frame IDs to associate each result. Cross-device timestamps require explicit clock-offset estimation/uncertainty, or same-source-clock return tracing; never subtract unsynchronized host clocks. `app_pipeline_latency` excludes full mouth-to-ear acoustic delay. Any loopback/acoustic test records separate OS/device/application-buffer boundary; waveform correlation after voice conversion is not inherently reliable timing proof.

Retain raw latency samples and sample count. Use a declared quantile method, for example nearest rank `ceil(p*n)` over ascending samples, and report p50/p95/p99. Ten samples do not substantiate p99; gather a meaningful steady-run population, report count and limitations, and preserve failures rather than deleting outliers. Worker RTF denominator is genuinely new input duration, excluding repeated context; report numerator, denominator, initialization, warmup, and per-step durations. Report all misses even when output filled with zeros; the union counter avoids double counting a frame with both miss and underflow.

Clock simulation may accelerate 28,800 simulated seconds per offset, but must record both simulated and wall duration. It cannot pass a 7,200-second Windows real-audio soak. Observe independent monitor/virtual clocks, occupancy min/max, safe correction bounds and monotonic counters, and sustained offsets beyond ±1000 ppm failing safely. No simulation has run yet.

Resource planning budgets (not measured guarantees): UI+Node steady private memory ≤500 MiB; Windows model memory ≤4 GiB and dedicated GPU memory ≤3 GiB. Explain over-budget sources/impact rather than hiding metrics. Sum/process-account private memory carefully across WebView, Node and worker; avoid shared-page double counting. CPU core equivalents = process CPU seconds / wall seconds; whole-machine percentage = 100 × core equivalents / logical cores. Record CPU per process plus logical core count. Record actual backend, power mode and competing load, p95/p99, peak/stable model memory and GPU memory; Mac unified memory has no fabricated separate VRAM. Resource contention tests require the same lease and must verify safe silence and no accumulation.

## Remaining matrix and evidence tiers

Acceptance still requires worker killed/OOM, UI freeze/killed, Node killed, output unplugged, input-default change, Mute/Stop, network disconnect/reconnect, sleep/wake, certificate revocation, late old epoch, disk full, damaged model, incompatible profile, compute busy, interrupted download. These are planned, NOT_RUN; do not damage user devices/files to simulate them. Coordinate isolated fault injection and restore conditions.

A = document/static; B = unit/contract/mock; C = real process/protocol/file source; D = physical device/real model/real virtual endpoint; E = two physical machines with bidirectional LAN loop. Fixtures and this plan cannot establish D or E. Windows D, human quality signoff, packaging, and all M0–M5 tasks are required for WINDOWS_DELIVERED; real bidirectional M6 evidence remains mandatory for COMPLETE.
