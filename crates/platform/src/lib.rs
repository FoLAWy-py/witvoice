//! Windows user-process controls. No audio, model, network or UI ownership.
pub mod identity;
pub mod discovery;
#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::*;
#[cfg(windows)]
mod process;
#[cfg(windows)]
pub use process::{Process, ProcessJob, launch_node, launch_node_from_ui_job};
