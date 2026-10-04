//! Native owners must be constructed, used and closed on one ordinary STA thread.
use super::{CapturePacket, PacketError, decode_packet};
use crate::{
    format::AudioFormat,
    notifications::{ChangeSignal, NotificationWatch, WatchError},
    wasapi::{
        Apartment, Flow, MetadataError, MixFormat, NativeDescriptor, TaskMem, client, enumerator,
        read_format, selected,
    },
};
use std::{ptr, sync::Arc, time::Duration};
use windows::{
    Win32::{
        Foundation::{CloseHandle, HANDLE, RPC_S_CALLPENDING},
        Media::Audio::{
            AUDCLNT_BUFFERFLAGS_SILENT, AUDCLNT_SHAREMODE_SHARED,
            AUDCLNT_STREAMFLAGS_EVENTCALLBACK, IAudioCaptureClient, IAudioClient,
            IAudioRenderClient,
        },
        System::{
            Com::{
                COWAIT_DISPATCH_CALLS, COWAIT_DISPATCH_WINDOW_MESSAGES, CoWaitForMultipleHandles,
            },
            Threading::CreateEventW,
        },
    },
    core::{HRESULT, Interface, PCWSTR},
};

#[derive(Debug)]
pub enum StreamError {
    Metadata(MetadataError),
    Com {
        operation: &'static str,
        hresult: i32,
    },
    ExactFormatRequired {
        hresult: i32,
        closest: Option<Box<MixFormat>>,
    },
    Capacity,
    State,
    WrongFlow,
    Packet(PacketError),
    Watch(WatchError),
    DeviceChanged,
}
fn failure(operation: &'static str, error: windows::core::Error) -> StreamError {
    StreamError::Com {
        operation,
        hresult: error.code().0,
    }
}
// Packet paths must not construct windows::core::Error: HRESULT::ok()/map()
// can query thread ErrorInfo and release COM references on a failure.
fn packet_status(operation: &'static str, status: HRESULT) -> Result<(), StreamError> {
    if status.0 < 0 {
        Err(StreamError::Com {
            operation,
            hresult: status.0,
        })
    } else {
        Ok(())
    }
}
fn padding_with(call: impl FnOnce(*mut u32) -> HRESULT) -> Result<u32, StreamError> {
    let mut padding = 0;
    packet_status("GetCurrentPadding", call(&mut padding))?;
    Ok(padding)
}
impl From<MetadataError> for StreamError {
    fn from(error: MetadataError) -> Self {
        Self::Metadata(error)
    }
}

/// The trusted Node must supply this only for a current, explicitly authorized
/// source Start. It is a declaration, not a substitute for a user/hardware lease.
pub enum ExplicitStart {
    UserApproved,
}

struct Event(HANDLE);
impl Drop for Event {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}
enum Service {
    Capture(IAudioCaptureClient),
    Silence(IAudioRenderClient),
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    Prepared,
    Running,
    Retired,
}

// Shared by actual owner methods and injected branch tests. Never drains a
// notification or resets sticky state; no device query, wait or allocation.
fn guard_change(signal: &ChangeSignal, state: &mut State) -> Result<(), StreamError> {
    if signal.has_changed() {
        *state = State::Retired;
        return Err(StreamError::DeviceChanged);
    }
    if *state == State::Retired {
        Err(StreamError::State)
    } else {
        Ok(())
    }
}
fn finish_operation<T>(
    signal: &ChangeSignal,
    state: &mut State,
    result: Result<T, StreamError>,
) -> Result<T, StreamError> {
    guard_change(signal, state)?;
    if result.is_err() {
        *state = State::Retired;
    }
    result
}
fn watched_packet(
    signal: &ChangeSignal,
    state: &mut State,
    output: &mut [f32],
    read: impl FnOnce(&mut [f32]) -> Result<Option<CapturePacket>, StreamError>,
) -> Result<Option<CapturePacket>, StreamError> {
    output.fill(0.0);
    guard_change(signal, state)?;
    let result = read(output);
    let result = finish_operation(signal, state, result);
    if result.is_err() {
        output.fill(0.0);
    }
    result
}

/// !Send/!Sync via Apartment. Fields release services before client, event and
/// COM apartment, including constructor failures. There is no default-device API.
pub struct SharedStream {
    service: Service,
    client: IAudioClient,
    event: Event,
    format: AudioFormat,
    capacity: u32,
    state: State,
    closed: bool,
    close_error: Option<(&'static str, i32)>,
    watch: NotificationWatch,
    signal: Arc<ChangeSignal>,
    _apartment: Apartment,
}
impl SharedStream {
    /// Initializes the selected native-rate stream, without Start or capture.
    /// This is an explicit device operation: never call in library tests or UI.
    /// The negotiated OS buffer must fit the caller's bound (at most 200ms).
    pub fn prepare(
        uid: &str,
        flow: Flow,
        format: AudioFormat,
        maximum_frames: u32,
    ) -> Result<Self, StreamError> {
        if maximum_frames == 0 || maximum_frames > format.sample_rate() / 5 {
            return Err(StreamError::Capacity);
        }
        let apartment = Apartment::enter()?;
        let signal = Arc::new(ChangeSignal::new());
        let watch = NotificationWatch::register(uid.to_owned(), flow, Arc::clone(&signal))
            .map_err(StreamError::Watch)?;
        if !signal.accept_initial_baseline() {
            return Err(StreamError::DeviceChanged);
        }
        let event = Event(
            unsafe { CreateEventW(None, false, false, PCWSTR::null()) }
                .map_err(|e| failure("CreateEventW", e))?,
        );
        let device = selected(&enumerator()?, uid, flow)?;
        let client = client(&device)?;
        let descriptor = NativeDescriptor::new(format);
        // S_FALSE is deliberately not success; no implicit closest/resampler.
        let mut closest = ptr::null_mut();
        let status = unsafe {
            client.IsFormatSupported(
                AUDCLNT_SHAREMODE_SHARED,
                descriptor.as_wave_ptr(),
                Some(&mut closest),
            )
        };
        let closest = TaskMem(closest);
        if status.0 != 0 {
            return Err(StreamError::ExactFormatRequired {
                hresult: status.0,
                closest: if status.0 == 1 && !closest.0.is_null() {
                    Some(Box::new(unsafe { read_format(closest.0) }?))
                } else {
                    None
                },
            });
        }
        if signal.has_changed() {
            return Err(StreamError::DeviceChanged);
        }
        unsafe {
            client.Initialize(
                AUDCLNT_SHAREMODE_SHARED,
                AUDCLNT_STREAMFLAGS_EVENTCALLBACK,
                0,
                0,
                descriptor.as_wave_ptr(),
                None,
            )
        }
        .map_err(|e| failure("Initialize", e))?;
        unsafe { client.SetEventHandle(event.0) }.map_err(|e| failure("SetEventHandle", e))?;
        let capacity =
            unsafe { client.GetBufferSize() }.map_err(|e| failure("GetBufferSize", e))?;
        if capacity == 0 || capacity > maximum_frames {
            return Err(StreamError::Capacity);
        }
        let service = match flow {
            Flow::Capture => Service::Capture(
                unsafe { client.GetService() }.map_err(|e| failure("GetCaptureService", e))?,
            ),
            Flow::Render => Service::Silence(
                unsafe { client.GetService() }.map_err(|e| failure("GetRenderService", e))?,
            ),
        };
        let mut stream = Self {
            service,
            client,
            event,
            format,
            capacity,
            state: State::Prepared,
            closed: false,
            close_error: None,
            watch,
            signal,
            _apartment: apartment,
        };
        guard_change(&stream.signal, &mut stream.state)?;
        if flow == Flow::Render {
            stream.submit_silence()?;
        }
        guard_change(&stream.signal, &mut stream.state)?;
        Ok(stream)
    }
    pub fn format(&self) -> AudioFormat {
        self.format
    }
    pub fn capacity_frames(&self) -> u32 {
        self.capacity
    }
    pub fn is_retired(&self) -> bool {
        self.state == State::Retired || self.signal.has_changed()
    }
    fn retire<T>(&mut self, result: Result<T, StreamError>) -> Result<T, StreamError> {
        finish_operation(&self.signal, &mut self.state, result)
    }
    pub fn start(&mut self, _authorization: ExplicitStart) -> Result<(), StreamError> {
        guard_change(&self.signal, &mut self.state)?;
        if self.state != State::Prepared {
            self.state = State::Retired;
            return Err(StreamError::State);
        }
        let result = unsafe { self.client.Start() }.map_err(|e| failure("Start", e));
        self.retire(result)?;
        self.state = State::Running;
        Ok(())
    }
    /// Ordinary owner-thread scheduler only; no wait is hidden in packet methods.
    pub fn wait_event(&mut self, timeout: Duration) -> Result<bool, StreamError> {
        guard_change(&self.signal, &mut self.state)?;
        if self.state != State::Running || timeout > Duration::from_millis(100) {
            self.state = State::Retired;
            return Err(StreamError::State);
        }
        // Ordinary STA scheduler dispatches notification COM calls while waiting.
        // Packet operations remain non-waiting. Even timeout checks sticky change.
        let result = unsafe {
            CoWaitForMultipleHandles(
                (COWAIT_DISPATCH_CALLS.0 | COWAIT_DISPATCH_WINDOW_MESSAGES.0) as u32,
                timeout.as_millis() as u32,
                &[self.event.0],
            )
        };
        self.retire(match result {
            Ok(_) => Ok(true),
            Err(error) if error.code() == RPC_S_CALLPENDING => Ok(false),
            Err(error) => Err(failure("CoWaitForMultipleHandles", error)),
        })
    }
    /// One packet per invocation, no drain-until-empty loop or allocation.
    pub fn capture_packet(
        &mut self,
        output: &mut [f32],
    ) -> Result<Option<CapturePacket>, StreamError> {
        output.fill(0.0);
        guard_change(&self.signal, &mut self.state)?;
        if self.state != State::Running {
            self.state = State::Retired;
            return Err(StreamError::State);
        }
        watched_packet(
            &self.signal,
            &mut self.state,
            output,
            |output| match &self.service {
                Service::Capture(capture) => {
                    read_packet(capture, self.format, self.capacity, output)
                }
                Service::Silence(_) => Err(StreamError::WrongFlow),
            },
        )
    }
    /// No data parameter: this slice has no original/converted PCM output path.
    pub fn submit_silence(&mut self) -> Result<u32, StreamError> {
        guard_change(&self.signal, &mut self.state)?;
        if self.state == State::Retired {
            return Err(StreamError::State);
        }
        let result = match &self.service {
            Service::Capture(_) => Err(StreamError::WrongFlow),
            Service::Silence(render) => (|| {
                let padding = padding_with(|padding| unsafe {
                    (Interface::vtable(&self.client).GetCurrentPadding)(
                        Interface::as_raw(&self.client),
                        padding,
                    )
                })?;
                let frames = self
                    .capacity
                    .checked_sub(padding)
                    .ok_or(StreamError::Capacity)?;
                if frames == 0 {
                    return Ok(0);
                }
                // SILENT allows ignoring the returned pointer, including null.
                render_silence(render, frames)?;
                Ok(frames)
            })(),
        };
        self.retire(result)
    }
    /// Irreversible stop/reset. A new explicit prepare is needed for any restart.
    pub fn close(&mut self) -> Result<(), StreamError> {
        if self.closed {
            return match self.close_error {
                Some((operation, hresult)) => Err(StreamError::Com { operation, hresult }),
                None => Ok(()),
            };
        }
        self.closed = true;
        self.state = State::Retired;
        let stopped = unsafe { self.client.Stop() }.map_err(|e| failure("Stop", e));
        let reset = unsafe { self.client.Reset() }.map_err(|e| failure("Reset", e));
        let unregistered = self.watch.close().map_err(|error| match error {
            WatchError::UnregisterFailed(hresult) => StreamError::Com {
                operation: "UnregisterEndpointNotificationCallback",
                hresult,
            },
            other => StreamError::Watch(other),
        });
        let result = stopped.and(reset).and(unregistered);
        if let Err(StreamError::Com { operation, hresult }) = &result {
            self.close_error = Some((*operation, *hresult));
        }
        result
    }
}
impl Drop for SharedStream {
    fn drop(&mut self) {
        let _ = self.close();
    }
}

struct CaptureLease<'a> {
    capture: &'a IAudioCaptureClient,
    released: bool,
}
impl CaptureLease<'_> {
    fn release(&mut self, frames: u32) -> Result<(), StreamError> {
        // A failed release is not retried (the OS state is unknown).
        self.released = true;
        packet_status("CaptureReleaseBuffer", unsafe {
            (Interface::vtable(self.capture).ReleaseBuffer)(Interface::as_raw(self.capture), frames)
        })
    }
}
impl Drop for CaptureLease<'_> {
    fn drop(&mut self) {
        if !self.released {
            let _ = self.release(0);
        }
    }
}
fn read_packet(
    capture: &IAudioCaptureClient,
    format: AudioFormat,
    capacity: u32,
    output: &mut [f32],
) -> Result<Option<CapturePacket>, StreamError> {
    output.fill(0.0);
    let (mut pointer, mut frames, mut flags, mut device_position, mut qpc_100ns) =
        (ptr::null_mut(), 0, 0, 0, 0);
    packet_status("CaptureGetBuffer", unsafe {
        (Interface::vtable(capture).GetBuffer)(
            Interface::as_raw(capture),
            &mut pointer,
            &mut frames,
            &mut flags,
            &mut device_position,
            &mut qpc_100ns,
        )
    })?;
    if frames == 0 {
        return Ok(None);
    }
    let mut lease = CaptureLease {
        capture,
        released: false,
    };
    let result = (|| {
        if frames > capacity || frames as usize > output.len() {
            return Err(StreamError::Capacity);
        }
        if flags & !(super::SILENT | super::DISCONTINUITY | super::TIMESTAMP_ERROR) != 0 {
            return Err(StreamError::Packet(PacketError::Flags));
        }
        let length = (frames as usize)
            .checked_mul(format.frame_bytes())
            .ok_or(StreamError::Capacity)?;
        let input = if flags & super::SILENT != 0 {
            None
        } else {
            if pointer.is_null() {
                return Err(StreamError::Packet(PacketError::NullData));
            }
            // WASAPI owns the full negotiated packet; bounds checked before slice.
            Some(unsafe { std::slice::from_raw_parts(pointer, length) })
        };
        decode_packet(format, frames as usize, flags, input, output).map_err(StreamError::Packet)
    })();
    // Do not consume a rejected packet. No subsequent read is permitted after
    // failure; this owner must retire. Release failure overrides validation.
    if let Err(error) = lease.release(if result.is_ok() { frames } else { 0 }) {
        output.fill(0.0);
        return Err(error);
    }
    result?;
    Ok(Some(CapturePacket {
        frames,
        flags,
        device_position,
        qpc_100ns,
    }))
}

fn render_silence(render: &IAudioRenderClient, frames: u32) -> Result<(), StreamError> {
    let mut pointer = ptr::null_mut();
    packet_status("RenderGetBuffer", unsafe {
        (Interface::vtable(render).GetBuffer)(Interface::as_raw(render), frames, &mut pointer)
    })?;
    // No pointer dereference; SILENT covers every requested frame on release.
    packet_status("RenderReleaseBuffer", unsafe {
        (Interface::vtable(render).ReleaseBuffer)(
            Interface::as_raw(render),
            frames,
            AUDCLNT_BUFFERFLAGS_SILENT.0 as u32,
        )
    })
}

#[cfg(test)]
#[path = "raw_tests.rs"]
mod raw_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::format::Encoding;
    use std::sync::{
        Arc,
        atomic::{AtomicU32, Ordering},
    };
    use windows::{
        Win32::Media::Audio::{IAudioCaptureClient_Impl, IAudioRenderClient_Impl},
        core::{HRESULT, implement},
    };

    #[implement(IAudioCaptureClient)]
    struct Capture {
        frames: u32,
        flags: u32,
        data: [u8; 8],
        null: bool,
        release_error: bool,
        releases: Arc<AtomicU32>,
        released_frames: Arc<AtomicU32>,
        acquire_change: Option<Arc<ChangeSignal>>,
        release_change: Option<Arc<ChangeSignal>>,
    }
    impl IAudioCaptureClient_Impl for Capture_Impl {
        fn GetBuffer(
            &self,
            data: *mut *mut u8,
            frames: *mut u32,
            flags: *mut u32,
            device: *mut u64,
            qpc: *mut u64,
        ) -> windows::core::Result<()> {
            if let Some(signal) = &self.acquire_change {
                signal.publish(crate::notifications::ChangeKind::Removed);
            }
            unsafe {
                *data = if self.null {
                    ptr::null_mut()
                } else {
                    self.data.as_ptr().cast_mut()
                };
                *frames = self.frames;
                *flags = self.flags;
                *device = 123;
                *qpc = 456;
            }
            Ok(())
        }
        fn ReleaseBuffer(&self, frames: u32) -> windows::core::Result<()> {
            if let Some(signal) = &self.release_change {
                signal.publish(crate::notifications::ChangeKind::Property);
            }
            self.releases.fetch_add(1, Ordering::Relaxed);
            self.released_frames.store(frames, Ordering::Relaxed);
            if self.release_error {
                Err(HRESULT(0x80004005u32 as i32).into())
            } else {
                Ok(())
            }
        }
        fn GetNextPacketSize(&self) -> windows::core::Result<u32> {
            Ok(self.frames)
        }
    }
    #[test]
    fn capture_pairs_every_nonempty_acquisition_and_retains_metadata() {
        for (frames, flags, null, release_error, capacity, success, consumed) in [
            (2, super::super::SILENT, true, false, 2, true, 2),
            (2, 0, true, false, 2, false, 0),
            (3, 0, false, false, 2, false, 0),
            (2, 8, false, false, 2, false, 0),
            (2, 0, false, true, 2, false, 2),
            (0, 0, true, false, 2, true, 0),
        ] {
            let releases = Arc::new(AtomicU32::new(0));
            let released_frames = Arc::new(AtomicU32::new(99));
            let capture: IAudioCaptureClient = Capture {
                frames,
                flags,
                null,
                release_error,
                data: [0, 64, 0, 64, 0, 0, 0, 0],
                releases: Arc::clone(&releases),
                released_frames: Arc::clone(&released_frames),
                acquire_change: None,
                release_change: None,
            }
            .into();
            let mut output = [0.75; 4];
            let result = read_packet(
                &capture,
                AudioFormat::new(16_000, 1, Encoding::Pcm16).unwrap(),
                capacity,
                &mut output,
            );
            assert_eq!(result.is_ok(), success);
            assert_eq!(output, [0.0; 4]);
            assert_eq!(releases.load(Ordering::Relaxed), u32::from(frames != 0));
            if frames != 0 {
                assert_eq!(released_frames.load(Ordering::Relaxed), consumed);
            }
            if let Ok(Some(packet)) = result {
                assert_eq!(packet.device_position, 123);
                assert_eq!(packet.qpc_100ns, 456);
            }
        }
    }
    #[test]
    fn changes_during_acquire_or_release_erase_entire_packet_and_forbid_retry() {
        use crate::notifications::ChangeKind;
        for during_acquire in [true, false] {
            let signal = Arc::new(ChangeSignal::new());
            signal.publish(ChangeKind::InitialValidation);
            assert!(signal.accept_initial_baseline());
            let releases = Arc::new(AtomicU32::new(0));
            let capture: IAudioCaptureClient = Capture {
                frames: 2,
                flags: 0,
                data: [0, 64, 0, 64, 0, 0, 0, 0],
                null: false,
                release_error: false,
                releases: Arc::clone(&releases),
                released_frames: Arc::new(AtomicU32::new(0)),
                acquire_change: during_acquire.then(|| Arc::clone(&signal)),
                release_change: (!during_acquire).then(|| Arc::clone(&signal)),
            }
            .into();
            let mut state = State::Running;
            let mut output = [0.75; 4];
            let result = watched_packet(&signal, &mut state, &mut output, |out| {
                read_packet(
                    &capture,
                    AudioFormat::new(16_000, 1, Encoding::Pcm16).unwrap(),
                    2,
                    out,
                )
            });
            assert!(matches!(result, Err(StreamError::DeviceChanged)));
            assert!(state == State::Retired);
            assert_eq!(output, [0.0; 4]);
            assert_eq!(releases.load(Ordering::Relaxed), 1);
            signal.take_batch();
            output.fill(0.75);
            assert!(
                watched_packet(&signal, &mut state, &mut output, |_| {
                    panic!("retired stream must not reacquire")
                })
                .is_err()
            );
            assert_eq!(output, [0.0; 4]);
        }
    }
    #[test]
    fn timeout_and_successful_operations_cannot_restore_changed_owner() {
        use crate::notifications::ChangeKind;
        for kind in [
            ChangeKind::Default,
            ChangeKind::State,
            ChangeKind::Removed,
            ChangeKind::Added,
            ChangeKind::Property,
            ChangeKind::Closed,
        ] {
            let signal = ChangeSignal::new();
            signal.publish(ChangeKind::InitialValidation);
            assert!(signal.accept_initial_baseline());
            let mut state = State::Running;
            signal.publish(kind);
            // Identical post-wait branch used for a real silent wait timeout.
            assert!(matches!(
                finish_operation(&signal, &mut state, Ok(false)),
                Err(StreamError::DeviceChanged)
            ));
            assert!(state == State::Retired);
            signal.take_batch();
            assert!(finish_operation(&signal, &mut state, Ok(())).is_err());
            assert!(guard_change(&signal, &mut state).is_err());
            assert!(signal.is_invalidated());
        }
        let fresh = ChangeSignal::new();
        fresh.publish(ChangeKind::InitialValidation);
        assert!(fresh.accept_initial_baseline());
        assert!(!finish_operation(&fresh, &mut State::Running, Ok(false)).unwrap());
    }
    #[implement(IAudioRenderClient)]
    struct Render {
        acquired: Arc<AtomicU32>,
        released: Arc<AtomicU32>,
        flags: Arc<AtomicU32>,
    }
    impl IAudioRenderClient_Impl for Render_Impl {
        fn GetBuffer(&self, frames: u32) -> windows::core::Result<*mut u8> {
            self.acquired.store(frames, Ordering::Relaxed);
            Ok(ptr::null_mut())
        }
        fn ReleaseBuffer(&self, frames: u32, flags: u32) -> windows::core::Result<()> {
            self.released.store(frames, Ordering::Relaxed);
            self.flags.store(flags, Ordering::Relaxed);
            Ok(())
        }
    }
    #[test]
    fn render_never_dereferences_data_and_releases_all_frames_silent() {
        let acquired = Arc::new(AtomicU32::new(0));
        let released = Arc::new(AtomicU32::new(0));
        let flags = Arc::new(AtomicU32::new(0));
        let render: IAudioRenderClient = Render {
            acquired: Arc::clone(&acquired),
            released: Arc::clone(&released),
            flags: Arc::clone(&flags),
        }
        .into();
        render_silence(&render, 480).unwrap();
        assert_eq!(acquired.load(Ordering::Relaxed), 480);
        assert_eq!(released.load(Ordering::Relaxed), 480);
        assert_eq!(flags.load(Ordering::Relaxed), 2);
    }
}
