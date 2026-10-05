#[path = "../integration/support/mod.rs"]
mod support;
use support::*;
use witvoice_contracts::{ErrorCode, control::Outcome, state::SessionState};

#[test]
fn fixture_requires_exact_binding_and_explicit_start_without_production_capability() {
    let (mut runtime, adapter) = fixture();
    ack(runtime.handle(&prepare(1)).unwrap());
    assert_eq!(runtime.state(), SessionState::Ready);
    let gate = adapter.gate();
    assert_zero(&gate);
    drop(gate);
    assert_eq!(
        code(runtime.handle(&start(2, 0)).unwrap()),
        ErrorCode::EpochMismatch
    );
    let wrong = request(
        3,
        serde_json::json!({"kind":"StartSession","args":{"session_id":"00000000-0000-0000-0000-000000000def","epoch":1}}),
    );
    assert_eq!(
        code(runtime.handle(&wrong).unwrap()),
        ErrorCode::SessionMismatch
    );
    ack(runtime.handle(&start(4, 1)).unwrap());
    assert_eq!(runtime.state(), SessionState::Running);
    assert!(adapter.gate().is_live());
    assert_eq!(
        code(
            runtime
                .handle(&request(5, serde_json::json!({"kind":"GetCapabilities"})))
                .unwrap()
        ),
        ErrorCode::EngineNotReady
    );
}

#[test]
fn validation_and_replay_do_not_retire_output_or_advance_epoch() {
    let (mut runtime, adapter) = fixture();
    running(&mut runtime);
    let version = runtime.state_version();
    let retirements = adapter.retirements();
    let bytes = start(2, 1);
    ack(runtime.handle(&bytes).unwrap());
    assert_eq!(runtime.state_version(), version);
    assert_eq!(
        code(runtime.handle(&stop(2)).unwrap()),
        ErrorCode::RequestIdConflict
    );
    assert!(runtime.handle(b"{bad").is_err());
    assert_eq!(adapter.retirements(), retirements);
    assert!(adapter.gate().is_live());
}

#[test]
fn commit_ticket_prevents_quiescent_ack_and_fault_requires_fresh_prepare() {
    let (mut runtime, adapter) = fixture();
    running(&mut runtime);
    let gate = adapter.gate();
    let ticket = gate.begin_commit().unwrap();
    assert_eq!(
        code(runtime.handle(&mute(3, 1, true)).unwrap()),
        ErrorCode::Busy
    );
    assert!(!ticket.is_live());
    assert_zero(&gate);
    assert_eq!(
        code(runtime.handle(&mute(4, 2, false)).unwrap()),
        ErrorCode::Busy
    );
    drop(ticket);
    ack(runtime.handle(&mute(5, 2, true)).unwrap());
    ack(runtime.handle(&mute(6, 2, false)).unwrap());
    assert_eq!(runtime.epoch(), 3);
    assert_zero(&gate);
    let new_gate = adapter.gate();
    assert!(new_gate.is_live());
    let binding = runtime.test_binding().unwrap().clone();
    runtime.test_worker_failed(&binding).unwrap();
    assert_eq!(runtime.state(), SessionState::FailedMuted);
    assert_zero(&new_gate);
    assert_eq!(
        code(runtime.handle(&mute(7, 4, false)).unwrap()),
        ErrorCode::InvalidState
    );
    assert!(runtime.take_resource_cleanup());
    ack(runtime.handle(&prepare(8)).unwrap());
    ack(runtime.handle(&start(9, 5)).unwrap());
    assert!(adapter.gate().is_live());
    assert_zero(&new_gate);
}

#[test]
fn saturated_history_cannot_suppress_mute_stop_or_exit_retirement() {
    for action in ["mute", "stop", "exit"] {
        let (mut runtime, adapter) = fixture();
        running(&mut runtime);
        let gate = adapter.gate();
        for id in 3..=128 {
            let _ = runtime
                .handle(&request(
                    id,
                    serde_json::json!({"kind":"SetGain","args":{"db":0.0}}),
                ))
                .unwrap();
        }
        let bytes = match action {
            "mute" => mute(200, 1, true),
            "stop" => stop(200),
            _ => request(200, serde_json::json!({"kind":"ExitNode"})),
        };
        let response = runtime.handle(&bytes).unwrap();
        if action == "exit" {
            assert!(matches!(response.outcome, Outcome::Ack));
            assert!(runtime.shutdown_requested());
        } else {
            assert_eq!(code(response), ErrorCode::Backpressure);
        }
        assert_zero(&gate);
    }
}

#[test]
fn release_default_owner_still_cannot_prepare_or_start() {
    let mut runtime = witvoice_session::Runtime::default();
    assert_eq!(
        code(runtime.handle(&prepare(1)).unwrap()),
        ErrorCode::EngineNotReady
    );
    assert_eq!(
        code(runtime.handle(&start(2, 1)).unwrap()),
        ErrorCode::EngineNotReady
    );
    assert_eq!(runtime.state(), SessionState::Blocked);
}
