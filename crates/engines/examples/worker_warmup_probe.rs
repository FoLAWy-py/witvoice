//! Debug-only, explicit-approval real worker warmup diagnostic.
//! Build/static checks do not execute this main or authorize GPU acquisition.
//! No audio endpoint, output sink, media frames, or network API exists here.

#[cfg(all(windows, debug_assertions))]
#[path = "support/warmup_diagnostics.rs"]
mod warmup_diagnostics;

#[cfg(all(windows, debug_assertions))]
mod probe {
    use super::warmup_diagnostics::{RequestDeadline, Stage, Timing, classify_io};
    use std::{
        ffi::OsString,
        fs::File,
        io::{self, Read},
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
        stage: Stage,
        raw_os_error: Option<i32>,
    }
    impl Failure {
        fn at(mut self, stage: Stage) -> Self {
            self.stage = stage;
            self
        }
    }
    type Result<T> = std::result::Result<T, Failure>;

    fn failure(code: &'static str) -> Failure {
        Failure {
            code,
            fault: Fault::Protocol,
            stage: Stage::Preflight,
            raw_os_error: None,
        }
    }
    fn io_failure(error: io::Error) -> Failure {
        let detail = classify_io(&error);
        Failure {
            code: detail.code,
            fault: if detail.eof {
                Fault::Eof
            } else {
                Fault::Protocol
            },
            stage: Stage::Preflight,
            raw_os_error: detail.raw_os_error,
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
                stage: Stage::Preflight,
                raw_os_error: None,
            });
        }
        Ok(())
    }

    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct ModelIdentity {
        controller_pid: u32,
        model_pid: u32,
    }
    impl ModelIdentity {
        fn matches(&self, authenticated_pid: u32, members: &[u32]) -> bool {
            self.controller_pid == authenticated_pid
                && self.model_pid != 0
                && self.model_pid != self.controller_pid
                && members.contains(&authenticated_pid)
                && members.contains(&self.model_pid)
        }
    }
    struct ModelJob<'a> {
        root: &'a Path,
        job: &'a ProcessJob,
        controller_pid: u32,
    }
    impl ModelJob<'_> {
        fn verify(&self) -> Result<bool> {
            let path = self
                .root
                .join(".local/t011-finalize-warmup-once/model-process-private.json");
            let mut bytes = Vec::with_capacity(513);
            File::open(path)
                .map_err(io_failure)?
                .take(513)
                .read_to_end(&mut bytes)
                .map_err(io_failure)?;
            if bytes.len() > 512 {
                return Err(failure("model_identity_limit"));
            }
            let identity: ModelIdentity =
                serde_json::from_slice(&bytes).map_err(|_| failure("model_identity_invalid"))?;
            let members = self.job.process_ids().map_err(io_failure)?;
            Ok(identity.matches(self.controller_pid, &members))
        }
    }

    struct Observations {
        ready: bool,
        stopped: bool,
        heartbeat_count: u64,
        model_job_verified: Option<bool>,
        timing: Timing,
        clock: Instant,
        stage: Stage,
    }
    impl Observations {
        fn new() -> Self {
            Self {
                ready: false,
                stopped: false,
                heartbeat_count: 0,
                model_job_verified: None,
                timing: Timing::default(),
                clock: Instant::now(),
                stage: Stage::Preflight,
            }
        }
        fn now_ms(&self) -> u64 {
            u64::try_from(self.clock.elapsed().as_millis()).unwrap_or(u64::MAX)
        }
        fn begin_request(&mut self, stage: Stage) -> Result<RequestDeadline> {
            self.stage = stage;
            self.timing.begin_request(self.now_ms());
            RequestDeadline::new(Instant::now(), IO).ok_or_else(|| failure("clock_overflow"))
        }
    }

    fn exchange(
        control: &mut WorkerPipeServer,
        binding: &WorkerBinding,
        local: Binding,
        lifecycle: &mut WorkerLifecycle,
        observations: &mut Observations,
        clock: Instant,
        model_job: &ModelJob<'_>,
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
        let window = observations.begin_request(Stage::WarmupWrite)?;
        send(control, &warmup, window.absolute())?;
        observations.timing.complete_request();
        let mut sequence = 2u64;
        loop {
            live(lifecycle, clock)?;
            let heartbeat = request(binding, sequence, WorkerCommand::Heartbeat)?;
            sequence = sequence
                .checked_add(1)
                .ok_or_else(|| failure("request_id_exhausted"))?;
            // One outstanding Heartbeat; its send and every asynchronous Ready
            // read share this SAME 400ms absolute deadline, never renewed.
            let window = observations.begin_request(Stage::HeartbeatWrite)?;
            let deadline = window.absolute();
            send(control, &heartbeat, deadline)?;
            loop {
                observations.stage = Stage::HeartbeatRead;
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
                        observations
                            .timing
                            .acknowledge_heartbeat(observations.now_ms());
                        observations.heartbeat_count = observations
                            .heartbeat_count
                            .checked_add(1)
                            .ok_or_else(|| failure("heartbeat_count_exhausted"))?;
                        break;
                    }
                    WorkerEvent::Ready { capabilities } => {
                        observations.stage = Stage::ReadyValidate;
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
                            stage: Stage::HeartbeatRead,
                            raw_os_error: None,
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
        // Resources are held until Stop. Verify exact PID membership while
        // both controller/model are alive; arbitrary Job membership is not proof.
        observations.stage = Stage::ReadyValidate;
        let verified = model_job.verify()?;
        observations.model_job_verified = Some(verified);
        if !verified {
            return Err(failure("model_not_in_owned_job"));
        }
        // No sink exists. Invalidate policy permission BEFORE any Stop I/O.
        lifecycle.stop();
        let stop = request(binding, sequence, WorkerCommand::Stop)?;
        let window = observations.begin_request(Stage::StopWrite)?;
        let deadline = window.absolute();
        send(control, &stop, deadline)?;
        observations.stage = Stage::StopRead;
        let response = receive(control, binding, deadline)?;
        if response.request_id != stop.request_id || !matches!(response.event, WorkerEvent::Stopped)
        {
            return Err(failure("stop_response"));
        }
        observations.stopped = true;
        observations.timing.complete_request();
        // Stopped is protocol acknowledgement, never a resource-release proof.
        Ok(())
    }

    fn cleanup_job(
        job: &ProcessJob,
        control: &mut WorkerPipeServer,
        media: &mut WorkerPipeServer,
    ) -> (bool, Option<u32>, Option<Failure>) {
        control.close();
        media.close();
        let termination = job.terminate();
        let processes = job.active_processes();
        let count = processes.as_ref().ok().copied();
        let fault = termination
            .err()
            .or_else(|| processes.err())
            .map(|error| io_failure(error).at(Stage::OwnedCleanup));
        (fault.is_none() && count == Some(0), count, fault)
    }
    fn report_failure(report: &mut serde_json::Value, error: &Failure) {
        report["failure_stage"] = error.stage.as_str().into();
        report["failure_os_error"] = error.raw_os_error.into();
    }
    fn report_cleanup(report: &mut serde_json::Value, fault: Option<Failure>) {
        if let Some(error) = fault {
            report["cleanup_failure"] = error.code.into();
            report["cleanup_failure_os_error"] = error.raw_os_error.into();
        }
    }
    fn run() -> serde_json::Value {
        let mut report = serde_json::json!({
            "result":"FAILED", "model_ready":false, "ready_received":false,
            "sink_present":false, "output_open":false, "media_exchanged":false,
            "automatic_retries":0, "cleanup_confirmed":false,
            "owned_active_processes":null, "heartbeat_count":0, "max_heartbeat_gap_ms":null,
            "failure_stage":null, "failure_os_error":null,
            "failure_last_heartbeat_age_ms":null, "failure_request_elapsed_ms":null,
            "cleanup_failure":null, "cleanup_failure_os_error":null,
            "host_budget_bytes":HOST_BUDGET, "device_budget_bytes":DEVICE_BUDGET,
            "model_sha256":MODEL, "reference_id":REFERENCE,
            "lookahead_samples":640, "lookahead_is_total_delay":false,
            "native_rate":16000, "chunk_samples":2560, "duration_preserving":false,
            "engine":"meanvc2", "backend":"cuda", "failure":"preflight",
            "diagnostic_stage_recording":true, "model_job_membership_verified":null
        });
        let setup = (|| {
            if current_process_is_elevated().map_err(io_failure)? {
                return Err(failure("elevated_process"));
            }
            let root = Path::new(env!("CARGO_MANIFEST_DIR"))
                .ancestors()
                .nth(2)
                .ok_or_else(|| failure("fixed_root"))?;
            // Launch the fixed base interpreter directly. The venv redirector
            // creates a different worker PID, which strict pipe auth rejects.
            let python = Path::new(
                r"C:\Users\22198\.cache\codex-runtimes\codex-primary-runtime\dependencies\python\python.exe",
            );
            let runtime = root.join("workers/vc_worker/model_bootstrap.py");
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
                OsString::from("--prepare-diagnostic"),
                OsString::from("--control"),
                control_tag.into(),
                OsString::from("--media"),
                media_tag.into(),
                OsString::from("--node-pid"),
                std::process::id().to_string().into(),
            ];
            // Fixed binary/script; stdin receives exactly32 secret bytes; worker
            // stdout/stderr go to NUL, never raw audio/paths or probe stdout.
            let worker = match job.spawn_bootstrapped(python, &args, false, &credential) {
                Ok(worker) => worker,
                Err(error) => {
                    let (zero, count, cleanup_fault) = cleanup_job(&job, &mut control, &mut media);
                    report_cleanup(&mut report, cleanup_fault);
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
            let model_job = ModelJob {
                root,
                job: &job,
                controller_pid: worker.id(),
            };
            let protocol = (|| {
                let startup = Instant::now() + Duration::from_secs(3);
                observations.stage = Stage::AuthenticateControl;
                control.accept(worker.id(), startup).map_err(io_failure)?;
                observations.stage = Stage::AuthenticateMedia;
                media.accept(worker.id(), startup).map_err(io_failure)?;
                observations.stage = Stage::Preflight;
                let clock = Instant::now();
                exchange(
                    &mut control,
                    &binding,
                    local,
                    &mut lifecycle,
                    &mut observations,
                    clock,
                    &model_job,
                )
            })()
            .map_err(|error: Failure| error.at(observations.stage));
            // Snapshot before fault handling and owned cleanup; their duration
            // must not inflate the age at the original failure.
            if let Err(error) = &protocol {
                let now = observations.now_ms();
                report_failure(&mut report, error);
                report["failure_last_heartbeat_age_ms"] =
                    observations.timing.failure_age_ms(now).into();
                report["failure_request_elapsed_ms"] =
                    observations.timing.pending_elapsed_ms(now).into();
            }
            if let Err(error) = &protocol
                && lifecycle.binding().is_some()
                && !lifecycle.resources_released()
            {
                let _ = lifecycle.worker_fault(local, error.fault);
            }
            if observations.model_job_verified.is_none() {
                // Optional failure-path evidence, never overwrites first fault.
                observations.model_job_verified = model_job.verify().ok();
            }
            report["model_job_membership_verified"] = observations.model_job_verified.into();
            // Always close channels and terminate/query this owned Job, even
            // on EOF, invalid response, timeout, MemoryPressure, or Stopped.
            let (zero, active, cleanup_fault) = cleanup_job(&job, &mut control, &mut media);
            report_cleanup(&mut report, cleanup_fault);

            let policy_clean = if zero && lifecycle.binding().is_some() {
                lifecycle.cleanup_confirmed(local, true, true).is_ok()
            } else {
                zero
            };
            if protocol.is_ok() && (!zero || !policy_clean) {
                report_failure(
                    &mut report,
                    &cleanup_fault
                        .unwrap_or_else(|| failure("owned_cleanup").at(Stage::OwnedCleanup)),
                );
            }
            report["ready_received"] = observations.ready.into();
            report["heartbeat_count"] = observations.heartbeat_count.into();
            report["max_heartbeat_gap_ms"] = observations.timing.max_successful_gap_ms.into();
            report["stopped_ack"] = observations.stopped.into();
            report["cleanup_confirmed"] = policy_clean.into();
            if let Some(count) = active {
                report["owned_active_processes"] = count.into();
            }
            report["policy_output_allowed_after_cleanup"] = lifecycle.output_allowed().into();
            let successful = protocol.is_ok()
                && observations.ready
                && observations.stopped
                && observations.model_job_verified == Some(true)
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
            report_failure(&mut report, &error);
        }
        report
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn model_membership_requires_exact_authenticated_controller_and_child() {
            let identity = ModelIdentity {
                controller_pid: 42,
                model_pid: 84,
            };
            assert!(identity.matches(42, &[42, 84]));
            assert!(!identity.matches(99, &[42, 84, 99]));
            assert!(!identity.matches(42, &[42]));
            assert!(!identity.matches(42, &[84]));
            assert!(
                !ModelIdentity {
                    controller_pid: 42,
                    model_pid: 42
                }
                .matches(42, &[42])
            );
            assert!(
                !ModelIdentity {
                    controller_pid: 42,
                    model_pid: 0
                }
                .matches(42, &[42, 0])
            );
        }

        #[test]
        fn stage_propagation_preserves_os_number_and_error_class() {
            let error = io_failure(io::Error::from_raw_os_error(5)).at(Stage::AuthenticateMedia);
            assert_eq!(error.code, "ipc_failure");
            assert_eq!(error.raw_os_error, Some(5));
            assert_eq!(error.stage, Stage::AuthenticateMedia);
            let partial =
                io_failure(io::Error::from(io::ErrorKind::UnexpectedEof)).at(Stage::HeartbeatRead);
            assert_eq!(partial.code, "ipc_eof");
            assert_eq!(partial.raw_os_error, None);
            assert_eq!(partial.stage, Stage::HeartbeatRead);
        }

        #[test]
        fn cleanup_observation_does_not_overwrite_first_protocol_failure() {
            let mut report = serde_json::json!({"failure":"ipc_deadline"});
            let first = io_failure(io::Error::from(io::ErrorKind::TimedOut)).at(Stage::StopRead);
            report_failure(&mut report, &first);
            report_cleanup(
                &mut report,
                Some(io_failure(io::Error::from_raw_os_error(5)).at(Stage::OwnedCleanup)),
            );
            assert_eq!(report["failure"], "ipc_deadline");
            assert_eq!(report["failure_stage"], "stop_read");
            assert!(report["failure_os_error"].is_null());
            assert_eq!(report["cleanup_failure_os_error"], 5);
        }
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
