//! Metadata only. Requires a separate authorized physical action/hardware lease.
//! Never creates SharedStream or calls Initialize/Start/GetBuffer/ReleaseBuffer.
mod support;
#[cfg(windows)]
fn main() -> Result<(), String> {
    use std::{
        io::Write,
        sync::Arc,
        time::{Duration, Instant},
    };
    use witvoice_audio::notifications::{ChangeKind, ChangeSignal, NotificationWatch, WatchError};
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() != 3 || args[2] != "--approve-metadata" {
        return Err("PRIVATE_SELECTION_JSON seconds(1..120) --approve-metadata".into());
    }
    let seconds: u64 = args[1]
        .to_str()
        .ok_or("invalid duration")?
        .parse()
        .map_err(|_| "invalid duration")?;
    if !(1..=120).contains(&seconds) {
        return Err("finite timeout 1..120 seconds required".into());
    }
    let (uid, flow) = support::selection(&args[0])?;
    let signal = Arc::new(ChangeSignal::new());
    let mut watch = NotificationWatch::register(uid.clone(), flow, Arc::clone(&signal))
        .map_err(|e| format!("registration failed: {e:?}"))?;
    let initial = watch
        .poll_revalidation()
        .ok_or("initial validation missing")?;
    let endpoint = initial
        .endpoint
        .map_err(|e| format!("initial validation failed: {e:?}"))?;
    if endpoint.uid != uid
        || endpoint.flow != flow
        || initial.changes.bits() != ChangeKind::InitialValidation as u32
        || signal.has_changed()
        || initial.more_changes_pending
    {
        return Err("initial baseline raced with change; no monitoring authority".into());
    }
    println!("MONITORING_READY");
    std::io::stdout()
        .flush()
        .map_err(|_| "ready flush failed")?;
    let started = Instant::now();
    let limit = Duration::from_secs(seconds);
    let mut reasons = 0;
    let mut observations = Vec::new();
    let mut unavailable = false;
    let mut dispatch_error = None;
    while let Some(remaining) = limit.checked_sub(started.elapsed()) {
        if remaining.is_zero() {
            break;
        }
        if let Err(error) = watch.dispatch(remaining.min(Duration::from_millis(50))) {
            dispatch_error = Some(format!("{error:?}"));
            break;
        }
        if let Some(check) = watch.poll_revalidation() {
            reasons |= check.changes.bits();
            let selected = match check.endpoint {
                Ok(endpoint) => serde_json::json!({"available":true,
                    "exact_uid_flow":endpoint.uid == uid && endpoint.flow == flow,
                    "state_bits":endpoint.state_bits}),
                Err(error) => {
                    unavailable = true;
                    serde_json::json!({"available":false,"error":error})
                }
            };
            observations.push(serde_json::json!({"reason_bits":check.changes.bits(),
                "selected":selected,"more_pending":check.more_changes_pending}));
            // Bound diagnostics independently of timeout and event frequency.
            if unavailable || observations.len() == 32 {
                break;
            }
        }
    }
    let observed = unavailable && signal.has_changed() && reasons != 0;
    let sticky_before_close = signal.is_invalidated();
    let changed_before_close = signal.has_changed();
    let close_hresult = match watch.close() {
        Ok(()) => 0,
        Err(WatchError::UnregisterFailed(code)) => code,
        Err(error) => return Err(format!("close failed: {error:?}")),
    };
    drop(watch);
    println!(
        "{}",
        serde_json::json!({
            "result":if observed {"OBSERVED_SELECTED_UNAVAILABLE"} else {"NOT_OBSERVED"},
            "reason_bits":reasons,"observations":observations,
            "is_invalidated_before_close":sticky_before_close,
            "has_changed_before_close":changed_before_close,"close_hresult":close_hresult,
            "dispatch_error":dispatch_error,"wall_including_close_seconds":started.elapsed().as_secs_f64(),
            "initialize_start_capture_render":"NOT_RUN","running_stream_removal":"NOT_RUN",
            "physical_action_attestation":"OPERATOR_REQUIRED"
        })
    );
    if !observed || close_hresult != 0 || dispatch_error.is_some() {
        std::process::exit(2);
    }
    Ok(())
}
#[cfg(not(windows))]
fn main() {
    eprintln!("Windows only; NOT_RUN");
    std::process::exit(2);
}
