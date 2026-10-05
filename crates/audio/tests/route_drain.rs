#[path = "../examples/support/route_drain.rs"]
mod route_drain;

use route_drain::{End, MAX_PACKET_FRAMES, MAX_PACKETS, MAX_PASS_FRAMES, Stats, drain};
use std::{cell::Cell, collections::VecDeque};
use witvoice_audio::stream::{CapturePacket, DISCONTINUITY, TIMESTAMP_ERROR};

fn clean(position: u64) -> CapturePacket {
    CapturePacket {
        frames: 480,
        flags: 0,
        device_position: position,
        qpc_100ns: position + 1,
    }
}

#[test]
fn an_observed_empty_buffer_allows_one_bounded_wait() {
    let mut stats = Stats::default();
    let mut packet = [0.25; MAX_PACKET_FRAMES];
    let mut calls = 0;
    let end = drain(
        &mut stats,
        &mut packet,
        100,
        || 1,
        || Ok(()),
        |storage| {
            calls += 1;
            storage[0] = 0.5;
            Ok(None)
        },
        |_, _| panic!("empty buffer must not deliver PCM"),
    )
    .unwrap();
    assert_eq!(end, End::Empty);
    assert!(end.may_wait());
    assert_eq!(calls, 1);
    assert_eq!(stats.observed_packets, 0);
    assert!(packet.iter().all(|sample| *sample == 0.0));
}

#[test]
fn one_scheduler_pass_drains_multiple_packets_without_another_event() {
    let mut source = VecDeque::from([Some(clean(0)), Some(clean(480)), None]);
    let mut delivered = Vec::new();
    let mut stats = Stats::default();
    let mut packet = [0.0; MAX_PACKET_FRAMES];
    let end = drain(
        &mut stats,
        &mut packet,
        100,
        || 1,
        || Ok(()),
        |storage| {
            storage.fill(0.125);
            Ok(source.pop_front().expect("only three reads needed"))
        },
        |meta, samples| {
            delivered.push(meta.device_position);
            assert_eq!(samples.len(), 480);
            assert!(samples.iter().all(|sample| *sample == 0.125));
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(end, End::Empty);
    assert_eq!(delivered, [0, 480]);
    assert_eq!(stats.read_calls, 3);
    assert_eq!(stats.accepted_frames, 960);
    assert!(packet.iter().all(|sample| *sample == 0.0));
}

#[test]
fn a_full_pass_yields_without_waiting_or_reading_the_fifth_packet() {
    let mut stats = Stats::default();
    let mut packet = [0.0; MAX_PACKET_FRAMES];
    let mut read_calls = 0;
    let end = drain(
        &mut stats,
        &mut packet,
        100,
        || 1,
        || Ok(()),
        |_| {
            read_calls += 1;
            Ok(Some(CapturePacket {
                frames: MAX_PACKET_FRAMES as u32,
                ..clean(0)
            }))
        },
        |_, _| Ok(()),
    )
    .unwrap();
    assert_eq!(end, End::Budget);
    assert!(!end.may_wait());
    assert_eq!(read_calls, MAX_PACKETS);
    assert_eq!(stats.maximum_frames_per_pass, MAX_PASS_FRAMES);
    assert_eq!(stats.budget_exhausted_passes, 1);
}

#[test]
fn next_pass_retrieves_the_unread_packet_and_keeps_the_same_deadline() {
    let mut source = VecDeque::from([
        Some(clean(0)),
        Some(clean(480)),
        Some(clean(960)),
        Some(clean(1440)),
        Some(clean(1920)),
        None,
    ]);
    let mut delivered = Vec::new();
    let mut stats = Stats::default();
    let mut packet = [0.0; MAX_PACKET_FRAMES];
    for (now, expected) in [(1, End::Budget), (3, End::Empty)] {
        assert_eq!(
            drain(
                &mut stats,
                &mut packet,
                4,
                || now,
                || Ok(()),
                |_| Ok(source.pop_front().unwrap()),
                |meta, _| {
                    delivered.push(meta.device_position);
                    Ok(())
                },
            )
            .unwrap(),
            expected
        );
    }
    assert_eq!(delivered, [0, 480, 960, 1440, 1920]);
    assert_eq!(stats.maximum_scheduler_gap_ns, 2);
    assert_eq!(
        drain(
            &mut stats,
            &mut packet,
            4,
            || 4,
            || panic!("deadline cannot be renewed"),
            |_| panic!("expired pass must not read"),
            |_, _| panic!("expired pass must not deliver"),
        )
        .unwrap(),
        End::Deadline
    );
}

#[test]
fn a_later_discontinuity_or_timestamp_error_is_not_hidden_by_drain() {
    for flag in [DISCONTINUITY, TIMESTAMP_ERROR] {
        let mut source = VecDeque::from([
            Some(clean(0)),
            Some(clean(480)),
            Some(CapturePacket {
                flags: flag,
                ..clean(960)
            }),
            Some(clean(1440)),
        ]);
        let mut stats = Stats::default();
        let mut packet = [0.0; MAX_PACKET_FRAMES];
        let mut delivered = Vec::new();
        let result = drain(
            &mut stats,
            &mut packet,
            100,
            || 1,
            || Ok(()),
            |storage| {
                storage.fill(0.5);
                Ok(source.pop_front().unwrap())
            },
            |meta, _| {
                delivered.push(meta.device_position);
                Ok(())
            },
        );
        assert_eq!(
            result.unwrap_err(),
            "capture discontinuity or invalid timestamp"
        );
        assert_eq!(delivered, [0, 480]);
        assert_eq!(stats.observed_packets, 3);
        assert_eq!(source.len(), 1);
        assert_eq!(stats.error_packet.as_ref().unwrap().flags, flag);
        assert!(packet.iter().all(|sample| *sample == 0.0));
    }
}

#[test]
fn a_packet_read_crossing_the_deadline_is_cleared_and_not_delivered() {
    let now = Cell::new(1);
    let mut stats = Stats::default();
    let mut packet = [0.0; MAX_PACKET_FRAMES];
    let end = drain(
        &mut stats,
        &mut packet,
        2,
        || now.get(),
        || Ok(()),
        |storage| {
            storage.fill(0.5);
            now.set(2);
            Ok(Some(clean(0)))
        },
        |_, _| panic!("late PCM cannot be delivered"),
    )
    .unwrap();
    assert_eq!(end, End::Deadline);
    assert!(!end.may_wait());
    assert_eq!(stats.deadline_discarded_frames, 480);
    assert_eq!(stats.accepted_packets, 0);
    assert!(packet.iter().all(|sample| *sample == 0.0));
}

#[test]
fn native_read_or_consumer_errors_erase_packet_storage() {
    for read_error in [true, false] {
        let mut stats = Stats::default();
        let mut packet = [0.0; MAX_PACKET_FRAMES];
        let result = drain(
            &mut stats,
            &mut packet,
            100,
            || 1,
            || Ok(()),
            |storage| {
                storage.fill(0.75);
                if read_error {
                    Err("native read error".into())
                } else {
                    Ok(Some(clean(0)))
                }
            },
            |_, _| Err("bounded consumer error".into()),
        );
        assert!(result.is_err());
        assert_eq!(stats.accepted_packets, 0);
        assert!(packet.iter().all(|sample| *sample == 0.0));
    }
}

#[test]
fn notification_dispatch_cannot_extend_the_absolute_deadline() {
    let now = Cell::new(1);
    let mut stats = Stats::default();
    let mut packet = [0.5; MAX_PACKET_FRAMES];
    let end = drain(
        &mut stats,
        &mut packet,
        2,
        || now.get(),
        || {
            now.set(2);
            Ok(())
        },
        |_| panic!("late notification dispatch cannot allow a packet read"),
        |_, _| panic!("deadline cannot allow PCM delivery"),
    )
    .unwrap();
    assert_eq!(end, End::Deadline);
    assert_eq!(stats.read_calls, 0);
    assert!(packet.iter().all(|sample| *sample == 0.0));
}

#[test]
fn malformed_packet_capacity_never_reaches_the_consumer() {
    for frames in [0, MAX_PACKET_FRAMES as u32 + 1] {
        let mut stats = Stats::default();
        let mut packet = [0.0; MAX_PACKET_FRAMES];
        let error = drain(
            &mut stats,
            &mut packet,
            100,
            || 1,
            || Ok(()),
            |storage| {
                storage.fill(0.5);
                Ok(Some(CapturePacket { frames, ..clean(0) }))
            },
            |_, _| panic!("malformed packet cannot be delivered"),
        )
        .unwrap_err();
        assert_eq!(error, "capture drain packet exceeds bounded capacity");
        assert_eq!(stats.accepted_packets, 0);
        assert!(packet.iter().all(|sample| *sample == 0.0));
    }
}

#[test]
fn a_regressing_clock_fails_closed_instead_of_increasing_remaining_time() {
    let now = Cell::new(2);
    let mut stats = Stats::default();
    let mut packet = [0.0; MAX_PACKET_FRAMES];
    let error = drain(
        &mut stats,
        &mut packet,
        3,
        || now.get(),
        || {
            now.set(1);
            Ok(())
        },
        |_| panic!("regressed clock cannot allow a packet read"),
        |_, _| panic!("regressed clock cannot allow PCM delivery"),
    )
    .unwrap_err();
    assert_eq!(error, "capture scheduler clock regressed");
    assert_eq!(stats.read_calls, 0);
    assert!(packet.iter().all(|sample| *sample == 0.0));
}
