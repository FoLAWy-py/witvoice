//! Explicit bounded native DNS-SD evidence harness. Building/tests never enter OS DNS.
//! No QUIC listener, microphone, pairing, trust, or private identity is involved.
#[cfg(windows)]
mod probe {
    use std::{
        net::{SocketAddr, SocketAddrV4},
        time::{Duration, Instant},
    };
    use witvoice_contracts::values::Id;
    use witvoice_platform::discovery::FailureSnapshot;
    use witvoice_platform::discovery::{DiscoveryError, Event, NativeDiscovery, SERVICE_TYPE};
    use witvoice_transport::discovery::manual_address;

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum Mode {
        Advertise,
        Browse,
    }
    struct Config {
        interface: u32,
        node: Id,
        address: SocketAddrV4,
        duration: Duration,
        mode: Mode,
    }
    impl Config {
        fn parse(args: &[String]) -> Result<Self, Box<dyn std::error::Error>> {
            if args.len() != 6 || args[0] != "--allow-lan" {
                return Err("usage: native_discovery_probe --allow-lan INTERFACE TEST_UUID advertise|browse EXACT_PRIVATE_IPV4_ENDPOINT DURATION_MS(1..9000)".into());
            }
            let interface: u32 = args[1].parse()?;
            if interface == 0 {
                return Err("explicit nonzero interface required".into());
            }
            let node = args[2].clone().try_into()?;
            let mode = match args[3].as_str() {
                "advertise" => Mode::Advertise,
                "browse" => Mode::Browse,
                _ => return Err("only advertise or fixed-service browse supported".into()),
            };
            let SocketAddr::V4(address) = manual_address(&args[4])? else {
                return Err("probe requires exact private IPv4 endpoint".into());
            };
            let milliseconds: u64 = args[5].parse()?;
            if !(1..=9000).contains(&milliseconds) {
                return Err("duration must be 1..9000ms".into());
            }
            Ok(Self {
                interface,
                node,
                address,
                duration: Duration::from_millis(milliseconds),
                mode,
            })
        }
    }
    struct Observer {
        config: Config,
        full_name: String,
        registered: bool,
        matched_expiry: Option<Instant>,
        actual_once: Option<serde_json::Value>,
        resolve_attempted: bool,
        requested_expiry: Option<Instant>,
        closing: bool,
        ignored: u64,
        failures: u64,
        first_failure: Option<String>,
        dropped: u64,
        failure_detail: Option<FailureSnapshot>,
    }
    impl Observer {
        fn new(config: Config) -> Self {
            let full_name = format!("{}.{}", String::from(config.node.clone()), SERVICE_TYPE);
            Self {
                config,
                full_name,
                registered: false,
                matched_expiry: None,
                actual_once: None,
                resolve_attempted: false,
                requested_expiry: None,
                closing: false,
                ignored: 0,
                failures: 0,
                first_failure: None,
                dropped: 0,
                failure_detail: None,
            }
        }
        fn fail(&mut self, message: String) {
            self.failures = self.failures.saturating_add(1);
            if self.first_failure.is_none() {
                self.first_failure = Some(message);
            }
        }
        fn observe(&mut self, event: Event, now: Instant) -> Option<(String, Instant)> {
            match event {
                Event::Failed(status) => self.fail(format!("native asynchronous status {status}")),
                Event::Stopped if !self.closing => {
                    self.fail("unexpected early terminal event".into())
                }
                Event::Registered if !self.closing && self.config.mode == Mode::Advertise => {
                    self.registered = true;
                }
                Event::Found {
                    full_name,
                    expires_at,
                } if !self.closing
                    && self.failures == 0
                    && self.dropped == 0
                    && self.failure_detail.is_none()
                    && self.config.mode == Mode::Browse =>
                {
                    if full_name.strip_suffix('.').unwrap_or(&full_name) == self.full_name {
                        if expires_at <= now {
                            self.fail("target discovery expired".into());
                        } else if !self.resolve_attempted {
                            self.resolve_attempted = true;
                            self.requested_expiry = Some(expires_at);
                            return Some((full_name, expires_at));
                        }
                    } else {
                        self.ignored = self.ignored.saturating_add(1);
                    }
                }
                Event::Resolved {
                    instance,
                    node_id,
                    address,
                    expires_at,
                } if !self.closing && self.config.mode == Mode::Browse => {
                    let target = String::from(self.config.node.clone());
                    if instance != target {
                        self.ignored = self.ignored.saturating_add(1);
                    } else if !self.resolve_attempted
                        || node_id != self.config.node
                        || address != SocketAddr::V4(self.config.address)
                        || manual_address(&address.to_string()).is_err()
                        || expires_at <= now
                        || Some(expires_at) != self.requested_expiry
                    {
                        self.fail("target resolved identity/endpoint/expiry mismatch".into());
                    } else if self.actual_once.is_none() {
                        self.matched_expiry = Some(expires_at);
                        self.actual_once = Some(serde_json::json!({
                            "instance": instance, "node_id": String::from(node_id),
                            "address": address.to_string(), "protocol_version": 1,
                            "remaining_ttl_ms": expires_at.duration_since(now).as_millis(),
                        }));
                    }
                }
                _ => (),
            }
            None
        }
        fn note_dropped(&mut self, dropped: u64) {
            self.dropped = self.dropped.max(dropped);
        }
        fn note_failure(&mut self, failure: Option<FailureSnapshot>) {
            if self.failure_detail.is_none() {
                self.failure_detail = failure;
            }
        }
        fn close_result(&mut self, result: Result<bool, DiscoveryError>) -> bool {
            match result {
                Ok(complete) => complete,
                Err(error) => {
                    self.fail(format!("native cleanup {error:?}"));
                    false
                }
            }
        }
        fn success(&self, complete: bool, pending: usize, now: Instant) -> bool {
            complete
                && pending == 0
                && self.failures == 0
                && self.dropped == 0
                && self.failure_detail.is_none()
                && match self.config.mode {
                    Mode::Advertise => self.registered,
                    Mode::Browse => self.matched_expiry.is_some_and(|expiry| expiry > now),
                }
        }
    }
    fn poll(native: &mut NativeDiscovery, observer: &mut Observer) {
        observer.note_failure(native.first_failure());
        for event in native.poll() {
            if let Some((name, expiry)) = observer.observe(event, Instant::now())
                && let Err(error) =
                    native.resolve_until(true, observer.config.interface, &name, expiry)
            {
                observer.fail(format!("native target resolve {error:?}"));
            }
        }
        observer.note_dropped(native.dropped_events());
        observer.note_failure(native.first_failure());
    }
    pub fn run() -> Result<(), Box<dyn std::error::Error>> {
        let args: Vec<String> = std::env::args().skip(1).take(7).collect();
        let config = Config::parse(&args)?;
        let mut observer = Observer::new(config);
        let mut native = NativeDiscovery::default();
        let started = Instant::now();
        let end = started + observer.config.duration;
        let hard_end = started + Duration::from_secs(10);
        let request = match observer.config.mode {
            Mode::Browse => native.browse(true, observer.config.interface),
            Mode::Advertise => native.advertise(
                true,
                observer.config.interface,
                &observer.config.node,
                observer.config.address,
            ),
        };
        if let Err(error) = request {
            observer.fail(format!("native start {error:?}"));
        }
        while Instant::now() < end
            && observer.failures == 0
            && observer.dropped == 0
            && observer.failure_detail.is_none()
        {
            poll(&mut native, &mut observer);
            std::thread::sleep(
                end.saturating_duration_since(Instant::now())
                    .min(Duration::from_millis(20)),
            );
        }
        // Every post-submission failure takes this same bounded cancellation path.
        observer.closing = true;
        let mut complete = observer.close_result(native.close());
        loop {
            poll(&mut native, &mut observer);
            if complete || Instant::now() >= hard_end {
                break;
            }
            std::thread::sleep(
                hard_end
                    .saturating_duration_since(Instant::now())
                    .min(Duration::from_millis(20)),
            );
            complete = observer.close_result(native.close());
        }
        let pending = native.pending_contexts();
        let now = Instant::now();
        let success = observer.success(complete, pending, now) && now <= hard_end;
        let failure_detail = observer.failure_detail.map(|failure| {
            serde_json::json!({
                "stage": format!("{:?}", failure.stage),
                "origin": format!("{:?}", failure.origin),
                "callback_status": failure.callback_status,
                "reported_status": failure.reported_status,
                "reason": failure.reason.map(|reason| format!("{reason:?}")),
                "text_error": failure.text_error.map(|reason| format!("{reason:?}")),
            })
        });
        println!(
            "{}",
            serde_json::json!({
                "operation": format!("{:?}", observer.config.mode).to_lowercase(),
                "interface": observer.config.interface, "node_id": String::from(observer.config.node.clone()),
                "expected_endpoint": observer.config.address.to_string(),
                "registered": observer.registered, "matched": observer.matched_expiry.is_some(),
                "matched_remaining_ttl_ms": observer.matched_expiry.map(|expiry| expiry.saturating_duration_since(now).as_millis()),
                "actual_once": observer.actual_once, "resolve_attempted": observer.resolve_attempted,
                "ignored_foreign_events": observer.ignored,
                "failure_first": observer.first_failure.or_else(|| observer.failure_detail.map(|failure| format!("native retained failure {}", failure.reported_status))),
                "failure_count": observer.failures.max(u64::from(observer.failure_detail.is_some())), "dropped": observer.dropped,
                "failure_detail": failure_detail,
                "pending": pending, "cleanup_confirmed": complete,
                "elapsed_ms": now.duration_since(started).as_millis(), "success": success,
            })
        );
        if !success {
            return Err(
                "required native evidence or bounded terminal cleanup not confirmed".into(),
            );
        }
        Ok(())
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        fn observer(mode: Mode) -> Observer {
            Observer::new(
                Config::parse(&[
                    "--allow-lan".into(),
                    "19".into(),
                    "00000000-0000-0000-0000-000000000001".into(),
                    if mode == Mode::Advertise {
                        "advertise"
                    } else {
                        "browse"
                    }
                    .into(),
                    "192.168.1.10:45678".into(),
                    "100".into(),
                ])
                .unwrap(),
            )
        }
        fn resolved(o: &Observer, expiry: Instant) -> Event {
            Event::Resolved {
                instance: String::from(o.config.node.clone()),
                node_id: o.config.node.clone(),
                address: SocketAddr::V4(o.config.address),
                expires_at: expiry,
            }
        }
        fn target(o: &mut Observer, now: Instant) {
            assert!(
                o.observe(
                    Event::Found {
                        full_name: o.full_name.clone(),
                        expires_at: now + Duration::from_secs(5)
                    },
                    now
                )
                .is_some()
            );
        }
        #[test]
        fn registered_required_and_async_failure_is_sticky() {
            let now = Instant::now();
            let mut o = observer(Mode::Advertise);
            assert!(!o.success(true, 0, now));
            o.observe(Event::Registered, now);
            assert!(o.success(true, 0, now));
            o.observe(Event::Failed(1234), now);
            o.observe(Event::Registered, now);
            assert!(!o.success(true, 0, now));
            assert_eq!(
                o.first_failure.as_deref(),
                Some("native asynchronous status 1234")
            );
        }
        #[test]
        fn foreign_discovery_never_resolves_or_matches() {
            let now = Instant::now();
            let mut o = observer(Mode::Browse);
            assert!(
                o.observe(
                    Event::Found {
                        full_name: "other._voice-node._udp.local".into(),
                        expires_at: now + Duration::from_secs(5)
                    },
                    now
                )
                .is_none()
            );
            let mut event = resolved(&o, now + Duration::from_secs(5));
            if let Event::Resolved { instance, .. } = &mut event {
                *instance = "other".into();
            }
            o.observe(event, now);
            assert!(!o.resolve_attempted);
            assert!(!o.success(true, 0, now));
            assert_eq!(o.ignored, 2);
        }
        #[test]
        fn only_one_target_resolve_and_original_expiry_preserved() {
            let now = Instant::now();
            let mut o = observer(Mode::Browse);
            let expiry = now + Duration::from_secs(5);
            let event = Event::Found {
                full_name: o.full_name.clone(),
                expires_at: expiry,
            };
            assert_eq!(
                o.observe(event.clone(), now),
                Some((o.full_name.clone(), expiry))
            );
            assert!(o.observe(event, now).is_none());
            o.observe(resolved(&o, expiry), now);
            assert!(o.success(true, 0, now));
            assert!(!o.success(true, 0, expiry));
        }
        #[test]
        fn wrong_endpoint_node_and_expiry_are_rejected() {
            let now = Instant::now();
            for variant in 0..4 {
                let mut o = observer(Mode::Browse);
                target(&mut o, now);
                let mut event = resolved(&o, now + Duration::from_secs(5));
                if let Event::Resolved {
                    address,
                    node_id,
                    expires_at,
                    ..
                } = &mut event
                {
                    match variant {
                        0 => *address = "192.168.1.11:45678".parse().unwrap(),
                        1 => {
                            *node_id = "00000000-0000-0000-0000-000000000002"
                                .to_owned()
                                .try_into()
                                .unwrap()
                        }
                        2 => *expires_at = now,
                        _ => *expires_at = now + Duration::from_secs(6),
                    }
                }
                o.observe(event, now);
                assert!(!o.success(true, 0, now));
                assert_eq!(o.failures, 1);
            }
        }
        #[test]
        fn drops_and_unconfirmed_cleanup_prevent_success() {
            let now = Instant::now();
            let mut o = observer(Mode::Advertise);
            o.observe(Event::Registered, now);
            assert!(!o.success(false, 0, now));
            assert!(!o.success(true, 1, now));
            o.note_dropped(1);
            o.note_dropped(0);
            assert!(!o.success(true, 0, now));
            assert_eq!(o.dropped, 1);
        }
        #[test]
        fn expired_found_and_prior_failure_never_start_target_resolution() {
            let now = Instant::now();
            let mut o = observer(Mode::Browse);
            assert!(
                o.observe(
                    Event::Found {
                        full_name: o.full_name.clone(),
                        expires_at: now,
                    },
                    now
                )
                .is_none()
            );
            assert_eq!(o.failures, 1);
            assert!(!o.resolve_attempted);
            assert!(
                o.observe(
                    Event::Found {
                        full_name: o.full_name.clone(),
                        expires_at: now + Duration::from_secs(5),
                    },
                    now
                )
                .is_none()
            );
            assert!(!o.resolve_attempted);
            assert!(!o.success(true, 0, now));
        }
        #[test]
        fn early_stopped_and_post_close_evidence_cannot_be_success() {
            let now = Instant::now();
            let mut o = observer(Mode::Advertise);
            o.observe(Event::Stopped, now);
            o.observe(Event::Registered, now);
            assert!(!o.success(true, 0, now));
            let mut o = observer(Mode::Advertise);
            o.closing = true;
            o.observe(Event::Stopped, now);
            o.observe(Event::Registered, now);
            assert_eq!(o.failures, 0);
            assert!(!o.success(true, 0, now));
        }
        #[test]
        fn start_and_cleanup_errors_are_retained_not_overridden() {
            let now = Instant::now();
            let mut o = observer(Mode::Advertise);
            o.fail("native start error".into());
            assert!(!o.close_result(Err(DiscoveryError::Native(5))));
            assert!(o.close_result(Ok(true)));
            o.observe(Event::Registered, now);
            assert!(!o.success(true, 0, now));
            assert_eq!(o.first_failure.as_deref(), Some("native start error"));
            assert_eq!(o.failures, 2);
        }
        #[test]
        fn strict_arguments_reject_unapproved_interfaces_public_addresses_and_unbounded_time() {
            let original = [
                "--allow-lan",
                "19",
                "00000000-0000-0000-0000-000000000001",
                "browse",
                "192.168.1.10:45678",
                "100",
            ]
            .map(String::from);
            for (index, value) in [
                (0, "--auto"),
                (1, "0"),
                (2, "not-uuid"),
                (3, "resolve"),
                (4, "8.8.8.8:45678"),
                (4, "example.com:45678"),
                (4, "[fc00::1]:45678"),
                (5, "0"),
                (5, "9001"),
            ] {
                let mut args = original.clone();
                args[index] = value.into();
                assert!(Config::parse(&args).is_err());
            }
        }
        #[test]
        fn retained_diagnostic_blocks_success_without_a_queued_failed_event() {
            use witvoice_platform::discovery::{FailureOrigin, FailureStage, ValidationReason};
            let now = Instant::now();
            let mut o = observer(Mode::Advertise);
            let first = FailureSnapshot {
                stage: FailureStage::Resolve,
                origin: FailureOrigin::LocalValidation,
                callback_status: Some(0),
                reported_status: 13,
                reason: Some(ValidationReason::InterfaceMismatch),
                text_error: None,
            };
            o.note_failure(Some(first));
            o.observe(Event::Registered, now);
            o.note_failure(None);
            o.note_failure(Some(FailureSnapshot {
                callback_status: Some(5),
                ..first
            }));
            assert_eq!(o.failure_detail, Some(first));
            assert!(!o.success(true, 0, now));
            let mut o = observer(Mode::Browse);
            o.note_failure(Some(first));
            assert!(
                o.observe(
                    Event::Found {
                        full_name: o.full_name.clone(),
                        expires_at: now + Duration::from_secs(5)
                    },
                    now
                )
                .is_none()
            );
            assert!(!o.resolve_attempted);
        }
    }
}
#[cfg(windows)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    probe::run()
}
#[cfg(not(windows))]
fn main() {
    eprintln!("Windows DNS-SD probe unsupported; Mac validation not performed");
    std::process::exit(2);
}
