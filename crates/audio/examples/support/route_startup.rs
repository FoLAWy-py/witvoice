//! Finite capture-only preparation for the explicitly selected route diagnostic.
//! Never grants sink authority. The caller owns an unarmed, unshared OutputGate.
use std::time::Duration;
use witvoice_audio::stream::CapturePacket;

pub const LIMIT: Duration = Duration::from_millis(100);

#[derive(Default, Debug, serde::Serialize)]
pub struct Startup {
    pub packets: u32,
    pub discarded_frames: u32,
    pub discontinuity_packets: u32,
    pub clean_packets: u32,
    pub complete: bool,
    next_position: Option<u64>,
    last_qpc: Option<u64>,
}
impl Startup {
    pub fn observe(
        &mut self,
        packet: CapturePacket,
        elapsed: Duration,
    ) -> Result<(), &'static str> {
        if self.complete || elapsed >= LIMIT || self.packets >= 16 {
            return Err("startup deadline or packet bound");
        }
        self.packets += 1;
        self.discontinuity_packets += u32::from(packet.discontinuity());
        if packet.frames == 0 || packet.frames > 1440 || !packet.timestamp_valid() {
            return Err("startup capacity or invalid timestamp");
        }
        self.discarded_frames = self
            .discarded_frames
            .checked_add(packet.frames)
            .ok_or("startup frame overflow")?;
        let next = packet
            .device_position
            .checked_add(u64::from(packet.frames))
            .ok_or("startup position overflow")?;
        if self.packets > 1 {
            if packet.discontinuity()
                || self.next_position != Some(packet.device_position)
                || self.last_qpc.is_some_and(|last| packet.qpc_100ns <= last)
            {
                return Err("startup subsequent discontinuity or timeline gap");
            }
            self.clean_packets += 1;
        }
        // Always discard the first packet, even when clean. Its flags are evidence,
        // not a claim that a first discontinuity is harmless or explained.
        self.next_position = Some(next);
        self.last_qpc = Some(packet.qpc_100ns);
        self.complete = self.clean_packets == 2;
        Ok(())
    }
}
