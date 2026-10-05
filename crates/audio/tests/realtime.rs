use std::{
    sync::{
        Barrier,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Instant,
};
use witvoice_audio::realtime::*;

fn binding() -> Binding {
    Binding::new(17, 3).unwrap()
}
fn processed(start: u64, arrival: u64) -> ProcessedBlock {
    ProcessedBlock::from_model_result(
        binding(),
        start,
        arrival,
        arrival + 60_000_000,
        &[0.25; 480],
    )
    .unwrap()
}
#[test]
fn concurrent_spsc_non_power_of_two_wrap_order_and_reuse() {
    let mut ring = Ring::<u64>::new(7).unwrap();
    let (mut producer, mut consumer) = ring.split();
    thread::scope(|scope| {
        scope.spawn(move || {
            for i in 0..100_000 {
                while producer.try_push(i).is_err() {
                    thread::yield_now();
                }
            }
        });
        for i in 0..100_000 {
            let value = loop {
                if let Some(v) = consumer.try_pop() {
                    break v;
                }
                thread::yield_now();
            };
            assert_eq!(value, i);
            assert!(consumer.queued() <= 7);
        }
    });
    let (mut producer, mut consumer) = ring.split();
    for i in 0..7 {
        producer.try_push(i).unwrap();
    }
    assert_eq!(producer.try_push(99), Err(RingError::Full));
    assert_eq!(consumer.try_pop(), Some(0));
    producer.try_push(7).unwrap();
    for i in 1..8 {
        assert_eq!(consumer.try_pop(), Some(i));
    }
    assert_eq!(consumer.try_pop(), None);
}
#[test]
fn invalid_queue_config_and_checked_sample_interval_fail_closed() {
    assert!(Ring::<u64>::new(0).is_err());
    assert!(Ring::<u64>::new(65).is_err());
    assert!(Binding::new(0, 1).is_err());
    assert!(
        ProcessedBlock::from_model_result(binding(), u64::MAX - 479, 0, 1, &[0.1; 480]).is_err()
    );
    assert!(ProcessedBlock::from_model_result(binding(), 0, 0, 1, &[0.1; 240]).is_err());
    assert!(ProcessedBlock::from_model_result(binding(), 0, 0, 1, &[f32::INFINITY; 480]).is_err());
    assert!(ProcessedBlock::from_model_result(binding(), 0, 2, 2, &[0.1; 480]).is_err());
    assert!(ProcessedBlock::from_model_result(binding(), 0, 0, u64::MAX, &[0.1; 480]).is_err());
    let mut ring = Ring::<CaptureBlock>::new(4).unwrap();
    let mut cfg = QueueConfig::capture();
    cfg.target_frames = 1440;
    assert!(capture_endpoints(&mut ring, binding(), cfg).is_err());
    cfg = QueueConfig::capture();
    cfg.maximum_age_ns += 1;
    assert!(capture_endpoints(&mut ring, binding(), cfg).is_err());
}
#[test]
fn capture_deadline_equals_now_erases_tail_and_counts_expired_not_underflow() {
    let mut ring = Ring::<CaptureBlock>::new(4).unwrap();
    let (mut source, mut worker) =
        capture_endpoints(&mut ring, binding(), QueueConfig::capture()).unwrap();
    let packet = CaptureBlock::from_capture(binding(), 0, 0, 20_000_000, &[0.8; 480]).unwrap();
    source.push(packet, 0).unwrap();
    let mut output = [0.9; 960];
    assert_eq!(
        worker.read(&mut output, 20_000_000),
        Err(BlockError::Expired)
    );
    assert_eq!(output, [0.0; 960]);
    assert_eq!(worker.counters().expired_blocks, 1);
    assert_eq!(worker.counters().underflow_frames, 0);
    assert_eq!(
        worker.read(&mut output, 20_000_001),
        Err(BlockError::Underflow)
    );
    assert_eq!(worker.counters().capture_empty_reads, 1);
    assert_eq!(worker.counters().underflow_frames, 0);
    assert_eq!(output, [0.0; 960]);
}
#[test]
fn rejected_overflow_duplicate_and_foreign_epoch_preserve_live_source_order() {
    let mut gate = OutputGate::new(binding()).unwrap();
    gate.arm().unwrap();
    let mut ring = Ring::<ProcessedBlock>::new(8).unwrap();
    let (mut writer, mut sink) =
        processed_endpoints(&mut ring, &gate, QueueConfig::output()).unwrap();
    for i in 0..8 {
        writer.push(processed(i * 480, 0), 0).unwrap();
    }
    assert_eq!(
        writer.push(processed(3840, 0), 0),
        Err(BlockError::Capacity)
    );
    assert_eq!(writer.push(processed(0, 0), 0), Err(BlockError::Stale));
    let other = ProcessedBlock::from_model_result(
        Binding::new(17, 4).unwrap(),
        3840,
        0,
        60_000_000,
        &[0.9; 480],
    )
    .unwrap();
    assert_eq!(writer.push(other, 0), Err(BlockError::Stale));
    let mut out = [0.0; 480];
    sink.render(&mut out, 0).unwrap();
    assert!(out.iter().all(|v| (*v - 0.25).abs() < 1e-6));
    assert_eq!(writer.counters().overflow_blocks, 1);
    assert_eq!(writer.counters().stale_blocks, 2);
    assert!(sink.source_position().unwrap() >= 478);
}
#[test]
fn backward_arrival_clock_and_future_timestamp_fault_requires_new_gate() {
    let mut gate = OutputGate::new(binding()).unwrap();
    gate.arm().unwrap();
    let mut ring = Ring::<ProcessedBlock>::new(8).unwrap();
    let (mut writer, mut sink) =
        processed_endpoints(&mut ring, &gate, QueueConfig::output()).unwrap();
    writer.push(processed(0, 20), 20).unwrap();
    assert_eq!(
        writer.push(processed(480, 10), 10),
        Err(BlockError::Timestamp)
    );
    assert!(gate.is_faulted());
    let mut out = [0.9; 480];
    assert_eq!(sink.render(&mut out, 21), Err(BlockError::Muted));
    assert_eq!(out, [0.0; 480]);
    assert_eq!(sink.counters().fault_silence_frames, 480);
    assert!(gate.arm().is_err());
    let mut fresh = OutputGate::new(Binding::new(17, 4).unwrap()).unwrap();
    fresh.arm().unwrap();
    let mut fresh_ring = Ring::<ProcessedBlock>::new(8).unwrap();
    let (mut writer, _) =
        processed_endpoints(&mut fresh_ring, &fresh, QueueConfig::output()).unwrap();
    let future =
        ProcessedBlock::from_model_result(fresh.binding(), 0, 100, 101, &[0.5; 480]).unwrap();
    assert_eq!(writer.push(future, 99), Err(BlockError::Timestamp));
}
#[test]
fn starvation_and_partial_callback_erase_are_separate_from_planned_silence() {
    let mut gate = OutputGate::new(binding()).unwrap();
    gate.arm().unwrap();
    let mut ring = Ring::<ProcessedBlock>::new(8).unwrap();
    let (mut writer, mut sink) =
        processed_endpoints(&mut ring, &gate, QueueConfig::output()).unwrap();
    writer.push(processed(0, 0), 0).unwrap();
    let mut out = [0.9; 960];
    assert_eq!(sink.render(&mut out, 0), Err(BlockError::Underflow));
    assert_eq!(out, [0.0; 960]); // partial converted data erased, not replayed
    assert_eq!(sink.counters().underflow_frames, 960);
    assert_eq!(sink.counters().assembled_frames, 0);
    assert_eq!(sink.counters().planned_silence_frames, 0);
    assert_eq!(sink.render(&mut out[..480], 0), Err(BlockError::Underflow));
    assert!(gate.is_faulted());
    assert_eq!(sink.counters().underflow_callbacks, 2);
    assert_eq!(sink.counters().fault_silence_frames, 1440);
    assert_eq!(sink.render(&mut out, 0), Err(BlockError::Muted));
    assert_eq!(sink.counters().underflow_callbacks, 2);
    let muted = OutputGate::new(binding()).unwrap();
    let mut other = Ring::<ProcessedBlock>::new(8).unwrap();
    let (_, mut idle) = processed_endpoints(&mut other, &muted, QueueConfig::output()).unwrap();
    idle.render(&mut out[..480], 0).unwrap_err();
    assert_eq!(idle.counters().planned_silence_frames, 480);
    assert_eq!(idle.counters().underflow_frames, 0);
}
#[test]
fn final_requested_sample_does_not_prefetch_future_callback_or_repeat_on_starvation() {
    let mut gate = OutputGate::new(binding()).unwrap();
    gate.arm().unwrap();
    let mut ring = Ring::<ProcessedBlock>::new(8).unwrap();
    let (mut writer, mut sink) =
        processed_endpoints(&mut ring, &gate, QueueConfig::output()).unwrap();
    writer.push(processed(0, 0), 0).unwrap();
    let mut out = [0.0; 480];
    sink.render(&mut out, 0).unwrap();
    assert_eq!(out, [0.25; 480]);
    assert_eq!(sink.counters().underflow_callbacks, 0);
    assert_eq!(
        sink.render(&mut out, 10_000_000),
        Err(BlockError::Underflow)
    );
    assert_eq!(out, [0.0; 480]);
    assert_eq!(sink.counters().underflow_frames, 480);
}
#[test]
fn packet_expiry_source_gap_and_commit_expiry_all_fail_silent() {
    for gap in [false, true] {
        let mut gate = OutputGate::new(binding()).unwrap();
        gate.arm().unwrap();
        let mut ring = Ring::<ProcessedBlock>::new(8).unwrap();
        let (mut writer, mut sink) =
            processed_endpoints(&mut ring, &gate, QueueConfig::output()).unwrap();
        writer.push(processed(0, 0), 0).unwrap();
        writer
            .push(processed(if gap { 960 } else { 480 }, 0), 0)
            .unwrap();
        let mut out = [0.9; 481];
        assert_eq!(
            sink.render(&mut out, if gap { 0 } else { 60_000_000 }),
            Err(if gap {
                BlockError::Interval
            } else {
                BlockError::Expired
            })
        );
        assert_eq!(out, [0.0; 481]);
        assert!(gate.is_faulted());
    }
    let mut gate = OutputGate::new(binding()).unwrap();
    gate.arm().unwrap();
    let mut ring = Ring::<ProcessedBlock>::new(8).unwrap();
    let (mut writer, mut sink) =
        processed_endpoints(&mut ring, &gate, QueueConfig::output()).unwrap();
    writer.push(processed(0, 0), 0).unwrap();
    writer.push(processed(480, 0), 0).unwrap();
    let mut out = [0.0; 480];
    sink.render(&mut out, 1).unwrap();
    assert!(!sink.commit_valid(0));
    assert!(sink.commit_valid(59_999_999));
    assert!(!sink.commit_valid(60_000_000));
    assert_eq!(sink.counters().sink_pcm_frames, 0); // assembly is not native commit
}
#[test]
fn mute_ticket_ack_never_waits_on_slow_cleanup_or_allows_new_old_commits() {
    let mut gate = OutputGate::new(binding()).unwrap();
    gate.arm().unwrap();
    let barrier = Barrier::new(2);
    let released = AtomicBool::new(false);
    thread::scope(|scope| {
        scope.spawn(|| {
            let ticket = gate.begin_commit().unwrap();
            barrier.wait(); // injected pause before the sink commit
            barrier.wait();
            assert!(!ticket.is_live());
            assert!(!gate.ack_ready());
            drop(ticket);
            released.store(true, Ordering::Release);
        });
        barrier.wait();
        gate.invalidate(); // before simulated slow worker cancellation/join
        assert!(!gate.is_live());
        assert!(!gate.ack_ready());
        assert!(gate.begin_commit().is_none());
        barrier.wait();
        let limit = Instant::now() + std::time::Duration::from_secs(5);
        while !released.load(Ordering::Acquire) {
            assert!(Instant::now() < limit);
            thread::yield_now();
        }
        assert!(gate.ack_ready());
        assert!(gate.begin_commit().is_none());
    });
    assert!(gate.arm().is_err());
}
#[test]
fn disabled_or_disconnected_monitor_has_no_virtual_queue_or_clock_dependency() {
    let mut virtual_gate = OutputGate::new(binding()).unwrap();
    virtual_gate.arm().unwrap();
    let monitor_gate = OutputGate::new(binding()).unwrap();
    let mut virtual_ring = Ring::<ProcessedBlock>::new(8).unwrap();
    let mut monitor_ring = Ring::<ProcessedBlock>::new(8).unwrap();
    let (mut writer, mut virtual_out) =
        processed_endpoints(&mut virtual_ring, &virtual_gate, QueueConfig::output()).unwrap();
    let (mut monitor_writer, mut monitor_out) =
        processed_endpoints(&mut monitor_ring, &monitor_gate, QueueConfig::output()).unwrap();
    assert_eq!(
        monitor_writer.push(processed(0, 0), 0),
        Err(BlockError::Muted)
    );
    writer.push(processed(0, 0), 0).unwrap();
    writer.push(processed(480, 0), 0).unwrap();
    monitor_gate.fail();
    let mut a = [0.9; 480];
    let mut b = [0.9; 480];
    assert_eq!(monitor_out.render(&mut b, 0), Err(BlockError::Muted));
    virtual_out.render(&mut a, 0).unwrap();
    assert_eq!(b, [0.0; 480]);
    assert_eq!(a, [0.25; 480]);
    assert!(virtual_gate.is_live());
    assert_eq!(
        virtual_out.correction_ppm(),
        (960.0 - 984.0) / 984.0 * 5000.0
    );
    assert_eq!(monitor_out.source_position(), None);
}
#[test]
fn sustained_correction_outside_range_retires_clock_without_widening() {
    let mut clock = DriftClock::new(960).unwrap();
    for _ in 0..99 {
        assert_eq!(clock.update(4000).unwrap(), 1.001);
    }
    assert_eq!(clock.update(4000), Err(ClockError::OutsideRange));
    assert_eq!(clock.ppm(), 1000.0);
    assert!(SamplePhase::default().advance(480, 1.0011).is_err());
    assert!(SamplePhase::default().advance(480, f64::NAN).is_err());
}
#[test]
fn phase_packet_advance_agrees_with_per_sample_interpolation() {
    for ratio in [0.999, 0.9995, 1.0, 1.0005, 1.001] {
        let mut packet = SamplePhase::default();
        let mut samples = SamplePhase::default();
        for _ in 0..1000 {
            packet.advance(480, ratio).unwrap();
            for _ in 0..480 {
                samples.advance(1, ratio).unwrap();
            }
        }
        assert!((packet.consumed() as i64 - samples.consumed() as i64).abs() <= 1);
        let position = packet.consumed() as f64 + packet.fraction();
        let per_sample = samples.consumed() as f64 + samples.fraction();
        assert!((position - per_sample).abs() < 1e-5);
    }
}
#[test]
fn interpolation_preserves_ramp_and_source_indices_across_packet_boundary() {
    let mut gate = OutputGate::new(binding()).unwrap();
    gate.arm().unwrap();
    let mut ring = Ring::<ProcessedBlock>::new(8).unwrap();
    let (mut writer, mut sink) =
        processed_endpoints(&mut ring, &gate, QueueConfig::output()).unwrap();
    let mut samples = [0.0; 480];
    for start in [0, 480] {
        for (i, s) in samples.iter_mut().enumerate() {
            *s = (start + i) as f32 / 2000.0;
        }
        writer
            .push(
                ProcessedBlock::from_model_result(binding(), start as u64, 0, 60_000_000, &samples)
                    .unwrap(),
                0,
            )
            .unwrap();
    }
    let mut out = [0.0; 480];
    sink.render(&mut out, 0).unwrap();
    let ratio1 = 1.0 + sink.correction_ppm() / 1_000_000.0;
    for (i, s) in out.iter().enumerate() {
        assert!((*s as f64 - i as f64 * ratio1 / 2000.0).abs() < 1e-6);
    }
    let first = sink.source_position().unwrap();
    for (i, s) in samples.iter_mut().enumerate() {
        *s = (960 + i) as f32 / 2000.0;
    }
    writer
        .push(
            ProcessedBlock::from_model_result(binding(), 960, 10_000_000, 70_000_000, &samples)
                .unwrap(),
            10_000_000,
        )
        .unwrap();
    sink.render(&mut out, 10_000_000).unwrap();
    let ratio2 = 1.0 + sink.correction_ppm() / 1_000_000.0;
    for (i, s) in out.iter().enumerate() {
        assert!((*s as f64 - (480.0 * ratio1 + i as f64 * ratio2) / 2000.0).abs() < 1e-6);
    }
    assert!(sink.source_position().unwrap() > first);
}
#[test]
fn four_eight_hour_synthetic_clocks_keep_occupancy_and_source_monotonic() {
    #[derive(Clone, Copy)]
    struct SimBlock {
        start: u64,
        arrival: u64,
        deadline: u64,
    }
    let start = Instant::now();
    for ppm in [-500.0, -150.0, 150.0, 500.0] {
        let wall = Instant::now();
        let mut clock = DriftClock::new(QueueConfig::output().target_frames).unwrap();
        let mut phase = SamplePhase::default();
        let mut ring = Ring::<SimBlock>::new(8).unwrap();
        let (mut producer, mut consumer) = ring.split();
        let mut produced = 0_u64;
        // 30ms finite startup inventory, within the stated 10-30ms output
        // range. No callback waits for capacity or for this inventory to fill.
        for _ in 0..3 {
            producer
                .try_push(SimBlock {
                    start: produced,
                    arrival: 0,
                    deadline: 60_000_000,
                })
                .unwrap();
            produced = produced.checked_add(480).unwrap();
        }
        let mut credit = 0.0_f64;
        let mut current: Option<SimBlock> = None;
        let mut offset = 0_u32;
        let mut cursor = 0_u64;
        let mut maximum_age = 0_u64;
        let mut maxq = 0.0_f64;
        let mut minq = f64::MAX;
        let mut maxppm = 0.0_f64;
        let mut last = 0;
        let callbacks = 2_880_000_u64; // 10ms x count = 8h
        for step in 0..callbacks {
            let now = step.checked_mul(10_000_000).unwrap();
            if step != 0 {
                credit += 480.0 * (1.0 + ppm / 1_000_000.0);
            }
            for _ in 0..2 {
                if credit < 480.0 {
                    break;
                }
                producer
                    .try_push(SimBlock {
                        start: produced,
                        arrival: now,
                        deadline: now.checked_add(60_000_000).unwrap(),
                    })
                    .unwrap();
                produced = produced.checked_add(480).unwrap();
                credit -= 480.0;
            }
            assert!(credit < 480.0);
            let available =
                (consumer.queued() * 480) as u32 + if current.is_some() { 480 - offset } else { 0 };
            let ratio = clock.update(available).unwrap();
            let mut used = phase.advance(480, ratio).unwrap();
            for _ in 0..3 {
                // bounded, at most two packet crossings
                if used == 0 {
                    break;
                }
                if current.is_none() {
                    current = Some(consumer.try_pop().expect("synthetic underflow"));
                    offset = 0;
                }
                let block = current.unwrap();
                assert!(block.arrival <= now && now < block.deadline);
                maximum_age = maximum_age.max(now - block.arrival);
                assert_eq!(block.start + offset as u64, cursor);
                let take = used.min(480 - offset);
                used -= take;
                offset += take;
                cursor = cursor.checked_add(take as u64).unwrap();
                if offset == 480 {
                    current = None;
                }
            }
            assert_eq!(used, 0);
            assert_eq!(phase.consumed(), cursor);
            assert!(phase.consumed() >= last);
            last = phase.consumed();
            assert!((1..=3840).contains(&available));
            let after =
                (consumer.queued() * 480) as u32 + if current.is_some() { 480 - offset } else { 0 };
            assert!(after >= 2, "interpolation lookahead must remain available");
            assert!(clock.ppm().abs() <= 1000.0);
            minq = minq.min(available as f64);
            maxq = maxq.max(available as f64);
            maxppm = maxppm.max(clock.ppm().abs());
        }
        let measured_ppm = (phase.consumed() as f64 / (callbacks * 480) as f64 - 1.0) * 1_000_000.0;
        assert!((measured_ppm - ppm).abs() < 1.0);
        println!(
            "CLOCK_B {{\"model\":\"packetized_spsc_metadata\",\"drift_ppm\":{ppm},\"measured_consumption_ppm\":{measured_ppm},\"simulated_seconds\":28800,\"callbacks\":{callbacks},\"output_frames\":{},\"source_produced\":{produced},\"source_consumed\":{},\"source_fraction\":{},\"min_occupancy_frames\":{minq},\"max_occupancy_frames\":{maxq},\"max_age_ns\":{maximum_age},\"underflows\":0,\"overflows\":0,\"expired\":0,\"max_correction_ppm\":{maxppm},\"final_correction_ppm\":{},\"wall_seconds\":{},\"hardware\":\"NOT_RUN\"}}",
            callbacks * 480,
            phase.consumed(),
            phase.fraction(),
            clock.ppm(),
            wall.elapsed().as_secs_f64()
        );
    }
    println!("CLOCK_ALL_WALL_SECONDS={}", start.elapsed().as_secs_f64());
}
