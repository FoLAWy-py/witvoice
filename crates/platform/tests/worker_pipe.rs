#![cfg(windows)]
// Real current-user Windows kernel IPC only. No Python/model/Job/audio/network claims.
use std::{
    io,
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};
use witvoice_platform::{
    CONTROL_MAX_BYTES, PipeClient, Secret, WorkerPipeClient, WorkerPipeServer,
    current_process_is_elevated,
};

static NEXT: AtomicU64 = AtomicU64::new(1);
const KEY: [u8; 32] = [9; 32];

fn tag() -> String {
    let n = NEXT
        .try_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
        .unwrap();
    format!("wp-{}-{n}", std::process::id())
}
fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(2)
}
fn spawn_server<F>(f: F) -> (String, thread::JoinHandle<()>)
where
    F: FnOnce(&mut WorkerPipeServer) + Send + 'static,
{
    assert!(
        !current_process_is_elevated().unwrap(),
        "ordinary current-user IPC test required"
    );
    let tag = tag();
    let server_tag = tag.clone();
    let (tx, rx) = mpsc::sync_channel(1);
    let join = thread::spawn(move || {
        let mut server = WorkerPipeServer::bind(&server_tag, Secret::from_bytes(KEY)).unwrap();
        tx.send(()).unwrap();
        f(&mut server);
    });
    rx.recv_timeout(Duration::from_secs(2)).unwrap();
    (tag, join)
}
fn client(tag: &str) -> WorkerPipeClient {
    WorkerPipeClient::connect(
        tag,
        std::process::id(),
        &Secret::from_bytes(KEY),
        deadline(),
    )
    .unwrap()
}
fn raw_client(tag: &str) -> PipeClient {
    let mut client = PipeClient::connect(tag, std::process::id(), Duration::from_secs(2)).unwrap();
    client.authenticate(&Secret::from_bytes(KEY)).unwrap();
    // Worker auth ACK remains unread. Raw writes are solely negative/protocol tests.
    client
}

#[test]
fn separate_control_pcm_connections_persist_across_multiple_bidirectional_frames() {
    let start = |expected: Vec<u8>| {
        spawn_server(move |server| {
            server.accept(std::process::id(), deadline()).unwrap();
            for _ in 0..3 {
                assert_eq!(server.read_frame(1024, deadline()).unwrap(), expected);
                server.write_frame(&expected, 1024, deadline()).unwrap();
            }
            // Peer consumption is confirmed before explicit close discards kernel buffers.
            assert_eq!(server.read_frame(16, deadline()).unwrap(), b"consumed");
            assert!(server.is_connected());
            server.close();
            assert!(!server.is_connected());
        })
    };
    let control = b"heartbeat".to_vec();
    let pcm = vec![0x5a; 257];
    let (control_tag, control_join) = start(control.clone());
    let (pcm_tag, pcm_join) = start(pcm.clone());
    assert_ne!(control_tag, pcm_tag);
    let mut control_client = client(&control_tag);
    let mut pcm_client = client(&pcm_tag);
    for _ in 0..3 {
        control_client
            .write_frame(&control, 1024, deadline())
            .unwrap();
        pcm_client.write_frame(&pcm, 1024, deadline()).unwrap();
        assert_eq!(
            control_client.read_frame(1024, deadline()).unwrap(),
            control
        );
        assert_eq!(pcm_client.read_frame(1024, deadline()).unwrap(), pcm);
    }
    control_client
        .write_frame(b"consumed", 16, deadline())
        .unwrap();
    pcm_client.write_frame(b"consumed", 16, deadline()).unwrap();
    control_join.join().unwrap();
    pcm_join.join().unwrap();
    control_client.close();
    pcm_client.close();
    assert!(!control_client.is_connected());
    assert!(!pcm_client.is_connected());
}

#[test]
fn first_instance_and_local_endpoint_tag_guards_are_real() {
    let tag = tag();
    let mut server = WorkerPipeServer::bind(&tag, Secret::from_bytes(KEY)).unwrap();
    assert!(WorkerPipeServer::bind(&tag, Secret::from_bytes(KEY)).is_err());
    assert!(WorkerPipeServer::bind(r"\\remote\pipe\bad", Secret::from_bytes(KEY)).is_err());
    server.close();
    let error = server.accept(std::process::id(), deadline()).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::NotConnected);
    assert!(!server.is_connected());
    // New explicit bind is possible after the old unique handle really closes.
    let rebound = WorkerPipeServer::bind(&tag, Secret::from_bytes(KEY)).unwrap();
    drop(rebound);
}

#[test]
fn wrong_worker_pid_is_denied_before_authenticated_connection() {
    let (tag, join) = spawn_server(|server| {
        let error = server
            .accept(std::process::id() + 1, deadline())
            .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
        assert!(!server.is_connected());
        assert_eq!(
            server.read_frame(16, deadline()).unwrap_err().kind(),
            io::ErrorKind::NotConnected
        );
    });
    assert!(
        WorkerPipeClient::connect(
            &tag,
            std::process::id(),
            &Secret::from_bytes(KEY),
            deadline()
        )
        .is_err()
    );
    join.join().unwrap();
}

#[test]
fn wrong_node_pid_is_denied_by_kernel_peer_identity_check() {
    let (tag, join) = spawn_server(|server| {
        assert!(server.accept(std::process::id(), deadline()).is_err());
        assert!(!server.is_connected());
    });
    let error = WorkerPipeClient::connect(
        &tag,
        std::process::id() + 1,
        &Secret::from_bytes(KEY),
        deadline(),
    )
    .err()
    .unwrap();
    assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
    join.join().unwrap();
}

#[test]
fn wrong_token_never_completes_client_authentication() {
    let (tag, join) = spawn_server(|server| {
        assert_eq!(
            server
                .accept(std::process::id(), deadline())
                .unwrap_err()
                .kind(),
            io::ErrorKind::PermissionDenied
        );
        assert!(!server.is_connected());
    });
    assert!(
        WorkerPipeClient::connect(
            &tag,
            std::process::id(),
            &Secret::from_bytes([8; 32]),
            deadline()
        )
        .is_err()
    );
    join.join().unwrap();
}

#[test]
fn startup_connection_wait_is_inside_absolute_deadline_and_cannot_reaccept() {
    let tag = tag();
    let mut server = WorkerPipeServer::bind(&tag, Secret::from_bytes(KEY)).unwrap();
    let start = Instant::now();
    let error = server
        .accept(std::process::id(), start + Duration::from_millis(100))
        .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::TimedOut);
    assert!(start.elapsed() < Duration::from_secs(1));
    assert!(!server.is_connected());
    assert_eq!(
        server
            .accept(std::process::id(), deadline())
            .unwrap_err()
            .kind(),
        io::ErrorKind::NotConnected
    );
    let start = Instant::now();
    let error = WorkerPipeClient::connect(
        &tag,
        std::process::id(),
        &Secret::from_bytes(KEY),
        start + Duration::from_millis(100),
    )
    .err()
    .unwrap();
    assert_eq!(error.kind(), io::ErrorKind::TimedOut);
    assert!(start.elapsed() < Duration::from_secs(1));
}

#[test]
fn partial_token_uses_remaining_startup_budget_and_closes() {
    let (done_tx, done_rx) = mpsc::sync_channel(1);
    let (tag, join) = spawn_server(move |server| {
        let start = Instant::now();
        assert_eq!(
            server
                .accept(std::process::id(), start + Duration::from_millis(150))
                .unwrap_err()
                .kind(),
            io::ErrorKind::TimedOut
        );
        assert!(start.elapsed() < Duration::from_secs(1));
        assert!(!server.is_connected());
        done_tx.send(()).unwrap();
    });
    let mut client = PipeClient::connect(&tag, std::process::id(), Duration::from_secs(2)).unwrap();
    client.write_fragment(&KEY[..8]).unwrap();
    done_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    drop(client);
    join.join().unwrap();
}

#[test]
fn header_limits_are_rejected_without_waiting_for_or_allocating_payload() {
    for (length, maximum) in [
        (0, 16),
        (17, 16),
        ((CONTROL_MAX_BYTES + 1) as u32, CONTROL_MAX_BYTES),
    ] {
        let (done_tx, done_rx) = mpsc::sync_channel(1);
        let (tag, join) = spawn_server(move |server| {
            server.accept(std::process::id(), deadline()).unwrap();
            let error = server.read_frame(maximum, deadline()).unwrap_err();
            assert_eq!(error.kind(), io::ErrorKind::InvalidData);
            assert!(!server.is_connected());
            done_tx.send(()).unwrap();
        });
        let mut client = raw_client(&tag);
        client.write_fragment(&length.to_be_bytes()).unwrap();
        // Client remains open and supplies no body. Rejection must precede body read.
        done_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        drop(client);
        join.join().unwrap();
    }
}

#[test]
fn raw_big_endian_frame_and_fragmented_body_decode_exactly() {
    let payload = vec![0x3c; 257];
    let expected = payload.clone();
    let (done_tx, done_rx) = mpsc::sync_channel(1);
    let (tag, join) = spawn_server(move |server| {
        server.accept(std::process::id(), deadline()).unwrap();
        assert_eq!(server.read_frame(1024, deadline()).unwrap(), expected);
        done_tx.send(()).unwrap();
    });
    let mut client = raw_client(&tag);
    client.write_fragment(&257u32.to_be_bytes()[..2]).unwrap();
    client.write_fragment(&257u32.to_be_bytes()[2..]).unwrap();
    client.write_fragment(&payload[..31]).unwrap();
    client.write_fragment(&payload[31..]).unwrap();
    done_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    drop(client);
    join.join().unwrap();
}

#[test]
fn partial_header_and_body_share_one_absolute_frame_deadline() {
    let (accepted_tx, accepted_rx) = mpsc::sync_channel(1);
    let (done_tx, done_rx) = mpsc::sync_channel(1);
    let (tag, join) = spawn_server(move |server| {
        server.accept(std::process::id(), deadline()).unwrap();
        let start = Instant::now();
        accepted_tx.send(()).unwrap();
        assert_eq!(
            server
                .read_frame(64, start + Duration::from_millis(150))
                .unwrap_err()
                .kind(),
            io::ErrorKind::TimedOut
        );
        assert!(start.elapsed() < Duration::from_secs(1));
        assert!(!server.is_connected());
        done_tx.send(()).unwrap();
    });
    let mut client = raw_client(&tag);
    accepted_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    let header = 8u32.to_be_bytes();
    client.write_fragment(&header[..2]).unwrap();
    thread::sleep(Duration::from_millis(20));
    client.write_fragment(&header[2..]).unwrap();
    client.write_fragment(&[1, 2]).unwrap();
    done_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    drop(client);
    join.join().unwrap();
}

#[test]
fn eof_mid_frame_closes_endpoint_without_waiting_for_deadline() {
    let (accepted_tx, accepted_rx) = mpsc::sync_channel(1);
    let (tag, join) = spawn_server(move |server| {
        server.accept(std::process::id(), deadline()).unwrap();
        accepted_tx.send(()).unwrap();
        let error = server.read_frame(64, deadline()).unwrap_err();
        assert_ne!(error.kind(), io::ErrorKind::TimedOut);
        assert!(!server.is_connected());
        assert_eq!(
            server.read_frame(64, deadline()).unwrap_err().kind(),
            io::ErrorKind::NotConnected
        );
    });
    let mut client = raw_client(&tag);
    accepted_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    client.write_fragment(&8u32.to_be_bytes()).unwrap();
    client.write_fragment(&[1, 2]).unwrap();
    drop(client);
    join.join().unwrap();
}

#[test]
fn client_read_timeout_and_invalid_writes_close_the_connection() {
    for mode in 0..4 {
        let (tag, join) = spawn_server(|server| {
            server.accept(std::process::id(), deadline()).unwrap();
            assert!(server.read_frame(64, deadline()).is_err());
            assert!(!server.is_connected());
        });
        let mut client = client(&tag);
        let error = match mode {
            0 => client
                .read_frame(64, Instant::now() + Duration::from_millis(100))
                .unwrap_err(),
            1 => client.write_frame(b"a", 0, deadline()).unwrap_err(),
            2 => client
                .write_frame(b"a", CONTROL_MAX_BYTES + 1, deadline())
                .unwrap_err(),
            _ => client.write_frame(&[0; 17], 16, deadline()).unwrap_err(),
        };
        assert_eq!(
            error.kind(),
            if mode == 0 {
                io::ErrorKind::TimedOut
            } else {
                io::ErrorKind::InvalidInput
            }
        );
        assert!(!client.is_connected());
        assert_eq!(
            client.write_frame(b"a", 16, deadline()).unwrap_err().kind(),
            io::ErrorKind::NotConnected
        );
        join.join().unwrap();
    }
}

#[test]
fn expired_or_excessive_deadline_never_starts_a_frame_and_closes_client() {
    for late in [false, true] {
        let (tag, join) = spawn_server(|server| {
            server.accept(std::process::id(), deadline()).unwrap();
            assert!(server.read_frame(64, deadline()).is_err());
            assert!(!server.is_connected());
        });
        let mut client = client(&tag);
        let d = if late {
            Instant::now() - Duration::from_millis(1)
        } else {
            Instant::now() + Duration::from_secs(4)
        };
        let error = client.write_frame(b"x", 16, d).unwrap_err();
        assert_eq!(
            error.kind(),
            if late {
                io::ErrorKind::TimedOut
            } else {
                io::ErrorKind::InvalidInput
            }
        );
        assert!(!client.is_connected());
        join.join().unwrap();
    }
}
