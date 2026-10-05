//! Owning control lifecycle. Resource preparation is intentionally unavailable.
#![forbid(unsafe_code)]

#[cfg(all(feature = "test-support", not(debug_assertions)))]
compile_error!("test-support is forbidden in release builds");
#[cfg(feature = "test-support")]
pub mod test_support;

use std::collections::BTreeMap;
use witvoice_contracts::{
    ErrorCode,
    control::{Command, Outcome, Request, Response, decode_request},
    state::{Event, SessionState, next_epoch, transition},
    values::DecimalU64,
};

pub const REQUEST_HISTORY_CAPACITY: usize = 128;

struct Recorded {
    canonical: Vec<u8>,
    response: Response,
}

/// Single Node owner; UI connections do not own or destroy this object.
/// No engine/device capability is fabricated by this control-only slice.
pub struct Runtime {
    state: SessionState,
    state_version: u64,
    epoch: u32,
    history: BTreeMap<String, Recorded>,
    shutdown_requested: bool,
    resource_cleanup_requested: bool,
    #[cfg(feature = "test-support")]
    fixture: Option<Box<dyn test_support::Lifecycle>>,
    #[cfg(feature = "test-support")]
    fixture_identity: Option<(witvoice_contracts::values::Id, u64)>,
    #[cfg(feature = "test-support")]
    binding: Option<test_support::Binding>,
}

impl Default for Runtime {
    fn default() -> Self {
        Self {
            state: SessionState::Idle,
            state_version: 0,
            epoch: 0,
            history: BTreeMap::new(),
            shutdown_requested: false,
            resource_cleanup_requested: false,
            #[cfg(feature = "test-support")]
            fixture: None,
            #[cfg(feature = "test-support")]
            fixture_identity: None,
            #[cfg(feature = "test-support")]
            binding: None,
        }
    }
}

impl Runtime {
    /// Set only after wire validation and request-id conflict checks.
    pub fn shutdown_requested(&self) -> bool {
        self.shutdown_requested
    }

    pub fn take_resource_cleanup(&mut self) -> bool {
        std::mem::take(&mut self.resource_cleanup_requested)
    }

    fn request_shutdown(&mut self) -> Outcome {
        self.retire(false);
        // Irreversible owner cleanup does not depend on counter availability.
        // This slice never owns playable output; a retiring epoch is never reused.
        let already_retired = self.shutdown_requested;
        self.shutdown_requested = true;
        self.resource_cleanup_requested = true;
        if already_retired {
            return self.quiescent_ack();
        }
        match self
            .apply(Event::Stop)
            .and_then(|()| self.apply(Event::Stopped))
        {
            Ok(()) => self.quiescent_ack(),
            Err(code) => error(code),
        }
    }
    pub fn state(&self) -> SessionState {
        self.state
    }

    pub fn epoch(&self) -> u32 {
        self.epoch
    }

    pub fn state_version(&self) -> u64 {
        self.state_version
    }

    /// No audio resources exist in this slice. It cannot authorize audio output.
    pub fn output_is_muted(&self) -> bool {
        #[cfg(feature = "test-support")]
        if self.fixture.is_some() {
            return !matches!(self.state, SessionState::Running | SessionState::Degraded);
        }
        true
    }

    fn retire(&mut self, fault: bool) {
        #[cfg(feature = "test-support")]
        if let Some(resources) = self.fixture.as_mut() {
            resources.invalidate_first(if fault {
                test_support::Retirement::Fault
            } else {
                test_support::Retirement::Planned
            });
        }
        #[cfg(not(feature = "test-support"))]
        let _ = fault;
    }

    fn quiescent_ack(&self) -> Outcome {
        #[cfg(feature = "test-support")]
        if self
            .fixture
            .as_ref()
            .is_some_and(|resources| !resources.ack_ready())
        {
            return error(ErrorCode::Busy);
        }
        Outcome::Ack
    }

    fn apply(&mut self, event: Event) -> Result<(), ErrorCode> {
        let change = transition(self.state, event)?;
        // Prepared keeps the NEW gate muted but armable; retiring it is irreversible.
        if change.invalidate_output && event != Event::Prepared {
            self.retire(matches!(event, Event::Fail | Event::Invalidate));
        }
        let checked_epoch = if change.advance_epoch {
            next_epoch(self.epoch)
        } else {
            Ok(self.epoch)
        };
        let epoch = match checked_epoch {
            Ok(epoch) => epoch,
            Err(code) => {
                self.retire(true);
                self.resource_cleanup_requested = true;
                return Err(code);
            }
        };
        let Some(version) = self.state_version.checked_add(1) else {
            self.retire(true);
            self.resource_cleanup_requested = true;
            return Err(ErrorCode::Busy);
        };
        if change.requires_ready_resources {
            #[cfg(feature = "test-support")]
            {
                let ready = self
                    .binding
                    .as_ref()
                    .filter(|binding| binding.epoch == epoch)
                    .zip(self.fixture.as_ref())
                    .is_some_and(|(binding, resources)| resources.ready(binding));
                if !ready {
                    return Err(ErrorCode::EngineNotReady);
                }
                if matches!(event, Event::Start | Event::Unmute) {
                    let result = self
                        .fixture
                        .as_mut()
                        .unwrap()
                        .activate(self.binding.as_ref().unwrap());
                    if let Err(code) = result {
                        self.retire(true);
                        self.resource_cleanup_requested = true;
                        return Err(code);
                    }
                }
            }
            #[cfg(not(feature = "test-support"))]
            return Err(ErrorCode::EngineNotReady);
        }
        self.epoch = epoch;
        self.state_version = version;
        self.state = change.next;
        Ok(())
    }

    fn execute(&mut self, command: &Command) -> Outcome {
        #[cfg(feature = "test-support")]
        if self.fixture.is_some() {
            return self.execute_fixture(command);
        }
        match command {
            Command::GetState => Outcome::State { state: self.state },
            Command::PrepareSession { .. } => {
                let result = self
                    .apply(Event::Prepare)
                    .and_then(|()| self.apply(Event::Block));
                match result {
                    Ok(()) => error(ErrorCode::EngineNotReady),
                    Err(code) => error(code),
                }
            }
            Command::StartSession { .. } | Command::SetMute { .. } => {
                // No prepared session or capture authorization exists.
                error(ErrorCode::EngineNotReady)
            }
            Command::StopSession { .. } => {
                self.resource_cleanup_requested = true;
                // There are no live session resources in this slice. An already
                // idle Stop is harmless and does not advance epoch repeatedly.
                if self.state == SessionState::Idle {
                    Outcome::Ack
                } else {
                    match self
                        .apply(Event::Stop)
                        .and_then(|()| self.apply(Event::Stopped))
                    {
                        Ok(()) => Outcome::Ack,
                        Err(code) => error(code),
                    }
                }
            }
            _ => error(ErrorCode::EngineNotReady),
        }
    }

    pub fn handle(&mut self, payload: &[u8]) -> Result<Response, ErrorCode> {
        let request = decode_request(payload)?;
        let canonical = serde_json::to_vec(&request).map_err(|_| ErrorCode::InvalidArgument)?;
        let key: String = request.request_id.clone().into();
        if let Some(previous) = self.history.get(&key) {
            return Ok(if previous.canonical == canonical {
                previous.response.clone()
            } else {
                self.response(request, error(ErrorCode::RequestIdConflict))
            });
        }
        if matches!(request.command, Command::ExitNode) {
            let outcome = self.request_shutdown();
            let response = self.response(request, outcome);
            // The accepted exit itself is authoritative even when history is full.
            if self.history.len() < REQUEST_HISTORY_CAPACITY {
                self.history.insert(
                    key,
                    Recorded {
                        canonical,
                        response: response.clone(),
                    },
                );
            }
            return Ok(response);
        }
        if self.shutdown_requested {
            return Ok(self.response(request, error(ErrorCode::Busy)));
        }
        if matches!(
            request.command,
            Command::GetState
                | Command::GetCapabilities
                | Command::GetMetrics
                | Command::ListDevices
                | Command::ListPeers
        ) {
            // Read-only polling must not consume the bounded mutation history.
            let outcome = self.execute(&request.command);
            return Ok(self.response(request, outcome));
        }
        if self.history.len() == REQUEST_HISTORY_CAPACITY {
            // Never evict a mutating request and accidentally execute its replay.
            // Emergency Stop still silences/tears down; once full, the owner must
            // be restarted before further control mutation can be accepted.
            if matches!(
                request.command,
                Command::StopSession { .. } | Command::SetMute { muted: true, .. }
            ) {
                self.execute(&request.command);
            }
            return Ok(self.response(request, error(ErrorCode::Backpressure)));
        }
        let outcome = self.execute(&request.command);
        let response = self.response(request, outcome);
        self.history.insert(
            key,
            Recorded {
                canonical,
                response: response.clone(),
            },
        );
        Ok(response)
    }

    fn response(&self, request: Request, outcome: Outcome) -> Response {
        Response {
            request_id: request.request_id,
            state_version: DecimalU64(self.state_version),
            outcome,
        }
    }
}

#[cfg(feature = "test-support")]
impl Runtime {
    /// Explicit trusted debug harness only; default Node construction never calls this.
    pub fn with_test_support(
        session_id: witvoice_contracts::values::Id,
        session_tag: u64,
        resources: Box<dyn test_support::Lifecycle>,
    ) -> Result<Self, ErrorCode> {
        if session_tag == 0 {
            return Err(ErrorCode::InvalidArgument);
        }
        let mut runtime = Self::default();
        runtime.fixture = Some(resources);
        runtime.fixture_identity = Some((session_id, session_tag));
        Ok(runtime)
    }
    pub fn test_binding(&self) -> Option<&test_support::Binding> {
        self.binding.as_ref()
    }

    /// Trusted worker supervisor notification, never an IPC command or independent state machine.
    pub fn test_worker_failed(&mut self, binding: &test_support::Binding) -> Result<(), ErrorCode> {
        self.check_binding(&binding.session_id, Some(binding.epoch))?;
        if self.binding.as_ref() != Some(binding) {
            return Err(ErrorCode::SessionMismatch);
        }
        self.retire(true);
        self.resource_cleanup_requested = true;
        self.apply(Event::Invalidate)
    }
    fn check_binding(
        &self,
        session_id: &witvoice_contracts::values::Id,
        epoch: Option<u32>,
    ) -> Result<(), ErrorCode> {
        let Some(binding) = self.binding.as_ref() else {
            return Err(ErrorCode::EngineNotReady);
        };
        if &binding.session_id != session_id {
            return Err(ErrorCode::SessionMismatch);
        }
        if epoch.is_some_and(|epoch| epoch != self.epoch) {
            return Err(ErrorCode::EpochMismatch);
        }
        Ok(())
    }
    fn prepare_fixture_epoch(&mut self, epoch: u32) -> Result<(), ErrorCode> {
        let (session_id, session_tag) = self
            .fixture_identity
            .as_ref()
            .ok_or(ErrorCode::EngineNotReady)?;
        let binding = test_support::Binding {
            session_id: session_id.clone(),
            session_tag: *session_tag,
            epoch,
        };
        self.fixture
            .as_mut()
            .ok_or(ErrorCode::EngineNotReady)?
            .prepare(&binding)?;
        self.binding = Some(binding);
        Ok(())
    }
    fn fixture_command(&mut self, command: &Command) -> Result<Outcome, ErrorCode> {
        match command {
            Command::GetState => Ok(Outcome::State { state: self.state }),
            Command::PrepareSession { .. } => {
                let event = if self.state == SessionState::Idle {
                    Event::Prepare
                } else {
                    Event::Reset
                };
                self.apply(event)?;
                if let Err(code) = self.prepare_fixture_epoch(self.epoch) {
                    self.retire(true);
                    self.resource_cleanup_requested = true;
                    let _ = self.apply(Event::Block);
                    return Err(code);
                }
                self.apply(Event::Prepared)?;
                Ok(Outcome::Ack)
            }
            Command::StartSession { session_id, epoch } => {
                self.check_binding(session_id, Some(*epoch))?;
                self.apply(Event::Start)?;
                Ok(Outcome::Ack)
            }
            Command::SetMute {
                session_id,
                epoch,
                muted,
            } => {
                self.check_binding(session_id, Some(*epoch))?;
                if *muted {
                    if self.state != SessionState::Muted {
                        self.apply(Event::Mute)?;
                    }
                    Ok(self.quiescent_ack())
                } else {
                    transition(self.state, Event::Unmute)?;
                    let epoch = next_epoch(self.epoch)?;
                    self.prepare_fixture_epoch(epoch)?;
                    self.apply(Event::Unmute)?;
                    Ok(Outcome::Ack)
                }
            }
            Command::StopSession { session_id } => {
                self.check_binding(session_id, None)?;
                self.retire(false);
                self.resource_cleanup_requested = true;
                if self.state != SessionState::Idle {
                    self.apply(Event::Stop)?;
                    self.apply(Event::Stopped)?;
                }
                Ok(self.quiescent_ack())
            }
            _ => Err(ErrorCode::EngineNotReady),
        }
    }
    fn execute_fixture(&mut self, command: &Command) -> Outcome {
        match self.fixture_command(command) {
            Ok(outcome) => outcome,
            Err(code) => error(code),
        }
    }
}

impl Drop for Runtime {
    fn drop(&mut self) {
        self.retire(false);
    }
}

fn error(code: ErrorCode) -> Outcome {
    Outcome::Error {
        code,
        message: format!("{code:?}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exhausted_counters_cannot_prevent_retirement_or_resource_cleanup() {
        for (epoch, version) in [(u32::MAX, 0), (1, u64::MAX)] {
            let mut runtime = Runtime::default();
            runtime.state = SessionState::Blocked;
            runtime.epoch = epoch;
            runtime.state_version = version;
            let response=runtime.handle(br#"{"protocol_version":1,"request_id":"00000000-0000-0000-0000-000000000001","command":{"kind":"ExitNode"}}"#).unwrap();
            assert!(matches!(response.outcome, Outcome::Error { .. }));
            assert!(runtime.shutdown_requested());
            assert!(runtime.take_resource_cleanup());
            assert!(runtime.output_is_muted());
            assert_eq!(runtime.epoch(), epoch);
            assert_eq!(runtime.state_version(), version);
        }
    }

    #[cfg(feature = "test-support")]
    #[test]
    fn exhausted_counters_retire_actual_gate_and_pending_exit_never_false_acks() {
        use std::sync::Arc;
        use test_support::*;
        use witvoice_audio::realtime::{
            Binding as AudioBinding, OutputGate, QueueConfig, Ring, processed_endpoints,
        };
        struct Hooks(Arc<OutputGate>);
        impl Lifecycle for Hooks {
            fn prepare(&mut self, _: &Binding) -> Result<(), ErrorCode> {
                Err(ErrorCode::EngineNotReady)
            }
            fn ready(&self, _: &Binding) -> bool {
                true
            }
            fn activate(&mut self, _: &Binding) -> Result<(), ErrorCode> {
                Ok(())
            }
            fn invalidate_first(&mut self, _: Retirement) {
                self.0.invalidate();
            }
            fn ack_ready(&self) -> bool {
                self.0.ack_ready()
            }
        }
        for (epoch, version) in [(u32::MAX, 0), (1, u64::MAX)] {
            let mut gate = OutputGate::new(AudioBinding::new(17, epoch).unwrap()).unwrap();
            gate.arm().unwrap();
            let gate = Arc::new(gate);
            let mut runtime = Runtime::with_test_support(
                "00000000-0000-0000-0000-000000000abc"
                    .to_owned()
                    .try_into()
                    .unwrap(),
                17,
                Box::new(Hooks(gate.clone())),
            )
            .unwrap();
            runtime.state = SessionState::Running;
            runtime.epoch = epoch;
            runtime.state_version = version;
            let ticket = gate.begin_commit().unwrap();
            let payload=br#"{"protocol_version":1,"request_id":"00000000-0000-0000-0000-000000000001","command":{"kind":"ExitNode"}}"#;
            assert!(matches!(
                runtime.handle(payload).unwrap().outcome,
                Outcome::Error { .. }
            ));
            assert!(!gate.is_live());
            assert!(!gate.ack_ready());
            assert!(!ticket.is_live());
            let other=br#"{"protocol_version":1,"request_id":"00000000-0000-0000-0000-000000000002","command":{"kind":"ExitNode"}}"#;
            assert!(matches!(
                runtime.handle(other).unwrap().outcome,
                Outcome::Error {
                    code: ErrorCode::Busy,
                    ..
                }
            ));
            let mut ring = Ring::new(8).unwrap();
            let (_, mut sink) =
                processed_endpoints(&mut ring, &gate, QueueConfig::output()).unwrap();
            let mut samples = [99.0; 480];
            assert!(sink.render(&mut samples, 1000).is_err());
            assert!(samples.iter().all(|sample| *sample == 0.0));
            drop(ticket);
            assert!(gate.ack_ready());
            assert!(runtime.shutdown_requested());
            assert!(runtime.take_resource_cleanup());
        }
    }
}
