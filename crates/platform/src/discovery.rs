//! Explicit, interface-scoped OS DNS-SD. Creating the adapter performs no OS calls.
//! App-owned memory is bounded; Windows DNS cache/temporary allocations are not.
pub const SERVICE_TYPE: &str = "_voice-node._udp.local";
pub const QUEUE_LIMIT: usize = 32;
pub const CONTEXT_LIMIT: usize = 16;
pub const RESOLVE_LIMIT: usize = 8;
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
    pub fn browse(&mut self, _: bool, _: u32) -> Result<(), DiscoveryError> {
        Err(DiscoveryError::Unsupported)
    }
}
