//! Pure Node-owned worker supervision policy; no process, IPC, model, or audio API.
//! The owner applies KillAndCleanup immediately to its real output gate and Job.
//! Ready matches a Node-provided expected asset/model-manifest digest only.
//! It is not proof that a real model has loaded or executed.
//! All times are monotonic milliseconds supplied by the Node control loop.

pub const HEARTBEAT_EXPIRY_MS: u64 = 500;
pub const MAX_AUTOMATIC_RESTARTS: u8 = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Binding {
    pub session_tag: u64,
    pub epoch: u32,
}

impl Binding {
    pub fn new(session_tag: u64, epoch: u32) -> Result<Self, Error> {
        if session_tag == 0 || epoch == 0 {
            return Err(Error::InvalidBinding);
        }
        Ok(Self { session_tag, epoch })
    }
}

/// Expected asset/model-manifest digest, supplied and validated by Node.
/// Matching this value alone is not a verified real-model capability.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WarmupProof(pub [u8; 32]);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Stopped,
    Warming,
    Ready,
    Cleaning,
    RetryPending,
    Stopping,
    FailedMuted,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fault {
    WarmupTimeout,
    HeartbeatTimeout,
    MediaTimeout,
    UnexpectedWarmupProof,
    UnexpectedMediaResult,
    MemoryPressure,
    Eof,
    Protocol,
    ClockRegression,
    ClockOverflow,
    SequenceExhausted,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CleanupReason {
    Stop,
    Fault(Fault),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    None,
    KillAndCleanup {
        binding: Binding,
        reason: CleanupReason,
    },
    /// Permission only. Node must advance its authoritative epoch and start
    /// a new owned process; the old process is already confirmed cleaned.
    RestartPermitted,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    InvalidBinding,
    WrongSession,
    StaleBinding,
    WrongPhase,
    InvalidDeadline,
    Busy,
    OutOfOrderSequence,
    CleanupIncomplete,
    EpochExhausted,
}

#[derive(Clone, Copy, Debug)]
struct Inflight {
    sequence: u32,
    deadline_ms: u64,
}

/// Bounded, allocation-free bookkeeping, independent of the canonical Session FSM.
/// There is at most one owned worker and one outstanding media task.
/// Cleanup confirmation must come from Node's process/lease owner, never worker
/// self-report. This policy does not kill a process or release a GPU itself.
pub struct WorkerLifecycle {
    session_tag: u64,
    binding: Option<Binding>,
    last_epoch: u32,
    phase: Phase,
    expected: WarmupProof,
    warmup_deadline_ms: u64,
    heartbeat_deadline_ms: u64,
    last_now_ms: u64,
    inflight: Option<Inflight>,
    last_sequence: Option<u32>,
    worker_owned: bool,
    output_allowed: bool,
    restarts: u8,
    first_fault: Option<Fault>,
    cleanup_reason: Option<CleanupReason>,
}

impl WorkerLifecycle {
    pub fn new(session_tag: u64) -> Result<Self, Error> {
        if session_tag == 0 {
            return Err(Error::InvalidBinding);
        }
        Ok(Self {
            session_tag,
            binding: None,
            last_epoch: 0,
            phase: Phase::Stopped,
            expected: WarmupProof([0; 32]),
            warmup_deadline_ms: 0,
            heartbeat_deadline_ms: 0,
            last_now_ms: 0,
            inflight: None,
            last_sequence: None,
            worker_owned: false,
            output_allowed: false,
            restarts: 0,
            first_fault: None,
            cleanup_reason: None,
        })
    }

    pub fn phase(&self) -> Phase {
        self.phase
    }

    pub fn binding(&self) -> Option<Binding> {
        self.binding
    }

    pub fn output_allowed(&self) -> bool {
        self.output_allowed && self.phase == Phase::Ready && !self.cleanup_pending()
    }

    pub fn resources_released(&self) -> bool {
        !self.worker_owned
    }

    pub fn cleanup_pending(&self) -> bool {
        self.cleanup_reason.is_some()
    }

    pub fn restart_count(&self) -> u8 {
        self.restarts
    }

    pub fn first_fault(&self) -> Option<Fault> {
        self.first_fault
    }

    pub fn media_deadline_ms(&self) -> Option<u64> {
        self.inflight.map(|task| task.deadline_ms)
    }

    /// The native control owner caps one I/O operation by the existing policy
    /// deadlines; no new event or I/O completion renews either deadline.
    pub fn control_deadline_ms(&self) -> Option<u64> {
        match self.phase {
            Phase::Warming => Some(self.heartbeat_deadline_ms.min(self.warmup_deadline_ms)),
            Phase::Ready => Some(self.heartbeat_deadline_ms),
            _ => None,
        }
    }

    /// Pending cleanup remains observable if a caller loses an earlier action.
    pub fn cleanup_action(&self) -> Action {
        match (self.binding, self.cleanup_reason) {
            (Some(binding), Some(reason)) => Action::KillAndCleanup { binding, reason },
            _ => Action::None,
        }
    }

    pub fn begin(
        &mut self,
        binding: Binding,
        expected: WarmupProof,
        now_ms: u64,
        warmup_deadline_ms: u64,
    ) -> Result<Action, Error> {
        if self.phase != Phase::Stopped || self.worker_owned {
            return Err(Error::WrongPhase);
        }
        self.start(binding, expected, now_ms, warmup_deadline_ms)?;
        self.restarts = 0;
        self.first_fault = None;
        Ok(Action::None)
    }

    pub fn restart(
        &mut self,
        binding: Binding,
        expected: WarmupProof,
        now_ms: u64,
        warmup_deadline_ms: u64,
    ) -> Result<Action, Error> {
        if self.phase != Phase::RetryPending || self.worker_owned {
            return Err(Error::WrongPhase);
        }
        if self.restarts >= MAX_AUTOMATIC_RESTARTS || self.last_epoch == u32::MAX {
            self.phase = Phase::FailedMuted;
            return Err(Error::EpochExhausted);
        }
        self.start(binding, expected, now_ms, warmup_deadline_ms)?;
        self.restarts += 1;
        Ok(Action::None)
    }

    fn start(
        &mut self,
        binding: Binding,
        expected: WarmupProof,
        now_ms: u64,
        warmup_deadline_ms: u64,
    ) -> Result<(), Error> {
        Binding::new(binding.session_tag, binding.epoch)?;
        if binding.session_tag != self.session_tag {
            return Err(Error::WrongSession);
        }
        if binding.epoch <= self.last_epoch {
            return Err(Error::StaleBinding);
        }
        let heartbeat_deadline_ms = now_ms
            .checked_add(HEARTBEAT_EXPIRY_MS)
            .ok_or(Error::InvalidDeadline)?;
        if now_ms < self.last_now_ms || warmup_deadline_ms <= now_ms {
            return Err(Error::InvalidDeadline);
        }
        self.binding = Some(binding);
        self.last_epoch = binding.epoch;
        self.expected = expected;
        self.last_now_ms = now_ms;
        self.warmup_deadline_ms = warmup_deadline_ms;
        self.heartbeat_deadline_ms = heartbeat_deadline_ms;
        self.inflight = None;
        self.last_sequence = None;
        self.worker_owned = true;
        self.cleanup_reason = None;
        self.output_allowed = false;
        self.phase = Phase::Warming;
        Ok(())
    }

    fn check_binding(&self, binding: Binding) -> Result<(), Error> {
        if self.binding != Some(binding) {
            return Err(Error::StaleBinding);
        }
        Ok(())
    }

    pub fn warmup_ready(
        &mut self,
        binding: Binding,
        proof: WarmupProof,
        now_ms: u64,
    ) -> Result<Action, Error> {
        self.check_binding(binding)?;
        if self.phase != Phase::Warming {
            return Err(Error::WrongPhase);
        }
        let action = self.monitor(now_ms);
        if action != Action::None {
            return Ok(action);
        }
        if proof != self.expected {
            return Ok(self.fail(Fault::UnexpectedWarmupProof));
        }
        self.phase = Phase::Ready;
        self.output_allowed = true;
        Ok(Action::None)
    }

    pub fn heartbeat(&mut self, binding: Binding, now_ms: u64) -> Result<Action, Error> {
        self.check_binding(binding)?;
        if !matches!(self.phase, Phase::Warming | Phase::Ready) {
            return Err(Error::WrongPhase);
        }
        let action = self.monitor(now_ms);
        if action != Action::None {
            return Ok(action);
        }
        match now_ms.checked_add(HEARTBEAT_EXPIRY_MS) {
            Some(deadline) => self.heartbeat_deadline_ms = deadline,
            None => return Ok(self.fail(Fault::ClockOverflow)),
        }
        Ok(Action::None)
    }

    pub fn submit_media(
        &mut self,
        binding: Binding,
        sequence: u32,
        now_ms: u64,
        deadline_ms: u64,
    ) -> Result<Action, Error> {
        self.check_binding(binding)?;
        if self.phase != Phase::Ready {
            return Err(Error::WrongPhase);
        }
        let action = self.monitor(now_ms);
        if action != Action::None {
            return Ok(action);
        }
        if self.inflight.is_some() {
            return Err(Error::Busy);
        }
        if self.last_sequence == Some(u32::MAX) {
            return Ok(self.fail(Fault::SequenceExhausted));
        }
        if self.last_sequence.is_some_and(|last| sequence <= last) {
            return Err(Error::OutOfOrderSequence);
        }
        if deadline_ms <= now_ms {
            return Err(Error::InvalidDeadline);
        }
        self.last_sequence = Some(sequence);
        self.inflight = Some(Inflight {
            sequence,
            deadline_ms,
        });
        Ok(Action::None)
    }

    pub fn media_completed(
        &mut self,
        binding: Binding,
        sequence: u32,
        now_ms: u64,
    ) -> Result<Action, Error> {
        self.check_binding(binding)?;
        if self.phase != Phase::Ready {
            return Err(Error::WrongPhase);
        }
        let action = self.monitor(now_ms);
        if action != Action::None {
            return Ok(action);
        }
        if self.inflight.is_none_or(|task| task.sequence != sequence) {
            return Ok(self.fail(Fault::UnexpectedMediaResult));
        }
        self.inflight = None;
        Ok(Action::None)
    }

    /// Control-loop watchdog. A heartbeat never touches the media deadline.
    /// Equality is expired; late results cannot restore permission.
    pub fn monitor(&mut self, now_ms: u64) -> Action {
        if !matches!(self.phase, Phase::Warming | Phase::Ready) {
            return Action::None;
        }
        if now_ms < self.last_now_ms {
            return self.fail(Fault::ClockRegression);
        }
        self.last_now_ms = now_ms;
        if self.phase == Phase::Warming && now_ms >= self.warmup_deadline_ms {
            return self.fail(Fault::WarmupTimeout);
        }
        if self.inflight.is_some_and(|task| now_ms >= task.deadline_ms) {
            return self.fail(Fault::MediaTimeout);
        }
        if now_ms >= self.heartbeat_deadline_ms {
            return self.fail(Fault::HeartbeatTimeout);
        }
        Action::None
    }

    pub fn worker_fault(&mut self, binding: Binding, fault: Fault) -> Result<Action, Error> {
        self.check_binding(binding)?;
        if !self.worker_owned {
            return Err(Error::WrongPhase);
        }
        if self.cleanup_pending() {
            return Ok(self.cleanup_action());
        }
        Ok(self.fail(fault))
    }

    fn fail(&mut self, fault: Fault) -> Action {
        self.output_allowed = false;
        self.inflight = None;
        self.first_fault.get_or_insert(fault);
        self.cleanup_reason = Some(CleanupReason::Fault(fault));
        self.phase = if self.restarts >= MAX_AUTOMATIC_RESTARTS {
            Phase::FailedMuted
        } else {
            Phase::Cleaning
        };
        self.cleanup_action()
    }

    pub fn stop(&mut self) -> Action {
        self.output_allowed = false;
        self.inflight = None;
        if !self.worker_owned {
            if self.phase != Phase::FailedMuted {
                self.phase = Phase::Stopped;
            }
            return Action::None;
        }
        if self.phase != Phase::FailedMuted {
            self.phase = Phase::Stopping;
        }
        self.cleanup_reason.get_or_insert(CleanupReason::Stop);
        self.cleanup_action()
    }

    /// Both facts must have been observed by the real Node process/lease owner.
    pub fn cleanup_confirmed(
        &mut self,
        binding: Binding,
        process_tree_exited: bool,
        leases_released: bool,
    ) -> Result<Action, Error> {
        self.check_binding(binding)?;
        if !self.worker_owned || !self.cleanup_pending() {
            return Err(Error::WrongPhase);
        }
        if !process_tree_exited || !leases_released {
            return Err(Error::CleanupIncomplete);
        }
        self.output_allowed = false;
        self.worker_owned = false;
        self.cleanup_reason = None;
        self.inflight = None;
        match self.phase {
            Phase::Stopping => self.phase = Phase::Stopped,
            Phase::FailedMuted => {}
            Phase::Cleaning => {
                if self.last_epoch == u32::MAX {
                    self.phase = Phase::FailedMuted;
                } else {
                    self.phase = Phase::RetryPending;
                    return Ok(Action::RestartPermitted);
                }
            }
            _ => return Err(Error::WrongPhase),
        }
        Ok(Action::None)
    }

    pub fn explicit_user_reset(&mut self) -> Result<(), Error> {
        if self.phase != Phase::FailedMuted {
            return Err(Error::WrongPhase);
        }
        if self.worker_owned || self.cleanup_pending() {
            return Err(Error::CleanupIncomplete);
        }
        if self.last_epoch == u32::MAX {
            return Err(Error::EpochExhausted);
        }
        self.phase = Phase::Stopped;
        self.restarts = 0;
        self.first_fault = None;
        Ok(())
    }
}
