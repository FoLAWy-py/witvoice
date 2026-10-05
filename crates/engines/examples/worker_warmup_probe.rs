//! Debug-only, explicit-approval real worker warmup diagnostic.
//! Build/static checks do not execute this main or authorize GPU acquisition.
//! No audio endpoint, output sink, media frames, or network API exists here.

#[cfg(all(windows, debug_assertions))]
mod probe {
    use std::{
        ffi::OsString,
        io,
        path::Path,
        time::{Duration, Instant},
    };
    use witvoice_contracts::{
        CONTROL_MAX_BYTES, PROTOCOL_VERSION,
        control::PreparedCapabilities,
        values::{DecimalU64, Id, Sha256},
        worker::{
            WorkerBackend, WorkerBinding, WorkerCommand, WorkerEvent, WorkerRequest,
            WorkerResponse, decode_worker_response,
        },
    };
    use witvoice_engines::lifecycle::{Action, Binding, Fault, WarmupProof, WorkerLifecycle};
    use witvoice_platform::{ProcessJob, Secret, WorkerPipeServer, current_process_is_elevated};

    const MODEL: &str = "01caafec9a3991a5514df9412952d24ae3370406358a01e20e03df41f2f5d515";
    const REFERENCE: &str = "00000000-0000-0000-0000-000000000011";
    const SCHEMA: &str = "meanvc2-reference-local-v1";
    const HOST_BUDGET: u64 = 6 * 1024 * 1024 * 1024;
    const DEVICE_BUDGET: u64 = 4 * 1024 * 1024 * 1024;
    const WARMUP_MS: u64 = 120_000;
    const IO: Duration = Duration::from_millis(400);
    const BEAT: Duration = Duration::from_millis(100);

    #[derive(Clone, Copy)]
    struct Failure {
        code: &'static str,
        fault: Fault,
    }
    type Result<T> = std::result::Result<T, Failure>;

    fn failure(code: &'static str) -> Failure {
        Failure {
            code,
            fault: Fault::Protocol,
        }
    }
    fn io_failure(error: io::Error) -> Failure {
        Failure {
            code: match error.kind() {
                io::ErrorKind::TimedOut => "ipc_deadline",
                io::ErrorKind::UnexpectedEof => "ipc_eof",
                _ => "ipc_failure",
            },
            fault: if error.kind() == io::ErrorKind::UnexpectedEof {
                Fault::Eof
            } else {
                Fault::Protocol
            },
        }
    }
    fn id(sequence: u64) -> Result<Id> {
        if sequence > 0xffff_ffff_ffff {
            return Err(failure("request_id_exhausted"));
        }
        Id::try_from(format!("00000000-0000-0000-0000-{sequence:012x}"))
            .map_err(|_| failure("request_id"))
    }
    fn model_hash() -> Result<Sha256> {
        Sha256::try_from(MODEL.to_owned()).map_err(|_| failure("fixed_model_hash"))
    }
    fn proof() -> Result<WarmupProof> {
        let mut digest = [0; 32];
        for (index, byte) in digest.iter_mut().enumerate() {
            *byte = u8::from_str_radix(&MODEL[index * 2..index * 2 + 2], 16)
                .map_err(|_| failure("fixed_model_hash"))?;
        }
        Ok(WarmupProof(digest))
    }
    fn elapsed_ms(start: Instant) -> Result<u64> {
        u64::try_from(start.elapsed().as_millis()).map_err(|_| failure("clock_overflow"))
    }
    fn request(
        binding: &WorkerBinding,
        sequence: u64,
        command: WorkerCommand,
    ) -> Result<WorkerRequest> {
        Ok(WorkerRequest {
            protocol_version: PROTOCOL_VERSION,
            request_id: id(sequence)?,
            binding: binding.clone(),
            command,
        })
    }
    fn send(pipe: &mut WorkerPipeServer, request: &WorkerRequest, deadline: Instant) -> Result<()> {
        let payload = serde_json::to_vec(request).map_err(|_| failure("encode_control"))?;
        pipe.write_frame(&payload, CONTROL_MAX_BYTES, deadline)
            .map_err(io_failure)
    }
    fn receive(
        pipe: &mut WorkerPipeServer,
        binding: &WorkerBinding,
        deadline: Instant,
    ) -> Result<WorkerResponse> {
        let payload = pipe
            .read_frame(CONTROL_MAX_BYTES, deadline)
            .map_err(io_failure)?;
        decode_worker_response(&payload, binding).map_err(|_| failure("invalid_control"))
    }
    fn validate_ready(cap: &PreparedCapabilities, warmup: &Id) -> Result<()> {
        cap.validate().map_err(|_| failure("invalid_capability"))?;
        let expected_run = format!("T011-warmup-{}", String::from(warmup.clone()));
        if cap.model_sha256 != model_hash()?
            || cap.engine_id != "meanvc2"
            || cap.backend != "cuda"
            || cap.native_input_rate != 16_000
            || cap.native_output_rate != 16_000
            || cap.chunk_samples != 2560
            || cap.lookahead_samples != 640
            || cap.conditioning_schema != SCHEMA
            || cap.duration_preserving
            || cap.capability_test_run_id != expected_run
            || cap.model_memory_budget_bytes.0 != HOST_BUDGET
            || cap.device_memory_budget_bytes != Some(DecimalU64(DEVICE_BUDGET))
        {
            return Err(failure("unexpected_capability"));
        }
        Ok(())
    }
    fn live(lifecycle: &mut WorkerLifecycle, clock: Instant) -> Result<()> {
        if lifecycle.monitor(elapsed_ms(clock)?) != Action::None {
            return Err(Failure {
                code: "lifecycle_deadline",
                fault: lifecycle.first_fault().unwrap_or(Fault::Protocol),
            });
        }
        Ok(())
    }

    struct Observations {
        ready: bool,
        stopped: bool,
        heartbeat_count: u64,
        last_heartbeat_ms: u64,
        max_gap_ms: u64,
    }
    impl Observations {
        fn new() -> Self {
            Self {
                ready: false,
                stopped: false,
                heartbeat_count: 0,
                last_heartbeat_ms: 0,
                max_gap_ms: 0,
            }
        }
    }

    fn exchange(
        control: &mut WorkerPipeServer,
        binding: &WorkerBinding,
        local: Binding,
        lifecycle: &mut WorkerLifecycle,
        observations: &mut Observations,
        clock: Instant,
    ) -> Result<()> {
        let expected_proof = proof()?;
        lifecycle
            .begin(local, expected_proof, 0, WARMUP_MS)
            .map_err(|_| failure("lifecycle_begin"))?;
        // Reference has no field in Ready. Fixed outbound reference plus the
        // original Warmup request_id is its correlation proof, not a new field.
        let warmup = request(
            binding,
            1,
            WorkerCommand::Warmup {
                model_sha256: model_hash()?,
                reference_id: Id::try_from(REFERENCE.to_owned())
                    .map_err(|_| failure("fixed_reference"))?,
                backend: WorkerBackend::Cuda,
            },
        )?;
        send(control, &warmup, Instant::now() + IO)?;
        let mut sequence = 2u64;
        loop {
            live(lifecycle, clock)?;
            let heartbeat = request(binding, sequence, WorkerCommand::Heartbeat)?;
            sequence = sequence
                .checked_add(1)
                .ok_or_else(|| failure("request_id_exhausted"))?;
            // One outstanding Heartbeat; its send and every asynchronous Ready
            // read share this SAME 400ms absolute deadline, never renewed.
            let deadline = Instant::now() + IO;
            send(control, &heartbeat, deadline)?;
            loop {
                let response = receive(control, binding, deadline)?;
                live(lifecycle, clock)?;
                match response.event {
                    WorkerEvent::Heartbeat => {
                        if response.request_id != heartbeat.request_id {
                            return Err(failure("heartbeat_request_id"));
                        }
                        let now = elapsed_ms(clock)?;
                        let action = lifecycle
                            .heartbeat(local, now)
                            .map_err(|_| failure("lifecycle_heartbeat"))?;
                        if action != Action::None {
                            return Err(failure("lifecycle_heartbeat"));
                        }
                        observations.max_gap_ms = observations
                            .max_gap_ms
                            .max(now - observations.last_heartbeat_ms);
                        observations.last_heartbeat_ms = now;
                        observations.heartbeat_count = observations
                            .heartbeat_count
                            .checked_add(1)
                            .ok_or_else(|| failure("heartbeat_count_exhausted"))?;
                        break;
                    }
                    WorkerEvent::Ready { capabilities } => {
                        if observations.ready || response.request_id != warmup.request_id {
                            return Err(failure("ready_request_id_or_duplicate"));
                        }
                        validate_ready(&capabilities, &warmup.request_id)?;
                        if lifecycle
                            .warmup_ready(local, expected_proof, elapsed_ms(clock)?)
                            .map_err(|_| failure("lifecycle_ready"))?
                            != Action::None
                        {
                            return Err(failure("lifecycle_ready"));
                        }
                        observations.ready = true;
                        // Continue waiting for the pending Heartbeat with its
                        // original deadline. Ready does not refresh liveness.
                    }
                    WorkerEvent::MemoryPressure => {
                        if response.request_id != warmup.request_id {
                            return Err(failure("memory_request_id"));
                        }
                        return Err(Failure {
                            code: "memory_pressure",
                            fault: Fault::MemoryPressure,
                        });
                    }
                    WorkerEvent::Failed { .. } => {
                        if response.request_id != warmup.request_id {
                            return Err(failure("failure_request_id"));
                        }
                        return Err(failure("worker_failed"));
                    }
                    _ => return Err(failure("unexpected_event")),
                }
            }
            if observations.ready {
                break;
            }
            std::thread::sleep(BEAT);
        }
        // No sink exists. Invalidate policy permission BEFORE any Stop I/O.
        lifecycle.stop();
        let stop = request(binding, sequence, WorkerCommand::Stop)?;
        let deadline = Instant::now() + IO;
        send(control, &stop, deadline)?;
        let response = receive(control, binding, deadline)?;
        if response.request_id != stop.request_id || !matches!(response.event, WorkerEvent::Stopped)
        {
            return Err(failure("stop_response"));
        }
        observations.stopped = true;
        // Stopped is protocol acknowledgement, never a resource-release proof.
        Ok(())
    }

    fn cleanup_job(
        job: &ProcessJob,
        control: &mut WorkerPipeServer,
        media: &mut WorkerPipeServer,
    ) -> (bool, Option<u32>) {
        control.close();
        media.close();
        let termination = job.terminate();
        let count = job.active_processes().ok();
        (termination.is_ok() && count == Some(0), count)
    }
    fn run() -> serde_json::Value {
        let mut report = serde_json::json!({
            "result":"FAILED", "model_ready":false, "ready_received":false,
            "sink_present":false, "output_open":false, "media_exchanged":false,
            "automatic_retries":0, "cleanup_confirmed":false,
            "owned_active_processes":null, "heartbeat_count":0, "max_heartbeat_gap_ms":null,
            "host_budget_bytes":HOST_BUDGET, "device_budget_bytes":DEVICE_BUDGET,
            "model_sha256":MODEL, "reference_id":REFERENCE,
            "lookahead_samples":640, "lookahead_is_total_delay":false,
            "native_rate":16000, "chunk_samples":2560, "duration_preserving":false,
            "engine":"meanvc2", "backend":"cuda", "failure":"preflight"
        });
        let setup = (|| {
            if current_process_is_elevated().map_err(io_failure)? {
                return Err(failure("elevated_process"));
            }
            let root = Path::new(env!("CARGO_MANIFEST_DIR"))
                .ancestors()
                .nth(2)
                .ok_or_else(|| failure("fixed_root"))?;
            let python = root.join(".local/venvs/meanvc2-m0/Scripts/python.exe");
            let runtime = root.join("workers/vc_worker/runtime.py");
            if !python.is_file() || !runtime.is_file() {
                return Err(failure("fixed_runtime_missing"));
            }
            let credential = Secret::generate().map_err(io_failure)?;
            // Endpoint/binding randomness is independent of the secret token.
            let entropy = Secret::generate().map_err(io_failure)?;
            let mut bytes = [0; 8];
            bytes.copy_from_slice(&entropy.as_bytes()[..8]);
            let session = u64::from_le_bytes(bytes);
            let local = Binding::new(session, 1).map_err(|_| failure("zero_random_binding"))?;
            let binding = WorkerBinding {
                session_tag: DecimalU64(session),
                epoch: 1,
            };
            let control_tag = format!("wu{session:016x}-c");
            let media_tag = format!("wu{session:016x}-m");
            let mut control =
                WorkerPipeServer::bind(&control_tag, Secret::from_bytes(*credential.as_bytes()))
                    .map_err(io_failure)?;
            let mut media =
                WorkerPipeServer::bind(&media_tag, Secret::from_bytes(*credential.as_bytes()))
                    .map_err(io_failure)?;
            let job = ProcessJob::new(false).map_err(io_failure)?;
            let args = vec![
                runtime.into_os_string(),
                OsString::from("--control"),
                control_tag.into(),
                OsString::from("--media"),
                media_tag.into(),
                OsString::from("--node-pid"),
                std::process::id().to_string().into(),
            ];
            // Fixed binary/script; stdin receives exactly32 secret bytes; worker
            // stdout/stderr go to NUL, never raw audio/paths or probe stdout.
            let worker = match job.spawn_bootstrapped(&python, &args, false, &credential) {
                Ok(worker) => worker,
                Err(error) => {
                    let (zero, count) = cleanup_job(&job, &mut control, &mut media);
                    report["cleanup_confirmed"] = zero.into();
                    if let Some(count) = count {
                        report["owned_active_processes"] = count.into();
                    }
                    return Err(io_failure(error));
                }
            };
            let mut lifecycle =
                WorkerLifecycle::new(session).map_err(|_| failure("lifecycle_new"))?;
            let mut observations = Observations::new();
            let protocol = (|| {
                let startup = Instant::now() + Duration::from_secs(3);
                control.accept(worker.id(), startup).map_err(io_failure)?;
                media.accept(worker.id(), startup).map_err(io_failure)?;
                let clock = Instant::now();
                exchange(
                    &mut control,
                    &binding,
                    local,
                    &mut lifecycle,
                    &mut observations,
                    clock,
                )
            })();
            if let Err(error) = &protocol
                && lifecycle.binding().is_some()
                && !lifecycle.resources_released()
            {
                let _ = lifecycle.worker_fault(local, error.fault);
            }
            // Always close channels and terminate/query this owned Job, even
            // on EOF, invalid response, timeout, MemoryPressure, or Stopped.
            let (zero, active) = cleanup_job(&job, &mut control, &mut media);
            let policy_clean = if zero && lifecycle.binding().is_some() {
                lifecycle.cleanup_confirmed(local, true, true).is_ok()
            } else {
                zero
            };
            report["ready_received"] = observations.ready.into();
            report["heartbeat_count"] = observations.heartbeat_count.into();
            if observations.heartbeat_count > 0 {
                report["max_heartbeat_gap_ms"] = observations.max_gap_ms.into();
            }
            report["stopped_ack"] = observations.stopped.into();
            report["cleanup_confirmed"] = policy_clean.into();
            if let Some(count) = active {
                report["owned_active_processes"] = count.into();
            }
            report["policy_output_allowed_after_cleanup"] = lifecycle.output_allowed().into();
            let successful = protocol.is_ok()
                && observations.ready
                && observations.stopped
                && zero
                && policy_clean;
            report["model_ready"] = successful.into();
            report["result"] = if successful { "READY" } else { "FAILED" }.into();
            report["failure"] = match protocol {
                Err(error) => error.code,
                Ok(()) if !zero || !policy_clean => "owned_cleanup",
                _ => "none",
            }
            .into();
            Ok::<(), Failure>(())
        })();
        if let Err(error) = setup {
            report["failure"] = error.code.into();
        }
        report
    }

    pub fn main() {
        let args: Vec<OsString> = std::env::args_os().collect();
        if args.len() != 2 || args[1] != "--approve-real-warmup" {
            println!(
                "{}",
                serde_json::json!({"result":"NOT_RUN","failure":"explicit_real_warmup_approval_required"})
            );
            std::process::exit(2);
        }
        let report = run();
        let success = report["result"] == "READY";
        println!("{report}");
        if !success {
            std::process::exit(1);
        }
    }
}

#[cfg(all(windows, debug_assertions))]
fn main() {
    probe::main();
}

#[cfg(not(all(windows, debug_assertions)))]
fn main() {
    eprintln!("worker warmup diagnostic requires a Windows debug build");
    std::process::exit(2);
}
