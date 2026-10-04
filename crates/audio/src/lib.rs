//! Endpoint metadata and format conversion. No stream is initialized or started.
//! Metadata operations belong on a normal COM thread, never an audio callback.

pub mod format;
pub mod notifications;
#[cfg(windows)]
pub mod wasapi;
