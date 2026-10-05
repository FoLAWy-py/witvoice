//! Limited same-rate drift correction, not a general sample-rate converter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClockError {
    Configuration,
    OutsideRange,
    Counter,
}
#[derive(Debug, Clone, Copy)]
pub struct DriftClock {
    target: u32,
    ppm: f64,
    outside: u32,
    updates: u64,
}
impl DriftClock {
    pub fn new(target_frames: u32) -> Result<Self, ClockError> {
        if !(480..=1440).contains(&target_frames) {
            return Err(ClockError::Configuration);
        }
        Ok(Self {
            target: target_frames,
            ppm: 0.0,
            outside: 0,
            updates: 0,
        })
    }
    /// One bounded update per callback. No queue eviction as clock control.
    /// Occupancy includes current interpolation packet, not just ring slots.
    pub fn update(&mut self, queued_frames: u32) -> Result<f64, ClockError> {
        self.updates = self.updates.checked_add(1).ok_or(ClockError::Counter)?;
        let occupancy_error = (queued_frames as f64 - self.target as f64) / self.target as f64;
        // Keep a full packet plus interpolation lookahead under packetized
        // +/-500ppm input. Clipping proportional feedback at a packet boundary
        // is normal; a persistently doubled backlog is a configuration fault.
        let requested = occupancy_error * 5000.0;
        self.outside = if occupancy_error.abs() > 1.0 {
            self.outside.saturating_add(1)
        } else {
            0
        };
        if self.outside >= 100 {
            return Err(ClockError::OutsideRange);
        }
        self.ppm = requested.clamp(-1000.0, 1000.0);
        Ok(1.0 + self.ppm / 1_000_000.0)
    }
    pub fn ppm(&self) -> f64 {
        self.ppm
    }
    pub fn updates(&self) -> u64 {
        self.updates
    }
}
#[derive(Debug, Clone, Copy, Default)]
pub struct SamplePhase {
    fraction: f64,
    consumed: u64,
}
impl SamplePhase {
    /// Used both by the interpolator (one frame) and accelerated clock test
    /// (one callback). checked consumption never wraps the source timeline.
    pub fn advance(&mut self, frames: u32, ratio: f64) -> Result<u32, ClockError> {
        if !(1..=960).contains(&frames) {
            return Err(ClockError::Configuration);
        }
        if !ratio.is_finite() || !(0.999..=1.001).contains(&ratio) {
            return Err(ClockError::OutsideRange);
        }
        let position = self.fraction + frames as f64 * ratio;
        if !position.is_finite() || position.floor() > u32::MAX as f64 {
            return Err(ClockError::Counter);
        }
        let count = position.floor() as u32;
        self.consumed = self
            .consumed
            .checked_add(count as u64)
            .ok_or(ClockError::Counter)?;
        self.fraction = position - count as f64;
        Ok(count)
    }
    pub fn fraction(&self) -> f64 {
        self.fraction
    }
    pub fn consumed(&self) -> u64 {
        self.consumed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn phase_extreme_batch_and_counter_overflow_leave_state_unchanged() {
        let mut phase = SamplePhase::default();
        assert_eq!(
            phase.advance(u32::MAX, 1.001),
            Err(ClockError::Configuration)
        );
        assert_eq!(phase.consumed(), 0);
        assert_eq!(phase.fraction(), 0.0);
        let mut phase = SamplePhase {
            fraction: 0.5,
            consumed: u64::MAX,
        };
        assert_eq!(phase.advance(1, 1.0), Err(ClockError::Counter));
        assert_eq!(phase.consumed(), u64::MAX);
        assert_eq!(phase.fraction(), 0.5);
    }
}
