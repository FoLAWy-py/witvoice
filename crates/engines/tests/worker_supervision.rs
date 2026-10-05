#![cfg(windows)]

use supervisor::{
    CleanupObservation, CleanupSnapshot, CleanupStage, Failure, cleanup_io_failure,
    remember_cleanup,
};
fn observed_primary() -> Failure {
    Failure {
        kind: FailureKind::Worker,
        fault: Fault::MemoryPressure,
        raw_os_error: None,
        wire_code: None,
        native_failure: None,
    }
}
fn observation() -> CleanupObservation {
    CleanupObservation::new(Some(observed_primary()), true)
}
fn through_active() -> CleanupObservation {
    let mut observed = observation();
    observed.terminate(Ok(())).unwrap();
    observed.active(Ok(0)).unwrap();
    observed
}
fn through_ids() -> CleanupObservation {
    let mut observed = through_active();
    observed.ids(Ok(Vec::new())).unwrap();
    observed
}
fn all_released() -> CleanupSnapshot {
    let mut observed = through_ids();
    observed.wait(Ok(Some(19))).unwrap();
    observed.confirm(Ok(lifecycle::Action::None)).unwrap();
    observed.finish(Some(12), true)
}
#[test]
fn cleanup_observation_nonzero_active_stops_before_ids_or_wait() {
    let mut observed = observation();
    observed.terminate(Ok(())).unwrap();
    assert!(observed.active(Ok(3)).is_err());
    let snapshot = observed.finish(Some(7), false);
    assert_eq!(snapshot.stage, CleanupStage::QueryActive);
    assert_eq!(snapshot.terminate_succeeded, Some(true));
    assert_eq!(snapshot.active_processes, Some(3));
    assert_eq!(snapshot.process_id_count, None);
    assert_eq!(snapshot.controller_signaled, None);
    assert_eq!(snapshot.policy_confirmed, None);
    assert_eq!(snapshot.primary_failure, Some(observed_primary()));
    assert!(snapshot.stopped_ack);
}
#[test]
fn cleanup_observation_nonempty_ids_stops_before_controller_wait() {
    let mut observed = through_active();
    assert!(observed.ids(Ok(vec![37, 41])).is_err());
    let snapshot = observed.finish(Some(8), false);
    assert_eq!(snapshot.stage, CleanupStage::QueryIds);
    assert_eq!(snapshot.active_processes, Some(0));
    assert_eq!(snapshot.process_id_count, Some(2));
    assert_eq!(snapshot.controller_signaled, None);
    assert_eq!(snapshot.controller_exit_code, None);
    assert_eq!(snapshot.cleanup_failure.unwrap().raw_os_error, None);
}
#[test]
fn cleanup_observation_unsignaled_controller_retains_actual_zero_queries() {
    let mut observed = through_ids();
    assert!(observed.wait(Ok(None)).is_err());
    let snapshot = observed.finish(Some(9), false);
    assert_eq!(snapshot.stage, CleanupStage::WaitController);
    assert_eq!(snapshot.active_processes, Some(0));
    assert_eq!(snapshot.process_id_count, Some(0));
    assert_eq!(snapshot.controller_signaled, Some(false));
    assert_eq!(snapshot.controller_exit_code, None);
    assert_eq!(snapshot.policy_confirmed, None);
}
#[test]
fn cleanup_observation_all_zero_and_signaled_require_policy_confirmation() {
    let snapshot = all_released();
    assert_eq!(snapshot.stage, CleanupStage::Released);
    assert_eq!(snapshot.terminate_succeeded, Some(true));
    assert_eq!(snapshot.active_processes, Some(0));
    assert_eq!(snapshot.process_id_count, Some(0));
    assert_eq!(snapshot.controller_signaled, Some(true));
    assert_eq!(snapshot.controller_exit_code, Some(19));
    assert_eq!(snapshot.policy_confirmed, Some(true));
    assert_eq!(snapshot.elapsed_ms, Some(12));
    assert_eq!(snapshot.cleanup_failure, None);
}
#[test]
fn cleanup_observation_native_stage_and_hresult_are_retained_separately() {
    use witvoice_platform::{ProcessNativeFailure, ProcessOperation};
    let code = 0x80070005u32 as i32;
    for (operation, stage) in [
        (ProcessOperation::TerminateJob, CleanupStage::Terminate),
        (ProcessOperation::QueryJobActive, CleanupStage::QueryActive),
        (ProcessOperation::QueryJobIds, CleanupStage::QueryIds),
        (
            ProcessOperation::GetProcessExitCode,
            CleanupStage::WaitController,
        ),
        (ProcessOperation::Unknown, CleanupStage::Terminate),
    ] {
        // Pure normalized-result fixture, not a performed native API call.
        let injected = Failure {
            kind: FailureKind::Cleanup,
            fault: Fault::Protocol,
            raw_os_error: None,
            wire_code: None,
            native_failure: Some(ProcessNativeFailure {
                operation,
                hresult: code,
            }),
        };
        let mut observed = observation();
        let result = match stage {
            CleanupStage::Terminate => observed.terminate(Err(injected)),
            CleanupStage::QueryActive => {
                observed.terminate(Ok(())).unwrap();
                observed.active(Err(injected))
            }
            CleanupStage::QueryIds => {
                observed = through_active();
                observed.ids(Err(injected))
            }
            CleanupStage::WaitController => {
                observed = through_ids();
                observed.wait(Err(injected))
            }
            _ => unreachable!(),
        };
        assert_eq!(result, Err(injected));
        let snapshot = observed.finish(Some(10), false);
        assert_eq!(snapshot.stage, stage);
        assert_eq!(snapshot.cleanup_failure, Some(injected));
        assert_eq!(snapshot.cleanup_failure.unwrap().raw_os_error, None);
        assert_eq!(snapshot.primary_failure, Some(observed_primary()));
        match stage {
            CleanupStage::Terminate => {
                assert_eq!(snapshot.active_processes, None);
                assert_eq!(snapshot.terminate_succeeded, Some(false));
            }
            CleanupStage::QueryActive => {
                assert_eq!(snapshot.active_processes, None);
                assert_eq!(snapshot.process_id_count, None);
            }
            CleanupStage::QueryIds => {
                assert_eq!(snapshot.process_id_count, None);
                assert_eq!(snapshot.controller_signaled, None);
            }
            CleanupStage::WaitController => {
                assert_eq!(snapshot.controller_signaled, None);
                assert_eq!(snapshot.controller_exit_code, None);
            }
            _ => unreachable!(),
        }
    }
}
#[test]
fn cleanup_observation_raw_win32_and_unknown_text_do_not_become_hresult() {
    let raw = cleanup_io_failure(std::io::Error::from_raw_os_error(5));
    assert_eq!(raw.raw_os_error, Some(5));
    assert_eq!(raw.native_failure, None);
    let unknown = cleanup_io_failure(std::io::Error::other(
        "Windows process operation failed (0x80070005)",
    ));
    assert_eq!(unknown.raw_os_error, None);
    assert_eq!(unknown.native_failure, None);
    let mut observed = observation();
    observed.terminate(Ok(())).unwrap();
    assert_eq!(observed.active(Err(unknown)), Err(unknown));
    let snapshot = observed.finish(None, false);
    assert_eq!(snapshot.active_processes, None);
    assert_eq!(snapshot.elapsed_ms, None);
}
#[test]
fn cleanup_observation_precondition_and_policy_refusal_keep_unexecuted_null() {
    let mut observed = CleanupObservation::new(None, false);
    let error = Failure {
        kind: FailureKind::Policy,
        fault: Fault::Protocol,
        raw_os_error: None,
        wire_code: None,
        native_failure: None,
    };
    observed.reject(error);
    let snapshot = observed.finish(Some(0), false);
    assert_eq!(snapshot.stage, CleanupStage::Precondition);
    assert_eq!(snapshot.terminate_succeeded, None);
    assert_eq!(snapshot.active_processes, None);
    assert_eq!(snapshot.process_id_count, None);
    assert_eq!(snapshot.controller_signaled, None);
    assert_eq!(snapshot.primary_failure, None);
    assert!(!snapshot.stopped_ack);
    let mut observed = through_ids();
    observed.wait(Ok(Some(0))).unwrap();
    assert!(
        observed
            .confirm(Err(lifecycle::Error::CleanupIncomplete))
            .is_err()
    );
    let snapshot = observed.finish(Some(13), false);
    assert_eq!(snapshot.stage, CleanupStage::ConfirmPolicy);
    assert_eq!(snapshot.policy_confirmed, Some(false));
    assert_eq!(snapshot.controller_exit_code, Some(0));
    assert!(snapshot.cleanup_failure.is_some());
}
#[test]
fn cleanup_observation_first_refusal_never_overwritten_by_later_success_or_failure() {
    let mut saved = None;
    remember_cleanup(&mut saved, all_released());
    let mut observed = through_active();
    observed.ids(Ok(vec![57])).unwrap_err();
    let first = observed.finish(Some(14), false);
    remember_cleanup(&mut saved, first);
    remember_cleanup(&mut saved, all_released());
    let mut second = observation();
    second
        .terminate(Err(cleanup_io_failure(std::io::Error::from_raw_os_error(
            5,
        ))))
        .unwrap_err();
    remember_cleanup(&mut saved, second.finish(Some(15), false));
    assert_eq!(saved, Some(first));
    assert_eq!(saved.unwrap().primary_failure, Some(observed_primary()));
    assert!(saved.unwrap().stopped_ack);
}

#[test]
fn cleanup_observation_controller_signaled_later_uses_only_remaining_budget() {
    let started = Instant::now();
    let deadline = started + Duration::from_secs(3);
    let after_job = started + Duration::from_millis(1200);
    let mut observed = through_ids();
    observed
        .wait_until(deadline, after_job, |remaining| {
            assert_eq!(remaining, Duration::from_millis(1800));
            // Deterministic native-result fixture: Job was already empty but the
            // retained controller handle signals during its remaining wait.
            Ok(Some(1))
        })
        .unwrap();
    observed.confirm(Ok(lifecycle::Action::None)).unwrap();
    let snapshot = observed.finish(Some(1500), true);
    assert_eq!(snapshot.stage, CleanupStage::Released);
    assert_eq!(snapshot.active_processes, Some(0));
    assert_eq!(snapshot.process_id_count, Some(0));
    assert_eq!(snapshot.controller_signaled, Some(true));
    assert_eq!(snapshot.controller_exit_code, Some(1));
    assert_eq!(snapshot.cleanup_failure, None);
}

#[test]
fn cleanup_observation_expired_remaining_budget_never_waits_or_renews() {
    let started = Instant::now();
    let deadline = started + Duration::from_secs(3);
    for now in [deadline, deadline + Duration::from_millis(1)] {
        let mut observed = through_ids();
        let error = observed
            .wait_until(deadline, now, |_| {
                panic!("expired absolute deadline must not query the controller")
            })
            .unwrap_err();
        assert_eq!(error.kind, FailureKind::Cleanup);
        let snapshot = observed.finish(Some(3000), false);
        assert_eq!(snapshot.stage, CleanupStage::WaitController);
        assert_eq!(snapshot.active_processes, Some(0));
        assert_eq!(snapshot.process_id_count, Some(0));
        assert_eq!(snapshot.controller_signaled, None);
        assert_eq!(snapshot.controller_exit_code, None);
        assert_eq!(snapshot.policy_confirmed, None);
        assert_eq!(snapshot.cleanup_failure, Some(error));
        assert_eq!(snapshot.primary_failure, Some(observed_primary()));
        assert!(snapshot.stopped_ack);
    }
}

#[test]
fn cleanup_observation_remaining_wait_api_error_retains_first_refusal() {
    let started = Instant::now();
    let deadline = started + Duration::from_secs(3);
    let mut observed = through_ids();
    let error = observed
        .wait_until(
            deadline,
            started + Duration::from_millis(2900),
            |remaining| {
                assert_eq!(remaining, Duration::from_millis(100));
                Err(std::io::Error::from_raw_os_error(5))
            },
        )
        .unwrap_err();
    assert_eq!(error.kind, FailureKind::Cleanup);
    assert_eq!(error.raw_os_error, Some(5));
    let first = observed.finish(Some(2900), false);
    assert_eq!(first.stage, CleanupStage::WaitController);
    assert_eq!(first.controller_signaled, None);
    assert_eq!(first.policy_confirmed, None);
    assert_eq!(first.primary_failure, Some(observed_primary()));
    let mut saved = None;
    remember_cleanup(&mut saved, first);
    remember_cleanup(&mut saved, all_released());
    assert_eq!(saved, Some(first));
    assert_eq!(saved.unwrap().cleanup_failure, Some(error));
}

// Compile exactly the production source with only its cfg(test) fixed-peer factory.
pub use witvoice_engines::lifecycle;
#[path = "../src/supervisor.rs"]
mod supervisor;
use lifecycle::{Binding, Fault, Phase};
use std::{
    sync::{
        Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};
use supervisor::{FailureKind, WorkerSupervisor};
use witvoice_platform::Process;
static SERIAL: Mutex<()> = Mutex::new(());
static SESSION: AtomicU64 = AtomicU64::new(90000);
fn owner(mode: &'static str) -> WorkerSupervisor {
    WorkerSupervisor::test_peer(SESSION.fetch_add(1, Ordering::Relaxed), mode).unwrap()
}
fn initial(owner: &mut WorkerSupervisor) {
    // Session is carried through explicit Binding, never generated by restart.
    owner
        .begin(Binding::new(SESSION.load(Ordering::Relaxed) - 1, 1).unwrap())
        .unwrap();
    assert!(owner.channels_authenticated());
    assert!(!owner.output_allowed());
}
fn ready(owner: &mut WorkerSupervisor) {
    let limit = Instant::now() + Duration::from_secs(3);
    while !owner.output_allowed() {
        owner.poll().unwrap();
        assert!(Instant::now() < limit, "finite ready wait");
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(owner.phase(), Phase::Ready);
    assert!(owner.capabilities().is_some());
}
fn fault(owner: &mut WorkerSupervisor) -> supervisor::Failure {
    let limit = Instant::now() + Duration::from_secs(3);
    loop {
        if let Err(error) = owner.poll() {
            return error;
        }
        assert!(Instant::now() < limit, "finite fault wait");
        std::thread::sleep(Duration::from_millis(10));
    }
}
fn reclaimed(owner: &WorkerSupervisor, handles: &[Process]) {
    assert!(owner.resources_released());
    assert!(!owner.output_allowed());
    assert!(!owner.channels_authenticated());
    for process in handles {
        assert!(
            process.wait(Duration::ZERO).unwrap().is_some(),
            "owned process not exited"
        );
    }
}
fn handles(owner: &WorkerSupervisor) -> Vec<Process> {
    owner
        .owned_process_ids()
        .unwrap()
        .into_iter()
        .map(|pid| {
            let process = Process::observe(pid).unwrap();
            println!(
                "owned native member {pid}: {}",
                process.image_filename().unwrap()
            );
            process
        })
        .collect()
}
#[test]
fn authenticated_dual_channels_ready_stop_ack_and_actual_tree_zero() {
    let _serial = SERIAL.lock().unwrap();
    let mut worker = owner("valid");
    initial(&mut worker);
    ready(&mut worker);
    let processes = handles(&worker);
    let expected = worker.test_peer_identity().unwrap();
    let mut python_ids = Vec::new();
    let mut console_count = 0;
    for process in &processes {
        match process.image_filename().unwrap().as_str() {
            "python.exe" => python_ids.push(process.id()),
            "conhost.exe" => console_count += 1,
            _ => panic!("unexpected owned test image"),
        }
    }
    python_ids.sort_unstable();
    let mut expected = expected.to_vec();
    expected.sort_unstable();
    assert_eq!(
        python_ids, expected,
        "actual controller/leaf both in exact owned Job"
    );
    assert_eq!(
        console_count, 2,
        "actual observed console helpers are also owned"
    );
    assert_eq!(
        processes.len(),
        4,
        "actual controller/leaf and observed console helpers"
    );
    let pid = worker.worker_pid().unwrap();
    let stopped = worker.stop();
    println!("first cleanup observation: {:?}", worker.cleanup_snapshot());
    println!(
        "stop result: {stopped:?}; output_allowed: {}; resources_released: {}; stopped_ack: {}",
        worker.output_allowed(),
        worker.resources_released(),
        worker.stopped_ack()
    );
    stopped.unwrap();
    assert!(worker.stopped_ack());
    assert_eq!(worker.last_cleaned_pid(), Some(pid));
    assert_eq!(worker.phase(), Phase::Stopped);
    reclaimed(&worker, &processes);
}
#[test]
fn bad_ready_request_binding_and_complete_capability_are_fail_closed() {
    let _serial = SERIAL.lock().unwrap();
    for mode in [
        "bad_request",
        "bad_binding",
        "bad_cap",
        "bad_budget",
        "bad_run",
    ] {
        let mut worker = owner(mode);
        initial(&mut worker);
        let error = fault(&mut worker);
        assert_eq!(error.kind, FailureKind::Protocol);
        assert_eq!(worker.phase(), Phase::RetryPending);
        assert_eq!(worker.first_failure(), Some(error));
        assert!(worker.capabilities().is_none());
        reclaimed(&worker, &[]);
    }
}
#[test]
fn eof_partial_frame_heartbeat_timeout_and_memory_pressure_retire_actual_job() {
    let _serial = SERIAL.lock().unwrap();
    for (mode, expected) in [
        ("eof", Fault::Eof),
        ("partial", Fault::HeartbeatTimeout),
        ("no_heartbeat", Fault::HeartbeatTimeout),
        ("memory_pressure", Fault::MemoryPressure),
    ] {
        let mut worker = owner(mode);
        initial(&mut worker);
        let error = fault(&mut worker);
        assert_eq!(error.fault, expected, "{mode}");
        assert!(worker.last_cleaned_pid().is_some());
        reclaimed(&worker, &[]);
    }
}
#[test]
fn two_fresh_native_pids_epochs_then_third_fault_requires_explicit_reset() {
    let _serial = SERIAL.lock().unwrap();
    let mut worker = owner("ready_eof");
    initial(&mut worker);
    let session = worker.binding().unwrap().session_tag;
    let mut previous = 0;
    for epoch in 1..=3 {
        ready(&mut worker);
        let pid = worker.worker_pid().unwrap();
        assert_ne!(pid, previous);
        previous = pid;
        fault(&mut worker);
        assert_eq!(worker.last_cleaned_pid(), Some(pid));
        assert!(worker.resources_released());
        if epoch < 3 {
            assert_eq!(worker.phase(), Phase::RetryPending);
            assert!(
                worker
                    .restart(Binding::new(session, epoch).unwrap())
                    .is_err()
            );
            assert!(worker.worker_pid().is_none());
            worker
                .restart(Binding::new(session, epoch + 1).unwrap())
                .unwrap();
            assert_eq!(worker.restart_count(), epoch as u8);
        }
    }
    assert_eq!(worker.phase(), Phase::FailedMuted);
    assert_eq!(worker.restart_count(), 2);
    assert!(worker.restart(Binding::new(session, 4).unwrap()).is_err());
    assert!(worker.begin(Binding::new(session, 4).unwrap()).is_err());
    worker.explicit_user_reset().unwrap();
    assert!(!worker.output_allowed());
    worker.begin(Binding::new(session, 4).unwrap()).unwrap();
    worker.stop().ok(); // Peer may return EOF during Stop; resources must still be reclaimed.
    assert!(worker.resources_released());
}
#[test]
fn old_epoch_after_real_restart_cannot_reopen_output() {
    let _serial = SERIAL.lock().unwrap();
    let mut worker = owner("late_old");
    initial(&mut worker);
    ready(&mut worker);
    let binding = worker.binding().unwrap();
    fault(&mut worker);
    worker
        .restart(Binding::new(binding.session_tag, 2).unwrap())
        .unwrap();
    assert!(!worker.output_allowed());
    let error = fault(&mut worker);
    assert_eq!(error.kind, FailureKind::Protocol);
    assert_eq!(worker.binding().unwrap().epoch, 2);
    assert!(worker.capabilities().is_none());
    reclaimed(&worker, &[]);
}
#[test]
fn both_channel_authentication_required_wrong_token_or_missing_media_never_ready() {
    let _serial = SERIAL.lock().unwrap();
    for mode in ["bad_token", "bad_media_token", "no_media"] {
        let mut worker = owner(mode);
        let tag = SESSION.load(Ordering::Relaxed) - 1;
        assert!(worker.begin(Binding::new(tag, 1).unwrap()).is_err());
        assert!(worker.last_cleaned_pid().is_some());
        assert!(!worker.stopped_ack());
        reclaimed(&worker, &[]);
    }
}
#[test]
fn shared_absolute_deadline_is_not_renewed_after_partial_header() {
    let _serial = SERIAL.lock().unwrap();
    let mut worker = owner("delayed_shared");
    initial(&mut worker);
    let start = Instant::now();
    let error = worker.poll().unwrap_err();
    assert_eq!(error.fault, Fault::HeartbeatTimeout);
    // Header arrives ~220ms in; body would arrive ~450ms, exceeding original400ms.
    // A refreshed per-part window would accept it. The actual kernel timeout rejects.
    assert!(start.elapsed() >= Duration::from_millis(380));
    assert!(start.elapsed() < Duration::from_millis(900));
    reclaimed(&worker, &[]);
}
#[test]
fn delayed_owner_monitor_cannot_accept_late_ready_or_heartbeat() {
    let _serial = SERIAL.lock().unwrap();
    let mut worker = owner("valid");
    initial(&mut worker);
    std::thread::sleep(Duration::from_millis(510));
    let error = worker.poll().unwrap_err();
    assert_eq!(error.fault, Fault::HeartbeatTimeout);
    assert!(worker.capabilities().is_none());
    reclaimed(&worker, &[]);
}
