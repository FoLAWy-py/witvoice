use witvoice_engines::lifecycle::*;

const PROOF: WarmupProof = WarmupProof([7; 32]);

fn binding(epoch: u32) -> Binding {
    Binding::new(17, epoch).unwrap()
}

fn ready() -> WorkerLifecycle {
    let mut worker = WorkerLifecycle::new(17).unwrap();
    assert_eq!(worker.begin(binding(1), PROOF, 0, 1000), Ok(Action::None));
    assert!(!worker.output_allowed());
    assert!(!worker.resources_released());
    assert_eq!(worker.warmup_ready(binding(1), PROOF, 1), Ok(Action::None));
    assert_eq!(worker.phase(), Phase::Ready);
    assert!(worker.output_allowed());
    worker
}

fn killed(action: Action, expected_binding: Binding, fault: Fault) {
    assert_eq!(
        action,
        Action::KillAndCleanup {
            binding: expected_binding,
            reason: CleanupReason::Fault(fault),
        }
    );
}

#[test]
fn rejects_zero_or_other_session_and_never_owns_resources_on_bad_begin() {
    assert!(matches!(
        WorkerLifecycle::new(0),
        Err(Error::InvalidBinding)
    ));
    assert_eq!(Binding::new(0, 1), Err(Error::InvalidBinding));
    assert_eq!(Binding::new(17, 0), Err(Error::InvalidBinding));
    let mut worker = WorkerLifecycle::new(17).unwrap();
    assert_eq!(
        worker.begin(Binding::new(18, 1).unwrap(), PROOF, 0, 1000),
        Err(Error::WrongSession)
    );
    assert_eq!(
        worker.begin(
            Binding {
                session_tag: 17,
                epoch: 0
            },
            PROOF,
            0,
            1000
        ),
        Err(Error::InvalidBinding)
    );
    assert!(!worker.output_allowed());
    assert!(worker.resources_released());
}

#[test]
fn ready_requires_expected_manifest_digest_and_cleanup_after_mismatch() {
    let mut worker = WorkerLifecycle::new(17).unwrap();
    worker.begin(binding(1), PROOF, 0, 1000).unwrap();
    killed(
        worker
            .warmup_ready(binding(1), WarmupProof([8; 32]), 1)
            .unwrap(),
        binding(1),
        Fault::UnexpectedWarmupProof,
    );
    assert!(!worker.output_allowed());
    assert!(worker.cleanup_pending());
    assert!(!worker.resources_released());
    assert_eq!(
        worker.warmup_ready(binding(1), PROOF, 2),
        Err(Error::WrongPhase)
    );
}

#[test]
fn warmup_timeout_is_expired_at_equal_deadline_and_late_ready_cannot_reopen() {
    let mut worker = WorkerLifecycle::new(17).unwrap();
    worker.begin(binding(1), PROOF, 0, 200).unwrap();
    assert_eq!(worker.monitor(199), Action::None);
    killed(worker.monitor(200), binding(1), Fault::WarmupTimeout);
    assert!(!worker.output_allowed());
    assert_eq!(
        worker.warmup_ready(binding(1), PROOF, 200),
        Err(Error::WrongPhase)
    );
}

#[test]
fn heartbeat_expiry_applies_during_warmup_and_ready() {
    for make_ready in [false, true] {
        let mut worker = WorkerLifecycle::new(17).unwrap();
        worker.begin(binding(1), PROOF, 0, 1000).unwrap();
        if make_ready {
            worker.warmup_ready(binding(1), PROOF, 1).unwrap();
        }
        assert_eq!(worker.monitor(499), Action::None);
        killed(
            worker.heartbeat(binding(1), 500).unwrap(),
            binding(1),
            Fault::HeartbeatTimeout,
        );
        assert!(!worker.output_allowed());
        assert!(!worker.resources_released());
    }
}

#[test]
fn heartbeat_never_renews_inflight_media_deadline() {
    let mut worker = ready();
    assert_eq!(
        worker.submit_media(binding(1), 0, 10, 300),
        Ok(Action::None)
    );
    assert_eq!(worker.heartbeat(binding(1), 100), Ok(Action::None));
    assert_eq!(worker.heartbeat(binding(1), 200), Ok(Action::None));
    assert_eq!(worker.media_deadline_ms(), Some(300));
    killed(
        worker.media_completed(binding(1), 0, 300).unwrap(),
        binding(1),
        Fault::MediaTimeout,
    );
    assert!(!worker.output_allowed());
    assert_eq!(
        worker.media_completed(binding(1), 0, 301),
        Err(Error::WrongPhase)
    );
}

#[test]
fn media_has_one_outstanding_task_and_sequence_cannot_replay() {
    let mut worker = ready();
    assert_eq!(
        worker.submit_media(binding(1), 4, 10, 100),
        Ok(Action::None)
    );
    assert_eq!(
        worker.submit_media(binding(1), 5, 11, 200),
        Err(Error::Busy)
    );
    assert_eq!(worker.media_deadline_ms(), Some(100));
    assert_eq!(worker.media_completed(binding(1), 4, 12), Ok(Action::None));
    assert_eq!(
        worker.submit_media(binding(1), 4, 13, 100),
        Err(Error::OutOfOrderSequence)
    );
    assert_eq!(
        worker.submit_media(binding(1), 3, 14, 100),
        Err(Error::OutOfOrderSequence)
    );
    assert_eq!(
        worker.submit_media(binding(1), 5, 15, 15),
        Err(Error::InvalidDeadline)
    );
    assert_eq!(worker.media_deadline_ms(), None);
    assert!(worker.output_allowed());
    assert_eq!(
        worker.submit_media(binding(1), 5, 16, 100),
        Ok(Action::None)
    );
}

#[test]
fn unsolicited_or_wrong_sequence_result_is_protocol_failure() {
    for outstanding in [false, true] {
        let mut worker = ready();
        if outstanding {
            worker.submit_media(binding(1), 1, 10, 100).unwrap();
        }
        killed(
            worker.media_completed(binding(1), 2, 11).unwrap(),
            binding(1),
            Fault::UnexpectedMediaResult,
        );
        assert!(!worker.output_allowed());
        assert!(worker.cleanup_pending());
    }
}

#[test]
fn memory_eof_and_protocol_faults_immediately_require_cleanup_and_stay_muted() {
    for fault in [Fault::MemoryPressure, Fault::Eof, Fault::Protocol] {
        let mut worker = ready();
        worker.submit_media(binding(1), 0, 10, 100).unwrap();
        let action = worker.worker_fault(binding(1), fault).unwrap();
        killed(action, binding(1), fault);
        assert!(!worker.output_allowed());
        assert!(!worker.resources_released());
        assert_eq!(worker.media_deadline_ms(), None);
        assert_eq!(worker.cleanup_action(), action);
        assert_eq!(worker.worker_fault(binding(1), Fault::Protocol), Ok(action));
        assert_eq!(worker.first_fault(), Some(fault));
        assert_eq!(worker.restart_count(), 0);
        assert_eq!(
            worker.restart(binding(2), PROOF, 11, 1000),
            Err(Error::WrongPhase)
        );
    }
}

#[test]
fn both_process_tree_and_lease_confirmation_are_required_for_restart() {
    let mut worker = ready();
    worker.worker_fault(binding(1), Fault::Eof).unwrap();
    for (exited, released) in [(false, false), (true, false), (false, true)] {
        assert_eq!(
            worker.cleanup_confirmed(binding(1), exited, released),
            Err(Error::CleanupIncomplete)
        );
        assert!(!worker.resources_released());
        assert!(worker.cleanup_pending());
        assert!(!worker.output_allowed());
    }
    assert_eq!(
        worker.cleanup_confirmed(binding(1), true, true),
        Ok(Action::RestartPermitted)
    );
    assert!(worker.resources_released());
    assert_eq!(worker.phase(), Phase::RetryPending);
    assert!(!worker.output_allowed());
    assert_eq!(
        worker.restart(binding(1), PROOF, 2, 1000),
        Err(Error::StaleBinding)
    );
    assert_eq!(worker.phase(), Phase::RetryPending);
    assert_eq!(worker.restart_count(), 0);
}

#[test]
fn two_restarts_total_ready_does_not_reset_budget_and_third_fault_needs_user_reset() {
    let mut worker = ready();
    for epoch in 1..=3 {
        let now = u64::from(epoch) * 10;
        worker.worker_fault(binding(epoch), Fault::Eof).unwrap();
        assert!(!worker.output_allowed());
        assert_eq!(
            worker.explicit_user_reset(),
            Err(if epoch == 3 {
                Error::CleanupIncomplete
            } else {
                Error::WrongPhase
            })
        );
        let action = worker
            .cleanup_confirmed(binding(epoch), true, true)
            .unwrap();
        if epoch < 3 {
            assert_eq!(action, Action::RestartPermitted);
            worker
                .restart(binding(epoch + 1), PROOF, now, now + 1000)
                .unwrap();
            worker
                .warmup_ready(binding(epoch + 1), PROOF, now + 1)
                .unwrap();
            assert_eq!(worker.restart_count(), epoch as u8);
            assert!(worker.output_allowed());
        } else {
            assert_eq!(action, Action::None);
            assert_eq!(worker.phase(), Phase::FailedMuted);
            assert_eq!(worker.restart_count(), 2);
            assert!(worker.resources_released());
            assert_eq!(worker.stop(), Action::None);
            assert_eq!(worker.phase(), Phase::FailedMuted);
            assert_eq!(
                worker.begin(binding(4), PROOF, now, now + 1000),
                Err(Error::WrongPhase)
            );
            assert_eq!(
                worker.restart(binding(4), PROOF, now, now + 1000),
                Err(Error::WrongPhase)
            );
            worker.explicit_user_reset().unwrap();
            assert!(!worker.output_allowed());
            worker.begin(binding(4), PROOF, now, now + 1000).unwrap();
            assert_eq!(worker.restart_count(), 0);
            assert_eq!(worker.first_fault(), None);
            assert!(!worker.output_allowed());
        }
    }
}

#[test]
fn old_binding_events_and_cleanup_cannot_touch_fresh_worker() {
    let mut worker = ready();
    worker.worker_fault(binding(1), Fault::Eof).unwrap();
    worker.cleanup_confirmed(binding(1), true, true).unwrap();
    worker.restart(binding(2), PROOF, 10, 1000).unwrap();
    worker.warmup_ready(binding(2), PROOF, 11).unwrap();
    worker.submit_media(binding(2), 0, 12, 100).unwrap();
    assert_eq!(
        worker.heartbeat(binding(1), u64::MAX),
        Err(Error::StaleBinding)
    );
    assert_eq!(
        worker.media_completed(binding(1), 0, 13),
        Err(Error::StaleBinding)
    );
    assert_eq!(
        worker.worker_fault(binding(1), Fault::MemoryPressure),
        Err(Error::StaleBinding)
    );
    assert_eq!(
        worker.cleanup_confirmed(binding(1), true, true),
        Err(Error::StaleBinding)
    );
    assert_eq!(
        worker.warmup_ready(binding(1), PROOF, 13),
        Err(Error::StaleBinding)
    );
    assert_eq!(worker.binding(), Some(binding(2)));
    assert_eq!(worker.phase(), Phase::Ready);
    assert!(worker.output_allowed());
    assert_eq!(worker.media_deadline_ms(), Some(100));
    assert_eq!(worker.media_completed(binding(2), 0, 14), Ok(Action::None));
}

#[test]
fn stop_cannot_claim_release_before_confirmed_cleanup_and_never_requests_restart() {
    let mut worker = ready();
    worker.submit_media(binding(1), 0, 10, 100).unwrap();
    let stop = worker.stop();
    assert_eq!(
        stop,
        Action::KillAndCleanup {
            binding: binding(1),
            reason: CleanupReason::Stop
        }
    );
    assert_eq!(worker.phase(), Phase::Stopping);
    assert!(!worker.output_allowed());
    assert!(!worker.resources_released());
    assert_eq!(worker.stop(), stop);
    assert_eq!(
        worker.cleanup_confirmed(binding(1), true, false),
        Err(Error::CleanupIncomplete)
    );
    assert_eq!(
        worker.restart(binding(2), PROOF, 11, 1000),
        Err(Error::WrongPhase)
    );
    assert_eq!(
        worker.cleanup_confirmed(binding(1), true, true),
        Ok(Action::None)
    );
    assert_eq!(worker.phase(), Phase::Stopped);
    assert!(worker.resources_released());
    assert_eq!(
        worker.cleanup_confirmed(binding(1), true, true),
        Err(Error::WrongPhase)
    );
    assert_eq!(
        worker.begin(binding(1), PROOF, 12, 1000),
        Err(Error::StaleBinding)
    );
    assert_eq!(worker.begin(binding(2), PROOF, 12, 1000), Ok(Action::None));
    assert!(!worker.output_allowed());
}

#[test]
fn clock_regression_fails_closed_and_invalid_initial_deadlines_own_nothing() {
    let mut worker = ready();
    assert_eq!(worker.heartbeat(binding(1), 100), Ok(Action::None));
    killed(worker.monitor(99), binding(1), Fault::ClockRegression);
    assert!(!worker.output_allowed());
    let mut fresh = WorkerLifecycle::new(17).unwrap();
    assert_eq!(
        fresh.begin(binding(1), PROOF, 10, 10),
        Err(Error::InvalidDeadline)
    );
    assert_eq!(
        fresh.begin(binding(1), PROOF, u64::MAX - 499, u64::MAX),
        Err(Error::InvalidDeadline)
    );
    assert!(fresh.resources_released());
    assert!(!fresh.output_allowed());
    assert_eq!(fresh.binding(), None);
}

#[test]
fn heartbeat_addition_overflow_never_wraps_to_a_live_deadline() {
    let mut worker = WorkerLifecycle::new(17).unwrap();
    worker
        .begin(binding(1), PROOF, u64::MAX - 1000, u64::MAX)
        .unwrap();
    worker
        .warmup_ready(binding(1), PROOF, u64::MAX - 999)
        .unwrap();
    assert_eq!(
        worker.heartbeat(binding(1), u64::MAX - 700),
        Ok(Action::None)
    );
    killed(
        worker.heartbeat(binding(1), u64::MAX - 300).unwrap(),
        binding(1),
        Fault::ClockOverflow,
    );
    assert!(!worker.output_allowed());
    assert!(!worker.resources_released());
}

#[test]
fn maximum_epoch_is_never_reused_or_wrapped_by_restart_or_user_reset() {
    let mut worker = WorkerLifecycle::new(17).unwrap();
    let last = binding(u32::MAX);
    worker.begin(last, PROOF, 0, 1000).unwrap();
    worker.warmup_ready(last, PROOF, 1).unwrap();
    worker.worker_fault(last, Fault::Eof).unwrap();
    assert_eq!(worker.cleanup_confirmed(last, true, true), Ok(Action::None));
    assert_eq!(worker.phase(), Phase::FailedMuted);
    assert_eq!(worker.explicit_user_reset(), Err(Error::EpochExhausted));
    assert_eq!(
        worker.restart(binding(1), PROOF, 2, 1000),
        Err(Error::WrongPhase)
    );
    assert!(!worker.output_allowed());
    assert!(worker.resources_released());
}

#[test]
fn maximum_media_sequence_needs_new_worker_binding_and_cannot_wrap() {
    let mut worker = ready();
    worker.submit_media(binding(1), u32::MAX, 10, 100).unwrap();
    worker.media_completed(binding(1), u32::MAX, 11).unwrap();
    killed(
        worker.submit_media(binding(1), 0, 12, 100).unwrap(),
        binding(1),
        Fault::SequenceExhausted,
    );
    assert!(!worker.output_allowed());
    assert!(!worker.resources_released());
}
