//! Explicit bounded route diagnostic. Never run as an unattended test.
#[cfg(windows)]
#[path = "support/route_marker.rs"]
mod route_marker;
#[cfg(windows)]
fn main() -> Result<(), String> {
    use std::{
        io::Read,
        path::PathBuf,
        sync::Arc,
        time::{Duration, Instant},
    };
    use witvoice_audio::{
        format::{AudioFormat, Encoding},
        notifications::{ChangeSignal, NotificationWatch},
        realtime::{Binding, OutputGate, ProcessedBlock, QueueConfig, Ring, processed_endpoints},
        route::{Direction, Observation, Origin, RouteSelection},
        stream::{ExplicitStart, SharedStream},
        wasapi::{self, Flow},
    };
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Scope {
        route_profile: String,
        driver_provider: String,
        driver_service: String,
        driver_version: String,
        driver_inf: String,
        render_uid: String,
        capture_uid: String,
        software_parent: String,
        software_evidence: String,
        render_channels: u16,
        capture_channels: u16,
        render_encoding: String,
        capture_encoding: String,
    }
    let total_started = Instant::now();
    let total_limit = Duration::from_secs(15);
    let within_total = || {
        if total_started.elapsed() < total_limit {
            Ok(())
        } else {
            Err("overall deadline expired")
        }
    };
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() != 2 || (args[1] != "--metadata-only" && args[1] != "--approve-route-probe") {
        return Err("absolute PRIVATE_SCOPE_JSON --metadata-only|--approve-route-probe; new permission and exclusive lease required".into());
    }
    let path = PathBuf::from(&args[0]);
    if !path.is_absolute() {
        return Err("explicit absolute private scope required".into());
    }
    let mut bytes = Vec::with_capacity(8193);
    std::fs::File::open(path)
        .map_err(|_| "scope unavailable")?
        .take(8193)
        .read_to_end(&mut bytes)
        .map_err(|_| "scope read failed")?;
    if bytes.len() > 8192 {
        return Err("scope exceeds 8192 bytes".into());
    }
    let scope: Scope = serde_json::from_slice(&bytes).map_err(|_| "invalid scope")?;
    if scope.route_profile != "VB_CABLE"
        || scope.driver_provider != "VB-Audio Software"
        || scope.driver_service != "VBAudioVACMME"
        || scope.driver_version != "3.3.1.7"
        || scope.driver_inf != "oem95.inf"
    {
        return Err("scope is not the currently audited VB-CABLE pair".into());
    }
    within_total()?;
    let encoding = |value: &str| match value {
        "pcm16" => Ok(Encoding::Pcm16),
        "float32" => Ok(Encoding::Float32),
        _ => Err("unsupported explicit encoding"),
    };
    let render_format = AudioFormat::new(
        48_000,
        scope.render_channels,
        encoding(&scope.render_encoding)?,
    )
    .map_err(|_| "render format invalid")?;
    let capture_format = AudioFormat::new(
        48_000,
        scope.capture_channels,
        encoding(&scope.capture_encoding)?,
    )
    .map_err(|_| "capture format invalid")?;
    // Private sidecar is trusted local operator metadata bound to exact UIDs;
    // it is not a signature, routing proof or authority for arbitrary UI input.
    fn snapshot<'a>(
        uid: &'a str,
        flow: Flow,
        format: Option<AudioFormat>,
        origin: Origin<'a>,
    ) -> Result<Observation<'a>, String> {
        let endpoint =
            wasapi::inspect_endpoint(uid, flow).map_err(|_| "saved endpoint unavailable")?;
        let exact = match format {
            Some(format) => {
                wasapi::probe_format(uid, flow, format)
                    .map_err(|_| "format query failed")?
                    .exact_supported
            }
            None => endpoint
                .mix_format
                .as_ref()
                .is_some_and(|m| m.adapter_format.is_some()),
        };
        Ok(Observation {
            uid,
            direction: if flow == Flow::Render {
                Direction::Render
            } else {
                Direction::Capture
            },
            active: endpoint.state_bits == 1,
            exact_format: exact,
            origin,
        })
    }
    let software = Origin::Software {
        parent: &scope.software_parent,
        evidence: &scope.software_evidence,
    };
    let r = snapshot(
        &scope.render_uid,
        Flow::Render,
        Some(render_format),
        software,
    )?;
    let c = snapshot(
        &scope.capture_uid,
        Flow::Capture,
        Some(capture_format),
        software,
    )?;
    let mut route =
        RouteSelection::virtual_candidate(r, c).map_err(|e| format!("route policy: {e:?}"))?;
    within_total()?;
    if args[1] == "--metadata-only" {
        println!(
            "{{\"status\":\"CANDIDATE_ONLY\",\"software_evidence\":\"operator_sidecar\",\"continuity\":\"NOT_RUN\",\"start\":\"NOT_RUN\"}}"
        );
        return Ok(());
    }
    // All operations below require a NEW explicit user permission and audio lease.
    let signal = Arc::new(ChangeSignal::new());
    let mut watch =
        NotificationWatch::register(scope.render_uid.clone(), Flow::Render, Arc::clone(&signal))
            .map_err(|e| format!("route watch registration: {e:?}"))?;
    if signal.has_changed() {
        return Err("route changed before prepare".into());
    }
    // Register before the current exact snapshots; no event can be cleared later.
    route
        .revalidate_pair(
            Some(snapshot(
                &scope.render_uid,
                Flow::Render,
                Some(render_format),
                software,
            )?),
            Some(snapshot(
                &scope.capture_uid,
                Flow::Capture,
                Some(capture_format),
                software,
            )?),
            &OutputGate::new(Binding::new(9, 1).map_err(|_| "binding")?).map_err(|_| "gate")?,
        )
        .map_err(|_| "route invalidated")?;
    let mut gate =
        OutputGate::new(Binding::new(9, 1).map_err(|_| "binding")?).map_err(|_| "gate")?;
    gate.arm().map_err(|_| "gate cannot arm")?;
    let gate = Arc::new(gate);
    within_total()?;
    route
        .check_changes(&signal, &gate)
        .map_err(|_| "route changed")?;
    let mut render =
        match SharedStream::prepare(&scope.render_uid, Flow::Render, render_format, 1440) {
            Ok(owner) => owner,
            Err(error) => {
                gate.invalidate();
                println!(
                    "{}",
                    route_marker::prepare_failure_report("render_prepare", &error, 1440)
                );
                return Err(format!("render prepare; no fallback: {error:?}"));
            }
        };
    within_total()?;
    let mut capture =
        match SharedStream::prepare(&scope.capture_uid, Flow::Capture, capture_format, 1440) {
            Ok(owner) => owner,
            Err(error) => {
                gate.invalidate();
                println!(
                    "{}",
                    route_marker::prepare_failure_report("capture_prepare", &error, 1440)
                );
                return Err(format!("capture prepare; no fallback: {error:?}"));
            }
        };
    within_total()?;
    render
        .bind_output_gate(Arc::clone(&gate))
        .map_err(|e| format!("gate binding: {e:?}"))?;
    let mut ring = Ring::<ProcessedBlock>::new(8).map_err(|_| "ring")?;
    let (passed, mut report) = {
        let (mut producer, mut playout) =
            processed_endpoints(&mut ring, &gate, QueueConfig::output()).map_err(|_| "queue")?;
        let mut marker = vec![0.0; route_marker::FRAMES];
        route_marker::marker(&mut marker);
        let mut captured = vec![0.0; route_marker::FRAMES];
        let mut scratch = [0.0; 960];
        let mut packet = [0.0; 1440];
        let mut sent = 0usize;
        let mut received = 0usize;
        let mut capture_packets = 0u64;
        let mut discontinuities = 0u64;
        let mut timestamp_errors = 0u64;
        let start = Instant::now();
        let requested = Duration::from_secs(2);
        let clock = || start.elapsed().as_nanos().min((u64::MAX - 1) as u128) as u64;
        // Marker is explicitly diagnostic, not model conversion. It only enters the
        // example through the governed typed sink; no production raw-render bypass.
        let run = (|| {
            within_total()?;
            route
                .check_changes(&signal, &gate)
                .map_err(|_| "route changed")?;
            capture
                .start(ExplicitStart::UserApproved)
                .map_err(|e| format!("capture Start: {e:?}"))?;
            render
                .start(ExplicitStart::UserApproved)
                .map_err(|e| format!("render Start: {e:?}"))?;
            while let Some(remaining) =
                route_marker::remaining(start.elapsed(), total_started.elapsed())
            {
                if remaining.is_zero() {
                    break;
                }
                within_total()?;
                watch
                    .dispatch(Duration::ZERO)
                    .map_err(|e| format!("route dispatch: {e:?}"))?;
                route
                    .check_changes(&signal, &gate)
                    .map_err(|_| "route changed")?;
                let now = clock();
                while playout.queued_frames() < 1440 && sent + 480 <= marker.len() {
                    let block = ProcessedBlock::from_model_result(
                        gate.binding(),
                        sent as u64,
                        now,
                        now.checked_add(60_000_000).ok_or("deadline overflow")?,
                        &marker[sent..sent + 480],
                    )
                    .map_err(|_| "marker block")?;
                    producer
                        .push(block, now)
                        .map_err(|_| "bounded queue rejected marker")?;
                    sent += 480;
                }
                if start.elapsed() >= requested {
                    break;
                }
                render
                    .submit_processed(&mut playout, &mut scratch, clock)
                    .map_err(|e| format!("governed render: {e:?}"))?;
                route
                    .check_changes(&signal, &gate)
                    .map_err(|_| "route changed")?;
                let Some(wait_remaining) =
                    route_marker::remaining(start.elapsed(), total_started.elapsed())
                else {
                    break;
                };
                if capture
                    .wait_event(wait_remaining.min(Duration::from_millis(2)))
                    .map_err(|e| format!("capture wait: {e:?}"))?
                    && start.elapsed() < requested
                {
                    if let Some(meta) = capture
                        .capture_packet(&mut packet)
                        .map_err(|e| format!("capture packet: {e:?}"))?
                    {
                        capture_packets += 1;
                        discontinuities += u64::from(meta.discontinuity());
                        timestamp_errors += u64::from(!meta.timestamp_valid());
                        if meta.discontinuity() || !meta.timestamp_valid() {
                            return Err("capture discontinuity or invalid timestamp".into());
                        }
                        let count = meta.frames as usize;
                        if received + count > captured.len() {
                            return Err("bounded capture exhausted".into());
                        }
                        captured[received..received + count].copy_from_slice(&packet[..count]);
                        received += count;
                    }
                    packet.fill(0.0);
                }
                route
                    .check_changes(&signal, &gate)
                    .map_err(|_| "route changed")?;
            }
            Ok::<(), String>(())
        })();
        // Invalidate before slow cleanup, even on failed Start/notification/timeout.
        gate.invalidate();
        route_marker::erase(&mut scratch);
        route_marker::erase(&mut packet);
        let active_wall = start.elapsed().as_secs_f64();
        let capture_close_error = capture.close().err().map(|e| format!("{e:?}"));
        let render_close_error = render.close().err().map(|e| format!("{e:?}"));
        let watch_close_error = watch.close().err().map(|e| format!("{e:?}"));
        let capture_closed = capture_close_error.is_none();
        let render_closed = render_close_error.is_none();
        let watch_closed = watch_close_error.is_none();
        let counters = playout.counters();
        let matched = if run.is_ok()
            && capture_closed
            && render_closed
            && watch_closed
            && counters.underflow_frames == 0
        {
            route_marker::correlate(&marker[..sent], &captured[..received])
        } else {
            None
        };
        route_marker::erase(&mut marker);
        route_marker::erase(&mut captured);
        let passed = matched.as_ref().is_some_and(|m| m.correlation >= 0.6)
            && gate.ack_ready()
            && within_total().is_ok();
        let report = serde_json::json!({"status":if passed {"ROUTE_MARKER_OBSERVED"} else {"FAILED_MUTED"},
        "route_profile":"VB_CABLE","requested_seconds":2,"maximum_marker_seconds":5,"maximum_process_seconds":15,
        "requested_maximum_frames":1440,"render_capacity_frames":render.capacity_frames(),"capture_capacity_frames":capture.capacity_frames(),
        "process_wall_seconds":total_started.elapsed().as_secs_f64(),"active_wall_seconds":active_wall,"wall_including_close_seconds":start.elapsed().as_secs_f64(),
        "marker_amplitude":route_marker::AMPLITUDE,"marker_frames":sent,"capture_frames":received,"capture_packets":capture_packets,
        "sink_pcm_frames":counters.sink_pcm_frames,"sink_silence_frames":counters.sink_silence_frames,"underflow_frames":counters.underflow_frames,
        "match":matched,"run_ok":run.is_ok(),"run_error":run.err(),"discontinuity_packets":discontinuities,"timestamp_error_packets":timestamp_errors,
        "capture_closed":capture_closed,"render_closed":render_closed,"watch_closed":watch_closed,
        "capture_close_error":capture_close_error,"render_close_error":render_close_error,"watch_close_error":watch_close_error,
        "ack_ready":gate.ack_ready(),"physical_capture":"NEVER_STARTED","pcm_saved":false,"model_quality":"NOT_TESTED","os_tail":"UNKNOWN"});
        (passed, report)
    }; // borrowed queue endpoints and their interpolation state leave scope
    // Normal-thread overwrite of every allocated queue slot before deallocation.
    // No claim to erase OS buffers or compiler-created transient stack copies.
    let zero = ProcessedBlock::from_model_result(gate.binding(), 0, 0, 1, &[0.0; 480])
        .map_err(|_| "zero block")?;
    {
        let (mut overwrite, mut drain) = ring.split();
        for _ in 0..8 {
            let _ = drain.try_pop();
        }
        for _ in 0..8 {
            overwrite.try_push(zero).map_err(|_| "queue erase failed")?;
            let _ = drain.try_pop();
        }
    }
    drop(ring);
    let passed = passed && within_total().is_ok();
    report["status"] = serde_json::json!(if passed {
        "ROUTE_MARKER_OBSERVED"
    } else {
        "FAILED_MUTED"
    });
    report["process_wall_seconds"] = serde_json::json!(total_started.elapsed().as_secs_f64());
    println!("{report}");
    if passed {
        Ok(())
    } else {
        Err("no verified route; no restart or fallback".into())
    }
}
#[cfg(not(windows))]
fn main() {
    eprintln!("Windows route probe only; Mac NOT_RUN");
    std::process::exit(1);
}
