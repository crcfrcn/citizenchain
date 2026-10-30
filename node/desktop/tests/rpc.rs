use crate::rpc::build_local_rpc_client;
use crate::rpc::current_rpc_port;
use crate::rpc::local_rpc_http_url;
use crate::rpc::normalize_genesis_hash;

#[test]
fn desktop_rpc_url_uses_own_loopback_identity() {
    crate::tls::prepare_desktop_rpc_tls().unwrap();
    assert_eq!(
        local_rpc_http_url().unwrap(),
        format!("https://127.0.0.1:{}", current_rpc_port())
    );
    assert!(build_local_rpc_client(&[1, 2, 3]).is_err());
}

#[test]
fn normalize_genesis_hash_accepts_valid_hash() {
    let hash = format!("0x{}", "Aa".repeat(32));
    let normalized = normalize_genesis_hash(&hash).unwrap();
    assert_eq!(normalized, format!("0x{}", "aa".repeat(32)));
}

#[test]
fn normalize_genesis_hash_rejects_missing_prefix() {
    let hash = "aa".repeat(32);
    assert!(normalize_genesis_hash(&hash).is_err());
}

#[test]
fn normalize_genesis_hash_rejects_wrong_length() {
    assert!(normalize_genesis_hash("0x1234").is_err());
}

#[test]
fn normalize_genesis_hash_rejects_non_hex() {
    let hash = format!("0x{}zz", "aa".repeat(31));
    assert!(normalize_genesis_hash(&hash).is_err());
}
