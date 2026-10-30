use crate::signing::build_register_call_data;
use crate::signing::build_unregister_call_data;
use crate::signing::parse_signer_public_key;
use crate::signing::CALL_REGISTER;
use crate::signing::CALL_UNREGISTER;
use crate::signing::PALLET_INDEX;

#[test]
fn register_call_data_starts_with_pallet_and_call() {
    let cd = build_register_call_data(
        "AH001-SCB0V-123456789-2026",
        "12D3KooWAbcDef0123456789012345678901234567890123456",
        "rpc.example.com",
        9944,
    )
    .unwrap();
    assert_eq!(cd[0], PALLET_INDEX);
    assert_eq!(cd[1], CALL_REGISTER);
    // tail 应为 little-endian u16 端口
    let n = cd.len();
    assert_eq!(u16::from_le_bytes([cd[n - 2], cd[n - 1]]), 9944);
}

#[test]
fn register_call_rejects_empty_cid() {
    let err = build_register_call_data(
        "",
        "12D3KooWaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "x.com",
        9944,
    )
    .unwrap_err();
    assert!(err.contains("actor_cid_number"));
}

#[test]
fn register_call_rejects_low_port() {
    let err = build_register_call_data(
        "S",
        "12D3KooWaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "x.com",
        80,
    )
    .unwrap_err();
    assert!(err.contains("rpc_port"));
}

#[test]
fn unregister_call_only_has_cid() {
    let cid_number = "AH001-SCB0V-123456789-2026";
    let cd = build_unregister_call_data(cid_number).unwrap();
    assert_eq!(cd[0], PALLET_INDEX);
    assert_eq!(cd[1], CALL_UNREGISTER);
    // 单字节模式 compact: len << 2
    assert_eq!(cd[2], (cid_number.len() as u8) << 2);
    assert_eq!(&cd[3..], cid_number.as_bytes());
}

#[test]
fn signer_public_key_uses_prefixed_lowercase_hex() {
    let public_key = "0xaabbccddeeff00112233445566778899aabbccddeeff00112233445566778899";
    let (clean, bytes) = parse_signer_public_key(public_key).unwrap();
    assert_eq!(clean.len(), 66);
    assert_eq!(clean, public_key);
    assert_eq!(bytes.len(), 32);
    assert!(parse_signer_public_key(
        "0xAABBCCDDEEFF00112233445566778899AABBCCDDEEFF00112233445566778899"
    )
    .is_err());
}

#[test]
fn signer_public_key_rejects_bad_length() {
    assert!(parse_signer_public_key("0x").is_err());
    assert!(parse_signer_public_key("xyz").is_err());
}
