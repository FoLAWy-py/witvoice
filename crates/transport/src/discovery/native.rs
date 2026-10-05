use super::*;
use std::time::Instant;
use witvoice_platform::discovery::{DiscoveryError, Event, NativeDiscovery, validate_full_name};

/// Real Windows adapter. Addresses stay untrusted; this type has no TrustStore access.
pub struct NativePeers {
    native: NativeDiscovery,
    records: DiscoveryRecords,
    started: Instant,
    interface: Option<u32>,
    rejected: u64,
}
impl Default for NativePeers {
    fn default()->Self {Self {native:NativeDiscovery::default(),records:DiscoveryRecords::default(),started:Instant::now(),interface:None,rejected:0}}
}
impl NativePeers {
    pub fn browse(&mut self,explicit_lan_approval:bool,interface:u32)->Result<(),DiscoveryError> {
        self.native.browse(explicit_lan_approval,interface)?;
        self.interface=Some(interface);Ok(())
    }
    pub fn resolve(&mut self,explicit_lan_approval:bool,interface:u32,full_name:&str)->Result<(),DiscoveryError> {
        self.native.resolve(explicit_lan_approval,interface,full_name,120)?;
        self.interface=Some(interface);Ok(())
    }
    pub fn advertise(&mut self,explicit_lan_approval:bool,interface:u32,node_id:&witvoice_contracts::values::Id,address:std::net::SocketAddrV4)->Result<(),DiscoveryError> {
        self.native.advertise(explicit_lan_approval,interface,node_id,address)?;
        self.interface=Some(interface);Ok(())
    }
    fn apply(&mut self,event:Event,now:Instant)->Result<(),Error> {
        let elapsed=now.checked_duration_since(self.started).ok_or(Error::ClockWentBackwards)?;
        if self.interface.is_none() && matches!(&event,Event::Found {..}|Event::Resolved {..}|Event::Registered) {return Err(Error::InvalidIdentity);}
        match event {
            Event::Found {full_name,expires_at}=>{
                validate_full_name(&full_name).map_err(|_|Error::InvalidIdentity)?;
                let ttl=expires_at.saturating_duration_since(now).as_secs().min(120) as u32;
                if ttl==0 {return Err(Error::InvalidIdentity);}
                self.native.resolve_until(true,self.interface.ok_or(Error::InvalidIdentity)?,&full_name,expires_at).map_err(|_|Error::Limit)?;
            }
            Event::Removed {full_name}=>{
                let name=validate_full_name(&full_name).map_err(|_|Error::InvalidIdentity)?;
                self.records.expire(elapsed)?;
                self.records.records.remove(name);
            }
            Event::Resolved {instance,node_id,address,expires_at}=>{
                let ttl=expires_at.checked_duration_since(now).ok_or(Error::InvalidIdentity)?;
                self.records.observe(Advertisement {service_type:SERVICE_TYPE.into(),instance,node_id,protocol_version:1,address},ttl.min(MAX_TTL),elapsed)?;
            }
            Event::Stopped|Event::Failed(_)=>{self.records.expire(elapsed)?;self.records.records.clear();}
            Event::Registered=>(),
        }
        Ok(())
    }
    pub fn poll(&mut self)->Result<Vec<Advertisement>,Error> {
        let now=Instant::now();
        for event in self.native.poll() {if self.apply(event,now).is_err() {self.rejected=self.rejected.saturating_add(1);}}
        self.records.snapshot(now.duration_since(self.started))
    }
    /// false means cancellation is pending/quarantined; never report it as cleanup complete.
    pub fn close(&mut self)->Result<bool,DiscoveryError> {
        self.interface=None;self.records.records.clear();self.native.close()
    }
    pub fn rejected_events(&self)->u64 {self.rejected.saturating_add(self.native.dropped_events())}
    pub fn pending_contexts(&self)->usize {self.native.pending_contexts()}
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn untrusted_native_results_use_address_and_ttl_guards_without_pairing() {
        let mut peers=NativePeers::default();peers.interface=Some(19);let now=Instant::now();
        let id="00000000-0000-0000-0000-000000000001".to_owned().try_into().unwrap();
        assert_eq!(peers.apply(Event::Resolved {instance:"node".into(),node_id:id,address:"8.8.8.8:1".parse().unwrap(),expires_at:now+Duration::from_secs(1)},now),Err(Error::InvalidAddress));
        assert!(peers.poll().unwrap().is_empty());
        let id="00000000-0000-0000-0000-000000000001".to_owned().try_into().unwrap();
        peers.apply(Event::Resolved {instance:"node".into(),node_id:id,address:"192.168.1.4:4000".parse().unwrap(),expires_at:now+Duration::from_secs(1)},now).unwrap();
        assert_eq!(peers.records.snapshot(Duration::from_secs(2)).unwrap().len(),0);
        assert_eq!(peers.apply(Event::Found {full_name:"other.example.com".into(),expires_at:now+Duration::from_secs(1)},now),Err(Error::InvalidIdentity));
        assert_eq!(peers.close(),Ok(true));
    }
    #[test]
    fn closing_cannot_restart_resolution_or_repopulate_records_from_queued_results() {
        let mut peers=NativePeers::default();peers.interface=Some(19);peers.close().unwrap();
        let now=Instant::now();
        assert_eq!(peers.apply(Event::Found {full_name:"node._voice-node._udp.local".into(),expires_at:now+Duration::from_secs(1)},now),Err(Error::InvalidIdentity));
        let id="00000000-0000-0000-0000-000000000001".to_owned().try_into().unwrap();
        assert_eq!(peers.apply(Event::Resolved {instance:"node".into(),node_id:id,address:"192.168.1.4:4000".parse().unwrap(),expires_at:now+Duration::from_secs(1)},now),Err(Error::InvalidIdentity));
        assert!(peers.poll().unwrap().is_empty());assert_eq!(peers.pending_contexts(),0);
    }
}
