use crate::{CONTROL_MAX_BYTES, ErrorCode, PROTOCOL_VERSION, state::SessionState, values::*};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub protocol_version: u16,
    pub request_id: Id,
    pub command: Command,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", content = "args", deny_unknown_fields)]
pub enum Command {
    /// Authenticated local exit; never available in the peer protocol.
    /// The Node invalidates output and releases all owned resources before exit.
    ExitNode,
    ListDevices,
    GetCapabilities,
    GetState,
    CreateVoice {
        display_name: String,
        reference_handle: String,
        consent_confirmed: bool,
    },
    DeleteVoice {
        voice_id: Id,
    },
    PreviewVoice {
        voice_id: Id,
        source_handle: String,
    },
    PrepareSession {
        input_device_id: String,
        output_device_id: String,
        voice_id: Id,
        route: Route,
    },
    StartSession {
        session_id: Id,
        epoch: u32,
    },
    StopSession {
        session_id: Id,
    },
    SetMute {
        session_id: Id,
        epoch: u32,
        muted: bool,
    },
    SelectRoute {
        route: Route,
    },
    SetMonitor {
        enabled: bool,
        device_id: Option<String>,
    },
    SetGain {
        db: f32,
    },
    ListPeers,
    ExportIdentity,
    ImportPeerIdentity {
        identity: String,
        allow_compute: bool,
        allow_profile_transfer: bool,
    },
    RevokePeer {
        node_id: Id,
    },
    InstallEngineAssets {
        manifest_id: String,
        download_approved: bool,
    },
    GetMetrics,
    ExportDiagnostics {
        destination_handle: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", content = "args", deny_unknown_fields)]
pub enum Route {
    Local,
    Remote { node_id: Id },
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Response {
    pub request_id: Id,
    pub state_version: DecimalU64,
    pub outcome: Outcome,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", content = "args", deny_unknown_fields)]
pub enum Outcome {
    Ack,
    State { state: SessionState },
    Error { code: ErrorCode, message: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PreparedCapabilities {
    pub engine_id: String,
    pub model_sha256: Sha256,
    pub backend: String,
    pub native_input_rate: u32,
    pub native_output_rate: u32,
    pub chunk_samples: u32,
    pub lookahead_samples: u32,
    pub conditioning_schema: String,
    pub duration_preserving: bool,
    pub capability_test_run_id: String,
    pub model_memory_budget_bytes: DecimalU64,
    pub device_memory_budget_bytes: Option<DecimalU64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct QueueBudget {
    pub capacity_frames: u32,
    pub target_frames: u32,
    pub maximum_age_ms: u32,
    pub overflow_policy: OverflowPolicy,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub enum OverflowPolicy {
    RejectNewest,
    DropExpiredOldest,
}
impl QueueBudget {
    pub fn validate(&self) -> Result<(), ErrorCode> {
        if self.capacity_frames == 0
            || self.target_frames > self.capacity_frames
            || self.maximum_age_ms == 0
        {
            return Err(ErrorCode::InvalidArgument);
        }
        Ok(())
    }
}

impl PreparedCapabilities {
    pub fn validate(&self) -> Result<(), ErrorCode> {
        if self.engine_id.is_empty()
            || self.backend.is_empty()
            || self.conditioning_schema.is_empty()
            || self.capability_test_run_id.is_empty()
            || self.native_input_rate == 0
            || self.native_output_rate == 0
            || self.chunk_samples == 0
        {
            return Err(ErrorCode::EngineNotReady);
        }
        Ok(())
    }
}

/// LAN messages cannot deserialize as local commands that open capture devices.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PeerRequest {
    pub protocol_version: u16,
    pub request_id: Id,
    pub context: Option<SessionContext>,
    pub message: PeerMessage,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SessionContext {
    pub session_id: Id,
    pub epoch: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", content = "args", deny_unknown_fields)]
pub enum PeerMessage {
    Hello {
        node_id: Id,
    },
    Capabilities,
    PrepareSession {
        model_sha256: Sha256,
        profile_sha256: Sha256,
    },
    Prepared {
        capabilities: PreparedCapabilities,
        loaded_profile_sha256: Sha256,
        session_tag: DecimalU64,
    },
    RejectBusy,
    AuthorizeProfileTransfer {
        profile_sha256: Sha256,
        total_bytes: u32,
    },
    ProfileChunk {
        offset: u32,
        data_base64: String,
    },
    ProfileReady {
        profile_sha256: Sha256,
    },
    Start,
    Started,
    Mute,
    Muted,
    Reset {
        new_epoch: u32,
    },
    ResetReady,
    Stop,
    Stopped,
    Ping,
    Pong,
    Metrics,
    Error {
        code: ErrorCode,
        message: String,
    },
}
impl PeerRequest {
    pub fn validate(&self) -> Result<(), ErrorCode> {
        if self.protocol_version != PROTOCOL_VERSION {
            return Err(ErrorCode::UnsupportedVersion);
        }
        let pre_session = matches!(
            self.message,
            PeerMessage::Hello { .. }
                | PeerMessage::Capabilities
                | PeerMessage::PrepareSession { .. }
                | PeerMessage::RejectBusy
                // Errors may reject Hello/Prepare before a session exists.
                // Transport must still correlate request_id and authenticate the peer.
                | PeerMessage::Error { .. }
        );
        if !pre_session && self.context.is_none() {
            return Err(ErrorCode::SessionMismatch);
        }
        if let PeerMessage::AuthorizeProfileTransfer { total_bytes, .. } = self.message
            && total_bytes as usize > crate::PROFILE_MAX_BYTES
        {
            return Err(ErrorCode::SizeLimit);
        }
        Ok(())
    }
}

impl PeerMessage {
    /// Ready requires the peer's actually loaded assets to match our preparation.
    /// This contract check does not itself authenticate a remote claim.
    pub fn validate_prepared(
        &self,
        expected_model: &Sha256,
        expected_profile: &Sha256,
    ) -> Result<(), ErrorCode> {
        let Self::Prepared {
            capabilities,
            loaded_profile_sha256,
            session_tag,
        } = self
        else {
            return Err(ErrorCode::InvalidArgument);
        };
        capabilities.validate()?;
        if &capabilities.model_sha256 != expected_model
            || loaded_profile_sha256 != expected_profile
            || session_tag.0 == 0
        {
            return Err(ErrorCode::EngineNotReady);
        }
        Ok(())
    }
}

pub fn decode_request(payload: &[u8]) -> Result<Request, ErrorCode> {
    if payload.is_empty() || payload.len() > CONTROL_MAX_BYTES {
        return Err(ErrorCode::SizeLimit);
    }
    let request: Request =
        serde_json::from_slice(payload).map_err(|_| ErrorCode::InvalidArgument)?;
    if request.protocol_version != PROTOCOL_VERSION {
        return Err(ErrorCode::UnsupportedVersion);
    }
    if let Command::SetGain { db } = request.command
        && (!db.is_finite() || !(-60.0..=12.0).contains(&db))
    {
        return Err(ErrorCode::InvalidArgument);
    }
    Ok(request)
}

/// Must be called on four header bytes before allocating or reading a payload.
pub fn control_length(header: [u8; 4]) -> Result<usize, ErrorCode> {
    let length = u32::from_be_bytes(header) as usize;
    if length == 0 || length > CONTROL_MAX_BYTES {
        Err(ErrorCode::SizeLimit)
    } else {
        Ok(length)
    }
}
