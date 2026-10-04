use crate::{Error, identity::PublicIdentity};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Permissions {
    pub compute: bool,
    pub profile_transfer: bool,
}
#[derive(Debug, Clone)]
pub struct Permit {
    node_id: String,
    generation: u64,
}
struct Pin {
    identity: PublicIdentity,
    generation: u64,
    permissions: Permissions,
}
#[derive(Default)]
pub struct TrustStore {
    pins: BTreeMap<String, Pin>,
    generation: u64,
    exhausted: bool,
}
impl TrustStore {
    fn advance(&mut self) -> Result<u64, Error> {
        if self.exhausted {
            return Err(Error::GenerationExhausted);
        }
        if let Some(next) = self.generation.checked_add(1) {
            self.generation = next;
            Ok(next)
        } else {
            self.pins.clear();
            self.exhausted = true;
            Err(Error::GenerationExhausted)
        }
    }
    /// Only authenticated Node handling explicit local user approval may call this.
    /// Discovery and certificate receipt never call it. One side's import is not mutual pairing.
    pub fn approve(
        &mut self,
        identity: PublicIdentity,
        permissions: Permissions,
        now: i64,
    ) -> Result<(), Error> {
        identity.validate(now)?;
        let id = String::from(identity.node_id.clone());
        if let Some(old) = self.pins.get(&id) {
            if old.identity.certificate_sha256 != identity.certificate_sha256 {
                return Err(Error::RePairRequired);
            }
        } else if self.pins.len() >= 128 {
            return Err(Error::Limit);
        }
        let generation = self.advance()?;
        self.pins.insert(
            id,
            Pin {
                identity,
                generation,
                permissions,
            },
        );
        Ok(())
    }
    /// Actual certificate DER is checked; this is NOT a TLS private-key possession proof.
    pub fn verify_presented(
        &self,
        node_id: &witvoice_contracts::values::Id,
        certificate_der: &[u8],
        now: i64,
    ) -> Result<Permit, Error> {
        let id = String::from(node_id.clone());
        let pin = self.pins.get(&id).ok_or(Error::Untrusted)?;
        if pin.identity.validate(now)?.as_slice() != certificate_der {
            return Err(Error::InvalidCertificate);
        }
        Ok(Permit {
            node_id: id,
            generation: pin.generation,
        })
    }
    pub fn permissions(&self, permit: &Permit) -> Result<Permissions, Error> {
        let pin = self.pins.get(&permit.node_id).ok_or(Error::Revoked)?;
        if pin.generation != permit.generation {
            return Err(Error::Revoked);
        }
        Ok(pin.permissions)
    }
    /// Caller must synchronously stop matching sessions and delete transient profiles in T018.
    /// Removing the pin invalidates previously issued permits immediately, even on overflow.
    pub fn revoke(&mut self, node_id: &witvoice_contracts::values::Id) -> Result<(), Error> {
        self.pins.remove(&String::from(node_id.clone()));
        self.advance().map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn generation_exhaustion_clears_pins_and_cannot_restore_trust() {
        let now = crate::identity::now_unix().unwrap();
        let local = crate::identity::LocalIdentity::generate("test", None, now).unwrap();
        let mut store = TrustStore::default();
        store
            .approve(local.public().clone(), Permissions::default(), now)
            .unwrap();
        let permit = store
            .verify_presented(
                &local.public().node_id,
                &local.public().validate(now).unwrap(),
                now,
            )
            .unwrap();
        store.generation = u64::MAX;
        assert_eq!(
            store.revoke(&local.public().node_id),
            Err(Error::GenerationExhausted)
        );
        assert_eq!(store.permissions(&permit), Err(Error::Revoked));
        assert_eq!(
            store.approve(local.public().clone(), Permissions::default(), now),
            Err(Error::GenerationExhausted)
        );
        assert!(store.pins.is_empty());
    }
}
