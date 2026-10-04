//! Metadata-only WASAPI operations on an ordinary COM thread. No Initialize,
//! Start, capture buffer, render buffer or default selection API.

use crate::format::{AudioFormat, Encoding, FormatError};
use serde::Serialize;
use std::{ffi::c_void, marker::PhantomData, ptr, rc::Rc};
use windows::{
    Win32::{
        Media::Audio::{
            AUDCLNT_SHAREMODE_SHARED, DEVICE_STATE, DEVICE_STATE_ACTIVE, IAudioClient, IMMDevice,
            IMMDeviceEnumerator, IMMEndpoint, MMDeviceEnumerator, WAVEFORMATEX,
            WAVEFORMATEXTENSIBLE, WAVEFORMATEXTENSIBLE_0, eAll, eCapture, eRender,
        },
        Media::KernelStreaming::KSDATAFORMAT_SUBTYPE_PCM,
        System::Com::{
            CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
            CoTaskMemFree, CoUninitialize,
        },
    },
    core::{GUID, Interface, PCWSTR},
};

const IEEE_FLOAT: GUID = GUID::from_u128(0x00000003_0000_0010_8000_00aa00389b71);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Flow {
    Capture,
    Render,
}

#[derive(Debug, Serialize)]
pub enum MetadataError {
    Com {
        operation: &'static str,
        hresult: i32,
    },
    InvalidUid,
    Inactive,
    WrongFlow,
    Format {
        reason: String,
    },
    UnexpectedFormatStatus(i32),
}

fn com(operation: &'static str, error: windows::core::Error) -> MetadataError {
    MetadataError::Com {
        operation,
        hresult: error.code().0,
    }
}

fn format_error(error: FormatError) -> MetadataError {
    MetadataError::Format {
        reason: format!("{error:?}"),
    }
}

pub(crate) struct Apartment(PhantomData<Rc<()>>);
impl Apartment {
    pub(crate) fn enter() -> Result<Self, MetadataError> {
        // S_OK and S_FALSE both require balancing CoUninitialize. A changed
        // apartment failure returns before constructing this thread-bound guard.
        // Use an ordinary STA thread for IAudioClient's documented first-use
        // requirement. An existing MTA is rejected; no silent mode fallback.
        unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok() }
            .map_err(|e| com("CoInitializeEx", e))?;
        Ok(Self(PhantomData))
    }
}
impl Drop for Apartment {
    fn drop(&mut self) {
        unsafe { CoUninitialize() };
    }
}

struct TaskMem<T>(*mut T);
impl<T> Drop for TaskMem<T> {
    fn drop(&mut self) {
        unsafe { CoTaskMemFree(Some(self.0.cast::<c_void>())) };
    }
}

/// Raw fields are retained even when the conversion adapter rejects the format.
#[derive(Debug, Clone, Serialize)]
pub struct MixFormat {
    pub format_tag: u16,
    pub sample_rate: u32,
    pub channels: u16,
    pub container_bits: u16,
    pub valid_bits: u16,
    pub block_align: u16,
    pub average_bytes_per_second: u32,
    pub extra_bytes: u16,
    pub channel_mask: Option<u32>,
    pub subformat: Option<String>,
    pub adapter_format: Option<AudioFormat>,
    pub adapter_error: Option<String>,
}

// Only called on pointers returned by WASAPI or owned WAVEFORMATEX values.
// cbSize must be checked before reading the extension. Packed native structs
// are copied with read_unaligned; no reference is taken to a packed field.
unsafe fn read_format(pointer: *const WAVEFORMATEX) -> Result<MixFormat, MetadataError> {
    if pointer.is_null() {
        return Err(format_error(FormatError::Layout));
    }
    let raw = unsafe { ptr::read_unaligned(pointer) };
    let mut valid_bits = raw.wBitsPerSample;
    let mut mask = None;
    let mut subtype = None;
    if raw.wFormatTag == 0xfffe {
        if raw.cbSize < 22 {
            return Err(format_error(FormatError::Layout));
        }
        let extension = unsafe { ptr::read_unaligned(pointer.cast::<WAVEFORMATEXTENSIBLE>()) };
        valid_bits = unsafe { extension.Samples.wValidBitsPerSample };
        mask = Some(extension.dwChannelMask);
        subtype = Some(extension.SubFormat);
    }
    let result = (|| {
        let float =
            raw.wFormatTag == 3 || (raw.wFormatTag == 0xfffe && subtype == Some(IEEE_FLOAT));
        let pcm = raw.wFormatTag == 1
            || (raw.wFormatTag == 0xfffe && subtype == Some(KSDATAFORMAT_SUBTYPE_PCM));
        let encoding = match (pcm, float, raw.wBitsPerSample) {
            (true, false, 8) => Encoding::Pcm8,
            (true, false, 16) => Encoding::Pcm16,
            (true, false, 24) => Encoding::Pcm24,
            (true, false, 32) => Encoding::Pcm32,
            (false, true, 32) => Encoding::Float32,
            _ => return Err(FormatError::UnsupportedEncoding),
        };
        // Reduced valid bits require a separate, verified left-aligned adapter.
        // Do not incorrectly treat e.g. 24-in-32 as ordinary signed PCM32.
        if valid_bits != raw.wBitsPerSample {
            return Err(FormatError::Layout);
        }
        let checked = AudioFormat::new(raw.nSamplesPerSec, raw.nChannels, encoding)?;
        if raw.nBlockAlign as usize != checked.frame_bytes()
            || raw.nAvgBytesPerSec != raw.nSamplesPerSec * raw.nBlockAlign as u32
        {
            return Err(FormatError::Layout);
        }
        if let Some(mask) = mask {
            // Defined mono center / stereo left-right, or unspecified ordering.
            if mask != 0 && mask != if raw.nChannels == 1 { 4 } else { 3 } {
                return Err(FormatError::Layout);
            }
        }
        Ok(checked)
    })();
    Ok(MixFormat {
        format_tag: raw.wFormatTag,
        sample_rate: raw.nSamplesPerSec,
        channels: raw.nChannels,
        container_bits: raw.wBitsPerSample,
        valid_bits,
        block_align: raw.nBlockAlign,
        average_bytes_per_second: raw.nAvgBytesPerSec,
        extra_bytes: raw.cbSize,
        channel_mask: mask,
        subformat: subtype.map(|id| format!("{id:?}")),
        adapter_format: result.as_ref().ok().copied(),
        adapter_error: result.err().map(|e| format!("{e:?}")),
    })
}

#[derive(Debug, Serialize)]
pub struct Endpoint {
    pub uid: String,
    pub flow: Flow,
    pub state_bits: u32,
    pub mix_format: Option<MixFormat>,
    pub mix_error: Option<MetadataError>,
}

fn device_flow(device: &IMMDevice) -> Result<Flow, MetadataError> {
    let endpoint: IMMEndpoint = device
        .cast()
        .map_err(|e| com("IMMEndpoint::QueryInterface", e))?;
    match unsafe { endpoint.GetDataFlow() }.map_err(|e| com("GetDataFlow", e))? {
        f if f == eCapture => Ok(Flow::Capture),
        f if f == eRender => Ok(Flow::Render),
        _ => Err(MetadataError::WrongFlow),
    }
}

fn client(device: &IMMDevice) -> Result<IAudioClient, MetadataError> {
    unsafe { device.Activate(CLSCTX_INPROC_SERVER, None) }
        .map_err(|e| com("ActivateIAudioClient", e))
}

fn mix(client: &IAudioClient) -> Result<MixFormat, MetadataError> {
    let memory = TaskMem(unsafe { client.GetMixFormat() }.map_err(|e| com("GetMixFormat", e))?);
    unsafe { read_format(memory.0) }
}

fn snapshot(device: &IMMDevice) -> Result<Endpoint, MetadataError> {
    let id = unsafe { device.GetId() }.map_err(|e| com("GetId", e))?;
    let memory = TaskMem(id.0);
    if memory.0.is_null() {
        return Err(MetadataError::InvalidUid);
    }
    let uid = unsafe { id.to_string() }.map_err(|_| MetadataError::InvalidUid)?;
    let flow = device_flow(device)?;
    let state = unsafe { device.GetState() }.map_err(|e| com("GetState", e))?;
    let result = if state == DEVICE_STATE_ACTIVE {
        client(device).and_then(|c| mix(&c))
    } else {
        Err(MetadataError::Inactive)
    };
    Ok(Endpoint {
        uid,
        flow,
        state_bits: state.0,
        mix_format: result.as_ref().ok().cloned(),
        mix_error: result.err(),
    })
}

pub(crate) fn enumerator() -> Result<IMMDeviceEnumerator, MetadataError> {
    unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_INPROC_SERVER) }
        .map_err(|e| com("CoCreateInstance", e))
}

/// Reads all endpoint metadata, including inactive entries. No friendly name
/// inference: neither virtual-line trust nor a fallback device is selected.
pub fn enumerate_endpoints() -> Result<Vec<Endpoint>, MetadataError> {
    let _apartment = Apartment::enter()?;
    let enumerator = enumerator()?;
    let collection = unsafe { enumerator.EnumAudioEndpoints(eAll, DEVICE_STATE(15)) }
        .map_err(|e| com("EnumAudioEndpoints", e))?;
    let count = unsafe { collection.GetCount() }.map_err(|e| com("GetCount", e))?;
    let mut endpoints = Vec::new();
    for index in 0..count {
        let device = unsafe { collection.Item(index) }.map_err(|e| com("Item", e))?;
        endpoints.push(snapshot(&device)?);
    }
    Ok(endpoints)
}

fn selected(
    enumerator: &IMMDeviceEnumerator,
    uid: &str,
    flow: Flow,
) -> Result<IMMDevice, MetadataError> {
    if uid.is_empty() || uid.contains('\0') {
        return Err(MetadataError::InvalidUid);
    }
    let wide: Vec<u16> = uid.encode_utf16().chain(Some(0)).collect();
    let device = unsafe { enumerator.GetDevice(PCWSTR(wide.as_ptr())) }
        .map_err(|e| com("GetDeviceByUid", e))?;
    if device_flow(&device)? != flow {
        return Err(MetadataError::WrongFlow);
    }
    if unsafe { device.GetState() }.map_err(|e| com("GetState", e))? != DEVICE_STATE_ACTIVE {
        return Err(MetadataError::Inactive);
    }
    Ok(device)
}

pub fn inspect_endpoint(uid: &str, flow: Flow) -> Result<Endpoint, MetadataError> {
    let _apartment = Apartment::enter()?;
    snapshot(&selected(&enumerator()?, uid, flow)?)
}

#[derive(Debug, Serialize)]
pub struct FormatProbe {
    pub requested: AudioFormat,
    pub exact_supported: bool,
    pub closest: Option<MixFormat>,
}

// Own the complete native descriptor through the synchronous WASAPI call.
// PCM precision above 16 bits requires the extension rather than tag 1.
enum NativeDescriptor {
    Basic(WAVEFORMATEX),
    Extensible(WAVEFORMATEXTENSIBLE),
}

impl NativeDescriptor {
    fn new(format: AudioFormat) -> Self {
        let extended = matches!(format.encoding(), Encoding::Pcm24 | Encoding::Pcm32);
        let bits = (format.encoding().bytes() * 8) as u16;
        let base = WAVEFORMATEX {
            wFormatTag: if extended {
                0xfffe
            } else if format.encoding() == Encoding::Float32 {
                3
            } else {
                1
            },
            nChannels: format.channels(),
            nSamplesPerSec: format.sample_rate(),
            nAvgBytesPerSec: format.sample_rate() * format.frame_bytes() as u32,
            nBlockAlign: format.frame_bytes() as u16,
            wBitsPerSample: bits,
            cbSize: if extended { 22 } else { 0 },
        };
        if extended {
            Self::Extensible(WAVEFORMATEXTENSIBLE {
                Format: base,
                Samples: WAVEFORMATEXTENSIBLE_0 {
                    wValidBitsPerSample: bits,
                },
                dwChannelMask: if format.channels() == 1 { 4 } else { 3 },
                SubFormat: KSDATAFORMAT_SUBTYPE_PCM,
            })
        } else {
            Self::Basic(base)
        }
    }

    // The pointer is borrowed from self's storage, not a temporary descriptor.
    // Native callers must retain self without moving it until the call returns.
    fn as_wave_ptr(&self) -> *const WAVEFORMATEX {
        match self {
            Self::Basic(base) => ptr::from_ref(base),
            Self::Extensible(extension) => ptr::addr_of!(extension.Format),
        }
    }
}

/// Shared-mode format probe on the explicit active UID. A closest match is
/// reported, never treated as exact success or silently used to start a stream.
pub fn probe_format(
    uid: &str,
    flow: Flow,
    requested: AudioFormat,
) -> Result<FormatProbe, MetadataError> {
    let _apartment = Apartment::enter()?;
    let device = selected(&enumerator()?, uid, flow)?;
    let client = client(&device)?;
    let native = NativeDescriptor::new(requested);
    let mut closest = ptr::null_mut();
    let status = unsafe {
        client.IsFormatSupported(
            AUDCLNT_SHAREMODE_SHARED,
            native.as_wave_ptr(),
            Some(&mut closest),
        )
    };
    let memory = TaskMem(closest);
    match status.0 {
        0 => Ok(FormatProbe {
            requested,
            exact_supported: true,
            closest: None,
        }),
        1 if !memory.0.is_null() => Ok(FormatProbe {
            requested,
            exact_supported: false,
            closest: Some(unsafe { read_format(memory.0) }?),
        }),
        code if code < 0 => Err(MetadataError::Com {
            operation: "IsFormatSupported",
            hresult: code,
        }),
        code => Err(MetadataError::UnexpectedFormatStatus(code)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn high_precision_pcm_descriptors_own_complete_extension_and_roundtrip() {
        for encoding in [Encoding::Pcm24, Encoding::Pcm32] {
            for channels in 1..=2 {
                let expected = AudioFormat::new(48_000, channels, encoding).unwrap();
                let native = NativeDescriptor::new(expected);
                let NativeDescriptor::Extensible(extension) = &native else {
                    panic!("high-precision PCM must use WAVEFORMATEXTENSIBLE");
                };
                // Copy packed fields before comparisons; never borrow them.
                let raw = extension.Format;
                let valid_bits = unsafe { extension.Samples.wValidBitsPerSample };
                let channel_mask = extension.dwChannelMask;
                let subformat = extension.SubFormat;
                let tag = raw.wFormatTag;
                let extra_bytes = raw.cbSize;
                let bits = raw.wBitsPerSample;
                let align = raw.nBlockAlign;
                let byte_rate = raw.nAvgBytesPerSec;
                assert_eq!(tag, 0xfffe);
                assert_eq!(extra_bytes, 22);
                assert_eq!(bits, (encoding.bytes() * 8) as u16);
                assert_eq!(valid_bits, bits);
                assert_eq!(channel_mask, if channels == 1 { 4 } else { 3 });
                assert_eq!(subformat, KSDATAFORMAT_SUBTYPE_PCM);
                assert_eq!(align as usize, expected.frame_bytes());
                assert_eq!(byte_rate, 48_000 * expected.frame_bytes() as u32);
                let parsed = unsafe { read_format(native.as_wave_ptr()) }.unwrap();
                assert_eq!(parsed.adapter_format, Some(expected));
                assert!(parsed.adapter_error.is_none());
                assert_eq!(parsed.extra_bytes, 22);
                assert_eq!(parsed.valid_bits, bits);
                assert_eq!(parsed.channel_mask, Some(channel_mask));
            }
        }
    }

    #[test]
    fn basic_pcm_and_float_descriptors_keep_documented_tags_and_roundtrip() {
        for encoding in [Encoding::Pcm8, Encoding::Pcm16, Encoding::Float32] {
            for channels in 1..=2 {
                let expected = AudioFormat::new(44_100, channels, encoding).unwrap();
                let native = NativeDescriptor::new(expected);
                let NativeDescriptor::Basic(base) = &native else {
                    panic!("simple formats should keep their basic descriptor");
                };
                let tag = base.wFormatTag;
                let extra_bytes = base.cbSize;
                assert_eq!(tag, if encoding == Encoding::Float32 { 3 } else { 1 });
                assert_eq!(extra_bytes, 0);
                let parsed = unsafe { read_format(native.as_wave_ptr()) }.unwrap();
                assert_eq!(parsed.adapter_format, Some(expected));
                assert!(parsed.adapter_error.is_none());
                assert!(parsed.channel_mask.is_none());
                assert!(parsed.subformat.is_none());
            }
        }
    }

    #[test]
    fn validates_native_layout_and_rejects_short_extension() {
        let mut native = WAVEFORMATEX {
            wFormatTag: 1,
            nChannels: 2,
            nSamplesPerSec: 44_100,
            nAvgBytesPerSec: 176_400,
            nBlockAlign: 4,
            wBitsPerSample: 16,
            cbSize: 0,
        };
        let parsed = unsafe { read_format(&native) }.unwrap();
        assert!(parsed.adapter_format.unwrap().needs_bus_resampling());
        native.nBlockAlign = 3;
        assert!(
            unsafe { read_format(&native) }
                .unwrap()
                .adapter_format
                .is_none()
        );
        native.wFormatTag = 0xfffe;
        assert!(unsafe { read_format(&native) }.is_err());
    }
    #[test]
    fn refuses_reduced_valid_bits_and_unexpected_channel_mask() {
        let mut native = WAVEFORMATEXTENSIBLE {
            Format: WAVEFORMATEX {
                wFormatTag: 0xfffe,
                nChannels: 2,
                nSamplesPerSec: 48_000,
                nAvgBytesPerSec: 384_000,
                nBlockAlign: 8,
                wBitsPerSample: 32,
                cbSize: 22,
            },
            SubFormat: KSDATAFORMAT_SUBTYPE_PCM,
            ..Default::default()
        };
        native.Samples.wValidBitsPerSample = 24;
        assert!(
            unsafe { read_format(ptr::addr_of!(native.Format)) }
                .unwrap()
                .adapter_format
                .is_none()
        );
        native.Samples.wValidBitsPerSample = 32;
        native.dwChannelMask = 3;
        assert!(
            unsafe { read_format(ptr::addr_of!(native.Format)) }
                .unwrap()
                .adapter_format
                .is_some()
        );
        native.dwChannelMask = 12;
        assert!(
            unsafe { read_format(ptr::addr_of!(native.Format)) }
                .unwrap()
                .adapter_format
                .is_none()
        );
    }
}
