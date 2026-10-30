use super::*;

#[test]
fn client_configuration_has_no_server_and_rejects_invalid_trust_anchor() {
    assert!(Config::client().server.is_none());
    assert!(Config::builder()
        .add_trust(&Certificate::new(vec![0, 1, 2]))
        .is_err());
}

#[test]
fn server_key_and_certificate_must_match() {
    let identity = rcgen::generate_simple_self_signed(vec!["127.0.0.1".into()]).unwrap();
    let other = rcgen::KeyPair::generate().unwrap();
    assert!(Config::new(
        PrivateKey::new(other.serialize_der()),
        [Certificate::new(identity.cert.der().to_vec())]
    )
    .is_err());
    assert!(Config::new(
        PrivateKey::new(identity.key_pair.serialize_der()),
        [Certificate::new(identity.cert.der().to_vec())]
    )
    .is_ok());
}
