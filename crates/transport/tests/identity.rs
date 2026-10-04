use witvoice_transport::{
    Error,
    identity::{LocalIdentity, MAX_IDENTITY_BYTES, PublicIdentity, now_unix},
    trust::{Permissions, TrustStore},
};

#[test]
fn public_roundtrip_strict_schema_versions_lengths_and_tampering() {
    let now = now_unix().unwrap();
    let local = LocalIdentity::generate("Computer", Some("192.168.1.4:4500".into()), now).unwrap();
    let text = local.public().export(now).unwrap();
    assert_eq!(PublicIdentity::parse(&text, now).unwrap(), *local.public());
    assert!(!text.contains("PRIVATE"));
    let mut json: serde_json::Value = serde_json::from_str(&text).unwrap();
    json["secret"] = "not permitted".into();
    assert_eq!(
        PublicIdentity::parse(&json.to_string(), now),
        Err(Error::InvalidIdentity)
    );
    assert_eq!(
        PublicIdentity::parse(&"x".repeat(MAX_IDENTITY_BYTES + 1), now),
        Err(Error::Limit)
    );
    let mut public = local.public().clone();
    public.protocol_version = 2;
    assert_eq!(public.validate(now), Err(Error::UnsupportedVersion));
    public = local.public().clone();
    public.certificate_der_hex.replace_range(0..2, "00");
    assert_eq!(public.validate(now), Err(Error::InvalidCertificate));
    public = local.public().clone();
    public.node_id = "00000000-0000-0000-0000-000000000000"
        .to_owned()
        .try_into()
        .unwrap();
    assert_eq!(public.validate(now), Err(Error::InvalidCertificate));
    assert_eq!(
        local.public().validate(now - 400),
        Err(Error::InvalidCertificate)
    );
    assert_eq!(
        local.public().validate(now + 6 * 365 * 86400),
        Err(Error::InvalidCertificate)
    );
    assert!(LocalIdentity::generate("bad\nname", None, now).is_err());
    assert!(LocalIdentity::generate("ok", Some("example.com:443".into()), now).is_err());
}

#[test]
fn altered_certificate_with_recomputed_fingerprint_still_fails_standard_signature() {
    let now = now_unix().unwrap();
    let mut public = LocalIdentity::generate("name", None, now)
        .unwrap()
        .public()
        .clone();
    let mut der = public.validate(now).unwrap();
    let last = der.len() - 1;
    der[last] ^= 1;
    public.certificate_der_hex = der.iter().map(|b| format!("{b:02x}")).collect();
    public.certificate_sha256 = witvoice_transport::identity::fingerprint(&der)
        .try_into()
        .unwrap();
    assert_eq!(public.validate(now), Err(Error::InvalidCertificate));
}

#[test]
fn no_discovery_or_same_name_trust_and_revocation_invalidates_old_permits() {
    let now = now_unix().unwrap();
    let real = LocalIdentity::generate("same name", None, now).unwrap();
    let forged = LocalIdentity::generate("same name", None, now).unwrap();
    let der = real.public().validate(now).unwrap();
    let mut trust = TrustStore::default();
    assert!(matches!(
        trust.verify_presented(&real.public().node_id, &der, now),
        Err(Error::Untrusted)
    ));
    trust
        .approve(real.public().clone(), Permissions::default(), now)
        .unwrap();
    let permit = trust
        .verify_presented(&real.public().node_id, &der, now)
        .unwrap();
    assert_eq!(trust.permissions(&permit).unwrap(), Permissions::default());
    assert!(matches!(
        trust.verify_presented(
            &real.public().node_id,
            &forged.public().validate(now).unwrap(),
            now
        ),
        Err(Error::InvalidCertificate)
    ));
    assert!(matches!(
        trust.verify_presented(
            &forged.public().node_id,
            &forged.public().validate(now).unwrap(),
            now
        ),
        Err(Error::Untrusted)
    ));
    trust
        .approve(
            real.public().clone(),
            Permissions {
                compute: true,
                profile_transfer: false,
            },
            now,
        )
        .unwrap();
    assert_eq!(trust.permissions(&permit), Err(Error::Revoked));
    let current = trust
        .verify_presented(&real.public().node_id, &der, now)
        .unwrap();
    assert_eq!(
        trust.permissions(&current).unwrap(),
        Permissions {
            compute: true,
            profile_transfer: false
        }
    );
    trust.revoke(&real.public().node_id).unwrap();
    assert_eq!(trust.permissions(&current), Err(Error::Revoked));
    assert!(matches!(
        trust.verify_presented(&real.public().node_id, &der, now),
        Err(Error::Untrusted)
    ));
    trust
        .approve(real.public().clone(), Permissions::default(), now)
        .unwrap();
    assert_eq!(trust.permissions(&current), Err(Error::Revoked));
}

#[test]
fn changed_key_for_existing_node_requires_re_pair_and_preserves_old_pin() {
    let now = now_unix().unwrap();
    let local = LocalIdentity::generate("name", None, now).unwrap();
    let mut changed = local.public().clone();
    let mut params = rcgen::CertificateParams::new(Vec::new()).unwrap();
    params.distinguished_name = rcgen::DistinguishedName::new();
    params.distinguished_name.push(
        rcgen::DnType::CommonName,
        String::from(changed.node_id.clone()),
    );
    let key = rcgen::KeyPair::generate().unwrap();
    let cert = params.self_signed(&key).unwrap();
    changed.certificate_der_hex = cert.der().iter().map(|b| format!("{b:02x}")).collect();
    changed.certificate_sha256 = witvoice_transport::identity::fingerprint(cert.der())
        .try_into()
        .unwrap();
    changed.validate(now).unwrap();
    let mut trust = TrustStore::default();
    trust
        .approve(local.public().clone(), Permissions::default(), now)
        .unwrap();
    assert_eq!(
        trust.approve(changed.clone(), Permissions::default(), now),
        Err(Error::RePairRequired)
    );
    let old = local.public().validate(now).unwrap();
    assert!(
        trust
            .verify_presented(&local.public().node_id, &old, now)
            .is_ok()
    );
    trust.revoke(&local.public().node_id).unwrap();
    trust
        .approve(changed.clone(), Permissions::default(), now)
        .unwrap();
    assert!(matches!(
        trust.verify_presented(&local.public().node_id, &old, now),
        Err(Error::InvalidCertificate)
    ));
}

#[test]
fn trusted_peer_capacity_is_enforced_without_eviction() {
    let now = now_unix().unwrap();
    let mut trust = TrustStore::default();
    let first = LocalIdentity::generate("first", None, now).unwrap();
    trust
        .approve(first.public().clone(), Permissions::default(), now)
        .unwrap();
    let permit = trust
        .verify_presented(
            &first.public().node_id,
            &first.public().validate(now).unwrap(),
            now,
        )
        .unwrap();
    for _ in 1..128 {
        let local = LocalIdentity::generate("name", None, now).unwrap();
        trust
            .approve(local.public().clone(), Permissions::default(), now)
            .unwrap();
    }
    let overflow = LocalIdentity::generate("overflow", None, now).unwrap();
    assert_eq!(
        trust.approve(overflow.public().clone(), Permissions::default(), now),
        Err(Error::Limit)
    );
    assert_eq!(trust.permissions(&permit), Ok(Permissions::default()));
}

#[cfg(windows)]
#[test]
fn restart_load_preserves_real_public_identity_and_private_key_matching() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join(".local")
        .join(format!("t016-transport-{}", std::process::id()));
    std::fs::create_dir(&root).unwrap();
    let now = now_unix().unwrap();
    let local = LocalIdentity::generate("restart", None, now).unwrap();
    let store = witvoice_platform::identity::IdentityStore::open(&root).unwrap();
    local.save_new(&store, now).unwrap();
    assert!(local.save_new(&store, now).is_err());
    let public = local.public().clone();
    drop(local);
    drop(store);
    let store = witvoice_platform::identity::IdentityStore::open(&root).unwrap();
    let loaded = LocalIdentity::load(&store, now).unwrap();
    assert_eq!(*loaded.public(), public);
    drop(store);
    // Only this test's explicitly constructed .local child is removed.
    std::fs::remove_dir_all(root).unwrap();
}
