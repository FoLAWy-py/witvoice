//! Native callback lifetime and control-thread UID validation. Never starts audio.

use super::{ChangeBatch, ChangeKind, ChangeSignal};
use crate::wasapi::{Apartment, Endpoint, Flow, MetadataError, enumerator, inspect_endpoint};
use std::{sync::Arc, time::Duration};
use windows::{
    Win32::{
        Foundation::{CloseHandle, HANDLE, PROPERTYKEY, RPC_S_CALLPENDING},
        Media::Audio::{
            DEVICE_STATE, EDataFlow, ERole, IMMDeviceEnumerator, IMMNotificationClient,
            IMMNotificationClient_Impl,
        },
        System::{
            Com::{
                COWAIT_DISPATCH_CALLS, COWAIT_DISPATCH_WINDOW_MESSAGES, CoWaitForMultipleHandles,
            },
            Threading::CreateEventW,
        },
    },
    core::{PCWSTR, implement},
};

#[implement(IMMNotificationClient)]
struct NotificationClient {
    signal: Arc<ChangeSignal>,
}

#[allow(non_snake_case)]
impl IMMNotificationClient_Impl for NotificationClient_Impl {
    fn OnDeviceStateChanged(
        &self,
        _uid: &PCWSTR,
        _state: DEVICE_STATE,
    ) -> windows::core::Result<()> {
        self.signal.publish(ChangeKind::State);
        Ok(())
    }
    fn OnDeviceAdded(&self, _uid: &PCWSTR) -> windows::core::Result<()> {
        self.signal.publish(ChangeKind::Added);
        Ok(())
    }
    fn OnDeviceRemoved(&self, _uid: &PCWSTR) -> windows::core::Result<()> {
        self.signal.publish(ChangeKind::Removed);
        Ok(())
    }
    fn OnDefaultDeviceChanged(
        &self,
        _flow: EDataFlow,
        _role: ERole,
        _uid: &PCWSTR,
    ) -> windows::core::Result<()> {
        self.signal.publish(ChangeKind::Default);
        Ok(())
    }
    fn OnPropertyValueChanged(
        &self,
        _uid: &PCWSTR,
        _key: &PROPERTYKEY,
    ) -> windows::core::Result<()> {
        self.signal.publish(ChangeKind::Property);
        Ok(())
    }
}

#[derive(Debug)]
pub enum WatchError {
    RegistrationAlreadyClaimed,
    ControlState,
    Metadata(MetadataError),
    UnregisterFailed(i32),
}

/// A check result is metadata only, never Ready, Start or an epoch acknowledgement.
#[derive(Debug)]
pub struct SelectionRevalidation {
    pub changes: ChangeBatch,
    pub endpoint: Result<Endpoint, MetadataError>,
    pub more_changes_pending: bool,
}

struct Registration {
    enumerator: IMMDeviceEnumerator,
    callback: IMMNotificationClient,
}
struct DispatchEvent(HANDLE);
impl Drop for DispatchEvent {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

/// Thread-bound owner. Construct/close/drop only on a normal STA control thread.
/// Keep this owner alive for the full registration: Windows does not AddRef it.
pub struct NotificationWatch {
    registration: Option<Registration>,
    uid: String,
    flow: Flow,
    signal: Arc<ChangeSignal>,
    dispatch_event: DispatchEvent,
    apartment: Option<Apartment>,
    closed: bool,
}

fn unregister_slot<T>(
    slot: &mut Option<T>,
    signal: &ChangeSignal,
    unregister: impl FnOnce(&T) -> Result<(), i32>,
) -> Result<(), i32> {
    if let Some(registration) = slot.as_ref() {
        match unregister(registration) {
            Ok(()) => {
                *slot = None;
            }
            Err(code) => {
                signal.record_unregister_error(code);
                return Err(code);
            }
        }
    }
    Ok(())
}

impl NotificationWatch {
    pub fn register(
        uid: String,
        flow: Flow,
        signal: Arc<ChangeSignal>,
    ) -> Result<Self, WatchError> {
        if uid.is_empty() || uid.contains('\0') {
            return Err(WatchError::Metadata(MetadataError::InvalidUid));
        }
        if !signal.claim_registration() {
            return Err(WatchError::RegistrationAlreadyClaimed);
        }
        // Keep failed or newly registered owners invalidated until an explicit
        // external Prepare workflow; this API has no permission-restoring step.
        signal.publish(ChangeKind::InitialValidation);
        let apartment = Apartment::enter().map_err(WatchError::Metadata)?;
        let dispatch_event = DispatchEvent(
            unsafe { CreateEventW(None, false, false, PCWSTR::null()) }.map_err(|e| {
                WatchError::Metadata(MetadataError::Com {
                    operation: "CreateNotificationDispatchEvent",
                    hresult: e.code().0,
                })
            })?,
        );
        inspect_endpoint(&uid, flow).map_err(WatchError::Metadata)?;
        let enumerator = enumerator().map_err(WatchError::Metadata)?;
        let callback: IMMNotificationClient = NotificationClient {
            signal: Arc::clone(&signal),
        }
        .into();
        unsafe { enumerator.RegisterEndpointNotificationCallback(&callback) }.map_err(|e| {
            WatchError::Metadata(MetadataError::Com {
                operation: "RegisterEndpointNotificationCallback",
                hresult: e.code().0,
            })
        })?;
        Ok(Self {
            registration: Some(Registration {
                enumerator,
                callback,
            }),
            uid,
            flow,
            signal,
            dispatch_event,
            apartment: Some(apartment),
            closed: false,
        })
    }

    /// Normal STA control scheduler only. No audio Initialize/Start; dispatch
    /// service callbacks for at most 100ms. Call poll_revalidation separately.
    pub fn dispatch(&mut self, timeout: Duration) -> Result<(), WatchError> {
        if self.closed || timeout > Duration::from_millis(100) {
            return Err(WatchError::ControlState);
        }
        match unsafe {
            CoWaitForMultipleHandles(
                (COWAIT_DISPATCH_CALLS.0 | COWAIT_DISPATCH_WINDOW_MESSAGES.0) as u32,
                timeout.as_millis() as u32,
                &[self.dispatch_event.0],
            )
        } {
            Ok(_) => Ok(()),
            Err(error) if error.code() == RPC_S_CALLPENDING => Ok(()),
            Err(error) => Err(WatchError::Metadata(MetadataError::Com {
                operation: "CoWaitNotificationDispatch",
                hresult: error.code().0,
            })),
        }
    }

    /// Re-read the saved exact UID/expected flow, never the changed default.
    /// Events during inspect stay in the next batch; sticky invalidation stays set.
    pub fn poll_revalidation(&mut self) -> Option<SelectionRevalidation> {
        if self.closed {
            return None;
        }
        self.registration.as_ref()?;
        let changes = self.signal.take_batch()?;
        let endpoint = inspect_endpoint(&self.uid, self.flow);
        Some(SelectionRevalidation {
            changes,
            endpoint,
            more_changes_pending: self.signal.has_pending(),
        })
    }

    /// Normal-thread explicit teardown. Failure retains callback/enumerator and
    /// its apartment; the same signal can never register again or clear invalidation.
    pub fn close(&mut self) -> Result<(), WatchError> {
        self.closed = true;
        self.signal.publish(ChangeKind::Closed);
        unregister_slot(&mut self.registration, &self.signal, |r| {
            unsafe {
                r.enumerator
                    .UnregisterEndpointNotificationCallback(&r.callback)
            }
            .map_err(|e| e.code().0)
        })
        .map_err(WatchError::UnregisterFailed)
    }
}

impl Drop for NotificationWatch {
    fn drop(&mut self) {
        if self.close().is_err() {
            // Unknown registration state: releasing the final client would leave
            // Windows with a dangling callback. Quarantine one fixed-size pair
            // per claimed owner, plus its COM initialization, for process life.
            // No retry loop, allocation, log, fallback or recovery permission.
            if let Some(registration) = self.registration.take() {
                std::mem::forget(registration);
            }
            if let Some(apartment) = self.apartment.take() {
                std::mem::forget(apartment);
            }
        }
        // Success: registration is gone before the apartment guard drops.
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::{
        alloc::{GlobalAlloc, Layout, System},
        cell::Cell,
    };
    use windows::Win32::Media::Audio::{eCapture, eConsole};

    thread_local! {
        static COUNTING: Cell<bool> = const { Cell::new(false) };
        static ALLOCATIONS: Cell<usize> = const { Cell::new(0) };
    }
    struct CountingAllocator;
    unsafe impl GlobalAlloc for CountingAllocator {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            if COUNTING.try_with(Cell::get).unwrap_or(false) {
                let _ = ALLOCATIONS.try_with(|count| count.set(count.get() + 1));
            }
            unsafe { System.alloc(layout) }
        }
        unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
            unsafe { System.dealloc(pointer, layout) }
        }
        unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
            if COUNTING.try_with(Cell::get).unwrap_or(false) {
                let _ = ALLOCATIONS.try_with(|count| count.set(count.get() + 1));
            }
            unsafe { System.realloc(pointer, layout, size) }
        }
    }
    #[global_allocator]
    static ALLOCATOR: CountingAllocator = CountingAllocator;

    pub(crate) fn count_allocations<T>(operation: impl FnOnce() -> T) -> (T, usize) {
        ALLOCATIONS.with(|count| count.set(0));
        COUNTING.with(|enabled| enabled.set(true));
        let result = operation();
        COUNTING.with(|enabled| enabled.set(false));
        (result, ALLOCATIONS.with(Cell::get))
    }

    #[test]
    fn all_native_callback_methods_publish_without_uid_reads_or_allocations() {
        let signal = Arc::new(ChangeSignal::new());
        let callback: IMMNotificationClient = NotificationClient {
            signal: Arc::clone(&signal),
        }
        .into();
        // Local COM implementation only; not registered with any Windows service.
        ALLOCATIONS.with(|count| count.set(0));
        COUNTING.with(|enabled| enabled.set(true));
        unsafe {
            for _ in 0..1000 {
                callback
                    .OnDeviceStateChanged(PCWSTR::null(), DEVICE_STATE(0))
                    .unwrap();
                callback.OnDeviceAdded(PCWSTR::null()).unwrap();
                callback.OnDeviceRemoved(PCWSTR::null()).unwrap();
                callback
                    .OnDefaultDeviceChanged(eCapture, eConsole, PCWSTR::null())
                    .unwrap();
                callback
                    .OnPropertyValueChanged(PCWSTR::null(), PROPERTYKEY::default())
                    .unwrap();
            }
        }
        COUNTING.with(|enabled| enabled.set(false));
        assert_eq!(ALLOCATIONS.with(Cell::get), 0);
        assert_eq!(signal.take_batch().unwrap().bits(), 31);
        assert!(signal.is_invalidated());
    }

    #[test]
    fn registration_owner_cannot_be_reused_even_after_failure() {
        let signal = ChangeSignal::new();
        assert!(signal.claim_registration());
        signal.record_unregister_error(-1);
        assert!(!signal.claim_registration());
        assert_eq!(signal.unregister_hresult(), -1);
        assert!(signal.is_invalidated());
    }

    #[test]
    fn teardown_failure_keeps_reference_then_success_releases_once() {
        struct Reference(Arc<AtomicUsize>);
        impl Drop for Reference {
            fn drop(&mut self) {
                self.0.fetch_add(1, Ordering::Relaxed);
            }
        }
        let dropped = Arc::new(AtomicUsize::new(0));
        let mut slot = Some(Reference(Arc::clone(&dropped)));
        let signal = ChangeSignal::new();
        assert_eq!(unregister_slot(&mut slot, &signal, |_| Err(-1)), Err(-1));
        assert!(slot.is_some());
        assert_eq!(dropped.load(Ordering::Relaxed), 0);
        assert_eq!(signal.unregister_hresult(), -1);
        unregister_slot(&mut slot, &signal, |_| Ok(())).unwrap();
        assert!(slot.is_none());
        assert_eq!(dropped.load(Ordering::Relaxed), 1);
        unregister_slot(&mut slot, &signal, |_| panic!("already unregistered")).unwrap();
        assert_eq!(dropped.load(Ordering::Relaxed), 1);
        assert!(signal.is_invalidated());
    }
}
