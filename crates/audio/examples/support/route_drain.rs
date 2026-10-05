//! Ordinary-thread diagnostic capture drain. No wait or hardware access here.
use witvoice_audio::stream::CapturePacket;

pub const MAX_PACKETS: usize = 4;
pub const MAX_PACKET_FRAMES: usize = 1440;
pub const MAX_PASS_FRAMES: usize = MAX_PACKETS * MAX_PACKET_FRAMES;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum End {
    Empty,
    Budget,
    Deadline,
}
impl End {
    pub fn may_wait(self) -> bool {
        self == Self::Empty
    }
}

#[derive(Debug, serde::Serialize)]
pub struct PacketMetadata {
    pub frames: u32,
    pub flags: u32,
    pub device_position: u64,
    pub qpc_100ns: u64,
}
impl From<CapturePacket> for PacketMetadata {
    fn from(value: CapturePacket) -> Self {
        Self {
            frames: value.frames,
            flags: value.flags,
            device_position: value.device_position,
            qpc_100ns: value.qpc_100ns,
        }
    }
}

#[derive(Debug, Default, serde::Serialize)]
pub struct Stats {
    pub passes: u64,
    pub read_calls: u64,
    pub empty_passes: u64,
    pub budget_exhausted_passes: u64,
    pub deadline_stops: u64,
    pub observed_packets: u64,
    pub observed_frames: u64,
    pub accepted_packets: u64,
    pub accepted_frames: u64,
    pub deadline_discarded_frames: u64,
    pub discontinuity_packets: u64,
    pub timestamp_error_packets: u64,
    pub maximum_packets_per_pass: usize,
    pub maximum_frames_per_pass: usize,
    pub maximum_scheduler_gap_ns: u64,
    pub error_packet: Option<PacketMetadata>,
    #[serde(skip)]
    last_pass_started_ns: Option<u64>,
    #[serde(skip)]
    last_observed_ns: Option<u64>,
}

impl Stats {
    fn observe_clock(&mut self, now: u64) -> Result<(), String> {
        if self.last_observed_ns.is_some_and(|previous| now < previous) {
            return Err("capture scheduler clock regressed".into());
        }
        self.last_observed_ns = Some(now);
        Ok(())
    }
}

/// Same absolute deadline is supplied to every pass. A full pass yields to the
/// caller's render/watch/deadline work; only an observed empty buffer may wait.
/// `read` performs exactly one native packet operation. `consume` sees only
/// clean packets before the deadline. Storage is reused and cleared on errors.
pub fn drain(
    stats: &mut Stats,
    packet: &mut [f32; MAX_PACKET_FRAMES],
    deadline_ns: u64,
    mut clock: impl FnMut() -> u64,
    mut guard: impl FnMut() -> Result<(), String>,
    mut read: impl FnMut(&mut [f32]) -> Result<Option<CapturePacket>, String>,
    mut consume: impl FnMut(CapturePacket, &[f32]) -> Result<(), String>,
) -> Result<End, String> {
    packet.fill(0.0);
    let started = clock();
    stats.observe_clock(started)?;
    if let Some(previous) = stats.last_pass_started_ns {
        stats.maximum_scheduler_gap_ns = stats.maximum_scheduler_gap_ns.max(started - previous);
    }
    stats.last_pass_started_ns = Some(started);
    stats.passes += 1;
    let mut pass_frames = 0usize;
    for index in 0..MAX_PACKETS {
        let step: Result<Option<End>, String> = (|| {
            let before = clock();
            stats.observe_clock(before)?;
            if before >= deadline_ns {
                stats.deadline_stops += 1;
                return Ok(Some(End::Deadline));
            }
            guard()?;
            // A guard can dispatch notifications. It cannot renew the deadline.
            let guarded = clock();
            stats.observe_clock(guarded)?;
            if guarded >= deadline_ns {
                stats.deadline_stops += 1;
                return Ok(Some(End::Deadline));
            }
            stats.read_calls += 1;
            let observed = read(packet)?;
            let after = clock();
            stats.observe_clock(after)?;
            let Some(meta) = observed else {
                if after >= deadline_ns {
                    stats.deadline_stops += 1;
                    return Ok(Some(End::Deadline));
                }
                stats.empty_passes += 1;
                return Ok(Some(End::Empty));
            };
            stats.observed_packets += 1;
            stats.observed_frames += u64::from(meta.frames);
            stats.discontinuity_packets += u64::from(meta.discontinuity());
            stats.timestamp_error_packets += u64::from(!meta.timestamp_valid());
            let frames = meta.frames as usize;
            if frames == 0 || frames > MAX_PACKET_FRAMES {
                stats.error_packet = Some(meta.into());
                return Err("capture drain packet exceeds bounded capacity".into());
            }
            pass_frames += frames;
            stats.maximum_packets_per_pass = stats.maximum_packets_per_pass.max(index + 1);
            stats.maximum_frames_per_pass = stats.maximum_frames_per_pass.max(pass_frames);
            if pass_frames > MAX_PASS_FRAMES {
                stats.error_packet = Some(meta.into());
                return Err("capture drain pass exceeds bounded capacity".into());
            }
            if meta.discontinuity() || !meta.timestamp_valid() {
                stats.error_packet = Some(meta.into());
                return Err("capture discontinuity or invalid timestamp".into());
            }
            if after >= deadline_ns {
                stats.deadline_stops += 1;
                stats.deadline_discarded_frames += u64::from(meta.frames);
                return Ok(Some(End::Deadline));
            }
            if let Err(error) = consume(meta, &packet[..frames]) {
                stats.error_packet = Some(meta.into());
                return Err(error);
            }
            stats.accepted_packets += 1;
            stats.accepted_frames += u64::from(meta.frames);
            Ok(None)
        })();
        packet.fill(0.0);
        if let Some(end) = step? {
            return Ok(end);
        }
    }
    stats.budget_exhausted_passes += 1;
    Ok(End::Budget)
}
