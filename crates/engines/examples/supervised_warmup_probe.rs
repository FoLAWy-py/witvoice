//! Finite debug diagnostic of the production owner with the fixed real model.
//! No media frames, audio devices, PCM output, network or automatic restart.
#[cfg(all(windows, debug_assertions))]
fn main() -> std::process::ExitCode {
    use std::time::{Duration, Instant};
    use witvoice_engines::{
        lifecycle::{Binding, Phase},
        supervisor::{CleanupStage, WorkerSupervisor},
    };
    if std::env::args().skip(1).collect::<Vec<_>>() != ["--approve-fixed-model-warmup"] {
        eprintln!("explicit fixed-model warmup approval required");
        return std::process::ExitCode::from(2);
    }
    let started = Instant::now();
    let deadline = started + Duration::from_secs(123);
    let mut owner = match WorkerSupervisor::new(0x5749_5456_4f49_4345) {
        Ok(owner) => owner,
        Err(error) => {
            eprintln!("owner configuration refused: {error:?}");
            return std::process::ExitCode::FAILURE;
        }
    };
    let binding = Binding::new(0x5749_5456_4f49_4345, 1).expect("constant valid binding");
    let mut ready = false;
    let mut authenticated = false;
    let mut ready_capabilities = None;
    let mut ready_at = None;
    let mut ready_hold_polls = 0u32;
    let run = (|| {
        owner.begin(binding)?;
        authenticated = owner.channels_authenticated();
        while Instant::now() < deadline {
            owner.poll()?;
            if owner.phase() == Phase::Ready && owner.output_allowed() {
                ready = true;
                ready_capabilities = owner.capabilities().cloned();
                let since = ready_at.get_or_insert_with(Instant::now);
                ready_hold_polls += 1;
                if since.elapsed() >= Duration::from_millis(500) {
                    return Ok::<bool, witvoice_engines::supervisor::Failure>(true);
                }
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        // Stop is still attempted below; an expired preparation never passes.
        Ok(false)
    })();
    // Always request Stop; this invalidates capability permission before IPC.
    let stop = owner.stop();
    let cleanup = owner.cleanup_snapshot();
    let released = owner.resources_released();
    let output_closed = !owner.output_allowed();
    let cleanup_confirmed = cleanup.is_some_and(|s| {
        s.stage == CleanupStage::Released
            && s.terminate_succeeded == Some(true)
            && s.active_processes == Some(0)
            && s.process_id_count == Some(0)
            && s.controller_signaled == Some(true)
            && s.policy_confirmed == Some(true)
            && s.cleanup_failure.is_none()
    });
    let passed = run == Ok(true)
        && stop.is_ok()
        && ready
        && authenticated
        && ready_hold_polls >= 2
        && ready_at.is_some_and(|t| t.elapsed() >= Duration::from_millis(500))
        && started.elapsed() <= Duration::from_secs(130)
        && owner.first_failure().is_none()
        && cleanup_confirmed
        && released
        && owner.stopped_ack()
        && output_closed;
    println!(
        "{}",
        serde_json::json!({
            "status": if passed { "PASS_PRODUCTION_OWNER_WARMUP_ONLY" } else { "FAILED_MUTED" },
            "ready": ready, "channels_authenticated": authenticated,
            "ready_hold_polls": ready_hold_polls, "capabilities": ready_capabilities,
            "run_error": run.err().map(|e| format!("{e:?}")),
            "stop_error": stop.err().map(|e| format!("{e:?}")),
            "first_failure": owner.first_failure().map(|e| format!("{e:?}")),
            "cleanup": cleanup.map(|s| format!("{s:?}")),
            "cleanup_confirmed": cleanup_confirmed, "resources_released": released,
            "stopped_ack": owner.stopped_ack(), "output_closed": output_closed,
            "wall_seconds": started.elapsed().as_secs_f64(),
            "configured_model": "fixed MeanVC2 CUDA", "configured_host_gate_bytes": 8589934592u64,
            "audio_media_network": "NOT_STARTED", "automatic_restart": false,
            "real_voice_conversion_route": "NOT_TESTED"
        })
    );
    if passed {
        std::process::ExitCode::SUCCESS
    } else {
        std::process::ExitCode::FAILURE
    }
}

#[cfg(not(all(windows, debug_assertions)))]
fn main() -> std::process::ExitCode {
    eprintln!("diagnostic requires a native Windows debug build");
    std::process::ExitCode::from(2)
}
