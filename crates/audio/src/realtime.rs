//! Control-preallocated, source-clocked queues and fail-closed output governance.
//! No hardware is opened. Node must bind genuine converted results separately.
mod clock;
mod pipeline;
mod spsc;
pub use clock::{ClockError, DriftClock, SamplePhase};
pub use pipeline::{
    BLOCK_FRAMES, BUS_RATE, Binding, BlockError, CaptureBlock, CaptureConsumer, CaptureProducer,
    CommitTicket, Counters, OutputGate, Playout, ProcessedBlock, ProcessedProducer, QueueConfig,
    QueuePolicy, capture_endpoints, processed_endpoints,
};
pub use spsc::{Consumer, Producer, Ring, RingError};
