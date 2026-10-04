//! Explicit private UID selection; registers notifications but never starts audio.
//! Full output may contain endpoint metadata and must stay under .local.
#[cfg(windows)]
fn main() -> Result<(), String> {
    use serde::Deserialize;
    use std::{io::Read, path::PathBuf, sync::Arc, time::Instant};
    use witvoice_audio::{
        notifications::{ChangeSignal, NotificationWatch},
        wasapi::Flow,
    };

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Selection {
        uid: String,
        flow: String,
    }
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() != 1 {
        return Err("usage: notification_probe ABSOLUTE_PRIVATE_SELECTION_JSON".into());
    }
    let path = PathBuf::from(&args[0]);
    if !path.is_absolute() {
        return Err("selection must be an absolute file <=8192 bytes".into());
    }
    let mut bytes = Vec::with_capacity(8193);
    std::fs::File::open(path)
        .map_err(|e| e.to_string())?
        .take(8193)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > 8192 {
        return Err("selection file exceeds 8192 bytes".into());
    }
    let selections: Vec<Selection> = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    if selections.is_empty() || selections.len() > 2 {
        return Err("select one or two exact endpoints; no default fallback".into());
    }
    // Reject the whole manifest before performing any COM operation.
    let flows: Vec<Flow> = selections
        .iter()
        .map(|s| {
            if s.uid.is_empty() || s.uid.len() > 1024 || s.uid.contains('\0') {
                return Err("invalid explicit UID".to_string());
            }
            match s.flow.as_str() {
                "Capture" => Ok(Flow::Capture),
                "Render" => Ok(Flow::Render),
                _ => Err("flow must be Capture or Render".to_string()),
            }
        })
        .collect::<Result<_, _>>()?;
    let started = Instant::now();
    let mut completed = Vec::new();
    for (selection, flow) in selections.iter().zip(flows) {
        for iteration in 0..3 {
            let signal = Arc::new(ChangeSignal::new());
            let mut watch = NotificationWatch::register(selection.uid.clone(), flow, signal)
                .map_err(|e| format!("registration failed: {e:?}"))?;
            let revalidation = watch
                .poll_revalidation()
                .ok_or("initial revalidation was missing")?;
            let endpoint = revalidation.endpoint.map_err(|e| format!("{e:?}"))?;
            if endpoint.uid != selection.uid || endpoint.flow != flow {
                return Err("exact UID/flow revalidation mismatch".into());
            }
            watch.close().map_err(|e| format!("close failed: {e:?}"))?;
            if watch.poll_revalidation().is_some() {
                return Err("closed registration still permitted revalidation".into());
            }
            completed.push(serde_json::json!({
                "flow": selection.flow, "iteration": iteration,
                "register": "SUCCESS", "exact_uid_flow": true, "close": "SUCCESS"
            }));
        }
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "result": "PASS_REGISTRATION_LIFECYCLE_ONLY",
            "completed": completed, "wall_seconds": started.elapsed().as_secs_f64(),
            "service_notification_delivery": "NOT_RUN",
            "physical_removal": "NOT_RUN", "initialize_start_capture_render": "NOT_RUN"
        }))
        .map_err(|e| e.to_string())?
    );
    Ok(())
}

#[cfg(not(windows))]
fn main() {
    eprintln!("Windows notification service probe unavailable on this platform.");
    std::process::exit(2);
}
