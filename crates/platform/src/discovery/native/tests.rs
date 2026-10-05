use super::*;
static SERIAL:Mutex<()>=Mutex::new(());
static STARTS:AtomicUsize=AtomicUsize::new(0);
static FREED_RECORDS:AtomicUsize=AtomicUsize::new(0);
static FREED_INSTANCES:AtomicUsize=AtomicUsize::new(0);
static MODE:AtomicU32=AtomicU32::new(0);
static DEREGISTERS:AtomicUsize=AtomicUsize::new(0);
struct TestGuard(std::sync::MutexGuard<'static,()>);
fn guard()->TestGuard {
    let lock=SERIAL.lock().unwrap_or_else(|p|p.into_inner());
    assert!(pool().lock().unwrap().is_empty());
    for counter in [&STARTS,&FREED_RECORDS,&FREED_INSTANCES,&DEREGISTERS] {counter.store(0,Ordering::Relaxed);}
    MODE.store(0,Ordering::Relaxed);
    TestGuard(lock)
}
impl Drop for TestGuard {fn drop(&mut self) {let _=&self.0; pool().lock().unwrap_or_else(|p|p.into_inner()).clear();}}
unsafe fn fake_browse(r:*const DNS_SERVICE_BROWSE_REQUEST,c:*mut DNS_SERVICE_CANCEL)->u32 {
    STARTS.fetch_add(1,Ordering::Relaxed);
    unsafe {(*c).reserved=(*r).pQueryContext;}
    PENDING
}
unsafe fn fake_resolve(r:*const DNS_SERVICE_RESOLVE_REQUEST,c:*mut DNS_SERVICE_CANCEL)->u32 {
    STARTS.fetch_add(1,Ordering::Relaxed);
    unsafe {(*c).reserved=(*r).pQueryContext;}
    if MODE.load(Ordering::Relaxed)==3 {
        unsafe {resolve_callback(CANCELLED,(*r).pQueryContext,std::ptr::null());}
        let mut entries=pool().lock().unwrap(); reap(&mut entries);
        assert!(entries.iter().any(|op|op.token==unsafe {(*r).pQueryContext as usize}));
    }
    PENDING
}
unsafe fn fake_register(r:*const DNS_SERVICE_REGISTER_REQUEST,c:*mut DNS_SERVICE_CANCEL)->u32 {
    STARTS.fetch_add(1,Ordering::Relaxed);
    unsafe {(*c).reserved=(*r).pQueryContext;}
    if MODE.load(Ordering::Relaxed)==3 {unsafe {register_callback(0,(*r).pQueryContext,std::ptr::null());}}
    PENDING
}
unsafe fn fake_deregister(r:*const DNS_SERVICE_REGISTER_REQUEST)->u32 {
    DEREGISTERS.fetch_add(1,Ordering::Relaxed);
    match MODE.load(Ordering::Relaxed) {
        1|3=>unsafe {register_callback(0,(*r).pQueryContext,std::ptr::null());},
        4=>unsafe {register_callback(5,(*r).pQueryContext,std::ptr::null());},
        _=>(),
    }
    PENDING
}
unsafe fn fake_cancel_browse(c:*const DNS_SERVICE_CANCEL)->u32 {
    match MODE.load(Ordering::Relaxed) {1=>unsafe {browse_callback(CANCELLED,(*c).reserved,std::ptr::null());},2=>return 5,_=>()};0
}
unsafe fn fake_cancel_resolve(c:*const DNS_SERVICE_CANCEL)->u32 {
    match MODE.load(Ordering::Relaxed) {1=>unsafe {resolve_callback(CANCELLED,(*c).reserved,std::ptr::null());},2=>return 5,_=>()};0
}
unsafe fn fake_cancel_register(c:*const DNS_SERVICE_CANCEL)->u32 {
    if MODE.load(Ordering::Relaxed)==1 {unsafe {register_callback(CANCELLED,(*c).reserved,std::ptr::null());}}0
}
unsafe fn count_records(r:*const DNS_RECORDW) {if !r.is_null() {FREED_RECORDS.fetch_add(1,Ordering::Relaxed);}}
unsafe fn count_instance(r:*const DNS_SERVICE_INSTANCE) {if !r.is_null() {FREED_INSTANCES.fetch_add(1,Ordering::Relaxed);}}
const FAKE:Api=Api {browse:fake_browse,resolve:fake_resolve,register:fake_register,deregister:fake_deregister,cancel_browse:fake_cancel_browse,cancel_resolve:fake_cancel_resolve,cancel_register:fake_cancel_register,free_records:count_records,free_instance:count_instance};
fn adapter()->NativeDiscovery {NativeDiscovery {api:FAKE,..Default::default()}}
const NAME:&str="device._voice-node._udp.local";
const ID:&str="00000000-0000-0000-0000-000000000001";
fn id()->witvoice_contracts::values::Id {ID.to_owned().try_into().unwrap()}
fn context(d:&NativeDiscovery)->*const c_void {d.operations.last().unwrap().token as *const c_void}

#[test]
fn default_off_and_invalid_inputs_do_not_enter_os() {
    let _guard=guard();
    let mut d=adapter();
    assert_eq!(d.browse(false,19),Err(DiscoveryError::ApprovalRequired));
    assert_eq!(d.browse(true,0),Err(DiscoveryError::InterfaceRequired));
    assert_eq!(d.resolve(true,19,"other._http._tcp.local",10),Err(DiscoveryError::InvalidInput));
    assert_eq!(d.resolve(true,19,NAME,121),Err(DiscoveryError::InvalidInput));
    assert_eq!(d.advertise(true,19,&id(),std::net::SocketAddrV4::new(Ipv4Addr::LOCALHOST,1234)),Err(DiscoveryError::InvalidInput));
    assert_eq!(STARTS.load(Ordering::Relaxed),0);
    assert!(d.poll().is_empty()); assert_eq!(d.close(),Ok(true));
}

#[test]
fn cancel_return_is_not_terminal_and_full_queue_cannot_drop_terminal_state() {
    let _guard=guard(); let mut d=adapter(); d.browse(true,19).unwrap();
    let token=context(&d);
    for _ in 0..QUEUE_LIMIT+1 {d.queue.push(Event::Registered);}
    assert_eq!(d.close(),Ok(false));
    assert_eq!(d.pending_contexts(),1);
    let referenced=callback_operation(token).unwrap();
    unsafe {browse_callback(CANCELLED,token,std::ptr::null());}
    assert!(referenced.terminal.load(Ordering::Acquire));
    assert_eq!(d.close(),Ok(false));
    assert_eq!(pool().lock().unwrap().len(),1); // callback/reference still owns request memory.
    drop(referenced); assert_eq!(d.close(),Ok(true));
    assert!(pool().lock().unwrap().is_empty());
    assert_eq!(d.poll().len(),QUEUE_LIMIT);
    assert_eq!(d.dropped_events(),2);
}

#[test]
fn contended_queue_never_waits_or_loses_out_of_band_terminal_flag() {
    let _guard=guard();let mut d=adapter();d.browse(true,19).unwrap();let token=context(&d);
    let lock=d.queue.items.lock().unwrap();
    unsafe {browse_callback(CANCELLED,token,std::ptr::null());}
    assert_eq!(d.dropped_events(),1);
    assert!(d.operations[0].terminal.load(Ordering::Acquire));
    drop(lock);assert_eq!(d.close(),Ok(true));
}

#[test]
fn simultaneous_operation_caps_and_cancel_failure_quarantine() {
    let _guard=guard(); let mut d=adapter();
    d.browse(true,19).unwrap();
    assert_eq!(d.browse(true,19),Err(DiscoveryError::Capacity));
    for _ in 0..RESOLVE_LIMIT {d.resolve(true,19,NAME,120).unwrap();}
    assert_eq!(d.resolve(true,19,NAME,120),Err(DiscoveryError::Capacity));
    MODE.store(2,Ordering::Relaxed);
    assert_eq!(d.close(),Err(DiscoveryError::Native(5)));
    assert_eq!(d.pending_contexts(),9);
    let mut other=adapter();
    assert_eq!(other.browse(true,19),Err(DiscoveryError::Busy));
    let references=d.operations.clone();
    for op in references {unsafe {if op.kind==Kind::Browse {browse_callback(CANCELLED,op.token as *const c_void,std::ptr::null());} else {resolve_callback(CANCELLED,op.token as *const c_void,std::ptr::null());}}}
    d.clean(); assert!(pool().lock().unwrap().is_empty());
}

#[test]
fn fixed_context_budget_and_opaque_token_exhaustion() {
    let _guard=guard(); let mut d=adapter(); let mut held=Vec::new();
    for _ in 0..CONTEXT_LIMIT {
        d.resolve(true,19,NAME,10).unwrap();
        held.push(d.operations.last().unwrap().clone());
        unsafe {resolve_callback(CANCELLED,context(&d),std::ptr::null());}
        d.clean();
    }
    assert_eq!(pool().lock().unwrap().len(),CONTEXT_LIMIT);
    assert_eq!(d.resolve(true,19,NAME,10),Err(DiscoveryError::Capacity));
    held.clear();d.clean();assert!(pool().lock().unwrap().is_empty());
    let old=NEXT.swap(usize::MAX,Ordering::Relaxed);
    assert_eq!(d.resolve(true,19,NAME,10),Err(DiscoveryError::Capacity));
    assert_eq!(NEXT.load(Ordering::Relaxed),usize::MAX);
    NEXT.store(old,Ordering::Relaxed);
}

#[test]
fn immediate_callback_retains_call_reference_until_os_return() {
    let _guard=guard();MODE.store(3,Ordering::Relaxed);
    let mut d=adapter(); d.resolve(true,19,NAME,10).unwrap();
    assert_eq!(d.close(),Ok(true));assert!(pool().lock().unwrap().is_empty());
}

#[test]
fn register_cancel_race_still_deregisters_same_request_and_retains_failure() {
    let _guard=guard(); let mut d=adapter();
    d.advertise(true,19,&id(),std::net::SocketAddrV4::new(Ipv4Addr::new(192,168,1,4),4000)).unwrap();
    let token=context(&d);
    let before=unsafe {&*d.operations[0].storage.get()}.register.pServiceInstance;
    assert_eq!(d.close(),Ok(false));
    MODE.store(1,Ordering::Relaxed);
    unsafe {register_callback(0,token,std::ptr::null());}
    assert_eq!(DEREGISTERS.load(Ordering::Relaxed),1);
    assert_eq!(unsafe {&*d.operations[0].storage.get()}.register.pServiceInstance,before);
    assert_eq!(d.close(),Ok(true));
    MODE.store(3,Ordering::Relaxed);
    d.advertise(true,19,&id(),std::net::SocketAddrV4::new(Ipv4Addr::new(192,168,1,4),4000)).unwrap();
    MODE.store(4,Ordering::Relaxed);
    assert_eq!(d.close(),Ok(false)); // asynchronous failure is recorded by callback.
    assert_eq!(d.close(),Err(DiscoveryError::Native(5)));
    assert_eq!(d.pending_contexts(),1);
    assert_eq!(d.advertise(true,19,&id(),std::net::SocketAddrV4::new(Ipv4Addr::new(192,168,1,4),4000)),Err(DiscoveryError::Capacity));
}

struct InstanceFixture {dns:DNS_SERVICE_INSTANCE,name:Vec<u16>,keys_text:[Vec<u16>;2],values_text:[Vec<u16>;2],keys:[PWSTR;2],values:[PWSTR;2],ip4:u32}
fn instance()->Box<InstanceFixture> {
    let mut owned=Box::new(InstanceFixture {dns:Default::default(),name:wide(NAME),keys_text:[wide("node_id"),wide("protocol")],values_text:[wide(ID),wide("1")],keys:[PWSTR::null();2],values:[PWSTR::null();2],ip4:u32::from_ne_bytes([192,168,1,4])});
    for i in 0..2 {owned.keys[i]=PWSTR(owned.keys_text[i].as_mut_ptr());owned.values[i]=PWSTR(owned.values_text[i].as_mut_ptr());}
    owned.dns=DNS_SERVICE_INSTANCE {pszInstanceName:PWSTR(owned.name.as_mut_ptr()),ip4Address:&mut owned.ip4,wPort:4000,dwPropertyCount:2,keys:owned.keys.as_mut_ptr(),values:owned.values.as_mut_ptr(),dwInterfaceIndex:19,..Default::default()};
    owned
}
#[test]
fn instance_schema_lengths_interface_and_free_exactly_once() {
    let _guard=guard();let mut d=adapter();
    for invalid in 0..5 {
        d.resolve(true,19,NAME,30).unwrap();let mut fixture=instance();
        match invalid {0=>fixture.dns.dwPropertyCount=3,1=>fixture.dns.dwInterfaceIndex=0,2=>fixture.values_text[1]=wide("2"),3=>fixture.values_text[0]=wide(&"x".repeat(65)),_=>()}
        for i in 0..2 {fixture.values[i]=PWSTR(fixture.values_text[i].as_mut_ptr());}
        unsafe {resolve_callback(0,context(&d),&fixture.dns);}
        let events=d.poll();
        if invalid<4 {assert_eq!(events,vec![Event::Failed(INVALID)]);} else {assert!(matches!(&events[0],Event::Resolved {node_id,address,..} if node_id==&id() && *address=="192.168.1.4:4000".parse().unwrap()));}
        assert!(pool().lock().unwrap().is_empty());
    }
    assert_eq!(FREED_INSTANCES.load(Ordering::Relaxed),5);
}

#[test]
fn record_link_budget_cycle_data_length_utf16_and_once_free() {
    let _guard=guard();let mut d=adapter();d.browse(true,19).unwrap();let token=context(&d);
    let mut owner=wide(SERVICE_TYPE);let mut name=wide(NAME);
    let mut record=Box::new(DNS_RECORDW {pName:PWSTR(owner.as_mut_ptr()),wType:DNS_TYPE_PTR.0,wDataLength:std::mem::size_of::<DNS_PTR_DATAW>() as u16,dwTtl:1000,Data:DNS_RECORDW_1 {PTR:DNS_PTR_DATAW {pNameHost:PWSTR(name.as_mut_ptr())}},..Default::default()});
    unsafe {browse_callback(0,token,&*record);}
    assert!(matches!(&d.poll()[0],Event::Found {expires_at,..} if expires_at.saturating_duration_since(Instant::now())<=Duration::from_secs(120)));
    record.wDataLength=0;unsafe {browse_callback(0,token,&*record);}
    assert_eq!(d.poll(),vec![Event::Failed(INVALID)]);
    record.wDataLength=std::mem::size_of::<DNS_PTR_DATAW>() as u16;
    record.pNext=&mut *record;unsafe {browse_callback(0,token,&*record);}
    assert!(d.poll().contains(&Event::Failed(INVALID)));
    record.pNext=std::ptr::null_mut();let mut unterminated=vec![65u16;129];unsafe {record.Data.PTR.pNameHost=PWSTR(unterminated.as_mut_ptr());}
    unsafe {browse_callback(0,token,&*record);}
    assert_eq!(d.poll(),vec![Event::Failed(INVALID)]);
    let mut chain:Vec<Box<DNS_RECORDW>>=(0..RECORD_LIMIT+1).map(|_|Box::new(DNS_RECORDW::default())).collect();
    for index in 0..chain.len()-1 {let (before,after)=chain.split_at_mut(index+1);before[index].pNext=&mut *after[0];}
    unsafe {browse_callback(0,token,&*chain[0]);}
    assert_eq!(d.poll(),vec![Event::Failed(INVALID)]);
    assert_eq!(FREED_RECORDS.load(Ordering::Relaxed),5);
    MODE.store(1,Ordering::Relaxed);assert_eq!(d.close(),Ok(true));
}

#[test]
fn late_unknown_context_never_dereferences_token_and_frees_each_result_once() {
    let _guard=guard();let mut d=adapter();d.browse(true,19).unwrap();let old=context(&d);
    MODE.store(1,Ordering::Relaxed);assert_eq!(d.close(),Ok(true));
    let record=DNS_RECORDW::default();let result=DNS_SERVICE_INSTANCE::default();
    unsafe {browse_dispatch(0,old,&record,&FAKE);resolve_dispatch(0,usize::MAX as *const c_void,&result,&FAKE);register_dispatch(0,old,&result,&FAKE);}
    assert_eq!(FREED_RECORDS.load(Ordering::Relaxed),1);assert_eq!(FREED_INSTANCES.load(Ordering::Relaxed),2);
    assert_eq!(d.poll(),vec![Event::Stopped]);
    MODE.store(0,Ordering::Relaxed);d.browse(true,19).unwrap();assert_ne!(old,context(&d));
    MODE.store(1,Ordering::Relaxed);assert_eq!(d.close(),Ok(true));
}

#[test]
fn queue_expiry_and_resolve_deadline_are_not_wall_clock_or_ttl_reset() {
    let _guard=guard();let mut d=adapter();d.resolve(true,19,NAME,120).unwrap();
    let op=Arc::get_mut(d.operations.last_mut().unwrap());assert!(op.is_none()); // published state is shared/pinned.
    d.queue.push(Event::Found {full_name:NAME.into(),expires_at:Instant::now()-Duration::from_secs(1)});
    assert!(d.poll().is_empty());
    let mut detached=adapter();detached.interface=Some(19);
    // Test the real deadline branch without sleeping: immutable deadline is prepared before publish.
    let old=d.operations.pop().unwrap();
    unsafe {resolve_callback(CANCELLED,old.token as *const c_void,std::ptr::null());}
    drop(old);d.clean();
    detached.resolve(true,19,NAME,120).unwrap();
    let reference=detached.operations.pop().unwrap();
    let mut entries=pool().lock().unwrap();entries.clear();drop(entries);
    let mut reference=reference;Arc::get_mut(&mut reference).unwrap().deadline=Instant::now()-Duration::from_secs(1);
    pool().lock().unwrap().push(reference.clone());detached.operations.push(reference);MODE.store(1,Ordering::Relaxed);
    detached.poll();assert_eq!(detached.pending_contexts(),0);
}
