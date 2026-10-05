//! Fixed-size notification invalidation, with exactly one control consumer.
//! This state never grants playback authority or acknowledges an audio epoch.

use crate::realtime::OutputGate;
use std::sync::{
    Arc, OnceLock,
    atomic::{AtomicBool, AtomicI32, AtomicU32, Ordering},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum ChangeKind {
    State = 1,
    Added = 2,
    Removed = 4,
    Default = 8,
    Property = 16,
    InitialValidation = 32,
    Closed = 64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChangeBatch(u32);
impl ChangeBatch {
    pub const fn contains(self, kind: ChangeKind) -> bool {
        self.0 & kind as u32 != 0
    }
    pub const fn bits(self) -> u32 {
        self.0
    }
}

/// The bitset coalesces events; it is not a count, queue or media deadline.
/// Invalidation is sticky and cannot be cleared by taking a notification batch.
/// One owner can claim registration once; close/failure never enables reuse.
#[derive(Debug, Default)]
pub struct ChangeSignal {
    pending: AtomicU32,
    invalidated: AtomicBool,
    // Independent of batching/baseline: once a real callback begins, a stream
    // cannot regain authority even if a control consumer drains pending bits.
    changed: AtomicBool,
    #[cfg(windows)]
    registration_claimed: AtomicBool,
    unregister_hresult: AtomicI32,
    output_gate: OnceLock<Arc<OutputGate>>,
}

impl ChangeSignal {
    pub fn new() -> Self {
        Self::default()
    }

    /// Callback path: bounded atomics and nonwaiting OnceLock::get, no allocation
    /// or ownership destruction. Binding is an ordinary control operation.
    pub fn publish(&self, kind: ChangeKind) {
        if kind != ChangeKind::InitialValidation {
            if let Some(gate) = self.output_gate.get() {
                if kind == ChangeKind::Closed {
                    gate.invalidate();
                } else {
                    gate.fail();
                }
            }
            self.changed.store(true, Ordering::Release);
        }
        self.invalidated.store(true, Ordering::Release);
        self.pending.fetch_or(kind as u32, Ordering::Release);
    }
    /// Control thread only. One gate may belong to exactly one native stream;
    /// notification teardown cannot accidentally mute a different monitor.
    pub(crate) fn bind_output_gate(&self, gate: Arc<OutputGate>) -> bool {
        if self.output_gate.get().is_some() || !gate.claim_native_owner() {
            gate.fail();
            return false;
        }
        if let Err(gate) = self.output_gate.set(gate) {
            gate.fail();
            return false;
        }
        let gate = self.output_gate.get().expect("completed OnceLock set");
        if self.has_changed() {
            gate.fail();
            return false;
        }
        true
    }
    pub(crate) fn output_gate(&self) -> Option<&Arc<OutputGate>> {
        self.output_gate.get()
    }

    /// Control-thread only, single consumer. Events linearized after the swap
    /// remain pending, including another event of the same kind during a check.
    pub fn take_batch(&self) -> Option<ChangeBatch> {
        let bits = self.pending.swap(0, Ordering::AcqRel);
        (bits != 0).then_some(ChangeBatch(bits))
    }

    pub fn is_invalidated(&self) -> bool {
        self.invalidated.load(Ordering::Acquire)
    }
    pub fn has_pending(&self) -> bool {
        self.pending.load(Ordering::Acquire) != 0
    }
    pub fn has_changed(&self) -> bool {
        self.changed.load(Ordering::Acquire)
    }

    /// One normal-thread consumer may accept the initial registration marker.
    /// No sticky flag is reset. A callback racing either side prevents reuse.
    #[cfg(any(windows, test))]
    pub(crate) fn accept_initial_baseline(&self) -> bool {
        self.take_batch()
            .is_some_and(|batch| batch.bits() == ChangeKind::InitialValidation as u32)
            && !self.has_changed()
    }
    pub fn unregister_hresult(&self) -> i32 {
        self.unregister_hresult.load(Ordering::Acquire)
    }

    #[cfg(windows)]
    fn claim_registration(&self) -> bool {
        self.registration_claimed
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    }

    #[cfg(windows)]
    fn record_unregister_error(&self, hresult: i32) {
        self.unregister_hresult.store(hresult, Ordering::Release);
        self.publish(ChangeKind::Closed);
    }
}

#[cfg(windows)]
mod native;
#[cfg(all(windows, test))]
pub(crate) use native::tests::count_allocations;
#[cfg(windows)]
pub use native::{NotificationWatch, SelectionRevalidation, WatchError};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bound_notification_invalidates_exact_output_before_cleanup_and_keeps_monitor_independent() {
        use crate::realtime::Binding;
        let mut virtual_gate = OutputGate::new(Binding::new(7, 4).unwrap()).unwrap();
        virtual_gate.arm().unwrap();
        let mut monitor_gate = OutputGate::new(Binding::new(7, 4).unwrap()).unwrap();
        monitor_gate.arm().unwrap();
        let virtual_gate = Arc::new(virtual_gate);
        let monitor_gate = Arc::new(monitor_gate);
        let virtual_signal = ChangeSignal::new();
        let monitor_signal = ChangeSignal::new();
        assert!(virtual_signal.bind_output_gate(Arc::clone(&virtual_gate)));
        assert!(monitor_signal.bind_output_gate(Arc::clone(&monitor_gate)));
        let ticket = monitor_gate.begin_commit().unwrap();
        #[cfg(windows)]
        {
            let (_, allocations) =
                super::count_allocations(|| monitor_signal.publish(ChangeKind::Removed));
            assert_eq!(allocations, 0);
        }
        #[cfg(not(windows))]
        monitor_signal.publish(ChangeKind::Removed);
        assert!(monitor_gate.is_faulted());
        assert!(!ticket.is_live());
        assert!(!monitor_gate.ack_ready());
        assert!(virtual_gate.is_live());
        drop(ticket);
        assert!(monitor_gate.ack_ready());
        virtual_signal.publish(ChangeKind::Closed);
        assert!(!virtual_gate.is_live());
        assert!(!virtual_gate.is_faulted());
        assert!(virtual_gate.ack_ready());
        assert!(!ChangeSignal::new().bind_output_gate(Arc::clone(&virtual_gate)));
    }

    #[test]
    fn baseline_does_not_clear_sticky_or_hide_a_real_change() {
        let signal = ChangeSignal::new();
        signal.publish(ChangeKind::InitialValidation);
        assert!(signal.accept_initial_baseline());
        assert!(signal.is_invalidated());
        assert!(!signal.has_changed());
        signal.publish(ChangeKind::Removed);
        assert!(signal.has_changed());
        signal.take_batch();
        assert!(signal.has_changed());
        assert!(!signal.accept_initial_baseline());
    }

    #[test]
    fn change_during_or_after_baseline_cannot_be_acknowledged_away() {
        for before in [true, false] {
            let signal = ChangeSignal::new();
            signal.publish(ChangeKind::InitialValidation);
            if before {
                signal.publish(ChangeKind::Property);
            }
            assert_eq!(signal.accept_initial_baseline(), !before);
            signal.publish(ChangeKind::Default);
            signal.take_batch();
            signal.publish(ChangeKind::InitialValidation);
            assert!(!signal.accept_initial_baseline());
            assert!(signal.has_changed());
        }
    }

    #[test]
    fn multiple_notifications_coalesce_without_clearing_invalidation() {
        let signal = ChangeSignal::new();
        assert!(!signal.is_invalidated());
        signal.publish(ChangeKind::Removed);
        signal.publish(ChangeKind::Removed);
        signal.publish(ChangeKind::State);
        let batch = signal.take_batch().unwrap();
        assert_eq!(
            batch.bits(),
            ChangeKind::Removed as u32 | ChangeKind::State as u32
        );
        assert!(signal.is_invalidated());
        assert!(!signal.has_pending());
        assert!(signal.take_batch().is_none());
    }

    #[test]
    fn notification_arriving_during_control_check_remains_pending() {
        let signal = ChangeSignal::new();
        signal.publish(ChangeKind::Property);
        let checking = signal.take_batch().unwrap();
        assert!(checking.contains(ChangeKind::Property));
        // Control is processing that batch; a callback repeats the same event.
        signal.publish(ChangeKind::Property);
        signal.publish(ChangeKind::Removed);
        assert!(signal.is_invalidated());
        let later = signal.take_batch().unwrap();
        assert!(later.contains(ChangeKind::Property));
        assert!(later.contains(ChangeKind::Removed));
        assert!(!signal.has_pending());
    }

    #[test]
    fn concurrent_producers_cannot_overwrite_other_reasons() {
        use std::sync::Arc;
        let signal = Arc::new(ChangeSignal::new());
        std::thread::scope(|scope| {
            for kind in [
                ChangeKind::Added,
                ChangeKind::Removed,
                ChangeKind::Default,
                ChangeKind::Property,
                ChangeKind::State,
            ] {
                let shared = Arc::clone(&signal);
                scope.spawn(move || {
                    for _ in 0..1000 {
                        shared.publish(kind);
                    }
                });
            }
        });
        assert_eq!(signal.take_batch().unwrap().bits(), 31);
        assert!(signal.is_invalidated());
    }
}
