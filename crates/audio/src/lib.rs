//! Endpoint metadata and format conversion. No stream is initialized or started.
//! Metadata operations belong on a normal COM thread, never an audio callback.

pub mod format;
#[cfg(windows)]
pub mod wasapi;
