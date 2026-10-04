use witvoice_contracts::{
    ErrorCode,
    control::{Outcome, Response},
    state::SessionState,
};
use witvoice_session::{REQUEST_HISTORY_CAPACITY, Runtime};

fn request(number: usize, command: serde_json::Value) -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({"protocol_version":1,
        "request_id":format!("00000000-0000-0000-0000-{number:012x}"),"command":command}))
    .unwrap()
}
fn prepare(number: usize) -> Vec<u8> {
    request(
        number,
        serde_json::json!({"kind":"PrepareSession","args":{
        "input_device_id":"input","output_device_id":"output",
        "voice_id":"00000000-0000-0000-0000-000000000abc","route":{"kind":"Local"}}}),
    )
}
fn stop(number: usize) -> Vec<u8> {
    request(
        number,
        serde_json::json!({"kind":"StopSession","args":{
        "session_id":"00000000-0000-0000-0000-000000000abc"}}),
    )
}
fn code(response: &Response) -> ErrorCode {
    let Outcome::Error { code, .. } = response.outcome else {
        panic!("expected error");
    };
    code
}

#[test]
fn missing_resources_cannot_advertise_ready_or_start_audio() {
    let mut runtime = Runtime::default();
    assert_eq!(
        code(&runtime.handle(&prepare(1)).unwrap()),
        ErrorCode::EngineNotReady
    );
    assert_eq!(runtime.state(), SessionState::Blocked);
    assert_eq!(runtime.epoch(), 1);
    assert_eq!(runtime.state_version(), 2);
    let start = request(
        2,
        serde_json::json!({"kind":"StartSession","args":{
        "session_id":"00000000-0000-0000-0000-000000000abc","epoch":1}}),
    );
    assert_eq!(
        code(&runtime.handle(&start).unwrap()),
        ErrorCode::EngineNotReady
    );
    assert_eq!(runtime.state(), SessionState::Blocked);
    assert!(runtime.output_is_muted());
    assert!(matches!(
        runtime.handle(&stop(3)).unwrap().outcome,
        Outcome::Ack
    ));
    assert_eq!(runtime.state(), SessionState::Idle);
    assert_eq!(runtime.epoch(), 2);
    assert!(runtime.output_is_muted());
}

#[test]
fn duplicate_mutation_is_idempotent_and_conflicting_id_is_rejected() {
    let mut runtime = Runtime::default();
    let first = runtime.handle(&prepare(10)).unwrap();
    let version = runtime.state_version();
    let second = runtime.handle(&prepare(10)).unwrap();
    assert_eq!(
        serde_json::to_vec(&first).unwrap(),
        serde_json::to_vec(&second).unwrap()
    );
    assert_eq!(runtime.state_version(), version);
    assert_eq!(
        code(&runtime.handle(&stop(10)).unwrap()),
        ErrorCode::RequestIdConflict
    );
    assert_eq!(runtime.state(), SessionState::Blocked);
}

#[test]
fn bounded_history_never_evicts_mutations_and_stop_remains_fail_closed() {
    let mut runtime = Runtime::default();
    runtime.handle(&prepare(1)).unwrap();
    for number in 2..=REQUEST_HISTORY_CAPACITY {
        runtime
            .handle(&request(
                number,
                serde_json::json!({"kind":"SetGain","args":{"db":0.0}}),
            ))
            .unwrap();
    }
    assert_eq!(
        code(&runtime.handle(&prepare(200)).unwrap()),
        ErrorCode::Backpressure
    );
    assert_eq!(
        code(&runtime.handle(&stop(201)).unwrap()),
        ErrorCode::Backpressure
    );
    assert_eq!(runtime.state(), SessionState::Idle);
    let epoch = runtime.epoch();
    runtime.handle(&stop(201)).unwrap();
    assert_eq!(runtime.epoch(), epoch);
    assert_eq!(
        code(&runtime.handle(&prepare(1)).unwrap()),
        ErrorCode::EngineNotReady
    );
    assert!(runtime.output_is_muted());
}

#[test]
fn read_only_polling_does_not_exhaust_mutation_budget() {
    let mut runtime = Runtime::default();
    for number in 1..=1000 {
        assert!(matches!(
            runtime
                .handle(&request(number, serde_json::json!({"kind":"GetState"})))
                .unwrap()
                .outcome,
            Outcome::State {
                state: SessionState::Idle
            }
        ));
    }
    assert_eq!(
        code(&runtime.handle(&prepare(1001)).unwrap()),
        ErrorCode::EngineNotReady
    );
    assert_eq!(runtime.state(), SessionState::Blocked);
}

#[test]
fn invalid_wire_does_not_change_owner_state() {
    let mut runtime = Runtime::default();
    assert!(runtime.handle(&[0; 65_537]).is_err());
    assert!(runtime.handle(br#"{"shell":"cmd.exe"}"#).is_err());
    let unknown = request(
        1,
        serde_json::json!({"kind":"GetState","args":{"pcm":"bad"}}),
    );
    assert!(runtime.handle(&unknown).is_err());
    assert_eq!(runtime.state_version(), 0);
    assert_eq!(runtime.state(), SessionState::Idle);
}

#[test]
fn exit_is_authoritative_after_validation_and_conflicts_even_when_history_is_full() {
    let mut runtime = Runtime::default();
    runtime.handle(&prepare(1)).unwrap();
    let exit = request(1, serde_json::json!({"kind":"ExitNode"}));
    assert_eq!(
        code(&runtime.handle(&exit).unwrap()),
        ErrorCode::RequestIdConflict
    );
    assert!(!runtime.shutdown_requested());
    let malformed = request(
        2,
        serde_json::json!({"kind":"ExitNode","args":{"shell":"bad"}}),
    );
    assert!(runtime.handle(&malformed).is_err());
    assert!(!runtime.shutdown_requested());
    for id in 2..=REQUEST_HISTORY_CAPACITY {
        runtime
            .handle(&request(
                id,
                serde_json::json!({"kind":"SetGain","args":{"db":0.0}}),
            ))
            .unwrap();
    }
    let exit = request(1000, serde_json::json!({"kind":"ExitNode"}));
    assert!(matches!(
        runtime.handle(&exit).unwrap().outcome,
        Outcome::Ack
    ));
    assert!(runtime.shutdown_requested());
    assert!(runtime.take_resource_cleanup());
    assert!(runtime.output_is_muted());
    assert_eq!(runtime.state(), SessionState::Idle);
    assert_eq!(runtime.epoch(), 2);
    let epoch = runtime.epoch();
    assert!(matches!(
        runtime.handle(&exit).unwrap().outcome,
        Outcome::Ack
    ));
    assert_eq!(runtime.epoch(), epoch);
    assert_eq!(
        code(&runtime.handle(&prepare(1001)).unwrap()),
        ErrorCode::Busy
    );
}

#[test]
fn duplicate_exit_replays_response_without_advancing_epoch() {
    let mut runtime = Runtime::default();
    let exit = request(1, serde_json::json!({"kind":"ExitNode"}));
    let first = runtime.handle(&exit).unwrap();
    assert_eq!(runtime.epoch(), 1);
    assert_eq!(
        serde_json::to_vec(&first).unwrap(),
        serde_json::to_vec(&runtime.handle(&exit).unwrap()).unwrap()
    );
    assert_eq!(runtime.epoch(), 1);
    assert_eq!(
        code(&runtime.handle(&stop(1)).unwrap()),
        ErrorCode::RequestIdConflict
    );
}
