//! Windows user-process controls. No audio, model, network or UI ownership.
#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::*;
#[cfg(windows)]
mod process;
#[cfg(windows)]
pub use process::{Process, ProcessJob, launch_node, launch_node_from_ui_job};
