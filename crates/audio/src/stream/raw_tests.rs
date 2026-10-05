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
    data: [u8; 4],
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
        *frames = 2;
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
            data: [0, 64, 0, 64],
        };
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
