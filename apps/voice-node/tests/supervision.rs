#![cfg(all(windows, feature = "process-tests"))]
use std::{
    io::{BufRead, BufReader, Read, Write},
    path::Path,
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};
use witvoice_contracts::{
    ErrorCode,
    control::{Outcome, Response},
};
use witvoice_platform::{PipeClient, Process, ProcessJob, Secret, current_process_is_elevated};
const LIMIT: Duration = Duration::from_secs(3);
static NEXT: AtomicU64 = AtomicU64::new(1);
fn tag() -> String {
    assert!(
        !current_process_is_elevated().unwrap(),
        "requires ordinary non-elevated user"
    );
    format!(
        "sup-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )
}
fn line(output: impl Read + Send + 'static) -> String {
    let (sender, receiver) = mpsc::sync_channel(1);
    std::thread::spawn(move || {
        let mut text = String::new();
        let result = BufReader::new(output.take(128))
            .read_line(&mut text)
            .map(|_| text);
        let _ = sender.send(result);
    });
    receiver
        .recv_timeout(LIMIT)
        .unwrap()
        .unwrap()
        .trim()
        .to_owned()
}
fn request(
    tag: &str,
    pid: u32,
    secret: &Secret,
    number: usize,
    command: serde_json::Value,
) -> Response {
    let mut client = PipeClient::connect(tag, pid, LIMIT).unwrap();
    client.authenticate(secret).unwrap();
    let bytes = serde_json::to_vec(&serde_json::json!({"protocol_version":1,"request_id":format!("00000000-0000-0000-0000-{number:012x}"),"command":command})).unwrap();
    serde_json::from_slice(&client.request(&bytes).unwrap()).unwrap()
}
fn state(tag: &str, pid: u32, secret: &Secret) {
    assert!(matches!(
        request(tag, pid, secret, 1, serde_json::json!({"kind":"GetState"})).outcome,
        Outcome::State { .. }
    ));
}
struct ChildGuard(Child);
impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
struct DetachedGuard(Process);
impl Drop for DetachedGuard {
    fn drop(&mut self) {
        let _ = self.0.terminate();
    }
}
fn exited(process: &Process) {
    assert!(
        process.wait(LIMIT).unwrap().is_some(),
        "PID {} survived cleanup",
        process.id()
    );
    println!("reclaimed PID {}", process.id());
}
fn members(job: &ProcessJob) -> Vec<(u32, String)> {
    let result: Vec<_> = job
        .process_ids()
        .unwrap()
        .into_iter()
        .map(|pid| {
            (
                pid,
                Process::observe(pid).unwrap().image_filename().unwrap(),
            )
        })
        .collect();
    println!("actual job members {result:?}");
    result
}
fn child_exited(child: &mut Child) {
    let deadline = Instant::now() + LIMIT + Duration::from_millis(500);
    while child.try_wait().unwrap().is_none() {
        assert!(Instant::now() < deadline, "PID {} did not exit", child.id());
        std::thread::sleep(Duration::from_millis(5));
    }
    println!("reclaimed owner PID {}", child.id());
}

#[test]
fn actual_ui_job_close_and_parent_death_leave_node_independent() {
    for close_job in [true, false] {
        let endpoint = tag();
        let secret = Secret::generate().unwrap();
        let job_name = format!("witvoice-ui-{endpoint}");
        let job = ProcessJob::named_ui(&job_name, true).unwrap();
        let mut ui = job
            .spawn_bootstrapped(
                Path::new(env!("CARGO_BIN_EXE_process-probe")),
                &["ui-parent".into(), endpoint.clone().into(), job_name.into()],
                true,
                &secret,
            )
            .unwrap();
        let text = line(ui.take_output().unwrap());
        let pid: u32 = text.strip_prefix("node:").expect(&text).parse().unwrap();
        println!("independent Node PID {pid}");
        let node = DetachedGuard(Process::observe_for_cleanup(pid).unwrap());
        assert_eq!(members(&job).len(), 1);
        assert_eq!(
            job.active_processes().unwrap(),
            1,
            "Node must not belong to UI job"
        );
        state(&endpoint, pid, &secret);
        if close_job {
            drop(job);
        } else {
            ui.terminate().unwrap();
            drop(job);
        }
        exited(&ui);
        assert!(node.0.wait(Duration::ZERO).unwrap().is_none());
        state(&endpoint, pid, &secret);
        let response = request(
            &endpoint,
            pid,
            &secret,
            2,
            serde_json::json!({"kind":"ExitNode"}),
        );
        assert!(matches!(response.outcome, Outcome::Ack));
        exited(&node.0);
        assert!(PipeClient::connect(&endpoint, pid, Duration::from_millis(50)).is_err());
    }
}

#[test]
fn denying_ui_job_breakaway_fails_without_children_or_endpoint() {
    let endpoint = tag();
    let secret = Secret::generate().unwrap();
    let job_name = format!("witvoice-ui-{endpoint}");
    let job = ProcessJob::named_ui(&job_name, false).unwrap();
    let mut ui = job
        .spawn_bootstrapped(
            Path::new(env!("CARGO_BIN_EXE_process-probe")),
            &["ui-parent".into(), endpoint.clone().into(), job_name.into()],
            true,
            &secret,
        )
        .unwrap();
    let text = line(ui.take_output().unwrap());
    assert!(text.starts_with("denied:"), "{text}");
    assert_ne!(ui.wait(LIMIT).unwrap().unwrap(), 0);
    let deadline = Instant::now() + LIMIT;
    while job.active_processes().unwrap() != 0 {
        assert!(
            Instant::now() < deadline,
            "job still contains processes after denied launch"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(job.active_processes().unwrap(), 0);
    assert!(PipeClient::connect(&endpoint, ui.id(), Duration::from_millis(50)).is_err());
    println!(
        "denied breakaway; UI PID {}, active job processes 0",
        ui.id()
    );
}

#[test]
fn closing_or_stopping_job_reclaims_real_worker_descendants() {
    for explicit_stop in [false, true] {
        let _ = tag();
        let job = ProcessJob::new(false).unwrap();
        let mut worker = job
            .spawn(
                Path::new(env!("CARGO_BIN_EXE_process-probe")),
                &["worker-tree".into()],
                true,
            )
            .unwrap();
        let child_pid: u32 = line(worker.take_output().unwrap()).parse().unwrap();
        let descendant = Process::observe(child_pid).unwrap();
        let rows = members(&job);
        assert!(rows.iter().all(|(_, name)| name == "process-probe.exe"));
        assert_eq!(job.active_processes().unwrap(), 2);
        if explicit_stop {
            job.terminate().unwrap();
            assert_eq!(job.active_processes().unwrap(), 0);
        }
        drop(job);
        exited(&worker);
        exited(&descendant);
    }
}

#[test]
fn trusted_argv_roundtrips_and_failed_job_assignment_never_resumes_child() {
    let endpoint = tag();
    let name = format!("witvoice-ui-{endpoint}");
    let job = ProcessJob::named_ui(&name, false).unwrap();
    assert!(
        ProcessJob::named_ui(&name, true).is_err(),
        "must not mutate a pre-existing job"
    );
    let query = ProcessJob::open_ui_query(&name).unwrap();
    assert!(
        query
            .spawn(
                Path::new(env!("CARGO_BIN_EXE_process-probe")),
                &["worker-tree".into()],
                true
            )
            .is_err()
    );
    assert_eq!(job.active_processes().unwrap(), 0);
    assert!(job.process_ids().unwrap().is_empty());
    let values = ["", "space 中文", "quote\"inner", "ends\\", "slash\\\"quote"];
    let mut args = vec!["argv-check".into()];
    args.extend(values.iter().map(|s| (*s).into()));
    let mut child = job
        .spawn(Path::new(env!("CARGO_BIN_EXE_process-probe")), &args, true)
        .unwrap();
    let output = line(child.take_output().unwrap());
    assert_eq!(
        serde_json::from_str::<Vec<String>>(&output).unwrap(),
        values
    );
    assert_eq!(child.wait(LIMIT).unwrap(), Some(0));
    assert!(job.spawn(Path::new("relative.exe"), &[], false).is_err());
    println!("failed query-only assignment cleaned suspended child; trusted argv roundtripped");
}

struct ProbeNode {
    child: ChildGuard,
    endpoint: String,
    secret: Secret,
    worker: Process,
    descendant: Process,
}
impl ProbeNode {
    fn new() -> Self {
        let endpoint = tag();
        let secret = Secret::generate().unwrap();
        let mut child = Command::new(env!("CARGO_BIN_EXE_process-probe"))
            .args(["--endpoint", &endpoint, "--bootstrap-stdin"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(secret.as_bytes())
            .unwrap();
        let text = line(child.stdout.take().unwrap());
        let parts: Vec<_> = text.split(':').collect();
        assert_eq!(parts[0], "worker");
        let worker = Process::observe(parts[1].parse().unwrap()).unwrap();
        let descendant = Process::observe(parts[2].parse().unwrap()).unwrap();
        let node = Self {
            child: ChildGuard(child),
            endpoint,
            secret,
            worker,
            descendant,
        };
        state(&node.endpoint, node.child.0.id(), &node.secret);
        node
    }
}
#[test]
fn node_owned_job_is_reclaimed_on_stop_exit_and_owner_kill() {
    for action in ["stop", "exit", "kill"] {
        let mut node = ProbeNode::new();
        let pid = node.child.0.id();
        if action == "kill" {
            node.child.0.kill().unwrap();
        } else {
            let cmd = if action == "exit" {
                serde_json::json!({"kind":"ExitNode"})
            } else {
                serde_json::json!({"kind":"StopSession","args":{"session_id":"00000000-0000-0000-0000-000000000abc"}})
            };
            assert!(matches!(
                request(&node.endpoint, pid, &node.secret, 2, cmd).outcome,
                Outcome::Ack
            ));
        }
        exited(&node.worker);
        exited(&node.descendant);
        if action == "stop" {
            state(&node.endpoint, pid, &node.secret);
            assert!(node.child.0.try_wait().unwrap().is_none());
        } else {
            child_exited(&mut node.child.0);
            assert!(PipeClient::connect(&node.endpoint, pid, Duration::from_millis(50)).is_err());
        }
    }
}

#[test]
fn exit_conflict_full_history_and_lost_ack_use_runtime_authority() {
    let mut node = ProbeNode::new();
    let pid = node.child.0.id();
    let gain = serde_json::json!({"kind":"SetGain","args":{"db":0.0}});
    request(&node.endpoint, pid, &node.secret, 10, gain.clone());
    let conflict = request(
        &node.endpoint,
        pid,
        &node.secret,
        10,
        serde_json::json!({"kind":"ExitNode"}),
    );
    assert!(matches!(
        conflict.outcome,
        Outcome::Error {
            code: ErrorCode::RequestIdConflict,
            ..
        }
    ));
    assert!(node.worker.wait(Duration::ZERO).unwrap().is_none());
    state(&node.endpoint, pid, &node.secret);
    for id in 11..=137 {
        request(&node.endpoint, pid, &node.secret, id, gain.clone());
    }
    // Full history cannot suppress an accepted Exit; deliberately do not read/ACK response.
    let mut client = PipeClient::connect(&node.endpoint, pid, LIMIT).unwrap();
    client.authenticate(&node.secret).unwrap();
    let bytes=br#"{"protocol_version":1,"request_id":"00000000-0000-0000-0000-000000000999","command":{"kind":"ExitNode"}}"#;
    client
        .write_fragment(&(bytes.len() as u32).to_be_bytes())
        .unwrap();
    client.write_fragment(bytes).unwrap();
    child_exited(&mut node.child.0);
    exited(&node.worker);
    exited(&node.descendant);
    assert!(PipeClient::connect(&node.endpoint, pid, Duration::from_millis(50)).is_err());
}

#[test]
fn malformed_bootstrap_timeout_and_endpoint_failure_start_no_worker() {
    for timeout in [false, true] {
        let endpoint = tag();
        let mut guard = ChildGuard(
            Command::new(env!("CARGO_BIN_EXE_process-probe"))
                .args(["--endpoint", &endpoint, "--bootstrap-stdin"])
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
                .unwrap(),
        );
        let input = guard.0.stdin.take().unwrap();
        if !timeout {
            drop(input);
        } else {
            child_exited(&mut guard.0);
            drop(input);
        }
        child_exited(&mut guard.0);
        assert!(!guard.0.wait().unwrap().success());
        assert_eq!(line(guard.0.stdout.take().unwrap()), "");
        assert!(PipeClient::connect(&endpoint, guard.0.id(), Duration::from_millis(50)).is_err());
    }
    let node = ProbeNode::new();
    let mut other = ChildGuard(
        Command::new(env!("CARGO_BIN_EXE_process-probe"))
            .args(["--endpoint", &node.endpoint, "--bootstrap-stdin"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    other
        .0
        .stdin
        .take()
        .unwrap()
        .write_all(node.secret.as_bytes())
        .unwrap();
    child_exited(&mut other.0);
    assert!(!other.0.wait().unwrap().success());
    assert_eq!(line(other.0.stdout.take().unwrap()), "");
    state(&node.endpoint, node.child.0.id(), &node.secret);
}
