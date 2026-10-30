use crate::validation::normalize_account_id;
use crate::validation::normalize_grandpa_key;
use crate::validation::normalize_node_key;
use crate::validation::normalize_public_key;

#[test]
fn miner_transfer_capability_is_single_use_and_can_be_revoked() {
    use crate::security::{
        consume_miner_transfer_token, issue_miner_transfer_token, revoke_miner_transfer_token,
    };
    assert!(!consume_miner_transfer_token("unissued-test-capability"));
    let token = issue_miner_transfer_token().unwrap();
    assert!(consume_miner_transfer_token(&token));
    assert!(!consume_miner_transfer_token(&token));
    let revoked = issue_miner_transfer_token().unwrap();
    revoke_miner_transfer_token(&revoked);
    assert!(!consume_miner_transfer_token(&revoked));
}

#[test]
fn empty_unlock_input_is_rejected_before_any_device_authentication() {
    use crate::security::ensure_unlock_password;
    assert!(ensure_unlock_password("").is_err());
    assert!(ensure_unlock_password(" \n\t ").is_err());
    assert_eq!(ensure_unlock_password(" 合成输入 ").unwrap(), "合成输入");
}

// --- normalize_account_id ---

#[test]
fn account_id_empty_rejected() {
    assert!(normalize_account_id("").is_err());
    assert!(normalize_account_id("   ").is_err());
}

#[test]
fn account_id_valid_hex() {
    let hex = format!("0x{}", "a1b2c3d4".repeat(8));
    let result = normalize_account_id(&hex).unwrap();
    assert!(result.starts_with("0x"));
    assert_eq!(result.len(), 66);
    assert_eq!(result, result.to_ascii_lowercase());
}

#[test]
fn account_id_hex_wrong_length() {
    assert!(normalize_account_id("0xabcd").is_err());
}

#[test]
fn account_id_hex_non_hex_chars() {
    let bad = format!("0x{}zz", "a1".repeat(31));
    assert!(normalize_account_id(&bad).is_err());
}

#[test]
fn account_id_hex_uppercase_rejected() {
    let hex = format!("0x{}", "A1B2C3D4".repeat(8));
    assert!(normalize_account_id(&hex).is_err());
}

#[test]
fn account_id_requires_0x_prefix() {
    assert!(normalize_account_id(&"a1".repeat(32)).is_err());
}

#[test]
fn public_key_uses_prefixed_lowercase_hex() {
    let input = format!("0x{}", "a1b2".repeat(16));
    assert_eq!(normalize_public_key(&input).unwrap(), input);
    assert!(normalize_public_key(&format!("0x{}", "A1B2".repeat(16))).is_err());
    assert!(normalize_public_key(&"a1".repeat(32)).is_err());
}

// --- normalize_node_key ---

#[test]
fn node_key_empty_rejected() {
    assert!(normalize_node_key("").is_err());
}

#[test]
fn node_key_valid_no_prefix() {
    let key = "a1b2c3d4".repeat(8);
    let result = normalize_node_key(&key).unwrap();
    assert_eq!(result.len(), 64);
    assert!(!result.starts_with("0x"));
}

#[test]
fn node_key_valid_with_0x_prefix() {
    let key = format!("0x{}", "a1b2c3d4".repeat(8));
    let result = normalize_node_key(&key).unwrap();
    assert_eq!(result.len(), 64);
    assert!(!result.starts_with("0x"));
}

#[test]
fn node_key_wrong_length() {
    assert!(normalize_node_key("abcdef").is_err());
}

// --- normalize_grandpa_key ---

#[test]
fn grandpa_key_empty_rejected() {
    assert!(normalize_grandpa_key("").is_err());
}

#[test]
fn grandpa_key_valid() {
    let key = "a1b2c3d4".repeat(8);
    let result = normalize_grandpa_key(&key).unwrap();
    assert_eq!(result.len(), 64);
}

#[test]
fn grandpa_key_strips_0x() {
    let key = format!("0x{}", "A1B2C3D4".repeat(8));
    let result = normalize_grandpa_key(&key).unwrap();
    assert_eq!(result, "a1b2c3d4".repeat(8));
}

#[test]
fn grandpa_key_wrong_length() {
    assert!(normalize_grandpa_key("abcd").is_err());
}
