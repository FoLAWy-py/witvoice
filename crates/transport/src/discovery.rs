use crate::Error;
use std::{collections::BTreeMap, net::SocketAddr, time::Duration};

pub const SERVICE_TYPE: &str = "_voice-node._udp.local";
pub const MAX_RECORDS: usize = 128;
pub const MAX_TTL: Duration = Duration::from_secs(120);

/// Addressing only: accepting an address never authorizes a peer or microphone.
pub fn manual_address(text: &str) -> Result<SocketAddr, Error> {
    if text.len() > 96 || text.trim() != text {
        return Err(Error::InvalidAddress);
    }
    let address: SocketAddr = text.parse().map_err(|_| Error::InvalidAddress)?;
    let allowed = match address {
        SocketAddr::V4(a) => a.ip().is_private() || a.ip().is_link_local(),
        SocketAddr::V6(a) => {
            let first = a.ip().segments()[0];
            if first & 0xffc0 == 0xfe80 {
                a.scope_id() != 0
            } else {
                first & 0xfe00 == 0xfc00 && a.scope_id() == 0
            }
        }
    };
    if !allowed || address.port() == 0 {
        return Err(Error::InvalidAddress);
    }
    Ok(address)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Advertisement {
    pub service_type: String,
    pub instance: String,
    pub node_id: witvoice_contracts::values::Id,
    pub protocol_version: u16,
    pub address: SocketAddr,
}
pub struct DiscoveryRecords {
    records: BTreeMap<String, (Advertisement, Duration)>,
    last_now: Duration,
}
impl Default for DiscoveryRecords {
    fn default() -> Self {
        Self {
            records: BTreeMap::new(),
            last_now: Duration::ZERO,
        }
    }
}
impl DiscoveryRecords {
    fn expire(&mut self, now: Duration) -> Result<(), Error> {
        if now < self.last_now {
            return Err(Error::ClockWentBackwards);
        }
        self.last_now = now;
        self.records.retain(|_, (_, expiry)| *expiry > now);
        Ok(())
    }
    pub fn observe(
        &mut self,
        record: Advertisement,
        ttl: Duration,
        now: Duration,
    ) -> Result<(), Error> {
        self.expire(now)?;
        if record.service_type != SERVICE_TYPE
            || record.instance.is_empty()
            || record.instance.len() > 63
            || record.instance.chars().any(char::is_control)
            || record.protocol_version != witvoice_contracts::PROTOCOL_VERSION
            || ttl.is_zero()
            || ttl > MAX_TTL
        {
            return Err(Error::InvalidIdentity);
        }
        manual_address(&record.address.to_string())?;
        if !self.records.contains_key(&record.instance) && self.records.len() == MAX_RECORDS {
            return Err(Error::Limit);
        }
        let expiry = now.checked_add(ttl).ok_or(Error::Limit)?;
        self.records
            .insert(record.instance.clone(), (record, expiry));
        Ok(())
    }
    pub fn snapshot(&mut self, now: Duration) -> Result<Vec<Advertisement>, Error> {
        self.expire(now)?;
        Ok(self.records.values().map(|(r, _)| r.clone()).collect())
    }
}

/// Fail closed BEFORE constructing mdns-sd's socket-owning ServiceDaemon.
/// Its 0.21.4 DNS cache has unbounded HashMap/Vec inserts despite bounded channels.
pub fn enable_mdns(_explicit_lan_approval: bool) -> Result<(), Error> {
    Err(Error::MdnsCacheUnbounded)
}
