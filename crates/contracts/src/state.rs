use crate::ErrorCode;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum SessionState {
    Idle,
    Preparing,
    Ready,
    Running,
    Stopping,
    Error,
    Blocked,
    Degraded,
    FailedMuted,
    Muted,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum Event {
    Prepare,
    Prepared,
    Start,
    Mute,
    Unmute,
    Degrade,
    Recover,
    Fail,
    Block,
    Stop,
    Stopped,
    Reset,
    Invalidate,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Transition {
    pub next: SessionState,
    pub invalidate_output: bool,
    pub clear_queues: bool,
    pub advance_epoch: bool,
    pub requires_ready_resources: bool,
    pub requires_explicit_user_action: bool,
}
/// A pure transition contract, not an owning runtime state machine.
pub fn transition(state: SessionState, event: Event) -> Result<Transition, ErrorCode> {
    use Event as E;
    use SessionState as S;
    let next = match (state, event) {
        (_, E::Stop) => S::Stopping,
        (S::Stopping, E::Stopped) => S::Idle,
        (S::Idle, E::Prepare) => S::Preparing,
        (S::Preparing, E::Prepared) => S::Ready,
        (S::Ready, E::Start) => S::Running,
        (S::Running | S::Degraded, E::Mute) => S::Muted,
        (S::Muted, E::Unmute) => S::Running,
        (S::Running, E::Degrade) => S::Degraded,
        (S::Degraded, E::Recover) => S::Running,
        (S::Preparing, E::Fail) => S::Error,
        (S::Preparing, E::Block) => S::Blocked,
        (S::Ready | S::Running | S::Muted | S::Degraded, E::Fail | E::Invalidate) => S::FailedMuted,
        (S::Ready | S::Running | S::Muted | S::Degraded | S::FailedMuted, E::Reset) => S::Preparing,
        _ => return Err(ErrorCode::InvalidState),
    };
    let invalidate_output = !matches!(next, S::Running | S::Degraded);
    let clear_queues = invalidate_output || matches!(event, E::Start | E::Unmute);
    Ok(Transition {
        next,
        invalidate_output,
        clear_queues,
        advance_epoch: matches!(
            event,
            E::Prepare | E::Reset | E::Invalidate | E::Stop | E::Mute | E::Unmute
        ),
        requires_ready_resources: matches!(event, E::Prepared | E::Start | E::Unmute | E::Recover),
        requires_explicit_user_action: matches!(event, E::Start | E::Unmute),
    })
}
pub fn next_epoch(epoch: u32) -> Result<u32, ErrorCode> {
    epoch.checked_add(1).ok_or(ErrorCode::EpochExhausted)
}
