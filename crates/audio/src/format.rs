//! Validated mono/stereo formats and allocation-free boundary conversion.

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Encoding {
    Pcm8,
    Pcm16,
    Pcm24,
    Pcm32,
    Float32,
}

impl Encoding {
    pub const fn bytes(self) -> usize {
        match self {
            Self::Pcm8 => 1,
            Self::Pcm16 => 2,
            Self::Pcm24 => 3,
            Self::Pcm32 | Self::Float32 => 4,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct AudioFormat {
    sample_rate: u32,
    channels: u16,
    encoding: Encoding,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormatError {
    SampleRate,
    Channels,
    BufferLength,
    NonFinite,
    Gain,
    UnsupportedEncoding,
    Layout,
}

impl AudioFormat {
    pub fn new(sample_rate: u32, channels: u16, encoding: Encoding) -> Result<Self, FormatError> {
        // Bound calculations and reject device formats outside this adapter's scope.
        if !(8_000..=192_000).contains(&sample_rate) {
            return Err(FormatError::SampleRate);
        }
        if !(1..=2).contains(&channels) {
            return Err(FormatError::Channels);
        }
        Ok(Self {
            sample_rate,
            channels,
            encoding,
        })
    }

    pub const fn sample_rate(self) -> u32 {
        self.sample_rate
    }
    pub const fn channels(self) -> u16 {
        self.channels
    }
    pub const fn encoding(self) -> Encoding {
        self.encoding
    }
    pub const fn frame_bytes(self) -> usize {
        self.channels as usize * self.encoding.bytes()
    }
    pub const fn needs_bus_resampling(self) -> bool {
        self.sample_rate != 48_000
    }

    /// Byte-level silence, including unsigned PCM8 midpoint. Partial trailing
    /// bytes are also silenced; malformed buffers must not retain old audio.
    pub fn silence(self, output: &mut [u8]) {
        output.fill(if self.encoding == Encoding::Pcm8 {
            128
        } else {
            0
        });
    }
}

fn decode(encoding: Encoding, bytes: &[u8]) -> f32 {
    match encoding {
        Encoding::Pcm8 => (bytes[0] as f32 - 128.0) / 128.0,
        Encoding::Pcm16 => i16::from_le_bytes([bytes[0], bytes[1]]) as f32 / 32768.0,
        Encoding::Pcm24 => {
            let value = ((bytes[0] as i32) | ((bytes[1] as i32) << 8) | ((bytes[2] as i32) << 16))
                << 8
                >> 8;
            value as f32 / 8_388_608.0
        }
        Encoding::Pcm32 => {
            i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as f32 / 2_147_483_648.0
        }
        Encoding::Float32 => f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
    }
}

fn encode(encoding: Encoding, sample: f32, output: &mut [u8]) {
    match encoding {
        Encoding::Pcm8 => output[0] = (sample * 128.0 + 128.0).round().clamp(0.0, 255.0) as u8,
        Encoding::Pcm16 => {
            let value = (sample * 32768.0).round().clamp(-32768.0, 32767.0) as i16;
            output.copy_from_slice(&value.to_le_bytes());
        }
        Encoding::Pcm24 => {
            let value = (sample as f64 * 8_388_608.0)
                .round()
                .clamp(-8_388_608.0, 8_388_607.0) as i32;
            output.copy_from_slice(&value.to_le_bytes()[..3]);
        }
        Encoding::Pcm32 => {
            let value = (sample as f64 * 2_147_483_648.0)
                .round()
                .clamp(-2_147_483_648.0, 2_147_483_647.0) as i32;
            output.copy_from_slice(&value.to_le_bytes());
        }
        Encoding::Float32 => output.copy_from_slice(&sample.to_le_bytes()),
    }
}

/// No resampling is performed. Frames are one-to-one; the caller must perform
/// any declared sample-rate conversion once at the boundary before bus use.
/// Errors erase the entire destination, including a nonfinite late frame.
pub fn capture_to_mono(
    format: AudioFormat,
    input: &[u8],
    output: &mut [f32],
) -> Result<(), FormatError> {
    output.fill(0.0);
    if output.len().checked_mul(format.frame_bytes()) != Some(input.len()) {
        return Err(FormatError::BufferLength);
    }
    if format.encoding == Encoding::Float32
        && input
            .as_chunks::<4>()
            .0
            .iter()
            .any(|b| !decode(format.encoding, b).is_finite())
    {
        return Err(FormatError::NonFinite);
    }
    let width = format.encoding.bytes();
    for (frame, dest) in input
        .chunks_exact(format.frame_bytes())
        .zip(output.iter_mut())
    {
        let left = decode(format.encoding, &frame[..width]);
        *dest = if format.channels == 2 {
            // Scale before sum to avoid overflow on two large finite float samples.
            left * 0.5 + decode(format.encoding, &frame[width..]) * 0.5
        } else {
            left
        };
        *dest = dest.clamp(-1.0, 1.0);
    }
    Ok(())
}

/// Gain and hard safety limiting precede identical mono-to-stereo mapping.
/// Destination buffers are caller-owned; there are no allocations or locks.
pub fn mono_to_render(
    format: AudioFormat,
    input: &[f32],
    gain: f32,
    output: &mut [u8],
) -> Result<(), FormatError> {
    format.silence(output);
    if input.len().checked_mul(format.frame_bytes()) != Some(output.len()) {
        return Err(FormatError::BufferLength);
    }
    if !gain.is_finite() || !(0.0..=4.0).contains(&gain) {
        return Err(FormatError::Gain);
    }
    if input.iter().any(|v| !v.is_finite()) {
        return Err(FormatError::NonFinite);
    }
    let width = format.encoding.bytes();
    for (source, frame) in input
        .iter()
        .zip(output.chunks_exact_mut(format.frame_bytes()))
    {
        let sample = (*source as f64 * gain as f64).clamp(-1.0, 1.0) as f32;
        for channel in frame.chunks_exact_mut(width) {
            encode(format.encoding, sample, channel);
        }
    }
    Ok(())
}
