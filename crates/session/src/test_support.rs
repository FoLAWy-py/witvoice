//! Debug-only trusted resource seam. No engine, PCM, audio type or wire capability.
use witvoice_contracts::{ErrorCode, values::Id};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Binding {
    pub session_id: Id,
    pub session_tag: u64,
    pub epoch: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Retirement {
    Planned,
    Fault,
}
/// Control-thread calls only. Implementations must keep resources bounded and
/// retire output synchronously before returning, including on cleanup failure.
/// `prepare` must refuse a new owner while any retired commit is outstanding.
pub trait Lifecycle: Send {
    fn prepare(&mut self, binding: &Binding) -> Result<(), ErrorCode>;
    fn ready(&self, binding: &Binding) -> bool;
    fn activate(&mut self, binding: &Binding) -> Result<(), ErrorCode>;
    fn invalidate_first(&mut self, reason: Retirement);
    fn ack_ready(&self) -> bool;
}
