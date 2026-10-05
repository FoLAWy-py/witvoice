#![cfg(windows)]
//! Deterministic real kernel IPC failure ordering; no model, audio or network.
use std::{
    ffi::OsString,
    path::Path,
    thread,
    time::{Duration, Instant},
};
use witvoice_contracts::{
    CONTROL_MAX_BYTES,
    values::DecimalU64,
    worker::{WorkerBinding, WorkerEvent, decode_worker_response},
};
use witvoice_platform::{ProcessJob, Secret, WorkerPipeServer, current_process_is_elevated};

fn run(cause: &str) {
    assert!(!current_process_is_elevated().unwrap());
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let python = Path::new(
        r"C:\Users\22198\.cache\codex-runtimes\codex-primary-runtime\dependencies\python\python.exe",
    );
    let tag = format!("term-{}-{cause}", std::process::id());
    let secret = Secret::from_bytes([0xa4; 32]);
    let mut control =
        WorkerPipeServer::bind(&(tag.clone() + "-c"), Secret::from_bytes([0xa4; 32])).unwrap();
    let mut media =
        WorkerPipeServer::bind(&(tag.clone() + "-m"), Secret::from_bytes([0xa4; 32])).unwrap();
    let job = ProcessJob::new(false).unwrap();
    let args: Vec<OsString> = vec![
        "-I".into(),
        "-S".into(),
        root.join("tests/integration/worker_terminal_peer.py")
            .into_os_string(),
        cause.into(),
        "--control".into(),
        (tag.clone() + "-c").into(),
        "--media".into(),
        (tag + "-m").into(),
        "--node-pid".into(),
        std::process::id().to_string().into(),
    ];
    let worker = job
        .spawn_bootstrapped(python, &args, false, &secret)
        .unwrap();
    control
        .accept(worker.id(), Instant::now() + Duration::from_secs(3))
        .unwrap();
    media
        .accept(worker.id(), Instant::now() + Duration::from_secs(3))
        .unwrap();
    let binding = WorkerBinding {
        session_tag: DecimalU64(11),
        epoch: 1,
    };
    let send = |control: &mut WorkerPipeServer, sequence: u8, kind: &str| {
        let mut command = serde_json::json!({"kind":kind});
        if kind == "Warmup" {
            command["args"] = serde_json::json!({"model_sha256":"01caafec9a3991a5514df9412952d24ae3370406358a01e20e03df41f2f5d515","reference_id":"00000000-0000-0000-0000-000000000011","backend":"Cuda"});
        }
        let value = serde_json::json!({"protocol_version":1,"request_id":format!("00000000-0000-0000-0000-{sequence:012x}"),"binding":{"session_tag":"11","epoch":1},"command":command});
        control
            .write_frame(
                &serde_json::to_vec(&value).unwrap(),
                CONTROL_MAX_BYTES,
                Instant::now() + Duration::from_millis(400),
            )
            .unwrap();
    };
    send(&mut control, 1, "Warmup");
    // The test peer deterministically queues its injected terminal outcome
    // before this ACK; its first poll is suppressed until this heartbeat.
    send(&mut control, 2, "Heartbeat");
    let ack = decode_worker_response(
        &control
            .read_frame(
                CONTROL_MAX_BYTES,
                Instant::now() + Duration::from_millis(400),
            )
            .unwrap(),
        &binding,
    )
    .unwrap();
    assert!(matches!(ack.event, WorkerEvent::Heartbeat));
    // Reproduce the Node ordering: ACK ends its read, 100ms interval, then
    // WRITE next heartbeat before reading the asynchronous terminal frame.
    thread::sleep(Duration::from_millis(100));
    send(&mut control, 3, "Heartbeat");
    let fault = decode_worker_response(
        &control
            .read_frame(
                CONTROL_MAX_BYTES,
                Instant::now() + Duration::from_millis(400),
            )
            .unwrap(),
        &binding,
    )
    .unwrap();
    assert_eq!(
        String::from(fault.request_id),
        "00000000-0000-0000-0000-000000000001"
    );
    match cause {
        "MemoryPressure" => assert!(matches!(fault.event, WorkerEvent::MemoryPressure)),
        _ => assert!(matches!(fault.event, WorkerEvent::Failed { .. })),
    }
    control.close();
    media.close();
    job.terminate().unwrap();
    assert_eq!(job.active_processes().unwrap(), 0);
}

#[test]
fn failed_after_heartbeat_survives_next_native_write() {
    run("Failed");
}
#[test]
fn memory_pressure_after_heartbeat_survives_next_native_write() {
    run("MemoryPressure");
}
