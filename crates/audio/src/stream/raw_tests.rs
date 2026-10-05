//! Stack-owned ABI mocks return literal HRESULTs, never Error/ErrorInfo.
//! Allocation counts cover Rust packet/error handling, not the OS service.
use super::*;
use crate::{
    format::Encoding,
    notifications::{ChangeKind, count_allocations},
};
use std::{cell::Cell, ffi::c_void, mem::ManuallyDrop};
use windows::{
    Win32::Media::Audio::{IAudioCaptureClient_Vtbl, IAudioRenderClient_Vtbl},
    core::{GUID, IUnknown_Vtbl},
};

const FAILED: HRESULT = HRESULT(0x80004005_u32 as i32);
#[test]
fn prepare_negotiated_capacity_keeps_zero_and_over_bound_reasons_exact_without_start() {
    for actual in [0, 960, 961] {
        let (result, allocations) = count_allocations(|| negotiated_capacity(actual, 960));
        assert_eq!(allocations, 0);
        match (actual, result) {
            (960, Ok(value)) => assert_eq!(value, 960),
            (
                0 | 961,
                Err(StreamError::NegotiatedCapacity {
                    actual_frames,
                    maximum_frames,
                }),
            ) => {
                assert_eq!(actual_frames, actual);
                assert_eq!(maximum_frames, 960);
            }
            _ => panic!("must retain exact rejected size without widening"),
        }
    }
    // The injected helper has no IAudioClient or Start operation. The production
    // Prepare branch invokes it before service acquisition and has no Start call.
}
#[test]
fn negotiated_30ms_bound_and_single_commit_limits_are_checked_without_allocation() {
    for actual in [0, 960, 961, 1056, 1440, 1441] {
        let (result, allocations) = count_allocations(|| negotiated_capacity(actual, 1440));
        assert_eq!(allocations, 0);
        if actual == 0 || actual > 1440 {
            assert!(matches!(result, Err(StreamError::NegotiatedCapacity {
                actual_frames, maximum_frames: 1440
            }) if actual_frames == actual));
        } else {
            assert_eq!(result.unwrap(), actual);
        }
    }
    for (capacity, padding, scratch, expected) in [
        (1440, 1440, 960, Some(0)),
        (960, 0, 960, Some(960)),
        (961, 0, 960, Some(960)),
        (1056, 0, 960, Some(960)),
        (1440, 0, 960, Some(960)),
        (1056, 96, 960, Some(960)),
        (1056, 0, 64, Some(64)),
        (1056, 0, 1440, Some(960)),
        (0, 0, 960, None),
        (1056, 1057, 960, None),
        (1056, 0, 0, None),
    ] {
        let (result, allocations) =
            count_allocations(|| render_packet_frames(capacity, padding, scratch));
        assert_eq!(allocations, 0);
        match expected {
            Some(frames) => assert_eq!(result.unwrap(), frames),
            None => assert!(matches!(result, Err(StreamError::Capacity))),
        }
    }
}
unsafe extern "system" fn query(_: *mut c_void, _: *const GUID, out: *mut *mut c_void) -> HRESULT {
    if !out.is_null() {
        unsafe {
            *out = ptr::null_mut();
        }
    }
    HRESULT(0x80004002_u32 as i32)
}
unsafe extern "system" fn reference(_: *mut c_void) -> u32 {
    1
}
const UNKNOWN: IUnknown_Vtbl = IUnknown_Vtbl {
    QueryInterface: query,
    AddRef: reference,
    Release: reference,
};

#[repr(C)]
struct RawCapture {
    vtable: &'static IAudioCaptureClient_Vtbl,
    failure_step: u32,
    gets: Cell<u32>,
    releases: Cell<u32>,
    consumed: Cell<u32>,
    frames: u32,
    data: [u8; 5760],
}
unsafe extern "system" fn capture_get(
    this: *mut c_void,
    data: *mut *mut u8,
    frames: *mut u32,
    flags: *mut u32,
    device: *mut u64,
    qpc: *mut u64,
) -> HRESULT {
    let mock = unsafe { &*this.cast::<RawCapture>() };
    mock.gets.set(mock.gets.get() + 1);
    if mock.failure_step == 1 {
        return FAILED;
    }
    unsafe {
        *data = mock.data.as_ptr().cast_mut();
        *frames = mock.frames;
        *flags = 0;
        *device = 123;
        *qpc = 456;
    }
    HRESULT(0)
}
unsafe extern "system" fn capture_release(this: *mut c_void, frames: u32) -> HRESULT {
    let mock = unsafe { &*this.cast::<RawCapture>() };
    mock.releases.set(mock.releases.get() + 1);
    mock.consumed.set(frames);
    if mock.failure_step == 2 {
        FAILED
    } else {
        HRESULT(0)
    }
}
unsafe extern "system" fn next_packet(_: *mut c_void, out: *mut u32) -> HRESULT {
    unsafe {
        *out = 2;
    }
    HRESULT(0)
}
static CAPTURE: IAudioCaptureClient_Vtbl = IAudioCaptureClient_Vtbl {
    base__: UNKNOWN,
    GetBuffer: capture_get,
    ReleaseBuffer: capture_release,
    GetNextPacketSize: next_packet,
};

#[test]
fn raw_capture_get_and_release_failures_allocate_zero_and_pair_correctly() {
    for failure_step in [0, 1, 2] {
        let mut mock = RawCapture {
            vtable: &CAPTURE,
            failure_step,
            gets: Cell::new(0),
            releases: Cell::new(0),
            consumed: Cell::new(0),
            frames: 2,
            data: [0; 5760],
        };
        mock.data[..4].copy_from_slice(&[0, 64, 0, 64]);
        // Borrowed stack object stays alive; no clone/QueryInterface or destructor.
        let capture = ManuallyDrop::new(unsafe {
            IAudioCaptureClient::from_raw(ptr::from_mut(&mut mock).cast())
        });
        let format = AudioFormat::new(16_000, 1, Encoding::Pcm16).unwrap();
        let signal = ChangeSignal::new();
        signal.publish(ChangeKind::InitialValidation);
        assert!(signal.accept_initial_baseline());
        let mut state = State::Running;
        let mut output = [0.75; 4];
        let (result, allocations) = count_allocations(|| {
            watched_packet(&signal, &mut state, &mut output, |output| {
                read_packet(&capture, format, 2, output)
            })
        });
        assert_eq!(allocations, 0);
        assert_eq!(mock.gets.get(), 1);
        assert_eq!(mock.releases.get(), u32::from(failure_step != 1));
        if failure_step == 0 {
            assert!(result.is_ok());
            assert_eq!(output, [0.5, 0.5, 0.0, 0.0]);
        } else {
            assert!(state == State::Retired);
            assert_eq!(output, [0.0; 4]);
            let expected = if failure_step == 1 {
                "CaptureGetBuffer"
            } else {
                "CaptureReleaseBuffer"
            };
            assert!(matches!(result, Err(StreamError::Com {operation,hresult})
                if operation == expected && hresult == FAILED.0));
        }
        if failure_step == 2 {
            assert_eq!(mock.consumed.get(), 2);
        }
    }
}

#[test]
fn bounded_capture_stereo_packets_use_1440_preallocated_mono_and_reject_insufficient_space() {
    for (frames, output_len) in [
        (0, 1440),
        (960, 1440),
        (961, 1440),
        (1056, 1440),
        (1440, 1440),
        (1441, 1440),
        (960, 959),
        (1, 0),
    ] {
        let mut mock = RawCapture {
            vtable: &CAPTURE,
            failure_step: 0,
            gets: Cell::new(0),
            releases: Cell::new(0),
            consumed: Cell::new(0),
            frames,
            data: [0; 5760],
        };
        for sample in mock.data.as_chunks_mut::<2>().0 {
            sample.copy_from_slice(&[0, 64]);
        }
        let capture = ManuallyDrop::new(unsafe {
            IAudioCaptureClient::from_raw(ptr::from_mut(&mut mock).cast())
        });
        let format = AudioFormat::new(48_000, 2, Encoding::Pcm16).unwrap();
        let signal = ChangeSignal::new();
        signal.publish(ChangeKind::InitialValidation);
        assert!(signal.accept_initial_baseline());
        let mut state = State::Running;
        let mut output = [0.75; 1440];
        let (result, allocations) = count_allocations(|| {
            watched_packet(&signal, &mut state, &mut output[..output_len], |out| {
                read_packet(&capture, format, 1440, out)
            })
        });
        assert_eq!(allocations, 0);
        assert_eq!(mock.gets.get(), 1);
        assert_eq!(mock.releases.get(), u32::from(frames != 0));
        if frames == 0 {
            assert!(result.unwrap().is_none());
            assert_eq!(mock.consumed.get(), 0);
            assert!(output[..output_len].iter().all(|value| *value == 0.0));
        } else if frames > 1440 || frames as usize > output_len {
            assert!(matches!(result, Err(StreamError::Capacity)));
            assert!(state == State::Retired);
            assert_eq!(mock.consumed.get(), 0);
            assert!(output[..output_len].iter().all(|value| *value == 0.0));
        } else {
            assert_eq!(result.unwrap().unwrap().frames, frames);
            assert_eq!(mock.consumed.get(), frames);
            assert!(output[..frames as usize].iter().all(|value| *value == 0.5));
            assert!(
                output[frames as usize..output_len]
                    .iter()
                    .all(|value| *value == 0.0)
            );
        }
    }
}

#[repr(C)]
struct BoundedRender {
    vtable: &'static IAudioRenderClient_Vtbl,
    gate: *const OutputGate,
    signal: *const ChangeSignal,
    step: u32,
    gets: Cell<u32>,
    releases: Cell<u32>,
    frames: Cell<u32>,
    flags: Cell<u32>,
    ack_during_release: Cell<bool>,
    data: [u8; 7680],
}
unsafe extern "system" fn bounded_get(
    this: *mut c_void,
    frames: u32,
    out: *mut *mut u8,
) -> HRESULT {
    let mock = unsafe { &mut *this.cast::<BoundedRender>() };
    mock.gets.set(mock.gets.get() + 1);
    mock.frames.set(frames);
    unsafe { *out = mock.data.as_mut_ptr() };
    if mock.step == 1 {
        unsafe { &*mock.signal }.publish(ChangeKind::Removed);
    } else if mock.step == 2 {
        unsafe { &*mock.gate }.invalidate();
    }
    HRESULT(0)
}
unsafe extern "system" fn bounded_release(this: *mut c_void, frames: u32, flags: u32) -> HRESULT {
    let mock = unsafe { &*this.cast::<BoundedRender>() };
    assert_eq!(frames, mock.frames.get());
    mock.releases.set(mock.releases.get() + 1);
    mock.flags.set(flags);
    mock.ack_during_release
        .set(unsafe { &*mock.gate }.ack_ready());
    HRESULT(0)
}
static BOUNDED_RENDER: IAudioRenderClient_Vtbl = IAudioRenderClient_Vtbl {
    base__: UNKNOWN,
    GetBuffer: bounded_get,
    ReleaseBuffer: bounded_release,
};
#[test]
fn production_bounded_render_branch_commits_once_and_notification_or_mute_never_passes_pcm() {
    use crate::realtime::{Binding, ProcessedBlock, QueueConfig, Ring, processed_endpoints};
    for (capacity, padding, scratch_len, step, expected) in [
        (1440, 1440, 960, 0, Some(0)),
        (960, 0, 960, 0, Some(960)),
        (961, 0, 960, 0, Some(960)),
        (1056, 0, 960, 0, Some(960)),
        (1440, 0, 960, 0, Some(960)),
        (1056, 0, 64, 0, Some(64)),
        (1056, 1057, 960, 0, None),
        (1056, 0, 0, 0, None),
        (0, 0, 960, 0, None),
        (1056, 0, 960, 1, Some(960)),
        (1056, 0, 960, 2, Some(960)),
        (1056, 0, 960, 3, None),
        (1056, 0, 960, 4, Some(960)),
    ] {
        let mut gate = OutputGate::new(Binding::new(5, 9).unwrap()).unwrap();
        gate.arm().unwrap();
        let signal = ChangeSignal::new();
        signal.publish(ChangeKind::InitialValidation);
        assert!(signal.accept_initial_baseline());
        let mut mock = BoundedRender {
            vtable: &BOUNDED_RENDER,
            gate: &gate,
            signal: &signal,
            step,
            gets: Cell::new(0),
            releases: Cell::new(0),
            frames: Cell::new(0),
            flags: Cell::new(99),
            ack_during_release: Cell::new(true),
            data: [99; 7680],
        };
        let render = ManuallyDrop::new(unsafe {
            IAudioRenderClient::from_raw(ptr::from_mut(&mut mock).cast())
        });
        let format = AudioFormat::new(48_000, 2, Encoding::Float32).unwrap();
        let mut ring = Ring::new(8).unwrap();
        let (mut producer, mut playout) =
            processed_endpoints(&mut ring, &gate, QueueConfig::output()).unwrap();
        for block in 0..4 {
            producer
                .push(
                    ProcessedBlock::from_model_result(
                        gate.binding(),
                        block * 480,
                        0,
                        60_000_000,
                        &[0.25; 480],
                    )
                    .unwrap(),
                    0,
                )
                .unwrap();
        }
        if step == 3 {
            signal.publish(ChangeKind::Removed);
        }
        if step == 4 {
            gate.invalidate();
        }
        let mut scratch = [0.75; 960];
        let (result, allocations) = count_allocations(|| {
            render_processed_packet(
                &render,
                format,
                RenderWindow { capacity, padding },
                &mut playout,
                &mut scratch[..scratch_len],
                || 1,
                &signal,
            )
        });
        assert_eq!(allocations, 0);
        let frames = expected.unwrap_or(0);
        assert_eq!(mock.gets.get(), u32::from(frames != 0));
        assert_eq!(mock.releases.get(), u32::from(frames != 0));
        match expected {
            Some(frames) => assert_eq!(result.unwrap(), frames),
            None if step == 3 => assert!(matches!(result, Err(StreamError::DeviceChanged))),
            None => assert!(matches!(result, Err(StreamError::Capacity))),
        }
        if frames != 0 {
            assert_eq!(mock.frames.get(), frames);
            assert!(frames <= 960);
            assert_eq!(mock.flags.get(), if step == 0 { 0 } else { 2 });
            assert_eq!(mock.ack_during_release.get(), step == 4);
            if step == 0 {
                assert_eq!(playout.counters().sink_pcm_frames, u64::from(frames));
                assert_eq!(playout.counters().underflow_frames, 0);
                for sample in mock.data[..frames as usize * 8].as_chunks::<4>().0 {
                    assert_eq!(*sample, 0.25_f32.to_le_bytes());
                }
            } else {
                assert_eq!(playout.counters().sink_pcm_frames, 0);
                assert!(scratch[..scratch_len].iter().all(|value| *value == 0.0));
                assert!(gate.begin_commit().is_none());
                assert!(gate.ack_ready());
            }
        }
    }
}

#[repr(C)]
struct RawRender {
    vtable: &'static IAudioRenderClient_Vtbl,
    failure_step: u32,
    gets: Cell<u32>,
    releases: Cell<u32>,
    flags: Cell<u32>,
    frames: Cell<u32>,
}
unsafe extern "system" fn render_get(this: *mut c_void, _: u32, data: *mut *mut u8) -> HRESULT {
    let mock = unsafe { &*this.cast::<RawRender>() };
    mock.gets.set(mock.gets.get() + 1);
    if mock.failure_step == 1 {
        return FAILED;
    }
    unsafe {
        *data = ptr::null_mut();
    }
    HRESULT(0)
}
unsafe extern "system" fn render_release(this: *mut c_void, frames: u32, flags: u32) -> HRESULT {
    let mock = unsafe { &*this.cast::<RawRender>() };
    mock.releases.set(mock.releases.get() + 1);
    mock.flags.set(flags);
    mock.frames.set(frames);
    if mock.failure_step == 2 {
        FAILED
    } else {
        HRESULT(0)
    }
}
static RENDER: IAudioRenderClient_Vtbl = IAudioRenderClient_Vtbl {
    base__: UNKNOWN,
    GetBuffer: render_get,
    ReleaseBuffer: render_release,
};

#[repr(C)]
struct GovernedRender {
    vtable: &'static IAudioRenderClient_Vtbl,
    gate: *const crate::realtime::OutputGate,
    step: u32,
    gets: Cell<u32>,
    releases: Cell<u32>,
    flags: Cell<u32>,
    ack_during_release: Cell<bool>,
    data: [u8; 8],
}
unsafe extern "system" fn governed_get(this: *mut c_void, _: u32, out: *mut *mut u8) -> HRESULT {
    let mock = unsafe { &mut *this.cast::<GovernedRender>() };
    mock.gets.set(mock.gets.get() + 1);
    if mock.step == 1 {
        return FAILED;
    }
    unsafe {
        *out = if mock.step == 3 {
            ptr::null_mut()
        } else {
            mock.data.as_mut_ptr()
        };
    }
    if mock.step == 4 {
        unsafe { &*mock.gate }.invalidate();
    }
    HRESULT(0)
}
unsafe extern "system" fn governed_release(this: *mut c_void, _: u32, flags: u32) -> HRESULT {
    let mock = unsafe { &*this.cast::<GovernedRender>() };
    mock.releases.set(mock.releases.get() + 1);
    mock.flags.set(flags);
    if mock.step == 5 {
        unsafe { &*mock.gate }.invalidate();
    }
    mock.ack_during_release
        .set(unsafe { &*mock.gate }.ack_ready());
    if mock.step == 2 { FAILED } else { HRESULT(0) }
}
static GOVERNED: IAudioRenderClient_Vtbl = IAudioRenderClient_Vtbl {
    base__: UNKNOWN,
    GetBuffer: governed_get,
    ReleaseBuffer: governed_release,
};
#[test]
fn governed_native_commit_pairs_errors_and_mute_ticket_covers_release_without_allocating() {
    use crate::realtime::{Binding, OutputGate};
    for step in 0..=7 {
        let mut gate = OutputGate::new(Binding::new(5, 9).unwrap()).unwrap();
        gate.arm().unwrap();
        let mut mock = GovernedRender {
            vtable: &GOVERNED,
            gate: &gate,
            step,
            gets: Cell::new(0),
            releases: Cell::new(0),
            flags: Cell::new(99),
            ack_during_release: Cell::new(true),
            data: [99; 8],
        };
        let render = ManuallyDrop::new(unsafe {
            IAudioRenderClient::from_raw(ptr::from_mut(&mut mock).cast())
        });
        let calls = Cell::new(0);
        let format = AudioFormat::new(48_000, 2, Encoding::Pcm16).unwrap();
        let samples = if step == 7 { [f32::NAN; 2] } else { [0.5; 2] };
        let (result, allocations) = count_allocations(|| {
            let ticket = gate.begin_commit().unwrap();
            let result = render_governed(&render, format, 2, &samples, || {
                calls.set(calls.get() + 1);
                if step == 6 && calls.get() == 2 {
                    gate.invalidate();
                }
                ticket.is_live()
            });
            if result.is_err() {
                gate.fail();
            }
            if !gate.is_live() {
                assert!(!gate.ack_ready());
            }
            drop(ticket);
            result
        });
        assert_eq!(allocations, 0);
        assert_eq!(mock.gets.get(), 1);
        assert_eq!(mock.releases.get(), u32::from(step != 1));
        if step != 1 {
            assert!(!mock.ack_during_release.get());
        }
        if [1, 2, 3, 7].contains(&step) {
            assert!(result.is_err());
            assert!(gate.is_faulted());
        }
        if [3, 4, 6, 7].contains(&step) {
            assert_eq!(mock.flags.get(), 2);
        }
        if [0, 2, 5].contains(&step) {
            assert_eq!(mock.flags.get(), 0);
            assert_eq!(mock.data, [0, 64, 0, 64, 0, 64, 0, 64]);
        }
        if [6, 7].contains(&step) {
            assert_eq!(mock.data, [0; 8]);
        }
        if step == 5 {
            assert!(result.unwrap());
            assert!(gate.ack_ready());
            assert!(gate.begin_commit().is_none());
        }
        if !gate.is_live() {
            assert!(gate.ack_ready());
        }
    }
}
#[test]
fn raw_render_get_and_release_failures_allocate_zero_and_never_submit_pcm() {
    for failure_step in [0, 1, 2] {
        let mut mock = RawRender {
            vtable: &RENDER,
            failure_step,
            gets: Cell::new(0),
            releases: Cell::new(0),
            flags: Cell::new(0),
            frames: Cell::new(0),
        };
        let render = ManuallyDrop::new(unsafe {
            IAudioRenderClient::from_raw(ptr::from_mut(&mut mock).cast())
        });
        let signal = ChangeSignal::new();
        signal.publish(ChangeKind::InitialValidation);
        assert!(signal.accept_initial_baseline());
        let mut state = State::Running;
        let (result, allocations) = count_allocations(|| {
            finish_operation(&signal, &mut state, render_silence(&render, 480))
        });
        assert_eq!(allocations, 0);
        assert_eq!(mock.gets.get(), 1);
        assert_eq!(mock.releases.get(), u32::from(failure_step != 1));
        if failure_step != 1 {
            assert_eq!(mock.frames.get(), 480);
            assert_eq!(mock.flags.get(), 2);
        }
        if failure_step == 0 {
            assert!(result.is_ok());
        } else {
            assert!(state == State::Retired);
            let expected = if failure_step == 1 {
                "RenderGetBuffer"
            } else {
                "RenderReleaseBuffer"
            };
            assert!(matches!(result, Err(StreamError::Com {operation,hresult})
                if operation == expected && hresult == FAILED.0));
        }
    }
}
#[test]
fn raw_padding_failure_ignores_written_value_and_retires_without_allocation() {
    let signal = ChangeSignal::new();
    signal.publish(ChangeKind::InitialValidation);
    assert!(signal.accept_initial_baseline());
    let mut state = State::Running;
    let (result, allocations) = count_allocations(|| {
        let result = padding_with(|out| {
            unsafe {
                *out = 999;
            }
            FAILED
        });
        finish_operation(&signal, &mut state, result)
    });
    assert_eq!(allocations, 0);
    assert!(state == State::Retired);
    assert!(
        matches!(result, Err(StreamError::Com {operation:"GetCurrentPadding",hresult})
        if hresult == FAILED.0)
    );
}
