//! Windows user-process controls. No audio, model, network or UI ownership.
#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::*;
