use super::{ClockError, Consumer, DriftClock, Producer, Ring, SamplePhase};
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicUsize, Ordering};

pub const BUS_RATE: u32 = 48_000;
pub const BLOCK_FRAMES: usize = 480;
const BLOCK_NS: u64 = 10_000_000;
const MAX_CALLBACK: usize = 960;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Binding {
    pub session_tag: u64,
    pub epoch: u32,
}
impl Binding {
    pub fn new(session_tag: u64, epoch: u32) -> Result<Self, BlockError> {
        if session_tag == 0 {
            Err(BlockError::Binding)
        } else {
            Ok(Self { session_tag, epoch })
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueuePolicy {
    RejectNewest,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QueueConfig {
    pub capacity_blocks: usize,
    pub target_frames: u32,
    pub maximum_age_ns: u64,
    pub overflow: QueuePolicy,
}
impl QueueConfig {
    pub fn capture() -> Self {
        Self {
            capacity_blocks: 4,
            target_frames: 480,
            maximum_age_ns: 20_000_000,
            overflow: QueuePolicy::RejectNewest,
        }
    }
    pub fn output() -> Self {
        Self {
            capacity_blocks: 8,
            target_frames: 984,
            maximum_age_ns: 60_000_000,
            overflow: QueuePolicy::RejectNewest,
        }
    }
    fn validate(self, capacity: usize, capture: bool) -> Result<(), BlockError> {
        if self.capacity_blocks != capacity
            || !(2..=64).contains(&capacity)
            || self.target_frames < 480
            || self.target_frames as usize >= capacity * BLOCK_FRAMES
            || self.target_frames > if capture { 960 } else { 1440 }
            || self.maximum_age_ns < BLOCK_NS
            || self.maximum_age_ns > if capture { 20_000_000 } else { 200_000_000 }
        {
            return Err(BlockError::Configuration);
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockError {
    Configuration,
    Binding,
    Interval,
    Timestamp,
    NonFinite,
    Capacity,
    Expired,
    Stale,
    Underflow,
    Muted,
    Clock(ClockError),
}
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Counters {
    pub planned_silence_frames: u64,
    pub fault_silence_frames: u64,
    pub underflow_frames: u64,
    pub underflow_callbacks: u64,
    pub overflow_blocks: u64,
    pub expired_blocks: u64,
    pub stale_blocks: u64,
    pub invalid_blocks: u64,
    pub assembled_frames: u64,
    pub sink_pcm_frames: u64,
    pub sink_silence_frames: u64,
    pub sink_failures: u64,
    pub callback_count: u64,
    pub capture_empty_reads: u64,
}
#[derive(Clone, Copy)]
struct Block {
    binding: Binding,
    source_start: u64,
    arrival_ns: u64,
    deadline_ns: u64,
    count: u16,
    samples: [f32; BLOCK_FRAMES],
}
fn block(
    binding: Binding,
    source_start: u64,
    arrival_ns: u64,
    deadline_ns: u64,
    samples: &[f32],
) -> Result<Block, BlockError> {
    if binding.session_tag == 0 {
        return Err(BlockError::Binding);
    }
    if samples.is_empty()
        || samples.len() > BLOCK_FRAMES
        || source_start.checked_add(samples.len() as u64).is_none()
    {
        return Err(BlockError::Interval);
    }
    if arrival_ns == u64::MAX || deadline_ns == u64::MAX || deadline_ns <= arrival_ns {
        return Err(BlockError::Timestamp);
    }
    if samples.iter().any(|v| !v.is_finite()) {
        return Err(BlockError::NonFinite);
    }
    let mut storage = [0.0; BLOCK_FRAMES];
    storage[..samples.len()].copy_from_slice(samples);
    Ok(Block {
        binding,
        source_start,
        arrival_ns,
        deadline_ns,
        count: samples.len() as u16,
        samples: storage,
    })
}
/// Capture data cannot be passed to processed playout: separate opaque types.
#[derive(Clone, Copy)]
pub struct CaptureBlock(Block);
impl CaptureBlock {
    /// Bus-rate PCM only. Native non-48k capture needs its explicit boundary
    /// converter in its assigned task, never an implicit rate capability here.
    pub fn from_capture(
        binding: Binding,
        source_start: u64,
        arrival_ns: u64,
        deadline_ns: u64,
        samples: &[f32],
    ) -> Result<Self, BlockError> {
        block(binding, source_start, arrival_ns, deadline_ns, samples).map(Self)
    }
}
#[derive(Clone, Copy)]
pub struct ProcessedBlock(Block);
impl ProcessedBlock {
    /// Trusted Node/model assembler supplies converted PCM. This does not prove
    /// model provenance and is not an IdentityEngine or CaptureBlock adapter.
    pub fn from_model_result(
        binding: Binding,
        source_start: u64,
        arrival_ns: u64,
        deadline_ns: u64,
        samples: &[f32],
    ) -> Result<Self, BlockError> {
        if samples.len() != BLOCK_FRAMES {
            return Err(BlockError::Interval);
        }
        block(binding, source_start, arrival_ns, deadline_ns, samples).map(Self)
    }
}

/// One epoch owner, default muted, irreversible invalidation. arm requires
/// exclusive control-thread ownership before sharing; recovery needs a NEW
/// gate/queues with a fresh epoch, never arm of the old owner.
#[derive(Debug)]
pub struct OutputGate {
    binding: Binding,
    state: AtomicU8,
    in_flight: AtomicUsize,
    native_claimed: AtomicBool,
}
impl OutputGate {
    pub fn new(binding: Binding) -> Result<Self, BlockError> {
        if binding.session_tag == 0 {
            return Err(BlockError::Binding);
        }
        Ok(Self {
            binding,
            state: AtomicU8::new(0),
            in_flight: AtomicUsize::new(0),
            native_claimed: AtomicBool::new(false),
        })
    }
    pub fn arm(&mut self) -> Result<(), BlockError> {
        self.state
            .compare_exchange(0, 1, Ordering::SeqCst, Ordering::SeqCst)
            .map(|_| ())
            .map_err(|_| BlockError::Muted)
    }
    pub fn binding(&self) -> Binding {
        self.binding
    }
    pub(crate) fn claim_native_owner(&self) -> bool {
        self.native_claimed
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    }
    pub fn is_live(&self) -> bool {
        self.state.load(Ordering::SeqCst) == 1
    }
    pub fn is_faulted(&self) -> bool {
        self.state.load(Ordering::SeqCst) & 4 != 0
    }
    /// FIRST step of Stop/Mute, before IPC, join, COM cleanup or cancellation.
    pub fn invalidate(&self) {
        self.state.fetch_or(2, Ordering::SeqCst);
    }
    pub fn fail(&self) {
        self.state.fetch_or(6, Ordering::SeqCst);
    }
    /// Control thread may poll with a deadline; false is NOT a successful ACK.
    /// Tickets cover the actual sink ReleaseBuffer, not just PCM assembly.
    pub fn ack_ready(&self) -> bool {
        !self.is_live() && self.in_flight.load(Ordering::SeqCst) == 0
    }
    pub fn begin_commit(&self) -> Option<CommitTicket<'_>> {
        if !self.is_live() {
            return None;
        }
        let count = self.in_flight.load(Ordering::SeqCst);
        // One CAS, no spin/retry. Contention or too many tickets fails silent.
        if count >= 8
            || self
                .in_flight
                .compare_exchange(count, count + 1, Ordering::SeqCst, Ordering::SeqCst)
                .is_err()
        {
            return None;
        }
        let ticket = CommitTicket { gate: self };
        if !ticket.is_live() {
            return None;
        }
        Some(ticket)
    }
}
pub struct CommitTicket<'a> {
    gate: &'a OutputGate,
}
impl CommitTicket<'_> {
    pub fn is_live(&self) -> bool {
        self.gate.is_live()
    }
}
impl Drop for CommitTicket<'_> {
    fn drop(&mut self) {
        self.gate.in_flight.fetch_sub(1, Ordering::SeqCst);
    }
}

struct Validate {
    binding: Binding,
    config: QueueConfig,
    end: Option<u64>,
    last_now: Option<u64>,
}
impl Validate {
    fn new(binding: Binding, config: QueueConfig) -> Self {
        Self {
            binding,
            config,
            end: None,
            last_now: None,
        }
    }
    fn now(&mut self, now: u64) -> Result<(), BlockError> {
        if now == u64::MAX || self.last_now.is_some_and(|last| now < last) {
            return Err(BlockError::Timestamp);
        }
        self.last_now = Some(now);
        Ok(())
    }
    fn check(&mut self, b: Block, now: u64) -> Result<(), BlockError> {
        self.now(now)?;
        if b.binding != self.binding {
            return Err(BlockError::Stale);
        }
        if b.arrival_ns > now || b.deadline_ns - b.arrival_ns > self.config.maximum_age_ns {
            return Err(BlockError::Timestamp);
        }
        if now >= b.deadline_ns || now - b.arrival_ns >= self.config.maximum_age_ns {
            return Err(BlockError::Expired);
        }
        if self.end.is_some_and(|end| b.source_start < end) {
            return Err(BlockError::Stale);
        }
        Ok(())
    }
    fn accepted(&mut self, b: Block) {
        self.end = Some(b.source_start + b.count as u64);
    }
}
fn count_error(stats: &mut Counters, error: BlockError) {
    match error {
        BlockError::Expired => stats.expired_blocks = stats.expired_blocks.saturating_add(1),
        BlockError::Stale => stats.stale_blocks = stats.stale_blocks.saturating_add(1),
        BlockError::Capacity => stats.overflow_blocks = stats.overflow_blocks.saturating_add(1),
        _ => stats.invalid_blocks = stats.invalid_blocks.saturating_add(1),
    }
}
pub struct CaptureProducer<'a> {
    producer: Producer<'a, CaptureBlock>,
    validate: Validate,
    stats: Counters,
}
pub struct CaptureConsumer<'a> {
    consumer: Consumer<'a, CaptureBlock>,
    validate: Validate,
    stats: Counters,
}
pub fn capture_endpoints(
    ring: &mut Ring<CaptureBlock>,
    binding: Binding,
    config: QueueConfig,
) -> Result<(CaptureProducer<'_>, CaptureConsumer<'_>), BlockError> {
    config.validate(ring.capacity(), true)?;
    if binding.session_tag == 0 {
        return Err(BlockError::Binding);
    }
    let (producer, consumer) = ring.split();
    Ok((
        CaptureProducer {
            producer,
            validate: Validate::new(binding, config),
            stats: Counters::default(),
        },
        CaptureConsumer {
            consumer,
            validate: Validate::new(binding, config),
            stats: Counters::default(),
        },
    ))
}
impl CaptureProducer<'_> {
    pub fn push(&mut self, value: CaptureBlock, now_ns: u64) -> Result<(), BlockError> {
        let result = self.validate.check(value.0, now_ns).and_then(|_| {
            self.producer
                .try_push(value)
                .map_err(|_| BlockError::Capacity)
        });
        if let Err(error) = result {
            count_error(&mut self.stats, error);
        } else {
            self.validate.accepted(value.0);
        }
        result
    }
    pub fn counters(&self) -> Counters {
        self.stats
    }
}
impl CaptureConsumer<'_> {
    pub fn read(&mut self, output: &mut [f32], now_ns: u64) -> Result<(u64, usize), BlockError> {
        output.fill(0.0);
        let b = match self.consumer.try_pop() {
            Some(value) => value.0,
            None => {
                // Worker empty polls are observable, not invented missing
                // physical capture frames; output remains entirely zero.
                self.stats.capture_empty_reads = self.stats.capture_empty_reads.saturating_add(1);
                return Err(BlockError::Underflow);
            }
        };
        let result = self.validate.check(b, now_ns).and({
            if output.len() < b.count as usize {
                Err(BlockError::Capacity)
            } else {
                Ok(())
            }
        });
        if let Err(error) = result {
            count_error(&mut self.stats, error);
            return Err(error);
        }
        output[..b.count as usize].copy_from_slice(&b.samples[..b.count as usize]);
        self.validate.accepted(b);
        Ok((b.source_start, b.count as usize))
    }
    pub fn counters(&self) -> Counters {
        self.stats
    }
}
pub struct ProcessedProducer<'a> {
    producer: Producer<'a, ProcessedBlock>,
    validate: Validate,
    gate: &'a OutputGate,
    stats: Counters,
}
pub fn processed_endpoints<'a>(
    ring: &'a mut Ring<ProcessedBlock>,
    gate: &'a OutputGate,
    config: QueueConfig,
) -> Result<(ProcessedProducer<'a>, Playout<'a>), BlockError> {
    config.validate(ring.capacity(), false)?;
    let clock = DriftClock::new(config.target_frames).map_err(BlockError::Clock)?;
    let (producer, consumer) = ring.split();
    Ok((
        ProcessedProducer {
            producer,
            validate: Validate::new(gate.binding(), config),
            gate,
            stats: Counters::default(),
        },
        Playout {
            consumer,
            validate: Validate::new(gate.binding(), config),
            gate,
            stats: Counters::default(),
            current: None,
            offset: 0,
            left: None,
            right: None,
            clock,
            phase: SamplePhase::default(),
            missing: 0,
            last_source: None,
            commit_until: None,
            pending_advance: 0,
        },
    ))
}
impl ProcessedProducer<'_> {
    pub fn push(&mut self, value: ProcessedBlock, now_ns: u64) -> Result<(), BlockError> {
        if !self.gate.is_live() {
            return Err(BlockError::Muted);
        }
        let result = self.validate.check(value.0, now_ns).and_then(|_| {
            self.producer
                .try_push(value)
                .map_err(|_| BlockError::Capacity)
        });
        if let Err(error) = result {
            count_error(&mut self.stats, error);
            if !matches!(
                error,
                BlockError::Stale | BlockError::Expired | BlockError::Capacity
            ) {
                self.gate.fail();
            }
        } else {
            self.validate.accepted(value.0);
        }
        result
    }
    pub fn counters(&self) -> Counters {
        self.stats
    }
}
#[derive(Clone, Copy)]
struct SourceSample {
    value: f32,
    index: u64,
    arrival: u64,
    deadline: u64,
}
pub struct Playout<'a> {
    consumer: Consumer<'a, ProcessedBlock>,
    validate: Validate,
    gate: &'a OutputGate,
    stats: Counters,
    current: Option<Block>,
    offset: usize,
    left: Option<SourceSample>,
    right: Option<SourceSample>,
    clock: DriftClock,
    phase: SamplePhase,
    missing: u32,
    last_source: Option<u64>,
    commit_until: Option<u64>,
    pending_advance: u32,
}
impl<'a> Playout<'a> {
    pub fn gate(&self) -> &'a OutputGate {
        self.gate
    }
    pub fn config(&self) -> QueueConfig {
        self.validate.config
    }
    pub fn counters(&self) -> Counters {
        self.stats
    }
    pub fn correction_ppm(&self) -> f64 {
        self.clock.ppm()
    }
    pub fn source_position(&self) -> Option<u64> {
        self.last_source
    }
    /// Recheck using the SAME local monotonic clock immediately before native
    /// commit. Assembly-time expiry alone is insufficient if GetBuffer stalls.
    pub fn commit_valid(&self, now_ns: u64) -> bool {
        self.gate.is_live()
            && self.validate.last_now.is_some_and(|start| now_ns >= start)
            && self.commit_until.is_some_and(|until| now_ns < until)
    }
    pub(crate) fn record_sink(&mut self, frames: u32, pcm: bool) {
        if pcm {
            self.stats.sink_pcm_frames = self.stats.sink_pcm_frames.saturating_add(frames as u64);
        } else {
            self.stats.sink_silence_frames =
                self.stats.sink_silence_frames.saturating_add(frames as u64);
        }
    }
    pub(crate) fn record_sink_failure(&mut self) {
        self.stats.sink_failures = self.stats.sink_failures.saturating_add(1);
    }
    pub fn queued_frames(&self) -> u32 {
        let remaining = self.current.map_or(0, |b| b.count as usize - self.offset);
        (self.consumer.queued() * BLOCK_FRAMES
            + remaining
            + usize::from(self.left.is_some())
            + usize::from(self.right.is_some()))
        .saturating_sub(self.pending_advance as usize) as u32
    }
    fn next_source(&mut self, now: u64) -> Result<SourceSample, BlockError> {
        if self.current.is_none_or(|b| self.offset == b.count as usize) {
            self.current = None;
            let b = self.consumer.try_pop().ok_or(BlockError::Underflow)?.0;
            self.validate.check(b, now)?;
            if self.validate.end.is_some_and(|end| b.source_start != end) {
                return Err(BlockError::Interval);
            }
            self.validate.accepted(b);
            self.current = Some(b);
            self.offset = 0;
        }
        let b = self.current.ok_or(BlockError::Underflow)?;
        let sample = SourceSample {
            value: b.samples[self.offset],
            index: b.source_start + self.offset as u64,
            arrival: b.arrival_ns,
            deadline: b.deadline_ns,
        };
        self.offset += 1;
        Ok(sample)
    }
    fn sample_live(&self, s: SourceSample, now: u64) -> bool {
        s.arrival <= now
            && now < s.deadline
            && now - s.arrival < self.validate.config.maximum_age_ns
    }
    fn assemble(&mut self, output: &mut [f32], now: u64) -> Result<(), BlockError> {
        self.validate.now(now)?;
        let ratio = self
            .clock
            .update(self.queued_frames())
            .map_err(BlockError::Clock)?;
        if self.left.is_none() {
            self.left = Some(self.next_source(now)?);
        }
        for value in output {
            // Advance only when the NEXT output sample actually needs it.
            // Do not turn a complete current callback into underflow merely
            // because a future callback's packet has not yet arrived.
            for _ in 0..self.pending_advance {
                self.left = Some(match self.right.take() {
                    Some(right) => right,
                    None => self.next_source(now)?,
                });
            }
            self.pending_advance = 0;
            let left = self.left.ok_or(BlockError::Underflow)?;
            if self.phase.fraction() > 0.0 && self.right.is_none() {
                self.right = Some(self.next_source(now)?);
            }
            let right = if self.phase.fraction() > 0.0 {
                self.right.ok_or(BlockError::Underflow)?
            } else {
                left
            };
            if !self.sample_live(left, now) || !self.sample_live(right, now) {
                return Err(BlockError::Expired);
            }
            let until = left
                .deadline
                .min(right.deadline)
                .min(
                    left.arrival
                        .saturating_add(self.validate.config.maximum_age_ns),
                )
                .min(
                    right
                        .arrival
                        .saturating_add(self.validate.config.maximum_age_ns),
                );
            self.commit_until = Some(self.commit_until.map_or(until, |old| old.min(until)));
            if self.phase.fraction() > 0.0
                && right.index != left.index.checked_add(1).ok_or(BlockError::Interval)?
            {
                return Err(BlockError::Interval);
            }
            if self.last_source.is_some_and(|last| left.index < last) {
                return Err(BlockError::Stale);
            }
            *value = (left.value as f64
                + (right.value as f64 - left.value as f64) * self.phase.fraction())
            .clamp(-1.0, 1.0) as f32;
            self.last_source = Some(left.index);
            self.pending_advance = self.phase.advance(1, ratio).map_err(BlockError::Clock)?;
        }
        Ok(())
    }
    /// One callback <=20ms. Full destination zero on any partial failure.
    /// Bounded work <=1920 sample advances + <=6 block reads; never drain a
    /// backlog. No target priming wait, VAD deletion or normal-block eviction.
    pub fn render(&mut self, output: &mut [f32], now_ns: u64) -> Result<(), BlockError> {
        output.fill(0.0);
        self.commit_until = None;
        self.stats.callback_count = self.stats.callback_count.saturating_add(1);
        if !self.gate.is_live() {
            if self.gate.is_faulted() {
                self.stats.fault_silence_frames = self
                    .stats
                    .fault_silence_frames
                    .saturating_add(output.len() as u64);
            } else {
                self.stats.planned_silence_frames = self
                    .stats
                    .planned_silence_frames
                    .saturating_add(output.len() as u64);
            }
            return Err(BlockError::Muted);
        }
        let result = if output.is_empty() || output.len() > MAX_CALLBACK {
            Err(BlockError::Capacity)
        } else {
            self.assemble(output, now_ns)
        };
        if let Err(error) = result {
            output.fill(0.0);
            self.left = None;
            self.right = None;
            self.current = None;
            self.pending_advance = 0;
            self.stats.fault_silence_frames = self
                .stats
                .fault_silence_frames
                .saturating_add(output.len() as u64);
            if error == BlockError::Underflow {
                self.stats.underflow_callbacks = self.stats.underflow_callbacks.saturating_add(1);
                self.stats.underflow_frames = self
                    .stats
                    .underflow_frames
                    .saturating_add(output.len() as u64);
                self.missing = self.missing.saturating_add(output.len() as u32);
                if self.missing >= 1440 {
                    self.gate.fail();
                }
            } else {
                count_error(&mut self.stats, error);
                self.gate.fail();
            }
            return Err(error);
        }
        self.missing = 0;
        if !self.gate.is_live() {
            output.fill(0.0);
            self.stats.planned_silence_frames = self
                .stats
                .planned_silence_frames
                .saturating_add(output.len() as u64);
            return Err(BlockError::Muted);
        }
        self.stats.assembled_frames = self
            .stats
            .assembled_frames
            .saturating_add(output.len() as u64);
        Ok(())
    }
}
