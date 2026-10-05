use super::*;
use std::time::Instant;
use witvoice_platform::discovery::{
    DiscoveryError, Event, FailureSnapshot, NativeDiscovery, validate_full_name,
};

#[derive(Clone, Copy)]
struct Fault {
    reported_status: u32,
    snapshot: Option<FailureSnapshot>,
}

/// Real Windows adapter. Addresses stay untrusted; this type has no TrustStore access.
pub struct NativePeers {
    native: NativeDiscovery,
    records: DiscoveryRecords,
    started: Instant,
    interface: Option<u32>,
    rejected: u64,
    first_fault: Option<Fault>,
}
impl Default for NativePeers {
    fn default() -> Self {
        Self {
            native: NativeDiscovery::default(),
            records: DiscoveryRecords::default(),
            started: Instant::now(),
            interface: None,
            rejected: 0,
            first_fault: None,
        }
    }
}
impl NativePeers {
    pub fn browse(
        &mut self,
        explicit_lan_approval: bool,
        interface: u32,
    ) -> Result<(), DiscoveryError> {
        self.reject_fault()?;
        self.native.browse(explicit_lan_approval, interface)?;
        self.interface = Some(interface);
        Ok(())
    }
    pub fn resolve(
        &mut self,
        explicit_lan_approval: bool,
        interface: u32,
        full_name: &str,
    ) -> Result<(), DiscoveryError> {
        self.reject_fault()?;
        self.native
            .resolve(explicit_lan_approval, interface, full_name, 120)?;
        self.interface = Some(interface);
        Ok(())
    }
    pub fn advertise(
        &mut self,
        explicit_lan_approval: bool,
        interface: u32,
        node_id: &witvoice_contracts::values::Id,
        address: std::net::SocketAddrV4,
    ) -> Result<(), DiscoveryError> {
        self.reject_fault()?;
        self.native
            .advertise(explicit_lan_approval, interface, node_id, address)?;
        self.interface = Some(interface);
        Ok(())
    }
    fn apply(&mut self, event: Event, now: Instant) -> Result<(), Error> {
        if let Event::Failed(status) = event {
            self.latch_fault(status, None);
            return Err(Error::NativeDiscoveryFailed(
                self.first_fault.unwrap().reported_status,
            ));
        }
        if let Event::Stopped = event {
            self.records.records.clear();
            self.interface = None;
            return Ok(());
        }
        if let Some(fault) = self.first_fault {
            return Err(Error::NativeDiscoveryFailed(fault.reported_status));
        }
        let elapsed = now
            .checked_duration_since(self.started)
            .ok_or(Error::ClockWentBackwards)?;
        if self.interface.is_none()
            && matches!(
                &event,
                Event::Found { .. } | Event::Resolved { .. } | Event::Registered
            )
        {
            return Err(Error::InvalidIdentity);
        }
        match event {
            Event::Found {
                full_name,
                expires_at,
            } => {
                validate_full_name(&full_name).map_err(|_| Error::InvalidIdentity)?;
                let ttl = expires_at.saturating_duration_since(now).as_secs().min(120) as u32;
                if ttl == 0 {
                    return Err(Error::InvalidIdentity);
                }
                self.native
                    .resolve_until(
                        true,
                        self.interface.ok_or(Error::InvalidIdentity)?,
                        &full_name,
                        expires_at,
                    )
                    .map_err(|_| Error::Limit)?;
            }
            Event::Removed { full_name } => {
                let name = validate_full_name(&full_name).map_err(|_| Error::InvalidIdentity)?;
                self.records.expire(elapsed)?;
                self.records.records.remove(name);
            }
            Event::Resolved {
                instance,
                node_id,
                address,
                expires_at,
            } => {
                let ttl = expires_at
                    .checked_duration_since(now)
                    .ok_or(Error::InvalidIdentity)?;
                self.records.observe(
                    Advertisement {
                        service_type: SERVICE_TYPE.into(),
                        instance,
                        node_id,
                        protocol_version: 1,
                        address,
                    },
                    ttl.min(MAX_TTL),
                    elapsed,
                )?;
            }
            Event::Stopped | Event::Failed(_) => {
                unreachable!("handled before any clock/record work")
            }
            Event::Registered => (),
        }
        Ok(())
    }
    pub fn poll(&mut self) -> Result<Vec<Advertisement>, Error> {
        // Snapshot is independent of the bounded queue: observe it both sides of drain.
        let before = self.native.first_failure();
        let events = self.native.poll();
        let after = self.native.first_failure();
        let now = Instant::now();
        self.consume(events, before, after, now)
    }
    fn consume(
        &mut self,
        events: Vec<Event>,
        before: Option<FailureSnapshot>,
        after: Option<FailureSnapshot>,
        now: Instant,
    ) -> Result<Vec<Advertisement>, Error> {
        self.capture_snapshot(before);
        self.capture_snapshot(after);
        for event in events {
            if self.apply(event, now).is_err() {
                self.rejected = self.rejected.saturating_add(1);
            }
            // Found can submit a resolve whose completion happens during apply.
            // Observe retained failures before processing any next success event.
            self.capture_snapshot(self.native.first_failure());
        }
        self.capture_snapshot(self.native.first_failure());
        if let Some(fault) = self.first_fault {
            return Err(Error::NativeDiscoveryFailed(fault.reported_status));
        }
        self.records.snapshot(
            now.checked_duration_since(self.started)
                .ok_or(Error::ClockWentBackwards)?,
        )
    }
    fn latch_fault(&mut self, reported_status: u32, snapshot: Option<FailureSnapshot>) {
        if self.first_fault.is_none() {
            self.first_fault = Some(Fault {
                reported_status,
                snapshot,
            });
        }
        self.records.records.clear();
    }
    fn capture_snapshot(&mut self, snapshot: Option<FailureSnapshot>) {
        if let Some(snapshot) = snapshot {
            self.latch_fault(snapshot.reported_status, Some(snapshot));
        }
    }
    fn reject_fault(&mut self) -> Result<(), DiscoveryError> {
        self.capture_snapshot(self.native.first_failure());
        match self.first_fault {
            Some(fault) => Err(DiscoveryError::Native(fault.reported_status)),
            None => Ok(()),
        }
    }
    /// Compatibility Event code, not proof that it came directly from the OS.
    /// Consult failure_snapshot for actual callback status, stage, origin and reason.
    pub fn failure_status(&self) -> Option<u32> {
        self.first_fault.map(|fault| fault.reported_status)
    }
    /// Immutable typed first failure if observed; missing metadata remains unknown.
    pub fn failure_snapshot(&self) -> Option<FailureSnapshot> {
        self.first_fault.and_then(|fault| fault.snapshot)
    }
    /// false means cancellation is pending/quarantined; never report it as cleanup complete.
    pub fn close(&mut self) -> Result<bool, DiscoveryError> {
        self.capture_snapshot(self.native.first_failure());
        self.interface = None;
        self.records.records.clear();
        let result = self.native.close();
        self.capture_snapshot(self.native.first_failure());
        result
    }
    pub fn rejected_events(&self) -> u64 {
        self.rejected.saturating_add(self.native.dropped_events())
    }
    pub fn pending_contexts(&self) -> usize {
        self.native.pending_contexts()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use witvoice_platform::discovery::{FailureOrigin, FailureStage, ValidationReason};
    fn configured() -> NativePeers {
        NativePeers {
            interface: Some(19),
            ..Default::default()
        }
    }
    fn resolved(now: Instant) -> Event {
        Event::Resolved {
            instance: "node".into(),
            node_id: "00000000-0000-0000-0000-000000000001"
                .to_owned()
                .try_into()
                .unwrap(),
            address: "192.168.1.4:4000".parse().unwrap(),
            expires_at: now + Duration::from_secs(10),
        }
    }
    fn local_failure() -> FailureSnapshot {
        FailureSnapshot {
            stage: FailureStage::Resolve,
            origin: FailureOrigin::LocalValidation,
            callback_status: Some(0),
            reported_status: 13,
            reason: Some(ValidationReason::InterfaceMismatch),
            text_error: None,
        }
    }
    #[test]
    fn failed_event_blocks_same_batch_later_success_and_native_reentry_without_metadata() {
        let mut peers = configured();
        let now = Instant::now();
        peers.apply(resolved(now), now).unwrap();
        assert_eq!(peers.records.records.len(), 1);
        assert_eq!(
            peers.consume(
                vec![Event::Failed(77), resolved(now), Event::Registered],
                None,
                None,
                now
            ),
            Err(Error::NativeDiscoveryFailed(77))
        );
        assert_eq!(peers.failure_status(), Some(77));
        assert_eq!(peers.failure_snapshot(), None);
        assert!(peers.records.records.is_empty());
        assert_eq!(peers.rejected_events(), 3);
        assert_eq!(
            peers.apply(resolved(now), now),
            Err(Error::NativeDiscoveryFailed(77))
        );
        assert_eq!(
            peers.apply(Event::Registered, now),
            Err(Error::NativeDiscoveryFailed(77))
        );
        // These calls return before every actual WinDNS entry; they do not authorize a network test.
        assert_eq!(peers.browse(true, 19), Err(DiscoveryError::Native(77)));
        assert_eq!(
            peers.resolve(true, 19, "node._voice-node._udp.local"),
            Err(DiscoveryError::Native(77))
        );
        let id = "00000000-0000-0000-0000-000000000001"
            .to_owned()
            .try_into()
            .unwrap();
        assert_eq!(
            peers.advertise(true, 19, &id, "192.168.1.4:4000".parse().unwrap()),
            Err(DiscoveryError::Native(77))
        );
        assert_eq!(peers.pending_contexts(), 0);
        assert_eq!(peers.poll(), Err(Error::NativeDiscoveryFailed(77)));
    }
    #[test]
    fn retained_snapshot_without_failed_event_clears_records_before_and_after_queue_drain() {
        for after_drain in [false, true] {
            let mut peers = configured();
            let now = Instant::now();
            let snapshot = local_failure();
            peers.apply(resolved(now), now).unwrap();
            assert_eq!(peers.records.records.len(), 1);
            let (before, after) = if after_drain {
                (None, Some(snapshot))
            } else {
                (Some(snapshot), None)
            };
            assert_eq!(
                peers.consume(vec![resolved(now), Event::Registered], before, after, now),
                Err(Error::NativeDiscoveryFailed(13))
            );
            assert!(peers.records.records.is_empty());
            assert_eq!(peers.failure_status(), Some(13));
            assert_eq!(peers.failure_snapshot(), Some(snapshot));
            assert_eq!(peers.failure_snapshot().unwrap().callback_status, Some(0));
            assert_eq!(peers.poll(), Err(Error::NativeDiscoveryFailed(13)));
            assert_eq!(peers.pending_contexts(), 0);
        }
    }
    #[test]
    fn first_fault_and_typed_origin_survive_later_events_and_close() {
        let mut peers = configured();
        let now = Instant::now();
        let snapshot = local_failure();
        assert_eq!(
            peers.consume(Vec::new(), Some(snapshot), None, now),
            Err(Error::NativeDiscoveryFailed(13))
        );
        let later = FailureSnapshot {
            stage: FailureStage::Browse,
            origin: FailureOrigin::OsCallback,
            callback_status: Some(55),
            reported_status: 55,
            reason: None,
            text_error: None,
        };
        assert_eq!(
            peers.consume(
                vec![
                    Event::Failed(99),
                    Event::Stopped,
                    resolved(now),
                    Event::Registered
                ],
                Some(later),
                None,
                now
            ),
            Err(Error::NativeDiscoveryFailed(13))
        );
        assert_eq!(peers.close(), Ok(true));
        assert_eq!(peers.close(), Ok(true));
        assert_eq!(peers.failure_snapshot(), Some(snapshot));
        assert_eq!(peers.failure_status(), Some(13));
        assert_eq!(peers.poll(), Err(Error::NativeDiscoveryFailed(13)));
        assert!(peers.records.records.is_empty());
    }
    #[test]
    fn zero_and_max_event_codes_remain_faults_and_unknown_metadata_is_not_fabricated() {
        for code in [0, u32::MAX] {
            let mut peers = configured();
            let now = Instant::now();
            assert_eq!(
                peers.consume(vec![Event::Failed(code)], None, None, now),
                Err(Error::NativeDiscoveryFailed(code))
            );
            assert_eq!(
                peers.consume(Vec::new(), Some(local_failure()), None, now),
                Err(Error::NativeDiscoveryFailed(code))
            );
            assert_eq!(peers.failure_status(), Some(code));
            assert_eq!(peers.failure_snapshot(), None);
            assert_eq!(peers.close(), Ok(true));
            assert_eq!(peers.poll(), Err(Error::NativeDiscoveryFailed(code)));
        }
    }
    #[test]
    fn normal_stopped_clears_records_without_creating_a_fault() {
        let mut peers = configured();
        let now = Instant::now();
        peers.apply(resolved(now), now).unwrap();
        assert_eq!(
            peers.consume(vec![Event::Stopped], None, None, now),
            Ok(Vec::new())
        );
        assert_eq!(peers.failure_status(), None);
        assert_eq!(peers.failure_snapshot(), None);
        assert!(peers.records.records.is_empty());
        assert_eq!(peers.interface, None);
        assert_eq!(peers.poll(), Ok(Vec::new()));
        assert_eq!(peers.close(), Ok(true));
        assert_eq!(
            peers.browse(false, 19),
            Err(DiscoveryError::ApprovalRequired)
        );
        assert_eq!(peers.failure_status(), None);
        assert_eq!(peers.pending_contexts(), 0);
    }
    #[test]
    fn untrusted_native_results_use_address_and_ttl_guards_without_pairing() {
        let mut peers = NativePeers {
            interface: Some(19),
            ..Default::default()
        };
        let now = Instant::now();
        let id = "00000000-0000-0000-0000-000000000001"
            .to_owned()
            .try_into()
            .unwrap();
        assert_eq!(
            peers.apply(
                Event::Resolved {
                    instance: "node".into(),
                    node_id: id,
                    address: "8.8.8.8:1".parse().unwrap(),
                    expires_at: now + Duration::from_secs(1)
                },
                now
            ),
            Err(Error::InvalidAddress)
        );
        assert!(
            peers
                .records
                .snapshot(now.duration_since(peers.started))
                .unwrap()
                .is_empty()
        );
        let id = "00000000-0000-0000-0000-000000000001"
            .to_owned()
            .try_into()
            .unwrap();
        peers
            .apply(
                Event::Resolved {
                    instance: "node".into(),
                    node_id: id,
                    address: "192.168.1.4:4000".parse().unwrap(),
                    expires_at: now + Duration::from_secs(1),
                },
                now,
            )
            .unwrap();
        assert_eq!(
            peers
                .records
                .snapshot(Duration::from_secs(2))
                .unwrap()
                .len(),
            0
        );
        assert_eq!(
            peers.apply(
                Event::Found {
                    full_name: "other.example.com".into(),
                    expires_at: now + Duration::from_secs(1)
                },
                now
            ),
            Err(Error::InvalidIdentity)
        );
        assert_eq!(peers.close(), Ok(true));
    }
    #[test]
    fn closing_cannot_restart_resolution_or_repopulate_records_from_queued_results() {
        let mut peers = NativePeers {
            interface: Some(19),
            ..Default::default()
        };
        peers.close().unwrap();
        let now = Instant::now();
        assert_eq!(
            peers.apply(
                Event::Found {
                    full_name: "node._voice-node._udp.local".into(),
                    expires_at: now + Duration::from_secs(1)
                },
                now
            ),
            Err(Error::InvalidIdentity)
        );
        let id = "00000000-0000-0000-0000-000000000001"
            .to_owned()
            .try_into()
            .unwrap();
        assert_eq!(
            peers.apply(
                Event::Resolved {
                    instance: "node".into(),
                    node_id: id,
                    address: "192.168.1.4:4000".parse().unwrap(),
                    expires_at: now + Duration::from_secs(1)
                },
                now
            ),
            Err(Error::InvalidIdentity)
        );
        assert!(peers.poll().unwrap().is_empty());
        assert_eq!(peers.pending_contexts(), 0);
    }
}
