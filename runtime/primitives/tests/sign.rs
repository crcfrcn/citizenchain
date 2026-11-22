//! 验证公民业务签名原语及最大年份CID的固定槽位。

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

#[test]
fn u32_max_year_cid_fills_existing_signing_slots_without_shifting_fields() {
    use crate::cid::generator::{generate_cid_number, GenerateCidNumberInput};
    use crate::sign::*;
    let cid = generate_cid_number(GenerateCidNumberInput {
        public_key: "cid-signing-boundary", p1: "0", province_code: "GD",
        province_name: "广东省", city_code: "001", city_name: "荔湾市",
        year: u32::MAX, institution: "CGOV",
    }).unwrap();
    assert_eq!(cid.len(), 32);
    let signer = [0x22; 32];
    let nonce = [0x33; 16];
    let activate = activate_admin_payload(cid.as_bytes(), b"CGOV", 0, &signer, 123, &nonce).unwrap();
    assert_eq!(activate.len(), 97);
    assert_eq!(&activate[4..36], cid.as_bytes());
    assert_eq!(&activate[36..40], b"CGOV");
    assert_eq!(&activate[41..73], &signer);
    assert_eq!(&activate[73..81], &123u64.to_le_bytes());
    assert_eq!(&activate[81..97], &nonce);
    let decrypt = decrypt_admin_payload(cid.as_bytes(), &signer, 123, &nonce).unwrap();
    assert_eq!(decrypt.len(), 92);
    assert_eq!(&decrypt[4..36], cid.as_bytes());
    assert_eq!(&decrypt[36..68], &signer);
    assert_eq!(&decrypt[68..76], &123u64.to_le_bytes());
    assert_eq!(&decrypt[76..92], &nonce);
}
