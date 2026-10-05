//! Explicit debug diagnostic: native Rust/Python transport + owned Job only.
//! No model/production Ready/audio/LAN. The synthetic source frame never reaches a sink.
#[cfg(all(windows, debug_assertions))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use std::{
        ffi::OsString,
        io::Read,
        path::PathBuf,
        time::{Duration, Instant, SystemTime, UNIX_EPOCH},
    };
    use witvoice_contracts::{
        values::DecimalU64,
        worker::{WorkerBinding, WorkerMediaHeader, WorkerMediaKind},
    };
    use witvoice_platform::{ProcessJob, Secret, WorkerPipeServer};
    if std::env::args_os().skip(1).collect::<Vec<_>>()
        != [OsString::from("--approve-native-interop")]
    {
        return Err("explicit native transport diagnostic flag required".into());
    }
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .ok_or("repository root missing")?
        .to_owned();
    let python = PathBuf::from(
        r"C:\Users\22198\.cache\codex-runtimes\codex-primary-runtime\dependencies\python\python.exe",
    );
    let script = root.join("workers/vc_worker/native_interop_probe.py");
    if !python.is_file() || !script.is_file() {
        return Err("fixed local probe runtime unavailable".into());
    }
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let control_tag = format!("wc-{}-{nonce:x}", std::process::id());
    let media_tag = format!("wm-{}-{nonce:x}", std::process::id());
    let secret = Secret::generate()?;
    let mut control = WorkerPipeServer::bind(&control_tag, Secret::from_bytes(*secret.as_bytes()))?;
    let mut media = WorkerPipeServer::bind(&media_tag, Secret::from_bytes(*secret.as_bytes()))?;
    let job = ProcessJob::new(false)?;
    let started = Instant::now();
    let args = [
        script.into_os_string(),
        "--control".into(),
        control_tag.into(),
        "--media".into(),
        media_tag.into(),
        "--node-pid".into(),
        std::process::id().to_string().into(),
    ];
    let mut child = job.spawn_bootstrapped(&python, &args, true, &secret)?;
    let mut received_samples = 0;
    let run = (|| -> Result<(), Box<dyn std::error::Error>> {
        let startup = Instant::now() + Duration::from_millis(2800);
        control.accept(child.id(), startup)?;
        media.accept(child.id(), startup)?;
        let deadline = Instant::now() + Duration::from_millis(2200);
        if control.read_frame(65536, deadline)? != b"transport-test-only" {
            return Err("control payload mismatch".into());
        }
        control.write_frame(b"transport-ack", 65536, deadline)?;
        let packet = media.read_frame(40 + 4096 * 4, deadline)?;
        let binding = WorkerBinding {
            session_tag: DecimalU64(1),
            epoch: 1,
        };
        let header = WorkerMediaHeader::decode(&packet, &binding, WorkerMediaKind::Source)
            .map_err(|_| "worker source codec rejected frame")?;
        if header.sequence != 0 || header.source_sample_index != 0 || header.sample_count != 2560 {
            return Err("source timeline mismatch".into());
        }
        for sample in packet[40..].as_chunks::<4>().0 {
            if f32::from_le_bytes(*sample) != 0.01_f32 {
                return Err("synthetic PCM mismatch".into());
            }
        }
        received_samples = header.sample_count;
        media.write_frame(&packet, 40 + 4096 * 4, deadline)?;
        if child.wait(Duration::from_secs(3))? != Some(0) {
            return Err("Python diagnostic failed or timed out".into());
        }
        Ok(())
    })();
    control.close();
    media.close();
    // Cleanup is confirmed by the owner querying the real Job, never a worker assertion.
    let cleanup = job.terminate();
    let remaining = job.active_processes();
    let clean = cleanup.is_ok() && matches!(remaining, Ok(0));
    if !clean {
        // A still-live child may hold stdout open. Never block draining its
        // scalar pipe when the real owner has not confirmed process-tree exit.
        return Err("owned process cleanup unconfirmed".into());
    }
    let mut scalar = Vec::new();
    if let Some(output) = child.take_output() {
        output.take(4097).read_to_end(&mut scalar)?;
    }
    if scalar.len() > 4096 {
        return Err("diagnostic scalar output bound".into());
    }
    let passed = run.is_ok() && clean;
    println!(
        "{{\"status\":\"{}\",\"control_and_media\":{},\"source_samples\":{},\"owned_job_empty\":{},\"wall_seconds\":{},\"model\":\"NOT_RUN\",\"audio\":\"NOT_RUN\",\"pcm_saved\":false}}",
        if passed {
            "NATIVE_PYTHON_TRANSPORT_ONLY_PASS"
        } else {
            "FAILED_MUTED"
        },
        run.is_ok(),
        received_samples,
        clean,
        started.elapsed().as_secs_f64()
    );
    run
}
#[cfg(not(all(windows, debug_assertions)))]
fn main() {
    eprintln!("Windows debug diagnostic only; NOT_RUN");
    std::process::exit(2);
}
