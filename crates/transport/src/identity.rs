use crate::{Error, discovery::manual_address};
use rcgen::{CertificateParams, DistinguishedName, DnType, KeyPair};
use ring::{
    digest,
    rand::{SecureRandom, SystemRandom},
};
use serde::{Deserialize, Serialize};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use witvoice_contracts::values::{Id, Sha256};

pub const MAX_IDENTITY_BYTES: usize = 16 * 1024;
pub const MAX_CERT_BYTES: usize = 4096;
pub fn now_unix() -> Result<i64, Error> {
    i64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| Error::Crypto)?
            .as_secs(),
    )
    .map_err(|_| Error::Crypto)
}
fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut result = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(result, "{byte:02x}");
    }
    result
}
fn unhex(text: &str) -> Result<Vec<u8>, Error> {
    if text.is_empty()
        || text.len() > MAX_CERT_BYTES * 2
        || !text.len().is_multiple_of(2)
        || !text
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(Error::InvalidIdentity);
    }
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).map_err(|_| Error::InvalidIdentity))
        .collect()
}
pub fn fingerprint(der: &[u8]) -> String {
    hex(digest::digest(&digest::SHA256, der).as_ref())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PublicIdentity {
    pub identity_version: u16,
    pub protocol_version: u16,
    pub node_id: Id,
    pub display_name: String,
    pub certificate_der_hex: String,
    pub certificate_sha256: Sha256,
    pub endpoint_hint: Option<String>,
}
impl PublicIdentity {
    pub fn parse(text: &str, now: i64) -> Result<Self, Error> {
        if text.len() > MAX_IDENTITY_BYTES {
            return Err(Error::Limit);
        }
        let value: Self = serde_json::from_str(text).map_err(|_| Error::InvalidIdentity)?;
        value.validate(now)?;
        Ok(value)
    }
    pub fn validate(&self, now: i64) -> Result<Vec<u8>, Error> {
        if self.identity_version != 1
            || self.protocol_version != witvoice_contracts::PROTOCOL_VERSION
        {
            return Err(Error::UnsupportedVersion);
        }
        if self.display_name.is_empty()
            || self.display_name.len() > 63
            || self.display_name.chars().any(char::is_control)
        {
            return Err(Error::InvalidIdentity);
        }
        if let Some(hint) = &self.endpoint_hint {
            manual_address(hint)?;
        }
        let der = unhex(&self.certificate_der_hex)?;
        if fingerprint(&der) != String::from(self.certificate_sha256.clone()) {
            return Err(Error::InvalidCertificate);
        }
        let (rest, cert) =
            x509_parser::parse_x509_certificate(&der).map_err(|_| Error::InvalidCertificate)?;
        if !rest.is_empty()
            || cert.subject() != cert.issuer()
            || cert.signature_algorithm.algorithm.to_id_string() != "1.2.840.10045.4.3.2"
            || cert.public_key().algorithm.algorithm.to_id_string() != "1.2.840.10045.2.1"
            || cert
                .public_key()
                .algorithm
                .parameters
                .as_ref()
                .and_then(|p| p.as_oid().ok())
                .map(|o| o.to_id_string())
                .as_deref()
                != Some("1.2.840.10045.3.1.7")
            || cert.validity().not_before.timestamp() > now
            || cert.validity().not_after.timestamp() < now
            || cert.verify_signature(None).is_err()
        {
            return Err(Error::InvalidCertificate);
        }
        let mut cn = cert.subject().iter_common_name();
        let id = String::from(self.node_id.clone());
        if cn.next().and_then(|a| a.as_str().ok()) != Some(id.as_str()) || cn.next().is_some() {
            return Err(Error::InvalidCertificate);
        }
        drop(cn);
        Ok(der)
    }
    pub fn export(&self, now: i64) -> Result<String, Error> {
        self.validate(now)?;
        let text = serde_json::to_string(self).map_err(|_| Error::InvalidIdentity)?;
        if text.len() > MAX_IDENTITY_BYTES {
            return Err(Error::Limit);
        }
        Ok(text)
    }
}

/// Intentionally no Debug/Clone/private-key export API.
pub struct LocalIdentity {
    public: PublicIdentity,
    key: KeyPair,
}
impl LocalIdentity {
    pub fn generate(
        display_name: &str,
        endpoint_hint: Option<String>,
        now: i64,
    ) -> Result<Self, Error> {
        if !(300..=253_244_620_799).contains(&now) {
            return Err(Error::Crypto);
        }
        let mut random = [0; 16];
        SystemRandom::new()
            .fill(&mut random)
            .map_err(|_| Error::Crypto)?;
        random[6] = (random[6] & 15) | 0x40;
        random[8] = (random[8] & 63) | 0x80;
        let h = hex(&random);
        let id = format!(
            "{}-{}-{}-{}-{}",
            &h[..8],
            &h[8..12],
            &h[12..16],
            &h[16..20],
            &h[20..]
        );
        let key = KeyPair::generate().map_err(|_| Error::Crypto)?;
        let mut params =
            CertificateParams::new(vec![format!("{id}.voice.local")]).map_err(|_| Error::Crypto)?;
        params.distinguished_name = DistinguishedName::new();
        params
            .distinguished_name
            .push(DnType::CommonName, id.clone());
        let base = rcgen::date_time_ymd(1970, 1, 1);
        params.not_before = base + Duration::from_secs((now - 300) as u64);
        let expiry = now.checked_add(365 * 5 * 86400).ok_or(Error::Crypto)?;
        params.not_after = base + Duration::from_secs(expiry as u64);
        let cert = params.self_signed(&key).map_err(|_| Error::Crypto)?;
        let public = PublicIdentity {
            identity_version: 1,
            protocol_version: witvoice_contracts::PROTOCOL_VERSION,
            node_id: id.try_into().map_err(|_| Error::Crypto)?,
            display_name: display_name.to_owned(),
            certificate_der_hex: hex(cert.der()),
            certificate_sha256: fingerprint(cert.der())
                .try_into()
                .map_err(|_| Error::Crypto)?,
            endpoint_hint,
        };
        public.validate(now)?;
        Ok(Self { public, key })
    }
    pub fn public(&self) -> &PublicIdentity {
        &self.public
    }
    #[cfg(windows)]
    pub fn save_new(
        &self,
        store: &witvoice_platform::identity::IdentityStore,
        now: i64,
    ) -> Result<(), Error> {
        let public = self.public.export(now)?;
        let mut bytes = Vec::with_capacity(public.len() + self.key.serialized_der().len() + 4);
        bytes.extend_from_slice(&(public.len() as u32).to_be_bytes());
        bytes.extend_from_slice(public.as_bytes());
        bytes.extend_from_slice(self.key.serialized_der());
        let result = store.save_new(&bytes).map_err(|_| Error::Storage);
        bytes.fill(0);
        result
    }
    #[cfg(windows)]
    pub fn load(
        store: &witvoice_platform::identity::IdentityStore,
        now: i64,
    ) -> Result<Self, Error> {
        let secret = store.load().map_err(|_| Error::Storage)?;
        let bytes = secret.as_bytes();
        let len = u32::from_be_bytes(
            bytes
                .get(..4)
                .ok_or(Error::Storage)?
                .try_into()
                .map_err(|_| Error::Storage)?,
        ) as usize;
        if len > MAX_IDENTITY_BYTES {
            return Err(Error::Storage);
        }
        let text = std::str::from_utf8(bytes.get(4..4 + len).ok_or(Error::Storage)?)
            .map_err(|_| Error::Storage)?;
        let public = PublicIdentity::parse(text, now)?;
        let key = KeyPair::try_from(bytes.get(4 + len..).ok_or(Error::Storage)?)
            .map_err(|_| Error::Crypto)?;
        let der = public.validate(now)?;
        let (_, cert) =
            x509_parser::parse_x509_certificate(&der).map_err(|_| Error::InvalidCertificate)?;
        if cert.public_key().subject_public_key.data.as_ref() != key.public_key_raw() {
            return Err(Error::InvalidCertificate);
        }
        Ok(Self { public, key })
    }
}
