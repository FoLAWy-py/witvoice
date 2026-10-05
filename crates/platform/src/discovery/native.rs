use super::*;
use std::{
    cell::UnsafeCell,
    collections::VecDeque,
    ffi::c_void,
    net::{Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV6},
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};
use windows::{
    Win32::NetworkManagement::Dns::*,
    core::{PCWSTR, PWSTR},
};
const PENDING: u32 = windows::Win32::Foundation::DNS_REQUEST_PENDING as u32;
const CANCELLED: u32 = windows::Win32::Foundation::ERROR_CANCELLED.0;
const INVALID: u32 = 13;
const RECORD_LIMIT: usize = 64;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Browse,
    Resolve,
    Register,
}
#[derive(Clone, Copy)]
struct Api {
    browse: unsafe fn(*const DNS_SERVICE_BROWSE_REQUEST, *mut DNS_SERVICE_CANCEL) -> u32,
    resolve: unsafe fn(*const DNS_SERVICE_RESOLVE_REQUEST, *mut DNS_SERVICE_CANCEL) -> u32,
    register: unsafe fn(*const DNS_SERVICE_REGISTER_REQUEST, *mut DNS_SERVICE_CANCEL) -> u32,
    deregister: unsafe fn(*const DNS_SERVICE_REGISTER_REQUEST) -> u32,
    cancel_browse: unsafe fn(*const DNS_SERVICE_CANCEL) -> u32,
    cancel_resolve: unsafe fn(*const DNS_SERVICE_CANCEL) -> u32,
    cancel_register: unsafe fn(*const DNS_SERVICE_CANCEL) -> u32,
    free_records: unsafe fn(*const DNS_RECORDW),
    free_instance: unsafe fn(*const DNS_SERVICE_INSTANCE),
}
unsafe fn browse(r: *const DNS_SERVICE_BROWSE_REQUEST, c: *mut DNS_SERVICE_CANCEL) -> u32 {
    unsafe { DnsServiceBrowse(r, c) as u32 }
}
unsafe fn resolve(r: *const DNS_SERVICE_RESOLVE_REQUEST, c: *mut DNS_SERVICE_CANCEL) -> u32 {
    unsafe { DnsServiceResolve(r, c) as u32 }
}
unsafe fn register(r: *const DNS_SERVICE_REGISTER_REQUEST, c: *mut DNS_SERVICE_CANCEL) -> u32 {
    unsafe { DnsServiceRegister(r, Some(c)) }
}
unsafe fn deregister(r: *const DNS_SERVICE_REGISTER_REQUEST) -> u32 {
    unsafe { DnsServiceDeRegister(r, None) }
}
unsafe fn cancel_browse(c: *const DNS_SERVICE_CANCEL) -> u32 {
    unsafe { DnsServiceBrowseCancel(c) as u32 }
}
unsafe fn cancel_resolve(c: *const DNS_SERVICE_CANCEL) -> u32 {
    unsafe { DnsServiceResolveCancel(c) as u32 }
}
unsafe fn cancel_register(c: *const DNS_SERVICE_CANCEL) -> u32 {
    unsafe { DnsServiceRegisterCancel(c) }
}
unsafe fn free_records(r: *const DNS_RECORDW) {
    if !r.is_null() {
        unsafe {
            DnsFree(Some(r.cast()), DnsFreeRecordList);
        }
    }
}
unsafe fn free_instance(r: *const DNS_SERVICE_INSTANCE) {
    if !r.is_null() {
        unsafe {
            DnsServiceFreeInstance(r.cast_mut());
        }
    }
}
const OS: Api = Api {
    browse,
    resolve,
    register,
    deregister,
    cancel_browse,
    cancel_resolve,
    cancel_register,
    free_records,
    free_instance,
};

struct Queue {
    items: Mutex<VecDeque<Event>>,
    dropped: AtomicU64,
}
impl Default for Queue {
    fn default() -> Self {
        Self {
            items: Mutex::new(VecDeque::with_capacity(QUEUE_LIMIT)),
            dropped: AtomicU64::new(0),
        }
    }
}
impl Queue {
    fn push(&self, event: Event) {
        if let Ok(mut items) = self.items.try_lock()
            && items.len() < QUEUE_LIMIT
        {
            items.push_back(event);
            return;
        }
        let _ = self
            .dropped
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |n| {
                Some(n.saturating_add(1))
            });
    }
}
// Buffers live in the same pinned Arc as request/cancel structures, never move after publish.
struct Storage {
    query: Vec<u16>,
    host: Vec<u16>,
    keys_text: [Vec<u16>; 2],
    values_text: [Vec<u16>; 2],
    keys: [PWSTR; 2],
    values: [PWSTR; 2],
    ip4: u32,
    instance: DNS_SERVICE_INSTANCE,
    browse: DNS_SERVICE_BROWSE_REQUEST,
    resolve: DNS_SERVICE_RESOLVE_REQUEST,
    register: DNS_SERVICE_REGISTER_REQUEST,
    cancel: DNS_SERVICE_CANCEL,
}
struct Operation {
    token: usize,
    kind: Kind,
    interface: u32,
    queue: Arc<Queue>,
    api: Api,
    storage: UnsafeCell<Storage>,
    terminal: AtomicBool,
    registered: AtomicBool,
    closing: AtomicBool,
    deregistering: AtomicBool,
    failure: AtomicU32,
    active_calls: AtomicUsize,
    expires: Instant,
    deadline: Instant,
}
// SAFETY: storage is initialized before publish and remains at a stable heap address.
// Request buffers are thereafter immutable; native calls/cancellation are serialized by
// the single non-Clone adapter, and OS owns the cancel handle's internal writes.
unsafe impl Send for Operation {}
unsafe impl Sync for Operation {}
static POOL: OnceLock<Mutex<Vec<Arc<Operation>>>> = OnceLock::new();
static NEXT: AtomicUsize = AtomicUsize::new(1);
fn pool() -> &'static Mutex<Vec<Arc<Operation>>> {
    POOL.get_or_init(|| Mutex::new(Vec::with_capacity(CONTEXT_LIMIT)))
}
fn reap(entries: &mut Vec<Arc<Operation>>) {
    entries.retain(|op| {
        !(op.terminal.load(Ordering::Acquire)
            && op.active_calls.load(Ordering::Acquire) == 0
            && Arc::strong_count(op) == 1)
    });
}
fn callback_operation(context: *const c_void) -> Option<Arc<Operation>> {
    // Opaque token, never dereferenced: a late callback cannot access freed Rust memory.
    pool()
        .lock()
        .ok()?
        .iter()
        .find(|op| op.token == context as usize)
        .cloned()
}
struct Records<'a>(*const DNS_RECORDW, &'a Api);
impl Drop for Records<'_> {
    fn drop(&mut self) {
        unsafe {
            (self.1.free_records)(self.0);
        }
    }
}
struct Instance<'a>(*const DNS_SERVICE_INSTANCE, &'a Api);
impl Drop for Instance<'_> {
    fn drop(&mut self) {
        unsafe {
            (self.1.free_instance)(self.0);
        }
    }
}
unsafe fn text(pointer: *const u16, cap: usize) -> Option<String> {
    if pointer.is_null() {
        return None;
    }
    let mut count = 0;
    // SAFETY: WinDNS owns a valid terminated UTF16 allocation. Scan/copy at most cap+1.
    while count <= cap && unsafe { *pointer.add(count) } != 0 {
        count += 1;
    }
    if count > cap {
        return None;
    }
    String::from_utf16(unsafe { std::slice::from_raw_parts(pointer, count) }).ok()
}
unsafe fn browse_records(
    op: &Operation,
    mut record: *const DNS_RECORDW,
) -> Result<(), DiscoveryError> {
    let mut visited = [0usize; RECORD_LIMIT];
    for index in 0..RECORD_LIMIT {
        if record.is_null() {
            return Ok(());
        }
        if visited[..index].contains(&(record as usize)) {
            return Err(DiscoveryError::InvalidInput);
        }
        visited[index] = record as usize;
        let value = unsafe { &*record };
        if value.wType == DNS_TYPE_PTR.0 {
            if value.wDataLength < std::mem::size_of::<DNS_PTR_DATAW>() as u16 {
                return Err(DiscoveryError::InvalidInput);
            }
            let name = unsafe { text(value.pName.0, 128) }.ok_or(DiscoveryError::InvalidInput)?;
            if name.trim_end_matches('.') != SERVICE_TYPE {
                return Err(DiscoveryError::InvalidInput);
            }
            let full = unsafe { text(value.Data.PTR.pNameHost.0, 128) }
                .ok_or(DiscoveryError::InvalidInput)?;
            validate_full_name(&full)?;
            if value.dwTtl == 0 {
                op.queue.push(Event::Removed { full_name: full });
            } else {
                op.queue.push(Event::Found {
                    full_name: full,
                    expires_at: Instant::now() + Duration::from_secs(value.dwTtl.min(120) as u64),
                });
            }
        }
        record = value.pNext;
    }
    if record.is_null() {
        Ok(())
    } else {
        Err(DiscoveryError::Capacity)
    }
}
unsafe fn browse_dispatch(
    status: u32,
    context: *const c_void,
    records: *const DNS_RECORDW,
    fallback: &Api,
) {
    let operation = callback_operation(context);
    let api = operation.as_ref().map(|op| &op.api).unwrap_or(fallback);
    let _records = Records(records, api);
    let Some(op) = operation.as_ref() else {
        return;
    };
    if status == CANCELLED {
        op.terminal.store(true, Ordering::Release);
        op.queue.push(Event::Stopped);
        return;
    }
    if status != 0 {
        op.queue.push(Event::Failed(status));
        return;
    }
    if op.closing.load(Ordering::Acquire) {
        return;
    }
    if unsafe { browse_records(op, records) }.is_err() {
        op.queue.push(Event::Failed(INVALID));
    }
}
unsafe fn resolved(
    op: &Operation,
    pointer: *const DNS_SERVICE_INSTANCE,
) -> Result<Event, DiscoveryError> {
    if pointer.is_null() {
        return Err(DiscoveryError::InvalidInput);
    }
    let instance = unsafe { &*pointer };
    if instance.dwInterfaceIndex != op.interface
        || instance.dwPropertyCount != 2
        || instance.keys.is_null()
        || instance.values.is_null()
        || instance.wPort == 0
    {
        return Err(DiscoveryError::InvalidInput);
    }
    let name =
        unsafe { text(instance.pszInstanceName.0, 128) }.ok_or(DiscoveryError::InvalidInput)?;
    let label = validate_full_name(&name)?.to_owned();
    let expected = unsafe { text((*op.storage.get()).resolve.QueryName.0, 128) }
        .ok_or(DiscoveryError::InvalidInput)?;
    if validate_full_name(&expected)? != label {
        return Err(DiscoveryError::InvalidInput);
    }
    let mut node = None;
    let mut protocol = None;
    for index in 0..2 {
        let key = unsafe { text((*instance.keys.add(index)).0, 32) }
            .ok_or(DiscoveryError::InvalidInput)?;
        let value = unsafe { text((*instance.values.add(index)).0, 64) }
            .ok_or(DiscoveryError::InvalidInput)?;
        match key.as_str() {
            "node_id" if node.is_none() => node = Some(value),
            "protocol" if protocol.is_none() => protocol = Some(value),
            _ => return Err(DiscoveryError::InvalidInput),
        }
    }
    if protocol.as_deref() != Some("1") {
        return Err(DiscoveryError::InvalidInput);
    }
    let node_id = node
        .ok_or(DiscoveryError::InvalidInput)?
        .try_into()
        .map_err(|_| DiscoveryError::InvalidInput)?;
    let address = if !instance.ip4Address.is_null() {
        SocketAddr::from((
            Ipv4Addr::from(unsafe { *instance.ip4Address }.to_ne_bytes()),
            instance.wPort,
        ))
    } else if !instance.ip6Address.is_null() {
        let ip = Ipv6Addr::from(unsafe { (*instance.ip6Address).IP6Byte });
        let scope = if ip.segments()[0] & 0xffc0 == 0xfe80 {
            op.interface
        } else {
            0
        };
        SocketAddr::V6(SocketAddrV6::new(ip, instance.wPort, 0, scope))
    } else {
        return Err(DiscoveryError::InvalidInput);
    };
    if op.expires <= Instant::now() {
        return Err(DiscoveryError::InvalidInput);
    }
    Ok(Event::Resolved {
        instance: label,
        node_id,
        address,
        expires_at: op.expires,
    })
}
unsafe fn resolve_dispatch(
    status: u32,
    context: *const c_void,
    instance: *const DNS_SERVICE_INSTANCE,
    fallback: &Api,
) {
    let operation = callback_operation(context);
    let api = operation.as_ref().map(|op| &op.api).unwrap_or(fallback);
    let _instance = Instance(instance, api);
    if let Some(op) = operation.as_ref() {
        if !op.closing.load(Ordering::Acquire) {
            if status == 0 {
                match unsafe { resolved(op, instance) } {
                    Ok(event) => op.queue.push(event),
                    Err(_) => op.queue.push(Event::Failed(INVALID)),
                }
            } else {
                op.queue.push(Event::Failed(status));
            }
        }
        op.terminal.store(true, Ordering::Release);
    }
}
unsafe fn register_dispatch(
    status: u32,
    context: *const c_void,
    instance: *const DNS_SERVICE_INSTANCE,
    fallback: &Api,
) {
    let operation = callback_operation(context);
    let api = operation.as_ref().map(|op| &op.api).unwrap_or(fallback);
    let _instance = Instance(instance, api);
    if let Some(op) = operation.as_ref() {
        if op.deregistering.load(Ordering::Acquire) {
            if status == 0 {
                op.terminal.store(true, Ordering::Release);
                op.queue.push(Event::Stopped);
            } else {
                op.failure.store(status, Ordering::Release);
                op.queue.push(Event::Failed(status));
            }
        } else if status == 0 {
            op.registered.store(true, Ordering::Release);
            if op.closing.load(Ordering::Acquire) {
                let _ = NativeDiscovery::cancel(op);
            } else {
                op.queue.push(Event::Registered);
            }
        } else {
            op.terminal.store(true, Ordering::Release);
            op.queue.push(Event::Failed(status));
        }
    }
}
fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}
fn callback_guard(context: *const c_void, call: impl FnOnce()) {
    if std::panic::catch_unwind(std::panic::AssertUnwindSafe(call)).is_err()
        && let Some(op) = callback_operation(context)
    {
        op.failure.store(INVALID, Ordering::Release);
        op.queue.push(Event::Failed(INVALID));
    }
}
unsafe extern "system" fn browse_callback(
    status: u32,
    context: *const c_void,
    records: *const DNS_RECORDW,
) {
    callback_guard(context, || unsafe {
        browse_dispatch(status, context, records, &OS)
    });
}
unsafe extern "system" fn resolve_callback(
    status: u32,
    context: *const c_void,
    instance: *const DNS_SERVICE_INSTANCE,
) {
    callback_guard(context, || unsafe {
        resolve_dispatch(status, context, instance, &OS)
    });
}
unsafe extern "system" fn register_callback(
    status: u32,
    context: *const c_void,
    instance: *const DNS_SERVICE_INSTANCE,
) {
    callback_guard(context, || unsafe {
        register_dispatch(status, context, instance, &OS)
    });
}

pub struct NativeDiscovery {
    api: Api,
    queue: Arc<Queue>,
    operations: Vec<Arc<Operation>>,
    interface: Option<u32>,
}
impl Default for NativeDiscovery {
    fn default() -> Self {
        Self {
            api: OS,
            queue: Arc::new(Queue::default()),
            operations: Vec::new(),
            interface: None,
        }
    }
}
impl NativeDiscovery {
    fn authorize(&mut self, approved: bool, interface: u32) -> Result<(), DiscoveryError> {
        if !approved {
            return Err(DiscoveryError::ApprovalRequired);
        }
        if interface == 0 {
            return Err(DiscoveryError::InterfaceRequired);
        }
        if self.interface.is_some_and(|old| old != interface) {
            return Err(DiscoveryError::Busy);
        }
        self.interface = Some(interface);
        Ok(())
    }
    fn clean(&mut self) {
        self.operations.retain(|op| {
            !(op.terminal.load(Ordering::Acquire)
                && op.active_calls.load(Ordering::Acquire) == 0
                && Arc::strong_count(op) <= 2)
        });
        if let Ok(mut entries) = pool().lock() {
            reap(&mut entries);
        }
    }
    fn make(
        &mut self,
        kind: Kind,
        query: &str,
        expires: Instant,
    ) -> Result<Arc<Operation>, DiscoveryError> {
        self.clean();
        let mut entries = pool().lock().map_err(|_| DiscoveryError::Capacity)?;
        reap(&mut entries);
        if entries.len() >= CONTEXT_LIMIT {
            return Err(DiscoveryError::Capacity);
        }
        if entries
            .iter()
            .any(|op| !Arc::ptr_eq(&op.queue, &self.queue))
        {
            return Err(DiscoveryError::Busy);
        }
        let count = entries
            .iter()
            .filter(|op| op.kind == kind && !op.terminal.load(Ordering::Acquire))
            .count();
        if count
            >= if kind == Kind::Resolve {
                RESOLVE_LIMIT
            } else {
                1
            }
        {
            return Err(DiscoveryError::Capacity);
        }
        let token = NEXT
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
            .map_err(|_| DiscoveryError::Capacity)?;
        let interface = self.interface.ok_or(DiscoveryError::InterfaceRequired)?;
        let mut op = Arc::new(Operation {
            token,
            kind,
            interface,
            queue: self.queue.clone(),
            api: self.api,
            storage: UnsafeCell::new(Storage {
                query: wide(query),
                host: Vec::new(),
                keys_text: [Vec::new(), Vec::new()],
                values_text: [Vec::new(), Vec::new()],
                keys: [PWSTR::null(); 2],
                values: [PWSTR::null(); 2],
                ip4: 0,
                instance: DNS_SERVICE_INSTANCE::default(),
                browse: DNS_SERVICE_BROWSE_REQUEST::default(),
                resolve: DNS_SERVICE_RESOLVE_REQUEST::default(),
                register: DNS_SERVICE_REGISTER_REQUEST::default(),
                cancel: DNS_SERVICE_CANCEL::default(),
            }),
            terminal: AtomicBool::new(false),
            registered: AtomicBool::new(false),
            closing: AtomicBool::new(false),
            deregistering: AtomicBool::new(false),
            failure: AtomicU32::new(0),
            active_calls: AtomicUsize::new(0),
            expires,
            deadline: Instant::now() + Duration::from_secs(3),
        });
        let storage = Arc::get_mut(&mut op)
            .ok_or(DiscoveryError::Capacity)?
            .storage
            .get_mut();
        let context = token as *mut c_void;
        storage.browse = DNS_SERVICE_BROWSE_REQUEST {
            Version: 1,
            InterfaceIndex: interface,
            QueryName: PCWSTR(storage.query.as_ptr()),
            Anonymous: DNS_SERVICE_BROWSE_REQUEST_0 {
                pBrowseCallback: Some(browse_callback),
            },
            pQueryContext: context,
        };
        storage.resolve = DNS_SERVICE_RESOLVE_REQUEST {
            Version: 1,
            InterfaceIndex: interface,
            QueryName: PWSTR(storage.query.as_mut_ptr()),
            pResolveCompletionCallback: Some(resolve_callback),
            pQueryContext: context,
        };
        entries.push(op.clone());
        self.operations.push(op.clone());
        Ok(op)
    }
    fn start_result(op: &Operation, result: u32) -> Result<(), DiscoveryError> {
        op.active_calls.fetch_sub(1, Ordering::Release);
        if result != PENDING {
            op.terminal.store(true, Ordering::Release);
            return Err(DiscoveryError::Native(result));
        }
        Ok(())
    }
    pub fn browse(&mut self, approved: bool, interface: u32) -> Result<(), DiscoveryError> {
        self.authorize(approved, interface)?;
        let op = self.make(
            Kind::Browse,
            SERVICE_TYPE,
            Instant::now() + Duration::from_secs(120),
        )?;
        op.active_calls.fetch_add(1, Ordering::Acquire);
        let storage = op.storage.get();
        let result = unsafe { (op.api.browse)(&(*storage).browse, &mut (*storage).cancel) };
        Self::start_result(&op, result)
    }
    pub fn resolve(
        &mut self,
        approved: bool,
        interface: u32,
        name: &str,
        ttl: u32,
    ) -> Result<(), DiscoveryError> {
        if ttl == 0 || ttl > 120 {
            return Err(DiscoveryError::InvalidInput);
        }
        self.resolve_until(
            approved,
            interface,
            name,
            Instant::now() + Duration::from_secs(ttl as u64),
        )
    }
    /// Preserve the browse result's absolute monotonic expiry; queueing never refreshes TTL.
    pub fn resolve_until(
        &mut self,
        approved: bool,
        interface: u32,
        name: &str,
        expires: Instant,
    ) -> Result<(), DiscoveryError> {
        self.authorize(approved, interface)?;
        validate_full_name(name)?;
        let remaining = expires
            .checked_duration_since(Instant::now())
            .ok_or(DiscoveryError::InvalidInput)?;
        if remaining.is_zero() || remaining > Duration::from_secs(120) {
            return Err(DiscoveryError::InvalidInput);
        }
        let op = self.make(Kind::Resolve, name, expires)?;
        op.active_calls.fetch_add(1, Ordering::Acquire);
        let storage = op.storage.get();
        let result = unsafe { (op.api.resolve)(&(*storage).resolve, &mut (*storage).cancel) };
        Self::start_result(&op, result)
    }
    pub fn advertise(
        &mut self,
        approved: bool,
        interface: u32,
        node_id: &witvoice_contracts::values::Id,
        address: std::net::SocketAddrV4,
    ) -> Result<(), DiscoveryError> {
        self.authorize(approved, interface)?;
        if address.port() == 0 || !(address.ip().is_private() || address.ip().is_link_local()) {
            return Err(DiscoveryError::InvalidInput);
        }
        let id = String::from(node_id.clone());
        let full = format!("{id}.{SERVICE_TYPE}");
        let op = self.make(
            Kind::Register,
            &full,
            Instant::now() + Duration::from_secs(120),
        )?;
        // SAFETY: operation has not been handed to OS; sole caller sets stable buffer pointers.
        let storage = unsafe { &mut *op.storage.get() };
        storage.host = wide(&format!("{id}.local"));
        storage.keys_text = [wide("node_id"), wide("protocol")];
        storage.values_text = [wide(&id), wide("1")];
        for i in 0..2 {
            storage.keys[i] = PWSTR(storage.keys_text[i].as_mut_ptr());
            storage.values[i] = PWSTR(storage.values_text[i].as_mut_ptr());
        }
        storage.ip4 = u32::from_ne_bytes(address.ip().octets());
        storage.instance = DNS_SERVICE_INSTANCE {
            pszInstanceName: PWSTR(storage.query.as_mut_ptr()),
            pszHostName: PWSTR(storage.host.as_mut_ptr()),
            ip4Address: &mut storage.ip4,
            wPort: address.port(),
            dwPropertyCount: 2,
            keys: storage.keys.as_mut_ptr(),
            values: storage.values.as_mut_ptr(),
            dwInterfaceIndex: interface,
            ..Default::default()
        };
        storage.register = DNS_SERVICE_REGISTER_REQUEST {
            Version: 1,
            InterfaceIndex: interface,
            pServiceInstance: &mut storage.instance,
            pRegisterCompletionCallback: Some(register_callback),
            pQueryContext: op.token as *mut c_void,
            ..Default::default()
        };
        op.active_calls.fetch_add(1, Ordering::Acquire);
        let result = unsafe { (op.api.register)(&storage.register, &mut storage.cancel) };
        Self::start_result(&op, result)
    }
    fn cancel(op: &Operation) -> Result<(), DiscoveryError> {
        let failure = op.failure.load(Ordering::Acquire);
        if failure != 0 {
            return Err(DiscoveryError::Native(failure));
        }
        if op.terminal.load(Ordering::Acquire) {
            return Ok(());
        }
        let already_closing = op.closing.swap(true, Ordering::AcqRel);
        let deregister = op.kind == Kind::Register && op.registered.load(Ordering::Acquire);
        if deregister {
            if op.deregistering.swap(true, Ordering::AcqRel) {
                return Ok(());
            }
        } else if already_closing {
            return Ok(());
        }
        op.active_calls.fetch_add(1, Ordering::Acquire);
        let storage = op.storage.get();
        let result = unsafe {
            match op.kind {
                Kind::Browse => (op.api.cancel_browse)(&(*storage).cancel),
                Kind::Resolve => (op.api.cancel_resolve)(&(*storage).cancel),
                Kind::Register if deregister => (op.api.deregister)(&(*storage).register),
                Kind::Register => (op.api.cancel_register)(&(*storage).cancel),
            }
        };
        op.active_calls.fetch_sub(1, Ordering::Release);
        // Cancel return is NOT quiescence: retain op until actual terminal callback.
        if result != 0 && !(deregister && result == PENDING) {
            op.failure.store(result, Ordering::Release);
            return Err(DiscoveryError::Native(result));
        }
        Ok(())
    }
    pub fn close(&mut self) -> Result<bool, DiscoveryError> {
        let mut error = None;
        for op in &self.operations {
            if let Err(e) = Self::cancel(op) {
                error = Some(e);
            }
        }
        self.clean();
        if let Some(error) = error {
            Err(error)
        } else {
            Ok(self.operations.is_empty())
        }
    }
    pub fn poll(&mut self) -> Vec<Event> {
        for op in &self.operations {
            if op.kind == Kind::Resolve && Instant::now() >= op.deadline {
                let _ = Self::cancel(op);
            }
        }
        self.clean();
        self.queue
            .items
            .lock()
            .map(|mut items| {
                items
                    .drain(..)
                    .filter(|event| match event {
                        Event::Found { expires_at, .. } | Event::Resolved { expires_at, .. } => {
                            *expires_at > Instant::now()
                        }
                        _ => true,
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
    pub fn dropped_events(&self) -> u64 {
        self.queue.dropped.load(Ordering::Relaxed)
    }
    pub fn pending_contexts(&self) -> usize {
        self.operations.len()
    }
}
impl Drop for NativeDiscovery {
    fn drop(&mut self) {
        let _ = self.close();
    }
}

#[cfg(test)]
mod tests;
