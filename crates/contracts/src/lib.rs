//! Authoritative wire types and pure validation; no devices, network or models.
#![forbid(unsafe_code)]
pub mod control;
pub mod media;
pub mod state;
pub mod values;

pub const PROTOCOL_VERSION: u16 = 1;
pub const CONTROL_MAX_BYTES: usize = 65_536;
pub const PROFILE_MAX_BYTES: usize = 50 * 1024 * 1024;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCode {
    InvalidArgument,
    UnsupportedVersion,
    InvalidState,
    RequestIdConflict,
    Busy,
    UntrustedPeer,
    CertificateMismatch,
    PermissionDenied,
    DeviceMissing,
    EngineNotReady,
    EngineFailed,
    InvalidProfile,
    SizeLimit,
    SessionMismatch,
    EpochMismatch,
    InvalidMedia,
    Expired,
    Backpressure,
    EpochExhausted,
}
