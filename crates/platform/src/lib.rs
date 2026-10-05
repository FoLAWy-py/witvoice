//! Windows user-process controls. No audio, model, network or UI ownership.
pub mod discovery;
pub mod identity;
#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::*;
#[cfg(windows)]
mod process;
#[cfg(windows)]
pub use process::{
    MemberFailureReason, MemberFailureSnapshot, MemberFailureStage, Process, ProcessJob,
    ProcessNativeFailure, ProcessOperation, launch_node, launch_node_from_ui_job,
    process_native_failure,
};
