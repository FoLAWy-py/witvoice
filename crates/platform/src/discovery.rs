//! Explicit, interface-scoped OS DNS-SD. Creating the adapter performs no OS calls.
//! App-owned memory is bounded; Windows DNS cache/temporary allocations are not.
pub const SERVICE_TYPE: &str = "_voice-node._udp.local";
pub const QUEUE_LIMIT: usize = 32;
pub const CONTEXT_LIMIT: usize = 16;
pub const RESOLVE_LIMIT: usize = 8;
// Closed scalar-only diagnostics: no record text, addresses, context pointers or identities.
macro_rules! diagnostic_enum {
    ($name:ident { $($variant:ident = $code:literal),+ $(,)? }) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        #[repr(u8)]
        pub enum $name { $($variant = $code),+ }
        impl $name {
            #[cfg(windows)]
            fn from_code(code: u8) -> Option<Self> {
                match code { $($code => Some(Self::$variant),)+ _ => None }
            }
        }
    };
}
diagnostic_enum!(FailureStage { Browse = 1, Resolve = 2, Register = 3, Deregister = 4 });
diagnostic_enum!(FailureOrigin { OsCallback = 1, LocalValidation = 2, Panic = 3 });
diagnostic_enum!(TextFailure { NullPointer = 1, Length = 2, InvalidUtf16 = 3 });
diagnostic_enum!(ValidationReason {
    RecordCycle = 1, RecordLength = 2, RecordOwnerText = 3, RecordOwner = 4,
    RecordTargetText = 5, RecordTargetName = 6, RecordBudget = 7,
    NullInstance = 8, InterfaceMismatch = 9, PropertyCount = 10,
    NullKeys = 11, NullValues = 12, ZeroPort = 13, InstanceNameText = 14,
    InstanceName = 15, ExpectedNameText = 16, ExpectedName = 17,
    QueryMismatch = 18, PropertyKeyText = 19, PropertyValueText = 20,
    DuplicateNodeId = 21, DuplicateProtocol = 22, UnknownProperty = 23,
    ProtocolVersion = 24, MissingNodeId = 25, NodeId = 26,
    MissingAddress = 27, Expired = 28,
});
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FailureSnapshot {
    pub stage: FailureStage,
    pub origin: FailureOrigin,
    /// Actual callback input; None means unknown, never a fabricated Win32 code.
    pub callback_status: Option<u32>,
    /// Compatibility Event::Failed code; local validation/panic retains 13.
    pub reported_status: u32,
    pub reason: Option<ValidationReason>,
    pub text_error: Option<TextFailure>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiscoveryError {
    ApprovalRequired,
    InterfaceRequired,
    InvalidInput,
    Busy,
    Capacity,
    Native(u32),
    Unsupported,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    Found {
        full_name: String,
        expires_at: std::time::Instant,
    },
    Removed {
        full_name: String,
    },
    Resolved {
        instance: String,
        node_id: witvoice_contracts::values::Id,
        address: std::net::SocketAddr,
        expires_at: std::time::Instant,
    },
    Registered,
    Stopped,
    Failed(u32),
}
/// Exact service suffix, ASCII single label; never an arbitrary DNS query.
pub fn validate_full_name(name: &str) -> Result<&str, DiscoveryError> {
    let name = name.strip_suffix('.').unwrap_or(name);
    let label = name
        .strip_suffix("._voice-node._udp.local")
        .ok_or(DiscoveryError::InvalidInput)?;
    if label.is_empty()
        || label.len() > 63
        || !label
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err(DiscoveryError::InvalidInput);
    }
    Ok(label)
}
#[cfg(windows)]
mod native;
#[cfg(windows)]
pub use native::NativeDiscovery;
#[cfg(not(windows))]
#[derive(Default)]
pub struct NativeDiscovery;
#[cfg(not(windows))]
impl NativeDiscovery {
    pub fn first_failure(&self) -> Option<FailureSnapshot> {
        None
    }
    pub fn browse(&mut self, _: bool, _: u32) -> Result<(), DiscoveryError> {
        Err(DiscoveryError::Unsupported)
    }
}
