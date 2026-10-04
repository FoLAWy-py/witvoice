use witvoice_audio::{
    format::{AudioFormat, Encoding, FormatError},
    stream::*,
};
#[test]
fn silent_null_and_native_metadata_are_explicit() {
    let format = AudioFormat::new(44_100, 2, Encoding::Pcm16).unwrap();
    let mut output = [0.7; 8];
    decode_packet(format, 4, SILENT | DISCONTINUITY, None, &mut output).unwrap();
    assert_eq!(output, [0.0; 8]);
    assert!(format.needs_bus_resampling());
    let packet = CapturePacket {
        frames: 4,
        flags: DISCONTINUITY | TIMESTAMP_ERROR,
        device_position: 200,
        qpc_100ns: 100,
    };
    assert!(packet.discontinuity());
    assert!(!packet.timestamp_valid());
}
#[test]
fn every_invalid_packet_erases_the_entire_destination() {
    let format = AudioFormat::new(16_000, 1, Encoding::Float32).unwrap();
    let bytes: Vec<u8> = [0.5f32, f32::NAN]
        .into_iter()
        .flat_map(f32::to_le_bytes)
        .collect();
    for (frames, flags, input, expected) in [
        (9, SILENT, None, PacketError::Capacity),
        (2, 8, Some(bytes.as_slice()), PacketError::Flags),
        (2, 0, None, PacketError::NullData),
        (
            2,
            0,
            Some(&bytes[..7]),
            PacketError::Format(FormatError::BufferLength),
        ),
        (
            2,
            0,
            Some(bytes.as_slice()),
            PacketError::Format(FormatError::NonFinite),
        ),
    ] {
        let mut output = [0.7; 8];
        assert_eq!(
            decode_packet(format, frames, flags, input, &mut output),
            Err(expected)
        );
        assert_eq!(output, [0.0; 8]);
    }
}
#[test]
fn valid_capture_leaves_unused_tail_zero() {
    let format = AudioFormat::new(16_000, 1, Encoding::Pcm16).unwrap();
    let mut output = [0.7; 4];
    decode_packet(format, 1, 0, Some(&16384i16.to_le_bytes()), &mut output).unwrap();
    assert_eq!(output, [0.5, 0.0, 0.0, 0.0]);
}
