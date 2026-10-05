mod support;
use support::*;
use witvoice_audio::realtime::*;

#[test]
fn capture_identity_processed_real_playout_and_every_retirement_is_zero() {
    for action in ["mute", "stop", "exit", "fault", "drop"] {
        let (mut runtime, adapter) = fixture();
        running(&mut runtime);
        let gate = adapter.gate();
        let (rendered, result) = identity_render(&gate);
        result.unwrap();
        assert!(
            rendered
                .iter()
                .all(|sample| (*sample - 0.25).abs() < 0.00001)
        );
        match action {
            "mute" => ack(runtime.handle(&mute(3, 1, true)).unwrap()),
            "stop" => ack(runtime.handle(&stop(3)).unwrap()),
            "exit" => ack(runtime
                .handle(&request(3, serde_json::json!({"kind":"ExitNode"})))
                .unwrap()),
            "fault" => {
                let binding = runtime.test_binding().unwrap().clone();
                runtime.test_worker_failed(&binding).unwrap();
            }
            _ => drop(runtime),
        }
        assert_zero(&gate);
    }
}

#[test]
fn bounded_processed_queue_rejects_wrong_binding_expired_duplicates_future_and_overflow() {
    let (mut runtime, adapter) = fixture();
    running(&mut runtime);
    let gate = adapter.gate();
    let mut ring = Ring::new(8).unwrap();
    let binding = gate.binding();
    let block = |binding, start, arrival, deadline| {
        ProcessedBlock::from_model_result(binding, start, arrival, deadline, &[0.25; BLOCK_FRAMES])
            .unwrap()
    };
    {
        let (mut producer, _sink) =
            processed_endpoints(&mut ring, &gate, QueueConfig::output()).unwrap();
        assert_eq!(
            producer.push(block(Binding::new(18, 1).unwrap(), 0, 1000, 2000), 1000),
            Err(BlockError::Stale)
        );
        assert_eq!(
            producer.push(block(Binding::new(17, 0).unwrap(), 0, 1000, 2000), 1000),
            Err(BlockError::Stale)
        );
        assert_eq!(
            producer.push(block(binding, 0, 1000, 2000), 2000),
            Err(BlockError::Expired)
        );
    }
    // Use fresh queue after nonmonotonic time would independently be an error.
    let (mut producer, mut sink) =
        processed_endpoints(&mut ring, &gate, QueueConfig::output()).unwrap();
    for index in 0..8 {
        producer
            .push(
                block(binding, index * BLOCK_FRAMES as u64, 1000, 10_001_000),
                1000,
            )
            .unwrap();
    }
    assert_eq!(
        producer.push(block(binding, 0, 1000, 10_001_000), 1000),
        Err(BlockError::Stale)
    );
    assert_eq!(
        producer.push(
            block(binding, 8 * BLOCK_FRAMES as u64, 1000, 10_001_000),
            1000
        ),
        Err(BlockError::Capacity)
    );
    let mut rendered = [99.0; BLOCK_FRAMES];
    assert!(sink.render(&mut rendered, 10_001_000).is_err());
    assert!(rendered.iter().all(|sample| *sample == 0.0));
    assert!(sink.counters().expired_blocks > 0);
}

#[test]
fn future_arrival_fault_cannot_open_a_source_bypass() {
    let (mut runtime, adapter) = fixture();
    running(&mut runtime);
    let gate = adapter.gate();
    let mut ring = Ring::new(8).unwrap();
    let (mut producer, mut sink) =
        processed_endpoints(&mut ring, &gate, QueueConfig::output()).unwrap();
    let block =
        ProcessedBlock::from_model_result(gate.binding(), 0, 2000, 3000, &[0.75; BLOCK_FRAMES])
            .unwrap();
    assert_eq!(producer.push(block, 1000), Err(BlockError::Timestamp));
    assert!(gate.is_faulted());
    let mut rendered = [99.0; BLOCK_FRAMES];
    assert_eq!(sink.render(&mut rendered, 1000), Err(BlockError::Muted));
    assert!(rendered.iter().all(|sample| *sample == 0.0));
}
