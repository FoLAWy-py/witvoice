//! Diagnostic-only synthesized marker; never model output or physical source.
pub const FRAMES: usize = 96_000;
pub const AMPLITUDE: f32 = 0.01;
pub fn remaining(
    active: std::time::Duration,
    process: std::time::Duration,
) -> Option<std::time::Duration> {
    if process >= std::time::Duration::from_secs(15) {
        return None;
    }
    std::time::Duration::from_secs(2)
        .checked_sub(active)
        .filter(|value| !value.is_zero())
}
/// Retained caller buffers are overwritten before free, including error paths.
/// Volatile prevents elimination; not a claim about OS or transient stack copies.
pub fn erase(output: &mut [f32]) {
    for value in output {
        unsafe { std::ptr::write_volatile(value, 0.0) };
    }
}
/// Deterministic low-level bipolar chips, bounded to two seconds at 48k.
pub fn marker(output: &mut [f32]) {
    let mut random = 0x6d2b_79f5u32;
    let mut sample = 0.0;
    for (index, value) in output.iter_mut().enumerate() {
        if index % 32 == 0 {
            random ^= random << 13;
            random ^= random >> 17;
            random ^= random << 5;
            sample = if random & 1 == 0 {
                -AMPLITUDE
            } else {
                AMPLITUDE
            };
        }
        *value = sample;
    }
}
#[derive(Debug, serde::Serialize)]
pub struct Match {
    pub correlation: f64,
    pub lag_samples: i32,
    pub capture_rms: f64,
    pub compared_samples: usize,
}
/// Fixed finite work after close, not a callback. Search +/-200ms; coarse then
/// refine +/-16 samples. Correlation is route continuity, not latency or VC quality.
pub fn correlate(expected: &[f32], captured: &[f32]) -> Option<Match> {
    if expected.len() > FRAMES
        || captured.len() > FRAMES
        || expected.len() < 30_000
        || captured.len() < 30_000
        || expected.iter().chain(captured).any(|v| !v.is_finite())
    {
        return None;
    }
    const START: usize = 12_000;
    const N: usize = 8_192;
    let rms = (captured[START..START + N]
        .iter()
        .map(|v| f64::from(*v).powi(2))
        .sum::<f64>()
        / N as f64)
        .sqrt();
    if !(0.0001..=0.05).contains(&rms) {
        return None;
    }
    let score = |lag: i32| {
        let begin = (START as i32 + lag) as usize;
        let (mut dot, mut left, mut right) = (0.0, 0.0, 0.0);
        for i in 0..N {
            let a = f64::from(expected[begin + i]);
            let b = f64::from(captured[START + i]);
            dot += a * b;
            left += a * a;
            right += b * b;
        }
        if left > 0.0 && right > 0.0 {
            dot / (left * right).sqrt()
        } else {
            0.0
        }
    };
    let mut best = (0.0, 0);
    for lag in (-9_600..=9_600).step_by(16) {
        let value = score(lag);
        if value > best.0 {
            best = (value, lag);
        }
    }
    for lag in (best.1 - 16).max(-9_600)..=(best.1 + 16).min(9_600) {
        let value = score(lag);
        if value > best.0 {
            best = (value, lag);
        }
    }
    Some(Match {
        correlation: best.0,
        lag_samples: best.1,
        capture_rms: rms,
        compared_samples: N,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn active_and_process_deadlines_forbid_another_packet_at_the_boundary() {
        use std::time::Duration;
        assert_eq!(
            remaining(Duration::from_millis(1999), Duration::from_secs(14)),
            Some(Duration::from_millis(1))
        );
        assert_eq!(
            remaining(Duration::from_secs(2), Duration::from_secs(3)),
            None
        );
        assert_eq!(
            remaining(Duration::from_secs(1), Duration::from_secs(15)),
            None
        );
        assert_eq!(remaining(Duration::from_secs(5), Duration::ZERO), None);
    }
    #[test]
    fn delayed_scaled_marker_matches_but_silence_and_unrelated_audio_do_not() {
        let mut expected = vec![0.0; FRAMES];
        marker(&mut expected);
        let mut captured = vec![0.0; FRAMES];
        for i in 137..FRAMES {
            captured[i] = expected[i - 137] * 0.5;
        }
        let matched = correlate(&expected, &captured).unwrap();
        assert!(matched.correlation > 0.99);
        assert_eq!(matched.lag_samples, -137);
        captured.fill(0.0);
        assert!(correlate(&expected, &captured).is_none());
        for (i, sample) in captured.iter_mut().enumerate() {
            *sample = (i as f32 * 0.019).sin() * 0.01;
        }
        assert!(correlate(&expected, &captured).unwrap().correlation < 0.6);
        captured[25_000] = f32::NAN;
        assert!(correlate(&expected, &captured).is_none());
        captured.fill(0.7);
        erase(&mut captured);
        assert!(captured.iter().all(|sample| *sample == 0.0));
        assert!(correlate(&expected[..29_999], &captured).is_none());
    }
}
