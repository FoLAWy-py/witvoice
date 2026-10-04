//! Bounded capture packet validation. No resampling or playback authority.
use crate::format::{AudioFormat, FormatError, capture_to_mono};

pub const DISCONTINUITY: u32 = 1;
pub const SILENT: u32 = 2;
pub const TIMESTAMP_ERROR: u32 = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PacketError {
    Capacity,
    Flags,
    NullData,
    Format(FormatError),
}

/// Decode exactly one complete native-rate packet into preallocated storage.
/// Success leaves the unused destination tail zero; every failure erases all.
/// A silent packet may omit data entirely. Unknown flag bits are rejected.
pub fn decode_packet(
    format: AudioFormat,
    frames: usize,
    flags: u32,
    input: Option<&[u8]>,
    output: &mut [f32],
) -> Result<(), PacketError> {
    output.fill(0.0);
    if frames > output.len() {
        return Err(PacketError::Capacity);
    }
    if flags & !(DISCONTINUITY | SILENT | TIMESTAMP_ERROR) != 0 {
        return Err(PacketError::Flags);
    }
    if flags & SILENT != 0 || frames == 0 {
        return Ok(());
    }
    let bytes = input.ok_or(PacketError::NullData)?;
    capture_to_mono(format, bytes, &mut output[..frames]).map_err(PacketError::Format)
}

/// OS positions are actual native stream frames and QPC converted to 100ns.
/// They are not the session source timeline or a cross-machine wall clock.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CapturePacket {
    pub frames: u32,
    pub flags: u32,
    pub device_position: u64,
    pub qpc_100ns: u64,
}
impl CapturePacket {
    pub fn timestamp_valid(self) -> bool {
        self.flags & TIMESTAMP_ERROR == 0
    }
    pub fn discontinuity(self) -> bool {
        self.flags & DISCONTINUITY != 0
    }
}

#[cfg(windows)]
mod native;
#[cfg(windows)]
pub use native::{ExplicitStart, SharedStream, StreamError};
