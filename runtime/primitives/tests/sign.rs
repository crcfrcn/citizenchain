//! 所属原语的真实单元测试，保留原断言与固定输入。
mod sign {

    use crate::sign::*;

    #[test]
    fn activate_admin_payload_uses_the_shared_32_byte_cid_slot() {
        let cid_number = b"LN001-NRC0G-944805165-2026";
        let institution_code = *b"NRC\0";
        let signer_public_key = [0x22; 32];
        let nonce = [0u8; 16];
        let payload = activate_admin_payload(
            cid_number,
            &institution_code,
            0,
            &signer_public_key,
            1_700_000_000,
            &nonce,
        )
        .expect("valid CID should build an activation payload");

        assert_eq!(payload.len(), ACTIVATE_ADMIN_PAYLOAD_LEN);
        assert_eq!(&payload[..BINARY_PREFIX_LEN], b"GMB\x18");
        assert_eq!(
            &payload[BINARY_PREFIX_LEN..BINARY_PREFIX_LEN + cid_number.len()],
            cid_number
        );
        assert!(payload
            [BINARY_PREFIX_LEN + cid_number.len()..BINARY_PREFIX_LEN + ACTIVATE_ADMIN_CID_LEN]
            .iter()
            .all(|byte| *byte == 0));
    }

    #[test]
    fn decrypt_admin_payload_uses_the_same_cid_limit_and_rejects_invalid_cids() {
        let cid_number = b"AH001-SCB0V-123456789-2026";
        let signer_public_key = [0x33; 32];
        let nonce = [0u8; 16];
        let payload = decrypt_admin_payload(cid_number, &signer_public_key, 1_700_000_000, &nonce)
            .expect("valid CID should build a decrypt payload");

        assert_eq!(payload.len(), DECRYPT_ADMIN_PAYLOAD_LEN);
        assert_eq!(&payload[..BINARY_PREFIX_LEN], b"GMB\x19");
        assert_eq!(
            &payload[BINARY_PREFIX_LEN..BINARY_PREFIX_LEN + cid_number.len()],
            cid_number
        );
        assert!(payload
            [BINARY_PREFIX_LEN + cid_number.len()..BINARY_PREFIX_LEN + DECRYPT_ADMIN_CID_LEN]
            .iter()
            .all(|byte| *byte == 0));
        assert!(decrypt_admin_payload(&[], &signer_public_key, 0, &nonce).is_none());
        assert!(decrypt_admin_payload(
            &[b'X'; DECRYPT_ADMIN_CID_LEN + 1],
            &signer_public_key,
            0,
            &nonce,
        )
        .is_none());
    }
}
