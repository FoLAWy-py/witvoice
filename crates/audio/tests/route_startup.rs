#[path = "../examples/support/route_startup.rs"]
mod route_startup;
use route_startup::Startup;
use std::time::Duration;
use witvoice_audio::{
    realtime::{Binding, OutputGate},
    stream::{CapturePacket, DISCONTINUITY, TIMESTAMP_ERROR},
};
fn packet(index: u64, flags: u32) -> CapturePacket {
    CapturePacket {
        frames: 480,
        flags,
        device_position: index * 480,
        qpc_100ns: index * 100_000,
    }
}
#[test]
fn preparation_discards_first_and_requires_two_clean_packets_without_output_authority() {
    for first_flags in [0, DISCONTINUITY] {
        let mut startup = Startup::default();
        let mut gate = OutputGate::new(Binding::new(10, 1).unwrap()).unwrap();
        for i in 0..3 {
            startup
                .observe(
                    packet(i, if i == 0 { first_flags } else { 0 }),
                    Duration::from_millis(i * 10),
                )
                .unwrap();
            assert!(!gate.is_live());
            assert!(gate.begin_commit().is_none());
            assert_eq!(startup.complete, i == 2);
        }
        assert_eq!(startup.discarded_frames, 1440);
        assert_eq!(startup.discontinuity_packets, u32::from(first_flags != 0));
        gate.arm().unwrap();
        assert!(gate.is_live());
        assert!(
            startup
                .observe(packet(3, 0), Duration::from_millis(30))
                .is_err()
        );
    }
}
#[test]
fn timestamp_errors_and_any_later_discontinuity_are_rejected() {
    for first_flags in [TIMESTAMP_ERROR, DISCONTINUITY | TIMESTAMP_ERROR] {
        assert!(
            Startup::default()
                .observe(packet(0, first_flags), Duration::ZERO)
                .is_err()
        );
    }
    let mut startup = Startup::default();
    startup
        .observe(packet(0, DISCONTINUITY), Duration::ZERO)
        .unwrap();
    assert!(
        startup
            .observe(packet(1, DISCONTINUITY), Duration::from_millis(10))
            .is_err()
    );
    assert!(!startup.complete);
}
#[test]
fn preparation_rejects_position_gap_regression_overflow_and_capacity() {
    for second in [
        packet(2, 0),
        packet(0, 0),
        CapturePacket {
            qpc_100ns: 0,
            ..packet(1, 0)
        },
    ] {
        let mut startup = Startup::default();
        startup.observe(packet(0, 0), Duration::ZERO).unwrap();
        assert!(startup.observe(second, Duration::from_millis(10)).is_err());
    }
    for bad in [
        CapturePacket {
            frames: 1441,
            ..packet(0, 0)
        },
        CapturePacket {
            frames: 0,
            ..packet(0, 0)
        },
        CapturePacket {
            device_position: u64::MAX,
            ..packet(0, 0)
        },
    ] {
        assert!(Startup::default().observe(bad, Duration::ZERO).is_err());
    }
}
#[test]
fn preparation_does_not_renew_absolute_active_deadline() {
    let mut startup = Startup::default();
    startup
        .observe(packet(0, 0), Duration::from_millis(99))
        .unwrap();
    assert!(
        startup
            .observe(packet(1, 0), Duration::from_millis(100))
            .is_err()
    );
    assert!(!startup.complete);
}
