use witvoice_contracts::{ErrorCode, values::DecimalU64, worker::*};

fn binding() -> WorkerBinding {
    WorkerBinding {
        session_tag: DecimalU64(0x0102030405060708),
        epoch: 9,
    }
}
fn packet(kind: WorkerMediaKind, count: u32) -> Vec<u8> {
    let h = WorkerMediaHeader {
        kind,
        session_tag: binding().session_tag.0,
        epoch: 9,
        sequence: 0xffff_fffe,
        source_sample_index: 2560,
        sample_rate: 16000,
        sample_count: count,
    };
    let mut p = h.encode().unwrap().to_vec();
    p.resize(40 + count as usize * 4, 0);
    p
}
#[test]
fn golden_header_and_variable_converted_output() {
    let p = packet(WorkerMediaKind::Source, 2560);
    assert_eq!(
        p[..40]
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>(),
        "01010000010203040506070800000009fffffffe0000000000000a0000003e8000000a0000000000"
    );
    assert_eq!(
        WorkerMediaHeader::decode(&p, &binding(), WorkerMediaKind::Source)
            .unwrap()
            .sample_count,
        2560
    );
    for count in [1, 160, 2560, 4096] {
        assert_eq!(
            WorkerMediaHeader::decode(
                &packet(WorkerMediaKind::Converted, count),
                &binding(),
                WorkerMediaKind::Converted
            )
            .unwrap()
            .sample_count,
            count
        );
    }
}
#[test]
fn rejects_corruption_nonfinite_and_stale_before_any_pcm_use() {
    let original = packet(WorkerMediaKind::Source, 2560);
    for (offset, value, error) in [
        (0, 2, ErrorCode::UnsupportedVersion),
        (1, 0, ErrorCode::InvalidMedia),
        (2, 1, ErrorCode::InvalidMedia),
        (4, 0, ErrorCode::SessionMismatch),
        (15, 10, ErrorCode::EpochMismatch),
        (36, 1, ErrorCode::InvalidMedia),
        (28, 1, ErrorCode::InvalidMedia),
        (32, 1, ErrorCode::InvalidMedia),
    ] {
        let mut p = original.clone();
        p[offset] = value;
        assert_eq!(
            WorkerMediaHeader::decode(&p, &binding(), WorkerMediaKind::Source),
            Err(error)
        );
    }
    for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        let mut p = original.clone();
        p[40..44].copy_from_slice(&value.to_le_bytes());
        assert_eq!(
            WorkerMediaHeader::decode(&p, &binding(), WorkerMediaKind::Source),
            Err(ErrorCode::InvalidMedia)
        );
    }
    let mut long = original.clone();
    long.push(0);
    assert_eq!(
        WorkerMediaHeader::decode(&long, &binding(), WorkerMediaKind::Source),
        Err(ErrorCode::InvalidMedia)
    );
    assert_eq!(
        WorkerMediaHeader::decode(&original[..39], &binding(), WorkerMediaKind::Source),
        Err(ErrorCode::SizeLimit)
    );
    let oversized = vec![0; 40 + 4097 * 4];
    assert_eq!(
        WorkerMediaHeader::decode(&oversized, &binding(), WorkerMediaKind::Source),
        Err(ErrorCode::SizeLimit)
    );
}
#[test]
fn no_source_chunk_or_timeline_overflow_ambiguity() {
    let base = WorkerMediaHeader {
        kind: WorkerMediaKind::Source,
        session_tag: 1,
        epoch: 1,
        sequence: 0,
        source_sample_index: 0,
        sample_rate: 16000,
        sample_count: 2560,
    };
    for count in [0, 1, 2559, 2561, 4097, u32::MAX] {
        assert_eq!(
            WorkerMediaHeader {
                sample_count: count,
                ..base
            }
            .encode(),
            Err(ErrorCode::InvalidMedia)
        );
    }
    assert_eq!(
        WorkerMediaHeader {
            source_sample_index: u64::MAX - 2559,
            ..base
        }
        .encode(),
        Err(ErrorCode::InvalidMedia)
    );
    assert_eq!(
        WorkerMediaHeader { epoch: 0, ..base }.encode(),
        Err(ErrorCode::InvalidMedia)
    );
}
#[test]
fn control_version_fields_identity_and_length_are_strict() {
    let good=br#"{"protocol_version":1,"request_id":"00000000-0000-0000-0000-000000000001","binding":{"session_tag":"1","epoch":1},"command":{"kind":"Heartbeat"}}"#;
    assert!(decode_worker_request(good).is_ok());
    for (from, to) in [
        ("\"session_tag\":\"1\"", "\"session_tag\":1"),
        ("\"epoch\":1", "\"epoch\":0"),
        ("Heartbeat", "FakeEngine"),
        ("\"protocol_version\":1", "\"protocol_version\":2"),
        ("\"epoch\":1", "\"epoch\":1,\"extra\":true"),
        ("000000000001", "00000000000G"),
    ] {
        let bad = std::str::from_utf8(good).unwrap().replace(from, to);
        assert!(
            decode_worker_request(bad.as_bytes()).is_err(),
            "{from} -> {to}"
        );
    }
    assert!(decode_worker_request(&vec![b' '; 65537]).is_err());
    assert!(decode_worker_request(b"").is_err());
    let response=br#"{"protocol_version":1,"request_id":"00000000-0000-0000-0000-000000000001","binding":{"session_tag":"72623859790382856","epoch":8},"event":{"kind":"Heartbeat"}}"#;
    assert_eq!(
        decode_worker_response(response, &binding()).unwrap_err(),
        ErrorCode::EpochMismatch
    );
}

#[test]
fn ready_event_rejects_unverified_backend_shape_and_empty_proof() {
    let mut response = serde_json::json!({"protocol_version":1,
        "request_id":"00000000-0000-0000-0000-000000000001",
        "binding":{"session_tag":"72623859790382856","epoch":9},
        "event":{"kind":"Ready","args":{"capabilities":{
            "engine_id":"meanvc2","model_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "backend":"cuda","native_input_rate":16000,"native_output_rate":16000,
            "chunk_samples":2560,"lookahead_samples":640,"conditioning_schema":"codec_fixture",
            "duration_preserving":false,"capability_test_run_id":"codec_fixture_not_model",
            "model_memory_budget_bytes":"1","device_memory_budget_bytes":null}}}});
    assert!(decode_worker_response(&serde_json::to_vec(&response).unwrap(), &binding()).is_ok());
    for (field, bad) in [
        ("backend", serde_json::json!("cpu")),
        ("engine_id", serde_json::json!("identity")),
        ("native_output_rate", serde_json::json!(48000)),
        ("chunk_samples", serde_json::json!(480)),
        ("capability_test_run_id", serde_json::json!("")),
    ] {
        let saved = response["event"]["args"]["capabilities"][field].clone();
        response["event"]["args"]["capabilities"][field] = bad;
        assert_eq!(
            decode_worker_response(&serde_json::to_vec(&response).unwrap(), &binding())
                .unwrap_err(),
            ErrorCode::EngineNotReady
        );
        response["event"]["args"]["capabilities"][field] = saved;
    }
}
