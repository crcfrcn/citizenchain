use crate::unlock::build_challenge_payload;
use crate::unlock::build_decrypt_admin_request;
use crate::unlock::decrypted_map;
use crate::unlock::list_decrypted_admins;
use crate::unlock::lock_decrypted_admin;
use crate::unlock::validate_challenge_payload;
use crate::unlock::MemoryEntry;
use primitives::sign::binary_domain_prefix;
use primitives::sign::BINARY_PREFIX_LEN;
use primitives::sign::DECRYPT_ADMIN_CID_LEN;
use primitives::sign::DECRYPT_ADMIN_PAYLOAD_LEN;
use primitives::sign::OP_SIGN_DECRYPT;

#[test]
fn challenge_payload_layout_uses_shared_protocol_length() {
    let p = build_challenge_payload(&[0xAA; 32], "AH001-SCB0V-123456789-2026", 1234567890)
        .expect("valid payload");
    assert_eq!(p.len(), DECRYPT_ADMIN_PAYLOAD_LEN);
    assert_eq!(
        p[..BINARY_PREFIX_LEN],
        binary_domain_prefix(OP_SIGN_DECRYPT)
    );
}

#[test]
fn challenge_payload_signer_public_key_position() {
    let p = build_challenge_payload(&[0xCC; 32], "AH001-FCB0P-123456789-2026", 0)
        .expect("valid payload");
    assert_eq!(
        &p[BINARY_PREFIX_LEN + DECRYPT_ADMIN_CID_LEN
            ..BINARY_PREFIX_LEN + DECRYPT_ADMIN_CID_LEN + 32],
        &[0xCC; 32]
    );
}

#[test]
fn legacy_108_byte_payload_is_rejected() {
    assert!(validate_challenge_payload(&[0u8; 108]).is_err());
}

#[test]
fn build_decrypt_admin_request_rejects_short_signer_public_key() {
    let err = build_decrypt_admin_request("0xAA", "AH001-SCB0V-123456789-2026").unwrap_err();
    assert!(err.contains("公钥格式"));
}

#[test]
fn list_decrypted_admins_filters_by_cid() {
    decrypted_map().lock().unwrap().insert(
        format!("0x{}", "aa".repeat(32)),
        MemoryEntry {
            cid_number: "AH001-SCB0V-123456789-2026".to_string(),
            decrypted_at_ms: 1,
        },
    );
    decrypted_map().lock().unwrap().insert(
        format!("0x{}", "bb".repeat(32)),
        MemoryEntry {
            cid_number: "AH001-SCB0H-202605070-2026".to_string(),
            decrypted_at_ms: 2,
        },
    );
    let r = list_decrypted_admins("AH001-SCB0V-123456789-2026");
    assert_eq!(r.len(), 1);
    assert!(r[0].signer_public_key.contains("aa"));

    // 清理:只移除本用例插入的两个 key,绝不 clear() 整表——否则并行执行时
    // 会误删其他用例(如 lock_decrypted_admin_removes_entry 的 cc… )刚插入的条目。
    let mut guard = decrypted_map().lock().unwrap();
    guard.remove(&format!("0x{}", "aa".repeat(32)));
    guard.remove(&format!("0x{}", "bb".repeat(32)));
}

#[test]
fn lock_decrypted_admin_removes_entry() {
    decrypted_map().lock().unwrap().insert(
        format!("0x{}", "cc".repeat(32)),
        MemoryEntry {
            cid_number: "AH001-SCB0E-202605180-2026".to_string(),
            decrypted_at_ms: 10,
        },
    );
    assert!(lock_decrypted_admin(&format!("0x{}", "cc".repeat(32))).is_ok());
    assert!(lock_decrypted_admin(&format!("0x{}", "cc".repeat(32))).is_err());
}
