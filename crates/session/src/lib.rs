//! Owning control lifecycle. Resource preparation is intentionally unavailable.
#![forbid(unsafe_code)]

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
}

impl Default for Runtime {
    fn default() -> Self {
        Self {
            state: SessionState::Idle,
            state_version: 0,
            epoch: 0,
            history: BTreeMap::new(),
        }
    }
}

impl Runtime {
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
        true
    }

    fn apply(&mut self, event: Event) -> Result<(), ErrorCode> {
        let change = transition(self.state, event)?;
        if change.requires_ready_resources {
            return Err(ErrorCode::EngineNotReady);
        }
        let epoch = if change.advance_epoch {
            next_epoch(self.epoch)?
        } else {
            self.epoch
        };
        let version = self.state_version.checked_add(1).ok_or(ErrorCode::Busy)?;
        self.epoch = epoch;
        self.state_version = version;
        self.state = change.next;
        Ok(())
    }

    fn execute(&mut self, command: &Command) -> Outcome {
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
            if matches!(request.command, Command::StopSession { .. }) {
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

fn error(code: ErrorCode) -> Outcome {
    Outcome::Error {
        code,
        message: format!("{code:?}"),
    }
}
