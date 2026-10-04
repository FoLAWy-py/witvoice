#![cfg(windows)]

use std::{
    io::{BufRead, BufReader, Write},
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
    state::SessionState,
};
use witvoice_platform::{PipeClient, Secret, current_process_is_elevated};

static NEXT: AtomicU64 = AtomicU64::new(1);
const DEADLINE: Duration = Duration::from_secs(3);

struct ChildGuard(Child);
impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
struct Node {
    child: ChildGuard,
    tag: String,
    secret: Secret,
}
impl Node {
    fn new() -> Self {
        assert!(
            !current_process_is_elevated().unwrap(),
            "native process acceptance requires an ordinary, non-elevated user"
        );
        let tag = format!(
            "test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        );
        let secret = Secret::generate().unwrap();
        let mut child = Command::new(env!("CARGO_BIN_EXE_witvoice-node"))
            .args(["--endpoint", &tag, "--bootstrap-stdin"])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let mut input = child.stdin.take().unwrap();
        input.write_all(secret.as_bytes()).unwrap();
        drop(input);
        Self {
            child: ChildGuard(child),
            tag,
            secret,
        }
    }
    fn client(&self) -> PipeClient {
        PipeClient::connect(&self.tag, self.child.0.id(), DEADLINE).unwrap()
    }
    fn query(&self, id: usize) -> Response {
        let mut client = self.client();
        client.authenticate(&self.secret).unwrap();
        let bytes = format!(
            r#"{{"protocol_version":1,"request_id":"00000000-0000-0000-0000-{id:012x}","command":{{"kind":"GetState"}}}}"#
        );
        serde_json::from_slice(&client.request(bytes.as_bytes()).unwrap()).unwrap()
    }
    fn partial_child(&self) -> ChildGuard {
        let mut child = Command::new(env!("CARGO_BIN_EXE_ipc-probe"))
            .args([&self.tag, &self.child.0.id().to_string(), "hold-partial"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(self.secret.as_bytes())
            .unwrap();
        let output = child.stdout.take().unwrap();
        let (sender, receiver) = mpsc::sync_channel(1);
        std::thread::spawn(move || {
            let mut line = String::new();
            let result = BufReader::new(output).read_line(&mut line).map(|_| line);
            let _ = sender.send(result);
        });
        let guard = ChildGuard(child);
        assert_eq!(
            receiver.recv_timeout(DEADLINE).unwrap().unwrap().trim(),
            "partial-open"
        );
        guard
    }
}

#[test]
fn real_node_authentication_frames_and_fail_closed_lifecycle() {
    let node = Node::new();
    assert!(matches!(
        node.query(1).outcome,
        Outcome::State {
            state: SessionState::Idle
        }
    ));
    assert!(PipeClient::connect(&node.tag, std::process::id(), DEADLINE).is_err());
    {
        let mut client = node.client();
        client.authenticate(&Secret::generate().unwrap()).unwrap();
        assert!(client.request(br#"{}"#).is_err());
    }
    for header in [0u32.to_be_bytes(), 65_537u32.to_be_bytes()] {
        let mut client = node.client();
        client.authenticate(&node.secret).unwrap();
        client.write_fragment(&header).unwrap();
        assert!(client.read_response().is_err());
    }
    {
        let mut client = node.client();
        client.authenticate(&node.secret).unwrap();
        assert!(client.request(br#"{"shell":"cmd.exe"}"#).is_err());
    }
    let prepare = br#"{"protocol_version":1,"request_id":"00000000-0000-0000-0000-000000000002","command":{"kind":"PrepareSession","args":{"input_device_id":"input","output_device_id":"output","voice_id":"00000000-0000-0000-0000-000000000abc","route":{"kind":"Local"}}}}"#;
    let mut client = node.client();
    client.authenticate(&node.secret).unwrap();
    // Exercise fragmented header and payload, rather than relying on one WriteFile.
    let header = (prepare.len() as u32).to_be_bytes();
    for part in header.chunks(1) {
        client.write_fragment(part).unwrap();
    }
    for part in prepare.chunks(7) {
        client.write_fragment(part).unwrap();
    }
    let first = client.read_response().unwrap();
    let response: Response = serde_json::from_slice(&first).unwrap();
    assert!(matches!(
        response.outcome,
        Outcome::Error {
            code: ErrorCode::EngineNotReady,
            ..
        }
    ));
    drop(client);
    let mut client = node.client();
    client.authenticate(&node.secret).unwrap();
    assert_eq!(client.request(prepare).unwrap(), first);
    assert!(matches!(
        node.query(3).outcome,
        Outcome::State {
            state: SessionState::Blocked
        }
    ));
}

#[test]
fn client_death_and_partial_deadline_do_not_kill_node() {
    let mut node = Node::new();
    let mut ui = node.partial_child();
    ui.0.kill().unwrap();
    ui.0.wait().unwrap();
    assert!(matches!(
        node.query(10).outcome,
        Outcome::State {
            state: SessionState::Idle
        }
    ));
    assert!(node.child.0.try_wait().unwrap().is_none());
    let _stalled_ui = node.partial_child();
    let busy_start = Instant::now();
    assert!(PipeClient::connect(&node.tag, node.child.0.id(), Duration::from_millis(100)).is_err());
    assert!(busy_start.elapsed() < Duration::from_millis(500));
    let started = Instant::now();
    // The live partial writer remains alive; server cancellation must free its sole
    // instance after the 3-second total connection deadline, allowing this query.
    std::thread::sleep(Duration::from_millis(3200));
    assert!(matches!(
        node.query(11).outcome,
        Outcome::State {
            state: SessionState::Idle
        }
    ));
    assert!(started.elapsed() < Duration::from_secs(6));
    assert!(node.child.0.try_wait().unwrap().is_none());
}

#[test]
fn first_instance_rejects_endpoint_takeover() {
    let node = Node::new();
    node.query(1);
    let mut second = ChildGuard(
        Command::new(env!("CARGO_BIN_EXE_witvoice-node"))
            .args(["--endpoint", &node.tag, "--bootstrap-stdin"])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    second
        .0
        .stdin
        .take()
        .unwrap()
        .write_all(node.secret.as_bytes())
        .unwrap();
    let deadline = Instant::now() + DEADLINE;
    let status = loop {
        if let Some(status) = second.0.try_wait().unwrap() {
            break status;
        }
        assert!(Instant::now() < deadline, "second Node startup hung");
        std::thread::sleep(Duration::from_millis(10));
    };
    assert!(!status.success());
    assert!(matches!(
        node.query(2).outcome,
        Outcome::State {
            state: SessionState::Idle
        }
    ));
}
