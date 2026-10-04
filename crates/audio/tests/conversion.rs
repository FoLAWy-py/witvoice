use witvoice_audio::format::{AudioFormat, Encoding, FormatError, capture_to_mono, mono_to_render};

fn format(channels: u16, encoding: Encoding) -> AudioFormat {
    AudioFormat::new(48_000, channels, encoding).unwrap()
}

#[test]
fn rejects_unimplemented_channels_and_rates() {
    assert_eq!(
        AudioFormat::new(48_000, 6, Encoding::Pcm16),
        Err(FormatError::Channels)
    );
    assert_eq!(
        AudioFormat::new(0, 1, Encoding::Pcm16),
        Err(FormatError::SampleRate)
    );
    assert!(
        AudioFormat::new(44_100, 2, Encoding::Float32)
            .unwrap()
            .needs_bus_resampling()
    );
    assert!(!format(1, Encoding::Float32).needs_bus_resampling());
}

#[test]
fn pcm16_downmix_does_not_overflow() {
    let mut output = [9.0; 3];
    let bytes = [0, 128, 0, 128, 255, 127, 255, 127, 0, 64, 0, 192];
    capture_to_mono(format(2, Encoding::Pcm16), &bytes, &mut output).unwrap();
    assert_eq!(output, [-1.0, 32767.0 / 32768.0, 0.0]);
}

#[test]
fn pcm24_sign_extension_and_endpoints() {
    let mut output = [0.0; 3];
    capture_to_mono(
        format(1, Encoding::Pcm24),
        &[0, 0, 128, 255, 255, 127, 255, 255, 255],
        &mut output,
    )
    .unwrap();
    assert_eq!(output, [-1.0, 8388607.0 / 8388608.0, -1.0 / 8388608.0]);
}

#[test]
fn pcm8_midpoint_is_silence() {
    let mut output = [42; 4];
    mono_to_render(format(2, Encoding::Pcm8), &[0.0, -1.0], 1.0, &mut output).unwrap();
    assert_eq!(output, [128, 128, 0, 0]);
    let mut mono = [9.0; 2];
    capture_to_mono(format(1, Encoding::Pcm8), &[128, 255], &mut mono).unwrap();
    assert_eq!(mono, [0.0, 127.0 / 128.0]);
}

#[test]
fn render_limits_then_duplicates_channels() {
    let mut output = [0; 12];
    mono_to_render(
        format(2, Encoding::Pcm16),
        &[0.25, 1.0, -1.0],
        2.0,
        &mut output,
    )
    .unwrap();
    assert_eq!(output, [0, 64, 0, 64, 255, 127, 255, 127, 0, 128, 0, 128]);
}

#[test]
fn late_nonfinite_frame_silences_entire_destination() {
    for invalid in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        let mut rendered = [0x55; 8];
        assert_eq!(
            mono_to_render(
                format(2, Encoding::Pcm16),
                &[0.5, invalid],
                1.0,
                &mut rendered
            ),
            Err(FormatError::NonFinite)
        );
        assert_eq!(rendered, [0; 8]);
        let input = [0.5f32.to_le_bytes(), invalid.to_le_bytes()].concat();
        let mut captured = [0.75; 2];
        assert_eq!(
            capture_to_mono(format(1, Encoding::Float32), &input, &mut captured),
            Err(FormatError::NonFinite)
        );
        assert_eq!(captured, [0.0; 2]);
    }
}

#[test]
fn malformed_buffers_and_gain_are_silent() {
    let mut output = [77; 3];
    assert_eq!(
        mono_to_render(format(1, Encoding::Pcm8), &[1.0], 1.0, &mut output),
        Err(FormatError::BufferLength)
    );
    assert_eq!(output, [128; 3]);
    let mut output = [77; 2];
    for gain in [-0.1, 4.1, f32::NAN] {
        assert_eq!(
            mono_to_render(format(1, Encoding::Pcm16), &[1.0], gain, &mut output),
            Err(FormatError::Gain)
        );
        assert_eq!(output, [0; 2]);
    }
    let mut output = [9.0; 2];
    assert_eq!(
        capture_to_mono(format(1, Encoding::Pcm16), &[0, 0], &mut output),
        Err(FormatError::BufferLength)
    );
    assert_eq!(output, [0.0; 2]);
}

#[test]
fn float_downmix_large_finite_values_is_bounded() {
    let bytes = [f32::MAX.to_le_bytes(), f32::MAX.to_le_bytes()].concat();
    let mut output = [0.0];
    capture_to_mono(format(2, Encoding::Float32), &bytes, &mut output).unwrap();
    assert_eq!(output, [1.0]);
    let mut output = [0u8; 4];
    mono_to_render(format(1, Encoding::Float32), &[f32::MAX], 4.0, &mut output).unwrap();
    assert_eq!(f32::from_le_bytes(output), 1.0);
}

#[test]
fn signed_encodings_roundtrip_and_clamp_edges() {
    for encoding in [
        Encoding::Pcm16,
        Encoding::Pcm24,
        Encoding::Pcm32,
        Encoding::Float32,
    ] {
        let mut bytes = vec![0u8; 5 * encoding.bytes()];
        mono_to_render(
            format(1, encoding),
            &[-1.0, -0.5, 0.0, 0.5, 1.0],
            1.0,
            &mut bytes,
        )
        .unwrap();
        let mut decoded = [0.0; 5];
        capture_to_mono(format(1, encoding), &bytes, &mut decoded).unwrap();
        assert_eq!(&decoded[..4], &[-1.0, -0.5, 0.0, 0.5]);
        assert!((decoded[4] - 1.0).abs() <= 1.0 / 32768.0);
    }
}

#[test]
fn gain_zero_and_empty_buffers_are_safe() {
    let mut output = [99; 4];
    mono_to_render(format(2, Encoding::Pcm16), &[0.8], 0.0, &mut output).unwrap();
    assert_eq!(output, [0; 4]);
    mono_to_render(format(1, Encoding::Float32), &[], 1.0, &mut []).unwrap();
    capture_to_mono(format(1, Encoding::Float32), &[], &mut []).unwrap();
}
