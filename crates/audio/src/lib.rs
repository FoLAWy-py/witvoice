//! Explicit endpoint metadata, boundary conversion and opt-in native streams.
//! Merely loading this crate never initializes a stream or starts capture.

pub mod format;
pub mod notifications;
pub mod stream;
#[cfg(windows)]
pub mod wasapi;
