use super::*;

#[test]
fn miner_key_filename_requires_powr_type_and_exact_public_key_length() {
    assert_eq!(
        miner_account_id_from_keystore_filename(&format!(
            "{}{}",
            POWR_KEY_TYPE_HEX_PREFIX,
            "AB".repeat(32)
        )),
        Some(format!("0x{}", "ab".repeat(32)))
    );
    for name in [
        String::new(),
        format!("{}{}", POWR_KEY_TYPE_HEX_PREFIX, "11".repeat(31)),
        format!("{}{}", POWR_KEY_TYPE_HEX_PREFIX, "gg".repeat(32)),
        format!("other{}", "11".repeat(32)),
    ] {
        assert!(miner_account_id_from_keystore_filename(&name).is_none());
    }
    assert!(decode_storage_account_id(&[0; 31]).is_err());
    assert_eq!(decode_storage_account_id(&[17; 32]).unwrap(), [17; 32]);
}
