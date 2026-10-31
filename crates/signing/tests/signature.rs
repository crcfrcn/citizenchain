//! 原所属签名回归，输入和断言保持。
#![allow(clippy::expect_used, clippy::unwrap_used)]
use chain_signing::*;
use sp_core::H256;

#[test]
fn public_key_requires_prefixed_lowercase_hex() {
    let valid = format!("0x{}", "ab".repeat(32));
    assert!(parse_sr25519_public_key(&valid).is_ok());
    assert!(parse_sr25519_public_key(&"ab".repeat(32)).is_err());
    assert!(parse_sr25519_public_key(&format!("0x{}", "AB".repeat(32))).is_err());
}

#[test]
fn real_signatures_reject_wrong_key_modified_payload_and_corruption() {
    use sp_core::{sr25519, Pair};
    let signer = sr25519::Pair::from_seed(&[1; 32]);
    let other = sr25519::Pair::from_seed(&[2; 32]);
    for length in [0, 400] {
        let call = runtime::RuntimeCall::System(frame_system::Call::remark {
            remark: vec![3; length],
        });
        let material = build_signing_material_from_call(call, H256::repeat_byte(9), 2, 11, 7);
        let signature = sign_material_with_pair(&material, &signer);
        assert!(verify_signature(&material, &signature, &signer.public()));
        assert!(!verify_signature(&material, &signature, &other.public()));
        let mut modified = material;
        modified.signing_bytes.push(1);
        assert!(!verify_signature(&modified, &signature, &signer.public()));
    }
}

#[test]
fn key_and_signature_boundaries_are_checked_without_panics() {
    for text in [
        "".to_string(),
        "0x".to_string(),
        format!("0X{}", "ab".repeat(32)),
        format!("0x{}", "é".repeat(32)),
        format!("0x{}", "g0".repeat(32)),
    ] {
        assert!(parse_sr25519_public_key(&text).is_err());
    }
    for bytes in [0usize, 63, 65] {
        assert!(parse_sr25519_signature_hex(&"ab".repeat(bytes)).is_err());
    }
    assert!(parse_sr25519_signature_hex(&"ab".repeat(64)).is_ok());
    assert!(parse_sr25519_signature_hex(&format!("0x{}", "ab".repeat(64))).is_ok());
    assert!(parse_sr25519_signature_hex(&"é".repeat(64)).is_err());
    assert_eq!(
        sha256_hex(b""),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
}
