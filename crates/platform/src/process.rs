//! Trusted in-process launch API. Never accepts executable paths from control IPC.
use crate::Secret;
use std::{
    cell::RefCell,
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
            CloseHandle, DUPLICATE_SAME_ACCESS, DuplicateHandle, ERROR_ALREADY_EXISTS, FILETIME,
            GENERIC_READ, GENERIC_WRITE, GetLastError, HANDLE, HANDLE_FLAG_INHERIT,
            SetHandleInformation, WAIT_OBJECT_0, WAIT_TIMEOUT,
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
    VerifyJobMember,
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
    fn duplicate(&self) -> io::Result<Self> {
        let mut handle = HANDLE::default();
        // SAFETY: both pseudo handles identify this process. Duplicate only the
        // retained kernel object, never reopen its PID; the new handle is private.
        unsafe {
            DuplicateHandle(
                GetCurrentProcess(),
                self.handle.0,
                GetCurrentProcess(),
                &mut handle,
                0,
                false,
                DUPLICATE_SAME_ACCESS,
            )
            .map_err(fail)?;
        }
        Ok(Self {
            handle: Owned(handle),
            pid: self.pid,
            output: None,
        })
    }
    fn identity(&self) -> io::Result<MemberIdentity> {
        let (mut created, mut exited, mut kernel, mut user) = (
            FILETIME::default(),
            FILETIME::default(),
            FILETIME::default(),
            FILETIME::default(),
        );
        // SAFETY: query the retained object with four live scalar destinations.
        unsafe {
            GetProcessTimes(
                self.handle.0,
                &mut created,
                &mut exited,
                &mut kernel,
                &mut user,
            )
            .map_err(fail)?;
        }
        Ok(MemberIdentity {
            pid: self.pid,
            created: (u64::from(created.dwHighDateTime) << 32) | u64::from(created.dwLowDateTime),
        })
    }
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
    members: RefCell<MemberLedger>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct MemberIdentity {
    pid: u32,
    created: u64,
}
struct RetainedMember {
    identity: MemberIdentity,
    process: Process,
}
struct MemberLedger {
    handles: Vec<RetainedMember>,
    total: u32,
    first_error: Option<io::Error>,
    tracking: bool,
}
impl Default for MemberLedger {
    fn default() -> Self {
        Self {
            handles: Vec::with_capacity(8),
            total: 0,
            first_error: None,
            tracking: false,
        }
    }
}
impl MemberLedger {
    fn freeze_error(&mut self, error: io::Error) -> io::Error {
        repeat_member_error(self.first_error.get_or_insert(error))
    }
}
fn repeat_member_error(error: &io::Error) -> io::Error {
    if let Some(code) = error.raw_os_error() {
        return io::Error::from_raw_os_error(code);
    }
    if let Some(native) = process_native_failure(error) {
        return io::Error::other(NativeProcessError(native));
    }
    io::Error::new(error.kind(), "job lifetime member evidence unknown")
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
            members: RefCell::new(MemberLedger::default()),
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
        Ok(Self {
            handle,
            members: RefCell::new(MemberLedger::default()),
        })
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
        Ok(self.accounting()?.ActiveProcesses)
    }
    fn accounting(&self) -> io::Result<JOBOBJECT_BASIC_ACCOUNTING_INFORMATION> {
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
        Ok(info)
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
        self.terminate_until(Instant::now() + Duration::from_secs(3))
    }
    /// Share an owner's absolute cleanup deadline with later retained-handle waits.
    /// This blocking operation belongs exclusively on a non-realtime control task.
    pub fn terminate_until(&self, deadline: Instant) -> io::Result<()> {
        // SAFETY: private owner job, includes descendants (worker job disallows breakaway).
        unsafe {
            TerminateJobObject(self.handle.0, 1)
                .map_err(|error| fail_operation(ProcessOperation::TerminateJob, error))?;
        }
        while self.active_processes()? != 0 {
            if Instant::now() >= deadline {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "job cleanup deadline",
                ));
            }
            std::thread::sleep(
                Duration::from_millis(5).min(deadline.saturating_duration_since(Instant::now())),
            );
        }
        Ok(())
    }
    fn verify_member(&self, process: &Process) -> io::Result<()> {
        let mut in_job = BOOL(0);
        // SAFETY: live retained process and exact private owner Job handles.
        unsafe {
            IsProcessInJob(process.handle.0, Some(self.handle.0), &mut in_job)
                .map_err(|error| fail_operation(ProcessOperation::VerifyJobMember, error))?;
        }
        require_owned_member(in_job.as_bool())
    }
    /// Opt in on a fresh worker Job before its first spawn. Generic/UI Job launch
    /// and termination retain their existing behavior and do not acquire a ledger.
    pub fn track_lifetime_members(&self) -> io::Result<()> {
        let mut ledger = self.members.borrow_mut();
        if ledger.tracking || !ledger.handles.is_empty() || self.accounting()?.TotalProcesses != 0 {
            return Err(io::Error::other("lifetime tracking requires a fresh job"));
        }
        ledger.tracking = true;
        Ok(())
    }
    fn remember_spawned(&self, process: &Process) -> io::Result<()> {
        let mut ledger = self.members.borrow_mut();
        if !ledger.tracking {
            return Ok(());
        }
        if let Some(error) = &ledger.first_error {
            return Err(repeat_member_error(error));
        }
        let result = (|| {
            self.verify_member(process)?;
            let identity = process.identity()?;
            let known: Vec<_> = ledger
                .handles
                .iter()
                .map(|member| member.identity)
                .collect();
            validate_lifetime_members(&known, &[identity], known.len() as u32 + 1)?;
            let retained = process.duplicate()?;
            ledger.handles.push(RetainedMember {
                identity,
                process: retained,
            });
            Ok(())
        })();
        if let Err(error) = result {
            return Err(ledger.freeze_error(error));
        }
        Ok(())
    }
    /// Retain at most eight lifetime members. Historical handles remain owned after
    /// exit. New objects are opened once and verified; a reused PID is rejected.
    /// Missed births/exits and unstable snapshots freeze a failure, never renew proof.
    pub fn retain_lifetime_members(&self) -> io::Result<()> {
        let mut ledger = self.members.borrow_mut();
        if !ledger.tracking {
            return Err(io::Error::other("worker lifetime tracking not enabled"));
        }
        if let Some(error) = &ledger.first_error {
            return Err(repeat_member_error(error));
        }
        let result = (|| {
            let before = self.accounting()?;
            let ids = self.process_ids()?;
            let mut added = Vec::with_capacity(8);
            let mut active = Vec::with_capacity(8);
            for pid in &ids {
                if let Some(member) = ledger
                    .handles
                    .iter()
                    .find(|member| member.identity.pid == *pid)
                {
                    // A PID listed active must still name the same retained live
                    // object. Never reopen a historical PID to repair missing proof.
                    if member.process.wait(Duration::ZERO)?.is_some() {
                        return Err(io::Error::other(
                            "historical PID is active again or snapshot changed",
                        ));
                    }
                    active.push(member.identity);
                } else {
                    require_member_slot(ledger.handles.len(), added.len())?;
                    let process = Process::open_known(*pid, false)?;
                    self.verify_member(&process)?;
                    let identity = process.identity()?;
                    active.push(identity);
                    added.push(RetainedMember { identity, process });
                }
            }
            let after = self.accounting()?;
            let after_ids = self.process_ids()?;
            validate_member_snapshot(
                [before.TotalProcesses, after.TotalProcesses],
                [before.ActiveProcesses, after.ActiveProcesses],
                &ids,
                &after_ids,
            )?;
            let known: Vec<_> = ledger
                .handles
                .iter()
                .map(|member| member.identity)
                .collect();
            validate_lifetime_members(&known, &active, before.TotalProcesses)?;
            ledger.handles.extend(added);
            ledger.total = before.TotalProcesses;
            Ok(())
        })();
        if let Err(error) = result {
            return Err(ledger.freeze_error(error));
        }
        Ok(())
    }
    /// Duplicate only verified lifetime handles, including naturally exited members.
    /// No PID reopening; failed or missing history refuses the entire result.
    pub fn retained_member_handles(&self) -> io::Result<Vec<Process>> {
        self.retain_lifetime_members()?;
        self.members
            .borrow()
            .handles
            .iter()
            .map(|member| member.process.duplicate())
            .collect()
    }
    /// Strict worker cleanup: every lifetime member must be covered by a verified
    /// retained handle. Missing history or a new member means unknown, never clean.
    /// Frozen ledger failure still triggers termination; all waits share caller deadline.
    pub fn terminate_all_members_until(&self, deadline: Instant) -> io::Result<()> {
        let captured = self.retain_lifetime_members();
        self.terminate_until(deadline)?;
        captured?;
        let ledger = self.members.borrow();
        let after = self.accounting()?;
        require_complete_members(ledger.total, ledger.handles.len(), after.TotalProcesses)?;
        for member in &ledger.handles {
            let remaining = member_wait_budget(deadline, Instant::now())?;
            if member.process.wait(remaining)?.is_none() {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "member cleanup deadline",
                ));
            }
        }
        Ok(())
    }
}

fn require_member_slot(retained: usize, staged: usize) -> io::Result<()> {
    if retained.checked_add(staged).is_none_or(|count| count >= 8) {
        return Err(io::Error::other("job lifetime member limit"));
    }
    Ok(())
}
fn validate_lifetime_members(
    known: &[MemberIdentity],
    active: &[MemberIdentity],
    total: u32,
) -> io::Result<()> {
    for group in [known, active] {
        if group.len() > 8
            || group.iter().any(|item| item.pid == 0 || item.created == 0)
            || group.iter().enumerate().any(|(index, item)| {
                group[..index]
                    .iter()
                    .any(|previous| previous.pid == item.pid)
            })
        {
            return Err(io::Error::other("bounded member identities invalid"));
        }
    }
    let mut combined = known.to_vec();
    for item in active {
        if let Some(previous) = combined.iter().find(|previous| previous.pid == item.pid) {
            if previous != item {
                return Err(io::Error::other("job member PID identity changed"));
            }
        } else {
            if combined.len() == 8 {
                return Err(io::Error::other("job lifetime member limit"));
            }
            combined.push(*item);
        }
    }
    require_complete_members(total, combined.len(), total)
}

fn require_owned_member(verified: bool) -> io::Result<()> {
    if !verified {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "job member not verified",
        ));
    }
    Ok(())
}
fn validate_member_snapshot(
    total: [u32; 2],
    active: [u32; 2],
    before: &[u32],
    after: &[u32],
) -> io::Result<()> {
    if before.len() > 8
        || after.len() > 8
        || total[0] != total[1]
        || active[0] as usize != before.len()
        || active[1] as usize != after.len()
    {
        return Err(io::Error::other(
            "job membership changed during bounded capture",
        ));
    }
    let mut first = before.to_vec();
    let mut second = after.to_vec();
    first.sort_unstable();
    second.sort_unstable();
    if first != second || first.contains(&0) || first.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err(io::Error::other("job member identity set not verified"));
    }
    Ok(())
}
fn require_complete_members(created: u32, captured: usize, after_created: u32) -> io::Result<()> {
    if captured > 8 || created as usize != captured || created != after_created {
        return Err(io::Error::other("job lifetime member coverage unknown"));
    }
    Ok(())
}
fn member_wait_budget(deadline: Instant, now: Instant) -> io::Result<Duration> {
    deadline
        .checked_duration_since(now)
        .filter(|left| !left.is_zero())
        .ok_or_else(|| io::Error::new(io::ErrorKind::TimedOut, "member cleanup deadline"))
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
                // Retain the very process object before any child user code can
                // exit. Natural controller exit must not erase lifetime evidence.
                job.remember_spawned(&process)?;
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
mod member_snapshot_tests {
    use super::*;
    fn identity(pid: u32) -> MemberIdentity {
        MemberIdentity {
            pid,
            created: u64::from(pid) * 10,
        }
    }
    #[test]
    fn lifetime_retains_exact_historical_identities_after_natural_exit() {
        let retained = [identity(1), identity(2), identity(3)];
        validate_lifetime_members(&retained, &[identity(2)], 3).unwrap();
        validate_lifetime_members(&retained, &[], 3).unwrap();
        // A subsequently observed member extends the lifetime evidence once.
        validate_lifetime_members(&retained, &[identity(2), identity(4)], 4).unwrap();
    }
    #[test]
    fn lifetime_pid_match_never_substitutes_for_object_identity() {
        let retained = [identity(1)];
        let recycled = MemberIdentity {
            pid: 1,
            created: 999,
        };
        assert!(validate_lifetime_members(&retained, &[recycled], 2).is_err());
        assert!(validate_lifetime_members(&retained, &[recycled], 1).is_err());
        assert!(validate_lifetime_members(&[identity(1), recycled], &[], 2).is_err());
    }
    #[test]
    fn lifetime_duplicate_zero_and_capacity_are_rejected() {
        require_member_slot(7, 0).unwrap();
        assert!(require_member_slot(8, 0).is_err());
        assert!(require_member_slot(7, 1).is_err());
        assert!(require_member_slot(usize::MAX, 1).is_err());
        let full: Vec<_> = (1..=8).map(identity).collect();
        validate_lifetime_members(&full, &full, 8).unwrap();
        assert!(validate_lifetime_members(&full, &[identity(9)], 9).is_err());
        assert!(validate_lifetime_members(&[], &[identity(1), identity(1)], 2).is_err());
        assert!(validate_lifetime_members(&[identity(0)], &[], 1).is_err());
        assert!(
            validate_lifetime_members(&[MemberIdentity { pid: 1, created: 0 }], &[], 1).is_err()
        );
        let over: Vec<_> = (1..=9).map(identity).collect();
        assert!(validate_lifetime_members(&over, &[], 9).is_err());
    }
    #[test]
    fn lifetime_missing_or_new_history_never_becomes_complete_proof() {
        let retained = [identity(1), identity(2)];
        assert!(validate_lifetime_members(&retained, &[], 3).is_err());
        assert!(validate_lifetime_members(&retained, &[identity(3)], 4).is_err());
        assert!(validate_lifetime_members(&retained, &[], 1).is_err());
        require_complete_members(2, retained.len(), 2).unwrap();
        assert!(require_complete_members(2, retained.len(), 3).is_err());
    }
    #[test]
    fn lifetime_error_replay_preserves_native_and_win32_domains() {
        let native = fail_operation(
            ProcessOperation::VerifyJobMember,
            windows::core::Error::from_hresult(windows::core::HRESULT(0x80070005u32 as i32)),
        );
        let repeated = repeat_member_error(&native);
        assert_eq!(
            process_native_failure(&repeated),
            process_native_failure(&native)
        );
        assert_eq!(repeated.raw_os_error(), None);
        let win32 = repeat_member_error(&io::Error::from_raw_os_error(5));
        assert_eq!(win32.raw_os_error(), Some(5));
        assert_eq!(process_native_failure(&win32), None);
        let unknown = repeat_member_error(&io::Error::new(io::ErrorKind::TimedOut, "fixed"));
        assert_eq!(unknown.kind(), io::ErrorKind::TimedOut);
        assert_eq!(unknown.raw_os_error(), None);
        assert_eq!(process_native_failure(&unknown), None);
    }
    #[test]
    fn lifetime_first_missing_proof_is_immutable() {
        let mut ledger = MemberLedger::default();
        let first = ledger.freeze_error(io::Error::from_raw_os_error(5));
        let later = ledger.freeze_error(io::Error::new(io::ErrorKind::TimedOut, "later"));
        assert_eq!(first.raw_os_error(), Some(5));
        assert_eq!(later.raw_os_error(), Some(5));
        assert!(ledger.handles.is_empty());
        assert_eq!(ledger.total, 0);
    }
    #[test]
    fn exact_membership_and_stable_bounded_identity_are_required() {
        assert_eq!(
            require_owned_member(false).unwrap_err().kind(),
            io::ErrorKind::PermissionDenied
        );
        require_owned_member(true).unwrap();
        validate_member_snapshot([4, 4], [4, 4], &[1, 2, 3, 4], &[4, 3, 2, 1]).unwrap();
        for (total, active, before, after) in [
            ([4, 5], [4, 4], vec![1, 2, 3, 4], vec![1, 2, 3, 4]),
            ([4, 4], [4, 4], vec![1, 2, 3, 4], vec![1, 2, 3, 5]),
            ([4, 4], [4, 3], vec![1, 2, 3, 4], vec![1, 2, 3, 4]),
            ([4, 4], [4, 4], vec![1, 2, 3, 3], vec![1, 2, 3, 3]),
            ([4, 4], [4, 4], vec![0, 2, 3, 4], vec![0, 2, 3, 4]),
            ([9, 9], [9, 9], (1..=9).collect(), (1..=9).collect()),
        ] {
            assert!(validate_member_snapshot(total, active, &before, &after).is_err());
        }
    }
    #[test]
    fn historical_or_new_members_never_become_complete_coverage() {
        require_complete_members(4, 4, 4).unwrap();
        assert!(require_complete_members(4, 3, 4).is_err());
        assert!(require_complete_members(4, 4, 5).is_err());
        assert!(require_complete_members(9, 9, 9).is_err());
    }
    #[test]
    fn member_waits_share_remaining_deadline_without_renewal() {
        let start = Instant::now();
        let deadline = start + Duration::from_secs(3);
        assert_eq!(
            member_wait_budget(deadline, start + Duration::from_millis(1200)).unwrap(),
            Duration::from_millis(1800)
        );
        assert_eq!(
            member_wait_budget(deadline, start + Duration::from_millis(2950)).unwrap(),
            Duration::from_millis(50)
        );
        for now in [deadline, deadline + Duration::from_millis(1)] {
            assert_eq!(
                member_wait_budget(deadline, now).unwrap_err().kind(),
                io::ErrorKind::TimedOut
            );
        }
    }
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
            ProcessOperation::VerifyJobMember,
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
