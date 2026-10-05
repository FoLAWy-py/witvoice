use witvoice_platform::discovery::*;
#[test]
fn only_fixed_dns_service_single_labels_are_accepted() {
    for name in [
        "node._voice-node._udp.local",
        "node._voice-node._udp.local.",
    ] {
        assert_eq!(validate_full_name(name), Ok("node"));
    }
    for name in [
        "node.example.com",
        "a.b._voice-node._udp.local",
        "x._http._tcp.local",
        "._voice-node._udp.local",
        "x._voice-node._udp.local..",
    ] {
        assert_eq!(validate_full_name(name), Err(DiscoveryError::InvalidInput));
    }
}
#[cfg(windows)]
#[test]
fn actual_default_object_with_no_approval_never_starts_native_discovery() {
    let mut adapter = NativeDiscovery::default();
    assert_eq!(
        adapter.browse(false, 19),
        Err(DiscoveryError::ApprovalRequired)
    );
    assert_eq!(
        adapter.browse(true, 0),
        Err(DiscoveryError::InterfaceRequired)
    );
    assert!(adapter.poll().is_empty());
    assert_eq!(adapter.close(), Ok(true));
}
