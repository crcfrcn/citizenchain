//! 验证解锁挑战和签名公钥位置，拒绝旧布局及异常输入。

use crate::unlock::build_challenge_payload;
use crate::unlock::build_clearing_decrypt_request;
use crate::unlock::decrypted_map;
use crate::unlock::list_clearing_decrypt;
use crate::unlock::lock_clearing_decrypt;
use crate::unlock::validate_challenge_payload;
use crate::unlock::MemoryEntry;
use primitives::sign::binary_domain_prefix;
use primitives::sign::BINARY_PREFIX_LEN;
use primitives::sign::CLEARING_DECRYPT_CID_LEN;
use primitives::sign::CLEARING_DECRYPT_PAYLOAD_LEN;
use primitives::sign::OP_SIGN_CLEARING_DECRYPT;

#[test]
fn challenge_payload_layout_uses_shared_protocol_length() {
    let p = build_challenge_payload(&[0xAA; 32], "AH001-SCB0V-123456789-2026", 1234567890)
        .expect("valid payload");
    assert_eq!(p.len(), CLEARING_DECRYPT_PAYLOAD_LEN);
    assert_eq!(
        p[..BINARY_PREFIX_LEN],
        binary_domain_prefix(OP_SIGN_CLEARING_DECRYPT)
    );
}

#[test]
fn challenge_payload_signer_public_key_position() {
    let p = build_challenge_payload(&[0xCC; 32], "AH001-FCB0P-123456789-2026", 0)
        .expect("valid payload");
    assert_eq!(
        &p[BINARY_PREFIX_LEN + CLEARING_DECRYPT_CID_LEN
            ..BINARY_PREFIX_LEN + CLEARING_DECRYPT_CID_LEN + 32],
        &[0xCC; 32]
    );
}

#[test]
fn legacy_108_byte_payload_is_rejected() {
    assert!(validate_challenge_payload(&[0u8; 108]).is_err());
}

#[test]
fn build_clearing_decrypt_request_rejects_short_signer_public_key() {
    let err = build_clearing_decrypt_request("0xAA", "AH001-SCB0V-123456789-2026").unwrap_err();
    assert!(err.contains("公钥格式"));
}

#[test]
fn list_clearing_decrypt_filters_by_cid() {
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
    let r = list_clearing_decrypt("AH001-SCB0V-123456789-2026");
    assert_eq!(r.len(), 1);
    assert!(r[0].signer_public_key.contains("aa"));

    // 清理:只移除本用例插入的两个 key,绝不 clear() 整表——否则并行执行时
    // 会误删其他用例(如 lock_clearing_decrypt_removes_entry 的 cc… )刚插入的条目。
    let mut guard = decrypted_map().lock().unwrap();
    guard.remove(&format!("0x{}", "aa".repeat(32)));
    guard.remove(&format!("0x{}", "bb".repeat(32)));
}

#[test]
fn lock_clearing_decrypt_removes_entry() {
    decrypted_map().lock().unwrap().insert(
        format!("0x{}", "cc".repeat(32)),
        MemoryEntry {
            cid_number: "AH001-SCB0E-202605180-2026".to_string(),
            decrypted_at_ms: 10,
        },
    );
    assert!(lock_clearing_decrypt(&format!("0x{}", "cc".repeat(32))).is_ok());
    assert!(lock_clearing_decrypt(&format!("0x{}", "cc".repeat(32))).is_err());
}

#[test]
fn clearing_decrypt_accepts_existing_wallet_algorithms_and_consumes_each_request_once() {
    use crate::unlock::{pending_map, verify_clearing_decrypt, VerifyClearingDecryptInput};
    use nodeclient::signing::payload_b64;
    use sp_core::Pair;
    for pq in [false, true] {
        let (pair, _) = sp_core::sr25519::Pair::generate();
        let child: [u8; 32] = rand::random();
        let (public, _) =
            chain_signing::mldsa::sign_with_public(&child, b"public derivation").unwrap();
        let account = if pq {
            chain_signing::mldsa::account_id(&public)
        } else {
            pair.public().0
        };
        let signer = format!("0x{}", hex::encode(account));
        let request =
            build_clearing_decrypt_request(&signer, "AH001-SCB0V-123456789-2026").unwrap();
        let envelope: serde_json::Value = serde_json::from_str(&request.request_json).unwrap();
        assert_eq!(
            envelope["b"]["a"],
            primitives::sign::QR_ACTION_CLEARING_DECRYPT
        );
        assert_eq!(envelope["b"]["g"], 0);
        let context = pending_map().lock().unwrap()[&request.request_id].clone();
        let (signature, q) = if pq {
            let (public, signature) =
                chain_signing::mldsa::sign_with_public(&child, &context.payload).unwrap();
            (signature.0.to_vec(), Some(payload_b64(&public.0)))
        } else {
            (pair.sign(&context.payload).0.to_vec(), None)
        };
        let response =
            serde_json::json!({"p":"QR_V1","k":2,"i":request.request_id,"e":context.expires_at,
            "b":{"u":payload_b64(&account),"s":payload_b64(&signature),"q":q}})
            .to_string();
        let input = || VerifyClearingDecryptInput {
            request_id: request.request_id.clone(),
            signer_public_key: signer.clone(),
            expected_payload_hash: request.expected_payload_hash.clone(),
            response_json: response.clone(),
        };
        let result = verify_clearing_decrypt(input()).unwrap();
        assert_eq!(result.signer_public_key, signer);
        assert_eq!(result.cid_number, context.cid_number);
        assert!(verify_clearing_decrypt(input()).is_err());
        lock_clearing_decrypt(&signer).unwrap();
    }
}

#[test]
fn expired_or_substituted_clearing_proof_never_grants_memory_access() {
    use crate::unlock::{pending_map, verify_clearing_decrypt, VerifyClearingDecryptInput};
    use nodeclient::signing::payload_b64;
    use sp_core::Pair;
    for expired in [false, true] {
        let (pair, _) = sp_core::sr25519::Pair::generate();
        let signer = format!("0x{}", hex::encode(pair.public().0));
        let request =
            build_clearing_decrypt_request(&signer, "AH001-SCB0V-123456789-2026").unwrap();
        if expired {
            pending_map()
                .lock()
                .unwrap()
                .get_mut(&request.request_id)
                .unwrap()
                .expires_at = 0;
        }
        let context = pending_map().lock().unwrap()[&request.request_id].clone();
        let response=serde_json::json!({"p":"QR_V1","k":2,"i":request.request_id,"e":context.expires_at,"b":{"u":payload_b64(&pair.public().0),"s":payload_b64(&pair.sign(if expired {&context.payload}else{b"substituted"}).0)}}).to_string();
        assert!(verify_clearing_decrypt(VerifyClearingDecryptInput {
            request_id: request.request_id,
            signer_public_key: signer.clone(),
            expected_payload_hash: request.expected_payload_hash,
            response_json: response
        })
        .is_err());
        assert!(!decrypted_map().lock().unwrap().contains_key(&signer));
    }
}
