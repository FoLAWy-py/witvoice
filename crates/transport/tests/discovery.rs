use std::time::Duration;
use witvoice_transport::{Error, discovery::*};
#[test]
fn manual_addresses_are_numeric_private_and_scoped_without_dns() {
    for allowed in [
        "10.0.0.1:1234",
        "192.168.1.4:65535",
        "172.16.0.1:2",
        "169.254.3.1:2",
        "[fd00::1]:2",
        "[fe80::1%3]:5",
    ] {
        assert!(manual_address(allowed).is_ok(), "{allowed}");
    }
    for denied in [
        "example.com:1234",
        "127.0.0.1:2",
        "0.0.0.0:2",
        "8.8.8.8:2",
        "224.0.0.1:2",
        "192.168.1.1:0",
        "[::1]:2",
        "[::]:2",
        "[2001:4860::1]:2",
        "[fe80::1]:2",
        "[fe80::1%ethernet]:2",
        "[fd00::1%3]:2",
        "[::ffff:192.168.1.1]:2",
        " 10.0.0.1:2",
        "10.0.0.1:2 ",
    ] {
        assert_eq!(
            manual_address(denied),
            Err(Error::InvalidAddress),
            "{denied}"
        );
    }
}
fn record(i: usize) -> Advertisement {
    Advertisement {
        service_type: SERVICE_TYPE.into(),
        instance: format!("node{i}"),
        node_id: "00000000-0000-0000-0000-000000000001"
            .to_owned()
            .try_into()
            .unwrap(),
        protocol_version: 1,
        address: manual_address("192.168.1.1:4000").unwrap(),
    }
}
#[test]
fn bounded_records_monotonic_expiry_and_overflow_rejection() {
    let mut records = DiscoveryRecords::default();
    for i in 0..MAX_RECORDS {
        records
            .observe(record(i), Duration::from_secs(2), Duration::ZERO)
            .unwrap();
    }
    assert_eq!(
        records.observe(record(999), Duration::from_secs(2), Duration::ZERO),
        Err(Error::Limit)
    );
    records
        .observe(record(0), Duration::from_secs(1), Duration::ZERO)
        .unwrap();
    assert_eq!(records.snapshot(Duration::from_secs(1)).unwrap().len(), 127);
    assert_eq!(
        records.snapshot(Duration::ZERO),
        Err(Error::ClockWentBackwards)
    );
    assert!(records.snapshot(Duration::from_secs(2)).unwrap().is_empty());
    assert_eq!(
        records.observe(record(2), Duration::MAX, Duration::from_secs(2)),
        Err(Error::InvalidIdentity)
    );
    assert_eq!(
        records.observe(record(2), Duration::from_secs(1), Duration::MAX),
        Err(Error::Limit)
    );
}
#[test]
fn invalid_advertisements_and_upstream_daemon_are_fail_closed() {
    let mut records = DiscoveryRecords::default();
    for mutation in 0..4 {
        let mut r = record(1);
        match mutation {
            0 => r.service_type = "_other._udp.local".into(),
            1 => r.instance = "x".repeat(64),
            2 => r.instance = "control\n".into(),
            _ => r.protocol_version = 2,
        }
        assert!(
            records
                .observe(r, Duration::from_secs(1), Duration::ZERO)
                .is_err()
        );
    }
    assert!(records.snapshot(Duration::ZERO).unwrap().is_empty());
    #[cfg(windows)]
    {
        let mut peers = NativePeers::default();
        assert_eq!(
            peers.browse(false, 19),
            Err(witvoice_platform::discovery::DiscoveryError::ApprovalRequired)
        );
        assert_eq!(
            peers.browse(true, 0),
            Err(witvoice_platform::discovery::DiscoveryError::InterfaceRequired)
        );
        assert!(peers.poll().unwrap().is_empty());
        assert_eq!(peers.close(), Ok(true));
    }
}
