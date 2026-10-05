//! Scalar-only software diagnostics; no worker, model, or device APIs.
use std::{
    io,
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    Preflight,
    AuthenticateControl,
    AuthenticateMedia,
    WarmupWrite,
    HeartbeatWrite,
    HeartbeatRead,
    ReadyValidate,
    StopWrite,
    StopRead,
    OwnedCleanup,
}
impl Stage {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Preflight => "preflight",
            Self::AuthenticateControl => "authenticate_control",
            Self::AuthenticateMedia => "authenticate_media",
            Self::WarmupWrite => "warmup_write",
            Self::HeartbeatWrite => "heartbeat_write",
            Self::HeartbeatRead => "heartbeat_read",
            Self::ReadyValidate => "ready_validate",
            Self::StopWrite => "stop_write",
            Self::StopRead => "stop_read",
            Self::OwnedCleanup => "owned_cleanup",
        }
    }
}

pub struct IoDetail {
    pub code: &'static str,
    pub raw_os_error: Option<i32>,
    pub eof: bool,
}
pub fn classify_io(error: &io::Error) -> IoDetail {
    IoDetail {
        code: match error.kind() {
            io::ErrorKind::TimedOut => "ipc_deadline",
            io::ErrorKind::UnexpectedEof => "ipc_eof",
            _ => "ipc_failure",
        },
        raw_os_error: error.raw_os_error(),
        eof: error.kind() == io::ErrorKind::UnexpectedEof,
    }
}

/// One absolute horizon shared by write and all response reads. There is no
/// refresh operation: Ready processing cannot grant a fresh I/O duration.
pub struct RequestDeadline(Instant);
impl RequestDeadline {
    pub fn new(start: Instant, budget: Duration) -> Option<Self> {
        start.checked_add(budget).map(Self)
    }
    pub fn absolute(&self) -> Instant {
        self.0
    }
}

#[derive(Default)]
pub struct Timing {
    last_ack_ms: Option<u64>,
    pub max_successful_gap_ms: Option<u64>,
    pending_since_ms: Option<u64>,
}
impl Timing {
    pub fn begin_request(&mut self, now: u64) {
        self.pending_since_ms = Some(now);
    }
    pub fn complete_request(&mut self) {
        self.pending_since_ms = None;
    }
    pub fn acknowledge_heartbeat(&mut self, now: u64) {
        if let Some(last) = self.last_ack_ms {
            let gap = now.saturating_sub(last);
            self.max_successful_gap_ms = Some(self.max_successful_gap_ms.unwrap_or(0).max(gap));
        }
        self.last_ack_ms = Some(now);
        self.complete_request();
    }
    pub fn failure_age_ms(&self, now: u64) -> Option<u64> {
        self.last_ack_ms.map(|last| now.saturating_sub(last))
    }
    pub fn pending_elapsed_ms(&self, now: u64) -> Option<u64> {
        self.pending_since_ms.map(|start| now.saturating_sub(start))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    #[test]
    fn before_first_ack_age_and_gap_are_unknown() {
        let mut timing = Timing::default();
        timing.begin_request(50);
        assert_eq!(timing.failure_age_ms(500), None);
        assert_eq!(timing.max_successful_gap_ms, None);
        assert_eq!(timing.pending_elapsed_ms(500), Some(450));
        timing.acknowledge_heartbeat(500);
        assert_eq!(timing.max_successful_gap_ms, None);
        assert_eq!(timing.failure_age_ms(510), Some(10));
        assert_eq!(timing.pending_elapsed_ms(510), None);
    }

    #[test]
    fn failure_age_does_not_replace_successful_gap_or_request_elapsed() {
        let mut timing = Timing::default();
        timing.acknowledge_heartbeat(100);
        timing.acknowledge_heartbeat(215);
        timing.begin_request(315);
        assert_eq!(timing.failure_age_ms(715), Some(500));
        assert_eq!(timing.pending_elapsed_ms(715), Some(400));
        assert_eq!(timing.max_successful_gap_ms, Some(115));
    }

    #[test]
    fn saturated_and_regressed_clock_values_do_not_wrap() {
        let mut timing = Timing::default();
        timing.acknowledge_heartbeat(u64::MAX - 5);
        timing.begin_request(u64::MAX - 2);
        assert_eq!(timing.failure_age_ms(u64::MAX), Some(5));
        assert_eq!(timing.pending_elapsed_ms(u64::MAX), Some(2));
        assert_eq!(timing.failure_age_ms(0), Some(0));
        assert_eq!(timing.pending_elapsed_ms(0), Some(0));
        timing.acknowledge_heartbeat(0);
        assert_eq!(timing.max_successful_gap_ms, Some(0));
    }

    #[test]
    fn write_partial_read_and_async_ready_share_original_deadline() {
        let start = Instant::now();
        let pending = RequestDeadline::new(start, Duration::from_millis(400)).unwrap();
        let write_deadline = pending.absolute();
        let partial_read_deadline = pending.absolute();
        let ready_time = start + Duration::from_millis(390);
        let after_ready_read_deadline = pending.absolute();
        assert_eq!(write_deadline, partial_read_deadline);
        assert_eq!(write_deadline, after_ready_read_deadline);
        assert_eq!(
            after_ready_read_deadline.duration_since(ready_time),
            Duration::from_millis(10)
        );
        assert!(after_ready_read_deadline <= start + Duration::from_millis(400));
        let mut timing = Timing::default();
        timing.begin_request(0);
        // Ready is neither a heartbeat ACK nor a new request.
        assert_eq!(timing.pending_elapsed_ms(390), Some(390));
        assert_eq!(timing.failure_age_ms(390), None);
    }

    #[test]
    fn partial_header_and_partial_body_classify_eof_without_os_invention() {
        for (bytes, wanted) in [(vec![0, 0], 4), (vec![1, 2], 3)] {
            let mut reader = io::Cursor::new(bytes);
            let mut frame = vec![0; wanted];
            let error = reader.read_exact(&mut frame).unwrap_err();
            let detail = classify_io(&error);
            assert_eq!(detail.code, "ipc_eof");
            assert!(detail.eof);
            assert_eq!(detail.raw_os_error, None);
        }
    }

    #[test]
    fn partial_then_deadline_is_not_eof() {
        struct PartialDeadline(bool);
        impl Read for PartialDeadline {
            fn read(&mut self, target: &mut [u8]) -> io::Result<usize> {
                if !self.0 {
                    self.0 = true;
                    target[0] = 0;
                    Ok(1)
                } else {
                    Err(io::Error::from(io::ErrorKind::TimedOut))
                }
            }
        }
        let error = PartialDeadline(false).read_exact(&mut [0; 4]).unwrap_err();
        let detail = classify_io(&error);
        assert_eq!(detail.code, "ipc_deadline");
        assert!(!detail.eof);
        assert_eq!(detail.raw_os_error, None);
    }

    #[test]
    fn os_number_is_original_not_error_kind_or_guessed_thirteen() {
        let detail = classify_io(&io::Error::from_raw_os_error(5));
        assert_eq!(detail.raw_os_error, Some(5));
        assert_eq!(detail.code, "ipc_failure");
        assert!(!detail.eof);
    }
}
