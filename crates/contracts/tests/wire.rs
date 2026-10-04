use witvoice_contracts::{control::*, media::*, state::*, values::*, *};

fn binding() -> MediaBinding {
    MediaBinding {
        session_tag: 0x0102030405060708,
        epoch: 9,
        kind: MediaKind::Source,
        sample_count: 480,
        media_permitted: true,
    }
}
fn header() -> MediaHeader {
    MediaHeader {
        kind: MediaKind::Source,
        session_tag: binding().session_tag,
        epoch: 9,
        sequence: 0xffff_fffe,
        media_sample_index: 480,
        source_sample_index: 480,
        sample_count: 480,
    }
}
fn packet() -> Vec<u8> {
    let mut p = header().encode().unwrap().to_vec();
    p.resize(1000, 0);
    p[40..42].copy_from_slice(&(-32768_i16).to_le_bytes());
    p[42..44].copy_from_slice(&32767_i16.to_le_bytes());
    p
}

#[test]
fn golden_header_and_pcm_endianness() {
    let expected = [
        1, 1, 0, 0, 1, 2, 3, 4, 5, 6, 7, 8, 0, 0, 0, 9, 255, 255, 255, 254, 0, 0, 0, 0, 0, 0, 1,
        224, 0, 0, 0, 0, 0, 0, 1, 224, 1, 224, 0, 0,
    ];
    assert_eq!(header().encode().unwrap(), expected);
    let p = packet();
    assert_eq!(&p[40..44], &[0, 128, 255, 127]);
    assert_eq!(MediaHeader::decode(&p, binding()), Ok(header()));
}
#[test]
fn malformed_and_unbound_media_rejected() {
    for index in [0, 1, 2, 3, 38, 39] {
        let mut p = packet();
        p[index] = 7;
        assert!(MediaHeader::decode(&p, binding()).is_err(), "index {index}");
    }
    for length in [0, 39, 40, 999, 1001] {
        let mut p = packet();
        p.resize(length, 0);
        assert!(
            MediaHeader::decode(&p, binding()).is_err(),
            "length {length}"
        );
    }
    let mut b = binding();
    b.epoch = 10;
    assert_eq!(
        MediaHeader::decode(&packet(), b),
        Err(ErrorCode::EpochMismatch)
    );
    b = binding();
    b.session_tag = 42;
    assert_eq!(
        MediaHeader::decode(&packet(), b),
        Err(ErrorCode::SessionMismatch)
    );
    b = binding();
    b.media_permitted = false;
    assert_eq!(
        MediaHeader::decode(&packet(), b),
        Err(ErrorCode::PermissionDenied)
    );
    b = binding();
    b.kind = MediaKind::Converted;
    assert_eq!(
        MediaHeader::decode(&packet(), b),
        Err(ErrorCode::InvalidMedia)
    );
}
#[test]
fn negotiated_frames_and_unknown_source() {
    let mut h = header();
    h.sample_count = 240;
    let mut p = h.encode().unwrap().to_vec();
    p.resize(520, 0);
    assert!(MediaHeader::decode(&p, binding()).is_err());
    let mut b = binding();
    b.sample_count = 240;
    assert_eq!(MediaHeader::decode(&p, b), Ok(h));
    h.sample_count = 1;
    assert_eq!(h.encode(), Err(ErrorCode::InvalidMedia));
    h = header();
    h.source_sample_index = SOURCE_UNKNOWN;
    assert!(h.encode().is_err());
    h.kind = MediaKind::Converted;
    assert!(h.encode().is_ok());
    h.media_sample_index = u64::MAX - 10;
    assert!(h.encode().is_err());
    h = header();
    h.session_tag = 0;
    assert!(h.encode().is_err());
}
#[test]
fn wrap_and_deadline_are_explicit() {
    assert_eq!(sequence_order(0, u32::MAX), SequenceOrder::After);
    assert_eq!(sequence_order(u32::MAX, 0), SequenceOrder::Before);
    assert_eq!(sequence_order(1, 1), SequenceOrder::Equal);
    assert_eq!(sequence_order(0x80000000, 0), SequenceOrder::Ambiguous);
    assert!(!is_expired(9, 10));
    assert!(is_expired(10, 10));
    assert!(is_expired(11, 10));
    assert_eq!(next_epoch(u32::MAX), Err(ErrorCode::EpochExhausted));
}
#[test]
fn controls_reject_unknown_fields_versions_and_oversize() {
    let payload=br#"{"protocol_version":1,"request_id":"01234567-89ab-cdef-0123-456789abcdef","command":{"kind":"GetState"}}"#;
    assert!(decode_request(payload).is_ok());
    for p in [
        br#"{"protocol_version":1,"request_id":"bad","command":{"kind":"GetState"}}"#.as_slice(),
        br#"{"protocol_version":1,"request_id":"01234567-89ab-cdef-0123-456789abcdef","command":{"kind":"GetState","extra":1}}"#,
        br#"{"protocol_version":1,"request_id":"01234567-89ab-cdef-0123-456789abcdef","command":{"kind":"Other"}}"#,
        br#"{"protocol_version":1,"request_id":"01234567-89ab-cdef-0123-456789abcdef","command":{"kind":"SetGain","args":{"db":13}}}"#,
        br#"{"protocol_version":1,"request_id":"01234567-89ab-cdef-0123-456789abcdef","command":{"kind":"SetGain","args":{"db":0,"extra":1}}}"#,
    ] { assert!(decode_request(p).is_err()); }
    let mut value: serde_json::Value = serde_json::from_slice(payload).unwrap();
    value["protocol_version"] = 2.into();
    assert_eq!(
        decode_request(&serde_json::to_vec(&value).unwrap()).unwrap_err(),
        ErrorCode::UnsupportedVersion
    );
    value["protocol_version"] = 1.into();
    value["extra"] = true.into();
    assert!(decode_request(&serde_json::to_vec(&value).unwrap()).is_err());
    assert_eq!(control_length(65536_u32.to_be_bytes()), Ok(65536));
    for length in [0, 65537, u32::MAX] {
        assert_eq!(
            control_length(length.to_be_bytes()),
            Err(ErrorCode::SizeLimit)
        );
    }
    assert!(decode_request(&vec![b' '; 65537]).is_err());
}
#[test]
fn u64_json_precision_and_digest_validation() {
    let schema = schemars::schema_for!(DecimalU64);
    assert_eq!(serde_json::to_value(schema).unwrap()["type"], "string");
    assert_eq!(
        serde_json::to_string(&DecimalU64(u64::MAX)).unwrap(),
        "\"18446744073709551615\""
    );
    assert_eq!(
        serde_json::from_str::<DecimalU64>("\"18446744073709551615\"")
            .unwrap()
            .0,
        u64::MAX
    );
    for text in ["1", "\"01\"", "\"-1\"", "\"18446744073709551616\""] {
        assert!(serde_json::from_str::<DecimalU64>(text).is_err());
    }
    assert!(Sha256::try_from("a".repeat(64)).is_ok());
    for text in ["a".repeat(63), "A".repeat(64), "z".repeat(64)] {
        assert!(Sha256::try_from(text).is_err());
    }
}
#[test]
fn local_and_peer_start_are_distinct() {
    assert!(serde_json::from_str::<Command>(r#"{"kind":"Start"}"#).is_err());
    assert!(serde_json::from_str::<PeerMessage>(r#"{"kind":"StartSession","args":{"session_id":"01234567-89ab-cdef-0123-456789abcdef","epoch":1}}"#).is_err());
    let p = PeerRequest {
        protocol_version: 1,
        request_id: Id::try_from("01234567-89ab-cdef-0123-456789abcdef".to_string()).unwrap(),
        context: None,
        message: PeerMessage::Start,
    };
    assert_eq!(p.validate(), Err(ErrorCode::SessionMismatch));
}
#[test]
fn safe_state_guards_and_failure_effects() {
    assert!(transition(SessionState::Idle, Event::Start).is_err());
    assert!(transition(SessionState::FailedMuted, Event::Unmute).is_err());
    assert!(transition(SessionState::Preparing, Event::Start).is_err());
    let start = transition(SessionState::Ready, Event::Start).unwrap();
    assert!(start.requires_explicit_user_action && start.requires_ready_resources);
    for state in [
        SessionState::Idle,
        SessionState::Preparing,
        SessionState::Ready,
        SessionState::Running,
        SessionState::Stopping,
        SessionState::Error,
        SessionState::Blocked,
        SessionState::Degraded,
        SessionState::FailedMuted,
        SessionState::Muted,
    ] {
        let stop = transition(state, Event::Stop).unwrap();
        assert_eq!(stop.next, SessionState::Stopping);
        assert!(stop.invalidate_output && stop.clear_queues);
    }
    for event in [Event::Fail, Event::Invalidate, Event::Mute, Event::Reset] {
        let effect = transition(SessionState::Running, event).unwrap();
        assert!(effect.invalidate_output && effect.clear_queues);
    }
    let unmute = transition(SessionState::Muted, Event::Unmute).unwrap();
    assert!(unmute.clear_queues && unmute.advance_epoch && unmute.requires_explicit_user_action);
}
#[test]
fn queue_limits_cannot_be_unbounded() {
    let mut q = QueueBudget {
        capacity_frames: 20,
        target_frames: 2,
        maximum_age_ms: 100,
        overflow_policy: OverflowPolicy::RejectNewest,
    };
    assert!(q.validate().is_ok());
    q.target_frames = 21;
    assert!(q.validate().is_err());
    q.target_frames = 2;
    q.maximum_age_ms = 0;
    assert!(q.validate().is_err());
}
