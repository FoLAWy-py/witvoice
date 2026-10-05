//! Explicit bounded native DNS-SD smoke harness. Compiling does not run it.
//! Does not listen for QUIC/audio and does not pair or trust a discovered node.
#[cfg(windows)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use std::{
        net::SocketAddrV4,
        time::{Duration, Instant},
    };
    use witvoice_transport::discovery::{NativePeers, manual_address};
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() != 6 || args[0] != "--allow-lan" {
        return Err("usage: native_discovery_probe --allow-lan INTERFACE NODE_UUID browse|advertise|resolve ADDRESS_OR_SERVICE DURATION_MS(1..9000)".into());
    }
    let interface: u32 = args[1].parse()?;
    if interface == 0 {
        return Err("explicit nonzero interface required".into());
    }
    let node: witvoice_contracts::values::Id = args[2].clone().try_into()?;
    let duration: u64 = args[5].parse()?;
    if !(1..=9000).contains(&duration) {
        return Err("duration must be 1..9000ms".into());
    }
    let started = Instant::now();
    let end = started + Duration::from_millis(duration);
    let mut peers = NativePeers::default();
    let request = match args[3].as_str() {
        "browse" if args[4] == "_voice-node._udp.local" => peers.browse(true, interface),
        "resolve" => peers.resolve(true, interface, &args[4]),
        "advertise" => {
            let address = manual_address(&args[4])?;
            let address: SocketAddrV4 = match address {
                std::net::SocketAddr::V4(v4) => v4,
                _ => return Err("advertisement probe currently accepts IPv4 only".into()),
            };
            peers.advertise(true, interface, &node, address)
        }
        _ => return Err("fixed service and explicit operation required".into()),
    };
    request.map_err(|e| format!("native request: {e:?}"))?;
    let mut observations = 0usize;
    while Instant::now() < end {
        observations = observations.saturating_add(peers.poll()?.len());
        let remaining = end.saturating_duration_since(Instant::now());
        std::thread::sleep(remaining.min(Duration::from_millis(20)));
    }
    let mut complete = peers
        .close()
        .map_err(|e| format!("native cancellation: {e:?}"))?;
    while !complete && started.elapsed() < Duration::from_secs(10) {
        peers.poll()?;
        let remaining = Duration::from_secs(10).saturating_sub(started.elapsed());
        std::thread::sleep(remaining.min(Duration::from_millis(20)));
        complete = peers
            .close()
            .map_err(|e| format!("native cancellation: {e:?}"))?;
    }
    println!(
        "observations={observations} rejected={} pending={} cleanup_confirmed={complete}",
        peers.rejected_events(),
        peers.pending_contexts()
    );
    if !complete {
        return Err(
            "native terminal callback unconfirmed; contexts quarantined until process exit".into(),
        );
    }
    Ok(())
}
#[cfg(not(windows))]
fn main() {
    eprintln!("Windows DNS-SD harness unsupported on this platform; Mac validation not performed");
    std::process::exit(2);
}
