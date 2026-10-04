use crate::ErrorCode;

pub const HEADER_BYTES: usize = 40;
pub const SOURCE_UNKNOWN: u64 = u64::MAX;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaKind {
    Source = 1,
    Converted = 2,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MediaHeader {
    pub kind: MediaKind,
    pub session_tag: u64,
    pub epoch: u32,
    pub sequence: u32,
    pub media_sample_index: u64,
    pub source_sample_index: u64,
    pub sample_count: u16,
}
#[derive(Debug, Clone, Copy)]
pub struct MediaBinding {
    pub session_tag: u64,
    pub epoch: u32,
    pub kind: MediaKind,
    pub sample_count: u16,
    pub media_permitted: bool,
}
impl MediaHeader {
    pub fn encode(self) -> Result<[u8; HEADER_BYTES], ErrorCode> {
        self.validate_format()?;
        let mut out = [0; HEADER_BYTES];
        out[0] = 1;
        out[1] = self.kind as u8;
        out[4..12].copy_from_slice(&self.session_tag.to_be_bytes());
        out[12..16].copy_from_slice(&self.epoch.to_be_bytes());
        out[16..20].copy_from_slice(&self.sequence.to_be_bytes());
        out[20..28].copy_from_slice(&self.media_sample_index.to_be_bytes());
        out[28..36].copy_from_slice(&self.source_sample_index.to_be_bytes());
        out[36..38].copy_from_slice(&self.sample_count.to_be_bytes());
        Ok(out)
    }
    fn validate_format(self) -> Result<(), ErrorCode> {
        if ![240, 480].contains(&self.sample_count)
            || self.session_tag == 0
            || (self.kind == MediaKind::Source
                && (self.source_sample_index == SOURCE_UNKNOWN
                    || self.source_sample_index != self.media_sample_index))
            || self
                .media_sample_index
                .checked_add(u64::from(self.sample_count))
                .is_none()
            || (self.source_sample_index != SOURCE_UNKNOWN
                && self
                    .source_sample_index
                    .checked_add(u64::from(self.sample_count))
                    .is_none())
        {
            return Err(ErrorCode::InvalidMedia);
        }
        Ok(())
    }
    pub fn decode(packet: &[u8], binding: MediaBinding) -> Result<Self, ErrorCode> {
        if packet.len() < HEADER_BYTES {
            return Err(ErrorCode::InvalidMedia);
        }
        if packet[0] != 1 {
            return Err(ErrorCode::UnsupportedVersion);
        }
        if packet[2..4] != [0, 0] || packet[38..40] != [0, 0] {
            return Err(ErrorCode::InvalidMedia);
        }
        let kind = match packet[1] {
            1 => MediaKind::Source,
            2 => MediaKind::Converted,
            _ => return Err(ErrorCode::InvalidMedia),
        };
        let header = Self {
            kind,
            session_tag: u64::from_be_bytes(
                packet[4..12]
                    .try_into()
                    .map_err(|_| ErrorCode::InvalidMedia)?,
            ),
            epoch: u32::from_be_bytes(
                packet[12..16]
                    .try_into()
                    .map_err(|_| ErrorCode::InvalidMedia)?,
            ),
            sequence: u32::from_be_bytes(
                packet[16..20]
                    .try_into()
                    .map_err(|_| ErrorCode::InvalidMedia)?,
            ),
            media_sample_index: u64::from_be_bytes(
                packet[20..28]
                    .try_into()
                    .map_err(|_| ErrorCode::InvalidMedia)?,
            ),
            source_sample_index: u64::from_be_bytes(
                packet[28..36]
                    .try_into()
                    .map_err(|_| ErrorCode::InvalidMedia)?,
            ),
            sample_count: u16::from_be_bytes(
                packet[36..38]
                    .try_into()
                    .map_err(|_| ErrorCode::InvalidMedia)?,
            ),
        };
        header.validate_format()?;
        if packet.len() != HEADER_BYTES + usize::from(header.sample_count) * 2
            || header.sample_count != binding.sample_count
            || kind != binding.kind
        {
            return Err(ErrorCode::InvalidMedia);
        }
        if header.session_tag != binding.session_tag {
            return Err(ErrorCode::SessionMismatch);
        }
        if header.epoch != binding.epoch {
            return Err(ErrorCode::EpochMismatch);
        }
        if !binding.media_permitted {
            return Err(ErrorCode::PermissionDenied);
        }
        Ok(header)
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SequenceOrder {
    Equal,
    Before,
    After,
    Ambiguous,
}
pub fn sequence_order(candidate: u32, reference: u32) -> SequenceOrder {
    match candidate.wrapping_sub(reference) {
        0 => SequenceOrder::Equal,
        0x8000_0000 => SequenceOrder::Ambiguous,
        delta if delta < 0x8000_0000 => SequenceOrder::After,
        _ => SequenceOrder::Before,
    }
}
/// Local monotonic nanoseconds only, never cross-device wall-clock subtraction.
pub fn is_expired(now_ns: u64, deadline_ns: u64) -> bool {
    now_ns >= deadline_ns
}
