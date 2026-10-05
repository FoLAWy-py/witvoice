use witvoice_audio::{
    notifications::{ChangeKind, ChangeSignal},
    realtime::{Binding, OutputGate},
    route::{Direction, Observation, Origin, RouteError, RouteSelection},
};
fn observations() -> [Observation<'static>; 3] {
    [
        Observation {
            uid: "render-id",
            direction: Direction::Render,
            active: true,
            exact_format: true,
            origin: Origin::Software {
                parent: "ROOT\\driver",
                evidence: "local-driver-proof",
            },
        },
        Observation {
            uid: "capture-id",
            direction: Direction::Capture,
            active: true,
            exact_format: true,
            origin: Origin::Software {
                parent: "root\\DRIVER",
                evidence: "local-driver-proof",
            },
        },
        Observation {
            uid: "physical-id",
            direction: Direction::Capture,
            active: true,
            exact_format: true,
            origin: Origin::Physical {
                evidence: "local-usb-proof",
            },
        },
    ]
}
fn gate() -> OutputGate {
    let mut gate = OutputGate::new(Binding::new(10, 1).unwrap()).unwrap();
    gate.arm().unwrap();
    gate
}
#[test]
fn policy_rejects_unknown_physical_feedback_direction_and_format() {
    for (index, change, expected) in [
        (0, 0, RouteError::Inactive),
        (1, 1, RouteError::WrongDirection),
        (1, 2, RouteError::UnsupportedFormat),
        (0, 3, RouteError::UnknownOrigin),
        (0, 4, RouteError::PhysicalVirtualEndpoint),
        (1, 5, RouteError::DifferentSoftwareParent),
        (2, 6, RouteError::Feedback),
        (2, 7, RouteError::UnknownOrigin),
        (0, 8, RouteError::InvalidUid),
    ] {
        let mut items = observations();
        match change {
            0 => items[index].active = false,
            1 => items[index].direction = Direction::Render,
            2 => items[index].exact_format = false,
            3 => items[index].origin = Origin::Unknown,
            4 => {
                items[index].origin = Origin::Physical {
                    evidence: "physical",
                }
            }
            5 => {
                items[index].origin = Origin::Software {
                    parent: "other-driver",
                    evidence: "proof",
                }
            }
            6 => items[index].uid = "CAPTURE-id",
            7 => items[index].origin = items[0].origin,
            8 => items[index].uid = "id\0suffix",
            _ => unreachable!(),
        }
        assert_eq!(
            RouteSelection::candidate(items[0], items[1], items[2]).err(),
            Some(expected)
        );
    }
}
#[test]
fn exact_saved_uid_removal_or_replacement_fails_gate_and_cannot_recover() {
    for replaced in [false, true] {
        let mut items = observations();
        let mut route = RouteSelection::candidate(items[0], items[1], items[2]).unwrap();
        let gate = gate();
        if replaced {
            items[1].uid = "replacement-with-same-friendly-name";
        }
        assert_eq!(
            route.revalidate(
                Some(items[0]),
                replaced.then_some(items[1]),
                Some(items[2]),
                &gate
            ),
            Err(if replaced {
                RouteError::Replaced
            } else {
                RouteError::Missing
            })
        );
        assert!(!gate.is_live());
        assert!(gate.begin_commit().is_none());
        let original = observations();
        assert_eq!(
            route.revalidate(
                Some(original[0]),
                Some(original[1]),
                Some(original[2]),
                &gate
            ),
            Err(RouteError::Retired)
        );
    }
}
#[test]
fn unchanged_candidate_still_has_no_start_or_continuity_authority() {
    let mut items = observations();
    let mut route = RouteSelection::candidate(items[0], items[1], items[2]).unwrap();
    let muted = OutputGate::new(Binding::new(10, 1).unwrap()).unwrap();
    items[0].uid = "RENDER-ID";
    assert!(
        route
            .revalidate(Some(items[0]), Some(items[1]), Some(items[2]), &muted)
            .is_ok()
    );
    assert!(!muted.is_live());
    items[2].origin = Origin::Physical {
        evidence: "different-source-proof",
    };
    assert_eq!(
        route.revalidate(Some(items[0]), Some(items[1]), Some(items[2]), &muted),
        Err(RouteError::Replaced)
    );
}
#[test]
fn change_before_commit_faults_without_wait_and_batch_drain_never_recovers() {
    let items = observations();
    let mut route = RouteSelection::candidate(items[0], items[1], items[2]).unwrap();
    let gate = gate();
    let signal = ChangeSignal::new();
    signal.publish(ChangeKind::InitialValidation);
    assert!(route.check_changes(&signal, &gate).is_ok());
    let ticket = gate.begin_commit().unwrap();
    signal.publish(ChangeKind::Removed);
    assert_eq!(
        route.check_changes(&signal, &gate),
        Err(RouteError::DeviceChanged)
    );
    assert!(!ticket.is_live());
    assert!(!gate.ack_ready());
    drop(ticket);
    assert!(gate.ack_ready());
    signal.take_batch();
    assert!(route.check_changes(&signal, &gate).is_err());
    assert!(gate.begin_commit().is_none());
}
#[test]
fn diagnostic_pair_never_substitutes_for_production_physical_source_validation() {
    let items = observations();
    let mut pair = RouteSelection::virtual_candidate(items[0], items[1]).unwrap();
    let gate = gate();
    assert!(
        pair.revalidate_pair(Some(items[0]), Some(items[1]), &gate)
            .is_ok()
    );
    assert_eq!(
        pair.revalidate(Some(items[0]), Some(items[1]), Some(items[2]), &gate),
        Err(RouteError::PhysicalSourceRequired)
    );
    let mut production = RouteSelection::candidate(items[0], items[1], items[2]).unwrap();
    let gate = super_gate();
    assert_eq!(
        production.revalidate_pair(Some(items[0]), Some(items[1]), &gate),
        Err(RouteError::PhysicalSourceRequired)
    );
    assert!(!gate.is_live());
}
fn super_gate() -> OutputGate {
    gate()
}

#[test]
fn route_change_erases_already_queued_processed_output_before_slow_cleanup() {
    use witvoice_audio::realtime::{
        BlockError, ProcessedBlock, QueueConfig, Ring, processed_endpoints,
    };
    let items = observations();
    let mut route = RouteSelection::candidate(items[0], items[1], items[2]).unwrap();
    let gate = gate();
    let mut ring = Ring::new(8).unwrap();
    let (mut producer, mut output) =
        processed_endpoints(&mut ring, &gate, QueueConfig::output()).unwrap();
    producer
        .push(
            ProcessedBlock::from_model_result(gate.binding(), 0, 0, 60_000_000, &[0.5; 480])
                .unwrap(),
            0,
        )
        .unwrap();
    let signal = ChangeSignal::new();
    signal.publish(ChangeKind::Removed);
    assert_eq!(
        route.check_changes(&signal, &gate),
        Err(RouteError::DeviceChanged)
    );
    let mut samples = [0.9; 480];
    assert_eq!(output.render(&mut samples, 1), Err(BlockError::Muted));
    assert_eq!(samples, [0.0; 480]);
    assert!(gate.ack_ready());
    assert_eq!(output.counters().underflow_frames, 0);
}
