use crate::chain::derive_fee_account;
use crate::chain::derive_main_account;
use crate::chain::encode_cid_key_data;

#[test]
fn encode_cid_key_data_round_trip() {
    let raw = "LN001-NRC0G-944805165-2026";
    let encoded = encode_cid_key_data(raw).unwrap();
    // Compact<u32> 长度前缀(单字节模式 raw.len() < 64)+ raw 字节
    assert_eq!(encoded[0], (raw.len() as u8) << 2);
    assert_eq!(&encoded[1..], raw.as_bytes());
}

#[test]
fn empty_cid_rejected() {
    let err = encode_cid_key_data("").unwrap_err();
    assert!(err.contains("长度"));
}

#[test]
fn over_long_cid_rejected() {
    let s = "a".repeat(97);
    let err = encode_cid_key_data(&s).unwrap_err();
    assert!(err.contains("长度"));
}

#[test]
fn main_and_fee_accounts_differ() {
    let cid = "LN001-NRC0G-944805165-2026";
    assert_ne!(derive_main_account(cid), derive_fee_account(cid));
}
