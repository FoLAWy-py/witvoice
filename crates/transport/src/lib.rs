//! Identity, explicit trust and bounded addressing. Construction never opens a socket.
pub mod discovery;
pub mod identity;
pub mod trust;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    InvalidIdentity,
    InvalidCertificate,
    UnsupportedVersion,
    InvalidAddress,
    Limit,
    Untrusted,
    RePairRequired,
    Revoked,
    GenerationExhausted,
    ClockWentBackwards,
    MdnsCacheUnbounded,
    NativeDiscoveryFailed(u32),
    Storage,
    Crypto,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for Error {}
