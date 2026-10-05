//! Node-owned control worker owner. Run on a dedicated non-realtime control task.
//! The caller supplies authoritative epochs and polls at least every 100ms.
//! No capture, PCM inference, UI, retry loop, or model selector lives here.
//! Current fixed development runtime layout is not an installed distribution.
use crate::lifecycle::{Action, Binding, Fault, Phase, WarmupProof, WorkerLifecycle};
use std::{
    ffi::OsString,
    io,
    path::Path,
    time::{Duration, Instant},
};
use witvoice_contracts::{
    CONTROL_MAX_BYTES, ErrorCode, PROTOCOL_VERSION,
    control::PreparedCapabilities,
    values::{DecimalU64, Id, Sha256},
    worker::{WorkerBinding, WorkerCommand, WorkerEvent, WorkerRequest, decode_worker_response},
};
use witvoice_platform::{
    MemberFailureSnapshot, Process, ProcessJob, Secret, WorkerPipeServer,
    current_process_is_elevated,
};

const MODEL: &str = "01caafec9a3991a5514df9412952d24ae3370406358a01e20e03df41f2f5d515";
const REFERENCE: &str = "00000000-0000-0000-0000-000000000011";
const PYTHON: &str =
    r"C:\Users\22198\.cache\codex-runtimes\codex-primary-runtime\dependencies\python\python.exe";
const IO_WINDOW: Duration = Duration::from_millis(400);
const STARTUP: Duration = Duration::from_millis(2500);
const CLEANUP_WINDOW: Duration = Duration::from_secs(3);
const BEAT: Duration = Duration::from_millis(100);
const WARMUP_MS: u64 = 120_000;
const MAX_EVENTS_PER_REQUEST: usize = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FailureKind {
    Policy,
    Configuration,
    Protocol,
    Io,
    Worker,
    Cleanup,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Failure {
    pub kind: FailureKind,
    pub fault: Fault,
    pub raw_os_error: Option<i32>,
    pub wire_code: Option<ErrorCode>,
    pub native_failure: Option<witvoice_platform::ProcessNativeFailure>,
}

/// Closed stage names and scalar observations; no paths, credentials or PID list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CleanupStage {
    Precondition,
    Terminate,
    QueryActive,
    QueryIds,
    WaitController,
    ConfirmPolicy,
    Released,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CleanupSnapshot {
    pub stage: CleanupStage,
    pub terminate_succeeded: Option<bool>,
    pub active_processes: Option<u32>,
    pub process_id_count: Option<u32>,
    pub controller_signaled: Option<bool>,
    pub controller_exit_code: Option<u32>,
    pub policy_confirmed: Option<bool>,
    pub stopped_ack: bool,
    pub elapsed_ms: Option<u64>,
    pub primary_failure: Option<Failure>,
    pub cleanup_failure: Option<Failure>,
}
// The same closed predicates are driven by production native results and pure
// observation tests. This value cannot release a Job or affect an output gate.
pub(crate) struct CleanupObservation {
    pub(crate) snapshot: CleanupSnapshot,
}
impl CleanupObservation {
    pub(crate) fn new(primary_failure: Option<Failure>, stopped_ack: bool) -> Self {
        Self {
            snapshot: CleanupSnapshot {
                stage: CleanupStage::Precondition,
                terminate_succeeded: None,
                active_processes: None,
                process_id_count: None,
                controller_signaled: None,
                controller_exit_code: None,
                policy_confirmed: None,
                stopped_ack,
                elapsed_ms: None,
                primary_failure,
                cleanup_failure: None,
            },
        }
    }
    pub(crate) fn reject(&mut self, error: Failure) -> Failure {
        self.snapshot.cleanup_failure = Some(error);
        error
    }
    pub(crate) fn terminate(&mut self, result: Result<()>) -> Result<()> {
        self.snapshot.stage = CleanupStage::Terminate;
        self.snapshot.terminate_succeeded = Some(result.is_ok());
        result.map_err(|error| self.reject(error))
    }
    pub(crate) fn active(&mut self, result: Result<u32>) -> Result<()> {
        self.snapshot.stage = CleanupStage::QueryActive;
        let actual = result.map_err(|error| self.reject(error))?;
        self.snapshot.active_processes = Some(actual);
        if actual != 0 {
            return Err(self.reject(failure(FailureKind::Cleanup, Fault::Protocol)));
        }
        Ok(())
    }
    pub(crate) fn ids(&mut self, result: Result<Vec<u32>>) -> Result<()> {
        self.snapshot.stage = CleanupStage::QueryIds;
        let actual = result.map_err(|error| self.reject(error))?;
        let count = u32::try_from(actual.len())
            .map_err(|_| self.reject(failure(FailureKind::Cleanup, Fault::Protocol)))?;
        self.snapshot.process_id_count = Some(count);
        if count != 0 {
            return Err(self.reject(failure(FailureKind::Cleanup, Fault::Protocol)));
        }
        Ok(())
    }
    pub(crate) fn wait(&mut self, result: Result<Option<u32>>) -> Result<()> {
        self.snapshot.stage = CleanupStage::WaitController;
        let actual = result.map_err(|error| self.reject(error))?;
        self.snapshot.controller_signaled = Some(actual.is_some());
        self.snapshot.controller_exit_code = actual;
        if actual.is_none() {
            return Err(self.reject(failure(FailureKind::Cleanup, Fault::Protocol)));
        }
        Ok(())
    }
    pub(crate) fn wait_until(
        &mut self,
        deadline: Instant,
        now: Instant,
        wait: impl FnOnce(Duration) -> io::Result<Option<u32>>,
    ) -> Result<()> {
        self.snapshot.stage = CleanupStage::WaitController;
        let remaining = deadline
            .checked_duration_since(now)
            .filter(|remaining| !remaining.is_zero())
            .ok_or_else(|| {
                self.reject(cleanup_io_failure(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "controller cleanup deadline",
                )))
            })?;
        self.wait(wait(remaining).map_err(cleanup_io_failure))
    }
    pub(crate) fn confirm(
        &mut self,
        result: std::result::Result<Action, crate::lifecycle::Error>,
    ) -> Result<()> {
        self.snapshot.stage = CleanupStage::ConfirmPolicy;
        self.snapshot.policy_confirmed = Some(result.is_ok());
        policy(result)
            .map(|_| ())
            .map_err(|error| self.reject(error))
    }
    pub(crate) fn finish(mut self, elapsed_ms: Option<u64>, released: bool) -> CleanupSnapshot {
        self.snapshot.elapsed_ms = elapsed_ms;
        if released {
            self.snapshot.stage = CleanupStage::Released;
        }
        self.snapshot
    }
}
pub(crate) fn remember_cleanup(first: &mut Option<CleanupSnapshot>, actual: CleanupSnapshot) {
    // First refusal remains immutable, including across cleanup attempts/Drop.
    if first.is_none_or(|saved| saved.cleanup_failure.is_none()) {
        *first = Some(actual);
    }
}
pub(crate) fn cleanup_io_failure(error: io::Error) -> Failure {
    Failure {
        kind: FailureKind::Cleanup,
        ..io_failure(error)
    }
}

pub type Result<T> = std::result::Result<T, Failure>;
fn failure(kind: FailureKind, fault: Fault) -> Failure {
    Failure {
        kind,
        fault,
        raw_os_error: None,
        wire_code: None,
        native_failure: None,
    }
}
fn io_failure(error: io::Error) -> Failure {
    let fault = match (error.kind(), error.raw_os_error()) {
        (io::ErrorKind::TimedOut, _) => Fault::HeartbeatTimeout,
        (io::ErrorKind::UnexpectedEof | io::ErrorKind::BrokenPipe, _) | (_, Some(109 | 233)) => {
            Fault::Eof
        }
        _ => Fault::Protocol,
    };
    Failure {
        raw_os_error: error.raw_os_error(),
        native_failure: witvoice_platform::process_native_failure(&error),
        ..failure(FailureKind::Io, fault)
    }
}
fn policy<T>(value: std::result::Result<T, crate::lifecycle::Error>) -> Result<T> {
    value.map_err(|_| failure(FailureKind::Policy, Fault::Protocol))
}
fn digest() -> WarmupProof {
    let mut bytes = [0; 32];
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&MODEL[index * 2..index * 2 + 2], 16)
            .expect("constant model digest");
    }
    WarmupProof(bytes)
}
fn fixed_hash() -> Sha256 {
    Sha256::try_from(MODEL.to_owned()).expect("constant model hash")
}
fn identifier(sequence: u64) -> Result<Id> {
    if sequence == 0 || sequence > 0xffff_ffff_ffff {
        return Err(failure(FailureKind::Policy, Fault::SequenceExhausted));
    }
    Id::try_from(format!("00000000-0000-0000-0000-{sequence:012x}"))
        .map_err(|_| failure(FailureKind::Policy, Fault::Protocol))
}
fn validate_capabilities(cap: &PreparedCapabilities, warmup: &Id) -> Result<()> {
    if cap.validate().is_err()
        || cap.model_sha256 != fixed_hash()
        || cap.engine_id != "meanvc2"
        || cap.backend != "cuda"
        || cap.native_input_rate != 16000
        || cap.native_output_rate != 16000
        || cap.chunk_samples != 2560
        || cap.lookahead_samples != 640
        || cap.conditioning_schema != "meanvc2-reference-local-v1"
        || cap.duration_preserving
        || cap.capability_test_run_id != format!("T011-warmup-{}", String::from(warmup.clone()))
        || cap.model_memory_budget_bytes != DecimalU64(8 * 1024 * 1024 * 1024)
        || cap.device_memory_budget_bytes != Some(DecimalU64(4 * 1024 * 1024 * 1024))
    {
        return Err(failure(FailureKind::Protocol, Fault::UnexpectedWarmupProof));
    }
    Ok(())
}
struct OwnedWorker {
    job: ProcessJob,
    process: Option<Process>,
    control: WorkerPipeServer,
    media: WorkerPipeServer,
    binding: WorkerBinding,
    warmup_id: Id,
    next_beat: Instant,
    policy_started: bool,
    restarting: bool,
    control_tag: String,
    media_tag: String,
}

/// Sole owner of one Job and two persistent authenticated pipes.
/// A returned Ready permission is policy permission, never permission to open a sink.
/// T014 must bind failure invalidation to the canonical Runtime and real OutputGate.
pub struct WorkerSupervisor {
    lifecycle: WorkerLifecycle,
    session_tag: u64,
    clock: Instant,
    owned: Option<OwnedWorker>,
    sequence: u64,
    capabilities: Option<PreparedCapabilities>,
    first_failure: Option<Failure>,
    cleanup_snapshot: Option<CleanupSnapshot>,
    member_failure_snapshot: Option<MemberFailureSnapshot>,
    last_cleaned_pid: Option<u32>,
    stopped_ack: bool,
    #[cfg(test)]
    fixture: Option<&'static str>,
}
impl WorkerSupervisor {
    /// Uses only the fixed audited development entry point; no UI path argument.
    pub fn new(session_tag: u64) -> Result<Self> {
        if current_process_is_elevated().map_err(io_failure)? {
            return Err(failure(FailureKind::Configuration, Fault::Protocol));
        }
        Ok(Self {
            lifecycle: policy(WorkerLifecycle::new(session_tag))?,
            session_tag,
            clock: Instant::now(),
            owned: None,
            sequence: 1,
            capabilities: None,
            first_failure: None,
            cleanup_snapshot: None,
            member_failure_snapshot: None,
            last_cleaned_pid: None,
            stopped_ack: false,
            #[cfg(test)]
            fixture: None,
        })
    }
    pub fn phase(&self) -> Phase {
        self.lifecycle.phase()
    }
    pub fn binding(&self) -> Option<Binding> {
        self.lifecycle.binding()
    }
    pub fn output_allowed(&self) -> bool {
        self.lifecycle.output_allowed() && self.capabilities.is_some()
    }
    pub fn resources_released(&self) -> bool {
        self.owned.is_none() && self.lifecycle.resources_released()
    }
    pub fn restart_count(&self) -> u8 {
        self.lifecycle.restart_count()
    }
    pub fn first_failure(&self) -> Option<Failure> {
        self.first_failure
    }
    pub fn cleanup_snapshot(&self) -> Option<CleanupSnapshot> {
        self.cleanup_snapshot
    }
    pub fn capabilities(&self) -> Option<&PreparedCapabilities> {
        self.capabilities.as_ref()
    }
    pub fn worker_pid(&self) -> Option<u32> {
        self.owned.as_ref()?.process.as_ref().map(Process::id)
    }
    pub fn last_cleaned_pid(&self) -> Option<u32> {
        self.last_cleaned_pid
    }
    pub fn stopped_ack(&self) -> bool {
        self.stopped_ack
    }
    pub fn channels_authenticated(&self) -> bool {
        self.owned
            .as_ref()
            .is_some_and(|own| own.control.is_connected() && own.media.is_connected())
    }
    fn now_ms(&self) -> Result<u64> {
        u64::try_from(self.clock.elapsed().as_millis())
            .map_err(|_| failure(FailureKind::Policy, Fault::ClockOverflow))
    }
    fn request(&mut self, command: WorkerCommand) -> Result<WorkerRequest> {
        let id = identifier(self.sequence)?;
        self.sequence = self
            .sequence
            .checked_add(1)
            .ok_or_else(|| failure(FailureKind::Policy, Fault::SequenceExhausted))?;
        Ok(WorkerRequest {
            protocol_version: PROTOCOL_VERSION,
            request_id: id,
            binding: self
                .owned
                .as_ref()
                .ok_or_else(|| failure(FailureKind::Policy, Fault::Protocol))?
                .binding
                .clone(),
            command,
        })
    }
    fn send(&mut self, request: &WorkerRequest, deadline: Instant) -> Result<()> {
        let bytes = serde_json::to_vec(request)
            .map_err(|_| failure(FailureKind::Protocol, Fault::Protocol))?;
        self.owned
            .as_mut()
            .ok_or_else(|| failure(FailureKind::Policy, Fault::Protocol))?
            .control
            .write_frame(&bytes, CONTROL_MAX_BYTES, deadline)
            .map_err(io_failure)
    }
    fn launch(&mut self, binding: Binding, restarting: bool) -> Result<()> {
        if self.owned.is_some() {
            return Err(failure(FailureKind::Policy, Fault::Protocol));
        }
        // Validate policy before creating any process. These checks acquire no resources.
        if binding.session_tag != self.session_tag
            || binding.epoch == 0
            || self.binding().is_some_and(|old| binding.epoch <= old.epoch)
            || (!restarting && self.phase() != Phase::Stopped)
            || (restarting && self.phase() != Phase::RetryPending)
        {
            return Err(failure(FailureKind::Policy, Fault::Protocol));
        }
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()
            .map_err(io_failure)?;
        let script = root.join("workers/vc_worker/model_bootstrap.py");
        #[cfg(test)]
        let script = if self.fixture.is_some() {
            root.join("tests/integration/worker_supervision_peer.py")
        } else {
            script
        };
        if !Path::new(PYTHON).is_file() || !script.is_file() {
            return Err(failure(FailureKind::Configuration, Fault::Protocol));
        }
        let credential = Secret::generate().map_err(io_failure)?;
        let entropy = Secret::generate().map_err(io_failure)?;
        let unique = u64::from_le_bytes(
            entropy.as_bytes()[..8]
                .try_into()
                .expect("fixed secret length"),
        );
        let control_tag = format!("ws{unique:016x}-{}-c", binding.epoch);
        let media_tag = format!("ws{unique:016x}-{}-m", binding.epoch);
        let control =
            WorkerPipeServer::bind(&control_tag, Secret::from_bytes(*credential.as_bytes()))
                .map_err(io_failure)?;
        let media = WorkerPipeServer::bind(&media_tag, Secret::from_bytes(*credential.as_bytes()))
            .map_err(io_failure)?;
        let job = ProcessJob::new(false).map_err(io_failure)?;
        job.track_lifetime_members().map_err(io_failure)?;
        let warmup_id = identifier(self.sequence)?;
        if !restarting {
            self.first_failure = None;
        }
        self.capabilities = None;
        self.stopped_ack = false;
        self.owned = Some(OwnedWorker {
            job,
            process: None,
            control,
            media,
            binding: WorkerBinding {
                session_tag: DecimalU64(binding.session_tag),
                epoch: binding.epoch,
            },
            warmup_id,
            next_beat: Instant::now(),
            policy_started: false,
            restarting,
            control_tag,
            media_tag,
        });
        let started = (|| {
            let own = self.owned.as_mut().expect("new owner");
            let args = vec![
                OsString::from("-I"),
                OsString::from("-S"),
                OsString::from("-B"),
                script.into_os_string(),
                OsString::from("--control"),
                own.control_tag.clone().into(),
                OsString::from("--media"),
                own.media_tag.clone().into(),
                OsString::from("--node-pid"),
                std::process::id().to_string().into(),
            ];
            #[cfg(test)]
            let args = {
                let mut args = args;
                if let Some(mode) = self.fixture {
                    args.extend([OsString::from("--case"), mode.into()]);
                }
                args
            };
            let startup = Instant::now() + STARTUP;
            let process = own
                .job
                .spawn_bootstrapped(Path::new(PYTHON), &args, false, &credential)
                .map_err(io_failure)?;
            let pid = process.id();
            own.process = Some(process);
            own.job.retain_lifetime_members().map_err(io_failure)?;
            own.control.accept(pid, startup).map_err(io_failure)?;
            own.media.accept(pid, startup).map_err(io_failure)?;
            own.job.retain_lifetime_members().map_err(io_failure)?;
            self.start_policy()?;
            let warmup = self.request(WorkerCommand::Warmup {
                model_sha256: fixed_hash(),
                reference_id: Id::try_from(REFERENCE.to_owned()).expect("constant reference"),
                backend: witvoice_contracts::worker::WorkerBackend::Cuda,
            })?;
            self.owned.as_mut().expect("owner").warmup_id = warmup.request_id.clone();
            self.send(&warmup, self.request_deadline()?)
        })();
        if let Err(error) = started {
            return self.retire(error);
        }
        Ok(())
    }
    pub fn begin(&mut self, binding: Binding) -> Result<()> {
        self.launch(binding, false)
    }
    /// The caller provides a fresh canonical epoch; this method never invents one.
    pub fn restart(&mut self, binding: Binding) -> Result<()> {
        self.launch(binding, true)
    }
    fn start_policy(&mut self) -> Result<()> {
        let own = self.owned.as_ref().expect("owner");
        if own.policy_started {
            return Ok(());
        }
        let binding = Binding {
            session_tag: own.binding.session_tag.0,
            epoch: own.binding.epoch,
        };
        let restarting = own.restarting;
        let now = self.now_ms()?;
        let until = now
            .checked_add(WARMUP_MS)
            .ok_or_else(|| failure(FailureKind::Policy, Fault::ClockOverflow))?;
        if restarting {
            policy(self.lifecycle.restart(binding, digest(), now, until))?;
        } else {
            policy(self.lifecycle.begin(binding, digest(), now, until))?;
        }
        let own = self.owned.as_mut().expect("owner");
        own.policy_started = true;
        Ok(())
    }
    fn request_deadline(&self) -> Result<Instant> {
        let policy_deadline = self
            .lifecycle
            .control_deadline_ms()
            .ok_or_else(|| failure(FailureKind::Policy, Fault::Protocol))?;
        Ok((Instant::now() + IO_WINDOW).min(self.clock + Duration::from_millis(policy_deadline)))
    }
    fn monitor(&mut self) -> Result<()> {
        let action = self.lifecycle.monitor(self.now_ms()?);
        if action != Action::None {
            return Err(failure(
                FailureKind::Worker,
                self.lifecycle.first_fault().unwrap_or(Fault::Protocol),
            ));
        }
        Ok(())
    }
    /// A single control request at a time; asynchronous Warmup events share its
    /// original absolute 400ms deadline. No event extends liveness or I/O time.
    pub fn poll(&mut self) -> Result<()> {
        if self.owned.is_none() {
            return Err(failure(FailureKind::Policy, Fault::Protocol));
        }
        if self.lifecycle.cleanup_pending() {
            return self.cleanup();
        }
        let result = self.poll_inner();
        if let Err(error) = result {
            return self.retire(error);
        }
        Ok(())
    }
    fn poll_inner(&mut self) -> Result<()> {
        self.monitor()?;
        let own = self.owned.as_ref().expect("owner");
        own.job.retain_lifetime_members().map_err(io_failure)?;
        if own
            .process
            .as_ref()
            .expect("launched")
            .wait(Duration::ZERO)
            .map_err(io_failure)?
            .is_some()
        {
            return Err(failure(FailureKind::Worker, Fault::Eof));
        }
        if Instant::now() < own.next_beat {
            return Ok(());
        }
        let heartbeat = self.request(WorkerCommand::Heartbeat)?;
        let deadline = self.request_deadline()?;
        self.send(&heartbeat, deadline)?;
        for _ in 0..MAX_EVENTS_PER_REQUEST {
            let own = self.owned.as_mut().expect("owner");
            let bytes = own
                .control
                .read_frame(CONTROL_MAX_BYTES, deadline)
                .map_err(io_failure)?;
            let response =
                decode_worker_response(&bytes, &own.binding).map_err(|code| Failure {
                    wire_code: Some(code),
                    ..failure(FailureKind::Protocol, Fault::Protocol)
                })?;
            self.monitor()?;
            let binding = self.binding().expect("active binding");
            match response.event {
                WorkerEvent::Heartbeat if response.request_id == heartbeat.request_id => {
                    let now = self.now_ms()?;
                    if policy(self.lifecycle.heartbeat(binding, now))? != Action::None {
                        return Err(failure(FailureKind::Worker, Fault::HeartbeatTimeout));
                    }
                    self.owned.as_mut().expect("owner").next_beat = Instant::now() + BEAT;
                    return Ok(());
                }
                WorkerEvent::Ready { capabilities } => {
                    let own = self.owned.as_ref().expect("owner");
                    if self.capabilities.is_some() || response.request_id != own.warmup_id {
                        return Err(failure(FailureKind::Protocol, Fault::Protocol));
                    }
                    validate_capabilities(&capabilities, &own.warmup_id)?;
                    if policy(
                        self.lifecycle
                            .warmup_ready(binding, digest(), self.now_ms()?),
                    )? != Action::None
                    {
                        return Err(failure(FailureKind::Worker, Fault::WarmupTimeout));
                    }
                    self.capabilities = Some(capabilities);
                }
                WorkerEvent::MemoryPressure
                    if response.request_id == self.owned.as_ref().expect("owner").warmup_id =>
                {
                    return Err(failure(FailureKind::Worker, Fault::MemoryPressure));
                }
                WorkerEvent::Failed { code }
                    if response.request_id == self.owned.as_ref().expect("owner").warmup_id =>
                {
                    return Err(Failure {
                        wire_code: Some(code),
                        ..failure(FailureKind::Worker, Fault::Protocol)
                    });
                }
                _ => return Err(failure(FailureKind::Protocol, Fault::Protocol)),
            }
        }
        Err(failure(FailureKind::Protocol, Fault::Protocol))
    }
    fn retire(&mut self, error: Failure) -> Result<()> {
        self.first_failure.get_or_insert(error);
        self.capabilities = None; // no resources or IPC are touched before invalidation
        // Startup failures also consume the same bounded policy retry budget,
        // but startup's independent deadline never renews a started Warmup.
        let started = self.start_policy();
        if let Some(binding) = self.binding() {
            let _ = self.lifecycle.worker_fault(binding, error.fault);
        }
        self.cleanup()?;
        started?;
        Err(error)
    }
    /// Cleanup is confirmed exclusively from retained native Job/process handles.
    /// On query/termination error the owner remains quarantined and cannot restart.
    pub fn cleanup(&mut self) -> Result<()> {
        let started = Instant::now();
        let deadline = started + CLEANUP_WINDOW;
        let mut observation = CleanupObservation::new(self.first_failure, self.stopped_ack);
        if self
            .owned
            .as_ref()
            .is_none_or(|own| own.policy_started && !self.lifecycle.cleanup_pending())
        {
            let error = observation.reject(failure(FailureKind::Policy, Fault::Protocol));
            remember_cleanup(
                &mut self.cleanup_snapshot,
                observation.finish(u64::try_from(started.elapsed().as_millis()).ok(), false),
            );
            return Err(error);
        }
        self.capabilities = None;
        let own = self
            .owned
            .as_mut()
            .ok_or_else(|| failure(FailureKind::Cleanup, Fault::Protocol))?;
        // Save live evidence before closing IPC can let a worker naturally exit.
        // A frozen capture error is returned by strict termination after Job kill.
        let _ = own.job.retain_lifetime_members();
        own.control.close();
        own.media.close();
        let clean = (|| {
            observation.terminate(
                own.job
                    .terminate_all_members_until(deadline)
                    .map_err(cleanup_io_failure),
            )?;
            observation.active(own.job.active_processes().map_err(cleanup_io_failure))?;
            observation.ids(own.job.process_ids().map_err(cleanup_io_failure))?;
            if let Some(process) = own.process.as_ref() {
                observation.wait_until(deadline, Instant::now(), |remaining| {
                    process.wait(remaining)
                })?;
            }
            if own.policy_started {
                let binding = self.lifecycle.binding().expect("cleanup binding");
                observation.confirm(self.lifecycle.cleanup_confirmed(binding, true, true))?;
            }
            Ok(())
        })();
        if let Some(snapshot) = own.job.lifetime_failure_snapshot() {
            self.member_failure_snapshot.get_or_insert(snapshot);
        }
        let elapsed = u64::try_from(started.elapsed().as_millis()).ok();
        remember_cleanup(
            &mut self.cleanup_snapshot,
            observation.finish(elapsed, clean.is_ok()),
        );
        if let Err(error) = clean {
            self.first_failure.get_or_insert(error);
            return Err(error);
        }
        self.last_cleaned_pid = self.worker_pid();
        self.owned = None;
        Ok(())
    }
    /// Stopped acknowledgement never substitutes for actual native cleanup.
    pub fn stop(&mut self) -> Result<()> {
        self.capabilities = None;
        self.lifecycle.stop();
        if self.owned.is_none() {
            return Ok(());
        }
        let stopped = (|| {
            self.owned
                .as_ref()
                .expect("owner")
                .job
                .retain_lifetime_members()
                .map_err(io_failure)?;
            let request = self.request(WorkerCommand::Stop)?;
            let deadline = Instant::now() + IO_WINDOW;
            self.send(&request, deadline)?;
            let own = self.owned.as_mut().expect("owner");
            let bytes = own
                .control
                .read_frame(CONTROL_MAX_BYTES, deadline)
                .map_err(io_failure)?;
            let response =
                decode_worker_response(&bytes, &own.binding).map_err(|code| Failure {
                    wire_code: Some(code),
                    ..failure(FailureKind::Protocol, Fault::Protocol)
                })?;
            if response.request_id != request.request_id
                || !matches!(response.event, WorkerEvent::Stopped)
            {
                return Err(failure(FailureKind::Protocol, Fault::Protocol));
            }
            self.stopped_ack = true;
            Ok(())
        })();
        if let Err(error) = stopped {
            self.first_failure.get_or_insert(error);
            self.cleanup()?;
            return Err(error);
        }
        self.cleanup()
    }
    pub fn explicit_user_reset(&mut self) -> Result<()> {
        policy(self.lifecycle.explicit_user_reset())?;
        self.first_failure = None;
        Ok(())
    }
    #[cfg(test)]
    pub fn test_member_failure_snapshot(&self) -> Option<MemberFailureSnapshot> {
        self.member_failure_snapshot.or_else(|| {
            self.owned
                .as_ref()
                .and_then(|own| own.job.lifetime_failure_snapshot())
        })
    }
    #[cfg(test)]
    pub fn test_owned_member_handles(&self) -> Result<Vec<Process>> {
        let own = self
            .owned
            .as_ref()
            .ok_or_else(|| failure(FailureKind::Policy, Fault::Protocol))?;
        own.job.retained_member_handles().map_err(io_failure)
    }
    #[cfg(test)]
    pub fn test_peer_identity(&self) -> Result<[u32; 2]> {
        use std::io::Read;
        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Identity {
            controller_pid: u32,
            leaf_pid: u32,
        }
        let own = self
            .owned
            .as_ref()
            .ok_or_else(|| failure(FailureKind::Policy, Fault::Protocol))?;
        if self.fixture.is_none() {
            return Err(failure(FailureKind::Policy, Fault::Protocol));
        }
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../.local/t011-supervisor-author")
            .join(format!("{}.members.json", own.control_tag));
        let file = std::fs::File::open(path).map_err(io_failure)?;
        let mut bytes = Vec::new();
        file.take(513).read_to_end(&mut bytes).map_err(io_failure)?;
        if bytes.len() > 512 {
            return Err(failure(FailureKind::Protocol, Fault::Protocol));
        }
        let identity: Identity = serde_json::from_slice(&bytes)
            .map_err(|_| failure(FailureKind::Protocol, Fault::Protocol))?;
        if Some(identity.controller_pid) != self.worker_pid()
            || identity.leaf_pid == 0
            || identity.leaf_pid == identity.controller_pid
        {
            return Err(failure(FailureKind::Protocol, Fault::Protocol));
        }
        Ok([identity.controller_pid, identity.leaf_pid])
    }
    #[cfg(test)]
    pub fn test_peer(session_tag: u64, mode: &'static str) -> Result<Self> {
        if ![
            "valid",
            "bad_request",
            "bad_binding",
            "bad_cap",
            "eof",
            "no_heartbeat",
            "partial",
            "late_old",
            "no_media",
            "bad_token",
            "delayed_shared",
            "memory_pressure",
            "ready_eof",
            "bad_media_token",
            "bad_budget",
            "bad_run",
        ]
        .contains(&mode)
        {
            return Err(failure(FailureKind::Configuration, Fault::Protocol));
        }
        let mut owner = Self::new(session_tag)?;
        owner.fixture = Some(mode);
        Ok(owner)
    }
}
impl Drop for WorkerSupervisor {
    fn drop(&mut self) {
        self.capabilities = None;
        self.lifecycle.stop();
        if let Some(own) = self.owned.as_mut() {
            own.control.close();
            own.media.close();
            // RAII closes KillOnClose as a final fallback; never claims cleanup.
            let _ = own.job.terminate();
        }
    }
}
