//! Only integration-test targets compile this synthetic audio adapter/IdentityEngine.
#![allow(dead_code)]
use std::sync::{Arc, Mutex};
use witvoice_audio::realtime::*;
use witvoice_contracts::{
    ErrorCode,
    control::{Outcome, Response},
    values::Id,
};
use witvoice_session::{
    Runtime,
    test_support::{Binding as SessionBinding, Lifecycle, Retirement},
};

pub const SESSION: &str = "00000000-0000-0000-0000-000000000abc";
pub fn session_id() -> Id {
    SESSION.to_owned().try_into().unwrap()
}
#[derive(Default)]
struct Resources {
    binding: Option<SessionBinding>,
    gate: Option<Arc<OutputGate>>,
    retirements: usize,
}
#[derive(Clone, Default)]
pub struct AudioAdapter(Arc<Mutex<Resources>>);
impl AudioAdapter {
    pub fn gate(&self) -> Arc<OutputGate> {
        self.0.lock().unwrap().gate.as_ref().unwrap().clone()
    }
    pub fn retirements(&self) -> usize {
        self.0.lock().unwrap().retirements
    }
}
impl Lifecycle for AudioAdapter {
    fn prepare(&mut self, binding: &SessionBinding) -> Result<(), ErrorCode> {
        let mut resources = self.0.lock().unwrap();
        if resources
            .gate
            .as_ref()
            .is_some_and(|gate| !gate.ack_ready())
        {
            return Err(ErrorCode::Busy);
        }
        let gate = OutputGate::new(
            Binding::new(binding.session_tag, binding.epoch)
                .map_err(|_| ErrorCode::InvalidArgument)?,
        )
        .map_err(|_| ErrorCode::InvalidArgument)?;
        resources.gate = Some(Arc::new(gate));
        resources.binding = Some(binding.clone());
        Ok(())
    }
    fn ready(&self, binding: &SessionBinding) -> bool {
        let resources = self.0.lock().unwrap();
        resources.binding.as_ref() == Some(binding)
            && resources
                .gate
                .as_ref()
                .is_some_and(|gate| !gate.is_faulted())
    }
    fn activate(&mut self, binding: &SessionBinding) -> Result<(), ErrorCode> {
        if !self.ready(binding) {
            return Err(ErrorCode::EngineNotReady);
        }
        let mut resources = self.0.lock().unwrap();
        Arc::get_mut(resources.gate.as_mut().unwrap())
            .ok_or(ErrorCode::Busy)?
            .arm()
            .map_err(|_| ErrorCode::EngineNotReady)
    }
    fn invalidate_first(&mut self, reason: Retirement) {
        let mut resources = self.0.lock().unwrap();
        if let Some(gate) = &resources.gate {
            if reason == Retirement::Fault {
                gate.fail();
            } else {
                gate.invalidate();
            }
            resources.retirements += 1;
        }
    }
    fn ack_ready(&self) -> bool {
        self.0
            .lock()
            .unwrap()
            .gate
            .as_ref()
            .is_none_or(|gate| gate.ack_ready())
    }
    fn output_is_muted(&self) -> bool {
        self.0
            .lock()
            .unwrap()
            .gate
            .as_ref()
            .is_none_or(|gate| !gate.is_live())
    }
}
pub fn fixture() -> (Runtime, AudioAdapter) {
    let adapter = AudioAdapter::default();
    let runtime = Runtime::with_test_support(session_id(), 17, Box::new(adapter.clone())).unwrap();
    (runtime, adapter)
}
pub fn request(id: usize, command: serde_json::Value) -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({"protocol_version":1,"request_id":format!("00000000-0000-0000-0000-{id:012x}"),"command":command})).unwrap()
}
pub fn prepare(id: usize) -> Vec<u8> {
    request(
        id,
        serde_json::json!({"kind":"PrepareSession","args":{"input_device_id":"synthetic","output_device_id":"software-sink","voice_id":SESSION,"route":{"kind":"Local"}}}),
    )
}
pub fn start(id: usize, epoch: u32) -> Vec<u8> {
    request(
        id,
        serde_json::json!({"kind":"StartSession","args":{"session_id":SESSION,"epoch":epoch}}),
    )
}
pub fn mute(id: usize, epoch: u32, muted: bool) -> Vec<u8> {
    request(
        id,
        serde_json::json!({"kind":"SetMute","args":{"session_id":SESSION,"epoch":epoch,"muted":muted}}),
    )
}
pub fn stop(id: usize) -> Vec<u8> {
    request(
        id,
        serde_json::json!({"kind":"StopSession","args":{"session_id":SESSION}}),
    )
}
pub fn ack(response: Response) {
    assert!(
        matches!(response.outcome, Outcome::Ack),
        "{:?}",
        response.outcome
    );
}
pub fn code(response: Response) -> ErrorCode {
    match response.outcome {
        Outcome::Error { code, .. } => code,
        other => panic!("expected error: {other:?}"),
    }
}
pub fn running(runtime: &mut Runtime) {
    ack(runtime.handle(&prepare(1)).unwrap());
    ack(runtime.handle(&start(2, runtime.epoch())).unwrap());
}

/// This source-to-processed copy exists solely in test targets, never in a library capability.
pub struct IdentityEngine;
impl IdentityEngine {
    pub fn step(
        input: &mut CaptureConsumer<'_>,
        output: &mut ProcessedProducer<'_>,
        binding: Binding,
        now: u64,
    ) -> Result<(), BlockError> {
        let mut samples = [0.0; BLOCK_FRAMES];
        let (source, count) = input.read(&mut samples, now)?;
        if count != BLOCK_FRAMES {
            return Err(BlockError::Interval);
        }
        output.push(
            ProcessedBlock::from_model_result(binding, source, now, now + 10_000_000, &samples)?,
            now,
        )
    }
}
pub fn identity_render(gate: &OutputGate) -> ([f32; BLOCK_FRAMES], Result<(), BlockError>) {
    let mut capture = Ring::new(4).unwrap();
    let mut processed = Ring::new(8).unwrap();
    let binding = gate.binding();
    let (mut source, mut worker) =
        capture_endpoints(&mut capture, binding, QueueConfig::capture()).unwrap();
    let (mut output, mut sink) =
        processed_endpoints(&mut processed, gate, QueueConfig::output()).unwrap();
    for index in 0..4 {
        source
            .push(
                CaptureBlock::from_capture(
                    binding,
                    index * BLOCK_FRAMES as u64,
                    1000,
                    10_001_000,
                    &[0.25; BLOCK_FRAMES],
                )
                .unwrap(),
                1000,
            )
            .unwrap();
        let result = IdentityEngine::step(&mut worker, &mut output, binding, 1000);
        if gate.is_live() {
            result.unwrap();
        } else {
            assert_eq!(result, Err(BlockError::Muted));
        }
    }
    let mut rendered = [99.0; BLOCK_FRAMES];
    let result = sink.render(&mut rendered, 1000);
    (rendered, result)
}
pub fn assert_zero(gate: &OutputGate) {
    let (rendered, result) = identity_render(gate);
    assert_eq!(result, Err(BlockError::Muted));
    assert!(rendered.iter().all(|sample| *sample == 0.0));
}
