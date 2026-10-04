use std::{
    io, mem,
    time::{Duration, Instant},
};
use windows::{
    Win32::{
        Foundation::{
            CloseHandle, ERROR_IO_PENDING, ERROR_PIPE_CONNECTED, GENERIC_READ, GENERIC_WRITE,
            HANDLE, HLOCAL, INVALID_HANDLE_VALUE, LocalFree, WAIT_OBJECT_0, WAIT_TIMEOUT,
        },
        Security::{
            Authorization::{
                ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW,
            },
            Cryptography::{BCRYPT_USE_SYSTEM_PREFERRED_RNG, BCryptGenRandom},
            GetTokenInformation, PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES, TOKEN_ELEVATION,
            TOKEN_QUERY, TOKEN_USER, TokenElevation, TokenUser,
        },
        Storage::FileSystem::{
            CreateFileW, FILE_FLAG_FIRST_PIPE_INSTANCE, FILE_FLAG_OVERLAPPED,
            FILE_FLAGS_AND_ATTRIBUTES, FILE_SHARE_MODE, OPEN_EXISTING, ReadFile,
            SECURITY_IDENTIFICATION, SECURITY_SQOS_PRESENT, WriteFile,
        },
        System::{
            IO::{CancelIoEx, GetOverlappedResult, OVERLAPPED},
            Pipes::{
                ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe,
                GetNamedPipeClientProcessId, GetNamedPipeServerProcessId, PIPE_READMODE_BYTE,
                PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE, PIPE_WAIT, WaitNamedPipeW,
            },
            Threading::{
                CreateEventW, GetCurrentProcess, OpenProcess, OpenProcessToken,
                PROCESS_QUERY_LIMITED_INFORMATION, WaitForSingleObject,
            },
        },
    },
    core::{HRESULT, PCWSTR, PWSTR},
};

pub use witvoice_contracts::CONTROL_MAX_BYTES;
pub const MAX_CONNECTION_TIME: Duration = Duration::from_secs(3);

/// Private launcher credential. Intentionally does not implement Debug or Clone.
pub struct Secret([u8; 32]);
impl Secret {
    pub fn generate() -> io::Result<Self> {
        let mut bytes = [0; 32];
        // SAFETY: BCrypt receives the actual writable fixed-size buffer; no provider handle.
        let status = unsafe { BCryptGenRandom(None, &mut bytes, BCRYPT_USE_SYSTEM_PREFERRED_RNG) };
        if status.0 < 0 {
            return Err(io::Error::other("system random generation failed"));
        }
        Ok(Self(bytes))
    }
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
    fn matches(&self, other: &[u8; 32]) -> bool {
        self.0
            .iter()
            .zip(other)
            .fold(0u8, |difference, (a, b)| difference | (a ^ b))
            == 0
    }
}

struct Handle(HANDLE);
impl Drop for Handle {
    fn drop(&mut self) {
        // SAFETY: these are uniquely owned real kernel handles, never pseudo handles.
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}
struct LocalAllocation(*mut core::ffi::c_void);
impl Drop for LocalAllocation {
    fn drop(&mut self) {
        // SAFETY: conversions below return memory owned by LocalAlloc.
        unsafe {
            let _ = LocalFree(Some(HLOCAL(self.0)));
        }
    }
}
fn os_error(error: windows::core::Error) -> io::Error {
    io::Error::other(format!("Windows API error {:#x}", error.code().0))
}
fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}
fn is_code(error: &windows::core::Error, code: u32) -> bool {
    error.code() == HRESULT::from_win32(code)
}

fn token_sid(token: HANDLE) -> io::Result<String> {
    let mut needed = 0;
    // SAFETY: first call only queries the bounded required buffer size.
    unsafe {
        let _ = GetTokenInformation(token, TokenUser, None, 0, &mut needed);
    }
    if needed == 0 || needed > 4096 {
        return Err(io::Error::other("invalid token information size"));
    }
    // usize alignment is sufficient for TOKEN_USER on this Windows target.
    let mut buffer = vec![0usize; (needed as usize).div_ceil(mem::size_of::<usize>())];
    // SAFETY: aligned allocation has at least needed bytes; SID remains inside it.
    unsafe {
        GetTokenInformation(
            token,
            TokenUser,
            Some(buffer.as_mut_ptr().cast()),
            needed,
            &mut needed,
        )
        .map_err(os_error)?;
        let user = &*buffer.as_ptr().cast::<TOKEN_USER>();
        let mut text = PWSTR::null();
        ConvertSidToStringSidW(user.User.Sid, &mut text).map_err(os_error)?;
        let _allocation = LocalAllocation(text.0.cast());
        text.to_string()
            .map_err(|_| io::Error::other("invalid token SID text"))
    }
}
fn process_sid(process: HANDLE) -> io::Result<String> {
    let mut token = HANDLE::default();
    // SAFETY: caller supplies a live process handle and output token storage.
    unsafe {
        OpenProcessToken(process, TOKEN_QUERY, &mut token).map_err(os_error)?;
    }
    let token = Handle(token);
    token_sid(token.0)
}
pub fn current_user_sid() -> io::Result<String> {
    // SAFETY: pseudo handle used only for querying, never owned/closed.
    process_sid(unsafe { GetCurrentProcess() })
}

pub fn current_process_is_elevated() -> io::Result<bool> {
    let mut raw_token = HANDLE::default();
    // SAFETY: pseudo process handle queried only; uniquely owned token is closed below.
    unsafe {
        OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut raw_token).map_err(os_error)?;
    }
    let token = Handle(raw_token);
    let mut elevation = TOKEN_ELEVATION::default();
    let mut returned = 0;
    // SAFETY: exact aligned TOKEN_ELEVATION buffer is writable and lives throughout call.
    unsafe {
        GetTokenInformation(
            token.0,
            TokenElevation,
            Some((&mut elevation as *mut TOKEN_ELEVATION).cast()),
            mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut returned,
        )
        .map_err(os_error)?;
    }
    Ok(elevation.TokenIsElevated != 0)
}
fn peer_is_current_user(pid: u32, expected_sid: &str) -> io::Result<()> {
    // SAFETY: query-only process handle; kernel supplies this PID, not a request payload.
    let process = Handle(
        unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }.map_err(os_error)?,
    );
    if process_sid(process.0)? != expected_sid {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "IPC peer user mismatch",
        ));
    }
    Ok(())
}

fn pipe_name(tag: &str, sid: &str) -> io::Result<Vec<u16>> {
    if tag.is_empty()
        || tag.len() > 32
        || !tag.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid endpoint tag",
        ));
    }
    // The Windows pipe namespace uses the protected object DACL rather than a directory.
    Ok(wide(&format!(r"\\.\pipe\witvoice.{sid}.{tag}.control.v1")))
}

/// Complete or cancel an operation before allowing its buffers/event to be dropped.
fn finish(
    handle: HANDLE,
    operation: &mut OVERLAPPED,
    started: windows::core::Result<()>,
    deadline: Instant,
) -> io::Result<u32> {
    if let Err(error) = started {
        if !is_code(&error, ERROR_IO_PENDING.0) {
            return Err(os_error(error));
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        let millis = remaining.as_millis().min(u32::MAX as u128) as u32;
        // SAFETY: operation event and all buffers remain alive throughout wait/cancel/completion.
        let wait = unsafe { WaitForSingleObject(operation.hEvent, millis) };
        if wait != WAIT_OBJECT_0 {
            unsafe {
                let _ = CancelIoEx(handle, Some(operation));
                let mut ignored = 0;
                // Cancellation completion must be observed before releasing OVERLAPPED/buffer.
                let _ = GetOverlappedResult(handle, operation, &mut ignored, true);
            }
            return Err(if wait == WAIT_TIMEOUT {
                io::Error::new(io::ErrorKind::TimedOut, "IPC operation deadline")
            } else {
                io::Error::other("IPC event wait failed")
            });
        }
    }
    let mut transferred = 0;
    // SAFETY: completed operation belongs to this handle and remains alive.
    unsafe {
        GetOverlappedResult(handle, operation, &mut transferred, false).map_err(os_error)?;
    }
    Ok(transferred)
}
fn operation() -> io::Result<(Handle, OVERLAPPED)> {
    // SAFETY: private unnamed, non-inheritable manual-reset event.
    let event =
        Handle(unsafe { CreateEventW(None, true, false, PCWSTR::null()) }.map_err(os_error)?);
    let overlapped = OVERLAPPED {
        hEvent: event.0,
        ..Default::default()
    };
    Ok((event, overlapped))
}
fn check_deadline(deadline: Instant) -> io::Result<()> {
    if Instant::now() >= deadline {
        Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "IPC connection deadline",
        ))
    } else {
        Ok(())
    }
}
fn read_exact(handle: HANDLE, mut bytes: &mut [u8], deadline: Instant) -> io::Result<()> {
    while !bytes.is_empty() {
        check_deadline(deadline)?;
        let (_event, mut overlapped) = operation()?;
        // SAFETY: slice and OVERLAPPED stay alive until finish observes completion.
        let started = unsafe { ReadFile(handle, Some(bytes), None, Some(&mut overlapped)) };
        let count = finish(handle, &mut overlapped, started, deadline)? as usize;
        if count == 0 {
            return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "IPC closed"));
        }
        bytes = &mut bytes[count..];
    }
    Ok(())
}
fn write_all(handle: HANDLE, mut bytes: &[u8], deadline: Instant) -> io::Result<()> {
    while !bytes.is_empty() {
        check_deadline(deadline)?;
        let (_event, mut overlapped) = operation()?;
        // SAFETY: slice and OVERLAPPED stay alive until finish observes completion.
        let started = unsafe { WriteFile(handle, Some(bytes), None, Some(&mut overlapped)) };
        let count = finish(handle, &mut overlapped, started, deadline)? as usize;
        if count == 0 {
            return Err(io::Error::new(io::ErrorKind::WriteZero, "IPC closed"));
        }
        bytes = &bytes[count..];
    }
    Ok(())
}
fn read_frame(handle: HANDLE, deadline: Instant) -> io::Result<Vec<u8>> {
    let mut header = [0; 4];
    read_exact(handle, &mut header, deadline)?;
    let length = u32::from_be_bytes(header) as usize;
    if length == 0 || length > CONTROL_MAX_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "IPC frame size limit",
        ));
    }
    let mut payload = vec![0; length];
    read_exact(handle, &mut payload, deadline)?;
    check_deadline(deadline)?;
    Ok(payload)
}
fn write_frame(handle: HANDLE, payload: &[u8], deadline: Instant) -> io::Result<()> {
    if payload.is_empty() || payload.len() > CONTROL_MAX_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "IPC frame size limit",
        ));
    }
    write_all(handle, &(payload.len() as u32).to_be_bytes(), deadline)?;
    write_all(handle, payload, deadline)
}

/// Exactly one instance/client. Remote clients rejected in the kernel.
pub struct PipeServer {
    handle: Handle,
    sid: String,
    secret: Secret,
    deadline: Option<Instant>,
}
impl PipeServer {
    pub fn bind(tag: &str, secret: Secret) -> io::Result<Self> {
        let sid = current_user_sid()?;
        let name = pipe_name(tag, &sid)?;
        let sddl = wide(&format!("D:P(A;;GA;;;{sid})"));
        let mut descriptor = PSECURITY_DESCRIPTOR::default();
        // SAFETY: valid NUL-terminated SDDL; returned allocation is kept through pipe creation.
        unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                PCWSTR(sddl.as_ptr()),
                1,
                &mut descriptor,
                None,
            )
            .map_err(os_error)?;
        }
        let _allocation = LocalAllocation(descriptor.0);
        let attributes = SECURITY_ATTRIBUTES {
            nLength: mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: descriptor.0,
            bInheritHandle: false.into(),
        };
        // SAFETY: name/attributes valid during creation, server handle non-inheritable.
        let handle = unsafe {
            CreateNamedPipeW(
                PCWSTR(name.as_ptr()),
                FILE_FLAGS_AND_ATTRIBUTES(3) | FILE_FLAG_OVERLAPPED | FILE_FLAG_FIRST_PIPE_INSTANCE,
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
                1,
                CONTROL_MAX_BYTES as u32,
                CONTROL_MAX_BYTES as u32,
                100,
                Some(&attributes),
            )
        };
        if handle == INVALID_HANDLE_VALUE {
            return Err(io::Error::last_os_error());
        }
        Ok(Self {
            handle: Handle(handle),
            sid,
            secret,
            deadline: None,
        })
    }

    pub fn receive(&mut self, timeout: Duration) -> io::Result<Vec<u8>> {
        if timeout.is_zero() || timeout > MAX_CONNECTION_TIME {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid IPC deadline",
            ));
        }
        if self.deadline.is_some() {
            return Err(io::Error::other("IPC connection still active"));
        }
        let (_event, mut overlapped) = operation()?;
        // An idle server may wait without a live client. This does not retain a frame/queue.
        let started = unsafe { ConnectNamedPipe(self.handle.0, Some(&mut overlapped)) };
        if let Err(error) = &started {
            if is_code(error, ERROR_PIPE_CONNECTED.0) {
                // Client won the creation/connect race. No asynchronous operation exists.
            } else if is_code(error, ERROR_IO_PENDING.0) {
                // SAFETY: private event and OVERLAPPED live until connection completes.
                unsafe {
                    WaitForSingleObject(overlapped.hEvent, u32::MAX);
                }
                let mut ignored = 0;
                unsafe {
                    GetOverlappedResult(self.handle.0, &overlapped, &mut ignored, false)
                        .map_err(os_error)?;
                }
            } else {
                return Err(os_error(error.clone()));
            }
        }
        let deadline = Instant::now() + timeout;
        self.deadline = Some(deadline);
        let mut pid = 0;
        // SAFETY: connected server handle; kernel, not JSON, supplies peer PID.
        unsafe {
            GetNamedPipeClientProcessId(self.handle.0, &mut pid).map_err(os_error)?;
        }
        peer_is_current_user(pid, &self.sid)?;
        let mut presented = [0; 32];
        read_exact(self.handle.0, &mut presented, deadline)?;
        if !self.secret.matches(&presented) {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "IPC authentication failed",
            ));
        }
        read_frame(self.handle.0, deadline)
    }
    pub fn respond(&mut self, payload: &[u8]) -> io::Result<()> {
        let deadline = self
            .deadline
            .ok_or_else(|| io::Error::other("IPC not connected"))?;
        write_frame(self.handle.0, payload, deadline)?;
        // DisconnectNamedPipe discards unread buffered bytes. A bounded ACK
        // proves the client consumed the response, without blocking FlushFileBuffers.
        let mut ack = [0];
        read_exact(self.handle.0, &mut ack, deadline)?;
        if ack != [0xa5] {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "IPC response ACK invalid",
            ));
        }
        Ok(())
    }
    pub fn connection_is_current(&self) -> bool {
        self.deadline
            .is_some_and(|deadline| Instant::now() < deadline)
    }
    pub fn disconnect(&mut self) {
        // SAFETY: no pending I/O remains after receive/respond; handle uniquely owned.
        unsafe {
            let _ = DisconnectNamedPipe(self.handle.0);
        }
        self.deadline = None;
    }
}

pub struct PipeClient {
    handle: Handle,
    deadline: Instant,
}
impl PipeClient {
    pub fn connect(tag: &str, expected_server_pid: u32, timeout: Duration) -> io::Result<Self> {
        if timeout.is_zero() || timeout > MAX_CONNECTION_TIME {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid IPC deadline",
            ));
        }
        let deadline = Instant::now() + timeout;
        let sid = current_user_sid()?;
        let name = pipe_name(tag, &sid)?;
        let handle = loop {
            check_deadline(deadline)?;
            // SAFETY: private namespace, non-inheritable overlapped client, identification-only SQOS.
            let opened = unsafe {
                CreateFileW(
                    PCWSTR(name.as_ptr()),
                    GENERIC_READ.0 | GENERIC_WRITE.0,
                    FILE_SHARE_MODE(0),
                    None,
                    OPEN_EXISTING,
                    FILE_FLAG_OVERLAPPED | SECURITY_SQOS_PRESENT | SECURITY_IDENTIFICATION,
                    None,
                )
            };
            match opened {
                Ok(handle) => break Handle(handle),
                Err(error) if is_code(&error, 2) || is_code(&error, 231) => {
                    unsafe {
                        let _ = WaitNamedPipeW(PCWSTR(name.as_ptr()), 10);
                    }
                    std::thread::sleep(Duration::from_millis(2));
                }
                Err(error) => return Err(os_error(error)),
            }
        };
        let mut pid = 0;
        // SAFETY: connected pipe handle; expected PID came from the launcher's child object.
        unsafe {
            GetNamedPipeServerProcessId(handle.0, &mut pid).map_err(os_error)?;
        }
        if pid != expected_server_pid {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "IPC server process mismatch",
            ));
        }
        peer_is_current_user(pid, &sid)?;
        Ok(Self { handle, deadline })
    }
    pub fn authenticate(&mut self, secret: &Secret) -> io::Result<()> {
        write_all(self.handle.0, secret.as_bytes(), self.deadline)
    }
    pub fn request(&mut self, payload: &[u8]) -> io::Result<Vec<u8>> {
        write_frame(self.handle.0, payload, self.deadline)?;
        self.read_response()
    }
    /// Raw bounded writes allow partial-frame/negative protocol tests, never arbitrary handles.
    pub fn write_fragment(&mut self, bytes: &[u8]) -> io::Result<()> {
        if bytes.len() > CONTROL_MAX_BYTES + 36 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "IPC fragment size limit",
            ));
        }
        write_all(self.handle.0, bytes, self.deadline)
    }
    pub fn read_response(&mut self) -> io::Result<Vec<u8>> {
        let response = read_frame(self.handle.0, self.deadline)?;
        write_all(self.handle.0, &[0xa5], self.deadline)?;
        Ok(response)
    }
}
