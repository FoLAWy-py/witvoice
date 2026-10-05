//! Local worker wire contract. Control and PCM must use different authenticated channels.
use crate::{
    CONTROL_MAX_BYTES, ErrorCode, PROTOCOL_VERSION, control::PreparedCapabilities, values::*,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub const WORKER_HEADER_BYTES: usize = 40;
pub const WORKER_MAX_SAMPLES: u32 = 4096;
pub const WORKER_INPUT_RATE: u32 = 16000;
pub const WORKER_CHUNK_SAMPLES: u32 = 2560;
pub const WORKER_ENGINE_ID: &str = "meanvc2";
pub const WORKER_BACKEND_LABEL: &str = "cuda";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WorkerBinding {
    pub session_tag: DecimalU64,
    pub epoch: u32,
}
impl WorkerBinding {
    pub fn validate(&self) -> Result<(), ErrorCode> {
        if self.session_tag.0 == 0 || self.epoch == 0 {
            Err(ErrorCode::InvalidArgument)
        } else {
            Ok(())
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WorkerRequest {
    pub protocol_version: u16,
    pub request_id: Id,
    pub binding: WorkerBinding,
    pub command: WorkerCommand,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", content = "args", deny_unknown_fields)]
pub enum WorkerCommand {
    Warmup {
        model_sha256: Sha256,
        reference_id: Id,
        backend: WorkerBackend,
    },
    Heartbeat,
    Stop,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub enum WorkerBackend {
    Cuda,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WorkerResponse {
    pub protocol_version: u16,
    pub request_id: Id,
    pub binding: WorkerBinding,
    pub event: WorkerEvent,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", content = "args", deny_unknown_fields)]
pub enum WorkerEvent {
    Ready { capabilities: PreparedCapabilities },
    Heartbeat,
    Progress { sequence: u32 },
    MemoryPressure,
    Stopped,
    Failed { code: ErrorCode },
}
pub fn decode_worker_request(payload: &[u8]) -> Result<WorkerRequest, ErrorCode> {
    if payload.is_empty() || payload.len() > CONTROL_MAX_BYTES {
        return Err(ErrorCode::SizeLimit);
    }
    let request: WorkerRequest =
        serde_json::from_slice(payload).map_err(|_| ErrorCode::InvalidArgument)?;
    if request.protocol_version != PROTOCOL_VERSION {
        return Err(ErrorCode::UnsupportedVersion);
    }
    request.binding.validate()?;
    Ok(request)
}
pub fn decode_worker_response(
    payload: &[u8],
    expected: &WorkerBinding,
) -> Result<WorkerResponse, ErrorCode> {
    if payload.is_empty() || payload.len() > CONTROL_MAX_BYTES {
        return Err(ErrorCode::SizeLimit);
    }
    let response: WorkerResponse =
        serde_json::from_slice(payload).map_err(|_| ErrorCode::InvalidArgument)?;
    if response.protocol_version != PROTOCOL_VERSION {
        return Err(ErrorCode::UnsupportedVersion);
    }
    response.binding.validate()?;
    if response.binding.session_tag != expected.session_tag {
        return Err(ErrorCode::SessionMismatch);
    }
    if response.binding.epoch != expected.epoch {
        return Err(ErrorCode::EpochMismatch);
    }
    if let WorkerEvent::Ready { capabilities } = &response.event {
        capabilities.validate()?;
        if capabilities.engine_id != WORKER_ENGINE_ID
            || capabilities.backend != WORKER_BACKEND_LABEL
            || capabilities.native_input_rate != WORKER_INPUT_RATE
            || capabilities.native_output_rate != WORKER_INPUT_RATE
            || capabilities.chunk_samples != WORKER_CHUNK_SAMPLES
        {
            return Err(ErrorCode::EngineNotReady);
        }
    }
    Ok(response)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkerMediaKind {
    Source = 1,
    Converted = 2,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkerMediaHeader {
    pub kind: WorkerMediaKind,
    pub session_tag: u64,
    pub epoch: u32,
    pub sequence: u32,
    pub source_sample_index: u64,
    pub sample_rate: u32,
    pub sample_count: u32,
}
impl WorkerMediaHeader {
    fn validate(self) -> Result<(), ErrorCode> {
        if self.session_tag == 0
            || self.epoch == 0
            || self.sample_rate != WORKER_INPUT_RATE
            || self.sample_count == 0
            || self.sample_count > WORKER_MAX_SAMPLES
            || (self.kind == WorkerMediaKind::Source && self.sample_count != WORKER_CHUNK_SAMPLES)
            || self
                .source_sample_index
                .checked_add(u64::from(self.sample_count))
                .is_none()
        {
            return Err(ErrorCode::InvalidMedia);
        }
        Ok(())
    }
    pub fn encode(self) -> Result<[u8; WORKER_HEADER_BYTES], ErrorCode> {
        self.validate()?;
        let mut bytes = [0; WORKER_HEADER_BYTES];
        bytes[0] = 1;
        bytes[1] = self.kind as u8;
        bytes[4..12].copy_from_slice(&self.session_tag.to_be_bytes());
        bytes[12..16].copy_from_slice(&self.epoch.to_be_bytes());
        bytes[16..20].copy_from_slice(&self.sequence.to_be_bytes());
        bytes[20..28].copy_from_slice(&self.source_sample_index.to_be_bytes());
        bytes[28..32].copy_from_slice(&self.sample_rate.to_be_bytes());
        bytes[32..36].copy_from_slice(&self.sample_count.to_be_bytes());
        Ok(bytes)
    }
    /// Reject length before allocation. PCM is explicitly mono little-endian f32.
    pub fn decode(
        packet: &[u8],
        expected: &WorkerBinding,
        kind: WorkerMediaKind,
    ) -> Result<Self, ErrorCode> {
        if packet.len() < WORKER_HEADER_BYTES
            || packet.len() > WORKER_HEADER_BYTES + WORKER_MAX_SAMPLES as usize * 4
        {
            return Err(ErrorCode::SizeLimit);
        }
        if packet[0] != 1 {
            return Err(ErrorCode::UnsupportedVersion);
        }
        if packet[2..4] != [0, 0] || packet[36..40] != [0; 4] {
            return Err(ErrorCode::InvalidMedia);
        }
        let actual_kind = match packet[1] {
            1 => WorkerMediaKind::Source,
            2 => WorkerMediaKind::Converted,
            _ => return Err(ErrorCode::InvalidMedia),
        };
        let header = Self {
            kind: actual_kind,
            session_tag: u64::from_be_bytes(
                packet[4..12]
                    .try_into()
                    .map_err(|_| ErrorCode::InvalidMedia)?,
            ),
            epoch: u32::from_be_bytes(
                packet[12..16]
                    .try_into()
                    .map_err(|_| ErrorCode::InvalidMedia)?,
            ),
            sequence: u32::from_be_bytes(
                packet[16..20]
                    .try_into()
                    .map_err(|_| ErrorCode::InvalidMedia)?,
            ),
            source_sample_index: u64::from_be_bytes(
                packet[20..28]
                    .try_into()
                    .map_err(|_| ErrorCode::InvalidMedia)?,
            ),
            sample_rate: u32::from_be_bytes(
                packet[28..32]
                    .try_into()
                    .map_err(|_| ErrorCode::InvalidMedia)?,
            ),
            sample_count: u32::from_be_bytes(
                packet[32..36]
                    .try_into()
                    .map_err(|_| ErrorCode::InvalidMedia)?,
            ),
        };
        header.validate()?;
        if actual_kind != kind
            || packet.len() != WORKER_HEADER_BYTES + header.sample_count as usize * 4
        {
            return Err(ErrorCode::InvalidMedia);
        }
        if header.session_tag != expected.session_tag.0 {
            return Err(ErrorCode::SessionMismatch);
        }
        if header.epoch != expected.epoch {
            return Err(ErrorCode::EpochMismatch);
        }
        for sample in packet[WORKER_HEADER_BYTES..].chunks_exact(4) {
            if !f32::from_le_bytes(sample.try_into().map_err(|_| ErrorCode::InvalidMedia)?)
                .is_finite()
            {
                return Err(ErrorCode::InvalidMedia);
            }
        }
        Ok(header)
    }
}
