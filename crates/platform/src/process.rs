//! Trusted in-process launch API. Never accepts executable paths from control IPC.
use crate::Secret;
use std::{
    ffi::{OsStr, OsString},
    fs::File,
    io::{self, Write},
    mem,
    os::windows::{
        ffi::OsStrExt,
        io::{FromRawHandle, RawHandle},
    },
    path::Path,
    time::{Duration, Instant},
};
use windows::{
    Win32::{
        Foundation::{
            CloseHandle, ERROR_ALREADY_EXISTS, GENERIC_READ, GENERIC_WRITE, GetLastError, HANDLE,
            HANDLE_FLAG_INHERIT, SetHandleInformation, WAIT_OBJECT_0, WAIT_TIMEOUT,
        },
        Security::SECURITY_ATTRIBUTES,
        Storage::FileSystem::{
            CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
        },
        System::{
            JobObjects::*, Pipes::CreatePipe, SystemServices::JOB_OBJECT_QUERY, Threading::*,
        },
    },
    core::{BOOL, PCWSTR, PWSTR},
};

struct Owned(HANDLE);
impl Drop for Owned {
    fn drop(&mut self) {
        // SAFETY: owns a non-pseudo, unique kernel handle.
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}
/// Fixed native operation identity, never caller paths or command arguments.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProcessOperation {
    Unknown,
    TerminateJob,
    QueryJobActive,
    QueryJobIds,
    GetProcessExitCode,
}

/// HRESULT domain; this is deliberately separate from Win32 raw_os_error.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProcessNativeFailure {
    pub operation: ProcessOperation,
    pub hresult: i32,
}
#[derive(Debug)]
struct NativeProcessError(ProcessNativeFailure);
impl std::fmt::Display for NativeProcessError {
    fn fmt(&self, output: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            output,
            "Windows process operation failed ({:#x})",
            self.0.hresult
        )
    }
}
impl std::error::Error for NativeProcessError {}

/// Only our typed native source is recognized; arbitrary error text is not parsed.
pub fn process_native_failure(error: &io::Error) -> Option<ProcessNativeFailure> {
    error
        .get_ref()?
        .downcast_ref::<NativeProcessError>()
        .map(|source| source.0)
}
fn fail_operation(operation: ProcessOperation, error: windows::core::Error) -> io::Error {
    io::Error::other(NativeProcessError(ProcessNativeFailure {
        operation,
        hresult: error.code().0,
    }))
}
fn fail(error: windows::core::Error) -> io::Error {
    fail_operation(ProcessOperation::Unknown, error)
}
fn wide(value: &OsStr) -> io::Result<Vec<u16>> {
    let mut result: Vec<u16> = value.encode_wide().take(4097).collect();
    if result.len() > 4096 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "launch value too long",
        ));
    }
    if result.contains(&0) {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "embedded NUL"));
    }
    result.push(0);
    Ok(result)
}
fn quote(value: &OsStr) -> io::Result<Vec<u16>> {
    let chars = wide(value)?;
    let mut output = vec![b'"' as u16];
    let mut slashes = 0;
    for &c in &chars[..chars.len() - 1] {
        if c == b'\\' as u16 {
            slashes += 1;
            continue;
        }
        output.extend(std::iter::repeat_n(
            b'\\' as u16,
            if c == b'"' as u16 {
                slashes * 2 + 1
            } else {
                slashes
            },
        ));
        output.push(c);
        slashes = 0;
    }
    output.extend(std::iter::repeat_n(b'\\' as u16, slashes * 2));
    output.push(b'"' as u16);
    Ok(output)
}
fn pipe() -> io::Result<(Owned, Owned)> {
    let attributes = SECURITY_ATTRIBUTES {
        nLength: mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
        bInheritHandle: BOOL(1),
        ..Default::default()
    };
    let (mut read, mut write) = (HANDLE::default(), HANDLE::default());
    // SAFETY: writable handle destinations; initialized security descriptor, 4KiB bounded pipe.
    unsafe {
        CreatePipe(&mut read, &mut write, Some(&attributes), 4096).map_err(fail)?;
    }
    Ok((Owned(read), Owned(write)))
}
fn not_inherited(handle: &Owned) -> io::Result<()> {
    // SAFETY: live owned handle, clear only the inherit flag.
    unsafe {
        SetHandleInformation(handle.0, HANDLE_FLAG_INHERIT.0, Default::default()).map_err(fail)
    }
}
fn null() -> io::Result<Owned> {
    let path = wide(OsStr::new("NUL"))?;
    let attributes = SECURITY_ATTRIBUTES {
        nLength: mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
        bInheritHandle: BOOL(1),
        ..Default::default()
    };
    // SAFETY: valid terminated device name and initialized attributes.
    unsafe {
        CreateFileW(
            PCWSTR(path.as_ptr()),
            GENERIC_READ.0 | GENERIC_WRITE.0,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            Some(&attributes),
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL,
            None,
        )
        .map(Owned)
        .map_err(fail)
    }
}
fn into_file(handle: Owned) -> File {
    let raw = handle.0.0 as RawHandle;
    mem::forget(handle);
    // SAFETY: transfers exclusive owned kernel handle exactly once.
    unsafe { File::from_raw_handle(raw) }
}

/// A process handle does not own its lifetime. Dropping a UI launcher leaves Node alive.
pub struct Process {
    handle: Owned,
    pid: u32,
    output: Option<File>,
}
impl Process {
    pub fn id(&self) -> u32 {
        self.pid
    }
    pub fn take_output(&mut self) -> Option<File> {
        self.output.take()
    }
    pub fn wait(&self, timeout: Duration) -> io::Result<Option<u32>> {
        let ms = u32::try_from(timeout.as_millis())
            .unwrap_or(u32::MAX - 1)
            .min(u32::MAX - 1);
        // SAFETY: live process handle, bounded wait, writable exit-code destination.
        unsafe {
            match WaitForSingleObject(self.handle.0, ms) {
                WAIT_OBJECT_0 => {
                    let mut code = 0;
                    GetExitCodeProcess(self.handle.0, &mut code).map_err(|error| {
                        fail_operation(ProcessOperation::GetProcessExitCode, error)
                    })?;
                    Ok(Some(code))
                }
                WAIT_TIMEOUT => Ok(None),
                _ => Err(io::Error::last_os_error()),
            }
        }
    }
    pub fn terminate(&self) -> io::Result<()> {
        // SAFETY: live owned process handle; no PID re-resolution or PID-reuse race.
        unsafe {
            TerminateProcess(self.handle.0, 1).map_err(fail)?;
        }
        if self.wait(Duration::from_secs(3))?.is_none() {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "process termination deadline",
            ));
        }
        Ok(())
    }
    /// Observe only a known test PID; production supervision retains process handles.
    pub fn observe(pid: u32) -> io::Result<Self> {
        Self::open_known(pid, false)
    }
    /// Test cleanup for an explicitly known child; never exposed over control IPC.
    pub fn observe_for_cleanup(pid: u32) -> io::Result<Self> {
        Self::open_known(pid, true)
    }
    fn open_known(pid: u32, cleanup: bool) -> io::Result<Self> {
        // SAFETY: requests only query/synchronize access, non-inheritable handle.
        let handle = unsafe {
            OpenProcess(
                PROCESS_QUERY_LIMITED_INFORMATION
                    | PROCESS_SYNCHRONIZE
                    | if cleanup {
                        PROCESS_TERMINATE
                    } else {
                        PROCESS_ACCESS_RIGHTS(0)
                    },
                false,
                pid,
            )
            .map_err(fail)?
        };
        Ok(Self {
            handle: Owned(handle),
            pid,
            output: None,
        })
    }
    pub fn image_filename(&self) -> io::Result<String> {
        let mut path = vec![0u16; 32768];
        let mut len = path.len() as u32;
        // SAFETY: live QUERY handle and bounded writable UTF-16 buffer with exact capacity.
        unsafe {
            QueryFullProcessImageNameW(
                self.handle.0,
                PROCESS_NAME_WIN32,
                PWSTR(path.as_mut_ptr()),
                &mut len,
            )
            .map_err(fail)?;
        }
        let path = String::from_utf16_lossy(&path[..len as usize]);
        Ok(Path::new(&path)
            .file_name()
            .ok_or_else(|| io::Error::other("missing process image name"))?
            .to_string_lossy()
            .into_owned())
    }
}

/// Private unnamed, non-inheritable owner handle. Closing it kills the complete tree.
pub struct ProcessJob {
    handle: Owned,
}
impl ProcessJob {
    /// Worker owners use false; true is for an explicitly breakaway-capable UI owner.
    pub fn new(allow_breakaway: bool) -> io::Result<Self> {
        Self::create(allow_breakaway, None)
    }
    /// Explicit UI job identity; worker jobs stay unnamed and disallow breakaway.
    pub fn named_ui(name: &str, allow_breakaway: bool) -> io::Result<Self> {
        Self::create(allow_breakaway, Some(name))
    }
    pub fn open_ui_query(name: &str) -> io::Result<Self> {
        let name = wide(OsStr::new(name))?;
        // SAFETY: QUERY-only handle, never inheritable.
        let handle = unsafe {
            OpenJobObjectW(JOB_OBJECT_QUERY, false, PCWSTR(name.as_ptr())).map_err(fail)?
        };
        Ok(Self {
            handle: Owned(handle),
        })
    }
    fn create(allow_breakaway: bool, name: Option<&str>) -> io::Result<Self> {
        let name = name.map(|name| wide(OsStr::new(name))).transpose()?;
        // SAFETY: unnamed private job; default security creates a non-inheritable handle.
        let handle = Owned(unsafe {
            CreateJobObjectW(
                None,
                name.as_ref()
                    .map_or(PCWSTR::null(), |name| PCWSTR(name.as_ptr())),
            )
            .map_err(fail)?
        });
        // SAFETY: immediately follows CreateJobObject on this thread. Never mutate a pre-existing named job.
        if name.is_some() && unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "UI job identity already exists",
            ));
        }
        let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        info.BasicLimitInformation.LimitFlags =
            JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE | JOB_OBJECT_LIMIT_ACTIVE_PROCESS;
        info.BasicLimitInformation.ActiveProcessLimit = 8;
        if allow_breakaway {
            info.BasicLimitInformation.LimitFlags |= JOB_OBJECT_LIMIT_BREAKAWAY_OK;
        }
        // SAFETY: typed initialized structure with exact size, no pointer fields.
        unsafe {
            SetInformationJobObject(
                handle.0,
                JobObjectExtendedLimitInformation,
                &info as *const _ as _,
                mem::size_of_val(&info) as u32,
            )
            .map_err(fail)?;
        }
        Ok(Self { handle })
    }
    pub fn spawn(
        &self,
        program: &Path,
        args: &[OsString],
        capture_output: bool,
    ) -> io::Result<Process> {
        spawn(program, args, Some(self), false, capture_output, None, None)
    }
    pub fn spawn_bootstrapped(
        &self,
        program: &Path,
        args: &[OsString],
        capture_output: bool,
        secret: &Secret,
    ) -> io::Result<Process> {
        spawn(
            program,
            args,
            Some(self),
            false,
            capture_output,
            Some(secret),
            None,
        )
    }
    pub fn active_processes(&self) -> io::Result<u32> {
        let mut info = JOBOBJECT_BASIC_ACCOUNTING_INFORMATION::default();
        // SAFETY: correctly typed writable destination, exact structure size.
        unsafe {
            QueryInformationJobObject(
                Some(self.handle.0),
                JobObjectBasicAccountingInformation,
                &mut info as *mut _ as _,
                mem::size_of_val(&info) as u32,
                None,
            )
            .map_err(|error| fail_operation(ProcessOperation::QueryJobActive, error))?;
        }
        Ok(info.ActiveProcesses)
    }
    /// Bounded by this owner's eight-process limit; used for explicit cleanup evidence.
    pub fn process_ids(&self) -> io::Result<Vec<u32>> {
        #[repr(C)]
        struct List {
            assigned: u32,
            count: u32,
            ids: [usize; 8],
        }
        let mut info = List {
            assigned: 0,
            count: 0,
            ids: [0; 8],
        };
        // SAFETY: variable-length Windows structure with space for the full configured maximum.
        unsafe {
            QueryInformationJobObject(
                Some(self.handle.0),
                JobObjectBasicProcessIdList,
                &mut info as *mut _ as _,
                mem::size_of_val(&info) as u32,
                None,
            )
            .map_err(|error| fail_operation(ProcessOperation::QueryJobIds, error))?;
        }
        if info.count > 8 {
            return Err(io::Error::other("job exceeded bounded process list"));
        }
        info.ids[..info.count as usize]
            .iter()
            .map(|id| u32::try_from(*id).map_err(|_| io::Error::other("invalid process ID")))
            .collect()
    }
    pub fn terminate(&self) -> io::Result<()> {
        // SAFETY: private owner job, includes descendants (worker job disallows breakaway).
        unsafe {
            TerminateJobObject(self.handle.0, 1)
                .map_err(|error| fail_operation(ProcessOperation::TerminateJob, error))?;
        }
        let deadline = Instant::now() + Duration::from_secs(3);
        while self.active_processes()? != 0 {
            if Instant::now() >= deadline {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "job cleanup deadline",
                ));
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        Ok(())
    }
}

/// Fixed Node argv and private 32-byte stdin bootstrap; no shell or credential argv/env.
pub fn launch_node(program: &Path, endpoint: &str, secret: &Secret) -> io::Result<Process> {
    launch_node_inner(program, endpoint, secret, None)
}
/// Verify separation from a known UI owner, permitting an unrelated hosting job.
/// Caller must know this witness covers all UI-owned lifetime jobs. Unknown UI
/// ownership must be rejected by the launcher; this API does not invent an inventory.
pub fn launch_node_from_ui_job(
    program: &Path,
    endpoint: &str,
    secret: &Secret,
    ui: &ProcessJob,
) -> io::Result<Process> {
    launch_node_inner(program, endpoint, secret, Some(ui))
}
fn launch_node_inner(
    program: &Path,
    endpoint: &str,
    secret: &Secret,
    ui: Option<&ProcessJob>,
) -> io::Result<Process> {
    if endpoint.is_empty()
        || endpoint.len() > 32
        || !endpoint
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-')
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid endpoint",
        ));
    }
    spawn(
        program,
        &[
            "--endpoint".into(),
            endpoint.into(),
            "--bootstrap-stdin".into(),
        ],
        None,
        true,
        false,
        Some(secret),
        ui,
    )
}

struct Attributes {
    storage: Vec<usize>,
    list: LPPROC_THREAD_ATTRIBUTE_LIST,
}
impl Attributes {
    fn new(handles: &[HANDLE]) -> io::Result<Self> {
        let mut bytes = 0;
        // SAFETY: size query requires null destination; first call intentionally reports insufficient buffer.
        unsafe {
            let _ = InitializeProcThreadAttributeList(None, 1, None, &mut bytes);
        }
        if bytes == 0 || bytes > 65536 {
            return Err(io::Error::other("invalid attribute-list size"));
        }
        let mut storage = vec![0usize; bytes.div_ceil(mem::size_of::<usize>())];
        let list = LPPROC_THREAD_ATTRIBUTE_LIST(storage.as_mut_ptr().cast());
        // SAFETY: suitably aligned allocation stays alive until Delete; handles remain live through CreateProcess.
        unsafe {
            InitializeProcThreadAttributeList(Some(list), 1, None, &mut bytes).map_err(fail)?;
        }
        let owner = Self { storage, list };
        // SAFETY: exact bounded slice size and initialized list. Only specified stdio handles are inherited.
        unsafe {
            UpdateProcThreadAttribute(
                list,
                0,
                PROC_THREAD_ATTRIBUTE_HANDLE_LIST as usize,
                Some(handles.as_ptr().cast()),
                mem::size_of_val(handles),
                None,
                None,
            )
            .map_err(fail)?;
        }
        Ok(owner)
    }
}
impl Drop for Attributes {
    fn drop(&mut self) {
        // SAFETY: list remains inside the owned allocation until this call completes.
        unsafe {
            DeleteProcThreadAttributeList(self.list);
        }
        let _ = self.storage.len();
    }
}
#[allow(clippy::too_many_arguments)]
fn spawn(
    program: &Path,
    args: &[OsString],
    job: Option<&ProcessJob>,
    detached: bool,
    capture: bool,
    secret: Option<&Secret>,
    separate_from: Option<&ProcessJob>,
) -> io::Result<Process> {
    if !program.is_absolute() || !program.is_file() || args.len() > 16 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "trusted launch configuration invalid",
        ));
    }
    let application = wide(program.as_os_str())?;
    if let Some(ui) = separate_from {
        let mut in_job = BOOL(0);
        // SAFETY: borrowed current-process pseudo handle and live UI QUERY handle.
        unsafe {
            IsProcessInJob(GetCurrentProcess(), Some(ui.handle.0), &mut in_job).map_err(fail)?;
        }
        if !in_job.as_bool() {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "launcher does not belong to supplied UI job",
            ));
        }
    }
    let mut command = quote(program.as_os_str())?;
    for arg in args {
        command.push(b' ' as u16);
        command.extend(quote(arg)?);
    }
    if command.len() > 4096 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "launch arguments too long",
        ));
    }
    command.push(0);
    let null = null()?;
    let input = if secret.is_some() {
        Some(pipe()?)
    } else {
        None
    };
    if let Some((_, writer)) = &input {
        not_inherited(writer)?;
    }
    let output = if capture { Some(pipe()?) } else { None };
    if let Some((reader, _)) = &output {
        not_inherited(reader)?;
    }
    let stdin = input.as_ref().map_or(null.0, |(r, _)| r.0);
    let stdout = output.as_ref().map_or(null.0, |(_, w)| w.0);
    let mut handles = vec![null.0];
    if stdin != null.0 {
        handles.push(stdin);
    }
    if stdout != null.0 {
        handles.push(stdout);
    }
    let attributes = Attributes::new(&handles)?;
    let mut startup = STARTUPINFOEXW {
        lpAttributeList: attributes.list,
        ..Default::default()
    };
    startup.StartupInfo.cb = mem::size_of::<STARTUPINFOEXW>() as u32;
    startup.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
    startup.StartupInfo.hStdInput = stdin;
    startup.StartupInfo.hStdOutput = stdout;
    startup.StartupInfo.hStdError = null.0;
    let mut information = PROCESS_INFORMATION::default();
    let mut flags = CREATE_SUSPENDED | CREATE_NO_WINDOW | EXTENDED_STARTUPINFO_PRESENT;
    if detached {
        flags |= CREATE_BREAKAWAY_FROM_JOB;
    }
    // SAFETY: explicit absolute application, mutable terminated argv, live allow-list and stdio, exact extended struct.
    unsafe {
        CreateProcessW(
            PCWSTR(application.as_ptr()),
            Some(PWSTR(command.as_mut_ptr())),
            None,
            None,
            true,
            flags,
            None,
            PCWSTR::null(),
            &startup.StartupInfo,
            &mut information,
        )
        .map_err(fail)?;
    }
    let mut process = Process {
        handle: Owned(information.hProcess),
        pid: information.dwProcessId,
        output: None,
    };
    let thread = Owned(information.hThread);
    let establish = || -> io::Result<()> {
        let mut in_job = BOOL(0);
        // SAFETY: child is still suspended; no child user code has run before verified assignment/separation.
        unsafe {
            if let Some(job) = job {
                AssignProcessToJobObject(job.handle.0, process.handle.0).map_err(fail)?;
                IsProcessInJob(process.handle.0, Some(job.handle.0), &mut in_job).map_err(fail)?;
                if !in_job.as_bool() {
                    return Err(io::Error::other("worker job assignment unverified"));
                }
            } else if detached {
                IsProcessInJob(
                    process.handle.0,
                    separate_from.map(|ui| ui.handle.0),
                    &mut in_job,
                )
                .map_err(fail)?;
                if in_job.as_bool() {
                    return Err(io::Error::new(
                        io::ErrorKind::PermissionDenied,
                        "Node remains inside a UI or ancestor job",
                    ));
                }
            }
            if ResumeThread(thread.0) == u32::MAX {
                return Err(io::Error::last_os_error());
            }
        }
        Ok(())
    };
    if let Err(error) = establish() {
        process.terminate()?;
        return Err(error);
    }
    drop(thread);
    drop(attributes);
    if let Some((reader, writer)) = input {
        drop(reader);
        // Exactly 32 bytes fits the previously empty 4KiB bootstrap pipe without waiting for a reader.
        if let Err(error) =
            into_file(writer).write_all(secret.expect("input exists only with secret").as_bytes())
        {
            process.terminate()?;
            return Err(error);
        }
    }
    if let Some((reader, writer)) = output {
        drop(writer);
        process.output = Some(into_file(reader));
    }
    Ok(process)
}

#[cfg(test)]
mod native_error_tests {
    use super::*;

    #[test]
    fn structured_native_error_keeps_hresult_and_operation_without_win32_alias() {
        let code = 0x8007_0005_u32 as i32;
        for operation in [
            ProcessOperation::TerminateJob,
            ProcessOperation::QueryJobActive,
            ProcessOperation::QueryJobIds,
            ProcessOperation::GetProcessExitCode,
        ] {
            let error = fail_operation(
                operation,
                windows::core::Error::from_hresult(windows::core::HRESULT(code)),
            );
            assert_eq!(
                process_native_failure(&error),
                Some(ProcessNativeFailure {
                    operation,
                    hresult: code
                })
            );
            assert_eq!(error.raw_os_error(), None);
            assert_eq!(
                error.to_string(),
                "Windows process operation failed (0x80070005)"
            );
        }
        let raw = io::Error::from_raw_os_error(5);
        assert_eq!(raw.raw_os_error(), Some(5));
        assert_eq!(process_native_failure(&raw), None);
        assert_eq!(
            process_native_failure(&io::Error::other(
                "Windows process operation failed (0x80070005)"
            )),
            None
        );
        let unknown = fail(windows::core::Error::from_hresult(windows::core::HRESULT(
            0x8000_4005_u32 as i32,
        )));
        assert_eq!(
            process_native_failure(&unknown),
            Some(ProcessNativeFailure {
                operation: ProcessOperation::Unknown,
                hresult: 0x8000_4005_u32 as i32
            })
        );
    }
}
