use crate::endpoint::clearing_bank_nodes_key;
use crate::endpoint::encode_cid_key_data;

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
    let s = "A".repeat(33);
    let err = encode_cid_key_data(&s).unwrap_err();
    assert!(err.contains("长度"));
}

#[test]
fn clearing_bank_nodes_key_starts_with_pallet_prefix() {
    let key = clearing_bank_nodes_key("AH001-SCB0V-123456789-2026").unwrap();
    let pallet_hex = hex::encode(sp_crypto_hashing::twox_128(b"OffchainTransaction"));
    let storage_hex = hex::encode(sp_crypto_hashing::twox_128(b"ClearingBankNodes"));
    let prefix = format!("0x{pallet_hex}{storage_hex}");
    assert!(key.starts_with(&prefix), "实际 key:{key}");
}
